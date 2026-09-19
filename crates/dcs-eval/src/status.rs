//! What a client can say about an executor session without disturbing it.
//!
//! This is the report a user asks for first — is it installed, is it alive,
//! is it armed, and what is wrong — and the whole point of it is what it
//! does not do. It reads the handshake, reads the heartbeat, probes the
//! process id the handshake named, and stats the arm path the handshake
//! named. That is the entire budget. It publishes nothing, it ensures no
//! arm file, it opens no transport, it lists neither the request nor the
//! reply directory, and it never sleeps. A dormant session is not woken by
//! being asked about, which is the property that lets a caller ask as often
//! as it likes without the game noticing.
//!
//! **A problem is not a refusal.** [`status`] returns a [`Status`] and not a
//! `Result`, by construction: a report that errored because the executor is
//! not installed would be useless for the one question a user asks first.
//! A file that is missing, a file that will not parse, a heartbeat left by
//! somebody else, a process that is gone — each is reported, in the
//! [`problems`](Status::problems) list, with the session reported as far as
//! it could be read.
//!
//! Three fields a reader might look for are not here: how many requests are
//! answered, how many are queued, and whether the executor is busy. The
//! executor does not keep them and the heartbeat does not carry them —
//! decision record 0010 is why — and this module reports what it has and
//! says nothing it cannot see. Inventing a queue depth by listing the
//! request directory would also spend a read the budget above does not
//! allow, for a number that would be wrong the moment a frame ran.
//!
//! This module reports inputs; [`wait::decide`](crate::wait::decide) renders
//! a verdict. They read the same two files, and so must not disagree about
//! what they mean. Three rules keep them together: the heartbeat is read
//! before its stamp is looked at, so a file that is both foreign and
//! unreadable is a parse problem on either side; `armed` is read before any
//! age is taken; and a heartbeat belonging to another session is never
//! evidence about this one. Nothing here renders `stalled`, `waking`,
//! `dead` or `superseded` — those are one caller's reading of these inputs
//! against a request it sent, and this report has no request in hand.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use crate::paths::Real;
use crate::sys;

/// The build the embedded executor was last measured on.
///
/// Nothing has been measured yet: the binary embeds no executor and no run
/// against a real install has recorded the version it was measured under.
/// An invented figure would be worse than none, because every later report
/// would compare against it and call a truthful difference agreement. The
/// task that embeds an executor and the one that records a measurement
/// change this constant and reshape nothing else.
pub const MEASURED_ON: Option<&str> = None;

/// The whole report. Infallible by construction: everything that went
/// wrong is in `problems`, and `session` is what could still be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The output directory the report was taken against.
    pub output: PathBuf,
    /// The session the handshake named, where one could be read.
    pub session: Option<SessionStatus>,
    /// Everything worth reporting, in the order it was found.
    pub problems: Vec<Problem>,
}

/// The session the handshake named, and what the two other reads said
/// about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStatus {
    pub host: String,
    pub stamp: String,
    pub pid: u32,
    /// Local wall clock, display only.
    pub started: String,
    /// What the probe of `pid` established.
    pub process: Process,
    pub transport: Real,
    /// Whether this session will run a chunk at all.
    pub eval: bool,
    /// Whether the arm file the handshake named is on disk. Its absence is
    /// not a problem: it is one of the two things a caller needs in order
    /// to tell a stalled session from an unasked one, and which of those
    /// it is belongs to the caller.
    pub arm_file: bool,
    pub app_version: VersionCheck,
    /// What the executor's temp directory was, against this client's.
    pub tempdir: Agreement,
    /// The heartbeat, where there was one to read. `None` means the
    /// session has never armed, or that the file would not read — and in
    /// the second case there is a problem naming it.
    pub beat: Option<BeatStatus>,
}

