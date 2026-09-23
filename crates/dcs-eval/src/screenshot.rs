//! The capture: one request that asks DCS for a screenshot and answers with
//! the directory it writes into, then the file that lands there, watched
//! from outside DCS inside the same wait.
//!
//! The call DCS offers returns nothing and writes its file some frames
//! later, so the reply says only where to look. What makes a file this
//! capture's rather than an older one under the same name is its modified
//! time, measured against a clock reading taken before anything was
//! published; what makes it finished is `shot_file`'s judgement of its
//! bytes. Nothing here polls the executor: the request goes out once and
//! the rest is a directory read every [`LOOK_EVERY`].

use std::fmt;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use crate::pipeline::{PipeError, Pipeline, Spec};
use crate::protocol::Envelope;
use crate::readers::Handshake;
use crate::shot_file::{self, Finding, Picture};
use crate::shot_name::{self, NameRefusal};
use crate::sys;
use crate::wait::{self, Flag, Outcome};

/// How often the directory is read while the file is awaited: often enough
/// that a capture is answered soon after DCS finishes it, and a listing of
/// one directory is nothing to the game.
pub const LOOK_EVERY: Duration = Duration::from_millis(100);

/// What one capture came to.
///
/// `Pending` and `NotWritten` are not refusals. The first is a reply that
/// did not come inside the wait, collectable later like any other; the
/// second is a reply that came while the file did not, and the file may
/// still land.
#[derive(Debug)]
pub enum Capture {
    /// A whole file under the name, modified at or after the request.
    Ok { path: PathBuf, picture: Picture },
    /// The executor did not answer inside the wait. The directory is where
    /// the executor's own layout puts it, derived from the handshake, and
    /// `None` where that layout cannot be read off; decision record 0039.
    Pending {
        id: String,
        phase: String,
        flag: Option<Flag>,
        dir: Option<PathBuf>,
        name: String,
    },
    /// The executor answered and no whole file newer than the request
    /// arrived before the wait ran out.
    NotWritten { dir: PathBuf, name: String },
    /// A file arrived under the name and was still zero bytes when the wait
    /// ended, which is what an abandoned capture leaves.
    Empty { path: PathBuf },
    /// The caller's name broke the rule. Nothing was published.
    BadName(NameRefusal),
    /// The session's host cannot reach the capture, which runs in the
    /// `hook` state and so only on the hook host. Nothing was published.
    Unsupported { host: String },
    /// The executor answered with something other than `ok`, passed
    /// through as it came: a chunk that raised, a session it refused, its
    /// own failure.
    Replied(Envelope),
    /// An `ok` that is not the directory this chunk returns. The chunk
    /// always returns a string, so a reply saying otherwise did not run
    /// the chunk this side built.
    Malformed(Envelope),
    /// DCS restarted and the request will never run.
    Superseded { id: String },
    /// The process is gone and no session has replaced it.
    Dead { id: String },
}

/// What stopped a capture from coming to any answer at all.
#[derive(Debug)]
pub enum CaptureError {
    /// The request could not be published or its reply could not be read.
    Pipe(PipeError),
    /// The directory or a file in it could not be read.
    Disk { path: PathBuf, source: io::Error },
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pipe(why) => write!(f, "{why}"),
            Self::Disk { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for CaptureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pipe(why) => Some(why),
            Self::Disk { source, .. } => Some(source),
        }
    }
}

/// The host the capture runs on, as the executor names it.
const HOOK: &str = "hook";

