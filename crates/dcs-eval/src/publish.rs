//! A file published by rename, and the arm file: how a request reaches the
//! executor's directory whole, and how a dormant executor is told to look.
//!
//! The disk discipline is the executor's own `publish` in
//! `DcsEvalExecutor.lua`, mirrored so both ends of the wire keep the same
//! promise: a file appears under its final name whole or not at all. The
//! bytes go to `<path>.tmp` in the directory the file will live in, so the
//! rename never crosses a device, and a reader that lists nothing but an
//! exact suffix never meets a half-written file. From the moment the
//! `.tmp` opens, no failure past that point leaves it behind.
//!
//! What the tests here can pin is the name the bytes go through and what is
//! left on the disk after each refusal. Whether the final name arrives by a
//! rename rather than a copy and a remove is not visible from the directory
//! afterwards; only the harness's spy on the executor's calls sees that, and
//! this side is read against it.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::protocol::{FrameError, frame};

/// A step on the disk that refused: the path it was about, and what the OS
/// said. `Display` is `<path>: <reason>`, the executor's shape for the same
/// refusal, so a log line from either end reads the same way; the reason is
/// the OS's own text rather than the C runtime's, which is as close as the
/// two hosts get. `path` and `source` are open so a caller can read the
/// kind rather than the text.
#[derive(Debug)]
pub struct DiskError {
    /// The `.tmp` when it could not be opened; the final name otherwise.
    pub path: PathBuf,
    pub source: io::Error,
}

impl fmt::Display for DiskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.source)
    }
}

impl std::error::Error for DiskError {}

/// One file, published by rename: `bytes` to `<path>.tmp`, created or
/// truncated, then renamed onto `path`. `fs::rename` replaces a file
/// already under the final name on Windows, documented, so nothing is
/// removed first; the executor removes the final name before its rename
/// only because Lua's `os.rename` cannot replace one. A rename onto a name
/// something holds open, or that is a directory, refuses, and that is where
/// the verdict is read.
///
/// `File` does not buffer, so a write that fails fails at `write_all`; the
/// executor's separate check on `close` guards C stdio's buffer, which
/// there is none of here. The handle is dropped before the rename, and
/// before the remove on the way out. Nothing is synced: the reader is on
/// the same machine, through the same cache.
///
/// The refusal names the `.tmp` when it could not be opened and the final
/// name otherwise, and every path past the open tries to remove the `.tmp`;
/// as in the executor, that remove's own answer is not read.
pub fn publish(path: &Path, bytes: &[u8]) -> Result<(), DiskError> {
    let tmp = path.with_added_extension("tmp");
    let mut file = match File::create(&tmp) {
        Ok(file) => file,
        Err(source) => return Err(DiskError { path: tmp, source }),
    };
    let written = file.write_all(bytes);
    drop(file);
    if let Err(source) = written.and_then(|()| fs::rename(&tmp, path)) {
        let _ = fs::remove_file(&tmp);
        return Err(DiskError {
            path: path.to_owned(),
            source,
        });
    }
    Ok(())
}

/// The arm file, ensured: the signal to a dormant executor that a request
/// is waiting. Its existence is the whole signal and its content is
/// nothing, so it is stat'd first and created empty only when absent. The
/// executor arms on anything the stat returns, so anything present is left
/// as it is, a file with content or a directory alike, and a race that
/// makes the create find one already there is the same as finding it at
/// the stat. The client never removes it: the executor does, once it has
/// listed the request directory a last time, and that order is what keeps
/// a request from being stranded. A parent that is gone, a session
/// directory removed by the next load, is an error naming the arm path.
pub fn arm(path: &Path) -> Result<(), DiskError> {
    match fs::metadata(path) {
        Ok(_) => return Ok(()),
        Err(source) if source.kind() != io::ErrorKind::NotFound => {
            return Err(DiskError {
                path: path.to_owned(),
                source,
            });
        }
        Err(_) => {}
    }
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(_) => Ok(()),
        Err(source) if source.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(source) => Err(DiskError {
            path: path.to_owned(),
            source,
        }),
    }
}

/// Whether `id` is `<seq>-<tag>`: ten digits, one dash, four to twelve
/// letters or digits. The shape the client mints, so ids sort in
/// publication order and two clients sharing a session never collide. It
/// is also the containment check on the way to the disk: the id becomes
/// `<req>/<id>.req`, and this alphabet admits no dot, slash or backslash,
/// so no id names a path outside the request directory.
pub fn is_id(id: &str) -> bool {
    let b = id.as_bytes();
    let Some(dash) = b.iter().position(|&c| c == b'-') else {
        return false;
    };
    let (seq, tag) = (&b[..dash], &b[dash + 1..]);
    seq.len() == 10
        && seq.iter().all(u8::is_ascii_digit)
        && (4..=12).contains(&tag.len())
        && tag.iter().all(u8::is_ascii_alphanumeric)
}