/// The heartbeat as it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeatStatus {
    /// Whether this heartbeat's stamp is the handshake's.
    ///
    /// A field and not prose, because every other field here belongs to
    /// whichever session wrote the file. A printer with no flag to read
    /// would print another install's phase, ticks and age as this
    /// session's, which is exactly the confusion the foreign-stamp problem
    /// exists to surface.
    pub belongs: bool,
    pub host: String,
    pub transport: Real,
    pub phase: String,
    pub armed: bool,
    /// Local wall clock of the last arm or disarm, display only.
    pub since: String,
    pub ticks: u64,
    pub last_callback: Option<String>,
    pub callbacks: Vec<String>,
    pub age: Age,
}

/// How old the heartbeat file is, and what that number is worth.
///
/// An enum and not a duration beside a flag, so that no printer can carry
/// the number and forget the qualification. A dormant session stops
/// rewriting the file, so the age of a dormant one says when it went quiet
/// and nothing whatever about whether it is alive — and nothing in this
/// module reads a `Dormant` age as evidence. Decision record 0012 argues
/// the same reading on the deciding side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Age {
    /// The session was armed, so the file is being rewritten and its age
    /// is staleness.
    Ticking(Duration),
    /// The session was not armed. The number is when it went quiet.
    Dormant(Duration),
}

impl fmt::Display for Age {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ticking(d) => write!(f, "{}s since the last frame", d.as_secs()),
            Self::Dormant(d) => write!(
                f,
                "{}s since it went quiet, which is when it stopped writing and not whether it lives",
                d.as_secs()
            ),
        }
    }
}

/// What the probe of the process id established. This module's own view of
/// [`sys::Liveness`], which carries an `io::Error` and so is neither
/// `Clone` nor comparable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Process {
    Running,
    Exited,
    /// The probe could not decide — an access refusal among the reasons.
    /// Never folded into `Exited`: a handle that would not open says
    /// nothing about whether the process is there.
    Undecided {
        why: String,
    },
}

impl fmt::Display for Process {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Running => f.write_str("running"),
            Self::Exited => f.write_str("gone"),
            Self::Undecided { why } => write!(f, "could not be established: {why}"),
        }
    }
}

/// The running version against the build that was measured.
///
/// A difference is reported as a difference and never as a refusal or a
/// problem: a patched DCS is not a fault, and a report that listed every
/// patch among its problems would cry wolf on the first line a user reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionCheck {
    /// No build has been measured, so there is nothing to compare against.
    Unmeasured {
        running: Option<String>,
    },
    /// A build was measured and the session could not say what it runs.
    Unreadable {
        measured: String,
    },
    Same {
        build: String,
    },
    Differs {
        running: String,
        measured: String,
    },
}

impl fmt::Display for VersionCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unmeasured { running: Some(r) } => {
                write!(f, "{r}, against no measured build")
            }
            Self::Unmeasured { running: None } => {
                f.write_str("unreadable, against no measured build")
            }
            Self::Unreadable { measured } => {
                write!(f, "unreadable, and the measured build is {measured}")
            }
            Self::Same { build } => write!(f, "{build}, the build measured"),
            Self::Differs { running, measured } => {
                write!(f, "{running}, and the build measured was {measured}")
            }
        }
    }
}

/// The running version against `measured`, whatever either of them is.
///
/// A pure function, so all four answers are reachable from a test while
/// [`MEASURED_ON`] is `None` and none of them is dead code waiting for a
/// later task to switch it on.
pub fn measured_against(running: Option<&str>, measured: Option<&str>) -> VersionCheck {
    let Some(measured) = measured else {
        return VersionCheck::Unmeasured {
            running: running.map(str::to_owned),
        };
    };
    let Some(running) = running else {
        return VersionCheck::Unreadable {
            measured: measured.to_owned(),
        };
    };
    if running == measured {
        VersionCheck::Same {
            build: running.to_owned(),
        }
    } else {
        VersionCheck::Differs {
            running: running.to_owned(),
            measured: measured.to_owned(),
        }
    }
}

