//! What evaluating a file refuses before a byte of it is read.
//!
//! A Lua compile error carries the token it choked on, and the wire returns
//! that error verbatim; for an unterminated string the token is the rest of
//! the file. So "evaluate any file" plus "return errors as they came" is a
//! way to read any file on the machine through a failure to compile it. The
//! containment here is therefore the whole of the protection, and it is only
//! protection if it runs first: a refusal issued after the bytes are in hand
//! has already done the reading it was meant to prevent.
//!
//! Three rules and a ceiling, in that order. A path under a write
//! directory's `Config` is refused because `network.vault` there holds the
//! user's account credentials. A path under the DCS install is refused
//! because nothing served from here needs ED's own file as a chunk — an
//! agent that wants one runs `dofile` in a one-line chunk and lets DCS read
//! it. Anything else must lie under a root the caller allowed. Then the file
//! is stated, and its size plus the bytes of the request's own header block
//! must not exceed the request ceiling the handshake published.
//!
//! The caller supplies the roots: this is a library with no start-up, no
//! argument vector and no launch directory to fall back on, and decision
//! record 0014 says why that departs from a server that has all three.

use std::fmt;
use std::fs;
use std::io;

use crate::paths::{self, PathError, Real};
use crate::protocol::{self, FrameError};
use crate::readers::Handshake;

/// The roots a file path is judged against: the directories a caller
/// allowed, the `Config` inside each write directory, and the install.
///
/// All three are supplied. Nothing here reads an argument vector, an
/// environment variable, or this process's current directory, and nothing
/// here takes the install from a handshake: a rule that fired only once a
/// handshake had been read would make a refusal depend on what had happened
/// earlier in the session rather than on what the caller configured.
#[derive(Clone, Debug)]
pub struct Roots {
    allowed: Vec<Real>,
    configs: Vec<Real>,
    install: Option<Real>,
}

impl Roots {
    /// The roots, with a `Config` resolved once per write directory here so
    /// that a junction standing where one should be is followed rather than
    /// walked around.
    ///
    /// `writedirs` is a list because a machine normally carries several DCS
    /// variants side by side — stable, open beta, a dedicated server — each
    /// with its own write directory and its own `network.vault`, and a guard
    /// covering one of them leaves the others' credentials under whatever
    /// root the caller allowed. Which directories exist is a question about
    /// the machine, so finding them stays with the caller, as the install
    /// does.
    ///
    /// An empty `allowed` is legal and admits nothing. It does not mean
    /// "anything goes" and it does not fall back to wherever this process
    /// is sitting, which would be a containment check on an accident. An
    /// empty `writedirs` or an `install` left out switches its rule off for
    /// want of a root to fire on, and the caller that leaves one out is the
    /// one that decided to go without that guard.
    pub fn new(
        allowed: &[Real],
        writedirs: &[Real],
        install: Option<&Real>,
    ) -> Result<Self, PathError> {
        let mut configs = Vec::with_capacity(writedirs.len());
        for wd in writedirs {
            configs.push(paths::resolve(&wd.as_path().join("Config"))?);
        }
        Ok(Self {
            allowed: allowed.to_vec(),
            configs,
            install: install.cloned(),
        })
    }

    /// Whether `real` may be read at all, touching no file to decide it.
    ///
    /// The order is load-bearing rather than incidental. `Config` and the
    /// install are tested first so that a caller who allowed a root wide
    /// enough to cover one of them is told which rule really refused the
    /// path, not the weaker one it also happens to fail. The roots rule is
    /// last because it is the one a user can fix by allowing another
    /// directory.
    pub fn judge(&self, real: &Real) -> Result<(), FileRefusal> {
        if let Some(config) = self.configs.iter().find(|config| config.contains(real)) {
            return Err(FileRefusal {
                path: real.clone(),
                kind: Refusal::Credentials {
                    config: config.clone(),
                },
            });
        }
        if let Some(install) = &self.install
            && install.contains(real)
        {
            return Err(FileRefusal {
                path: real.clone(),
                kind: Refusal::Install {
                    install: install.clone(),
                },
            });
        }
        if self.allowed.iter().any(|root| root.contains(real)) {
            return Ok(());
        }
        Err(FileRefusal {
            path: real.clone(),
            kind: Refusal::OutsideEveryRoot,
        })
    }
}