/// Why `send` landed no request, or landed one the executor may not be
/// looking for. The first three write nothing that was not there; the
/// last says the request is on the disk.
#[derive(Debug)]
pub enum SendError {
    /// Not `<seq>-<tag>`, so not a name this side will put in a path.
    Id { id: String },
    /// A header the executor's `parse` would refuse, refused before the
    /// disk is touched.
    Frame(FrameError),
    /// The request did not land; the arm file was not touched.
    Publish(DiskError),
    /// The request landed and the arm file could not be made, so a dormant
    /// executor has nothing to wake it. The request stays where it is.
    Arm(DiskError),
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id { id } => write!(f, "the id {id} is not [0-9]{{10}}-[A-Za-z0-9]{{4,12}}"),
            Self::Frame(err) => write!(f, "{err}"),
            Self::Publish(err) => write!(f, "the request was not published: {err}"),
            Self::Arm(err) => {
                write!(f, "the request is published but the arm file is not: {err}")
            }
        }
    }
}

impl std::error::Error for SendError {}

/// A request to the executor: `headers` and `body` framed as an envelope,
/// published as `<req>/<id>.req` by rename, then the arm file at
/// `arm_path` ensured. In that order, so a request that did not land arms
/// nothing, and the executor's own order on the way to sleep, remove the
/// arm file and then list once more, meets a request that is either
/// already listed or has recreated the file behind it. Every header is the
/// caller's, the session stamp under `for` included; nothing is added, so
/// what lands is what was asked for.
pub fn send(
    req: &Path,
    arm_path: &Path,
    id: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Result<(), SendError> {
    if !is_id(id) {
        return Err(SendError::Id { id: id.to_owned() });
    }
    let bytes = frame(headers, body).map_err(SendError::Frame)?;
    publish(&req.join(format!("{id}.req")), &bytes).map_err(SendError::Publish)?;
    arm(arm_path).map_err(SendError::Arm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Sandbox, entries, slurp};

    // ---- a file, published by rename --------------------------------------

    #[test]
    fn publish_lands_the_bytes_under_the_final_name_and_nothing_else() {
        let b = Sandbox::new();
        let final_ = b.join("1.req");
        publish(&final_, b"op: ping\n\n").expect("the file publishes");
        assert_eq!(slurp(&final_), b"op: ping\n\n");
        assert_eq!(
            entries(&b.path),
            "1.req",
            "the directory holds the file and no .tmp"
        );
    }

    #[test]
    fn publish_over_itself_and_the_second_wins() {
        // No remove before the rename: a rename replaces on this host.
        let b = Sandbox::new();
        let path = b.join("executor.txt");
        publish(&path, b"one").expect("the first publish");
        publish(&path, b"two").expect("the second publish, over the first");
        assert_eq!(slurp(&path), b"two");
        assert_eq!(entries(&b.path), "executor.txt", "no .tmp is left");
    }

    #[test]
    fn publish_into_a_directory_that_is_not_there_names_the_tmp() {
        let b = Sandbox::new();
        let path = b.join("nowhere").join("x.res");
        let err = publish(&path, b"x").expect_err("nowhere to open the .tmp");
        assert_eq!(err.path, b.join("nowhere").join("x.res.tmp"));
        assert_eq!(err.source.kind(), io::ErrorKind::NotFound);
        assert!(
            err.to_string()
                .starts_with(&format!("{}: ", err.path.display()))
        );
        assert_eq!(entries(&b.path), "", "nothing is written");
    }

    #[test]
    fn publish_goes_through_the_tmp_a_directory_there_stops_it() {
        // A directory sitting where the .tmp goes makes the open refuse, and
        // the final name never appears. A publish that wrote the final name
        // directly would land here, so this is what tells the two apart.
        let b = Sandbox::new();
        let final_ = b.join("1.req");
        let tmp = b.join("1.req.tmp");
        fs::create_dir(&tmp).expect("a directory at the .tmp");
        let err = publish(&final_, b"x").expect_err("the .tmp cannot be opened");
        assert_eq!(err.path, tmp, "the refusal names the .tmp");
        assert!(!final_.exists(), "the final name never appears");
        assert!(tmp.is_dir(), "and the directory is left where it was");
        assert_eq!(entries(&b.path), "1.req.tmp");
    }

    #[test]
    fn publish_onto_a_directory_refuses_at_the_rename_and_leaves_no_tmp() {
        // An empty directory, the strict form: nothing a rename could
        // replace, and the tightest case of a rename that must refuse.
        let b = Sandbox::new();
        let final_ = b.join("1.req");
        fs::create_dir(&final_).expect("a directory at the final name");
        let err = publish(&final_, b"x").expect_err("the rename refuses");
        assert_eq!(err.path, final_, "the refusal names the final");
        assert!(final_.is_dir(), "the directory stays");
        assert_eq!(entries(&b.path), "1.req", "and no .tmp survives");
    }

    #[cfg(windows)]
    #[test]
    fn publish_onto_a_held_file_refuses_and_keeps_the_old_bytes() {
        // A hold that shares nothing, so the rename refuses whichever way
        // this host's rename is done: the one that replaces would need the
        // holder to share delete. Rust's own opens share delete by default,
        // which is why the hold is opened by hand.
        use std::os::windows::fs::OpenOptionsExt;
        let b = Sandbox::new();
        let final_ = b.join("1-a.res");
        publish(&final_, b"first").expect("the first publish lands");
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&final_)
            .expect("the hold");
        let err = publish(&final_, b"second").expect_err("a publish onto a held file refuses");
        // The hold denies this process's own reads too, so it goes first.
        drop(held);
        assert_eq!(err.path, final_, "the refusal names the final");
        assert_eq!(slurp(&final_), b"first", "the old bytes stay");
        assert_eq!(entries(&b.path), "1-a.res", "and no .tmp survives");
    }

    // ---- the arm file -----------------------------------------------------

    #[test]
    fn arm_creates_the_file_when_absent_and_empty() {
        let b = Sandbox::new();
        let path = b.join("arm");
        arm(&path).expect("the arm file is made");
        assert!(path.is_file(), "a regular file");
        assert_eq!(
            slurp(&path),
            b"",
            "with nothing in it: existence is the signal"
        );
        assert_eq!(entries(&b.path), "arm");
    }

    #[test]
    fn arm_leaves_a_present_file_as_it_is() {
        let b = Sandbox::new();
        let path = b.join("arm");
        fs::write(&path, b"left here by someone").expect("a present arm file");
        arm(&path).expect("a present arm file is fine");
        assert_eq!(
            slurp(&path),
            b"left here by someone",
            "its content is not touched"
        );
        arm(&path).expect("and again");
        assert_eq!(slurp(&path), b"left here by someone");
    }

    #[test]
    fn arm_takes_a_directory_at_the_path_as_present() {
        // The executor arms on any answer to its stat; so does this side.
        let b = Sandbox::new();
        let path = b.join("arm");
        fs::create_dir(&path).expect("a directory at the arm path");
        arm(&path).expect("present is present");
        assert!(path.is_dir(), "and it is left alone");
    }

    #[test]
    fn arm_with_its_parent_gone_names_the_arm_path() {
        let b = Sandbox::new();
        let path = b.join("gone").join("arm");
        let err = arm(&path).expect_err("no directory to make it in");
        assert_eq!(err.path, path);
        assert_eq!(err.source.kind(), io::ErrorKind::NotFound);
        assert_eq!(entries(&b.path), "", "nothing is made");
    }

    // ---- a request, sent --------------------------------------------------

    const ID: &str = "0000000001-abcd";
    const PING: [(&str, &str); 2] = [("op", "ping"), ("for", "0000000001-abcd")];

    /// A session directory: `req/` made, the arm path beside it, nothing
    /// else.
    fn session(b: &Sandbox) -> (PathBuf, PathBuf) {
        let req = b.join("req");
        fs::create_dir(&req).expect("the request directory");
        (req, b.join("arm"))
    }

    #[test]
    fn send_lands_the_envelope_under_the_id_and_makes_the_arm_file() {
        let b = Sandbox::new();
        let (req, arm_path) = session(&b);
        send(&req, &arm_path, ID, &PING, b"").expect("the request sends");
        assert_eq!(
            entries(&req),
            "0000000001-abcd.req",
            "the request under its final name, no .tmp"
        );
        let want = frame(&PING, b"").expect("the same envelope");
        assert_eq!(
            slurp(&req.join("0000000001-abcd.req")),
            want,
            "holding what frame writes"
        );
        assert!(arm_path.is_file(), "the arm file appears");
        assert_eq!(slurp(&arm_path), b"", "empty");
    }

    #[test]
    fn send_never_removes_the_arm_file() {
        // The arm file is there before the send, with content nobody here
        // wrote: after two sends it is still there, as it was. A send that
        // removed it, or made it afresh, goes red here.
        let b = Sandbox::new();
        let (req, arm_path) = session(&b);
        fs::write(&arm_path, b"present").expect("an arm file already there");
        send(&req, &arm_path, ID, &PING, b"").expect("the first send");
        assert!(
            arm_path.is_file(),
            "the arm file is still there after one send"
        );
        assert_eq!(slurp(&arm_path), b"present", "as it was");
        send(&req, &arm_path, "0000000002-abcd", &PING, b"return 1").expect("the second send");
        assert!(arm_path.is_file(), "and after two");
        assert_eq!(slurp(&arm_path), b"present", "as it was");
        assert_eq!(entries(&req), "0000000001-abcd.req 0000000002-abcd.req");
    }

    #[test]
    fn send_refuses_an_id_that_is_not_one_and_touches_nothing() {
        let b = Sandbox::new();
        let (req, arm_path) = session(&b);
        for id in [
            "1-a",
            "0000000001-abc",
            "0000000001-abcdefghijklm",
            "00000000001-abcd",
            "",
        ] {
            let err = send(&req, &arm_path, id, &PING, b"").expect_err(id);
            assert!(
                matches!(&err, SendError::Id { id: got } if got == id),
                "{id:?}: {err}"
            );
            assert_eq!(
                err.to_string(),
                format!("the id {id} is not [0-9]{{10}}-[A-Za-z0-9]{{4,12}}")
            );
        }
        assert_eq!(entries(&req), "", "nothing lands");
        assert!(!arm_path.exists(), "and nothing arms");
    }

    #[test]
    fn send_refuses_a_header_the_framer_refuses_and_touches_nothing() {
        let b = Sandbox::new();
        let (req, arm_path) = session(&b);
        let err = send(&req, &arm_path, ID, &[("op", "ping\nstatus: fake")], b"")
            .expect_err("the injection guard");
        assert!(
            matches!(err, SendError::Frame(FrameError::LineBreak { .. })),
            "{err}"
        );
        assert_eq!(err.to_string(), "op: the value carries a CR or LF");
        assert_eq!(entries(&req), "", "nothing lands");
        assert!(!arm_path.exists(), "and nothing arms");
    }

    #[test]
    fn send_with_no_request_directory_publishes_nothing_and_arms_nothing() {
        // Publish first, then arm: a request that did not land wakes no
        // executor to look for it.
        let b = Sandbox::new();
        let req = b.join("req");
        let arm_path = b.join("arm");
        let err = send(&req, &arm_path, ID, &PING, b"").expect_err("nowhere to land");
        let SendError::Publish(inner) = &err else {
            panic!("a publish refusal, not {err}");
        };
        assert_eq!(inner.path, req.join("0000000001-abcd.req.tmp"));
        assert!(
            err.to_string()
                .starts_with("the request was not published: "),
            "{err}"
        );
        assert!(!arm_path.exists(), "the arm file is not made");
        assert_eq!(entries(&b.path), "", "nothing at all is made");
    }

    #[test]
    fn send_that_cannot_arm_says_the_request_is_published() {
        let b = Sandbox::new();
        let (req, _) = session(&b);
        let arm_path = b.join("gone").join("arm");
        let err = send(&req, &arm_path, ID, &PING, b"").expect_err("nowhere to arm");
        let SendError::Arm(inner) = &err else {
            panic!("an arm refusal, not {err}");
        };
        assert_eq!(inner.path, arm_path);
        assert!(
            err.to_string()
                .starts_with("the request is published but the arm file is not: "),
            "{err}"
        );
        assert_eq!(
            entries(&req),
            "0000000001-abcd.req",
            "the request is on the disk, no .tmp"
        );
    }

    #[test]
    fn is_id_the_shape_and_the_alphabet() {
        for id in [
            "0000000001-abcd",
            "9999999999-ABCDEFGHIJKL",
            "0000000000-a1B2",
            "0000000001-0000",
        ] {
            assert!(is_id(id), "{id:?}");
        }
        let refused = [
            "",
            "1-a",
            "000000001-abcd",           // nine digits
            "00000000001-abcd",         // eleven
            "0000000001-abc",           // a tag of three
            "0000000001-abcdefghijklm", // of thirteen
            "0000000001-",              // of none
            "0000000001-ab-cd",         // a second dash
            "000000000a-abcd",          // a letter in the sequence
            "0000000001-caf\u{e9}",     // a byte past ASCII
            "0000000001-abcd\n",
            "0000000001_abcd",
            // The containment cases: nothing here can leave the directory.
            "../x",
            "0000000001-../x",
            "0000000001-a/bc",
            "0000000001-a\\bc",
            "0000000001-a.bc",
        ];
        for id in refused {
            assert!(!is_id(id), "{id:?}");
        }
    }
}
