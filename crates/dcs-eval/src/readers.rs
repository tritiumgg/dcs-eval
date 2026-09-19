//! The two files the executor publishes about itself, read back: the
//! handshake it writes once at load, and the heartbeat it rewrites while it
//! ticks.
//!
//! These are typed views over [`protocol`](crate::protocol), not a second
//! parser. The envelope is read there, and what is added here is what the
//! values mean: which headers must be present, which are numbers, which are
//! one of exactly two spellings, and which name paths. A header this
//! version does not know is stepped over, so an executor that grows a field
//! is still readable; one that drops a field is not, and that is the
//! direction the refusals point.
//!
//! Every path a file names is resolved as it is read and never kept as
//! text. A client that compares paths cannot be handed an unresolved one,
//! which is the whole reason [`paths::Real`] exists.
//!
//! The paths divide in two, and the two are treated differently. The
//! transport directories — `transport`, `req`, `res`, `arm`, `output` —
//! are where a request is published and a reply is read, and one of them
//! the filesystem will not own refuses the whole file: a client that went
//! on would be about to write into a path nothing resolved. `lfs_tempdir`
//! and `install_guard` are not used for anything; they are written down so
//! a client can say what the executor saw. One of those that does not
//! resolve is kept as a [`Diagnostic::Unresolved`], because a handshake
//! naming such a path is exactly the finding a report of the session's
//! problems is there to carry, and refusing the file would hide the
//! finding behind the fault it describes.
//!
//! Two values look like times and are not. `started` and `since` are
//! written with `os.date` off the local wall clock, with no zone and no
//! marker for the hour that repeats every autumn, so an age computed from
//! either is wrong twice a year and right in every test. They are display
//! only. The age of a heartbeat comes from the file's own modification
//! time, which the filesystem keeps in a form that does not move.

use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, SystemTime};

use crate::paths::{self, PathError, Real};
use crate::protocol::{self, Headers, PROTOCOL, ParseError};

/// What the executor writes where a read of its own did not answer: the
/// install guard it could not find, a temp directory `lfs` would not give,
/// a source name it could not recover, an application version that was not
/// there. A header absent altogether is a different thing and refuses the
/// file.
const ABSENT: &str = "ABSENT";

/// Which file could not be read, and why. `Display` is `<path>: <reason>`,
/// the shape every refusal in this crate takes, so one line reads the same
/// wherever it was raised.
#[derive(Debug)]
pub struct ReadError {
    pub path: PathBuf,
    pub kind: ReadErrorKind,
}

/// The reason half of a [`ReadError`]. Every arm but the two that carry
/// another error's own words names the header it is about, and the name is
/// the one from this module's list, so a message cannot spell a field some
/// way the wire never did.
#[derive(Debug)]
pub enum ReadErrorKind {
    /// The file could not be opened or read.
    Disk(io::Error),
    /// The bytes are not an envelope.
    Envelope(ParseError),
    /// A header this reader requires is not there.
    Absent { name: &'static str },
    /// The identity line names something other than this executor.
    Identity { saw: String },
    /// A protocol version this client does not speak.
    Protocol { saw: String },
    /// A figure that is not a number the way the executor spells one.
    NotANumber { name: &'static str, saw: String },
    /// A value outside the two spellings the executor writes.
    NotOneOf {
        name: &'static str,
        saw: String,
        one_of: &'static str,
    },
    /// A path the filesystem could not be asked about.
    Path {
        name: &'static str,
        source: PathError,
    },
}

impl fmt::Display for ReadErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disk(source) => write!(f, "{source}"),
            Self::Envelope(source) => write!(f, "{source}"),
            Self::Absent { name } => write!(f, "{name}: absent"),
            Self::Identity { saw } => write!(f, "executor: {saw}, which is not dcs-eval"),
            Self::Protocol { saw } => {
                write!(f, "protocol: {saw}, and this client speaks {PROTOCOL}")
            }
            Self::NotANumber { name, saw } => write!(f, "{name}: {saw} is not a number"),
            Self::NotOneOf { name, saw, one_of } => {
                write!(f, "{name}: {saw} is neither {one_of}")
            }
            Self::Path { name, source } => write!(f, "{name}: {source}"),
        }
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.kind)
    }
}

impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ReadErrorKind::Disk(source) => Some(source),
            ReadErrorKind::Envelope(source) => Some(source),
            ReadErrorKind::Path { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl ReadError {
    /// A reason with the file it was about put back on. The field helpers
    /// raise the reason alone, which is what keeps the path out of every
    /// one of them.
    fn at(path: &Path, kind: ReadErrorKind) -> Self {
        Self {
            path: path.to_owned(),
            kind,
        }
    }
}

/// The start of a value for a message, cut in bytes as the envelope's own
/// excerpt is, so a refusal names what it saw without carrying a whole
/// header line into a log.
fn excerpt(value: &str) -> String {
    if value.len() > 80 {
        let mut end = 80;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &value[..end])
    } else {
        value.to_owned()
    }
}

