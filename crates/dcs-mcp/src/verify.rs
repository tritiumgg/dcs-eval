//! What the installation looks like from outside, asked without changing it.
//!
//! **Nothing here writes.** Not a lock file, not a log beside what it
//! inspected, not a directory made on the way to a read. The tree being
//! looked at is somebody's DCS install: `Export.lua` belongs to SRS and
//! Tacview as much as to this build, and `autoexec.cfg` belongs to whoever
//! set the user's graphics and network options years before this binary
//! existed. A verb that answered "is this installed correctly?" by editing
//! one of them would destroy another tool's configuration silently, and the
//! only version of that promise worth making is one a test can hold: the
//! tree a verification read is byte for byte the tree it was handed.
//!
//! **A problem is not a refusal**, the way the session report is not one:
//! [`verify_at`] returns a [`Report`] and never a `Result`, because a verb
//! that errored when nothing is installed would be useless for the one
//! question it exists to answer. Everything found sits in
//! [`problems`](Report::problems).
//!
//! And a differing DCS build is not a problem at all. It is reported in
//! [`app_version`](Report::app_version) as the difference it is — a patched
//! game is the ordinary state of an install, and a report that filed every
//! patch among its faults would cry wolf on the line a user reads first.
//! There is deliberately no problem variant a version could be filed under.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use dcs_eval::paths::Real;
use dcs_eval::sha256::{digest, hex};
use dcs_eval::status::{self, VersionCheck};

use crate::embed;
use crate::export_line;
use crate::install::Executor;

/// What was at the hook's name, as far as its hash can say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hook {
    /// Nothing is there, so DCS loads nothing of ours.
    Absent,
    /// A file whose hash this project has shipped. `current` says whether
    /// it is the release this binary carries, as against an older one.
    Ours { sha256: String, current: bool },
    /// A file whose hash this project never shipped.
    Foreign { sha256: String },
    /// The directory or the file would not be read.
    Unreadable { why: String },
}

/// How many of `Export.lua`'s lines load the hook.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    /// The file is there and the line is not in it.
    Absent,
    /// Once, which is the only healthy answer.
    Once,
    /// Twice or more: DCS runs the hook once per line.
    Repeated {
        count: usize,
    },
    /// There is no `Export.lua` at all, so the line cannot be in it.
    NoFile,
    Unreadable {
        why: String,
    },
}

/// Whether `autoexec.cfg` could be read at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GateFile {
    /// Not there. Reported, and not a problem: DCS writes the file only
    /// when something has changed an option, so a fresh install has none.
    Absent,
    Read,
    Unreadable {
        why: String,
    },
}

/// The two keys in `autoexec.cfg` that decide what the executor may do.
///
/// Each is reported as the text it is written with and is never parsed into
/// a yes or a no. Nothing in this build has measured how DCS spells these
/// values — a boolean, a list of state names, something else — and a parser
/// that guessed would report a key it failed to understand as a key that is
/// not there, which is the one answer a user must never be given about a
/// policy gate. The states this build needs are in the handshake the
/// session half already carries; reading the two against each other into a
/// verdict is a measurement and not a read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gate {
    pub path: PathBuf,
    pub unsafe_api: Option<String>,
    pub dostring_in: Option<String>,
    pub file: GateFile,
}