/// A file that may be read, and what was learned about it without reading
/// it.
///
/// `size` is what the stat said and nothing more: the file may have grown
/// since, and whoever reads the bytes owns that gap. `headroom` is
/// `max_request_bytes` less the header block less `size` — the bytes a
/// reader still has in hand once this file's bytes are in the envelope —
/// defined here once so nobody has to derive it a second time.
///
/// The fields are private for the reason [`Real`]'s is: [`check`] is then
/// the only thing that can make one, so a reader taking an `Admitted` has
/// the judgement in its signature rather than in a note asking its callers
/// to have run one. A public field would let any caller assemble a path
/// nothing judged and a size that came from nowhere.
#[derive(Clone, Debug)]
pub struct Admitted {
    path: Real,
    size: u64,
    headroom: u64,
}

impl Admitted {
    /// The resolved path that was judged, which is the one to open: a
    /// reader that re-spells it has stepped outside what was checked.
    pub fn path(&self) -> &Real {
        &self.path
    }

    /// What the stat said, in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// What is left of the request ceiling once the header block and a file
    /// of `size` are in the envelope.
    pub fn headroom(&self) -> u64 {
        self.headroom
    }
}

/// Whether `real` may be read and would fit, deciding both before anything
/// opens it.
///
/// The parameter is a [`Real`] rather than a path because only
/// [`paths::resolve`] can make one: a short spelling and a junction are
/// exactly how a path outside a root wears a permitted name, so a
/// containment check that cannot be handed an unresolved path is a stronger
/// guarantee than a rule written in a comment. It also means the caller has
/// the resolved path in hand before it builds `headers`, which matters
/// because one of those headers names the resolved path and its bytes are
/// among the ones counted here.
///
/// `headers` is the exact set the caller will send, and the count is taken
/// by framing them with an empty body rather than estimated: the executor
/// stats the whole request file, so every header line and the blank line
/// ending the block counts against the ceiling. The ceiling itself comes off
/// the handshake, which is why this takes one rather than a number a caller
/// could have invented.
///
/// The order is judge, frame, stat, compare. Nothing that could disclose the
/// file runs before the judgement, and the framer's own refusal is raised
/// before the stat because a request that cannot be written is not a
/// question about this file at all.
pub fn check(
    roots: &Roots,
    h: &Handshake,
    headers: &[(&str, &str)],
    real: &Real,
) -> Result<Admitted, FileRefusal> {
    roots.judge(real)?;
    let block = protocol::frame(headers, b"").map_err(|source| FileRefusal {
        path: real.clone(),
        kind: Refusal::Frame(source),
    })?;
    let header_bytes = block.len() as u64;
    let size = fs::metadata(real.as_path())
        .map_err(|source| FileRefusal {
            path: real.clone(),
            kind: Refusal::Stat(source),
        })?
        .len();
    // The sum, never the limit less the header block: a header block at or
    // past the limit makes that subtraction saturate to nothing and then
    // admits an empty file whose framed request is already over.
    let total = size.saturating_add(header_bytes);
    if total > h.max_request_bytes {
        return Err(FileRefusal {
            path: real.clone(),
            kind: Refusal::Oversize {
                size,
                header_bytes,
                limit: h.max_request_bytes,
            },
        });
    }
    Ok(Admitted {
        path: real.clone(),
        size,
        headroom: h.max_request_bytes - total,
    })
}