/// The value under `name`, or the refusal that it is not there.
fn required<'a>(headers: &'a Headers, name: &'static str) -> Result<&'a str, ReadErrorKind> {
    headers.get(name).ok_or(ReadErrorKind::Absent { name })
}

/// A required figure. The executor spells all of these with Lua 5.1's
/// `tostring` over integers, so none carries a point, a sign or an
/// exponent, and a value that does is not one of ours.
fn number<T: FromStr>(headers: &Headers, name: &'static str) -> Result<T, ReadErrorKind> {
    let value = required(headers, name)?;
    value.parse().map_err(|_| ReadErrorKind::NotANumber {
        name,
        saw: excerpt(value),
    })
}

/// A required value that is one of exactly two spellings, `yes` being
/// `true`. There is no third answer and no default: an absent or
/// misspelt value refuses the file rather than being read as the quieter
/// of the two, because every such field decides something — whether the
/// session is armed, whether it will run a chunk — where guessing the
/// quiet answer is guessing wrong about a live executor.
fn one_of(
    headers: &Headers,
    name: &'static str,
    yes: &'static str,
    no: &'static str,
) -> Result<bool, ReadErrorKind> {
    one_of_value(name, required(headers, name)?, yes, no)
}

/// The same judgement on a value already in hand. Case is not folded: the
/// executor writes these two spellings and no others, and a reader that
/// also took `YES` would be taking a spelling nothing writes.
fn one_of_value(
    name: &'static str,
    value: &str,
    yes: &'static str,
    no: &'static str,
) -> Result<bool, ReadErrorKind> {
    if value == yes {
        Ok(true)
    } else if value == no {
        Ok(false)
    } else {
        Err(ReadErrorKind::NotOneOf {
            name,
            saw: excerpt(value),
            one_of: if yes == "yes" {
                "yes nor no"
            } else {
                "allowed nor disabled"
            },
        })
    }
}

/// A required path, resolved. What the handshake says a directory is and
/// what the filesystem says it is are two different things on Windows, and
/// only the second one can be compared; the tail that does not exist yet
/// comes back on, so a session directory not yet made still resolves.
fn path(headers: &Headers, name: &'static str) -> Result<Real, ReadErrorKind> {
    let value = required(headers, name)?;
    paths::resolve(Path::new(value)).map_err(|source| ReadErrorKind::Path { name, source })
}

/// A required header whose value may be the literal `ABSENT`.
fn maybe<'a>(headers: &'a Headers, name: &'static str) -> Result<Option<&'a str>, ReadErrorKind> {
    let value = required(headers, name)?;
    Ok(if value == ABSENT { None } else { Some(value) })
}

/// One of the two paths the handshake names for reporting rather than for
/// use. Three answers and no refusal: the executor's read did not answer,
/// the filesystem owns it, or it named something the filesystem would not
/// resolve — which is a finding, not a fault in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagnostic {
    /// The literal `ABSENT`: the executor looked and got nothing.
    Absent,
    /// What the filesystem says the named path really is.
    Real(Real),
    /// The path as the file spelt it, and why it would not resolve, in the
    /// words the resolver used.
    Unresolved { named: String, why: String },
}

impl Diagnostic {
    /// The resolved path, where there is one. A caller that means to
    /// compare against a real path gets nothing for the other two arms,
    /// which is the answer: there is no path here to compare.
    pub fn real(&self) -> Option<&Real> {
        match self {
            Self::Real(path) => Some(path),
            _ => None,
        }
    }

    /// What to report about this value, or nothing where there is nothing
    /// to report. `Absent` is not a problem — the executor says plainly
    /// that its read did not answer — while a path that would not resolve
    /// is.
    pub fn problem(&self) -> Option<&str> {
        match self {
            Self::Unresolved { why, .. } => Some(why),
            _ => None,
        }
    }
}

/// A required header naming one of those two paths.
fn diagnostic(headers: &Headers, name: &'static str) -> Result<Diagnostic, ReadErrorKind> {
    let Some(value) = maybe(headers, name)? else {
        return Ok(Diagnostic::Absent);
    };
    Ok(match paths::resolve(Path::new(value)) {
        Ok(real) => Diagnostic::Real(real),
        Err(source) => Diagnostic::Unresolved {
            named: value.to_owned(),
            why: ReadErrorKind::Path { name, source }.to_string(),
        },
    })
}

/// A comma-joined list as the executor concatenates one. An empty value is
/// no items rather than one empty one, which is what Rust's own split
/// would give.
fn list(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value.split(',').map(str::to_owned).collect()
    }
}

