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
//! A file that is missing, a file that will not parse, a heartbeat another
//! session wrote since this one loaded, a process that is gone — each is
//! reported, in the [`problems`](Status::problems) list, with the session
//! reported as far as it could be read.
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
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::paths::{self, Real};
use crate::readers::{Diagnostic, Handshake, Heartbeat, ReadError, ReadErrorKind};
use crate::sys;
use crate::wait::Session;

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
    pub arm_file: ArmFile,
    pub app_version: VersionCheck,
    /// What the executor's temp directory was, against this client's.
    pub tempdir: Agreement,
    /// The heartbeat, where there was one to read. `None` means the
    /// session has never armed, or that the file would not read — and in
    /// the second case there is a problem naming it — or that the file
    /// there is a leftover.
    pub beat: Option<BeatStatus>,
    /// The stamp of a heartbeat another session wrote before this one
    /// published its handshake, where that is the file there.
    ///
    /// The executor writes no heartbeat at load, so after every relaunch
    /// the file is the last session's until the first arm replaces it. It
    /// is no evidence about any writer since this session began, so it is
    /// not a problem, and none of its fields are this session's, so it is
    /// not a `beat` either (ADR 0030).
    pub leftover: Option<String>,
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

/// What the stat of the arm path established.
///
/// Three answers and not a `bool`, for the reason [`Process`] has three:
/// a stat that failed over a permission refusal, a path this process
/// cannot traverse, or a disk that would not answer has not established
/// that nothing has armed the session — it has established nothing. A
/// `bool` would report every one of those as "not armed", which is a
/// caller's cue that the session is idle rather than stalled, and the one
/// place in this module where a failure to find out would be dressed as a
/// fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArmFile {
    /// The file is there.
    Present,
    /// The file is not there, which the filesystem said in those words.
    Absent,
    /// The stat failed for some reason other than the file not being
    /// there, so whether anything has armed this session is unknown.
    Undecided { why: String },
}

impl fmt::Display for ArmFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Present => f.write_str("armed"),
            Self::Absent => f.write_str("not armed"),
            Self::Undecided { why } => {
                write!(f, "whether it is armed could not be established: {why}")
            }
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