/// A path that will not be read, and why. `Display` is `<path>: <reason>`,
/// the shape every refusal in this client takes.
///
/// No variant carries anything read out of the file, and none of them says
/// whether a path outside the roots exists: an error message that answered
/// that question would be the file listing this module exists to refuse,
/// asked one path at a time.
#[derive(Debug)]
pub struct FileRefusal {
    pub path: Real,
    pub kind: Refusal,
}

#[derive(Debug)]
pub enum Refusal {
    /// Under a write directory's `Config`, where the account credentials
    /// live. Refused whether or not an allowed root covers it.
    Credentials { config: Real },
    /// Under the DCS install. Refused whether or not an allowed root covers
    /// it. The wording is the executor's own for the same rule at the other
    /// end, so one rule reads the same from both sides.
    Install { install: Real },
    /// Under no allowed root. The same words whether the path is there or
    /// not.
    OutsideEveryRoot,
    /// Too big for one request. Every figure here comes off the stat and
    /// the handshake; none of them was derived from a byte of the file.
    Oversize {
        size: u64,
        header_bytes: u64,
        limit: u64,
    },
    /// The request's own headers could not be written, so there was no
    /// point asking how big the file is.
    Frame(FrameError),
    /// The file could not be stated. Not an answer about containment: that
    /// was settled before this ran.
    Stat(io::Error),
}

impl FileRefusal {
    /// The refusal without the path in front of it, so two refusals about
    /// different paths can be compared for having given the same reason.
    pub fn reason(&self) -> String {
        match &self.kind {
            Refusal::Credentials { config } => format!(
                "is inside {config}, which holds the account credentials and is never read from here"
            ),
            Refusal::Install { install } => format!("is inside the install, {install}"),
            Refusal::OutsideEveryRoot => "is not under any allowed root".to_owned(),
            Refusal::Oversize {
                size,
                header_bytes,
                limit,
            } => format!(
                "is {size} bytes and the request's headers are {header_bytes} more, over the \
                 {limit}-byte limit the handshake published; split the file, or dofile it from a \
                 one-line chunk in a state that has io"
            ),
            Refusal::Frame(source) => format!("the request's headers were refused: {source}"),
            Refusal::Stat(source) => format!("could not be stated: {source}"),
        }
    }
}

impl fmt::Display for FileRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.reason())
    }
}

impl std::error::Error for FileRefusal {}

#[cfg(test)]
mod file_refusals {
    use super::*;
    use crate::standin::Standin;
    use crate::testing::{Sandbox, held, junction, real, short_name, slurp, with};

    use std::fs;
    use std::path::{Path, PathBuf};

    /// A directory under the box, made, and resolved.
    fn dir(b: &Sandbox, name: &str) -> Real {
        let path = b.path.join(name);
        fs::create_dir_all(&path).expect("the directory is made");
        real(&path)
    }

    /// A file at `path` with `bytes` in it, resolved.
    fn file(path: &PathBuf, bytes: &[u8]) -> Real {
        fs::create_dir_all(path.parent().expect("a parent")).expect("the parent is made");
        fs::write(path, bytes).expect("the file is written");
        real(path)
    }

    /// The usual shape: a project root that is allowed, a write directory
    /// with a `Config` in it, an install, and a place that is none of them.
    struct Box_ {
        b: Sandbox,
        project: Real,
        writedir: Real,
        install: Real,
        outside: Real,
    }

    fn scene() -> Box_ {
        let b = Sandbox::new();
        let project = dir(&b, "project");
        let writedir = dir(&b, "Saved Games\\DCS");
        let install = dir(&b, "install");
        let outside = dir(&b, "elsewhere");
        dir(&b, "Saved Games\\DCS\\Config");
        Box_ {
            b,
            project,
            writedir,
            install,
            outside,
        }
    }

    impl Box_ {
        fn roots(&self) -> Roots {
            Roots::new(
                std::slice::from_ref(&self.project),
                std::slice::from_ref(&self.writedir),
                Some(&self.install),
            )
            .expect("the roots resolve")
        }
    }

