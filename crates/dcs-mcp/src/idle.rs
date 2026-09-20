//! What the server costs while nobody is asking it anything: nothing.
//!
//! The specification this is held to says it in one line — "The server's own
//! idle is zero. `rmcp`'s stdio reader blocks on stdin; no tokio timer runs
//! between tool calls." Two halves follow from that, and they are watched by
//! two different instruments here, because a keepalive that touched the
//! executor would not touch the wire and a periodic protocol `ping` would not
//! touch the executor.
//!
//! The wire half is read by counting the bytes the server writes to the
//! transport. The executor half is read off the session directories: the arm
//! file, which a publish makes and the executor removes, and the request
//! directory, which is empty exactly when nothing has been published.
//!
//! What those instruments do not reach is worth writing down, because the
//! sentence above is wider than they are. Three observables are read — the
//! bytes, the arm file, the request directory — and a task that woke on a
//! timer and touched none of the three would go by unseen here. Nothing
//! catches such a task by watching it wake; what stands against it instead is
//! the second test below, which holds the shipped runtime to a builder that
//! starts no IO driver, and the plain fact that this crate's own code spawns
//! nothing.
//!
//! **The sixty seconds are the runtime's own, not the wall clock's.** The
//! test runs under a paused clock, which tokio advances only when it has
//! nothing left to run. So the clock reaching sixty seconds at near-zero real
//! cost is not a shortcut around the measurement — it *is* the measurement: a
//! timer registered anywhere in the server would have been ready, and the
//! clock would have stopped at it instead. A real sixty seconds twice over
//! would be two minutes of CI spent measuring the machine.
//!
//! Two consequences of that pause are worth having written down. A test here
//! that gave a call a non-zero wait would sit in *real* time, because
//! `wait::wait` sleeps on the thread rather than on tokio's clock; every call
//! below therefore waits zero. And the runtime this test runs on is the test
//! harness's, not the one `run` builds — so the shape of that builder is read
//! separately, out of the source, by the second test below.

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use dcs_eval::standin::Standin;
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use tokio::io::{AsyncRead, AsyncWrite, DuplexStream, ReadBuf};

use crate::serve::{Host, Options, Serve};
use crate::testing::Sandbox;

/// The silence the control is about.
const SILENCE: Duration = Duration::from_secs(60);

/// The server's half of the transport, with every byte it writes counted.
///
/// `poll_write_vectored` and `is_write_vectored` are deliberately left at
/// their defaults, which funnel a vectored write back through `poll_write`:
/// an implementation that forwarded them would let bytes past the counter
/// and the instrument would read zero for a writer that was busy.
struct Counted {
    inner: DuplexStream,
    written: Arc<AtomicUsize>,
}

impl AsyncRead for Counted {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for Counted {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let wrote = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = &wrote {
            self.written.fetch_add(*n, Ordering::Relaxed);
        }
        wrote
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// The options, and a stand-in executor published where the server will look.
///
/// `armed` stays false: the handshake's field is not the arm *file*, and the
/// file is what this control reads.
fn fixture(box_: &Sandbox) -> (Options, Standin) {
    let opts = Options {
        saved_games: box_.path.clone(),
        variant: "DCS.openbeta".to_owned(),
        host: Host::Hook,
    };
    let mut ex = Standin::open(&opts.output(), "hook").expect("the stand-in opens");
    // A session whose process really is running, so what a call gets back is
    // the fixture's answer and not whatever else holds that pid on this host.
    ex.pid = std::process::id();
    ex.handshake().expect("the handshake is published");
    ex.beat(std::time::SystemTime::now())
        .expect("the heartbeat is published");
    (opts, ex)
}

/// The `.req` files standing in the session's request directory, by name.
fn requests(ex: &Standin) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(ex.req())
        .expect("the request directory reads")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".req"))
        .collect();
    names.sort();
    names
}

