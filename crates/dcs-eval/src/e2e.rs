//! The round-trip control: the client's own `send` puts a `ping` in front
//! of the shipped `DcsEvalExecutor.lua` while it is ticking under the
//! harness on the reference interpreter, the Lua answers on a frame, and
//! the client reads `pong` back. The interop control plants its requests
//! before one frame and reads what was left; this is the one place a
//! request lands at a moment the executor did not choose, from another
//! process, by rename, with the arm file behind it.
//!
//! The driver half is `tools/harness/executor/e2e.lua`, spawned through the
//! runner with `DCS_EVAL_E2E` set. It ticks until the reply is on the disk
//! or its own deadline passes, and its deadline is shorter than this side's
//! on purpose: when nothing arrives the Lua gives up first, with the
//! executor's counters in its failure line, and that line is what the panic
//! here carries. This side's deadlines are the backstop for an interpreter
//! that hangs, and a hang is red, never a wait without end.
//!
//! The interpreter is `lua5.1.exe` on `PATH`, spelt with its extension:
//! Windows appends `.exe` to a bare name only when the name has no dot, and
//! `lua5.1` has one. It is found on the same `PATH` entry the build gate's
//! own harness run uses one line after `cargo test`, and that entry is
//! there because cargo runs under mise. A missing interpreter is red, never
//! a skip.
//!
//! What reddens it. The control is behavioural, as the interop control is:
//! a byte of the executor's `frame` that changes the wire, and a suite that
//! stops ticking, which this side reports under the deadline rather than
//! hanging.
//!
//! The other half is a round trip that never comes back: a request whose
//! session restarts before any frame takes it. The suite loads a second
//! executor over the same box, as a DCS launched again would, and the
//! client's own `wait` on the first session must read the stamp the
//! handshake now names and answer `superseded`. The first session's
//! process is one that has exited, as the old game's is after a restart,
//! so a wait that stopped reading the stamp answers `dead` instead, and the
//! check goes red at once rather than at its deadline. A second restart
//! keeps the pid, live, as a relaunch handed the old one back would, so the
//! stamps differ in their time alone and a wait that compared pids rather
//! than stamps answers `pending` instead.
//!
//! The request is sent before the restart and waited on after it, never
//! across it. A wait in flight holds a watch on the first session's reply
//! directory, which is exactly what the next session's sweep has to
//! remove; what that costs belongs to the tests of the watch. Here only the
//! verdict is under test.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::protocol::{Envelope, PROTOCOL, parse};
use crate::publish::send;
use crate::readers::Handshake;
use crate::testing::{Sandbox, a_pid_that_has_exited};
use crate::wait::{Outcome, Session, wait};

const SUITE: &str = "executor/e2e";
/// The one id both sides agree on: the suite waits for this reply by name.
const ID: &str = "0000000001-ping";
/// Longer than the suite's own deadline, so the Lua reports first.
const HANDSHAKE_WAIT: Duration = Duration::from_secs(30);
const REPLY_WAIT: Duration = Duration::from_secs(30);
/// After the reply, the suite has only its own checks left to run.
const EXIT_WAIT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(5);
/// The restart is over before the wait starts, so its first look decides.
const VERDICT_WAIT: Duration = Duration::from_secs(5);
/// Two pids, the first session's and the second's, turn the suite into a
/// restart.
const RESTART: &str = "DCS_EVAL_E2E_RESTART";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate sits two below the root")
        .to_path_buf()
}

/// The interpreter, held so that no path out of a test leaves it running:
/// a panic drops this, and the drop kills and reaps it before the box goes.
struct Reaper(Option<Child>);

impl Reaper {
    /// Whether the interpreter has exited, without reaping it.
    fn exited(&mut self) -> bool {
        self.0
            .as_mut()
            .map(|c| c.try_wait().expect("the child's state reads").is_some())
            .unwrap_or(true)
    }

    /// The interpreter's exit status and both streams, once it has exited
    /// or been killed. Its stdout is one line, so the pipes never fill
    /// before this reads them.
    fn finish(&mut self) -> Output {
        let child = self.0.take().expect("the child is finished once");
        child.wait_with_output().expect("the child's output reads")
    }

    /// The same, giving the interpreter `wait` to exit on its own first:
    /// one that is still running after that is killed, so that a suite
    /// which answered and then stuck is red with what it printed, never a
    /// join without end.
    fn finish_within(&mut self, wait: Duration) -> Output {
        let deadline = Instant::now() + wait;
        while !self.exited() && Instant::now() < deadline {
            thread::sleep(POLL);
        }
        self.kill();
        self.finish()
    }

