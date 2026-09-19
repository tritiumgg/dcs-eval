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
//! Three rules and a ceiling, in that order. A path under the write
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

use crate::paths::{self, PathError, Real};

/// The roots a file path is judged against: the directories a caller
/// allowed, the `Config` inside the write directory, and the install.
///
/// All three are supplied. Nothing here reads an argument vector, an
/// environment variable, or this process's current directory, and nothing
/// here takes the install from a handshake: a rule that fired only once a
/// handshake had been read would make a refusal depend on what had happened
/// earlier in the session rather than on what the caller configured.
#[derive(Clone, Debug)]
pub struct Roots {
    allowed: Vec<Real>,
    config: Option<Real>,
    install: Option<Real>,
}

impl Roots {
    /// The roots, with `<writedir>\Config` resolved once here so that a
    /// junction standing where `Config` should be is followed rather than
    /// walked around.
    ///
    /// An empty `allowed` is legal and admits nothing. It does not mean
    /// "anything goes" and it does not fall back to wherever this process
    /// is sitting, which would be a containment check on an accident. A
    /// `writedir` or `install` left out switches its rule off for want of
    /// a root to fire on, and the caller that leaves one out is the one
    /// that decided to go without that guard.
    pub fn new(
        allowed: &[Real],
        writedir: Option<&Real>,
        install: Option<&Real>,
    ) -> Result<Self, PathError> {
        let config = match writedir {
            Some(wd) => Some(paths::resolve(&wd.as_path().join("Config"))?),
            None => None,
        };
        Ok(Self {
            allowed: allowed.to_vec(),
            config,
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
        if let Some(config) = &self.config
            && config.contains(real)
        {
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
    /// Under the write directory's `Config`, where the account credentials
    /// live. Refused whether or not an allowed root covers it.
    Credentials { config: Real },
    /// Under the DCS install. Refused whether or not an allowed root covers
    /// it. The wording is the executor's own for the same rule at the other
    /// end, so one rule reads the same from both sides.
    Install { install: Real },
    /// Under no allowed root. The same words whether the path is there or
    /// not.
    OutsideEveryRoot,
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
    use crate::testing::{Sandbox, held, junction, real, short_name};

    use std::fs;
    use std::path::PathBuf;

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
                Some(&self.writedir),
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
        let roots = Roots::new(&[], None, None).expect("the roots resolve");
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
            Some(&s.writedir),
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
            Some(&s.writedir),
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
            Some(&s.writedir),
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
            Some(&s.writedir),
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
    fn a_writedir_the_caller_did_not_supply_leaves_config_with_no_root() {
        // No write directory means no Config to fire on. It is not a rule
        // about the word: `C:\project\Config\x.lua` is an ordinary path.
        let s = scene();
        let roots = Roots::new(std::slice::from_ref(&s.project), None, Some(&s.install))
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
        let roots = Roots::new(&[project], None, Some(&install)).expect("the roots resolve");
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
        let roots = Roots::new(&[project], None, Some(&install)).expect("the roots resolve");
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
        let roots = Roots::new(std::slice::from_ref(&s.install), Some(&s.writedir), None)
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
}