    // ---- the roots --------------------------------------------------------

    #[test]
    fn refuses_a_path_outside_every_root_naming_the_rule() {
        // The fixture is a file this process genuinely cannot open, so a
        // guard that read first and refused afterwards could not pass for
        // one that refused first.
        let s = scene();
        let path = s.outside.as_path().join("secret.lua");
        let real = file(&path, b"local secret = 1\n");
        let hold = held(&path);
        let err = s.roots().judge(&real).expect_err("outside every root");
        drop(hold);
        assert!(matches!(err.kind, Refusal::OutsideEveryRoot), "{err}");
        assert_eq!(err.reason(), "is not under any allowed root");
        assert!(err.to_string().starts_with(&real.to_string()), "{err}");
    }

    #[test]
    fn admits_nothing_when_no_root_is_configured() {
        // An empty allow list is not "anything goes", and it does not fall
        // back to wherever this process is sitting.
        let s = scene();
        let roots = Roots::new(&[], &[], None).expect("the roots resolve");
        let real = file(&s.project.as_path().join("ok.lua"), b"return 1\n");
        let err = roots.judge(&real).expect_err("nothing is admitted");
        assert!(matches!(err.kind, Refusal::OutsideEveryRoot), "{err}");
    }

    #[test]
    fn admits_a_path_under_a_configured_root() {
        let s = scene();
        let real = file(&s.project.as_path().join("probe.lua"), b"return 1\n");
        s.roots().judge(&real).expect("under the allowed root");
    }

    #[test]
    fn admits_a_path_under_the_second_of_two_roots() {
        let s = scene();
        let second = dir(&s.b, "other-project");
        let roots = Roots::new(
            &[s.project.clone(), second.clone()],
            std::slice::from_ref(&s.writedir),
            Some(&s.install),
        )
        .expect("the roots resolve");
        let real = file(&second.as_path().join("probe.lua"), b"return 1\n");
        roots.judge(&real).expect("under the second root");
    }

    #[test]
    fn admits_a_path_under_logs_because_that_rule_is_about_writing() {
        // "Inside the write directory and not under Logs" is where the
        // client may *write*. Reading is judged by the roots, so a
        // readable file under Logs that a caller allowed is admitted.
        let s = scene();
        let logs = dir(&s.b, "Saved Games\\DCS\\Logs");
        let roots = Roots::new(
            &[s.project.clone(), logs.clone()],
            std::slice::from_ref(&s.writedir),
            Some(&s.install),
        )
        .expect("the roots resolve");
        let real = file(&logs.as_path().join("note.lua"), b"return 1\n");
        roots.judge(&real).expect("Logs is not a write rule here");
    }

    // ---- the credentials --------------------------------------------------

    #[test]
    fn refuses_a_config_path_though_it_is_under_a_root() {
        // The rule is "inside a root or not": an allowed root covering
        // Config does not excuse it.
        let s = scene();
        let config = real(&s.writedir.as_path().join("Config"));
        let roots = Roots::new(
            std::slice::from_ref(&s.writedir),
            std::slice::from_ref(&s.writedir),
            Some(&s.install),
        )
        .expect("the roots resolve");
        let real = file(&config.as_path().join("network.vault"), b"x");
        let err = roots.judge(&real).expect_err("Config is always refused");
        assert!(matches!(err.kind, Refusal::Credentials { .. }), "{err}");
        assert!(
            err.to_string().contains(&config.to_string()),
            "the rule names Config: {err}"
        );
    }

