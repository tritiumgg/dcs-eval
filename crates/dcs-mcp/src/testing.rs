//! A directory to work in that is gone when the test ends.
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
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

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