/// The envelope of a file, read whole.
fn envelope(path: &Path, bytes: &[u8]) -> Result<Headers, ReadError> {
    protocol::parse(bytes)
        .map(|e| e.headers)
        .map_err(|source| ReadError::at(path, ReadErrorKind::Envelope(source)))
}

/// `protocol` against what this client speaks.
fn protocol_is_ours(headers: &Headers) -> Result<(), ReadErrorKind> {
    let saw = required(headers, "protocol")?;
    if saw.parse::<u32>() == Ok(PROTOCOL) {
        Ok(())
    } else {
        Err(ReadErrorKind::Protocol { saw: excerpt(saw) })
    }
}

/// `<output>\executor.txt`, written once when the executor loads: who it
/// is, where it put its transport, what it will do, and the limits a client
/// has to honour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handshake {
    pub host: String,
    pub stamp: String,
    pub pid: u32,
    /// Local wall clock, display only.
    pub started: String,
    pub transport: Real,
    pub req: Real,
    pub res: Real,
    pub arm: Real,
    pub output: Real,
    /// Whether this session will run a chunk at all.
    pub eval: bool,
    pub ops: Vec<String>,
    /// The states line as it was written. What it is made of belongs to the
    /// reader of states, not here.
    pub states: String,
    pub namespace: String,
    pub source: Option<String>,
    /// What `lfs.tempdir()` gave the executor. Reported, never used: see
    /// the module note on why one that will not resolve is kept.
    pub lfs_tempdir: Diagnostic,
    pub transport_source: String,
    /// The install the executor guarded its own writes against. Reported
    /// on the same terms as `lfs_tempdir`.
    pub install_guard: Diagnostic,
    pub tick_budget_ms: u64,
    pub instruction_budget: u64,
    pub instruction_ceiling: u64,
    pub probe_every: u64,
    pub quiet_s: u64,
    /// The DCS version the session is running under, where it could be
    /// read. Where `_APP_VERSION` was neither a string nor nil the executor
    /// writes its type instead, which a reader cannot tell from a version
    /// and does not try to.
    pub app_version: Option<String>,
    pub max_request_bytes: u64,
    pub max_result_bytes: u64,
}

impl Handshake {
    /// The handshake at `path`.
    pub fn read(path: &Path) -> Result<Self, ReadError> {
        let bytes =
            std::fs::read(path).map_err(|why| ReadError::at(path, ReadErrorKind::Disk(why)))?;
        Self::from_bytes(path, &bytes)
    }

    /// The handshake in `bytes`, `path` being what a refusal names.
    pub fn from_bytes(path: &Path, bytes: &[u8]) -> Result<Self, ReadError> {
        let h = envelope(path, bytes)?;
        Self::fields(&h).map_err(|kind| ReadError::at(path, kind))
    }

    fn fields(h: &Headers) -> Result<Self, ReadErrorKind> {
        let executor = required(h, "executor")?;
        if executor != "dcs-eval" {
            return Err(ReadErrorKind::Identity {
                saw: excerpt(executor),
            });
        }
        protocol_is_ours(h)?;
        Ok(Self {
            host: required(h, "host")?.to_owned(),
            stamp: required(h, "stamp")?.to_owned(),
            pid: number(h, "pid")?,
            started: required(h, "started")?.to_owned(),
            transport: path(h, "transport")?,
            req: path(h, "req")?,
            res: path(h, "res")?,
            arm: path(h, "arm")?,
            output: path(h, "output")?,
            eval: one_of(h, "eval", "allowed", "disabled")?,
            ops: list(required(h, "ops")?),
            states: required(h, "states")?.to_owned(),
            namespace: required(h, "namespace")?.to_owned(),
            source: maybe(h, "source")?.map(str::to_owned),
            lfs_tempdir: diagnostic(h, "lfs_tempdir")?,
            transport_source: required(h, "transport_source")?.to_owned(),
            install_guard: diagnostic(h, "install_guard")?,
            tick_budget_ms: number(h, "tick_budget_ms")?,
            instruction_budget: number(h, "instruction_budget")?,
            instruction_ceiling: number(h, "instruction_ceiling")?,
            probe_every: number(h, "probe_every")?,
            quiet_s: number(h, "quiet_s")?,
            app_version: maybe(h, "app_version")?.map(str::to_owned),
            max_request_bytes: number(h, "max_request_bytes")?,
            max_result_bytes: number(h, "max_result_bytes")?,
        })
    }
}

