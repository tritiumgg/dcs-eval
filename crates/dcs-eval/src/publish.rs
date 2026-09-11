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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fresh directory under the host's temp directory, gone when the
    /// test ends. The name carries the process id and a counter, and the
    /// directory is cleared before use: process ids are recycled and a
    /// killed run leaves its directory behind.
    struct Sandbox {
        path: PathBuf,
    }

    impl Sandbox {
        fn new() -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("dcs-eval-publish-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("the box is made");
            Self { path }
        }

        fn join(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    /// The names in `dir`, sorted, joined with a space; empty when empty.
    fn entries(dir: &Path) -> String {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("the directory lists")
            .map(|e| {
                e.expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names.join(" ")
    }

    fn slurp(path: &Path) -> Vec<u8> {
        fs::read(path).expect("the file reads")
    }

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
}