    fn kill(&mut self) {
        if let Some(c) = self.0.as_mut() {
            let _ = c.kill();
        }
    }
}

impl Drop for Reaper {
    fn drop(&mut self) {
        self.kill();
        if let Some(mut c) = self.0.take() {
            let _ = c.wait();
        }
    }
}

/// A live executor: the interpreter, the box it runs over, and what its
/// handshake said. The fields drop in this order, the interpreter first,
/// so the box is removed after nothing holds a file in it.
struct Live {
    lua: Reaper,
    b: Sandbox,
    handshake: Envelope,
    handshake_path: PathBuf,
    req: PathBuf,
    res: PathBuf,
    arm: PathBuf,
}

/// Spawn the suite over a fresh box and wait for its handshake. With two
/// pids the suite restarts the session once the client has sent; with
/// none it ticks until the reply, whatever the shell exported.
fn start(restart: Option<(u32, u32)>) -> Live {
    let b = Sandbox::new();
    let mut command = Command::new("lua5.1.exe");
    command
        .args(["tools/harness.lua", SUITE])
        .current_dir(root())
        .env("DCS_EVAL_E2E", &b.path);
    match restart {
        Some((old, new)) => command.env(RESTART, format!("{old} {new}")),
        None => command.env_remove(RESTART),
    };
    let child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let child = match child {
        Ok(child) => child,
        Err(e) if e.kind() == io::ErrorKind::NotFound => panic!(
            "no lua5.1.exe on PATH: build it with `mise run lua-build`, then run cargo \
             under mise, `mise exec -- cargo test -p dcs-eval e2e`"
        ),
        Err(e) => panic!("lua5.1.exe did not start: {e}"),
    };
    let mut lua = Reaper(Some(child));
    let handshake_path = b
        .path
        .join("Saved Games")
        .join("DCS")
        .join("Logs")
        .join("DcsEval")
        .join("hook")
        .join("executor.txt");
    let bytes = wait_for(&mut lua, &handshake_path, HANDSHAKE_WAIT, "the handshake");
    let handshake = parse(&bytes).expect("the client reads the handshake");
    // The three paths are read out of the handshake, never derived.
    let named = |h: &str| {
        PathBuf::from(
            handshake
                .headers
                .get(h)
                .unwrap_or_else(|| panic!("the handshake names {h}")),
        )
    };
    let (req, res, arm) = (named("req"), named("res"), named("arm"));
    Live {
        lua,
        b,
        handshake,
        handshake_path,
        req,
        res,
        arm,
    }
}

