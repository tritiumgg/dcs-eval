//! Collecting a reply, and deciding what a silent request means.
//!
//! A session is addressed once, off the handshake that was read, and every
//! later question is asked against that one stamp. A reply carrying any
//! other stamp is not this client's answer: the executor already refuses a
//! request whose `for` is not its own session, and this is the same fence
//! from the other side, kept because the consequence of the directory
//! layout failing is a chunk run in a session nobody addressed.
//!
//! The handshake does not keep the path it was read from, so the two files
//! a session is watched through — the handshake itself and the heartbeat —
//! are derived from its resolved `output`. That is where the executor
//! writes them and where a client discovers them, so there is no path
//! parameter here to go looking for.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crate::protocol::{self, Envelope, ParseError};
use crate::publish::{Sent, is_id};
use crate::readers::{Handshake, Heartbeat, ReadError, ReadErrorKind};
use crate::sys::{self, Changes};
use crate::watch::{self, Pace, Tally};

/// One executor session, as a client addressed it. The stamp is the whole
/// identity: the session directory and the two files are where that
/// handshake said they were, and a handshake carrying a different stamp
/// read later is a different session, not this one moved.
#[derive(Debug, Clone)]
pub struct Session {
    stamp: String,
    pid: u32,
    res: PathBuf,
    handshake: PathBuf,
    heartbeat: PathBuf,
}

impl Session {
    /// The session `h` describes.
    pub fn addressed(h: &Handshake) -> Self {
        let output = h.output.as_path();
        Self {
            stamp: h.stamp.clone(),
            pid: h.pid,
            res: h.res.as_path().to_owned(),
            handshake: output.join("executor.txt"),
            heartbeat: output.join("heartbeat.txt"),
        }
    }

    /// The stamp this session was addressed on.
    pub fn stamp(&self) -> &str {
        &self.stamp
    }

    /// The process the handshake named.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Where the reply to a request appears.
    pub fn res(&self) -> &Path {
        &self.res
    }

    /// The handshake this session was addressed off, re-read to see
    /// whether the stamp is still the one that was addressed.
    pub fn handshake(&self) -> &Path {
        &self.handshake
    }

    /// The heartbeat, where there is one. A session that has never armed
    /// has not written it yet.
    pub fn heartbeat(&self) -> &Path {
        &self.heartbeat
    }
}

/// What a look in the reply directory found.
#[derive(Debug)]
pub enum Collected {
    /// The reply, on the stamp addressed.
    Reply(Envelope),
    /// A reply stamped for a session other than the one addressed. Kept
    /// as a cheap assertion rather than folded into "nothing yet",
    /// because the consequence of the directory structure failing is a
    /// chunk run in the wrong session, and that is worth naming both
    /// spellings of.
    Foreign { saw: String, wanted: String },
    /// No reply under that id yet.
    Nothing,
}

/// Which reply would not be read, and why. `Display` is
/// `<path>: <reason>`, the shape every refusal in this crate takes.
#[derive(Debug)]
pub struct CollectError {
    pub path: PathBuf,
    pub kind: CollectErrorKind,
}

/// The reason half of a [`CollectError`].
#[derive(Debug)]
pub enum CollectErrorKind {
    /// Not an id, so not a name this side will put in a path. The path a
    /// refusal names is the directory the id never became a file in.
    Id { id: String },
    /// The reply could not be read.
    Disk(std::io::Error),
    /// The bytes are not an envelope.
    Envelope(ParseError),
    /// A reply with no `stamp` at all, which cannot be checked against
    /// the session addressed and so is not admitted on the strength of
    /// having none.
    NoStamp,
}

impl fmt::Display for CollectErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id { id } => write!(f, "the id {id} is not [0-9]{{10}}-[A-Za-z0-9]{{4,12}}"),
            Self::Disk(source) => write!(f, "{source}"),
            Self::Envelope(source) => write!(f, "{source}"),
            Self::NoStamp => write!(f, "stamp: absent"),
        }
    }
}

impl fmt::Display for CollectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.kind)
    }
}

impl std::error::Error for CollectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            CollectErrorKind::Disk(source) => Some(source),
            CollectErrorKind::Envelope(source) => Some(source),
            _ => None,
        }
    }
}

/// The reply to `id`, where one has landed.
///
/// The id is checked before it becomes a path, so nothing that is not
/// `<seq>-<tag>` reaches the disk at all. A reply still being written is
/// under `<id>.res.tmp` and is never looked at: only the exact final name
/// is read, which is the other half of the promise the publisher makes.
pub fn collect(s: &Session, id: &str) -> Result<Collected, CollectError> {
    if !is_id(id) {
        return Err(CollectError {
            path: s.res.clone(),
            kind: CollectErrorKind::Id { id: id.to_owned() },
        });
    }
    let path = s.res.join(format!("{id}.res"));
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(why) if why.kind() == std::io::ErrorKind::NotFound => return Ok(Collected::Nothing),
        Err(why) => {
            return Err(CollectError {
                path,
                kind: CollectErrorKind::Disk(why),
            });
        }
    };
    let envelope = protocol::parse(&bytes).map_err(|source| CollectError {
        path: path.clone(),
        kind: CollectErrorKind::Envelope(source),
    })?;
    let Some(saw) = envelope.headers.get("stamp") else {
        return Err(CollectError {
            path,
            kind: CollectErrorKind::NoStamp,
        });
    };
    if saw != s.stamp {
        return Ok(Collected::Foreign {
            saw: saw.to_owned(),
            wanted: s.stamp.clone(),
        });
    }
    Ok(Collected::Reply(envelope))
}

