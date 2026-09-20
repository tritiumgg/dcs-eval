//! The transport control: the real binary, started as a client starts it,
//! with its two streams read apart.
//!
//! Nothing short of a real process proves this. A handler driven in-process
//! shares the harness's own stdout, and a logging layer misrouted there would
//! land in the test runner's output looking harmless. So this spawns the
//! executable the crate builds and asserts on the bytes each stream carried.
//!
//! **Every test function here begins `serve_`.** The done-condition for this
//! work is a `cargo test` filtered on `serve`, and `cargo test` exits 0 when a
//! filter matches nothing at all — so a test named some other way would make
//! that command vacuously green. The unit tests earn the same substring
//! through their module path.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Long enough that a cold, loaded machine is not mistaken for a hang, short
/// enough that a hang is reported by this test rather than by CI's own clock.
const PATIENCE: Duration = Duration::from_secs(20);

/// Read one of the child's pipes to the end on a thread of its own, and hand
/// back what it carried. The thread is never joined: if the child never closes
/// the pipe the test gives up on the channel and kills it, and the thread goes
/// with the process.
fn drain(mut pipe: impl Read + Send + 'static) -> mpsc::Receiver<io::Result<Vec<u8>>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let read = pipe.read_to_end(&mut bytes).map(|_| bytes);
        let _ = tx.send(read);
    });
    rx
}

#[test]
fn serve_writes_only_protocol_frames_to_stdout() {
    // A directory that is deliberately never created. The executor's output
    // directory appears only once DCS has loaded it, so a server that resolved
    // its client at start-up would have to fail here — and this one must come
    // up and serve regardless. Nothing is made, so nothing is cleaned up.
    let never_made = std::env::temp_dir().join(format!("dcs-mcp-serve-{}", std::process::id()));
    assert!(
        !never_made.exists(),
        "the fixture is a path nothing has made: {}",
        never_made.display()
    );

    let mut child = Command::new(env!("CARGO_BIN_EXE_dcs-mcp"))
        .arg("serve")
        .arg("--saved-games")
        .arg(&never_made)
        .arg("--variant")
        .arg("DCS.openbeta")
        .arg("--host")
        .arg("hook")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the server starts");

    {
        let mut stdin = child.stdin.take().expect("the server's stdin is a pipe");
        for line in [
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"dcs-eval-control","version":"0"}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        ] {
            writeln!(stdin, "{line}").expect("the frame is written");
        }
        stdin.flush().expect("the frames are flushed");
        // Dropped here: the server sees EOF and should come down.
    }

    // Reading both pipes to the end is what would hang if the server did not
    // come down, so it happens off this thread, which can give up on it.
    //
    // A thread each, and not one thread reading them in turn: a pipe whose
    // buffer fills blocks the writer, so a server that said more on stderr than
    // the buffer holds would stall there while this side was still waiting for
    // stdout to end. The hang would be reported as the server refusing to come
    // down, which is a different fault entirely.
    let out = drain(child.stdout.take().expect("the server's stdout is a pipe"));
    let err = drain(child.stderr.take().expect("the server's stderr is a pipe"));

    // One deadline over both, so a server that hangs is still reported inside
    // the patience rather than twice it.
    let deadline = Instant::now() + PATIENCE;
    let mut collect = |which: &str, rx: &mpsc::Receiver<io::Result<Vec<u8>>>| {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(Ok(bytes)) => String::from_utf8_lossy(&bytes).into_owned(),
            Ok(Err(why)) => {
                let _ = child.kill();
                panic!("the server's {which} did not read: {why}");
            }
            Err(_) => {
                let _ = child.kill();
                panic!(
                    "the server was still holding {which} open {} s after stdin closed: \
                     a client that hangs up leaves it running",
                    PATIENCE.as_secs()
                );
            }
        }
    };
    let out = collect("stdout", &out);
    let err = collect("stderr", &err);

    // Both pipes reached EOF, so the child is on its way down; how it went
    // down is a second claim. A server that answered correctly and then
    // panicked writes its stack trace to stderr, which is asserted on below
    // only by one substring — so without this the transport would look clean
    // over a process that crashed on the way out.
    let status = child.wait().expect("the server is waited for");
    assert!(
        status.success(),
        "the server answered and then exited {status}; it said this on stderr: {err}"
    );

    assert!(
        !out.trim().is_empty(),
        "the server answered nothing at all on stdout; it said this on stderr: {err}"
    );

    // The property under test. Not "the answer was correct" — a server that
    // negotiated a version this test did not expect, or answered with a
    // JSON-RPC error, still has clean transport. The failure worth catching is
    // a line that is not a frame at all.
    let mut answered = false;
    for line in BufReader::new(out.as_bytes()).lines() {
        let line = line.expect("stdout splits into lines");
        if line.trim().is_empty() {
            continue;
        }
        let frame: serde_json::Value = serde_json::from_str(&line).unwrap_or_else(|why| {
            panic!("stdout carried a line that is not a protocol frame ({why}): {line}")
        });
        assert_eq!(
            frame.get("jsonrpc").and_then(serde_json::Value::as_str),
            Some("2.0"),
            "stdout carried JSON that is not a protocol frame: {line}"
        );
        answered |= frame.get("id").and_then(serde_json::Value::as_u64) == Some(1);
    }
    assert!(
        answered,
        "nothing on stdout answered the initialize that was sent: {out}"
    );

    // And the other half: the diagnostic exists, and it went the other way.
    assert!(
        err.contains("DcsEval"),
        "stderr carries the start-up line naming where the executor will be looked for: {err}"
    );
}
