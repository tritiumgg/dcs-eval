//! The scaffolding this crate's tests share: a directory to work in that is
//! gone when the test ends, and the stand-in fixtures a verb is driven over.
//!
//! `dcs-eval` has one of these and it is not reachable: it is private to that
//! crate and compiled only for its own tests. The few lines are copied here
//! rather than made public there, because a test helper that two crates share
//! is a third thing to keep working, and this one is small enough that the
//! copy costs less than the seam would.
//!
//! Within *this* crate it is one helper and must stay one. Two of these side
//! by side would name their directories the same way and count from zero
//! apart, so two tests in the same binary would be handed the same path and
//! each would clear the other's tree out from under it.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use dcs_eval::standin::Standin;

/// A fresh directory under the host's temp directory. The name carries the
/// process id and a counter, and the directory is cleared before use: process
/// ids are recycled and a killed run leaves its directory behind.
pub(crate) struct Sandbox {
    pub(crate) path: PathBuf,
}

impl Sandbox {
    pub(crate) fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("dcs-mcp-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the box is made");
        Self { path }
    }

    /// A path inside the box. Nothing is made; the caller decides what the
    /// name is going to be.
    pub(crate) fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    /// A directory inside the box, made along with any parent it names.
    pub(crate) fn dir(&self, name: &str) -> PathBuf {
        let path = self.join(name);
        fs::create_dir_all(&path).expect("the directory is made");
        path
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// A session a verb will find alive: its handshake published, a process id
/// that really is running, armed, and a heartbeat just written.
///
/// The pid matters. A wait that finds a dormant session probes the process
/// the handshake named, and the stand-in's default is a number that is
/// nobody in particular — so a session left at the default would be answered
/// `dead` or `pending` depending on what else happens to be running on the
/// host. This process is certainly alive, which makes the outcome the
/// fixture's and not the machine's.
pub(crate) fn ticking(s: &mut Standin) {
    s.pid = std::process::id();
    s.armed = true;
    s.handshake().expect("the handshake publishes");
    s.beat(std::time::SystemTime::now())
        .expect("the heartbeat publishes");
}

/// One command line run to completion, with the stand-in ticked from this
/// thread until it has answered something.
pub(crate) fn ran(s: &mut Standin, line: Vec<String>) -> (i32, String) {
    let answered = std::thread::scope(|scope| {
        let running = scope.spawn(|| {
            let mut sink: Vec<u8> = Vec::new();
            let code = crate::cli::run(line, &mut sink).expect("the line parses");
            (code, sink)
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while !running.is_finished() && Instant::now() < deadline {
            s.tick();
            std::thread::sleep(Duration::from_millis(25));
        }
        running.join().expect("the verb does not panic")
    });
    let (code, sink) = answered;
    let shown = String::from_utf8_lossy(&sink)
        .trim_end_matches('\n')
        .to_owned();
    (code, shown)
}

/// The single reply file the stand-in published, read off the disk.
pub(crate) fn published(s: &Standin) -> Vec<u8> {
    let mut found: Vec<PathBuf> = fs::read_dir(s.res())
        .expect("the reply directory lists")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "res"))
        .collect();
    assert_eq!(found.len(), 1, "exactly one reply was published: {found:?}");
    fs::read(found.pop().expect("the one reply")).expect("the reply reads")
}

/// A session `verify` will call healthy, published at `output`: a handshake
/// naming this process, which is certainly running, and the temp directory
/// the host really has.
pub(crate) fn a_live_session(output: &Path) -> Standin {
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
pub(crate) fn reheader(path: &Path, name: &str, value: &str) {
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

/// Every file under `dir`, as a sorted list of relative path and bytes.
///
/// Taken beside a `git status` assertion where there is one, and not instead
/// of it: git does not track an empty directory, so a directory made on the
/// way to a read is something git would call clean and this would not.
pub(crate) fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
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
