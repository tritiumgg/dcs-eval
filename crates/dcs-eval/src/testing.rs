//! What the tests of more than one module need: a directory to work in that
//! is gone when the test ends, and two readers of what was left in it.
//! Compiled for the crate's own tests only.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A fresh directory under the host's temp directory, gone when the test
/// ends. The name carries the process id and a counter, and the directory
/// is cleared before use: process ids are recycled and a killed run leaves
/// its directory behind.
pub(crate) struct Sandbox {
    pub(crate) path: PathBuf,
}

impl Sandbox {
    pub(crate) fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("dcs-eval-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the box is made");
        Self { path }
    }

    pub(crate) fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// The names in `dir`, sorted, joined with a space; empty when empty.
pub(crate) fn entries(dir: &Path) -> String {
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

pub(crate) fn slurp(path: &Path) -> Vec<u8> {
    fs::read(path).expect("the file reads")
}