/// Sixty seconds of silence, a call, and sixty seconds more: nothing of the
/// server stirs in either quiet stretch.
///
/// Three beats over one fixture, because each is only meaningful against the
/// others. The call in the middle is the positive control: without it, the
/// two quiet beats would be equally consistent with a fixture in which the
/// arm file and the request directory could never have shown anything at all.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn sixty_seconds_of_silence_wakes_nothing() {
    let box_ = Sandbox::new();
    let (opts, mut ex) = fixture(&box_);

    let written = Arc::new(AtomicUsize::new(0));
    // Roomier than the conversation needs, so a stalled writer cannot be
    // mistaken from the far side for a server that never wrote.
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let server_io = Counted {
        inner: server_io,
        written: Arc::clone(&written),
    };
    // Started together: the client's own `serve` drives `initialize` and
    // cannot finish until the server has answered it.
    let (server, client) = tokio::join!(Serve::new(opts).serve(server_io), ().serve(client_io));
    let server = server.expect("the server side comes up");
    let client = client.expect("the client side comes up");

    // ---- up, and asked nothing --------------------------------------------

    let quiet = written.load(Ordering::Relaxed);
    // Under the paused clock this returns once the runtime has nothing left
    // to run, having moved its own clock the whole sixty seconds. So there is
    // nothing to assert about the time elapsed: it is sixty seconds by
    // construction, and asserting it would read like a measurement while
    // being none. The reading is what the instruments below say about the
    // interval the sleep spanned.
    tokio::time::sleep(SILENCE).await;

    assert_eq!(
        written.load(Ordering::Relaxed),
        quiet,
        "the server wrote to the transport in sixty seconds of silence: \
         {} bytes after the handshake, where the handshake left {quiet}",
        written.load(Ordering::Relaxed)
    );
    assert!(
        !ex.arm().exists(),
        "the executor was armed while no call was in flight: {}",
        ex.arm().display()
    );
    assert_eq!(
        requests(&ex),
        Vec::<String>::new(),
        "a request was published in sixty seconds of silence"
    );
    // ---- one call, so the instruments are known to read at all ------------

    let arguments = match serde_json::json!({ "wait_seconds": 0 }) {
        serde_json::Value::Object(map) => map,
        other => panic!("the arguments are an object: {other}"),
    };
    // Zero, because the body blocks rather than awaits and would otherwise
    // hold the one runtime the client is waiting on.
    client
        .call_tool(CallToolRequestParams::new("dcs_ping").with_arguments(arguments))
        .await
        .expect("dcs_ping is routed and answers");

    let spoken = written.load(Ordering::Relaxed);
    assert!(
        spoken > quiet,
        "a call went by without the server writing anything: still {spoken} bytes"
    );
    assert!(
        ex.arm().exists(),
        "the call published without arming the executor: {}",
        ex.arm().display()
    );
    assert_eq!(
        requests(&ex).len(),
        1,
        "the call published something other than one request: {:?}",
        requests(&ex)
    );

    // ---- the executor answers, and silence again --------------------------

    // The stand-in plays the executor's tick: it takes the request away and
    // leaves the reply standing in `res/`, which is the state any `pending`
    // leaves and not a leak. Removing the arm file is this test playing the
    // executor's disarm — the stand-in does not model it — so the assertion
    // below means "nothing armed it again", which is the whole sentence.
    assert_eq!(ex.tick().len(), 1, "the stand-in answered the one request");
    std::fs::remove_file(ex.arm()).expect("the executor disarms");

    let answered = written.load(Ordering::Relaxed);
    tokio::time::sleep(SILENCE).await;

    assert_eq!(
        written.load(Ordering::Relaxed),
        answered,
        "the server wrote to the transport between two calls: {} bytes after \
         the reply, where the call left {answered}",
        written.load(Ordering::Relaxed)
    );
    assert!(
        !ex.arm().exists(),
        "the executor was armed while no call was in flight: {}",
        ex.arm().display()
    );
    assert_eq!(
        requests(&ex),
        Vec::<String>::new(),
        "a request was published between two calls"
    );

    client.cancel().await.expect("the client hangs up");
    server.cancel().await.expect("the server comes down");
}

/// The runtime the binary actually serves on, read out of its own source.
///
/// The test above runs on the harness's runtime and so cannot see the shape
/// of the one `run` builds, and nothing else in the build watches that line.
/// An IO driver, or the blanket reach that brings one along, is precisely how
/// a thread would come to wake here, so the four builder calls below are held
/// by name.
///
/// Each is spelt as the call — leading dot, trailing parenthesis — and not as
/// a bare word, because the module note beside the builder argues in prose
/// about the very names this refuses, and a test that read prose would be
/// held hostage by the comment explaining it.
#[test]
fn the_shipped_runtime_is_current_thread_with_a_timer_and_no_io_driver() {
    let source = include_str!("serve.rs");
    for wanted in [
        // One thread, and no pool waiting behind it.
        "new_current_thread()",
        // Timers, which rmcp's shutdown needs and this crate's own code does
        // not use.
        ".enable_time()",
    ] {
        assert!(
            source.contains(wanted),
            "the server's runtime no longer calls {wanted}"
        );
    }
    for refused in [
        // The blanket reach, which brings the IO driver along with the timer.
        ".enable_all(",
        // The IO driver itself. Tokio's stdin and stdout are served by the
        // blocking pool, so nothing here needs one.
        ".enable_io(",
    ] {
        assert!(
            !source.contains(refused),
            "the server's runtime now calls {refused}…), which starts a driver \
             the stdio transport does not need"
        );
    }
}