    #[test]
    fn refuses_a_config_path_outside_every_root_with_the_same_rule() {
        // Outside every root too, so the stronger rule is what answers and
        // not the roots rule that would also have refused it.
        let s = scene();
        let config = real(&s.writedir.as_path().join("Config"));
        let under_root = Roots::new(
            std::slice::from_ref(&s.writedir),
            std::slice::from_ref(&s.writedir),
            Some(&s.install),
        )
        .expect("the roots resolve");
        let vault = file(&config.as_path().join("network.vault"), b"x");
        let a = under_root.judge(&vault).expect_err("refused");
        let b = s.roots().judge(&vault).expect_err("refused here too");
        assert!(matches!(b.kind, Refusal::Credentials { .. }), "{b}");
        // Both fixtures are the same path, so the reason is what is being
        // compared; the whole message would be comparing the path with
        // itself.
        assert_eq!(a.reason(), b.reason(), "one rule, one wording");
    }

    #[test]
    fn refuses_every_supplied_variants_config_and_only_those() {
        // A machine normally carries several DCS variants side by side, each
        // with its own vault, and an allowed root as wide as `Saved Games`
        // covers all of them. Each write directory the caller supplies is
        // guarded; one it does not name is not, which is the boundary
        // between this library and whoever finds the variants.
        let s = scene();
        let saved = dir(&s.b, "Saved Games");
        let beta = dir(&s.b, "Saved Games\\DCS.openbeta");
        let server = dir(&s.b, "Saved Games\\DCS.release_server");
        let roots = Roots::new(
            std::slice::from_ref(&saved),
            &[s.writedir.clone(), beta.clone()],
            Some(&s.install),
        )
        .expect("the roots resolve");
        for wd in [&s.writedir, &beta] {
            let vault = file(&wd.as_path().join("Config").join("network.vault"), b"x");
            let err = roots
                .judge(&vault)
                .expect_err("a supplied variant is guarded");
            assert!(matches!(err.kind, Refusal::Credentials { .. }), "{err}");
        }
        let unnamed = file(&server.as_path().join("Config").join("network.vault"), b"x");
        roots
            .judge(&unnamed)
            .expect("a variant nobody supplied has no root to fire on");
    }

    #[test]
    fn a_writedir_the_caller_did_not_supply_leaves_config_with_no_root() {
        // No write directory means no Config to fire on. It is not a rule
        // about the word: `C:\project\Config\x.lua` is an ordinary path.
        let s = scene();
        let roots = Roots::new(std::slice::from_ref(&s.project), &[], Some(&s.install))
            .expect("the roots resolve");
        let real = file(
            &s.project.as_path().join("Config").join("x.lua"),
            b"return 1\n",
        );
        roots
            .judge(&real)
            .expect("a project's own Config is not the vault");
    }

    // ---- the install ------------------------------------------------------

    #[test]
    fn refuses_an_install_path_naming_the_install() {
        // Outside every allowed root as well, so the install rule is what
        // answers rather than the roots rule.
        let s = scene();
        let real = file(
            &s.install
                .as_path()
                .join("Scripts")
                .join("MissionScripting.lua"),
            b"return 1\n",
        );
        let err = s.roots().judge(&real).expect_err("the install is refused");
        assert!(matches!(err.kind, Refusal::Install { .. }), "{err}");
        assert_eq!(
            err.reason(),
            format!("is inside the install, {}", s.install)
        );
    }

    #[test]
    fn refuses_the_install_through_an_eight_dot_three_short_spelling() {
        let b = Sandbox::new();
        let project = dir(&b, "project");
        let install = dir(&b, "DCS World OpenBeta");
        let roots = Roots::new(&[project], &[], Some(&install)).expect("the roots resolve");
        let short = short_name(install.as_path());
        let path = short.join("Scripts").join("x.lua");
        let real = file(&path, b"return 1\n");
        let err = roots
            .judge(&real)
            .expect_err("the short spelling is refused");
        assert_eq!(
            err.reason(),
            format!("is inside the install, {install}"),
            "the refusal names the install as it really is spelled"
        );
    }