impl VersionCheck {
    /// What the session said it is running, where it said anything.
    ///
    /// A check is a comparison already made, and the running version is
    /// folded into it the moment it is read. A second reader wanting to
    /// hold the same session up against a different build would otherwise
    /// have to open the handshake again — a second read of one file, which
    /// could disagree with the first — so the reading is handed back out
    /// here instead.
    pub fn running(&self) -> Option<&str> {
        match self {
            Self::Unmeasured { running } => running.as_deref(),
            Self::Unreadable { .. } => None,
            Self::Same { build } => Some(build),
            Self::Differs { running, .. } => Some(running),
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

/// Whether the executor's temp directory is this client's, or lies in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Agreement {
    /// The executor said plainly that its own read did not answer.
    Absent,
    Agree(Real),
    /// Under this client's, at a segment boundary: DCS keeps a folder of
    /// its own inside the user's temp directory and hands its process that
    /// (ADR 0029).
    Within {
        executor: Real,
        client: Real,
    },
    /// Neither the same directory nor under it.
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
            Self::Within { executor, client } => {
                write!(f, "{executor}, under this client's {client}")
            }
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
    /// The heartbeat carries another session's stamp and was written since
    /// this session published its handshake, or at a time that could not
    /// be placed against it: two executors writing into one output
    /// directory. One written before is a leftover, and no problem.
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
    /// The arm path could not be stated. A problem because a caller who
    /// asked whether anything has armed the session got no answer — and
    /// worded so that it can never be read as "nothing has".
    ArmUndecided { path: PathBuf, why: String },
    /// The executor's temp directory is neither this client's nor under it.
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
            Self::ArmUndecided { path, why } => write!(
                f,
                "{}: whether the session is armed could not be established: {why}",
                path.display()
            ),
            Self::TempdirDisagrees { executor, client } => write!(
                f,
                "the executor's temp directory is {executor}, which is neither this \
                 client's {client} nor under it"
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

/// What a stat of the arm path means for the report, and what it is worth
/// reporting.
///
/// A pure function over an answer already in hand, for the reason
/// [`process_of`] is one: the interesting case cannot be reached reliably
/// through the filesystem. A refusal has to be arranged with an access
/// control list, and the arrangement answers differently depending on who
/// is running the tests — an elevated host traverses what an unelevated
/// one does not — so a test that could only reach `Undecided` by statting
/// a real path would prove nothing on half the machines it ran on.
///
/// Only `NotFound` becomes [`ArmFile::Absent`]. Every other failure is the
/// stat saying it could not tell, which is not the same finding and is
/// never folded into it.
pub fn arm_of(path: &Path, stat: std::io::Result<()>) -> (ArmFile, Option<Problem>) {
    match stat {
        Ok(()) => (ArmFile::Present, None),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => (ArmFile::Absent, None),
        Err(err) => {
            let why = err.to_string();
            (
                ArmFile::Undecided { why: why.clone() },
                Some(Problem::ArmUndecided {
                    path: path.to_owned(),
                    why,
                }),
            )
        }
    }
}

/// What one of the two reported-only paths is worth saying, if anything.
///
/// These two are written down so a client can say what the executor saw,
/// and one the filesystem would not own is reported here rather than
/// refusing the file that named it: a handshake naming a temp directory
/// nothing resolves is the finding, and refusing the file would hide the
/// finding behind the fault it describes. The judgement is the reader's
/// own [`Diagnostic::problem`]: a path the executor said plainly it could
/// not read is no finding, and one that resolved is none either.
fn unresolved(name: &'static str, d: &Diagnostic) -> Option<Problem> {
    match d {
        Diagnostic::Unresolved { named, why } => Some(Problem::Unresolved {
            name,
            named: named.clone(),
            why: why.clone(),
        }),
        Diagnostic::Absent | Diagnostic::Real(_) => None,
    }
}

/// Whether two resolved paths are the same directory.
///
/// Mutual containment rather than `==`. A `Real`'s equality is byte-exact
/// by design, and only `contains` folds case, so `==` would call a
/// differently-cased spelling of one directory two directories — a false
/// finding in the first line of a report a user reads.
fn same_place(a: &Real, b: &Real) -> bool {
    a.contains(b) && b.contains(a)
}

/// The executor's temp directory against this client's. A report and not
/// an accusation: where either side will not resolve the field carries the
/// resolver's own words instead of naming a culprit. One under this
/// client's is what DCS was measured to hand its process (ADR 0029), and is
/// its own answer rather than a disagreement.
fn tempdir_of(named: &Diagnostic) -> Agreement {
    let executor = match named {
        Diagnostic::Absent => return Agreement::Absent,
        // The spelling itself is the finding, and there is nothing here to
        // compare it with.
        Diagnostic::Unresolved { why, .. } => return Agreement::Undecided { why: why.clone() },
        Diagnostic::Real(path) => path.clone(),
    };
    let client = match paths::resolve(&std::env::temp_dir()) {
        Ok(client) => client,
        // A fact about this host, which says nothing about the executor.
        Err(why) => {
            return Agreement::Undecided {
                why: why.to_string(),
            };
        }
    };
    if same_place(&executor, &client) {
        Agreement::Agree(executor)
    } else if client.contains(&executor) {
        Agreement::Within { executor, client }
    } else {
        Agreement::Differ { executor, client }
    }
}

/// The heartbeat as this report carries it, and everything the file is
/// worth reporting.
///
/// The three comparisons are against the handshake, and all three are
/// worth making: two executors writing into one output directory is the
/// thing they exist to surface. A file a gone session left before this one
/// loaded never reaches here; the caller sets it aside as a leftover first.
/// `host` and `transport` are carried in the file for exactly this, so
/// comparing only the stamp would drop half of what they were kept for.
///
/// `armed` decides what the age is before the age is taken, and the arm is
/// read off this file's own header even where the file is another
/// session's, because the age is a fact about the file in hand.
fn beat_of(beat: &Heartbeat, h: &Handshake, now: SystemTime) -> (BeatStatus, Vec<Problem>) {
    let mut problems = Vec::new();
    if beat.stamp != h.stamp {
        problems.push(Problem::ForeignStamp {
            saw: beat.stamp.clone(),
            wanted: h.stamp.clone(),
        });
    }
    if beat.host != h.host {
        problems.push(Problem::ForeignHost {
            saw: beat.host.clone(),
            wanted: h.host.clone(),
        });
    }
    if !same_place(&beat.transport, &h.transport) {
        problems.push(Problem::ForeignTransport {
            saw: beat.transport.clone(),
            wanted: h.transport.clone(),
        });
    }
    let elapsed = beat.age(now);
    let age = if beat.armed {
        Age::Ticking(elapsed)
    } else {
        Age::Dormant(elapsed)
    };
    let status = BeatStatus {
        belongs: beat.stamp == h.stamp,
        host: beat.host.clone(),
        transport: beat.transport.clone(),
        phase: beat.phase.clone(),
        armed: beat.armed,
        since: beat.since.clone(),
        ticks: beat.ticks,
        last_callback: beat.last_callback.clone(),
        callbacks: beat.callbacks.clone(),
        age,
    };
    (status, problems)
}

/// Whether `beat` is another session's file, left before this one
/// published `h` at `published`.
///
/// Before, strictly: a file whose time equals the handshake's cannot be
/// placed on either side of it, and a time that would not read cannot be
/// placed at all, so both stay what they were, a foreign heartbeat and a
/// problem. Being wrong in that direction costs a line a user reads; the
/// other would hide a second executor (ADR 0030).
fn left_before(beat: &Heartbeat, h: &Handshake, published: Option<SystemTime>) -> bool {
    beat.stamp != h.stamp && published.is_some_and(|at| beat.modified < at)
}

/// The report on the session whose output directory is `output`, taken
/// against the system clock.
pub fn status(output: &Path) -> Status {
    status_at(output, SystemTime::now())
}

/// The report, with the clock the heartbeat's age is taken against handed
/// in — once, so every age in one report is against one reading of it and
/// a test can fix what it is.
pub fn status_at(output: &Path, now: SystemTime) -> Status {
    let mut problems = Vec::new();
    let (handshake, at) = match Handshake::read_published(&output.join("executor.txt")) {
        Ok(read) => read,
        Err(err) => {
            problems.push(not_read(err));
            return Status {
                output: output.to_owned(),
                session: None,
                problems,
            };
        }
    };
    // Through the session, so that this report and a wait take the
    // heartbeat's path from one place and cannot drift about where it is.
    let session = Session::addressed(&handshake);
    let (process, gone) = process_of(session.pid(), sys::liveness(session.pid()));
    problems.extend(gone);
    problems.extend(unresolved("lfs_tempdir", &handshake.lfs_tempdir));
    problems.extend(unresolved("install_guard", &handshake.install_guard));
    let tempdir = tempdir_of(&handshake.lfs_tempdir);
    // Only a disagreement between two paths that both resolved is worth
    // reporting, and one under this client's is none. Absent is the
    // executor saying its own read did not answer, and undecided is a path
    // one side or the other could not resolve — which is somebody's
    // finding, but not this one.
    if let Agreement::Differ { executor, client } = &tempdir {
        problems.push(Problem::TempdirDisagrees {
            executor: executor.clone(),
            client: client.clone(),
        });
    }

    // The file is read before its stamp is looked at. A stamp is something
    // only a file this reader understood has, so one that is both another
    // session's and unreadable is a parse problem rather than a foreign
    // one — the order a wait takes, for the same reason.
    // A stat, which costs the executor nothing and is the only way to ask
    // whether the file is there. Nothing here makes one and nothing here
    // removes one.
    let arm = handshake.arm.as_path();
    let (arm_file, undecided) = arm_of(arm, std::fs::metadata(arm).map(|_| ()));
    problems.extend(undecided);

    let mut leftover = None;
    let beat = match Heartbeat::read(session.heartbeat()) {
        // The last session's, not yet replaced because this one has not
        // armed: what every relaunch leaves.
        Ok(beat) if left_before(&beat, &handshake, at) => {
            leftover = Some(beat.stamp);
            None
        }
        Ok(beat) => {
            let (status, found) = beat_of(&beat, &handshake, now);
            problems.extend(found);
            Some(status)
        }
        // Never armed, so never written: the expected state right after a
        // load, and no problem at all.
        Err(err) if is_missing(&err) => None,
        Err(err) => {
            problems.push(Problem::HeartbeatUnreadable {
                path: err.path.clone(),
                why: err.kind.to_string(),
            });
            None
        }
    };

    Status {
        output: output.to_owned(),
        session: Some(SessionStatus {
            host: handshake.host.clone(),
            stamp: handshake.stamp.clone(),
            pid: handshake.pid,
            started: handshake.started.clone(),
            process,
            transport: handshake.transport.clone(),
            eval: handshake.eval,
            arm_file,
            app_version: measured_against(handshake.app_version.as_deref(), MEASURED_ON),
            tempdir,
            beat,
            leftover,
        }),
        problems,
    }
}

/// Whether a read failed because the file was not there, as against
/// failing over what was in it. The same discrimination a wait makes, for
/// the same reason: an absent file and an unreadable one are different
/// findings.
fn is_missing(err: &ReadError) -> bool {
    matches!(&err.kind, ReadErrorKind::Disk(why) if why.kind() == std::io::ErrorKind::NotFound)
}

/// What a handshake that would not read is worth saying: that nothing has
/// loaded, or that what did would not parse.
fn not_read(err: ReadError) -> Problem {
    if is_missing(&err) {
        Problem::NotInstalled { path: err.path }
    } else {
        Problem::HandshakeUnreadable {
            why: err.kind.to_string(),
            path: err.path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::{self, ErrorKind};

    use crate::standin::Standin;
    use crate::testing::{Sandbox, a_pid_that_has_exited, entries, real, slurp, with};

    /// A stand-in session with its handshake published, naming `pid` as
    /// its process and this host's temp directory as the one its `lfs`
    /// read gave it.
    ///
    /// The stand-in names a temp directory of its own under the output,
    /// which no real executor would: `lfs.tempdir()` inside DCS gives the
    /// host's, or a folder under it (ADR 0029). The fixture says the
    /// host's, so a report of a healthy session carries nothing a test did
    /// not ask for and a disagreement has to be published on purpose to
    /// appear.
    fn published(b: &Sandbox, pid: u32) -> Standin {
        let mut s = Standin::open(&b.join("out"), "hook").expect("the session opens");
        s.pid = pid;
        s.handshake().expect("the handshake publishes");
        let path = s.output().join("executor.txt");
        let temp = std::env::temp_dir().display().to_string();
        fs::write(&path, with(&slurp(&path), "lfs_tempdir", &temp)).expect("the fixture lands");
        s
    }

    /// The same, naming a process a probe will find running: this one.
    fn live(b: &Sandbox) -> Standin {
        published(b, std::process::id())
    }

    /// Where the executor's own files are, as the filesystem spells them.
    /// The sandbox sits under a temp directory this host may spell short,
    /// and every path a report carries has been through the resolver.
    fn beside(s: &Standin, name: &str) -> PathBuf {
        real(s.output()).as_path().join(name)
    }

    #[test]
    fn an_output_with_nothing_in_it_is_reported_as_not_installed() {
        let b = Sandbox::new();
        let report = status(&b.path);
        assert!(
            report.session.is_none(),
            "there is no session to report: {:?}",
            report.session
        );
        assert_eq!(
            report.problems,
            vec![Problem::NotInstalled {
                path: b.join("executor.txt")
            }],
            "an output directory with nothing in it is a problem reported, not an error raised"
        );
    }

    #[test]
    fn status_reports_the_session_the_handshake_names() {
        let b = Sandbox::new();
        let mut s = live(&b);
        s.armed = true;
        s.phase = "simulation".to_owned();
        s.tick = 7;
        s.beat(SystemTime::now()).expect("the heartbeat publishes");

        let report = status(s.output());
        assert_eq!(report.problems, vec![], "a healthy session reports nothing");
        let session = report.session.expect("the handshake named a session");
        assert_eq!(session.host, "hook");
        assert_eq!(session.stamp, s.stamp);
        assert_eq!(session.pid, std::process::id());
        assert_eq!(session.started, s.since);
        assert_eq!(session.process, Process::Running);
        assert_eq!(session.transport, real(s.session()));
        assert!(session.eval, "the stand-in publishes eval: allowed");
        let beat = session.beat.expect("the session has armed");
        assert!(beat.belongs, "the heartbeat is this session's");
        assert_eq!(beat.phase, "simulation");
        assert_eq!(beat.ticks, 7);
        assert!(beat.armed);
        assert_eq!(beat.since, s.since);
    }

    #[test]
    fn a_handshake_that_will_not_parse_is_a_problem_and_not_an_error() {
        let b = Sandbox::new();
        fs::write(b.join("executor.txt"), b"this is not an envelope").expect("the fixture lands");
        let report = status(&b.path);
        assert!(report.session.is_none());
        match &report.problems[..] {
            [Problem::HandshakeUnreadable { path, why }] => {
                assert_eq!(path, &b.join("executor.txt"));
                assert!(!why.is_empty(), "the reader's own words are carried");
            }
            other => panic!("saw {other:?}, wanted one HandshakeUnreadable"),
        }
    }

    #[test]
    fn a_session_that_never_armed_has_no_heartbeat_and_that_is_not_a_problem() {
        let b = Sandbox::new();
        let s = live(&b);
        let report = status(s.output());
        assert_eq!(
            report.problems,
            vec![],
            "a session that has not armed has written no heartbeat, which is the expected state"
        );
        let session = report.session.expect("the handshake named a session");
        assert_eq!(session.beat, None);
    }

    #[test]
    fn a_heartbeat_that_will_not_parse_is_a_problem_naming_the_file() {
        let b = Sandbox::new();
        let s = live(&b);
        fs::write(s.output().join("heartbeat.txt"), b"rubbish").expect("the fixture lands");
        let report = status(s.output());
        let session = report.session.as_ref().expect("the session still reports");
        assert_eq!(
            session.beat, None,
            "a heartbeat that would not read leaves nothing to carry"
        );
        match &report.problems[..] {
            [Problem::HeartbeatUnreadable { path, .. }] => {
                assert_eq!(path, &beside(&s, "heartbeat.txt"));
            }
            other => panic!("saw {other:?}, wanted one HeartbeatUnreadable"),
        }
    }

    #[test]
    fn an_armed_heartbeats_age_is_staleness() {
        let b = Sandbox::new();
        let mut s = live(&b);
        s.armed = true;
        let now = SystemTime::now();
        s.beat(now - Duration::from_secs(30))
            .expect("the heartbeat publishes");
        let report = status_at(s.output(), now);
        let age = report
            .session
            .expect("the session reports")
            .beat
            .expect("the session has armed")
            .age;
        match age {
            Age::Ticking(d) => assert_eq!(d.as_secs(), 30, "saw {age:?}"),
            other => panic!("saw {other:?}, wanted Ticking(30s)"),
        }
    }

    #[test]
    fn a_process_that_has_exited_is_reported_gone() {
        let b = Sandbox::new();
        // The `Child` is held across the probe, so the id cannot be
        // recycled under the test and a failure stays a failure.
        let (_child, pid) = a_pid_that_has_exited();
        let s = published(&b, pid);
        let report = status(s.output());
        assert!(
            report.problems.contains(&Problem::ProcessGone { pid }),
            "saw {:?}, wanted a ProcessGone",
            report.problems
        );
        assert_eq!(
            report.session.expect("the session reports").process,
            Process::Exited
        );
    }

    #[test]
    fn a_live_probe_that_would_not_open_is_never_read_as_death() {
        let b = Sandbox::new();
        // Pid 4 is the System process: unelevated the handle is refused,
        // elevated it may open and read running. Either is right; what
        // must never happen is a report that it has gone.
        let s = published(&b, 4);
        let report = status(s.output());
        assert!(
            !report
                .problems
                .iter()
                .any(|p| matches!(p, Problem::ProcessGone { .. })),
            "a handle that would not open is not evidence of an exit: {:?}",
            report.problems
        );
        assert_ne!(
            report.session.expect("the session reports").process,
            Process::Exited
        );
    }

    /// `name` respelt in the envelope at `path`, landed again with the
    /// modification time it had. The time is put back because the age a
    /// report carries comes from it and a fixture about a stamp is not
    /// about an age.
    fn respell(path: &Path, name: &str, value: &str) {
        let at = fs::metadata(path)
            .and_then(|meta| meta.modified())
            .expect("the fixture has a modification time");
        fs::write(path, with(&slurp(path), name, value)).expect("the fixture lands");
        fs::File::options()
            .write(true)
            .open(path)
            .and_then(|file| file.set_modified(at))
            .expect("the modification time goes back");
    }

    /// A session with a heartbeat published, and that heartbeat's path.
    fn beating(s: &Standin) -> PathBuf {
        s.beat(SystemTime::now()).expect("the heartbeat publishes");
        s.output().join("heartbeat.txt")
    }

    #[test]
    fn a_heartbeat_from_another_stamp_is_a_problem_naming_both() {
        let b = Sandbox::new();
        let s = live(&b);
        respell(&beating(&s), "stamp", "1700000000-999");
        let report = status(s.output());
        assert!(
            report.problems.contains(&Problem::ForeignStamp {
                saw: "1700000000-999".to_owned(),
                wanted: s.stamp.clone(),
            }),
            "both spellings are named: {:?}",
            report.problems
        );
    }

    /// `path`'s modification time set to `at`, its bytes untouched.
    fn touched(path: &Path, at: SystemTime) {
        fs::File::options()
            .write(true)
            .open(path)
            .and_then(|file| file.set_modified(at))
            .expect("the modification time lands");
    }

    /// What the live run met after a relaunch: the executor writes nothing
    /// at load, so the heartbeat there is the last session's — another
    /// stamp and another transport, written before this session's handshake
    /// — until the first arm replaces it.
    #[test]
    fn a_heartbeat_the_last_session_left_before_this_one_loaded_is_no_problem() {
        let b = Sandbox::new();
        let s = live(&b);
        let now = SystemTime::now();
        touched(
            &s.output().join("executor.txt"),
            now - Duration::from_secs(60),
        );
        let beat = beating(&s);
        respell(&beat, "stamp", "1700000000-999");
        respell(
            &beat,
            "transport",
            &b.join("the-last-session").display().to_string(),
        );
        touched(&beat, now - Duration::from_secs(900));
        let report = status_at(s.output(), now);
        assert_eq!(
            report.problems,
            vec![],
            "a file untouched since before this session loaded says nothing about a second writer"
        );
        let session = report.session.expect("the session reports");
        assert_eq!(
            session.beat, None,
            "none of the leftover's fields are this session's"
        );
        assert_eq!(session.leftover.as_deref(), Some("1700000000-999"));
    }

    /// The other side of the line: a second executor that wrote into this
    /// output after this session loaded, which is what the foreign-stamp
    /// problem is for, however old the stamp it carries.
    #[test]
    fn a_heartbeat_another_session_wrote_after_this_one_loaded_is_still_a_problem() {
        let b = Sandbox::new();
        let s = live(&b);
        let now = SystemTime::now();
        touched(
            &s.output().join("executor.txt"),
            now - Duration::from_secs(600),
        );
        let beat = beating(&s);
        respell(&beat, "stamp", "1700000000-999");
        touched(&beat, now - Duration::from_secs(300));
        let report = status_at(s.output(), now);
        assert!(
            report.problems.contains(&Problem::ForeignStamp {
                saw: "1700000000-999".to_owned(),
                wanted: s.stamp.clone(),
            }),
            "a file written since this session loaded is a second writer: {:?}",
            report.problems
        );
        let session = report.session.expect("the session reports");
        assert_eq!(session.leftover, None);
        assert!(
            !session.beat.expect("the file is carried, flagged").belongs,
            "and flagged as not this session's"
        );
    }

    /// A file stamped at the handshake's own instant cannot be placed on
    /// either side of it, and is not given the benefit of the doubt.
    #[test]
    fn a_foreign_heartbeat_written_at_the_handshakes_instant_is_still_a_problem() {
        let b = Sandbox::new();
        let s = live(&b);
        let at = SystemTime::now() - Duration::from_secs(60);
        touched(&s.output().join("executor.txt"), at);
        let beat = beating(&s);
        respell(&beat, "stamp", "1700000000-999");
        touched(&beat, at);
        let report = status(s.output());
        assert!(
            report
                .problems
                .iter()
                .any(|p| matches!(p, Problem::ForeignStamp { .. })),
            "saw {:?}",
            report.problems
        );
    }

    #[test]
    fn a_heartbeat_naming_another_host_is_a_problem() {
        let b = Sandbox::new();
        let s = live(&b);
        respell(&beating(&s), "host", "export");
        let report = status(s.output());
        assert!(
            report.problems.contains(&Problem::ForeignHost {
                saw: "export".to_owned(),
                wanted: "hook".to_owned(),
            }),
            "a heartbeat written by the other host is two installs in one output: {:?}",
            report.problems
        );
    }

    #[test]
    fn a_heartbeat_naming_another_transport_is_a_problem() {
        let b = Sandbox::new();
        let s = live(&b);
        let elsewhere = b.join("somewhere-else");
        respell(&beating(&s), "transport", &elsewhere.display().to_string());
        let report = status(s.output());
        assert!(
            report.problems.iter().any(|p| matches!(
                p,
                Problem::ForeignTransport { saw, .. } if saw == &real(&elsewhere)
            )),
            "a heartbeat naming a transport the handshake does not is a problem: {:?}",
            report.problems
        );
    }

    #[test]
    fn the_same_transport_spelt_in_another_case_is_not_a_foreign_transport() {
        let b = Sandbox::new();
        let s = live(&b);
        let beat = beating(&s);
        // A tail the filesystem has nothing to canonicalise keeps the case
        // it was spelt with, and that is the only way two files can name
        // one directory in two spellings: a directory that exists comes
        // back from the resolver in the case the disk holds, whatever
        // either file said about it.
        let named = s.session().join("sub").display().to_string();
        respell(&s.output().join("executor.txt"), "transport", &named);
        respell(&beat, "transport", &named.to_uppercase());
        let report = status(s.output());
        assert_eq!(
            report.problems,
            vec![],
            "one directory spelt two ways is one directory, not two installs"
        );
    }

    #[test]
    fn a_dormant_heartbeats_age_is_qualified_rather_than_read_as_staleness() {
        let b = Sandbox::new();
        let mut s = live(&b);
        s.armed = false;
        let now = SystemTime::now();
        s.beat(now - Duration::from_secs(600))
            .expect("the heartbeat publishes");
        let age = status_at(s.output(), now)
            .session
            .expect("the session reports")
            .beat
            .expect("there is a heartbeat")
            .age;
        match age {
            // A dormant session stops rewriting the file, so this number
            // is when it went quiet and not how stale a ticking one is.
            Age::Dormant(d) => assert_eq!(d.as_secs(), 600, "saw {age:?}"),
            other => panic!("saw {other:?}, wanted Dormant(600s)"),
        }
    }

    #[test]
    fn a_foreign_heartbeat_is_reported_flagged_rather_than_read_as_this_sessions() {
        let b = Sandbox::new();
        let mut s = live(&b);
        s.armed = true;
        s.phase = "simulation".to_owned();
        s.tick = 99;
        respell(&beating(&s), "stamp", "1700000000-999");
        let beat = status(s.output())
            .session
            .expect("the session reports")
            .beat
            .expect("the file is real and is carried");
        assert!(
            !beat.belongs,
            "the flag is what stops a printer reading another install's phase and ticks as this session's"
        );
        assert_eq!(beat.phase, "simulation");
        assert_eq!(beat.ticks, 99);
    }

    #[test]
    fn a_diagnostic_path_that_would_not_resolve_is_reported_not_refused() {
        for name in ["lfs_tempdir", "install_guard"] {
            let b = Sandbox::new();
            let s = live(&b);
            // A relative spelling: the resolver refuses one outright,
            // because the drive it would land on is an accident of where
            // the client was started.
            respell(&s.output().join("executor.txt"), name, "tmp\\somewhere");
            let report = status(s.output());
            assert!(
                report.session.is_some(),
                "{name} is reported and never used, so a path that will not resolve is a finding \
                 rather than a reason to refuse the file"
            );
            assert!(
                report.problems.iter().any(|p| matches!(
                    p,
                    Problem::Unresolved { name: n, named, .. } if *n == name && named == "tmp\\somewhere"
                )),
                "{name}: saw {:?}",
                report.problems
            );
        }
    }

    #[test]
    fn lfs_tempdir_disagreeing_with_this_clients_temp_directory_is_a_problem() {
        let b = Sandbox::new();
        let s = live(&b);
        let elsewhere = "C:\\not-this-hosts-temp-directory";
        respell(&s.output().join("executor.txt"), "lfs_tempdir", elsewhere);
        let report = status(s.output());
        let client = paths::resolve(&std::env::temp_dir()).expect("this host has a temp directory");
        assert!(
            report.problems.contains(&Problem::TempdirDisagrees {
                executor: real(Path::new(elsewhere)),
                client: client.clone(),
            }),
            "saw {:?}",
            report.problems
        );
        assert_eq!(
            report.session.expect("the session reports").tempdir,
            Agreement::Differ {
                executor: real(Path::new(elsewhere)),
                client,
            },
            "the field is the report and the problem is what it is worth saying"
        );
    }

    /// What DCS was measured to hand its process: a folder of its own
    /// inside the user's temp directory (ADR 0029). It need not exist for
    /// the resolver to place it, and nothing here makes it.
    #[test]
    fn lfs_tempdir_under_this_clients_temp_directory_is_within_and_no_problem() {
        let b = Sandbox::new();
        let s = live(&b);
        let client = paths::resolve(&std::env::temp_dir()).expect("this host has a temp directory");
        let dcs = client.as_path().join("DCS");
        respell(
            &s.output().join("executor.txt"),
            "lfs_tempdir",
            &dcs.display().to_string(),
        );
        let report = status(s.output());
        assert!(
            !report
                .problems
                .iter()
                .any(|p| matches!(p, Problem::TempdirDisagrees { .. })),
            "a temp directory DCS keeps inside this client's is not a disagreement: {:?}",
            report.problems
        );
        assert_eq!(
            report.session.expect("the session reports").tempdir,
            Agreement::Within {
                executor: real(&dcs),
                client,
            }
        );
    }

    /// Within is containment at a segment boundary, not a shared prefix:
    /// `...\TempDCS` spells the client's directory and more, and is not in
    /// it.
    #[test]
    fn lfs_tempdir_sharing_only_this_clients_bytes_is_a_problem() {
        let b = Sandbox::new();
        let s = live(&b);
        let client = paths::resolve(&std::env::temp_dir()).expect("this host has a temp directory");
        // Beside the client's directory rather than after its spelling, so
        // a temp directory at a drive root cannot make the sibling a child.
        let name = client
            .as_path()
            .file_name()
            .expect("this host's temp directory is not a drive root")
            .to_string_lossy();
        let sibling = client
            .as_path()
            .with_file_name(format!("{name}DCS"))
            .display()
            .to_string();
        respell(&s.output().join("executor.txt"), "lfs_tempdir", &sibling);
        let report = status(s.output());
        assert!(
            report.problems.contains(&Problem::TempdirDisagrees {
                executor: real(Path::new(&sibling)),
                client,
            }),
            "saw {:?}",
            report.problems
        );
    }

    #[test]
    fn a_tempdir_this_client_cannot_resolve_is_undecided_and_not_an_accusation() {
        let b = Sandbox::new();
        let s = live(&b);
        respell(&s.output().join("executor.txt"), "lfs_tempdir", "tmp");
        let report = status(s.output());
        assert!(
            !report
                .problems
                .iter()
                .any(|p| matches!(p, Problem::TempdirDisagrees { .. })),
            "a path that would not resolve is not two directories disagreeing: {:?}",
            report.problems
        );
        match report.session.expect("the session reports").tempdir {
            Agreement::Undecided { why } => assert!(
                !why.is_empty(),
                "the resolver's own words, and no culprit named"
            ),
            other => panic!("saw {other:?}, wanted Undecided"),
        }
    }

    #[test]
    fn status_says_whether_the_arm_file_exists() {
        let b = Sandbox::new();
        let s = live(&b);
        let before = status(s.output()).session.expect("the session reports");
        assert_eq!(
            before.arm_file,
            ArmFile::Absent,
            "nothing has armed this session, and the filesystem said so in those words"
        );
        fs::write(s.arm(), b"").expect("the arm file lands");
        let after = status(s.output()).session.expect("the session reports");
        assert_eq!(
            after.arm_file,
            ArmFile::Present,
            "the field is wired to the disk"
        );
    }

    #[test]
    fn a_stat_that_could_not_tell_is_undecided_and_never_not_armed() {
        let path = Path::new(r"C:\nowhere\dcs\arm.txt");
        assert_eq!(
            arm_of(path, Ok(())),
            (ArmFile::Present, None),
            "a stat that answered is the file being there"
        );
        assert_eq!(
            arm_of(path, Err(io::Error::from(ErrorKind::NotFound))),
            (ArmFile::Absent, None),
            "and the one failure that is a fact about the file is its absence"
        );
        // Every other failure. A permission refusal is the one a real host
        // hands over, and the point of the third answer is that it is not
        // the second: a caller reading `Absent` would tell its user the
        // session is idle when nothing established that.
        let (saw, problem) = arm_of(
            path,
            Err(io::Error::new(ErrorKind::PermissionDenied, "access denied")),
        );
        match saw {
            ArmFile::Undecided { why } => {
                assert!(why.contains("access denied"), "the stat's own words: {why}")
            }
            other => panic!("saw {other:?}, wanted Undecided"),
        }
        match problem {
            Some(Problem::ArmUndecided { path: named, why }) => {
                assert_eq!(
                    named, path,
                    "the problem names the path that would not stat"
                );
                assert!(why.contains("access denied"), "and why: {why}");
            }
            other => panic!("saw {other:?}, wanted ArmUndecided"),
        }
        // Worded so that no reader can take it for "nothing has armed it".
        let said = ArmFile::Undecided {
            why: "access denied".to_owned(),
        }
        .to_string();
        assert!(
            said.contains("could not be established"),
            "an unknown is not a denial: {said}"
        );
    }

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
    fn a_check_hands_back_the_running_build_it_folded_in() {
        // Every arm, for the reason the comparison's own four are tested
        // here: a second reader holding this session up against another
        // build takes the reading from the check rather than opening the
        // handshake again, so an arm that answered nothing would send it
        // back to the file.
        assert_eq!(
            measured_against(Some("2.9.10.1234"), Some("2.9.10.1234")).running(),
            Some("2.9.10.1234")
        );
        assert_eq!(
            measured_against(Some("2.9.11.5000"), Some("2.9.10.1234")).running(),
            Some("2.9.11.5000")
        );
        assert_eq!(
            measured_against(Some("2.9.10.1234"), None).running(),
            Some("2.9.10.1234")
        );
        assert_eq!(measured_against(None, None).running(), None);
        assert_eq!(
            measured_against(None, Some("2.9.10.1234")).running(),
            None,
            "nothing was read, so there is nothing to hand back"
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

    /// A stand-in session whose frame panics, holding the only handle on
    /// it, so that nothing in a test can tick it by accident.
    ///
    /// What the panicking frame is worth, plainly: [`status`] takes a path
    /// and cannot reach this double, so the panic guards a later `status`
    /// that is handed a driver and carries none of today's claim. Today's
    /// claim rests on what the test looks at afterwards — the four
    /// directories, the two files' bytes and modification times, the arm
    /// file, and a rename the OS refuses while a handle is open.
    struct Idle(Standin);

    impl Idle {
        fn output(&self) -> &Path {
            self.0.output()
        }

        fn session(&self) -> &Path {
            self.0.session()
        }

        fn req(&self) -> &Path {
            self.0.req()
        }

        fn res(&self) -> &Path {
            self.0.res()
        }

        fn arm(&self) -> &Path {
            self.0.arm()
        }

        // Never called, and that is the point: it is here so that a later
        // `status` handed this double goes red rather than quiet.
        #[allow(dead_code)]
        fn tick(&mut self) -> Vec<String> {
            panic!("status made a round trip: the executor ticked")
        }
    }

    #[test]
    fn status_costs_the_executor_nothing() {
        let b = Sandbox::new();
        let s = live(&b);
        s.beat(SystemTime::now() - Duration::from_secs(60))
            .expect("the heartbeat publishes");
        // From here the session is reachable only through the double.
        let s = Idle(s);

        let out = b.join("out");
        let handshake = s.output().join("executor.txt");
        let heartbeat = s.output().join("heartbeat.txt");
        let listing = |s: &Idle| {
            [
                entries(s.req()),
                entries(s.res()),
                entries(s.session()),
                entries(s.output()),
            ]
        };
        let stamped = |path: &Path| {
            let at = fs::metadata(path)
                .and_then(|meta| meta.modified())
                .expect("the file has a modification time");
            (slurp(path), at)
        };
        // The same four directories by their own modification times, which
        // is the trace a file that appeared and vanished leaves behind:
        // adding a name to a directory moves its time, and taking one away
        // moves it again.
        let touched = |s: &Idle| {
            [s.req(), s.res(), s.session(), s.output()].map(|dir| {
                fs::metadata(dir)
                    .and_then(|meta| meta.modified())
                    .expect("the directory has a modification time")
            })
        };
        // The clock those stamps come from advances in steps rather than
        // continuously, so a create and a remove inside one step could
        // leave a directory's time exactly where it already was. Stamping
        // a file outside the watched tree until it reads later than every
        // recorded directory puts the whole call after that step, and any
        // create during it is then stamped later still.
        let clock = b.join("clock");
        fs::create_dir_all(&clock).expect("the scratch directory is made");
        let past_the_clock = |dirs: &[SystemTime; 4]| {
            let latest = *dirs.iter().max().expect("four stamps");
            for i in 0..500u32 {
                // A fresh name each time: Windows can hand back a
                // rewritten file's earlier stamp, and a marker that never
                // appears to move would hang this loop rather than read
                // the clock.
                let marker = clock.join(format!("tick-{i}"));
                fs::write(&marker, b"").expect("the marker lands");
                let at = fs::metadata(&marker)
                    .and_then(|meta| meta.modified())
                    .expect("the marker has a modification time");
                if at > latest {
                    return;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            panic!("the filesystem's clock never moved past the directory stamps");
        };

        let before = listing(&s);
        assert_eq!(before[0], "", "nothing is published before the call");
        assert_eq!(before[1], "", "nothing has been replied to");
        assert_eq!(
            before[2], "req res",
            "the session directory holds its two directories and nothing else"
        );
        let files_before = (stamped(&handshake), stamped(&heartbeat));
        let dirs_before = touched(&s);
        past_the_clock(&dirs_before);

        let report = status(s.output());
        let session = report.session.expect("the session reports");

        assert_eq!(
            listing(&s),
            before,
            "nothing appeared and nothing vanished: no request, no arm file, no half-written file \
             beside either of the two"
        );
        // What the listing above cannot see, and why this line is here.
        // A `status` that published a request and took it straight back —
        // or made an arm file and deleted it — leaves the listing exactly
        // as it was and passes. The executor wakes on a name appearing and
        // does not care that it later went, so that is the failure this
        // whole function exists to catch, and the listing is blind to it.
        // These stamps are not: the create moves the directory's time and
        // the remove moves it again, the clock has been proved past every
        // recorded stamp, and neither move can be taken back.
        //
        // What is still unseen is the event rather than its trace: a
        // create in some directory none of these four is, a filesystem
        // that does not stamp its directories, and the order things
        // happened in within the call. Watching a create as it happens
        // needs the directory watch that is a later task's, and this test
        // has no watch.
        assert_eq!(
            touched(&s),
            dirs_before,
            "no directory's modification time moved, so nothing was created and removed inside \
             the call either"
        );
        assert_eq!(
            (stamped(&handshake), stamped(&heartbeat)),
            files_before,
            "neither file's bytes nor its modification time moved, so reading the heartbeat did \
             not refresh the stamp its age is taken from"
        );
        assert!(!s.arm().exists(), "nothing here ensures an arm file");
        assert_eq!(session.arm_file, ArmFile::Absent, "and the report says so");

        // No handle held. Windows refuses to rename a directory holding an
        // open file, so a handle left open on either file reddens this
        // line — and the line is only as good as the mutation that leaves
        // one open. If that mutation does not redden it, this check proved
        // nothing and is rebuilt rather than kept.
        let moved = b.join("out-moved");
        fs::rename(&out, &moved).expect("no handle is held under the output directory");
        fs::rename(&moved, &out).expect("the directory goes back");

        // The second leg: an arm file this test made is neither removed
        // nor refreshed, and the field is read off the disk.
        fs::write(s.arm(), b"").expect("the arm file lands");
        let armed = stamped(s.arm());
        let dirs_armed = touched(&s);
        past_the_clock(&dirs_armed);
        let report = status(s.output());
        assert_eq!(
            report.session.expect("the session reports").arm_file,
            ArmFile::Present,
            "the field is wired to the disk"
        );
        assert_eq!(stamped(s.arm()), armed, "and nothing here touched the file");
        assert_eq!(
            touched(&s),
            dirs_armed,
            "and the armed branch created and removed nothing either"
        );
    }
}