/// The bytes of `path` once it can be read, which for a file published by
/// rename is once it is whole. A read that fails is "not yet": the name is
/// not there, or something holds a file that was renamed a moment ago. The
/// interpreter exiting first is red with its own output, and the deadline
/// passing is red with the phase named, after the interpreter is killed.
fn wait_for(lua: &mut Reaper, path: &Path, wait: Duration, what: &str) -> Vec<u8> {
    let deadline = Instant::now() + wait;
    loop {
        if let Ok(bytes) = fs::read(path) {
            return bytes;
        }
        if lua.exited() {
            let out = lua.finish();
            panic!(
                "the interpreter stopped before {what} was on the disk ({}):\n{}{}",
                out.status,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        if Instant::now() >= deadline {
            lua.kill();
            let out = lua.finish();
            panic!(
                "{what} was not on the disk after {} s, and the interpreter was still running:\n{}{}",
                wait.as_secs(),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        thread::sleep(POLL);
    }
}

#[test]
fn a_ping_the_client_sends_is_answered_by_the_shipped_executor() {
    let mut live = start(None);
    let stamp = live
        .handshake
        .headers
        .get("stamp")
        .expect("the handshake names the stamp")
        .to_owned();
    assert!(
        live.res.starts_with(&live.b.path),
        "res is under the box this test named: {}",
        live.res.display()
    );

    let _ = send(
        &live.req,
        &live.arm,
        ID,
        &[("op", "ping"), ("for", &stamp)],
        b"",
    )
    .expect("the client sends into the live executor's req");
    assert!(live.arm.is_file(), "the client armed the session");

    let bytes = wait_for(
        &mut live.lua,
        &live.res.join(format!("{ID}.res")),
        REPLY_WAIT,
        "the reply",
    );
    let e = parse(&bytes).unwrap_or_else(|why| panic!("the client refuses the reply: {why}"));
    assert_eq!(e.headers.get("status"), Some("ok"));
    assert_eq!(
        e.headers.get("protocol"),
        Some(PROTOCOL.to_string().as_str())
    );
    assert_eq!(e.headers.get("host"), Some("hook"));
    assert_eq!(
        e.headers.get("stamp"),
        Some(stamp.as_str()),
        "the session's stamp"
    );
    assert_eq!(e.headers.get("phase"), Some("menu"));
    assert_eq!(e.headers.get("id"), Some(ID));
    // The request landed at a frame the executor did not choose, so the
    // tick is whatever frame took it: a number, and at least the first.
    let tick: u64 = e
        .headers
        .get("tick")
        .expect("the reply names its tick")
        .parse()
        .expect("the tick is a number");
    assert!(tick >= 1, "answered on a frame that ran: tick {tick}");
    assert_eq!(e.body, b"pong");
    assert!(
        !bytes.contains(&b'\r'),
        "the Lua writes LF alone: these are its bytes, not the stand-in's"
    );

    // The suite ends once the reply is on the disk, and counts its checks.
    let out = live.lua.finish_within(EXIT_WAIT);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stdout.starts_with("e2e: "),
        "the harness did not run the suite to the end ({}):\n{stdout}{stderr}",
        out.status
    );
    // The arm file is still the client's: the executor removes it on its way
    // back to sleep, and this suite ends on the reply, well inside a quiet
    // period. Were the deadline above ever to let the Lua run past one, this
    // would start failing, and the fix would be a suite that ends sooner
    // rather than an assertion that expects less.
    assert!(
        live.arm.is_file(),
        "the arm file is the client's, and the executor had no quiet period in which to remove it"
    );
}

/// A request sent to a first session under `old`, a second session loaded
/// over the same box under `new`, and the client's `wait` on the first,
/// which must answer `superseded`. The two handshakes come back, the
/// first's and the one the verdict re-read, for the caller's own checks.
fn restarted(old: u32, new: u32) -> (Handshake, Handshake) {
    let mut live = start(Some((old, new)));
    let first = Handshake::read(&live.handshake_path)
        .expect("the client reads the first session's handshake");
    assert_eq!(first.pid, old, "the first session runs under the first pid");
    let session = Session::addressed(&first);

    let sent = send(
        &live.req,
        &live.arm,
        ID,
        &[("op", "ping"), ("for", session.stamp())],
        b"",
    )
    .expect("the client sends into the first session's req");
    // Only now may the suite restart: `send` has armed the session and let
    // go of every file in it.
    fs::write(live.b.path.join("restart"), b"").expect("the sentinel is written");

    let out = live.lua.finish_within(EXIT_WAIT);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stdout.starts_with("e2e: "),
        "the harness did not run the suite to the end ({}):\n{stdout}{stderr}",
        out.status
    );

    let got = wait(&session, &sent, VERDICT_WAIT).expect("the wait reads");
    assert!(
        matches!(&got, Outcome::Superseded { id } if id == ID),
        "a request to a session that restarted is superseded, not {got:?}"
    );
    // The handshake the verdict re-read is the one the second load wrote.
    let second = Handshake::read(session.handshake())
        .expect("the client reads the second session's handshake");
    assert_ne!(
        second.stamp, first.stamp,
        "the second session has a stamp of its own"
    );
    assert_eq!(
        second.pid, new,
        "the second session runs under the second pid"
    );
    assert!(
        !live.req.join(format!("{ID}.req")).exists(),
        "the request went with the first session's directory"
    );
    (first, second)
}

#[test]
fn a_request_to_a_session_that_restarted_is_read_as_superseded() {
    // The old game is gone after a restart. The child is held so that its
    // pid is not handed to another process while the test runs.
    let (_gone, old) = a_pid_that_has_exited();
    restarted(old, std::process::id());
}

#[test]
fn a_relaunch_handed_the_old_pid_back_is_read_as_superseded() {
    // Windows may hand a relaunched game the pid the old one had, and then
    // the stamp differs in its time alone. The pid is this process's, so
    // it is live throughout: a wait that compared pids rather than stamps
    // would see nothing changed and answer `pending` at its deadline.
    let pid = std::process::id();
    let (first, second) = restarted(pid, pid);
    let tail = format!("-{pid}");
    assert!(
        first.stamp.ends_with(&tail) && second.stamp.ends_with(&tail),
        "both stamps carry the one pid: {} and {}",
        first.stamp,
        second.stamp
    );
}