/// Everything a verification can find worth saying. `Display` is the
/// project's `<path>: <reason>` wherever a path is involved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    HookAbsent {
        path: PathBuf,
    },
    HookUnreadable {
        path: PathBuf,
        why: String,
    },
    HookNotOurs {
        path: PathBuf,
        sha256: String,
    },
    HookNotTheCurrentRelease {
        path: PathBuf,
        sha256: String,
    },
    /// A second file DCS would load beside ours, registering its callbacks
    /// a second time.
    OtherHook {
        path: PathBuf,
    },
    ExportFileAbsent {
        path: PathBuf,
    },
    ExportLineAbsent {
        path: PathBuf,
    },
    ExportLineRepeated {
        path: PathBuf,
        count: usize,
    },
    ExportFileUnreadable {
        path: PathBuf,
        why: String,
    },
    AutoexecUnreadable {
        path: PathBuf,
        why: String,
    },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HookAbsent { path } => write!(
                f,
                "{}: the executor is not there, so DCS loads nothing of ours",
                path.display()
            ),
            Self::HookUnreadable { path, why } => {
                write!(f, "{}: could not be read: {why}", path.display())
            }
            Self::HookNotOurs { path, sha256 } => write!(
                f,
                "{}: sha256 {sha256} is not one this project has shipped, so the file is \
                 somebody else's work",
                path.display()
            ),
            Self::HookNotTheCurrentRelease { path, sha256 } => write!(
                f,
                "{}: sha256 {sha256} is one of ours, but not the release this binary carries; \
                 install again to bring it up to date",
                path.display()
            ),
            Self::OtherHook { path } => write!(
                f,
                "{}: a second executor DCS would run beside ours, registering its callbacks \
                 twice",
                path.display()
            ),
            Self::ExportFileAbsent { path } => write!(
                f,
                "{}: no Export.lua, so nothing loads the executor into the export host",
                path.display()
            ),
            Self::ExportLineAbsent { path } => write!(
                f,
                "{}: the line that loads the executor is not in it",
                path.display()
            ),
            Self::ExportLineRepeated { path, count } => write!(
                f,
                "{}: the line that loads the executor is in it {count} times, so it is loaded \
                 {count} times",
                path.display()
            ),
            Self::ExportFileUnreadable { path, why } => {
                write!(f, "{}: would not read: {why}", path.display())
            }
            Self::AutoexecUnreadable { path, why } => write!(
                f,
                "{}: would not read, so the policy gate could not be reported: {why}",
                path.display()
            ),
        }
    }
}

/// The whole report. Infallible by construction, like the session report it
/// carries.
#[derive(Clone, Debug)]
pub struct Report {
    /// The write directory this was taken against.
    pub variant: PathBuf,
    /// The build this binary carries, as one line.
    pub release: String,
    pub hook: Hook,
    pub line: Line,
    /// What is readable of the executor session, taken by the same reader
    /// `dcs_status` uses, so the two can never disagree about the same two
    /// files.
    pub session: status::Status,
    /// The running build against the one handed in. Never a problem.
    pub app_version: VersionCheck,
    pub gate: Gate,
    pub problems: Vec<Problem>,
}

impl Report {
    /// Whether anything at all was found, on either half.
    ///
    /// The version is not consulted and must not be: a build that differs
    /// from the one measured is a difference, not a fault.
    pub fn verified(&self) -> bool {
        self.problems.is_empty() && self.session.problems.is_empty()
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.release)?;
        writeln!(f, "variant: {}", self.variant.display())?;
        match &self.hook {
            Hook::Absent => writeln!(f, "hook: not there")?,
            Hook::Ours {
                sha256,
                current: true,
            } => writeln!(
                f,
                "hook: ours, sha256 {sha256}, the release this binary carries"
            )?,
            Hook::Ours {
                sha256,
                current: false,
            } => writeln!(f, "hook: ours, sha256 {sha256}, an older release")?,
            Hook::Foreign { sha256 } => writeln!(f, "hook: sha256 {sha256}, never shipped by us")?,
            Hook::Unreadable { why } => writeln!(f, "hook: could not be read: {why}")?,
        }
        match &self.line {
            Line::Absent => writeln!(f, "Export.lua: no line loading the executor")?,
            Line::Once => writeln!(f, "Export.lua: one line loading the executor")?,
            Line::Repeated { count } => writeln!(f, "Export.lua: {count} such lines")?,
            Line::NoFile => writeln!(f, "Export.lua: not there")?,
            Line::Unreadable { why } => writeln!(f, "Export.lua: would not read: {why}")?,
        }
        writeln!(f, "{}: {}", self.gate.path.display(), gate_file(&self.gate))?;
        writeln!(
            f,
            "net.allow_unsafe_api: {}",
            written(&self.gate.unsafe_api)
        )?;
        writeln!(
            f,
            "net.allow_dostring_in: {}",
            written(&self.gate.dostring_in)
        )?;
        match &self.session.session {
            None => writeln!(f, "session: none published")?,
            Some(session) => {
                writeln!(
                    f,
                    "session: {} pid {}, {}",
                    session.stamp, session.pid, session.process
                )?;
                match &session.beat {
                    None => writeln!(f, "heartbeat: none written, so nothing has armed it")?,
                    Some(beat) => {
                        writeln!(f, "heartbeat: phase {}, {}", beat.phase, beat.age)?;
                    }
                }
            }
        }
        writeln!(f, "app_version: {}", self.app_version)?;
        for problem in &self.problems {
            writeln!(f, "problem: {problem}")?;
        }
        for problem in &self.session.problems {
            writeln!(f, "problem: {problem}")?;
        }
        let found = self.problems.len() + self.session.problems.len();
        if found == 0 {
            f.write_str("verified")
        } else {
            write!(f, "not verified: {found} found")
        }
    }
}

