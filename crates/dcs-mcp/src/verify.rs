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
    Repeated { count: usize },
    /// There is no `Export.lua` at all, so the line cannot be in it.
    NoFile,
    Unreadable { why: String },
}

/// Whether `autoexec.cfg` could be read at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GateFile {
    /// Not there. Reported, and not a problem: DCS writes the file only
    /// when something has changed an option, so a fresh install has none.
    Absent,
    Read,
    Unreadable { why: String },
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
            } => writeln!(f, "hook: ours, sha256 {sha256}, the release this binary carries")?,
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
        writeln!(f, "net.allow_unsafe_api: {}", written(&self.gate.unsafe_api))?;
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
                if folded.ends_with(".lua") && STRAY_PREFIXES.iter().any(|p| folded.starts_with(p)) {
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