/// `<output>\heartbeat.txt`, rewritten while the executor ticks: whether it
/// is armed, what phase it is in, how far its tick counter has gone, and
/// when the file was last written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heartbeat {
    pub host: String,
    pub stamp: String,
    pub transport: Real,
    pub phase: String,
    /// Whether the session is answering requests. Refused rather than
    /// defaulted where the file does not say, because every outcome a wait
    /// decides later turns on this one value and a live executor read as a
    /// dormant one is the wrong answer given quietly.
    pub armed: bool,
    /// Local wall clock of the last arm or disarm, display only.
    pub since: String,
    pub ticks: u64,
    /// `<name>@<tick>` of the last callback other than the frame, or none
    /// where none has fired yet. An empty value is a value: it says the
    /// session has seen no callback, not that the file is short a header.
    pub last_callback: Option<String>,
    pub callbacks: Vec<String>,
    /// When the file was last written, taken from the filesystem rather
    /// than from anything in the file.
    pub modified: SystemTime,
}

impl Heartbeat {
    /// The heartbeat at `path`, with the modification time taken off the
    /// same handle the bytes came from, so the time and the bytes belong to
    /// one file rather than to two stats either side of a rewrite.
    pub fn read(path: &Path) -> Result<Self, ReadError> {
        let disk = |why| ReadError::at(path, ReadErrorKind::Disk(why));
        let mut file = File::open(path).map_err(disk)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(disk)?;
        let modified = file
            .metadata()
            .and_then(|meta| meta.modified())
            .map_err(disk)?;
        Self::from_bytes(path, &bytes, modified)
    }

    /// The heartbeat in `bytes`, written at `modified`.
    pub fn from_bytes(path: &Path, bytes: &[u8], modified: SystemTime) -> Result<Self, ReadError> {
        let h = envelope(path, bytes)?;
        Self::fields(&h, modified).map_err(|kind| ReadError::at(path, kind))
    }

    /// How old the file is at `now`. A time in the future — a clock that
    /// has been put back, or a session on another machine's — is no age at
    /// all rather than an error: the caller asked how stale this is, and
    /// the honest answer to a file written in the future is "not".
    pub fn age(&self, now: SystemTime) -> Duration {
        now.duration_since(self.modified).unwrap_or(Duration::ZERO)
    }

