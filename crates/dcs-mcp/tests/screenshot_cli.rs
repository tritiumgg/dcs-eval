//! The `screenshot` verb's exit codes on the real binary, spawned as a user
//! runs it.
//!
//! The unit tests take the code `cli::run` hands back, which is 0 or 1. The
//! third, 2 for a line that will not parse, is `main`'s: `run` refuses the
//! line and the binary turns the refusal into the usage line and the code.
//! Only the executable can show that, so this spawns it, over a sandbox of
//! its own and never the real `Saved Games`.
//!
//! **Every test function here begins `screenshot_cli_`**, for the reason
//! `installer.rs` beside it gives its own prefix: the command this work is
//! proved by is a `cargo test` filtered on that substring, which exits 0
//! when it matches nothing. `screenshot_cli_names_every_test_here` holds it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

use dcs_eval::standin::Standin;

/// A fresh directory under the host's temp directory, cleared first and
/// removed when the test ends.
struct Sandbox {
    path: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("dcs-mcp-screenshot-cli-{}-{n}", std::process::id()));
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

/// One run of the binary's `screenshot` over the sandbox, the words given
/// first: its exit code, and its stdout with anything on stderr after it.
fn spawned(b: &Sandbox, words: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_dcs-mcp"))
        .arg("screenshot")
        .args(words)
        .arg("--saved-games")
        .arg(&b.path)
        .args(["--variant", "DCS.openbeta", "--data-dir"])
        .arg(b.join("data"))
        .output()
        .expect("the binary starts");
    let code = out.status.code().expect("the binary exits with a code");
    let shown = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (code, shown)
}

/// A session the binary will find alive and never hear from: published,
/// naming this process, armed and freshly beaten, and never ticked.
fn silent(output: &Path) -> Standin {
    let mut s = Standin::open(output, "hook").expect("the stand-in opens");
    s.pid = std::process::id();
    s.armed = true;
    s.handshake().expect("the handshake publishes");
    s.beat(SystemTime::now()).expect("the heartbeat publishes");
    s
}

#[test]
fn screenshot_cli_the_binary_exits_two_for_a_line_that_will_not_parse() {
    let b = Sandbox::new();
    for (words, named) in [
        (&["--capture"][..], "--capture"),
        (&["--name", "one", "--name", "two"], "--name"),
        (&["--wait-seconds", "soon"], "--wait-seconds"),
    ] {
        let (code, shown) = spawned(&b, words);
        assert_eq!(code, 2, "{words:?}: {shown}");
        assert!(
            shown.contains(named) && shown.contains("usage:"),
            "{words:?} is refused naming {named}, with the usage line: {shown}"
        );
    }
}

#[test]
fn screenshot_cli_the_binary_exits_one_for_a_refusal_and_zero_for_a_pending() {
    let b = Sandbox::new();
    let (code, shown) = spawned(&b, &["--host", "export"]);
    assert_eq!(code, 1, "{shown}");
    assert!(shown.starts_with("unsupported\n"), "{shown}");

    let _s = silent(
        &b.join("DCS.openbeta")
            .join("Logs")
            .join("DcsEval")
            .join("hook"),
    );
    let out = b.join("never.png");
    let kept = out.display().to_string();
    let (code, shown) = spawned(&b, &["--wait-seconds", "0", "--out", &kept]);
    assert_eq!(code, 0, "a pending is an answer: {shown}");
    assert!(shown.starts_with("pending\n"), "{shown}");
    assert!(!out.exists(), "no picture came, so nothing was written");
}

/// The naming rule, held rather than described.
#[test]
fn screenshot_cli_names_every_test_here() {
    let source = include_str!("screenshot_cli.rs");
    let mut lines = source.lines().enumerate();
    while let Some((number, line)) = lines.next() {
        if line.trim() != concat!("#[", "test]") {
            continue;
        }
        let declared = lines
            .by_ref()
            .map(|(_, next)| next.trim_start())
            .find(|next| next.contains("fn "))
            .unwrap_or_else(|| panic!("the test marker on line {} declares nothing", number + 1));
        let name = declared
            .split("fn ")
            .nth(1)
            .and_then(|rest| rest.split(['(', '<']).next())
            .expect("the declaration names the function");
        assert!(
            name.starts_with("screenshot_cli_"),
            "the test marked on line {} is named `{name}`, which the filtered \
             command this file is proved by would not select",
            number + 1
        );
    }
}
