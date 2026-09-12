//! The interop control: the shipped `DcsEvalExecutor.lua`, run under the
//! harness on the reference interpreter, writes a handshake and replies to
//! a directory this test named, and the client's parser reads them. It is
//! the one place the two implementations of the protocol meet: every other
//! control of the parser reads bytes some Rust wrote.
//!
//! The control is behavioural. Nothing here hashes the Lua: what reddens it
//! is a byte of the executor's `frame` that changes the wire, and a byte in
//! a comment leaves it green, which is right, because nothing on the wire
//! moved. The driver half is `tools/harness/executor/interop.lua`, spawned
//! through the runner with `DCS_EVAL_INTEROP` set; it plants its own
//! requests, because a request the client's `send` puts in front of a live
//! Lua is the round-trip control's claim, not this one's.
//!
//! The interpreter is `lua5.1.exe` on `PATH`, spelt with its extension:
//! Windows appends `.exe` to a bare name only when the name has no dot, and
//! `lua5.1` has one. It is found on the same `PATH` entry the build gate's
//! own harness run uses one line after `cargo test`, and that entry is there
//! because cargo runs under mise. A missing interpreter is red, never a skip: the failure
//! names the build step, so a checkout without one cannot read as proven.
//!
//! What is asserted empty. `eval` is declared and not yet served, so the
//! model's `net.dostring_in`, which runs nothing and answers empty, has not
//! reached the wire yet; the empty values the Lua does write are the
//! handshake's body and the ping's `last_callback` and `callbacks`, and the
//! parser must read each as empty, not as absent. When `eval` is served the
//! eval case here flips from `unsupported` to an empty body.
//!
//! A user name with a byte past ASCII puts that byte in every path the
//! handshake names, and the executor refuses its own handshake: this test
//! is red on such a machine by the executor's rule, not by a defect here.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::protocol::{Envelope, PROTOCOL, parse};
use crate::publish::send;
use crate::standin::Standin;
use crate::testing::{Sandbox, entries, slurp};

const SUITE: &str = "executor/interop";
const PING_ID: &str = "0000000001-ping";
const EVAL_ID: &str = "0000000002-eval";

/// The handshake's headers in order. The test's own copy, kept apart from
/// the executor's and the harness's on purpose.
const HANDSHAKE: [&str; 27] = [
    "executor",
    "protocol",
    "host",
    "stamp",
    "pid",
    "started",
    "transport",
    "req",
    "res",
    "arm",
    "output",
    "eval",
    "ops",
    "states",
    "namespace",
    "source",
    "lfs_tempdir",
    "transport_source",
    "install_guard",
    "tick_budget_ms",
    "instruction_budget",
    "instruction_ceiling",
    "probe_every",
    "quiet_s",
    "app_version",
    "max_request_bytes",
    "max_result_bytes",
];

/// The seven headers every reply carries first, and the three a `ping` adds.
const HEAD: [&str; 7] = ["status", "protocol", "host", "stamp", "phase", "id", "tick"];
const PING: [&str; 10] = [
    "status",
    "protocol",
    "host",
    "stamp",
    "phase",
    "id",
    "tick",
    "states",
    "last_callback",
    "callbacks",
];

/// The checkout root: two above this crate's manifest.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate sits two below the root")
        .to_path_buf()
}

/// One run of the suite over a fresh box, and what it left: the handshake,
/// parsed, and the reply directory it names.
struct Run {
    b: Sandbox,
    handshake_bytes: Vec<u8>,
    handshake: Envelope,
    res: PathBuf,
}

fn run() -> Run {
    let b = Sandbox::new();
    let out = Command::new("lua5.1.exe")
        .args(["tools/harness.lua", SUITE])
        .current_dir(root())
        .env("DCS_EVAL_INTEROP", &b.path)
        .output();
    let out = match out {
        Ok(out) => out,
        Err(e) if e.kind() == io::ErrorKind::NotFound => panic!(
            "no lua5.1.exe on PATH: build it with `mise run lua-build`, then run cargo \
             under mise, `mise exec -- cargo test -p dcs-eval interop`"
        ),
        Err(e) => panic!("lua5.1.exe did not start: {e}"),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stdout.starts_with("interop: "),
        "the harness did not run the suite to the end ({}):\n{stdout}{stderr}",
        out.status
    );
    let handshake_bytes = slurp(
        &b.path
            .join("Saved Games")
            .join("DCS")
            .join("Logs")
            .join("DcsEval")
            .join("hook")
            .join("executor.txt"),
    );
    let handshake = parse(&handshake_bytes).expect("the client reads the handshake");
    // The reply directory is read out of the handshake, never derived.
    let res = PathBuf::from(
        handshake
            .headers
            .get("res")
            .expect("the handshake names res"),
    );
    Run {
        b,
        handshake_bytes,
        handshake,
        res,
    }
}

/// The reply to `id`: its bytes as the Lua wrote them, and the envelope
/// the client reads out of them.
fn reply(r: &Run, id: &str) -> (Vec<u8>, Envelope) {
    let bytes = slurp(&r.res.join(format!("{id}.res")));
    let e = parse(&bytes).unwrap_or_else(|why| panic!("{id}: the client refuses the reply: {why}"));
    (bytes, e)
}

/// The header names of `e`, in wire order.
fn names(e: &Envelope) -> Vec<&str> {
    e.headers.iter().map(|(n, _)| n).collect()
}