/// Take a screenshot under `given`, or under a name supplied from the
/// local clock, and wait up to `upto` for both the reply and the file.
///
/// The handshake is the one the caller already read; it is not read again.
/// The name is checked before the host because both refusals are free and
/// the name is wholly the caller's; neither publishes anything.
///
/// `upto` is one wait over both phases. Whatever the reply leaves of it is
/// spent watching the directory, and the directory is looked at once even
/// when the reply spent the lot, as a wait reads its table at least once.
pub fn capture(
    h: &Handshake,
    given: Option<&str>,
    upto: Duration,
) -> Result<Capture, CaptureError> {
    let name = match shot_name::resolve(given) {
        Ok(name) => name,
        Err(why) => return Ok(Capture::BadName(why)),
    };
    if h.host != HOOK {
        return Ok(Capture::Unsupported {
            host: h.host.clone(),
        });
    }
    // Read before the request is published, from the clock a file's
    // modified time is stamped from: decision record 0038.
    let since = sys::file_clock_now();
    let deadline = wait::latest(Instant::now(), upto);
    // The name has passed the rule, which admits letters, digits, `_` and
    // `-` and nothing else, so it needs no escaping between the quotes.
    let body = format!("DCS.makeScreenShot(\"{name}\") return lfs.writedir()");
    let spec = Spec::new(
        &[
            ("op", "eval"),
            ("for", h.stamp.as_str()),
            ("state", HOOK),
            ("chunkname", "=dcs-eval screenshot"),
        ],
        body.as_bytes(),
    );
    // Publication is lazy, so the reading above really is taken first.
    let outcome = match Pipeline::over(h, vec![spec], 1, upto).next() {
        Some(Ok(outcome)) => outcome,
        Some(Err(why)) => return Err(CaptureError::Pipe(why)),
        None => unreachable!("a window over one spec yields that spec"),
    };
    let envelope = match outcome {
        Outcome::Reply(envelope) => envelope,
        Outcome::Pending { id, phase, flag } => return Ok(pending(h, id, phase, flag, name)),
        Outcome::Superseded { id } => return Ok(Capture::Superseded { id }),
        Outcome::Dead { id } => return Ok(Capture::Dead { id }),
    };
    if envelope.headers.get("status") != Some("ok") {
        return Ok(Capture::Replied(envelope));
    }
    let Some(dir) = writedir(&envelope) else {
        return Ok(Capture::Malformed(envelope));
    };
    let dir = dir.join("ScreenShots");
    let mut last = None;
    loop {
        let found = shot_file::find(&dir, &name).map_err(disk(&dir))?;
        let fresh = found.filter(|candidate| candidate.modified >= since);
        if let Some(candidate) = fresh {
            match candidate.examine().map_err(disk(&candidate.path))? {
                Finding::Whole(picture) => {
                    return Ok(Capture::Ok {
                        path: candidate.path,
                        picture,
                    });
                }
                finding => last = Some((candidate.path, finding)),
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        thread::sleep(LOOK_EVERY.min(deadline.saturating_duration_since(Instant::now())));
    }
    // Zero bytes is an answer only once the wait is over: a capture is
    // empty for part of its own writing. The last look decides, so a file
    // that was empty and has since grown is not called abandoned.
    if let Some((path, Finding::Empty)) = last {
        return Ok(Capture::Empty { path });
    }
    Ok(Capture::NotWritten { dir, name })
}

/// The directory an `ok` reply names, where it is one: a string, text,
/// and absolute. `lfs.writedir()` is always all three, and a relative
/// path would be read against this process's working directory, which is
/// not DCS's.
fn writedir(envelope: &Envelope) -> Option<PathBuf> {
    if envelope.headers.get("result_type") != Some("string") {
        return None;
    }
    let text = std::str::from_utf8(&envelope.body).ok()?;
    let dir = PathBuf::from(text);
    dir.is_absolute().then_some(dir)
}

/// A `pending`, with the directory the capture would land in.
fn pending(h: &Handshake, id: String, phase: String, flag: Option<Flag>, name: String) -> Capture {
    Capture::Pending {
        id,
        phase,
        flag,
        dir: pending_dir(h),
        name,
    }
}

/// Where a capture lands when no reply has said so: the executor writes
/// its output to `<writedir>\Logs\DcsEval\<host>`, so the write directory
/// is that output with those three removed. An output of any other shape
/// says nothing about where the write directory is, and the answer is
/// `None` rather than a guess. Decision record 0039.
fn pending_dir(h: &Handshake) -> Option<PathBuf> {
    let output = h.output.as_path();
    let mut tail = output.components().rev();
    let named = |part: Option<Component<'_>>, want: &str| {
        part.is_some_and(|part| part.as_os_str().eq_ignore_ascii_case(want))
    };
    if !(named(tail.next(), &h.host) && named(tail.next(), "DcsEval") && named(tail.next(), "Logs"))
    {
        return None;
    }
    Some(output.ancestors().nth(3)?.join("ScreenShots"))
}

/// A disk failure at `path`, for `map_err`.
fn disk(path: &Path) -> impl FnOnce(io::Error) -> CaptureError + '_ {
    move |source| CaptureError::Disk {
        path: path.to_owned(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::path::Path;
    use std::time::{Duration, Instant, SystemTime};

    use super::*;
    use crate::shot_file::Format;
    use crate::standin::Standin;
    use crate::testing::{Sandbox, entries, real};

    const PNG: &[u8] = include_bytes!("../fixtures/shot/picture.png");

    /// Long enough for any reply and any file a test stages to land.
    const PLENTY: Duration = Duration::from_secs(5);
    /// A wait that ends with nothing having arrived, kept short because
    /// every test that uses it spends all of it.
    const BRIEF: Duration = Duration::from_millis(500);
    /// How long the file is held back after the reply in the late-file
    /// test: several looks, so a watch that stopped at the reply sees none
    /// of it.
    const LATE: Duration = Duration::from_millis(400);

    /// A ticking session whose output sits where the executor puts it,
    /// under `<writedir>\Logs\DcsEval\<host>`, answering the capture's
    /// chunk with that writedir as `lfs.writedir()` spells it, trailing
    /// separator and all.
    fn session(b: &Sandbox, host: &str) -> (Standin, Handshake, PathBuf) {
        let (mut s, h, writedir) = unscripted(b, host);
        let answer = format!("{}\\", writedir.display());
        s.script("makeScreenShot", "ok", "string", answer.as_bytes());
        (s, h, writedir)
    }

    /// The same session with nothing scripted, for a test that stages an
    /// answer of its own: the stand-in takes the first script that matches.
    fn unscripted(b: &Sandbox, host: &str) -> (Standin, Handshake, PathBuf) {
        let writedir = b.join("Saved Games");
        let mut s = Standin::open(&writedir.join("Logs").join("DcsEval").join(host), host)
            .expect("the stand-in opens");
        s.pid = std::process::id();
        s.armed = true;
        s.handshake().expect("the handshake publishes");
        s.beat(SystemTime::now()).expect("a fresh beat");
        let h = Handshake::read(&s.output().join("executor.txt")).expect("the handshake reads");
        (s, h, writedir)
    }

    /// Poll until a request is on the disk, panicking where none comes:
    /// every caller gates a tick or a write on it, and a gate that quietly
    /// opened proves nothing.
    fn published(dir: &Path) {
        let give_up = Instant::now() + PLENTY;
        loop {
            let saw = fs::read_dir(dir)
                .expect("the directory lists")
                .filter_map(Result::ok)
                .any(|e| e.file_name().to_string_lossy().ends_with(".req"));
            if saw {
                return;
            }
            assert!(Instant::now() < give_up, "no request in {}", dir.display());
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn shots(writedir: &Path) -> PathBuf {
        let dir = writedir.join("ScreenShots");
        fs::create_dir_all(&dir).expect("the directory is made");
        dir
    }

    fn written(path: &Path, bytes: &[u8], modified: SystemTime) {
        fs::write(path, bytes).expect("the file is written");
        File::options()
            .write(true)
            .open(path)
            .and_then(|file| file.set_modified(modified))
            .expect("the time is set");
    }

    /// The capture, run while `then` answers the request on another
    /// thread once it is published.
    fn answered(
        s: &mut Standin,
        h: &Handshake,
        given: Option<&str>,
        upto: Duration,
        then: impl FnOnce(&mut Standin) + Send,
    ) -> Capture {
        std::thread::scope(|scope| {
            scope.spawn(|| {
                published(s.req());
                then(s);
            });
            capture(h, given, upto).expect("the capture comes to an answer")
        })
    }

    #[test]
    fn a_file_written_late_in_the_wait_is_the_capture() {
        let b = Sandbox::new();
        let (mut s, h, writedir) = session(&b, "hook");
        let dir = shots(&writedir);
        let answer = answered(&mut s, &h, Some("shot"), PLENTY, |s| {
            s.tick();
            std::thread::sleep(LATE);
            fs::write(dir.join("shot.png"), PNG).expect("the file is written");
        });
        let Capture::Ok { path, picture } = answer else {
            panic!("a whole file arrived in the wait, and the answer was {answer:?}");
        };
        assert_eq!(path, writedir.join("ScreenShots").join("shot.png"));
        assert_eq!(picture.format, Format::Png);
        assert_eq!(picture.bytes, PNG.len() as u64);
        assert_eq!((picture.width, picture.height), (37, 23));
    }

    #[test]
    fn a_whole_file_older_than_the_request_is_not_written() {
        let b = Sandbox::new();
        let (mut s, h, writedir) = session(&b, "hook");
        let dir = shots(&writedir);
        let old = SystemTime::now() - Duration::from_secs(60);
        written(&dir.join("shot.png"), PNG, old);
        let answer = answered(&mut s, &h, Some("shot"), BRIEF, |s| {
            s.tick();
        });
        let Capture::NotWritten { dir: watched, name } = answer else {
            panic!("the only file under the name is a minute old, and the answer was {answer:?}");
        };
        assert_eq!((watched, name.as_str()), (dir, "shot"));
    }

    #[test]
    fn a_reply_that_never_comes_is_pending_with_its_id() {
        let b = Sandbox::new();
        let (s, h, writedir) = session(&b, "hook");
        let answer = capture(&h, Some("shot"), BRIEF).expect("the capture comes to an answer");
        let Capture::Pending {
            id,
            phase,
            flag,
            dir,
            name,
        } = answer
        else {
            panic!("nothing answered, and the answer was {answer:?}");
        };
        let left = entries(s.req());
        assert_eq!(left, format!("{id}.req"), "the id is the request's own");
        assert_eq!(phase, "menu");
        assert_eq!(flag, None);
        assert_eq!(name, "shot");
        // The handshake's output is the resolved spelling, which this host
        // gives the sandbox's temp directory differently from the one that
        // made it.
        let output = real(&writedir.join("Logs").join("DcsEval").join("hook"));
        let expected = output
            .as_path()
            .ancestors()
            .nth(3)
            .map(|w| w.join("ScreenShots"));
        assert_eq!(dir, expected);
    }

    #[test]
    fn a_reply_with_no_file_is_not_written() {
        let b = Sandbox::new();
        let (mut s, h, writedir) = session(&b, "hook");
        let answer = answered(&mut s, &h, Some("shot"), BRIEF, |s| {
            s.tick();
        });
        let Capture::NotWritten { dir, name } = answer else {
            panic!("the reply came and no file did, and the answer was {answer:?}");
        };
        assert_eq!(dir, writedir.join("ScreenShots"));
        assert_eq!(name, "shot");
        assert!(!dir.exists(), "the directory is watched, never made");
    }

    #[test]
    fn the_export_host_is_refused_and_names_the_hook() {
        let b = Sandbox::new();
        let (s, h, _) = session(&b, "export");
        let answer = capture(&h, Some("shot"), BRIEF).expect("the capture comes to an answer");
        let Capture::Unsupported { host } = answer else {
            panic!("the export host cannot reach the capture, and the answer was {answer:?}");
        };
        assert_eq!(host, "export");
        assert_eq!(entries(s.req()), "", "nothing is published");
        assert!(!s.arm().exists(), "nothing is armed");
    }

    #[test]
    fn the_request_is_the_hook_chunk_under_the_name() {
        let b = Sandbox::new();
        let (mut s, h, _) = session(&b, "hook");
        let _ = answered(&mut s, &h, Some("shot"), BRIEF, |s| {
            s.tick();
        });
        let [seen] = s.seen() else {
            panic!("one request is published, and {} were", s.seen().len());
        };
        let text = String::from_utf8_lossy(&seen.bytes);
        let (head, body) = text.split_once("\n\n").expect("a header block");
        let stamp = format!("for: {}", h.stamp);
        for line in [
            "op: eval",
            "state: hook",
            stamp.as_str(),
            "chunkname: =dcs-eval screenshot",
        ] {
            assert!(
                head.lines().any(|l| l == line),
                "{line:?} missing from {head}"
            );
        }
        assert_eq!(body, "DCS.makeScreenShot(\"shot\") return lfs.writedir()");
    }

    #[test]
    fn a_file_written_while_the_request_is_published_counts() {
        let b = Sandbox::new();
        let (mut s, h, writedir) = session(&b, "hook");
        let dir = shots(&writedir);
        let answer = answered(&mut s, &h, Some("shot"), PLENTY, |s| {
            fs::write(dir.join("shot.png"), PNG).expect("the file is written");
            s.tick();
        });
        assert!(
            matches!(answer, Capture::Ok { .. }),
            "the file came after the reading, and the answer was {answer:?}"
        );
    }

    #[test]
    fn a_file_still_empty_when_the_wait_ends_is_empty() {
        let b = Sandbox::new();
        let (mut s, h, writedir) = session(&b, "hook");
        let dir = shots(&writedir);
        let answer = answered(&mut s, &h, Some("shot"), BRIEF, |s| {
            s.tick();
            fs::write(dir.join("shot"), b"").expect("the file is written");
        });
        let Capture::Empty { path } = answer else {
            panic!("a zero-byte file was all that came, and the answer was {answer:?}");
        };
        assert_eq!(path, writedir.join("ScreenShots").join("shot"));
    }

    #[test]
    fn a_file_finished_during_the_wait_is_looked_at_again() {
        let b = Sandbox::new();
        let (mut s, h, writedir) = session(&b, "hook");
        let dir = shots(&writedir);
        let answer = answered(&mut s, &h, Some("shot"), PLENTY, |s| {
            s.tick();
            fs::write(dir.join("shot.png"), &PNG[..PNG.len() / 2]).expect("half is written");
            std::thread::sleep(LATE);
            fs::write(dir.join("shot.png"), PNG).expect("the rest is written");
        });
        assert!(
            matches!(answer, Capture::Ok { .. }),
            "the file was finished inside the wait, and the answer was {answer:?}"
        );
    }

    #[test]
    fn a_name_that_breaks_the_rule_is_refused_unpublished() {
        let b = Sandbox::new();
        let (s, h, _) = session(&b, "hook");
        let answer = capture(&h, Some("a.b"), BRIEF).expect("the capture comes to an answer");
        let Capture::BadName(why) = answer else {
            panic!("a dot breaks the rule, and the answer was {answer:?}");
        };
        assert_eq!(
            why,
            NameRefusal::Character {
                at: 2,
                character: '.'
            }
        );
        assert_eq!(entries(s.req()), "", "nothing is published");
    }

    #[test]
    fn an_executor_refusal_is_passed_through() {
        let b = Sandbox::new();
        let (mut s, h, _) = unscripted(&b, "hook");
        s.script(
            "makeScreenShot",
            "run",
            "string",
            b"attempt to call a nil value",
        );
        let answer = answered(&mut s, &h, Some("shot"), PLENTY, |s| {
            s.tick();
        });
        let Capture::Replied(envelope) = answer else {
            panic!("the chunk raised, and the answer was {answer:?}");
        };
        assert_eq!(envelope.headers.get("status"), Some("run"));
        assert_eq!(envelope.body, b"attempt to call a nil value");
    }

    #[test]
    fn a_reply_that_is_not_a_directory_is_malformed() {
        let b = Sandbox::new();
        let (mut s, h, _) = unscripted(&b, "hook");
        s.script("makeScreenShot", "ok", "nil", b"");
        let answer = answered(&mut s, &h, Some("shot"), PLENTY, |s| {
            s.tick();
        });
        assert!(
            matches!(answer, Capture::Malformed(_)),
            "the reply named no directory, and the answer was {answer:?}"
        );
    }

    #[test]
    fn no_name_given_is_one_supplied() {
        let b = Sandbox::new();
        let (mut s, h, _) = session(&b, "hook");
        let answer = answered(&mut s, &h, None, BRIEF, |s| {
            s.tick();
        });
        let Capture::NotWritten { name, .. } = answer else {
            panic!("the reply came and no file did, and the answer was {answer:?}");
        };
        assert!(name.starts_with("dcs-eval-"), "{name}");
    }
}