/// The one threshold both age rules are measured against: how long an
/// armed session may go without writing a heartbeat, and how long a
/// dormant one has to notice the arm file. One constant rather than two
/// because it is one judgement — it covers the menu stall the
/// measurements found, and `probe_every` frames at any frame rate above
/// one per second sits inside it.
pub const WAKE_DEADLINE: Duration = Duration::from_secs(10);

/// The phase carried where nothing on the disk says what it is: a session
/// that has never armed has written no heartbeat, and a heartbeat from
/// another session is not evidence about this one.
///
/// Public because a `pending` the server answers without having waited —
/// a collect that found nothing — must name the same word this module
/// would have named, and a second `"unknown"` spelt in the server is a
/// second place for the two to drift apart.
pub const PHASE_UNKNOWN: &str = "unknown";

/// The phase a mission load takes, during which nothing fires at all.
pub(crate) const PHASE_LOAD: &str = "load";

/// What a `pending` is waiting on, where there is something to say about
/// it. No flag is the ordinary case: alive, ticking and answering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    /// The session was dormant when the request was published and has not
    /// yet had time to notice the arm file.
    Waking,
    /// The session did not wake, or stopped ticking, and the process is
    /// still there. The client says so and keeps waiting.
    Stalled,
}

/// What one look at a session says about one request.
///
/// `Superseded` and `Dead` are terminal: the request lies in a directory
/// nothing will ever list again, and the id comes back so a caller can log
/// it. A `Pending` is collectable later and carries the phase.
#[derive(Debug)]
pub enum Outcome {
    Reply(Envelope),
    Pending {
        id: String,
        phase: String,
        flag: Option<Flag>,
    },
    /// The handshake names a stamp other than the one addressed: DCS
    /// restarted, and the request did not and will not run.
    Superseded {
        id: String,
    },
    /// The process the handshake named is gone, and no new session has
    /// started.
    Dead {
        id: String,
    },
}

impl Outcome {
    /// Whether nothing will ever change this answer.
    pub fn terminal(&self) -> bool {
        matches!(self, Self::Superseded { .. } | Self::Dead { .. })
    }

    /// The request this is about. A reply's own id is the one it carries,
    /// which a reply with no `id` header does not have.
    pub fn id(&self) -> &str {
        match self {
            Self::Reply(envelope) => envelope.headers.get("id").unwrap_or_default(),
            Self::Pending { id, .. } | Self::Superseded { id } | Self::Dead { id } => id,
        }
    }
}

/// Which file would not be read, and why. One type over both halves, so a
/// caller has one error to print and `wait` one to return.
#[derive(Debug)]
pub struct WaitError {
    pub path: PathBuf,
    pub kind: WaitErrorKind,
}

/// The reason half of a [`WaitError`].
#[derive(Debug)]
pub enum WaitErrorKind {
    /// A reply that would not read, in `collect`'s own words.
    Collect(CollectErrorKind),
    /// The handshake, or a heartbeat that is there and will not parse, in
    /// the reader's own words.
    Read(ReadErrorKind),
}

impl fmt::Display for WaitErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Collect(kind) => write!(f, "{kind}"),
            Self::Read(kind) => write!(f, "{kind}"),
        }
    }
}

impl fmt::Display for WaitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.kind)
    }
}

impl std::error::Error for WaitError {}

impl From<CollectError> for WaitError {
    fn from(err: CollectError) -> Self {
        Self {
            path: err.path,
            kind: WaitErrorKind::Collect(err.kind),
        }
    }
}

impl From<ReadError> for WaitError {
    fn from(err: ReadError) -> Self {
        Self {
            path: err.path,
            kind: WaitErrorKind::Read(err.kind),
        }
    }
}

