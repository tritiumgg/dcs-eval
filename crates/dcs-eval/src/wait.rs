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
use crate::sys;

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
const PHASE_UNKNOWN: &str = "unknown";

/// The phase a mission load takes, during which nothing fires at all.
const PHASE_LOAD: &str = "load";

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
fn this_sessions_heartbeat(s: &Session) -> Result<Option<Heartbeat>, WaitError> {
    match Heartbeat::read(&s.heartbeat) {
        Ok(beat) if beat.stamp == s.stamp => Ok(Some(beat)),
        Ok(_) => Ok(None),
        Err(ReadError {
            kind: ReadErrorKind::Disk(why),
            ..
        }) if why.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
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
}
