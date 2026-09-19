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
//! Two values look like times and are not. `started` and `since` are
//! written with `os.date` off the local wall clock, with no zone and no
//! marker for the hour that repeats every autumn, so an age computed from
//! either is wrong twice a year and right in every test. They are display
//! only. The age of a heartbeat comes from the file's own modification
//! time, which the filesystem keeps in a form that does not move.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

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

/// The same, resolved where it is there.
fn maybe_path(headers: &Headers, name: &'static str) -> Result<Option<Real>, ReadErrorKind> {
    match maybe(headers, name)? {
        None => Ok(None),
        Some(value) => paths::resolve(Path::new(value))
            .map(Some)
            .map_err(|source| ReadErrorKind::Path { name, source }),
    }
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
    pub lfs_tempdir: Option<Real>,
    pub transport_source: String,
    pub install_guard: Option<Real>,
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
            lfs_tempdir: maybe_path(h, "lfs_tempdir")?,
            transport_source: required(h, "transport_source")?.to_owned(),
            install_guard: maybe_path(h, "install_guard")?,
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
        assert_eq!(h.lfs_tempdir, Some(real(&s.output().join("tmp"))));
        assert_eq!(h.transport_source, "fallback: beside the output");
        assert_eq!(
            h.install_guard,
            Some(real(&s.output().join("install_guard.txt")))
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
        assert_eq!(h.lfs_tempdir, None);
        assert_eq!(h.install_guard, None);
        assert_eq!(h.app_version, None);
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
    fn handshake_refuses_a_path_that_does_not_resolve_naming_the_header() {
        let b = Sandbox::new();
        let (_s, bytes) = published(&b);
        let why = refused(&with(&bytes, "req", r"rpc\0000-1\req"));
        assert!(why.contains("req: "), "{why}");
        assert!(why.contains("is relative"), "{why}");
        let why = refused(&with(&bytes, "lfs_tempdir", "tmp"));
        assert!(why.contains("lfs_tempdir: "), "{why}");
    }

    #[test]
    fn handshake_refuses_an_absent_header_naming_it() {
        let b = Sandbox::new();
        let (_s, bytes) = published(&b);
        for name in NAMES {
            let why = refused(&without(&bytes, name));
            assert!(
                why.ends_with(&format!("{name}: absent")),
                "{name} dropped: an absent header is refused naming it, and the \
                 handshake said: {why}"
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