/// Whether two paths that should be the same directory are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Agreement {
    /// The executor said plainly that its own read did not answer.
    Absent,
    Agree(Real),
    Differ {
        executor: Real,
        client: Real,
    },
    /// One of the two would not resolve. The resolver's own words, and no
    /// accusation either way.
    Undecided {
        why: String,
    },
}

impl fmt::Display for Agreement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent => f.write_str("the executor's read did not answer"),
            Self::Agree(path) => write!(f, "{path}, which this client agrees with"),
            Self::Differ { executor, client } => {
                write!(f, "{executor}, and this client's is {client}")
            }
            Self::Undecided { why } => write!(f, "undecided: {why}"),
        }
    }
}

/// Everything a report can find worth saying. `Display` is
/// `<path>: <reason>` wherever a path is involved, which is the shape every
/// refusal in this crate takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// No handshake at all: nothing is installed, or nothing has loaded.
    NotInstalled { path: PathBuf },
    /// The handshake is there and would not read.
    HandshakeUnreadable { path: PathBuf, why: String },
    /// The heartbeat is there and would not read. The rest of the session
    /// is still reported.
    HeartbeatUnreadable { path: PathBuf, why: String },
    /// The heartbeat carries another session's stamp: two installs writing
    /// into one output directory, or a file a session that is gone left
    /// behind.
    ForeignStamp { saw: String, wanted: String },
    /// The heartbeat names a transport the handshake does not.
    ForeignTransport { saw: Real, wanted: Real },
    /// The heartbeat names a host the handshake does not.
    ForeignHost { saw: String, wanted: String },
    /// The probe said the process is gone.
    ProcessGone { pid: u32 },
    /// The probe could not decide. A problem because a user who asked
    /// whether it is alive got no answer — and worded so that it can never
    /// be read as death.
    ProcessUndecided { pid: u32, why: String },
    /// The executor's temp directory and this client's are not the same
    /// directory.
    TempdirDisagrees { executor: Real, client: Real },
    /// A path the executor reported rather than used, which the filesystem
    /// would not resolve. A finding about what the session saw, not a
    /// fault in the file.
    Unresolved {
        name: &'static str,
        named: String,
        why: String,
    },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled { path } => {
                write!(f, "{}: no handshake, so nothing has loaded", path.display())
            }
            Self::HandshakeUnreadable { path, why } => {
                write!(f, "{}: the handshake would not read: {why}", path.display())
            }
            Self::HeartbeatUnreadable { path, why } => {
                write!(f, "{}: the heartbeat would not read: {why}", path.display())
            }
            Self::ForeignStamp { saw, wanted } => write!(
                f,
                "the heartbeat is stamped {saw}, and this session is {wanted}"
            ),
            Self::ForeignTransport { saw, wanted } => write!(
                f,
                "the heartbeat names the transport {saw}, and this session's is {wanted}"
            ),
            Self::ForeignHost { saw, wanted } => write!(
                f,
                "the heartbeat names the host {saw}, and this session's is {wanted}"
            ),
            Self::ProcessGone { pid } => write!(f, "process {pid} has gone"),
            Self::ProcessUndecided { pid, why } => write!(
                f,
                "whether process {pid} is running could not be established: {why}"
            ),
            Self::TempdirDisagrees { executor, client } => write!(
                f,
                "the executor's temp directory is {executor}, and this client's is {client}"
            ),
            Self::Unresolved { name, named, why } => {
                write!(f, "{name}: {named} would not resolve: {why}")
            }
        }
    }
}