    #[test]
    fn refuses_the_install_through_a_junction() {
        let b = Sandbox::new();
        let project = dir(&b, "project");
        let install = dir(&b, "DCS World");
        fs::create_dir_all(install.as_path().join("Scripts")).expect("Scripts");
        let link = b.join("project").join("link");
        junction(&link, &install.as_path().join("Scripts"));
        let roots = Roots::new(&[project], &[], Some(&install)).expect("the roots resolve");
        let path = link.join("x.lua");
        let real = file(&path, b"return 1\n");
        let err = roots.judge(&real).expect_err("the junction is followed");
        assert_eq!(
            err.reason(),
            format!("is inside the install, {install}"),
            "a link inside an allowed root is still the install"
        );
    }

    #[test]
    fn an_install_the_caller_did_not_supply_is_not_a_rule() {
        let s = scene();
        let roots = Roots::new(
            std::slice::from_ref(&s.install),
            std::slice::from_ref(&s.writedir),
            None,
        )
        .expect("the roots resolve");
        let real = file(&s.install.as_path().join("x.lua"), b"return 1\n");
        roots
            .judge(&real)
            .expect("with no install supplied there is no install rule");
    }

    // ---- what a refusal may say -------------------------------------------

    #[test]
    fn a_path_that_is_not_there_is_refused_in_the_same_words_as_one_that_is() {
        // A refusal never answers "does this exist", which is the file
        // listing this module refuses, asked one path at a time.
        let s = scene();
        let there = file(&s.outside.as_path().join("there.lua"), b"return 1\n");
        let not_there = real(&s.outside.as_path().join("not-there.lua"));
        assert!(!not_there.as_path().exists(), "the second is absent");
        let roots = s.roots();
        let a = roots.judge(&there).expect_err("refused");
        let b = roots.judge(&not_there).expect_err("refused");
        // The paths differ, so it is the reason that has to match; the
        // whole messages never could.
        assert_eq!(a.reason(), b.reason(), "existence is not disclosed");
    }

    // ---- the ceiling ------------------------------------------------------

    /// A handshake from the stand-in's own bytes. The ceiling has to come
    /// off one of these rather than out of a constant, which is why every
    /// test below needs a session to have published.
    fn handshake_bytes(b: &Sandbox) -> Vec<u8> {
        let root = b.path.join("session");
        fs::create_dir_all(&root).expect("the session root");
        let s = Standin::open(&root, "hook").expect("the session opens");
        s.handshake().expect("the handshake publishes");
        slurp(&s.output().join("executor.txt"))
    }

    fn handshake(bytes: &[u8]) -> Handshake {
        Handshake::from_bytes(Path::new("executor.txt"), bytes).expect("the handshake reads")
    }

    /// A plausible header set for a request. What is in it does not matter;
    /// how many bytes it frames to does.
    const HEADERS: &[(&str, &str)] = &[("op", "eval"), ("state", "hook")];

    fn block_len(headers: &[(&str, &str)]) -> u64 {
        protocol::frame(headers, b"")
            .expect("the headers frame")
            .len() as u64
    }

    /// A file of exactly `size` bytes under the allowed root, resolved.
    fn sized(s: &Box_, name: &str, size: u64) -> Real {
        file(
            &s.project.as_path().join(name),
            &vec![b'x'; size as usize][..],
        )
    }

    #[test]
    fn refuses_a_file_one_byte_over_the_file_ceiling_naming_the_limit_and_the_size() {
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let header_bytes = block_len(HEADERS);
        let size = h.max_request_bytes - header_bytes + 1;
        let real = sized(&s, "big.lua", size);
        let err = check(&s.roots(), &h, HEADERS, &real).expect_err("one byte over");
        assert!(
            matches!(err.kind, Refusal::Oversize { .. }),
            "over the ceiling: {err}"
        );
        let line = err.to_string();
        assert!(line.contains(&size.to_string()), "names the size: {line}");
        assert!(
            line.contains(&h.max_request_bytes.to_string()),
            "names the limit: {line}"
        );
    }

