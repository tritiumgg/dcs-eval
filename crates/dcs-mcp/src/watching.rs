//! What the server leaves behind on the executor's session directories:
//! nothing, once a call has returned.
//!
//! The rule is the executor's and not this crate's. At load the executor
//! removes every sibling session directory under its transport root, and on
//! Windows a directory somebody holds a handle on refuses its own removal —
//! so a client that kept a watch open between two calls, or opened one on a
//! session it had already reported superseded, would leave a directory the
//! next launch could never clear. The whole mechanism is in `dcs-eval`, and
//! this file makes no claim about how it is written there. It watches the
//! consequence from outside, through the only thing a user ever drives: the
//! server's public path.
//!
//! The tools are not built yet, so a "call" here is written out: the three
//! steps every one of them will make, in `call` below — resolve the client
//! afresh, publish a request, wait for the answer. When the tools land this
//! helper is what they replace, and the assertions do not move.
//!
//! Handle lifetime is never asserted directly. `dcs-eval`'s watch is private
//! to that crate and the count of handles it opened is not visible from here,
//! which is as it should be: what the rule is *for* is the next executor
//! session's sweep, so the sweep is what these tests read. `Standin::sweep`
//! is that sweep, and a test in which it succeeds while nothing at all was
//! ever opened would prove nothing — which is why the first test below is a
//! refusal rather than a success.

use std::time::Duration;

use dcs_eval::standin::Standin;
use dcs_eval::wait::{self, Outcome};
use dcs_eval::{publish, readers::Handshake};

use crate::serve::{Host, Options, Serve};
use crate::testing::Sandbox;

const ID_ONE: &str = "0000000001-abcd";
const ID_TWO: &str = "0000000002-abcd";

/// The second session's stamp. Named rather than minted, because two loads
/// inside one second would otherwise mint the same directory name.
const LATER: &str = "2000000000-7";

fn opts(box_: &Sandbox) -> Options {
    Options {
        saved_games: box_.path.clone(),
        variant: "DCS.openbeta".to_owned(),
        host: Host::Hook,
    }
}

/// A session a client will find alive: its handshake published, its process
/// id one that really is running, and a heartbeat just written.
///
/// The pid matters. A wait that finds a dormant session probes the process
/// the handshake named, and the stand-in's default is a number that is
/// nobody in particular — so a session left at the default would be answered
/// `dead` or `pending` depending on what else happens to be running on the
/// host. This process is certainly alive, which makes the outcome the
/// fixture's and not the machine's.
fn ticking(s: &mut Standin) {
    s.pid = std::process::id();
    s.armed = true;
    s.handshake().expect("the handshake publishes");
    s.beat(std::time::SystemTime::now())
        .expect("the heartbeat publishes");
}

/// One tool call, as every tool will make it: the client resolved afresh,
/// a request published into the session it named, and the wait for an answer.
fn call(serve: &Serve, id: &str, upto: Duration) -> Outcome {
    let client = serve.client().expect("the call finds a session");
    let sent = ping(client.handshake(), id);
    wait::wait(client.session(), &sent, upto).expect("the wait reads")
}

/// A `ping` published into the session `h` describes, fenced with that
/// session's own stamp the way every request is.
fn ping(h: &Handshake, id: &str) -> publish::Sent {
    publish::send(
        h.req.as_path(),
        h.arm.as_path(),
        id,
        &[("op", "ping"), ("for", h.stamp.as_str())],
        b"",
    )
    .expect("the request publishes")
}