/// What a probe's answer means for the report, and what it is worth
/// reporting.
///
/// A pure function over an answer already in hand, because the interesting
/// case cannot be reached reliably through a live probe: a handle that will
/// not open answers `Unknown` unelevated and may open and answer `Running`
/// elevated, so a test that could only reach `Unknown` by probing would
/// prove nothing on an elevated host. `Unknown` never becomes `Exited`:
/// only a probe that positively says the process is gone may produce an
/// answer a caller could read as "this will never run".
pub fn process_of(pid: u32, saw: sys::Liveness) -> (Process, Option<Problem>) {
    match saw {
        sys::Liveness::Running => (Process::Running, None),
        sys::Liveness::Exited => (Process::Exited, Some(Problem::ProcessGone { pid })),
        sys::Liveness::Unknown { why } => {
            let why = why.to_string();
            (
                Process::Undecided { why: why.clone() },
                Some(Problem::ProcessUndecided { pid, why }),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, ErrorKind};

    #[test]
    fn a_measured_build_is_a_difference_and_never_a_refusal() {
        // All four answers, reachable here while nothing has been measured
        // yet, so none of them is dead code waiting on a later task.
        assert_eq!(
            measured_against(Some("2.9.10.1234"), Some("2.9.10.1234")),
            VersionCheck::Same {
                build: "2.9.10.1234".to_owned()
            }
        );
        assert_eq!(
            measured_against(Some("2.9.11.5000"), Some("2.9.10.1234")),
            VersionCheck::Differs {
                running: "2.9.11.5000".to_owned(),
                measured: "2.9.10.1234".to_owned()
            }
        );
        assert_eq!(
            measured_against(None, Some("2.9.10.1234")),
            VersionCheck::Unreadable {
                measured: "2.9.10.1234".to_owned()
            }
        );
        assert_eq!(
            measured_against(Some("2.9.10.1234"), None),
            VersionCheck::Unmeasured {
                running: Some("2.9.10.1234".to_owned())
            }
        );
    }

    #[test]
    fn an_unmeasured_build_says_so_and_is_not_a_problem() {
        // The seam the embedding task and the measuring task fill in. A
        // difference read as agreement while nothing is measured would
        // make both of them look already done.
        let saw = measured_against(Some("2.9.10.1234"), MEASURED_ON);
        assert_eq!(
            saw,
            VersionCheck::Unmeasured {
                running: Some("2.9.10.1234".to_owned())
            },
            "saw {saw:?}, wanted Unmeasured"
        );
        assert!(
            saw.to_string().contains("no measured build"),
            "the line says there is nothing to compare against: {saw}"
        );
    }

    #[test]
    fn an_age_while_dormant_says_the_number_means_nothing() {
        let d = Duration::from_secs(600);
        let ticking = Age::Ticking(d).to_string();
        assert!(
            ticking.contains("600s") && ticking.contains("last frame"),
            "an armed session's age is staleness: {ticking}"
        );
        let dormant = Age::Dormant(d).to_string();
        assert!(
            dormant.contains("600s") && dormant.contains("not whether it lives"),
            "a dormant session's age is qualified where it is printed: {dormant}"
        );
    }

    #[test]
    fn a_probe_that_could_not_decide_is_never_mapped_to_an_exit() {
        assert_eq!(
            process_of(7, sys::Liveness::Running),
            (Process::Running, None)
        );
        assert_eq!(
            process_of(7, sys::Liveness::Exited),
            (Process::Exited, Some(Problem::ProcessGone { pid: 7 }))
        );
        // Constructed rather than probed for: a handle that will not open
        // answers `Unknown` unelevated and may answer `Running` elevated,
        // so a live probe cannot reach this arm on every host.
        let why = io::Error::from(ErrorKind::PermissionDenied);
        let wanted = why.to_string();
        let (process, problem) = process_of(7, sys::Liveness::Unknown { why });
        assert_eq!(
            process,
            Process::Undecided {
                why: wanted.clone()
            }
        );
        assert_eq!(
            problem,
            Some(Problem::ProcessUndecided {
                pid: 7,
                why: wanted
            })
        );
        let line = Problem::ProcessUndecided {
            pid: 7,
            why: "denied".to_owned(),
        }
        .to_string();
        assert!(
            line.contains("could not be established") && !line.contains("gone"),
            "the line cannot be read as death: {line}"
        );
    }
}
