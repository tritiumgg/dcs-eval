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

use crate::protocol::{self, Envelope, ParseError};
use crate::publish::is_id;
use crate::readers::Handshake;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::standin::Standin;
    use crate::testing::{Sandbox, entries, slurp};

    const ID: &str = "0000000001-abcd";

    /// A stand-in session with its handshake published, and the session a
    /// client reads off it.
    fn addressed(b: &Sandbox) -> (Standin, Session) {
        let s = Standin::open(&b.join("dcs"), "hook").expect("the stand-in opens");
        s.handshake().expect("the handshake publishes");
        let session = read_session(&s);
        (s, session)
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
}
