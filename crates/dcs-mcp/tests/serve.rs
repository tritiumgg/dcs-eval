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

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Long enough that a cold, loaded machine is not mistaken for a hang, short
/// enough that a hang is reported by this test rather than by CI's own clock.
const PATIENCE: Duration = Duration::from_secs(20);

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
    // come down, so it happens on a thread this test can give up on.
    let mut stdout = child.stdout.take().expect("the server's stdout is a pipe");
    let mut stderr = child.stderr.take().expect("the server's stderr is a pipe");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let read = std::io::copy(&mut stdout, &mut out).and_then(|_| {
            std::io::copy(&mut stderr, &mut err)?;
            Ok(())
        });
        let _ = tx.send(read.map(|()| (out, err)));
    });

    let (out, err) = match rx.recv_timeout(PATIENCE) {
        Ok(Ok(streams)) => streams,
        Ok(Err(why)) => {
            let _ = child.kill();
            panic!("the server's streams did not read: {why}");
        }
        Err(_) => {
            let _ = child.kill();
            panic!(
                "the server was still holding its streams open {} s after stdin closed: \
                 a client that hangs up leaves it running",
                PATIENCE.as_secs()
            );
        }
    };
    let _ = child.wait();

    let out = String::from_utf8_lossy(&out).into_owned();
    let err = String::from_utf8_lossy(&err).into_owned();

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
