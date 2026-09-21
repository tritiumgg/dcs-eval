//! The installer's verbs on the real binary, spawned as a user runs them.
//!
//! The unit tests call the verbs' function directly, which proves what each
//! verb does and nothing about whether the binary answers to it at all: a
//! word left out of `main`'s dispatch is a usage line and exit 2, and only
//! the executable can show that. So this spawns it, over a fixture
//! `Saved Games` and a data directory of its own, and never the real ones.
//!
//! **Every test function here begins `installer_`.** The done-condition for
//! this work is a `cargo test` filtered on `installer`, and `cargo test` exits
//! 0 when a filter matches nothing at all, so a test named some other way
//! would make that command vacuously green. The unit tests earn the same
//! substring through their module path. `installer_names_every_test_here_installer`
//! reads this file and holds the rule.
//!
//! The crate's shared test scaffolding is compiled into its unit tests only,
//! so the three helpers this needs from it are copied here rather than made
//! public there; the scaffolding's own header makes the same trade.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

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
            std::env::temp_dir().join(format!("dcs-mcp-installer-{}-{n}", std::process::id()));
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

/// A file with known bytes, and whatever directories it needs.
fn put(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("a file has a parent"))
        .expect("the directories are made");
    fs::write(path, bytes).expect("the file is written");
}

/// The published handshake with one header carrying another value.
fn reheader(path: &Path, name: &str, value: &str) {
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

/// Every file under `dir`, as a sorted list of relative path and bytes, with
/// each directory listed too so that one made and left empty is seen.
fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, at: &Path, into: &mut Vec<(String, Vec<u8>)>) {
        let entries = match fs::read_dir(at) {
            Ok(entries) => entries,
            Err(_) => return,
        };
        for entry in entries {
            let entry = entry.expect("the entry reads");
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .expect("under the root")
                .to_string_lossy()
                .into_owned();
            if path.is_dir() {
                into.push((format!("{relative}/"), Vec::new()));
                walk(root, &path, into);
            } else {
                into.push((relative, fs::read(&path).expect("the file reads")));
            }
        }
    }
    let mut found = Vec::new();
    walk(dir, dir, &mut found);
    found.sort();
    found
}

/// One run of the real binary: its exit code, and its stdout with anything on
/// stderr after it, so a failure shows both and a stray stderr line breaks the
/// last-line assertions rather than hiding.
fn spawned(args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_dcs-mcp"))
        .args(args)
        .output()
        .expect("the binary starts");
    let shown = String::from_utf8_lossy(&out.stdout).into_owned();
    let code = out.status.code().expect("the binary exits with a code");
    (
        code,
        format!("{shown}{}", String::from_utf8_lossy(&out.stderr)),
    )
}

fn last(shown: &str) -> &str {
    shown.trim_end().lines().last().unwrap_or("")
}

#[test]
fn installer_the_binary_installs_verifies_and_uninstalls() {
    let b = Sandbox::new();
    let variant = b.join("saved/DCS.openbeta");
    let scripts = variant.join("Scripts");
    put(&scripts.join("Export.lua"), b"-- Tacview\n");
    put(&scripts.join("Hooks/Other.lua"), b"-- other\n");
    let before = snapshot(&scripts);

    let saved = b.join("saved").display().to_string();
    let data = b.join("data").display().to_string();
    let (code, shown) = spawned(&[
        "install",
        "--saved-games",
        &saved,
        "--variant",
        "DCS.openbeta",
        "--data-dir",
        &data,
    ]);
    assert_eq!(code, 0, "{shown}");
    assert!(shown.lines().any(|line| line == "installed"), "{shown}");

    // A session DCS would have published once it loaded the hook: this
    // process's pid, which is certainly running, and the host's own temp
    // directory, which `lfs.tempdir()` inside the game is or lies under
    // (ADR 0029). Kept alive until `verify` has answered; dropped, it takes
    // the session with it.
    let output = fs::canonicalize(&variant)
        .expect("the variant resolves")
        .join("Logs/DcsEval/hook");
    let mut ex = Standin::open(&output, "hook").expect("the stand-in opens");
    ex.pid = std::process::id();
    ex.handshake().expect("the handshake is published");
    reheader(
        &output.join("executor.txt"),
        "lfs_tempdir",
        &std::env::temp_dir().display().to_string(),
    );
    let (code, shown) = spawned(&[
        "verify",
        "--saved-games",
        &saved,
        "--variant",
        "DCS.openbeta",
    ]);
    assert_eq!(code, 0, "{shown}");
    assert_eq!(last(&shown), "verified", "{shown}");
    drop(ex);

    let (code, shown) = spawned(&[
        "uninstall",
        "--saved-games",
        &saved,
        "--variant",
        "DCS.openbeta",
        "--data-dir",
        &data,
    ]);
    assert_eq!(code, 0, "{shown}");
    assert_eq!(last(&shown), "uninstalled", "{shown}");
    assert_eq!(snapshot(&scripts), before, "Scripts is not as it was found");
}

#[test]
fn installer_the_binary_refuses_an_ambiguity_with_exit_one() {
    let b = Sandbox::new();
    for name in ["DCS", "DCS_F4E", "DCS_OH58D"] {
        fs::create_dir_all(b.join("saved").join(name)).expect("the variant is made");
    }
    let before = snapshot(&b.path);
    let saved = b.join("saved").display().to_string();
    let data = b.join("data").display().to_string();
    let (code, shown) = spawned(&["install", "--saved-games", &saved, "--data-dir", &data]);
    assert_eq!(code, 1, "{shown}");
    for name in ["DCS,", "DCS_F4E", "DCS_OH58D", "--variant"] {
        assert!(shown.contains(name), "{name} is not named: {shown}");
    }
    assert_eq!(snapshot(&b.path), before, "the refusal wrote something");
}

/// The naming rule, held rather than described.
#[test]
fn installer_names_every_test_here_installer() {
    let source = include_str!("installer.rs");
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
            name.starts_with("installer_"),
            "the test marked on line {} is named `{name}`, which the filtered \
             command this file is proved by would not select",
            number + 1
        );
    }
}