/// The positive control, and the half of the rule that says a watch is held
/// *while* a wait is in flight.
///
/// Without this, every sweep that succeeds below would be equally consistent
/// with no handle ever having been opened at all, and the whole file would
/// prove nothing. Nothing here counts handles: the sweep being refused is the
/// observation, and it is the same refusal the executor reads at load.
#[test]
fn a_sweep_is_refused_while_a_wait_is_in_flight_and_succeeds_once_it_returns() {
    let box_ = Sandbox::new();
    let opts = opts(&box_);
    let mut a = Standin::open(&opts.output(), "hook").expect("the first session loads");
    ticking(&mut a);
    let h = Handshake::read(&a.output().join("executor.txt")).expect("the handshake reads");
    let session = dcs_eval::wait::Session::addressed(&h);
    let sent = ping(&h, ID_ONE);
    let a_stamp = a.stamp.clone();

    std::thread::scope(|scope| {
        // Only the session and the sent request cross over. The stand-ins
        // stay here, which is what lets this thread go on playing the
        // executor's side while the client waits.
        let waiting = scope.spawn(|| wait::wait(&session, &sent, Duration::from_secs(5)));

        // Long enough for the wait to have gone round its 25 ms poll several
        // times, so the watch is certainly open by now. If it were not, the
        // sweep below would remove the directory out from under a live wait
        // and this test would fail loudly rather than pass for the wrong
        // reason.
        std::thread::sleep(Duration::from_millis(250));

        // DCS loads again. The new session's directories are made first; its
        // handshake is deliberately not published yet, so the client waiting
        // above still believes session A is the one it is talking to and is
        // still holding its watch.
        let mut b =
            Standin::open_stamped(&opts.output(), "hook", LATER).expect("the second session loads");
        let blocked = b.sweep();
        assert!(
            blocked.left.iter().any(|(name, _)| *name == a_stamp),
            "a session with a wait in flight was swept away: removed {:?}, left {:?}",
            blocked.removed,
            blocked.left
        );
        assert!(
            a.res().is_dir(),
            "{} went while a wait was still reading it",
            a.res().display()
        );

        // Now the load finishes: the handshake names B, and the waiting call
        // learns its session was superseded. The stamp is what decides that,
        // ahead of any probe of the process — which is why this is an answer
        // about the reload and not about what is running on the host.
        ticking(&mut b);
        let got = waiting.join().expect("the waiting thread does not panic");
        assert!(
            matches!(got.expect("the wait reads"), Outcome::Superseded { .. }),
            "the wait was supposed to learn the session had been replaced"
        );

        // And the next load's sweep gets what the first one could not.
        let after = b.sweep();
        assert!(
            after.left.is_empty(),
            "a handle outlived the call it was opened for: {:?}",
            after.left
        );
        assert!(
            !a.session().exists(),
            "{} survived the sweep that followed the call",
            a.session().display()
        );
    });
}

/// No handle is held between two tool calls: the server is idle, and the
/// next executor session's sweep goes through.
#[test]
fn a_sibling_sweep_succeeds_while_the_server_is_idle_between_two_calls() {
    let box_ = Sandbox::new();
    let opts = opts(&box_);
    let serve = Serve::new(opts.clone());
    let mut a = Standin::open(&opts.output(), "hook").expect("the first session loads");
    ticking(&mut a);
    let a_stamp = a.stamp.clone();

    // A call that really slept, and so really opened a watch — nothing ever
    // answers this request, so the wait runs out its deadline.
    let first = call(&serve, ID_ONE, Duration::from_millis(150));
    assert!(
        matches!(first, Outcome::Pending { .. }),
        "the first call was supposed to wait and come back empty-handed: {first:?}"
    );

    // The server is now idle. DCS reloads, and the new session sweeps.
    let mut b =
        Standin::open_stamped(&opts.output(), "hook", LATER).expect("the second session loads");
    let swept = b.sweep();
    assert!(
        swept.left.is_empty(),
        "the next session could not remove a directory the last call had finished with: {:?}",
        swept.left
    );
    assert_eq!(swept.removed, vec![a_stamp]);
    assert!(
        !a.session().exists(),
        "{} outlived the sweep",
        a.session().display()
    );

    // And the second call addresses the session that swept, which is what
    // makes the sentence true of both calls rather than only of the first.
    ticking(&mut b);
    let second = call(&serve, ID_TWO, Duration::from_millis(150));
    assert!(
        matches!(second, Outcome::Pending { .. }),
        "the second call was supposed to reach the reloaded session: {second:?}"
    );
    assert_eq!(
        serve
            .client()
            .expect("the session resolves")
            .session()
            .stamp(),
        LATER
    );
}

/// A session a call reported `superseded` is one no watch was ever opened
/// on, so the sweep that follows finds nothing in its way.
///
/// This is the terminal path, and it never sleeps: the first look answers,
/// and a watch opened before that look rather than inside the sleep would be
/// left on a directory the caller has just been told is gone.
#[test]
fn a_superseded_session_is_waited_on_and_swept_straight_after() {
    let box_ = Sandbox::new();
    let opts = opts(&box_);
    let serve = Serve::new(opts.clone());
    let mut a = Standin::open(&opts.output(), "hook").expect("the first session loads");
    ticking(&mut a);

    // The call is addressed at A, as a tool call would be.
    let client = serve.client().expect("the call finds session A");
    let sent = ping(client.handshake(), ID_ONE);

    // DCS reloads before the wait begins: A's directories are still standing,
    // but the handshake names B.
    let mut b =
        Standin::open_stamped(&opts.output(), "hook", LATER).expect("the second session loads");
    ticking(&mut b);

    let got = wait::wait(client.session(), &sent, Duration::from_secs(5)).expect("the wait reads");
    assert!(
        matches!(got, Outcome::Superseded { .. }),
        "the call was supposed to report the session replaced: {got:?}"
    );

    let swept = b.sweep();
    assert!(
        swept.left.is_empty(),
        "a session this call reported superseded was still held when the next session swept: {:?}",
        swept.left
    );
    assert!(
        !a.session().exists(),
        "{} survived the sweep",
        a.session().display()
    );
}