/// How the policy-gate file itself read, in one phrase.
fn gate_file(gate: &Gate) -> String {
    match &gate.file {
        GateFile::Absent => "not there, so neither key is set".to_owned(),
        GateFile::Read => "read".to_owned(),
        GateFile::Unreadable { why } => format!("would not read: {why}"),
    }
}

/// A key as it is written, or the fact that it is not written at all.
fn written(value: &Option<String>) -> String {
    match value {
        Some(text) => format!("{text}, as written"),
        None => "not set in the file".to_owned(),
    }
}

/// Verify the installation under `variant`, reporting the session at
/// `output`, against `release` and the DCS build `measured`.
///
/// The release and the measured build are parameters for the reason the
/// placement's release is one: the shipped list has a single entry today
/// and [`status::MEASURED_ON`] is `None`, so "ours but an older release"
/// and "the running build differs" are both unreachable through the real
/// constants — and a control that watches a difference it cannot produce
/// watches nothing.
pub fn verify_at(
    variant: &Real,
    output: &Path,
    release: &Executor<'_>,
    measured: Option<&str>,
    now: SystemTime,
) -> Report {
    let mut problems = Vec::new();

    let hooks = variant.as_path().join("Scripts").join("Hooks");
    let (hook, strays) = look_at_hooks(&hooks, release, &mut problems);
    for stray in strays {
        problems.push(Problem::OtherHook { path: stray });
    }

    let export = export_line::path(variant);
    let line = look_at_export(&export, &mut problems);

    // The same reader `dcs_status` runs, which is what keeps the two from
    // drifting about what `executor.txt` and the heartbeat mean. It refuses
    // any protocol but the one this client speaks, so a file written by a
    // executor of another protocol arrives as an unreadable handshake
    // naming the version it carried, rather than as a silent agreement.
    let session = status::status_at(output, now);
    let app_version = status::measured_against(
        session
            .session
            .as_ref()
            .and_then(|session| session.app_version.running()),
        measured,
    );

    let gate = look_at_gate(
        &variant.as_path().join("Config").join("autoexec.cfg"),
        &mut problems,
    );

    Report {
        variant: variant.as_path().to_owned(),
        release: embed::release_line(),
        hook,
        line,
        session,
        app_version,
        gate,
        problems,
    }
}

/// The same, against the release this binary carries, the build it was
/// measured on, and the system clock.
pub fn verify(variant: &Real, output: &Path) -> Report {
    verify_at(
        variant,
        output,
        &Executor::embedded(),
        status::MEASURED_ON,
        SystemTime::now(),
    )
}

/// What a leaf name has to start with to be a hook one of these two
/// projects wrote.
///
/// The placement matches the prior project by two exact leaf names; this
/// matches a prefix, and the difference is deliberate. A placement only
/// moves files it can identify, so a pattern there would sweep up a file
/// somebody else happened to name that way. A report moves nothing and
/// names everything DCS would load beside ours, which is the wider question
/// and the one a user asking "why is every event handled twice?" is really
/// asking.
const STRAY_PREFIXES: [&str; 2] = ["dcseval", "dcsapi"];