/// What a silent request means, from two small file reads and, where they
/// leave it open, one process probe.
///
/// The cost was chosen rather than overlooked. Two reads per wake is what
/// the table asks for, and the probe fires on every wake while the session
/// is dormant — some hundreds of times over a long wait — because while
/// dormant the process id is the only liveness evidence there is: a
/// dormant session stops writing its heartbeat by design, so the age of
/// the one it left says when it fell silent, not whether it still lives.
/// An open, a zero-millisecond wait and a close cost the game nothing.
///
/// The arms are an ordered list and not a set, because two of the
/// readings hold at once often enough to matter. A stamp that changed
/// beats any heartbeat, since one left behind by a session that is gone is
/// not evidence about the live one. `armed` is read before the age, which
/// is the rule a dormant session's silence depends on. Four readings go
/// past the literal table — a dormant session whose process is gone, a
/// probe that could not decide, a heartbeat that is absent or belongs to
/// another session, and a request this process did not send — and decision
/// record 0012 argues all four.
///
/// `now` and `now_sys` are the caller's, so the two ages are taken once
/// against one clock each rather than re-read part way down the list.
pub fn decide(
    s: &Session,
    sent: &Sent,
    now: Instant,
    now_sys: SystemTime,
) -> Result<Outcome, WaitError> {
    let handshake = Handshake::read(&s.handshake)?;
    if handshake.stamp != s.stamp {
        return Ok(Outcome::Superseded {
            id: sent.id().to_owned(),
        });
    }
    let beat = this_sessions_heartbeat(s)?;
    let phase = beat
        .as_ref()
        .map_or(PHASE_UNKNOWN, |b| b.phase.as_str())
        .to_owned();
    let pending = |flag| Outcome::Pending {
        id: sent.id().to_owned(),
        phase: phase.clone(),
        flag,
    };
    // Only a probe that positively says the process is gone may produce a
    // terminal answer; one that could not decide falls in with a process
    // that is running, because `dead` says the request will never run and
    // a refused handle does not say that.
    let gone = || matches!(sys::liveness(s.pid), sys::Liveness::Exited);
    let loading = phase == PHASE_LOAD;

    match beat {
        // Armed: the age is evidence, and while it is fresh the process is
        // not probed at all — the table checks it only when the files are
        // stale.
        Some(b) if b.armed => {
            if b.age(now_sys) <= WAKE_DEADLINE {
                Ok(pending(None))
            } else if gone() {
                Ok(Outcome::Dead {
                    id: sent.id().to_owned(),
                })
            } else if loading {
                Ok(pending(None))
            } else {
                Ok(pending(Some(Flag::Stalled)))
            }
        }
        // Dormant, or nothing of this session's to read. The heartbeat's
        // age is never consulted on this branch.
        _ => {
            if gone() {
                Ok(Outcome::Dead {
                    id: sent.id().to_owned(),
                })
            } else if sent.elapsed(now).is_some_and(|since| since < WAKE_DEADLINE) {
                Ok(pending(Some(Flag::Waking)))
            } else if loading {
                Ok(pending(None))
            } else {
                Ok(pending(Some(Flag::Stalled)))
            }
        }
    }
}