    #[test]
    fn admits_a_file_exactly_at_the_file_ceiling() {
        // The file's ceiling is the request ceiling less the header block,
        // and such a file frames to exactly the request ceiling, which the
        // executor's own `size > MAX_REQUEST_BYTES` accepts.
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let header_bytes = block_len(HEADERS);
        let size = h.max_request_bytes - header_bytes;
        let real = sized(&s, "exact.lua", size);
        let ok = check(&s.roots(), &h, HEADERS, &real).expect("exactly at the ceiling");
        assert_eq!(ok.size(), size, "the size is what the stat said");
        assert_eq!(ok.headroom(), 0, "and nothing is left over");
    }

    #[test]
    fn the_header_block_counts_toward_the_ceiling() {
        // One file, two header sets one byte apart. The executor stats the
        // whole request file, so the envelope's own bytes are part of what
        // has to fit.
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let short: &[(&str, &str)] = &[("op", "eval"), ("state", "hook")];
        let long: &[(&str, &str)] = &[("op", "eval"), ("state", "hooks")];
        assert_eq!(
            block_len(long),
            block_len(short) + 1,
            "the two blocks differ by one byte"
        );
        let size = h.max_request_bytes - block_len(long) + 1;
        let real = sized(&s, "edge.lua", size);
        let roots = s.roots();
        check(&roots, &h, short, &real).expect("it fits under the shorter block");
        let err = check(&roots, &h, long, &real).expect_err("and not under the longer one");
        assert!(matches!(err.kind, Refusal::Oversize { .. }), "{err}");
    }

    #[test]
    fn the_ceiling_is_the_handshakes_own_and_not_the_default() {
        let s = scene();
        let h = handshake(&with(&handshake_bytes(&s.b), "max_request_bytes", "1024"));
        assert_eq!(h.max_request_bytes, 1024, "the session says 1024");
        let real = sized(&s, "two-thousand.lua", 2_000);
        let err = check(&s.roots(), &h, HEADERS, &real).expect_err("over this session's ceiling");
        let line = err.to_string();
        assert!(line.contains("1024"), "the session's figure: {line}");
        assert!(line.contains("2000"), "and the file's: {line}");
    }

    #[test]
    fn a_header_block_at_the_limit_refuses_even_an_empty_file() {
        // The case the limit-less-the-header spelling gets wrong: that
        // subtraction saturates to nothing and then admits a file whose
        // framed request is already over.
        let s = scene();
        let header_bytes = block_len(HEADERS);
        let limit = (header_bytes - 1).to_string();
        let h = handshake(&with(&handshake_bytes(&s.b), "max_request_bytes", &limit));
        let real = sized(&s, "empty.lua", 0);
        let err = check(&s.roots(), &h, HEADERS, &real).expect_err("the headers alone are over");
        assert!(matches!(err.kind, Refusal::Oversize { .. }), "{err}");
    }

    #[test]
    fn refuses_headers_the_framer_would_refuse_before_the_stat() {
        // The path is one that is not there, so an implementation that
        // stated first would answer about the stat instead.
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let real = real(&s.project.as_path().join("absent.lua"));
        let bad: &[(&str, &str)] = &[("op", "eval\nstate: hook")];
        let err = check(&s.roots(), &h, bad, &real).expect_err("the framer refuses");
        assert!(matches!(err.kind, Refusal::Frame(_)), "{err}");
    }

    #[test]
    fn refuses_a_chunkname_carrying_a_byte_past_ascii_before_the_stat() {
        // A resolved path with a byte past ASCII cannot go on the wire at
        // all, and that is settled before the file is asked about.
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let real = real(&s.project.as_path().join("absent.lua"));
        let bad: &[(&str, &str)] = &[("chunkname", "@C:\\Users\\Ünter\\x.lua")];
        let err = check(&s.roots(), &h, bad, &real).expect_err("the value is not ASCII");
        assert!(matches!(err.kind, Refusal::Frame(_)), "{err}");
    }