/// One read of `Scripts\Hooks`: what is at the hook's name, and every other
/// file there that DCS would load as a second executor.
fn look_at_hooks(
    hooks: &Path,
    release: &Executor<'_>,
    problems: &mut Vec<Problem>,
) -> (Hook, Vec<PathBuf>) {
    let mut ours: Option<PathBuf> = None;
    let mut strays: Vec<PathBuf> = Vec::new();
    match fs::read_dir(hooks) {
        Ok(entries) => {
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(why) => {
                        problems.push(Problem::HookUnreadable {
                            path: hooks.to_owned(),
                            why: why.to_string(),
                        });
                        return (
                            Hook::Unreadable {
                                why: why.to_string(),
                            },
                            strays,
                        );
                    }
                };
                // Windows matches names with case folded, so a hook written
                // back shouted is the same file to the game and has to be
                // the same file here.
                let leaf = entry.file_name().to_string_lossy().into_owned();
                if leaf.eq_ignore_ascii_case(release.name) {
                    ours = Some(entry.path());
                    continue;
                }
                let folded = leaf.to_ascii_lowercase();
                if folded.ends_with(".lua") && STRAY_PREFIXES.iter().any(|p| folded.starts_with(p))
                {
                    strays.push(entry.path());
                }
            }
        }
        // DCS makes the directory only when something has been put in it,
        // so its absence is an install that has not happened and not a disk
        // failure.
        Err(why) if why.kind() == io::ErrorKind::NotFound => {}
        Err(why) => {
            problems.push(Problem::HookUnreadable {
                path: hooks.to_owned(),
                why: why.to_string(),
            });
            return (
                Hook::Unreadable {
                    why: why.to_string(),
                },
                strays,
            );
        }
    }
    // So that two strays are named in the same order every time, whatever
    // order the filesystem hands them back in.
    strays.sort();

    let Some(path) = ours else {
        problems.push(Problem::HookAbsent {
            path: hooks.join(release.name),
        });
        return (Hook::Absent, strays);
    };
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(why) => {
            problems.push(Problem::HookUnreadable {
                path,
                why: why.to_string(),
            });
            return (
                Hook::Unreadable {
                    why: why.to_string(),
                },
                strays,
            );
        }
    };
    let sha256 = hex(&digest(&bytes));
    if !release.shipped.contains(&sha256.as_str()) {
        problems.push(Problem::HookNotOurs {
            path,
            sha256: sha256.clone(),
        });
        return (Hook::Foreign { sha256 }, strays);
    }
    let current = sha256 == release.sha256;
    if !current {
        problems.push(Problem::HookNotTheCurrentRelease {
            path,
            sha256: sha256.clone(),
        });
    }
    (Hook::Ours { sha256, current }, strays)
}

/// How many of `Export.lua`'s lines load the hook, counted by the module
/// that writes the line — so the count that decided whether to append is
/// the count reported here.
fn look_at_export(export: &Path, problems: &mut Vec<Problem>) -> Line {
    match fs::read(export) {
        Ok(bytes) => match export_line::occurrences(&bytes) {
            0 => {
                problems.push(Problem::ExportLineAbsent {
                    path: export.to_owned(),
                });
                Line::Absent
            }
            1 => Line::Once,
            count => {
                problems.push(Problem::ExportLineRepeated {
                    path: export.to_owned(),
                    count,
                });
                Line::Repeated { count }
            }
        },
        Err(why) if why.kind() == io::ErrorKind::NotFound => {
            problems.push(Problem::ExportFileAbsent {
                path: export.to_owned(),
            });
            Line::NoFile
        }
        Err(why) => {
            problems.push(Problem::ExportFileUnreadable {
                path: export.to_owned(),
                why: why.to_string(),
            });
            Line::Unreadable {
                why: why.to_string(),
            }
        }
    }
}

/// The two policy-gate keys, as the text they are written with.
fn look_at_gate(path: &Path, problems: &mut Vec<Problem>) -> Gate {
    let gate = |file, unsafe_api, dostring_in| Gate {
        path: path.to_owned(),
        unsafe_api,
        dostring_in,
        file,
    };
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        // Not a problem. DCS writes this file only when an option has been
        // changed, so a fresh install has none and neither key is set.
        Err(why) if why.kind() == io::ErrorKind::NotFound => {
            return gate(GateFile::Absent, None, None);
        }
        Err(why) => {
            problems.push(Problem::AutoexecUnreadable {
                path: path.to_owned(),
                why: why.to_string(),
            });
            return gate(
                GateFile::Unreadable {
                    why: why.to_string(),
                },
                None,
                None,
            );
        }
    };
    // Lossy on purpose: this file is hand-edited on machines set to a
    // Windows codepage and may hold bytes that are not UTF-8 at all. The
    // two keys and their values are ASCII, and a byte elsewhere that is not
    // is no reason to report the file as unreadable.
    let text = String::from_utf8_lossy(&bytes);
    gate(
        GateFile::Read,
        assigned(&text, "net.allow_unsafe_api"),
        assigned(&text, "net.allow_dostring_in"),
    )
}