/// The heartbeat, where this session has one to read.
///
/// A heartbeat that is not there is not a refusal: a session that has
/// never armed has not written one, and that is the expected state right
/// after a load. Neither is one carrying another session's stamp, which is
/// two installs writing into one output directory — a real problem, and
/// one `status` reports, but not evidence about this session. Both read as
/// the dormant branch, which consults no age anyway. A heartbeat that is
/// there and will not parse is a refusal naming the file, because a stamp
/// that changed is not the same thing as a file that would not read.
///
/// The file is read before the stamp is looked at, and that order decides
/// the case where both hold: a heartbeat that carries another session's
/// stamp *and* will not read is a refusal, not the dormant branch. It has
/// to be. A stamp is something only a file this reader understood has, so
/// forgiving a foreign stamp on a file it did not understand would be
/// forgiving it on the strength of a header read out of bytes whose shape
/// was never established.
fn this_sessions_heartbeat(s: &Session) -> Result<Option<Heartbeat>, WaitError> {
    match Heartbeat::read(&s.heartbeat) {
        Ok(beat) if beat.stamp == s.stamp => Ok(Some(beat)),
        // Read, understood, and somebody else's: two installs writing into
        // one output directory, which says nothing about this session.
        Ok(_) => Ok(None),
        Err(ReadError {
            kind: ReadErrorKind::Disk(why),
            ..
        }) if why.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

/// The instant `upto` from `now`, or the furthest one this clock can name
/// where `upto` reaches past the end of it.
///
/// Saturating rather than refusing, because of what a deadline is for. A
/// caller passing a duration bigger than the clock can hold is saying it
/// will wait as long as it takes, and the longest wait this clock can
/// express is the nearest thing to that there is; turning that into an
/// error would make the one call in this module that never fails on time
/// fail on asking for too much of it. Halving finds the furthest
/// representable instant in a few dozen steps, and ends at `now` — a wait
/// of no time, which is still an outcome — if even zero would not add.
fn latest(now: Instant, upto: Duration) -> Instant {
    let mut step = upto;
    loop {
        if let Some(at) = now.checked_add(step) {
            return at;
        }
        step /= 2;
    }
}

/// The answer to one request, waited for.
///
/// A reply returns at once. A reply carrying another session's stamp is
/// discarded and the loop goes on to read the table: there is nowhere in
/// an outcome to put it and nothing that would read it. A terminal
/// outcome returns at once, since nothing will change it. Otherwise the
/// wait goes round until `upto` is spent and the last `pending` comes
/// back — running out of time is never a failure, and the table is read at
/// least once however little time there was.
pub fn wait(s: &Session, sent: &Sent, upto: Duration) -> Result<Outcome, WaitError> {
    wait_paced(s, sent, upto, &Pace::default(), &mut Tally::default())
}

/// The wait, with the pacing named and what it did counted.
///
/// The watch lives in this function's own local and nowhere else. That is
/// what makes "no handle is held once `wait` returns" a property of where
/// the value sits rather than a rule somebody has to remember: there is no
/// public constructor for one, no way to hand one in, and every path out
/// of here — a reply, a deadline, a terminal outcome, a `?` on a file that
/// would not read, a panic — drops the local, and dropping it cancels the
/// pending read and waits for the cancellation before the buffer goes.
pub(crate) fn wait_paced(
    s: &Session,
    sent: &Sent,
    upto: Duration,
    pace: &Pace,
    tally: &mut Tally,
) -> Result<Outcome, WaitError> {
    let deadline = latest(Instant::now(), upto);
    let mut changes: Option<Changes> = None;
    loop {
        let now = Instant::now();
        // Before the look, so that a change landing afterwards signals
        // rather than being missed in the gap between the two. On the
        // first pass there is nothing open yet and the look is the whole
        // of it, which is the first look this wait always did.
        watch::arm_if_needed(&mut changes, tally);
        if let Collected::Reply(envelope) = collect(s, sent.id())? {
            return Ok(Outcome::Reply(envelope));
        }
        let outcome = decide(s, sent, now, SystemTime::now())?;
        if outcome.terminal() {
            return Ok(outcome);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(outcome);
        }
        watch::settle(&mut changes, s.res(), left, pace, tally);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::standin::Standin;
    use crate::testing::{Sandbox, a_pid_that_has_exited, entries, slurp};

    const ID: &str = "0000000001-abcd";

    /// A stand-in session with its handshake published, and the session a
    /// client reads off it.
    fn addressed(b: &Sandbox) -> (Standin, Session) {
        let s = standin(b);
        let session = address(&s);
        (s, session)
    }

    /// A stand-in the caller sets up before its handshake is published,
    /// which is where the pid a client will probe is fixed.
    fn standin(b: &Sandbox) -> Standin {
        Standin::open(&b.join("dcs"), "hook").expect("the stand-in opens")
    }

    /// The session a client addresses, off whatever the stand-in has just
    /// been set to.
    fn address(s: &Standin) -> Session {
        s.handshake().expect("the handshake publishes");
        read_session(s)
    }

    fn read_session(s: &Standin) -> Session {
        let h = Handshake::read(&s.output().join("executor.txt")).expect("the handshake reads");
        Session::addressed(&h)
    }

    #[test]
    fn collect_an_id_that_is_not_one_never_becomes_a_path() {
        let b = Sandbox::new();
        let (s, session) = addressed(&b);
        for id in ["", "../x", "0000000001-a/bc", "1-a"] {
            let err = collect(&session, id).expect_err(id);
            assert!(
                matches!(err.kind, CollectErrorKind::Id { .. }),
                "{id}: {err}"
            );
            assert_eq!(
                err.path, session.res,
                "the refusal names the directory, which the id never became a file in"
            );
            assert!(err.to_string().contains("is not [0-9]"), "{err}");
        }
        assert_eq!(entries(s.res()), "", "and nothing was made looking");
    }

    #[test]
    fn collect_with_no_reply_yet_is_nothing() {
        let b = Sandbox::new();
        let (_s, session) = addressed(&b);
        let got = collect(&session, ID).expect("an empty directory is not a refusal");
        assert!(matches!(got, Collected::Nothing), "{got:?}");
    }

    #[test]
    fn collect_does_not_read_a_reply_still_being_written() {
        let b = Sandbox::new();
        let (s, session) = addressed(&b);
        let tmp = s.res().join(format!("{ID}.res.tmp"));
        std::fs::write(&tmp, b"status: ok\r\n\r\n").expect("a half-written reply");
        let got = collect(&session, ID).expect("the .tmp is not a refusal either");
        assert!(
            matches!(got, Collected::Nothing),
            "the .tmp is not read: {got:?}"
        );
        std::fs::rename(&tmp, s.res().join(format!("{ID}.res"))).expect("the rename lands it");
        let got = collect(&session, ID).expect_err("that one carries no stamp");
        assert!(matches!(got.kind, CollectErrorKind::NoStamp), "{got}");
    }

    #[test]
    fn collect_returns_the_reply_on_the_stamp_addressed() {
        let b = Sandbox::new();
        let (s, session) = addressed(&b);
        s.reply(ID, "ok", &[("op", "ping")], b"pong")
            .expect("the reply publishes");
        let got = collect(&session, ID).expect("the reply reads");
        let Collected::Reply(envelope) = got else {
            panic!("the reply, not {got:?}");
        };
        assert_eq!(envelope.headers.get("status"), Some("ok"));
        assert_eq!(envelope.headers.get("id"), Some(ID));
        assert_eq!(envelope.headers.get("stamp"), Some(s.stamp.as_str()));
        assert_eq!(envelope.body, b"pong");
    }

    #[test]
    fn collect_discards_a_reply_carrying_another_sessions_stamp() {
        let b = Sandbox::new();
        let (mut s, session) = addressed(&b);
        let wanted = s.stamp.clone();
        s.stamp = format!("{wanted}-restarted");
        s.reply(ID, "ok", &[], b"not yours")
            .expect("the foreign reply publishes");
        let got = collect(&session, ID).expect("a foreign stamp is a verdict, not a refusal");
        let Collected::Foreign { saw, wanted: want } = got else {
            panic!("foreign, not {got:?}");
        };
        assert_eq!(saw, s.stamp, "what the reply said");
        assert_eq!(want, wanted, "and what was addressed");
        assert_eq!(
            entries(s.res()),
            format!("{ID}.res"),
            "the file is left where it is: this library removes nothing it did not write"
        );
        assert!(
            slurp(&s.res().join(format!("{ID}.res"))).ends_with(b"not yours"),
            "and is left as it was"
        );
    }

    #[test]
    fn collect_names_the_file_when_the_reply_is_not_an_envelope() {
        let b = Sandbox::new();
        let (_s, session) = addressed(&b);
        let path = session.res.join(format!("{ID}.res"));
        std::fs::write(&path, b"this is not an envelope").expect("the bytes land");
        let err = collect(&session, ID).expect_err("not an envelope");
        assert!(matches!(err.kind, CollectErrorKind::Envelope(_)), "{err}");
        assert_eq!(err.path, path);
        assert!(
            err.to_string()
                .starts_with(&format!("{}: ", path.display())),
            "{err}"
        );
    }

    #[test]
    fn collect_refuses_a_reply_with_no_stamp_at_all() {
        // A reply with no stamp cannot be checked against the session
        // addressed, and is not admitted on the strength of having none.
        let b = Sandbox::new();
        let (_s, session) = addressed(&b);
        let path = session.res.join(format!("{ID}.res"));
        std::fs::write(&path, b"status: ok\r\nid: 0000000001-abcd\r\n\r\n")
            .expect("the bytes land");
        let err = collect(&session, ID).expect_err("no stamp to check");
        assert!(matches!(err.kind, CollectErrorKind::NoStamp), "{err}");
        assert_eq!(err.path, path);
        assert!(err.to_string().ends_with(": stamp: absent"), "{err}");
    }

    // ---- the outcome table ------------------------------------------------

    const HOUR: Duration = Duration::from_secs(3600);

    /// An instant that far back. It panics rather than saturating: a host
    /// up for less than the backdating would quietly turn an old send into
    /// a fresh one, and a test must not pass on a fixture it did not get.
    fn ago(d: Duration) -> Instant {
        Instant::now()
            .checked_sub(d)
            .expect("this host has been up longer than the test backdates")
    }

    /// A send this process made a moment ago, and one it made earlier.
    fn just_sent() -> Sent {
        Sent::at(ID, Instant::now())
    }
    fn sent_ago(d: Duration) -> Sent {
        Sent::at(ID, ago(d))
    }

    fn verdict(session: &Session, sent: &Sent) -> Outcome {
        decide(session, sent, Instant::now(), SystemTime::now()).expect("the table reads")
    }

    /// The `Pending` this outcome is, or a panic naming what came instead.
    fn pending_of(got: &Outcome) -> (&str, Option<Flag>) {
        let Outcome::Pending { phase, flag, .. } = got else {
            panic!("a pending, not {got:?}");
        };
        (phase.as_str(), *flag)
    }

    #[test]
    fn a_stamp_that_changed_is_superseded_whatever_the_heartbeat_says() {
        // The sharp form: the heartbeat left behind is this session's own,
        // armed and an hour old, and the pid is one that has really gone.
        // Read in any other order that is `dead`; the stamp is tested
        // first because a heartbeat from a session that is over is not
        // evidence about the one that replaced it.
        let b = Sandbox::new();
        let mut s = standin(&b);
        let (_child, pid) = a_pid_that_has_exited();
        s.pid = pid;
        s.armed = true;
        let session = address(&s);
        s.beat(SystemTime::now() - HOUR).expect("the old beat");
        s.stamp = format!("{}-restarted", s.stamp);
        s.handshake()
            .expect("the new session's handshake lands over it");
        let got = verdict(&session, &just_sent());
        assert!(matches!(got, Outcome::Superseded { .. }), "{got:?}");
        assert!(got.terminal(), "nothing will change this answer");
    }

    #[test]
    fn an_armed_heartbeat_under_ten_seconds_is_pending() {
        let b = Sandbox::new();
        let mut s = standin(&b);
        s.armed = true;
        let session = address(&s);
        s.beat(SystemTime::now()).expect("a fresh beat");
        let got = verdict(&session, &just_sent());
        assert_eq!(pending_of(&got), ("menu", None), "{got:?}");
        assert!(!got.terminal());
    }

    #[test]
    fn an_armed_heartbeat_under_ten_seconds_is_pending_even_with_a_pid_that_is_gone() {
        // The table checks the process only when the files are stale, so a
        // probe consulted here would answer `dead` and this discriminates.
        let b = Sandbox::new();
        let mut s = standin(&b);
        let (_child, pid) = a_pid_that_has_exited();
        s.pid = pid;
        s.armed = true;
        let session = address(&s);
        s.beat(SystemTime::now()).expect("a fresh beat");
        let got = verdict(&session, &just_sent());
        assert_eq!(pending_of(&got), ("menu", None), "{got:?}");
    }

    #[test]
    fn a_dormant_heartbeat_an_hour_old_is_pending_waking_while_the_send_is_young() {
        // `armed` is read before the age: a dormant session's heartbeat is
        // as old as its last transition, and that says when it fell
        // silent, not whether it lives.
        let b = Sandbox::new();
        let mut s = standin(&b);
        s.armed = false;
        s.phase = "menu".to_owned();
        s.pid = std::process::id();
        let session = address(&s);
        s.beat(SystemTime::now() - HOUR).expect("an hour-old beat");
        let got = verdict(&session, &just_sent());
        assert_eq!(pending_of(&got), ("menu", Some(Flag::Waking)), "{got:?}");
    }

    #[test]
    fn a_dormant_session_in_a_mission_load_is_pending_at_any_age() {
        let b = Sandbox::new();
        let mut s = standin(&b);
        s.armed = false;
        s.phase = "load".to_owned();
        s.pid = std::process::id();
        let session = address(&s);
        let sent = sent_ago(Duration::from_secs(11));
        for at in [SystemTime::now(), SystemTime::now() - HOUR] {
            s.beat(at).expect("the beat");
            let got = verdict(&session, &sent);
            assert_eq!(pending_of(&got), ("load", None), "{got:?}");
        }
    }

    #[test]
    fn a_dormant_session_that_did_not_wake_in_ten_seconds_is_stalled() {
        let b = Sandbox::new();
        let mut s = standin(&b);
        s.armed = false;
        s.phase = "menu".to_owned();
        s.pid = std::process::id();
        let session = address(&s);
        s.beat(SystemTime::now()).expect("the beat");
        let got = verdict(&session, &sent_ago(Duration::from_secs(11)));
        assert_eq!(pending_of(&got), ("menu", Some(Flag::Stalled)), "{got:?}");
    }

    #[test]
    fn a_pid_that_is_gone_is_dead_with_an_armed_heartbeat_gone_stale() {
        let b = Sandbox::new();
        let mut s = standin(&b);
        let (_child, pid) = a_pid_that_has_exited();
        s.pid = pid;
        s.armed = true;
        let session = address(&s);
        s.beat(SystemTime::now() - HOUR).expect("a stale beat");
        let got = verdict(&session, &just_sent());
        assert!(matches!(got, Outcome::Dead { .. }), "{got:?}");
        assert!(got.terminal());
    }

    #[test]
    fn a_pid_that_is_gone_is_dead_while_dormant_too() {
        // The row the table does not literally hold: under `armed: no`
        // there is no age to compare, and the beat here is three seconds
        // old, not ten. Liveness while dormant is the pid and nothing
        // else, so saying `waking` about a process that no longer exists
        // would be the one wrong answer. Decision record 0012.
        let b = Sandbox::new();
        let mut s = standin(&b);
        let (_child, pid) = a_pid_that_has_exited();
        s.pid = pid;
        s.armed = false;
        let session = address(&s);
        s.beat(SystemTime::now() - Duration::from_secs(3))
            .expect("a beat three seconds old");
        let got = verdict(&session, &just_sent());
        assert!(matches!(got, Outcome::Dead { .. }), "{got:?}");
    }

    #[test]
    fn an_armed_heartbeat_over_ten_seconds_in_a_mission_load_is_pending() {
        let b = Sandbox::new();
        let mut s = standin(&b);
        s.armed = true;
        s.phase = "load".to_owned();
        s.pid = std::process::id();
        let session = address(&s);
        s.beat(SystemTime::now() - HOUR).expect("a stale beat");
        let got = verdict(&session, &just_sent());
        assert_eq!(pending_of(&got), ("load", None), "{got:?}");
    }

    #[test]
    fn an_armed_heartbeat_over_ten_seconds_is_pending_stalled() {
        let b = Sandbox::new();
        let mut s = standin(&b);
        s.armed = true;
        s.phase = "running".to_owned();
        s.pid = std::process::id();
        let session = address(&s);
        s.beat(SystemTime::now() - HOUR).expect("a stale beat");
        let got = verdict(&session, &just_sent());
        assert_eq!(
            pending_of(&got),
            ("running", Some(Flag::Stalled)),
            "{got:?}"
        );
    }

    #[test]
    fn a_pid_the_probe_cannot_decide_is_never_dead() {
        // Pid 4 is the System process: unelevated the handle is refused,
        // elevated it opens and reads as running. Either way the answer is
        // the same pending, and what must never happen is `dead`.
        let b = Sandbox::new();
        let mut s = standin(&b);
        s.pid = 4;
        s.armed = true;
        s.phase = "menu".to_owned();
        let session = address(&s);
        s.beat(SystemTime::now() - HOUR).expect("a stale beat");
        let got = verdict(&session, &just_sent());
        assert!(!matches!(got, Outcome::Dead { .. }), "{got:?}");
        assert_eq!(pending_of(&got), ("menu", Some(Flag::Stalled)), "{got:?}");
    }

    #[test]
    fn a_session_that_has_never_armed_reads_as_dormant() {
        // The state right after a load: the handshake is there and no
        // heartbeat has been written at all. That is not a refusal, and
        // the dormant branch consults no age anyway.
        let b = Sandbox::new();
        let mut s = standin(&b);
        s.pid = std::process::id();
        let session = address(&s);
        assert!(!session.heartbeat().exists(), "nothing has armed yet");
        let got = verdict(&session, &just_sent());
        assert_eq!(
            pending_of(&got),
            ("unknown", Some(Flag::Waking)),
            "nothing on the disk says what the phase is: {got:?}"
        );
    }

    #[test]
    fn a_heartbeat_from_another_stamp_is_not_this_sessions_evidence() {
        // Two installs writing into one output directory. Counted as this
        // session's it is armed and fresh, which would be a pending with
        // no flag; it is not this session's, so the dormant branch decides
        // and the phase is not read off it either.
        let b = Sandbox::new();
        let mut s = standin(&b);
        s.pid = std::process::id();
        s.armed = true;
        s.phase = "menu".to_owned();
        let mine = s.stamp.clone();
        s.stamp = format!("{mine}-somebody-else");
        s.beat(SystemTime::now()).expect("the other install's beat");
        s.stamp = mine;
        let session = address(&s);
        let got = verdict(&session, &sent_ago(Duration::from_secs(11)));
        assert_eq!(
            pending_of(&got),
            ("unknown", Some(Flag::Stalled)),
            "{got:?}"
        );
    }

    #[test]
    fn a_foreign_stamped_heartbeat_that_will_not_read_is_a_refusal() {
        // Where the two readings meet, the file is read before the stamp
        // is looked at, and that is what decides it: foreign and readable
        // is the dormant branch, foreign and unreadable is a refusal
        // naming the file. The stamp that would have excused it is only
        // as good as the read that produced it.
        let b = Sandbox::new();
        let mut s = standin(&b);
        s.pid = std::process::id();
        s.armed = true;
        s.phase = "menu".to_owned();
        let mine = s.stamp.clone();
        s.stamp = format!("{mine}-somebody-else");
        s.beat(SystemTime::now()).expect("the other install's beat");
        s.stamp = mine;
        let session = address(&s);
        let sent = sent_ago(Duration::from_secs(11));

        let got = verdict(&session, &sent);
        assert_eq!(
            pending_of(&got),
            ("unknown", Some(Flag::Stalled)),
            "one that read is the dormant branch: {got:?}"
        );

        // The same file, still the other install's, with the one header
        // every outcome turns on taken out of it.
        let beat = String::from_utf8(slurp(session.heartbeat())).expect("the beat is text");
        let mangled: String = beat
            .split_inclusive('\n')
            .filter(|line| !line.starts_with("armed:"))
            .collect();
        assert!(
            mangled.contains("-somebody-else") && !mangled.contains("armed:"),
            "still foreign, and now short a required header"
        );
        std::fs::write(session.heartbeat(), mangled.as_bytes()).expect("the bytes land");
        let err = decide(&session, &sent, Instant::now(), SystemTime::now())
            .expect_err("the foreign stamp does not excuse a file that would not read");
        assert!(matches!(err.kind, WaitErrorKind::Read(_)), "{err}");
        assert_eq!(err.path, session.heartbeat());
        assert!(err.to_string().ends_with(": armed: absent"), "{err}");
    }

    #[test]
    fn a_sent_this_process_did_not_mint_is_never_waking() {
        // `waking` says the arm file was ensured a moment ago. A caller
        // collecting an id it was handed cannot claim that.
        let b = Sandbox::new();
        let mut s = standin(&b);
        s.pid = std::process::id();
        s.armed = false;
        s.phase = "menu".to_owned();
        let session = address(&s);
        s.beat(SystemTime::now()).expect("the beat");
        let got = verdict(&session, &Sent::earlier(ID));
        assert_eq!(pending_of(&got), ("menu", Some(Flag::Stalled)), "{got:?}");
    }

    #[test]
    fn the_id_comes_back_on_every_terminal_outcome() {
        let b = Sandbox::new();
        let mut s = standin(&b);
        let (_child, pid) = a_pid_that_has_exited();
        s.pid = pid;
        s.armed = true;
        let session = address(&s);
        s.beat(SystemTime::now() - HOUR).expect("a stale beat");
        let dead = verdict(&session, &just_sent());
        assert!(matches!(dead, Outcome::Dead { .. }), "{dead:?}");
        assert_eq!(dead.id(), ID, "the id a caller logs");

        s.stamp = format!("{}-restarted", s.stamp);
        s.handshake().expect("the new session's handshake");
        let superseded = verdict(&session, &just_sent());
        assert!(
            matches!(superseded, Outcome::Superseded { .. }),
            "{superseded:?}"
        );
        assert_eq!(superseded.id(), ID);
    }

    // ---- the wait, over the poll ------------------------------------------

    /// A session that is alive and ticking, so the table says `pending`
    /// and the wait is about the reply directory and nothing else.
    fn ticking(b: &Sandbox) -> (Standin, Session) {
        let mut s = standin(b);
        s.pid = std::process::id();
        s.armed = true;
        let session = address(&s);
        s.beat(SystemTime::now()).expect("a fresh beat");
        (s, session)
    }

    #[test]
    fn wait_returns_the_reply_as_soon_as_it_lands() {
        let b = Sandbox::new();
        let (s, session) = ticking(&b);
        let sent = just_sent();
        let got = std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(60));
                s.reply(ID, "ok", &[("op", "ping")], b"pong")
                    .expect("the reply publishes");
            });
            wait(&session, &sent, Duration::from_secs(5)).expect("the wait reads")
        });
        let Outcome::Reply(envelope) = got else {
            panic!("the reply, not {got:?}");
        };
        assert_eq!(envelope.body, b"pong", "landed after more than one poll");
        assert_eq!(envelope.headers.get("status"), Some("ok"));
    }

    #[test]
    fn wait_hands_back_pending_when_its_time_is_up_and_that_is_not_a_failure() {
        let b = Sandbox::new();
        let (_s, session) = ticking(&b);
        let got = wait(&session, &just_sent(), Duration::from_millis(80))
            .expect("running out of time is not an error");
        assert_eq!(pending_of(&got), ("menu", None), "{got:?}");
        assert!(!got.terminal(), "and it is collectable later");
    }

    #[test]
    fn wait_stops_at_once_on_a_terminal_outcome() {
        // An upper bound only. A lower one would be asserting that the
        // machine is slow.
        let b = Sandbox::new();
        let (mut s, session) = ticking(&b);
        s.stamp = format!("{}-restarted", s.stamp);
        s.handshake().expect("the new session's handshake");
        let began = Instant::now();
        let got = wait(&session, &just_sent(), Duration::from_secs(5)).expect("the wait reads");
        assert!(matches!(got, Outcome::Superseded { .. }), "{got:?}");
        assert!(
            began.elapsed() < Duration::from_secs(1),
            "it did not wait its five seconds out: {:?}",
            began.elapsed()
        );
    }

    #[test]
    fn wait_keeps_looking_after_it_discards_a_foreign_reply() {
        let b = Sandbox::new();
        let (mut s, session) = ticking(&b);
        let mine = s.stamp.clone();
        s.stamp = format!("{mine}-restarted");
        s.reply(ID, "ok", &[], b"not yours")
            .expect("the foreign reply publishes");
        s.stamp = mine;
        let got = wait(&session, &just_sent(), Duration::from_millis(80))
            .expect("a foreign reply is not a refusal");
        assert_eq!(
            pending_of(&got),
            ("menu", None),
            "it went on to the table rather than handing the reply back: {got:?}"
        );
        assert_eq!(
            entries(session.res()),
            format!("{ID}.res"),
            "and left the file where it was"
        );
    }

    #[test]
    fn a_deadline_past_the_end_of_the_clock_is_the_furthest_one_it_can_name() {
        // Plain addition panics here. A caller asking to wait longer than
        // the clock can count is asking to wait as long as it takes, and
        // the answer is the longest wait there is rather than an error.
        let now = Instant::now();
        let far = latest(now, Duration::MAX);
        assert!(
            far.duration_since(now) > Duration::from_secs(365 * 24 * 3600),
            "it saturated far out, not back to now: {:?}",
            far.duration_since(now)
        );
        assert_eq!(
            latest(now, Duration::ZERO),
            now,
            "and a deadline that fits is just itself"
        );

        // And the whole wait survives one, which is the panic the caller
        // would otherwise have met. A terminal outcome so the wait does
        // not sit there for the rest of the clock, and a thread with a
        // bound on it so that a wait which never sees that outcome fails
        // this test rather than hanging the suite: an unbounded wait on a
        // session that stays pending is the design, not a fault, so nothing
        // inside it will ever give up.
        let b = Sandbox::new();
        let (mut s, session) = ticking(&b);
        s.stamp = format!("{}-restarted", s.stamp);
        s.handshake().expect("the new session's handshake");
        let (tx, rx) = std::sync::mpsc::channel();
        let sent = just_sent();
        std::thread::spawn(move || {
            let _ = tx.send(wait(&session, &sent, Duration::MAX));
        });
        let got = rx
            .recv_timeout(Duration::from_secs(30))
            .expect("a changed stamp ends an unbounded wait at once, not never")
            .expect("the wait reads");
        assert!(matches!(got, Outcome::Superseded { .. }), "{got:?}");
    }

    #[test]
    fn wait_names_the_file_when_the_heartbeat_will_not_parse() {
        let b = Sandbox::new();
        let (_s, session) = ticking(&b);
        std::fs::write(session.heartbeat(), b"this is not an envelope").expect("the bytes land");
        let err = wait(&session, &just_sent(), Duration::from_secs(5))
            .expect_err("a heartbeat that is there and will not read is a refusal");
        assert!(matches!(err.kind, WaitErrorKind::Read(_)), "{err}");
        assert_eq!(err.path, session.heartbeat());
        assert!(
            err.to_string()
                .starts_with(&format!("{}: ", session.heartbeat().display())),
            "{err}"
        );
    }
}