    #[test]
    fn a_stat_that_fails_is_named_and_is_not_a_containment_answer() {
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let real = real(&s.project.as_path().join("absent.lua"));
        let err = check(&s.roots(), &h, HEADERS, &real).expect_err("nothing to state");
        assert!(matches!(err.kind, Refusal::Stat(_)), "{err}");
        assert_ne!(
            err.reason(),
            "is not under any allowed root",
            "a failed stat is not a verdict about the roots"
        );
    }

    #[test]
    fn the_oversize_refusal_offers_the_two_ways_out() {
        let s = scene();
        let h = handshake(&with(&handshake_bytes(&s.b), "max_request_bytes", "1024"));
        let real = sized(&s, "too-big.lua", 2_000);
        let line = check(&s.roots(), &h, HEADERS, &real)
            .expect_err("over the ceiling")
            .to_string();
        assert!(line.contains("split the file"), "{line}");
        assert!(line.contains("dofile"), "{line}");
    }

    // ---- before the file is opened ----------------------------------------
    //
    // The same three containment fixtures, each held open with nothing
    // shared, so this process cannot read them. A guard that read first and
    // refused afterwards cannot pass here: it would fail on the read. The
    // stat still answers under such a hold, so these are specific to
    // reading rather than reddening anything that merely touches the file.
    //
    // The whole of `check` is driven, not `judge` alone, because the order
    // these prove is the order inside `check`.

    /// The bytes every held fixture carries, and the token no refusal may
    /// repeat back.
    const SECRET: &[u8] = b"local password = 'hunter2'\n";

    #[test]
    fn a_held_file_outside_every_root_is_refused_without_being_opened() {
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let path = s.outside.as_path().join("secret.lua");
        let real = file(&path, SECRET);
        let hold = held(&path);
        let err = check(&s.roots(), &h, HEADERS, &real).expect_err("refused");
        drop(hold);
        assert_eq!(err.reason(), "is not under any allowed root", "{err}");
    }

    #[test]
    fn a_held_config_file_is_refused_without_being_opened() {
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let path = s.writedir.as_path().join("Config").join("network.vault");
        let real = file(&path, SECRET);
        let hold = held(&path);
        let err = check(&s.roots(), &h, HEADERS, &real).expect_err("refused");
        drop(hold);
        assert!(matches!(err.kind, Refusal::Credentials { .. }), "{err}");
    }

    #[test]
    fn a_held_install_file_is_refused_without_being_opened() {
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let path = s.install.as_path().join("MissionScripting.lua");
        let real = file(&path, SECRET);
        let hold = held(&path);
        let err = check(&s.roots(), &h, HEADERS, &real).expect_err("refused");
        drop(hold);
        assert!(matches!(err.kind, Refusal::Install { .. }), "{err}");
    }

    #[test]
    fn no_refusal_carries_a_byte_of_the_file_or_an_open_error() {
        // A refusal that leaked the first line would be the compile-error
        // read this module exists to prevent, arriving by another door; one
        // that leaked the open error would be an admission that the file
        // was opened.
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let fixtures = [
            s.outside.as_path().join("secret.lua"),
            s.writedir.as_path().join("Config").join("network.vault"),
            s.install.as_path().join("MissionScripting.lua"),
        ];
        let roots = s.roots();
        for path in fixtures {
            let real = file(&path, SECRET);
            let hold = held(&path);
            let line = check(&roots, &h, HEADERS, &real)
                .expect_err("refused")
                .to_string();
            drop(hold);
            assert!(
                !line.contains("password") && !line.contains("hunter2"),
                "the refusal repeats the file back: {line}"
            );
            assert!(
                !line.contains("another process") && !line.contains("os error"),
                "the refusal admits the file was opened: {line}"
            );
        }
    }
}