/// What `key` is assigned in `text`, where it is assigned anything.
///
/// The last assignment wins, because that is the one Lua would leave
/// standing after running the file. Comment lines are skipped so that a key
/// somebody commented out is reported as not set rather than as set.
fn assigned(text: &str, key: &str) -> Option<String> {
    let mut found = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("--") {
            continue;
        }
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        // Without this, `net.allow_dostring_in_future` would be read as the
        // key it merely begins with.
        let rest = rest.trim_start();
        let Some(value) = rest.strip_prefix('=') else {
            continue;
        };
        found = Some(value.trim().to_owned());
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::process::Command;
    use std::time::{Duration, UNIX_EPOCH};

    use dcs_eval::paths;
    use dcs_eval::standin::Standin;

    use crate::testing::Sandbox;

    /// Every expectation compares resolved paths, never the spelling that
    /// made them: the host's temp directory is usually spelled short.
    fn real(path: &Path) -> Real {
        paths::resolve(path).expect("the path resolves")
    }

    /// A file with known bytes, and whatever directories it needs.
    fn put(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().expect("a file has a parent"))
            .expect("the directories are made");
        fs::write(path, bytes).expect("the file is written");
    }

    /// One instant, used wherever the test does not care which.
    fn an_instant() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_760_000_000)
    }

    const OLDER: &[u8] = b"-- an older release\n";
    const CURRENT: &[u8] = b"-- this release\n";

    /// A release with two shipped hashes, so that "ours, older" and "ours,
    /// current" are both reachable. The embedded list has one entry, which
    /// makes an older release unreachable through it.
    fn a_release() -> (Executor<'static>, String, String) {
        let older = Box::leak(hex(&digest(OLDER)).into_boxed_str());
        let current = Box::leak(hex(&digest(CURRENT)).into_boxed_str());
        let shipped: &'static [&'static str] =
            Box::leak(vec![&*older, &*current].into_boxed_slice());
        (
            Executor {
                name: "DcsEvalExecutor.lua",
                bytes: CURRENT,
                sha256: current,
                shipped,
            },
            older.to_owned(),
            current.to_owned(),
        )
    }

    /// A policy gate written the awkward way a real one is: a comment, an
    /// unrelated key, one value that is a boolean and one that is not.
    const AUTOEXEC: &[u8] = b"-- generated by DCS\n\
        net.download_recent_missions = false\n\
        net.allow_unsafe_api = true\n\
        net.allow_dostring_in = {\"mission\", \"server\"}\n";

    /// The layout a real call meets: one variant under a `Saved Games`, and
    /// the output directory where the options would compose it — inside the
    /// variant, which is what makes the clean-tree assertion mean something.
    fn fixture() -> (Sandbox, Real, PathBuf) {
        let b = Sandbox::new();
        b.dir("saved");
        let variant = real(&b.dir("saved/DCS.openbeta"));
        b.dir("saved/DCS.openbeta/Scripts/Hooks");
        b.dir("saved/DCS.openbeta/Config");
        let output = variant.as_path().join("Logs").join("DcsEval").join("hook");
        (b, variant, output)
    }

    /// One healthy install, which each test then perturbs in exactly one
    /// way: our current release at the hook's name, the line in an
    /// `Export.lua` that holds somebody else's lines too, the policy gate,
    /// and a published handshake naming a process that is running.
    fn installed(variant: &Real, output: &Path) -> Standin {
        let hooks = variant.as_path().join("Scripts").join("Hooks");
        put(&hooks.join("DcsEvalExecutor.lua"), CURRENT);
        put(
            &export_line::path(variant),
            format!(
                "-- Tacview\ndofile(lfs.writedir()..'Scripts/Hooks/SRS.lua')\n{}\n",
                export_line::LINE
            )
            .as_bytes(),
        );
        put(
            &variant.as_path().join("Config").join("autoexec.cfg"),
            AUTOEXEC,
        );
        let mut ex = Standin::open(output, "hook").expect("the stand-in opens");
        // A pid somebody is running, so the session half reports a live
        // process rather than a gone one.
        ex.pid = std::process::id();
        ex.handshake().expect("the handshake is published");
        // The stand-in names a temp directory of its own under the output,
        // which no real executor would: `lfs.tempdir()` inside DCS gives
        // the host's. Said the stand-in's way, every healthy fixture here
        // would carry a temp-directory disagreement nothing asked for.
        reheader(
            &output.join("executor.txt"),
            "lfs_tempdir",
            &std::env::temp_dir().display().to_string(),
        );
        ex
    }

    /// The published handshake with one header carrying another value.
    ///
    /// A whole-line rewrite, which the envelope admits because it is header
    /// lines and a blank line and nothing counts the bytes.
    fn reheader(path: &Path, name: &str, value: &str) {
        let was = fs::read_to_string(path).expect("the handshake reads");
        let head = format!("{name}: ");
        let mut out = String::new();
        let mut found = false;
        for line in was.split_inclusive('\n') {
            if line.starts_with(&head) {
                out.push_str(&head);
                out.push_str(value);
                out.push('\n');
                found = true;
            } else {
                out.push_str(line);
            }
        }
        assert!(found, "the fixture carries no {name} header to replace");
        fs::write(path, out.as_bytes()).expect("the handshake lands");
    }

    /// The whole report as a user would read it.
    fn rendered(report: &Report) -> String {
        report.to_string()
    }

    /// `git`, isolated from whatever the developer has configured.
    ///
    /// The global and system configuration are pointed at a path inside the
    /// sandbox that does not exist, so the signer, the user's identity and
    /// above all `core.autocrlf` cannot reach this repository: a machine set
    /// to `input` reports a committed CRLF file as modified with nothing
    /// having written to it, which would redden this control for a reason
    /// that is not the control.
    fn git(box_: &Sandbox, at: &Path, args: &[&str]) -> String {
        let nowhere = box_.join("no-such-gitconfig");
        let out = Command::new("git")
            .current_dir(at)
            .env("GIT_CONFIG_GLOBAL", &nowhere)
            .env("GIT_CONFIG_SYSTEM", &nowhere)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args([
                "-c",
                "init.defaultBranch=main",
                "-c",
                "user.name=verify",
                "-c",
                "user.email=verify@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.autocrlf=false",
                "-c",
                "core.safecrlf=false",
            ])
            .args(args)
            .output()
            .unwrap_or_else(|why| panic!("git {args:?} would not start: {why}"));
        assert!(
            out.status.success(),
            "git {args:?} failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Every file under `dir`, as a sorted list of relative path and bytes.
    ///
    /// Taken beside the `git status` assertion and not instead of it: git
    /// does not track an empty directory, so a directory made on the way to
    /// a read is something git would call clean and this would not.
    fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
        fn walk(root: &Path, at: &Path, into: &mut Vec<(String, Vec<u8>)>) {
            let entries = match fs::read_dir(at) {
                Ok(entries) => entries,
                Err(_) => return,
            };
            for entry in entries {
                let entry = entry.expect("the entry reads");
                let path = entry.path();
                if path.is_dir() {
                    into.push((
                        format!(
                            "{}/",
                            path.strip_prefix(root)
                                .expect("under the root")
                                .to_string_lossy()
                        ),
                        Vec::new(),
                    ));
                    walk(root, &path, into);
                } else {
                    into.push((
                        path.strip_prefix(root)
                            .expect("under the root")
                            .to_string_lossy()
                            .into_owned(),
                        fs::read(&path).expect("the file reads"),
                    ));
                }
            }
        }
        let mut found = Vec::new();
        walk(dir, dir, &mut found);
        found.sort();
        found
    }

    #[test]
    fn both_policy_gate_keys_are_reported_as_they_are_written() {
        let (_b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        let (release, _older, _current) = a_release();

        let report = verify_at(&variant, &output, &release, None, an_instant());

        assert_eq!(report.gate.file, GateFile::Read);
        assert_eq!(report.gate.unsafe_api.as_deref(), Some("true"));
        assert_eq!(
            report.gate.dostring_in.as_deref(),
            Some("{\"mission\", \"server\"}"),
            "reported as the text it is written with, not parsed into a yes"
        );
        assert!(report.problems.is_empty(), "{:?}", report.problems);
    }

    #[test]
    fn an_absent_autoexec_is_reported_and_is_not_a_problem() {
        let (_b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        let (release, _older, _current) = a_release();
        fs::remove_file(variant.as_path().join("Config").join("autoexec.cfg"))
            .expect("the gate file goes");

        let report = verify_at(&variant, &output, &release, None, an_instant());

        assert_eq!(report.gate.file, GateFile::Absent);
        assert_eq!(report.gate.unsafe_api, None);
        assert_eq!(report.gate.dostring_in, None);
        assert!(
            report.verified(),
            "a file DCS writes only when an option changed is not a fault: {}",
            rendered(&report)
        );
    }

    #[test]
    fn a_second_hook_file_beside_ours_is_named_a_problem() {
        let (_b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        let (release, _older, _current) = a_release();
        let stray = variant
            .as_path()
            .join("Scripts")
            .join("Hooks")
            .join("DcsApiEval.lua");
        put(&stray, b"-- the prior project\n");

        let report = verify_at(&variant, &output, &release, None, an_instant());

        assert!(
            report
                .problems
                .iter()
                .any(|p| matches!(p, Problem::OtherHook { path } if path == &stray)),
            "{:?}",
            report.problems
        );
        assert!(!report.verified());
        assert!(
            rendered(&report).contains("DcsApiEval.lua"),
            "the file is named where a user would read it: {}",
            rendered(&report)
        );
    }

    #[test]
    fn a_duplicated_dofile_line_is_named_a_problem() {
        let (_b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        let (release, _older, _current) = a_release();
        let export = export_line::path(&variant);
        put(
            &export,
            format!("{}\n-- Tacview\n{}\n", export_line::LINE, export_line::LINE).as_bytes(),
        );

        let report = verify_at(&variant, &output, &release, None, an_instant());

        assert_eq!(report.line, Line::Repeated { count: 2 });
        assert!(
            report.problems.iter().any(|p| matches!(
                p,
                Problem::ExportLineRepeated { path, count } if path == &export && *count == 2
            )),
            "{:?}",
            report.problems
        );
        assert!(!report.verified());
    }

    #[test]
    fn a_healthy_install_verifies() {
        let (_b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        let (release, _older, current) = a_release();

        let report = verify_at(&variant, &output, &release, None, an_instant());

        assert_eq!(
            report.hook,
            Hook::Ours {
                sha256: current,
                current: true
            }
        );
        assert_eq!(report.line, Line::Once);
        assert!(
            !report
                .problems
                .iter()
                .any(|p| matches!(p, Problem::OtherHook { .. })),
            "{:?}",
            report.problems
        );
        assert!(
            report.verified(),
            "so every negative above is about what it perturbed: {}",
            rendered(&report)
        );
    }

    #[test]
    fn a_hook_whose_hash_we_never_shipped_is_named() {
        let (_b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        let (release, _older, _current) = a_release();
        let stranger = b"-- somebody else's hook\n";
        let hook = variant
            .as_path()
            .join("Scripts")
            .join("Hooks")
            .join("DcsEvalExecutor.lua");
        put(&hook, stranger);

        let report = verify_at(&variant, &output, &release, None, an_instant());

        let sha = hex(&digest(stranger));
        assert_eq!(
            report.hook,
            Hook::Foreign {
                sha256: sha.clone()
            }
        );
        assert!(
            report.problems.iter().any(|p| matches!(
                p,
                Problem::HookNotOurs { sha256, .. } if sha256 == &sha
            )),
            "{:?}",
            report.problems
        );
    }

    #[test]
    fn an_older_shipped_hook_is_ours_and_still_reported() {
        let (_b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        let (release, older, _current) = a_release();
        put(
            &variant
                .as_path()
                .join("Scripts")
                .join("Hooks")
                .join("DcsEvalExecutor.lua"),
            OLDER,
        );

        let report = verify_at(&variant, &output, &release, None, an_instant());

        assert_eq!(
            report.hook,
            Hook::Ours {
                sha256: older.clone(),
                current: false
            }
        );
        assert!(
            report.problems.iter().any(|p| matches!(
                p,
                Problem::HookNotTheCurrentRelease { sha256, .. } if sha256 == &older
            )),
            "{:?}",
            report.problems
        );
    }

    #[test]
    fn an_executor_file_that_is_not_protocol_two_is_named() {
        let (_b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        let (release, _older, _current) = a_release();
        reheader(&output.join("executor.txt"), "protocol", "3");

        let report = verify_at(&variant, &output, &release, None, an_instant());

        let said = report
            .session
            .problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            said.contains("protocol: 3"),
            "the reader names the protocol it would not speak: {said}"
        );
        assert!(!report.verified());
    }

    #[test]
    fn a_differing_app_version_is_a_difference_and_never_a_refusal() {
        let (_b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        let (release, _older, _current) = a_release();

        // The stand-in publishes 2.9.10.1234; this is a different build.
        let report = verify_at(
            &variant,
            &output,
            &release,
            Some("2.9.29.27278"),
            an_instant(),
        );

        assert!(
            matches!(report.app_version, VersionCheck::Differs { .. }),
            "{:?}",
            report.app_version
        );
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        assert!(
            report.session.problems.is_empty(),
            "{:?}",
            report.session.problems
        );
        assert!(
            report.verified(),
            "a build that differs from the one measured is a difference, not a refusal"
        );
        assert!(
            rendered(&report).ends_with("verified"),
            "and the report a user reads still says so: {}",
            rendered(&report)
        );
    }

    #[test]
    fn a_matching_app_version_says_so() {
        let (_b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        let (release, _older, _current) = a_release();

        let report = verify_at(
            &variant,
            &output,
            &release,
            Some("2.9.10.1234"),
            an_instant(),
        );

        assert_eq!(
            report.app_version,
            VersionCheck::Same {
                build: "2.9.10.1234".to_owned()
            },
            "so the test beside this one is about the difference and not about \
             the field being ignored"
        );
    }

    #[test]
    fn the_tree_verify_read_is_git_clean_afterwards() {
        let (b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        let (release, _older, _current) = a_release();
        let saved = b.join("saved");

        git(&b, &saved, &["init", "-q"]);
        git(&b, &saved, &["add", "."]);
        git(&b, &saved, &["commit", "-q", "-m", "the install as found"]);
        let before = snapshot(&saved);

        let report = verify_at(&variant, &output, &release, None, an_instant());

        // A verification that wrote nothing because it read nothing would
        // pass the two assertions below on its own, so the report is held
        // to having been a full one first.
        assert!(
            matches!(report.hook, Hook::Ours { .. }),
            "{:?}",
            report.hook
        );
        assert_eq!(report.line, Line::Once);
        assert_eq!(report.gate.file, GateFile::Read);

        let dirty = git(&b, &saved, &["status", "--porcelain"]);
        assert!(
            dirty.trim().is_empty(),
            "verify wrote into the install it was asked about:\n{dirty}"
        );
        assert_eq!(
            snapshot(&saved),
            before,
            "verify changed the tree in a way git does not track, such as a \
             directory made on the way to a read"
        );
    }

    #[test]
    fn verify_reports_an_install_that_is_not_there() {
        let (_b, variant, output) = fixture();
        let (release, _older, _current) = a_release();

        let report = verify_at(&variant, &output, &release, None, an_instant());

        assert_eq!(report.hook, Hook::Absent);
        assert_eq!(report.line, Line::NoFile);
        assert!(
            report
                .session
                .problems
                .iter()
                .any(|p| matches!(p, status::Problem::NotInstalled { .. })),
            "{:?}",
            report.session.problems
        );
        assert!(!report.verified());
    }

    #[test]
    fn verify_of_the_embedded_release_is_the_one_a_report_names() {
        let (_b, variant, output) = fixture();

        let report = verify(&variant, &output);

        assert_eq!(
            report.release,
            embed::release_line(),
            "the line a user reads names the build this binary carries"
        );
    }
}
