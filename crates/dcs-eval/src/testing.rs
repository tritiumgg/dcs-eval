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

/// The 8.3 short spelling of an existing path, from the shell's own
/// expansion. The crate declares no Win32 call for this and does not want
/// one for a test helper; `cmd` is already a dependency of the harness's
/// world.
///
/// It panics when the volume hands back the long name, which is what a
/// volume with 8dot3name disabled does. The control it feeds is about a
/// spelling this host can make, so a host that cannot make one must say so
/// rather than let the test pass having proved nothing.
#[cfg(windows)]
pub(crate) fn short_name(path: &Path) -> PathBuf {
    let out = cmd(&format!("for %I in (\"{}\") do @echo %~sI", path.display()));
    let short = PathBuf::from(out.trim());
    assert_ne!(
        short.file_name(),
        path.file_name(),
        "no 8.3 short name for {}: 8dot3name is off on this volume, so the control cannot run",
        path.display()
    );
    short
}

/// A directory junction at `link` pointing at `target`. Junctions need no
/// privilege, unlike a directory symlink, which is why the control uses one
/// and why this can run unelevated.
///
/// It panics with `mklink`'s own words when the link is not made: a control
/// about following a junction that quietly did not have one to follow
/// proves nothing.
#[cfg(windows)]
pub(crate) fn junction(link: &Path, target: &Path) {
    cmd(&format!(
        "mklink /J \"{}\" \"{}\"",
        link.display(),
        target.display()
    ));
    assert!(link.is_dir(), "the junction is there: {}", link.display());
}

/// One `cmd /C` line, its output, or a panic carrying what it said. The
/// line goes on the command line as written, because `cmd` parses the tail
/// of `/C` itself and Rust's own quoting would be a second opinion about
/// it.
#[cfg(windows)]
fn cmd(line: &str) -> String {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("cmd")
        .raw_arg(format!("/C {line}"))
        .output()
        .expect("cmd runs");
    assert!(
        out.status.success(),
        "cmd /C {line} failed: {} {}",
        String::from_utf8_lossy(&out.stdout).trim(),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}