    fn fields(h: &Headers, modified: SystemTime) -> Result<Self, ReadErrorKind> {
        protocol_is_ours(h)?;
        let last = required(h, "last_callback")?;
        Ok(Self {
            host: required(h, "host")?.to_owned(),
            stamp: required(h, "stamp")?.to_owned(),
            transport: path(h, "transport")?,
            phase: required(h, "phase")?.to_owned(),
            armed: one_of(h, "armed", "yes", "no")?,
            since: required(h, "since")?.to_owned(),
            ticks: number(h, "ticks")?,
            last_callback: (!last.is_empty()).then(|| last.to_owned()),
            callbacks: list(required(h, "callbacks")?),
            modified,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::frame;
    use crate::standin::Standin;
    use crate::testing::{Sandbox, slurp};

    /// A session with its handshake published, and the bytes it wrote.
    fn published(b: &Sandbox) -> (Standin, Vec<u8>) {
        let s = Standin::open(&b.path, "hook").expect("the session opens");
        s.handshake().expect("the handshake publishes");
        let bytes = slurp(&s.output().join("executor.txt"));
        (s, bytes)
    }

    /// The header lines of an envelope, to be edited and framed again.
    fn lines(bytes: &[u8]) -> Vec<(String, String)> {
        protocol::parse(bytes)
            .expect("the fixture parses")
            .headers
            .iter()
            .map(|(n, v)| (n.to_owned(), v.to_owned()))
            .collect()
    }

    /// Header lines back into bytes. `frame`, not the stand-in's encoder,
    /// because a fixture is composed here rather than published.
    fn framed(lines: &[(String, String)]) -> Vec<u8> {
        let refs: Vec<(&str, &str)> = lines
            .iter()
            .map(|(n, v)| (n.as_str(), v.as_str()))
            .collect();
        frame(&refs, b"").expect("the fixture frames")
    }

    /// The same envelope without `name`.
    fn without(bytes: &[u8], name: &str) -> Vec<u8> {
        let mut lines = lines(bytes);
        lines.retain(|(n, _)| n != name);
        framed(&lines)
    }

    /// The same envelope with `name` carrying `value`.
    fn with(bytes: &[u8], name: &str, value: &str) -> Vec<u8> {
        let mut lines = lines(bytes);
        for line in lines.iter_mut() {
            if line.0 == name {
                line.1 = value.to_owned();
            }
        }
        framed(&lines)
    }

    /// The same envelope with one byte of `name`'s value past ASCII.
    /// `frame` refuses such a value outright, and the executor's framer
    /// refuses it too, so the only way to a fixture is to edit the bytes
    /// after they are framed.
    fn past_ascii(bytes: &[u8], name: &str) -> Vec<u8> {
        let mut out = bytes.to_vec();
        let needle = format!("\n{name}: ").into_bytes();
        let at = out
            .windows(needle.len())
            .position(|w| w == needle.as_slice())
            .expect("the header is in the fixture")
            + needle.len();
        out[at] = 0xC3;
        out
    }

    /// A fixture path is what the filesystem says it is, never the spelling
    /// that made it: the sandbox sits under a temp directory this host
    /// spells short, and a `Real` compares bytes.
    fn real(path: &Path) -> Real {
        paths::resolve(path).expect("the path resolves")
    }

    fn at(bytes: &[u8]) -> Handshake {
        Handshake::from_bytes(Path::new("executor.txt"), bytes).expect("the handshake reads")
    }

    fn refused(bytes: &[u8]) -> String {
        Handshake::from_bytes(Path::new("executor.txt"), bytes)
            .expect_err("the handshake is refused")
            .to_string()
    }

    /// Every header of the handshake, in the executor's order.
    const NAMES: [&str; 27] = [
        "executor",
        "protocol",
        "host",
        "stamp",
        "pid",
        "started",
        "transport",
        "req",
        "res",
        "arm",
        "output",
        "eval",
        "ops",
        "states",
        "namespace",
        "source",
        "lfs_tempdir",
        "transport_source",
        "install_guard",
        "tick_budget_ms",
        "instruction_budget",
        "instruction_ceiling",
        "probe_every",
        "quiet_s",
        "app_version",
        "max_request_bytes",
        "max_result_bytes",
    ];

    #[test]
    fn handshake_reads_every_field_the_stand_in_publishes() {
        let b = Sandbox::new();
        let (s, bytes) = published(&b);
        let h = Handshake::read(&s.output().join("executor.txt")).expect("the handshake reads");
        assert_eq!(h, at(&bytes), "off the disk and off the bytes alike");
        assert_eq!(h.host, "hook");
        assert_eq!(h.stamp, s.stamp);
        assert_eq!(h.pid, 4242);
        assert_eq!(h.started, "2026-09-19 11:03:07");
        assert_eq!(h.transport, real(s.session()));
        assert_eq!(h.req, real(s.req()));
        assert_eq!(h.res, real(s.res()));
        assert_eq!(h.arm, real(s.arm()));
        assert_eq!(h.output, real(s.output()));
        assert!(h.eval);
        assert!(h.states.starts_with("hook:carrier=local"));
        assert_eq!(h.namespace, "DcsEval");
        assert_eq!(h.source.as_deref(), Some("DcsEvalExecutor.lua"));
        assert_eq!(
            h.lfs_tempdir,
            Diagnostic::Real(real(&s.output().join("tmp")))
        );
        assert_eq!(h.transport_source, "fallback: beside the output");
        assert_eq!(
            h.install_guard,
            Diagnostic::Real(real(&s.output().join("install_guard.txt")))
        );
        assert_eq!(h.tick_budget_ms, 8);
        assert_eq!(h.instruction_budget, 1_000_000);
        assert_eq!(h.instruction_ceiling, 50_000_000);
        assert_eq!(h.probe_every, 8);
        assert_eq!(h.quiet_s, 3);
        assert_eq!(h.app_version.as_deref(), Some("2.9.10.1234"));
        assert_eq!(h.max_request_bytes, 262_144);
        assert_eq!(h.max_result_bytes, 65_536);
    }

    #[test]
    fn handshake_reads_the_ops_it_may_send() {
        let b = Sandbox::new();
        let (_s, bytes) = published(&b);
        assert_eq!(at(&bytes).ops, ["ping", "eval"]);
        let one = with(&bytes, "ops", "ping");
        let h = at(&one);
        assert_eq!(h.ops, ["ping"]);
        let off = at(&with(&bytes, "eval", "disabled"));
        assert!(!off.eval, "a session that will not run a chunk");
    }

    #[test]
    fn handshake_reads_absent_where_a_read_did_not_answer() {
        let b = Sandbox::new();
        let (_s, bytes) = published(&b);
        let mut fixture = bytes.clone();
        for name in ["source", "lfs_tempdir", "install_guard", "app_version"] {
            fixture = with(&fixture, name, "ABSENT");
        }
        let h = at(&fixture);
        assert_eq!(h.source, None);
        assert_eq!(h.lfs_tempdir, Diagnostic::Absent);
        assert_eq!(h.install_guard, Diagnostic::Absent);
        assert_eq!(h.app_version, None);
        // Nothing to report: the executor said plainly that it looked.
        assert_eq!(h.lfs_tempdir.problem(), None);
        assert_eq!(h.lfs_tempdir.real(), None);
        // The literal is a value; the header itself is still required.
        let why = refused(&without(&fixture, "source"));
        assert!(why.ends_with("source: absent"), "{why}");
    }

    #[test]
    fn handshake_resolves_every_path_it_names() {
        let b = Sandbox::new();
        let (s, bytes) = published(&b);
        let h = at(&bytes);
        // Every one comes back as what the filesystem says, which is not
        // the spelling the file carried when the box is spelt short.
        for (got, want) in [
            (&h.transport, s.session()),
            (&h.req, s.req()),
            (&h.res, s.res()),
            (&h.arm, s.arm()),
            (&h.output, s.output()),
        ] {
            assert_eq!(got, &real(want));
            assert!(
                real(s.output()).contains(got),
                "{got} lies under the output"
            );
        }
        assert!(h.output.contains(&h.transport));
    }

    #[test]
    fn handshake_refuses_a_transport_path_that_does_not_resolve_naming_the_header() {
        let b = Sandbox::new();
        let (_s, bytes) = published(&b);
        // Every directory a request is published into or read out of: the
        // client would be about to write there.
        for name in ["transport", "req", "res", "arm", "output"] {
            let why = refused(&with(&bytes, name, r"rpc\0000-1\req"));
            assert!(why.contains(&format!("{name}: ")), "{why}");
            assert!(why.contains("is relative"), "{why}");
        }
    }

    #[test]
    fn handshake_keeps_a_reported_path_that_does_not_resolve_rather_than_refusing() {
        let b = Sandbox::new();
        let (_s, bytes) = published(&b);
        // Neither of these is a path the client uses, and a session whose
        // executor named an unresolvable one is a session a report has
        // something to say about — so the file still reads.
        for name in ["lfs_tempdir", "install_guard"] {
            let h = at(&with(&bytes, name, "tmp"));
            let got = if name == "lfs_tempdir" {
                &h.lfs_tempdir
            } else {
                &h.install_guard
            };
            let Diagnostic::Unresolved { named, why } = got else {
                panic!("{name}: an unresolvable reported path is kept, not refused: {got:?}");
            };
            assert_eq!(named, "tmp", "kept as the file spelt it");
            assert!(why.starts_with(&format!("{name}: ")), "{why}");
            assert!(why.contains("is relative"), "{why}");
            assert_eq!(
                got.problem(),
                Some(why.as_str()),
                "and it is what a report would say"
            );
            assert_eq!(got.real(), None, "and there is no path to compare");
        }
        // The rest of the handshake reads as it did.
        let one = at(&with(&bytes, "lfs_tempdir", "tmp"));
        assert_eq!(one.transport, at(&bytes).transport);
    }

    #[test]
    fn handshake_refuses_an_absent_header_naming_it() {
        let b = Sandbox::new();
        let (_s, bytes) = published(&b);
        for name in NAMES {
            let short = without(&bytes, name);
            let why = match Handshake::from_bytes(Path::new("executor.txt"), &short) {
                Err(why) => why.to_string(),
                Ok(_) => panic!(
                    "{name}: an absent header is refused, not defaulted — the \
                     handshake read Ok"
                ),
            };
            assert!(
                why.ends_with(&format!("{name}: absent")),
                "{name} dropped, and the handshake said: {why}"
            );
        }
    }

    #[test]
    fn handshake_refuses_a_protocol_that_is_not_two() {
        let b = Sandbox::new();
        let (_s, bytes) = published(&b);
        for saw in ["3", "1", "2.0", "two", ""] {
            let why = refused(&with(&bytes, "protocol", saw));
            assert!(
                why.ends_with(&format!("protocol: {saw}, and this client speaks 2")),
                "{why}"
            );
        }
    }

    #[test]
    fn handshake_refuses_a_figure_that_is_not_a_number() {
        let b = Sandbox::new();
        let (_s, bytes) = published(&b);
        for name in [
            "pid",
            "tick_budget_ms",
            "instruction_budget",
            "instruction_ceiling",
            "probe_every",
            "quiet_s",
            "max_request_bytes",
            "max_result_bytes",
        ] {
            let why = refused(&with(&bytes, name, "lots"));
            assert!(
                why.ends_with(&format!("{name}: lots is not a number")),
                "{why}"
            );
        }
        // The executor spells these with Lua's own `tostring` over
        // integers, so a point or a sign is not one of ours.
        for saw in ["8.0", "-1", "1e3", ""] {
            let why = refused(&with(&bytes, "probe_every", saw));
            assert!(
                why.ends_with(&format!("probe_every: {saw} is not a number")),
                "{why}"
            );
        }
    }

    #[test]
    fn handshake_refuses_an_eval_that_is_neither_allowed_nor_disabled() {
        let b = Sandbox::new();
        let (_s, bytes) = published(&b);
        for saw in ["Allowed", "ALLOWED", "yes", "true", ""] {
            let why = refused(&with(&bytes, "eval", saw));
            assert!(
                why.ends_with(&format!("eval: {saw} is neither allowed nor disabled")),
                "{why}"
            );
        }
    }

    #[test]
    fn handshake_refuses_a_value_past_ascii_naming_the_header() {
        let b = Sandbox::new();
        let (_s, bytes) = published(&b);
        let why = refused(&past_ascii(&framed(&lines(&bytes)), "output"));
        assert!(why.contains("output: "), "{why}");
        assert!(why.contains("not ASCII"), "{why}");
    }

    #[test]
    fn handshake_keeps_reading_past_a_header_it_does_not_know() {
        let b = Sandbox::new();
        let (_s, bytes) = published(&b);
        let mut lines = lines(&bytes);
        lines.push(("answered".to_owned(), "12".to_owned()));
        lines.insert(1, ("queued".to_owned(), "0".to_owned()));
        assert_eq!(
            at(&framed(&lines)),
            at(&bytes),
            "a field a later executor grows is stepped over"
        );
    }

    // ---- the heartbeat ----------------------------------------------------

    /// A session that has beaten at `at`, and the bytes it wrote.
    fn beaten(b: &Sandbox, at: SystemTime) -> (Standin, Vec<u8>) {
        let s = Standin::open(&b.path, "hook").expect("the session opens");
        s.beat(at).expect("the beat publishes");
        let bytes = slurp(&s.output().join("heartbeat.txt"));
        (s, bytes)
    }

    fn beat_at(bytes: &[u8], at: SystemTime) -> Heartbeat {
        Heartbeat::from_bytes(Path::new("heartbeat.txt"), bytes, at).expect("the heartbeat reads")
    }

    fn beat_refused(bytes: &[u8]) -> String {
        Heartbeat::from_bytes(Path::new("heartbeat.txt"), bytes, SystemTime::UNIX_EPOCH)
            .expect_err("the heartbeat is refused")
            .to_string()
    }

    /// Every header of the heartbeat, in the executor's order.
    const BEAT: [&str; 10] = [
        "protocol",
        "host",
        "stamp",
        "transport",
        "phase",
        "armed",
        "since",
        "ticks",
        "last_callback",
        "callbacks",
    ];

    #[test]
    fn heartbeat_reads_every_field_the_stand_in_publishes() {
        let b = Sandbox::new();
        let now = SystemTime::now();
        let (mut s, _) = beaten(&b, now);
        s.armed = true;
        s.phase = "mission".to_owned();
        s.tick = 41;
        s.last_callback = "onMissionLoadEnd@12".to_owned();
        s.callbacks = vec![
            "onMissionLoadEnd".to_owned(),
            "onSimulationStart".to_owned(),
        ];
        s.beat(now).expect("the beat publishes");
        let path = s.output().join("heartbeat.txt");
        let h = Heartbeat::read(&path).expect("the heartbeat reads");
        assert_eq!(h.host, "hook");
        assert_eq!(h.stamp, s.stamp);
        assert_eq!(h.transport, real(s.session()));
        assert_eq!(h.phase, "mission");
        assert!(h.armed);
        assert_eq!(h.since, s.since);
        assert_eq!(h.ticks, 41);
        assert_eq!(h.last_callback.as_deref(), Some("onMissionLoadEnd@12"));
        assert_eq!(h.callbacks, ["onMissionLoadEnd", "onSimulationStart"]);
        let bytes = slurp(&path);
        assert_eq!(
            beat_at(&bytes, h.modified),
            h,
            "off the disk and off the bytes alike"
        );
    }

    #[test]
    fn heartbeat_age_comes_from_the_mtime_while_since_is_display_only() {
        let b = Sandbox::new();
        let now = SystemTime::now();
        let then = now - Duration::from_secs(30);
        let mut s = Standin::open(&b.path, "hook").expect("the session opens");
        // A wall clock an hour out of step with the file, which is what
        // `os.date` writes where a zone moved under it.
        s.since = "2026-09-19 10:03:07".to_owned();
        s.beat(then).expect("the beat publishes");
        let h = Heartbeat::read(&s.output().join("heartbeat.txt")).expect("the heartbeat reads");
        let age = h.age(now);
        assert!(
            age >= Duration::from_secs(30),
            "the age is taken from the file, not from the read: wanted at \
             least 30s, saw {age:?}"
        );
        assert!(
            age < Duration::from_secs(120),
            "and is not an invention: {age:?}"
        );
        assert_eq!(
            h.since, "2026-09-19 10:03:07",
            "and `since` comes back as it was written, never parsed"
        );
        // A file written in the future is no age at all.
        assert_eq!(h.age(then - Duration::from_secs(60)), Duration::ZERO);
    }

    #[test]
    fn heartbeat_refuses_an_absent_header_naming_it() {
        let b = Sandbox::new();
        let (_s, bytes) = beaten(&b, SystemTime::now());
        for name in BEAT {
            let short = without(&bytes, name);
            let read =
                Heartbeat::from_bytes(Path::new("heartbeat.txt"), &short, SystemTime::UNIX_EPOCH);
            let why = match read {
                Err(why) => why.to_string(),
                Ok(h) => panic!(
                    "{name}: an absent header is refused, not defaulted — the \
                     heartbeat read Ok, with armed: {}",
                    h.armed
                ),
            };
            assert!(
                why.ends_with(&format!("{name}: absent")),
                "{name} dropped, and the heartbeat said: {why}"
            );
        }
    }

    #[test]
    fn heartbeat_refuses_an_armed_that_is_neither_yes_nor_no() {
        let b = Sandbox::new();
        let (_s, bytes) = beaten(&b, SystemTime::now());
        for saw in ["YES", "Yes", "true", "1", ""] {
            let why = beat_refused(&with(&bytes, "armed", saw));
            assert!(
                why.ends_with(&format!("armed: {saw} is neither yes nor no")),
                "{why}"
            );
        }
    }

    #[test]
    fn heartbeat_refuses_a_protocol_that_is_not_two() {
        let b = Sandbox::new();
        let (_s, bytes) = beaten(&b, SystemTime::now());
        for saw in ["3", "2.0", ""] {
            let why = beat_refused(&with(&bytes, "protocol", saw));
            assert!(
                why.ends_with(&format!("protocol: {saw}, and this client speaks 2")),
                "{why}"
            );
        }
    }

    #[test]
    fn heartbeat_refuses_ticks_that_are_not_a_number() {
        let b = Sandbox::new();
        let (_s, bytes) = beaten(&b, SystemTime::now());
        for saw in ["lots", "-1", "12.0", ""] {
            let why = beat_refused(&with(&bytes, "ticks", saw));
            assert!(
                why.ends_with(&format!("ticks: {saw} is not a number")),
                "{why}"
            );
        }
    }

    #[test]
    fn heartbeat_refuses_a_value_past_ascii_naming_the_header() {
        let b = Sandbox::new();
        let (_s, bytes) = beaten(&b, SystemTime::now());
        let why = beat_refused(&past_ascii(&framed(&lines(&bytes)), "transport"));
        assert!(why.contains("transport: "), "{why}");
        assert!(why.contains("not ASCII"), "{why}");
    }

    #[test]
    fn heartbeat_reads_an_empty_last_callback_and_no_callbacks_as_none() {
        let b = Sandbox::new();
        let (_s, bytes) = beaten(&b, SystemTime::now());
        let h = beat_at(&bytes, SystemTime::now());
        assert_eq!(h.last_callback, None, "none has fired");
        assert!(
            h.callbacks.is_empty(),
            "and the list is empty, not one blank"
        );
        let one = beat_at(
            &with(&bytes, "callbacks", "onShowGameMenu"),
            SystemTime::now(),
        );
        assert_eq!(one.callbacks, ["onShowGameMenu"]);
    }

    #[test]
    fn heartbeat_keeps_reading_past_a_header_it_does_not_know() {
        let b = Sandbox::new();
        let (_s, bytes) = beaten(&b, SystemTime::now());
        let at = SystemTime::now();
        let mut lines = lines(&bytes);
        lines.push(("queued".to_owned(), "0".to_owned()));
        lines.push(("answered".to_owned(), "9".to_owned()));
        assert_eq!(
            beat_at(&framed(&lines), at),
            beat_at(&bytes, at),
            "a field a later executor grows is stepped over"
        );
    }

    #[test]
    fn a_file_that_is_not_there_is_refused_naming_the_path() {
        let b = Sandbox::new();
        let gone = b.join("executor.txt");
        let why = Handshake::read(&gone)
            .expect_err("there is no handshake")
            .to_string();
        assert!(why.starts_with(&format!("{}: ", gone.display())), "{why}");
        let gone = b.join("heartbeat.txt");
        let why = Heartbeat::read(&gone)
            .expect_err("there is no heartbeat")
            .to_string();
        assert!(why.starts_with(&format!("{}: ", gone.display())), "{why}");
    }

    #[test]
    fn an_envelope_without_a_blank_line_is_refused_as_the_parser_refuses_it() {
        let b = Sandbox::new();
        let (_s, handshake) = published(&b);
        let cut = &handshake[..handshake.len() - 2];
        let why = refused(cut);
        assert!(why.contains("the headers never end"), "{why}");
        let (_s, beat) = beaten(&Sandbox::new(), SystemTime::now());
        let why = beat_refused(&beat[..beat.len() - 2]);
        assert!(why.contains("the headers never end"), "{why}");
    }

    #[test]
    fn handshake_refuses_a_file_that_is_not_this_executors() {
        let b = Sandbox::new();
        let (_s, bytes) = published(&b);
        for saw in ["dcs-api-bridge", "DCS-EVAL", ""] {
            let why = refused(&with(&bytes, "executor", saw));
            assert!(
                why.ends_with(&format!("executor: {saw}, which is not dcs-eval")),
                "{why}"
            );
        }
    }
}