#[test]
fn the_shipped_executors_handshake_parses() {
    let r = run();
    let h = &r.handshake;
    assert_eq!(names(h), HANDSHAKE, "every field, in the wire's order");
    assert_eq!(h.body, b"", "the handshake has no body");
    assert_eq!(h.headers.get("executor"), Some("dcs-eval"));
    assert_eq!(
        h.headers.get("protocol"),
        Some(PROTOCOL.to_string().as_str())
    );
    assert_eq!(h.headers.get("host"), Some("hook"));
    assert_eq!(h.headers.get("ops"), Some("ping,eval"));
    assert_eq!(
        h.headers.get("transport_source"),
        Some("lfs.tempdir"),
        "the transport came from the model's tempdir, not the fallback"
    );
    assert!(
        r.res.starts_with(&r.b.path),
        "res is under the box this test named: {}",
        r.res.display()
    );
    assert!(
        !r.handshake_bytes.contains(&b'\r'),
        "the Lua writes LF alone: these are its bytes, not the stand-in's"
    );
}

#[test]
fn the_ping_reply_parses_and_an_empty_value_arrives_empty() {
    let r = run();
    let (bytes, e) = reply(&r, PING_ID);
    assert_eq!(names(&e), PING, "the ten headers, in the wire's order");
    assert_eq!(e.headers.get("status"), Some("ok"));
    assert_eq!(e.headers.get("protocol"), Some("2"));
    assert_eq!(e.headers.get("host"), Some("hook"));
    assert_eq!(
        e.headers.get("stamp"),
        r.handshake.headers.get("stamp"),
        "the session's stamp"
    );
    assert_eq!(e.headers.get("phase"), Some("menu"));
    assert_eq!(e.headers.get("id"), Some(PING_ID));
    assert_eq!(
        e.headers.get("tick"),
        Some("1"),
        "answered on the first frame"
    );
    assert_eq!(
        e.headers.get("states"),
        r.handshake.headers.get("states"),
        "the states the handshake declares"
    );
    assert_eq!(
        e.headers.get("last_callback"),
        Some(""),
        "an empty value is empty, not absent"
    );
    assert_eq!(e.headers.get("callbacks"), Some(""), "and so is the second");
    assert_eq!(e.body, b"pong");
    assert!(
        bytes.ends_with(b"callbacks: \n\npong"),
        "the wire's spelling of an empty value is the name, a colon, a blank and the line's end"
    );
}

#[test]
fn the_eval_reply_parses_as_unsupported_until_it_is_served() {
    let r = run();
    let (_, e) = reply(&r, EVAL_ID);
    assert_eq!(
        names(&e),
        HEAD,
        "a refusal carries the seven headers and no other"
    );
    assert_eq!(e.headers.get("status"), Some("unsupported"));
    assert_eq!(e.headers.get("id"), Some(EVAL_ID));
    assert_eq!(e.headers.get("tick"), Some("1"));
    assert_eq!(
        e.body,
        b"eval is declared and not yet served by this executor"
    );
    assert_eq!(
        entries(&r.res),
        format!("{PING_ID}.res {EVAL_ID}.res"),
        "both replies under their final names, no .tmp"
    );
}

/// The stand-in's reply to `id`: its bytes and the envelope read from them.
fn stood_in(s: &Standin, id: &str) -> (Vec<u8>, Envelope) {
    let bytes = slurp(&s.res().join(format!("{id}.res")));
    let e =
        parse(&bytes).unwrap_or_else(|why| panic!("{id}: the client refuses the stand-in: {why}"));
    (bytes, e)
}

/// The stand-in against the executor on one reply: the same names in the
/// same order, every value the same but the session's stamp, the same
/// body, and bytes that differ, because the stand-in's dialect is not the
/// Lua's and the parser read both as one envelope.
fn agree(id: &str, lua: &(Vec<u8>, Envelope), stand_in: &(Vec<u8>, Envelope)) {
    assert_eq!(
        names(&stand_in.1),
        names(&lua.1),
        "{id}: the same headers in the same order"
    );
    for (name, value) in lua.1.headers.iter() {
        if name != "stamp" {
            assert_eq!(stand_in.1.headers.get(name), Some(value), "{id}: {name}");
        }
    }
    assert_eq!(stand_in.1.body, lua.1.body, "{id}: the same body");
    assert_ne!(
        stand_in.0, lua.0,
        "{id}: two dialects, not one encoder read twice"
    );
}

#[test]
fn the_stand_in_answers_as_the_shipped_executor_does() {
    let r = run();
    let mut s = Standin::open(&r.b.join("standin"), "hook").expect("the stand-in opens");
    let stamp = s.stamp.clone();
    // The same two requests the Lua suite plants, answered on one tick as
    // the Lua answered them on one frame, so the ticks agree too.
    send(
        s.req(),
        s.arm(),
        PING_ID,
        &[("op", "ping"), ("for", &stamp)],
        b"",
    )
    .expect("the ping sends");
    send(
        s.req(),
        s.arm(),
        EVAL_ID,
        &[("op", "eval"), ("for", &stamp)],
        b"return 1",
    )
    .expect("the eval sends");
    s.tick();
    agree(PING_ID, &reply(&r, PING_ID), &stood_in(&s, PING_ID));
    agree(EVAL_ID, &reply(&r, EVAL_ID), &stood_in(&s, EVAL_ID));
}
