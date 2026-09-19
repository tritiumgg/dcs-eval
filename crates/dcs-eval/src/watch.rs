//! How a wait sleeps: on an event where the directory will report one,
//! and on the clock regardless.
//!
//! Nothing here is unsafe and nothing here decides an outcome. The table
//! `wait` reads is untouched by any of it; what changes is only how long
//! the loop is asleep between two looks at the reply directory.
//!
//! The poll is not a branch taken when the watch errs. There is no
//! runtime detection of a broken watch anywhere in this module, and that
//! is the point: the specification keeps the poll precisely because a
//! watch that reports nothing does so silently, so a design that only
//! polled once it had noticed would never notice. The sleep is
//! `min(left, poll)` on every path, the event merely ends it early, and
//! the loop lists the directory afterwards whichever of the three things
//! ended it. Decision record 0016 holds the argument.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use crate::sys::{Changes, Woke};

/// How a wait paces itself. `wait` takes the default; a test takes a
/// pace that makes one of the two mechanisms observable on its own.
#[derive(Debug, Clone)]
pub(crate) struct Pace {
    /// The longest one sleep may be. The fallback the specification
    /// requires for a watch that reports nothing, and the term an
    /// event-driven wait is supposed to remove from the round trip.
    pub(crate) poll: Duration,
    /// Whether a watch may be opened at all. A wait with this off is a
    /// wait on a filesystem whose watch says nothing, which is the case
    /// the poll exists for and which no fixture could otherwise produce.
    pub(crate) open: bool,
}

impl Default for Pace {
    fn default() -> Self {
        Self {
            poll: Duration::from_millis(25),
            open: true,
        }
    }
}

/// What one wait did, counted rather than timed.
///
/// A wake and a poll are indistinguishable by how long they took on a
/// loaded machine, so nothing here is a clock reading. `polls == 0` over
/// a wait that returned a reply is what says the reply was woken on.
#[derive(Debug, Default)]
pub(crate) struct Tally {
    /// Watches successfully opened.
    pub(crate) opens: u32,
    /// An open, an arm or a wait that failed, or a sleep on a watch with
    /// no read outstanding. Any of them ends the watch for the rest of
    /// this wait: a filesystem that has just refused will refuse again,
    /// and retrying it every 25 ms is a syscall storm for an answer that
    /// will not change.
    pub(crate) deaf: u32,
    /// Reads successfully armed.
    pub(crate) arms: u32,
    /// Sleeps the directory ended. **Not a count of anything on the
    /// disk:** a reply is published as a temporary file, written, and
    /// renamed over, so one reply is at least two notifications.
    pub(crate) events: u32,
    /// Sleeps that ended without the directory saying anything — a
    /// timed-out event wait and a plain sleep alike.
    pub(crate) polls: u32,
    /// The drain's answer, shared with whatever `Changes` this wait
    /// opened. It is a shared cell rather than a copied-out value
    /// because the cancellation runs in that value's `Drop`, which has
    /// nowhere to return to.
    sink: Option<Rc<Cell<Option<i32>>>>,
}

impl Tally {
    /// What the cancellation recorded, or `None` where no drain has run.
    ///
    /// Compiled for the tests, which are the only readers there are: the
    /// sink exists so that a drain running in a `Drop` leaves a trace,
    /// and reading the trace is the whole of what anything does with it.
    #[cfg(test)]
    pub(crate) fn drained(&self) -> Option<i32> {
        self.sink.as_ref().and_then(|cell| cell.get())
    }
}

/// Arm the watch, where there is one and it has no read outstanding.
///
/// The loop calls this before it looks at the directory, so that a change
/// landing after the arm signals and one landing before it is seen by the
/// look. On the first pass there is no watch yet and the look is the whole
/// of it.
pub(crate) fn arm_if_needed(changes: &mut Option<Changes>, tally: &mut Tally) {
    let Some(observer) = changes.as_mut() else {
        return;
    };
    if observer.armed() {
        return;
    }
    match observer.arm() {
        Ok(()) => tally.arms += 1,
        Err(_) => {
            tally.deaf += 1;
            *changes = None;
        }
    }
}

/// Sleep until the directory says something, until `min(left, poll)` is
/// spent, or until the deadline — whichever comes first.
///
/// The watch is opened here rather than before the loop, and that is what
/// keeps a terminal first look from ever touching the directory: a wait
/// that answers `superseded` or `dead` on its first pass never sleeps, so
/// it never opens a handle on a session it has just reported gone.
pub(crate) fn settle(
    changes: &mut Option<Changes>,
    res: &Path,
    left: Duration,
    pace: &Pace,
    tally: &mut Tally,
) {
    // One expression, shared by both paths, and the whole of the poll.
    let nap = left.min(pace.poll);

    if changes.is_none() && pace.open && tally.deaf == 0 {
        match Changes::open(res) {
            Ok(observer) => {
                tally.opens += 1;
                tally.sink = Some(observer.sink());
                *changes = Some(observer);
                arm_if_needed(changes, tally);
            }
            Err(_) => tally.deaf += 1,
        }
    }

    match changes.as_mut() {
        Some(observer) => match observer.woke(nap) {
            Woke::Event => tally.events += 1,
            Woke::Timeout => tally.polls += 1,
            Woke::Deaf => {
                tally.deaf += 1;
                tally.polls += 1;
                *changes = None;
            }
        },
        None => {
            std::thread::sleep(nap);
            tally.polls += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Instant, SystemTime};

    use crate::publish::Sent;
    use crate::readers::Handshake;
    use crate::standin::Standin;
    use crate::sys;
    use crate::testing::{Sandbox, a_pid_that_has_exited};
    use crate::wait::{Flag, Outcome, Session, wait_paced};

    const ID: &str = "0000000001-abcd";

    /// A poll no wait in this suite could ever reach. A wake is told from
    /// a poll by the tally and never by a stopwatch: on a loaded machine
    /// the two take the same time often enough to make a timing
    /// assertion a coin toss. With the poll set past any deadline here,
    /// a sleep that ended at all ended because the directory said
    /// something.
    const NEVER: Duration = Duration::from_secs(3600);

    fn woken() -> Pace {
        Pace {
            poll: NEVER,
            open: true,
        }
    }

    /// The case the specification keeps the poll for: a filesystem whose
    /// watch reports nothing, which no fixture can manufacture and which
    /// this seam stands in for.
    fn unwatched() -> Pace {
        Pace {
            poll: Duration::from_millis(25),
            open: false,
        }
    }

    fn standin(b: &Sandbox) -> Standin {
        Standin::open(&b.join("dcs"), "hook").expect("the stand-in opens")
    }

    fn address(s: &Standin) -> Session {
        s.handshake().expect("the handshake publishes");
        let h = Handshake::read(&s.output().join("executor.txt")).expect("the handshake reads");
        Session::addressed(&h)
    }

    /// A session that is alive and ticking, so the table says `pending`
    /// and the wait is about the reply directory and nothing else.
    fn ticking(b: &Sandbox) -> (Standin, Session) {
        let mut s = standin(b);
        s.pid = std::process::id();
        s.armed = true;
        let session = address(&s);
        s.beat(SystemTime::now()).expect("a fresh beat");
        (s, session)
    }

    fn just_sent() -> Sent {
        Sent::at(ID, Instant::now())
    }

    fn pending_of(got: &Outcome) -> (&str, Option<Flag>) {
        let Outcome::Pending { phase, flag, .. } = got else {
            panic!("a pending, not {got:?}");
        };
        (phase.as_str(), *flag)
    }

    /// The reply published from another thread after `after`, so the wait
    /// is already asleep when it lands. A reply written at nothing would
    /// be found by the first look and would prove nothing about a watch.
    fn wait_while<R>(after: Duration, publish: impl FnOnce() + Send, run: impl FnOnce() -> R) -> R {
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(after);
                publish();
            });
            run()
        })
    }

    #[test]
    fn a_reply_wakes_the_wait_rather_than_being_polled_for() {
        let b = Sandbox::new();
        let (s, session) = ticking(&b);
        let sent = just_sent();
        let mut tally = Tally::default();
        let got = wait_while(
            Duration::from_millis(150),
            || {
                s.reply(ID, "ok", &[("op", "ping")], b"pong")
                    .expect("the reply publishes");
            },
            || {
                wait_paced(
                    &session,
                    &sent,
                    Duration::from_secs(5),
                    &woken(),
                    &mut tally,
                )
                .expect("the wait reads")
            },
        );
        let Outcome::Reply(envelope) = got else {
            panic!("the reply, not {got:?}");
        };
        assert_eq!(envelope.body, b"pong");
        assert_eq!(
            tally.polls, 0,
            "the poll is an hour away, so nothing here was polled for: {tally:?}"
        );
        assert!(
            tally.events >= 1,
            "and the directory is what ended the sleep: {tally:?}"
        );
    }

    #[test]
    fn the_wake_is_one_event_and_not_a_signal_left_standing() {
        // An exact count is not available: a reply is published as a
        // temporary file, written, and renamed over it, so one reply is
        // several notifications. The bound separates a handful of them
        // from an event that stayed signalled and spun the loop for the
        // whole deadline.
        let b = Sandbox::new();
        let (s, session) = ticking(&b);
        let sent = just_sent();
        let mut tally = Tally::default();
        let got = wait_while(
            Duration::from_millis(150),
            || {
                s.reply(ID, "ok", &[], b"pong")
                    .expect("the reply publishes");
            },
            || {
                wait_paced(
                    &session,
                    &sent,
                    Duration::from_secs(5),
                    &woken(),
                    &mut tally,
                )
                .expect("the wait reads")
            },
        );
        assert!(matches!(got, Outcome::Reply(_)), "{got:?}");
        assert!(
            tally.events <= 8,
            "events: {}, wanted at most 8 — the event stayed signalled and the loop spun on it",
            tally.events
        );
    }

    #[test]
    fn a_reply_already_on_the_disk_is_found_before_any_sleeping() {
        // Not about the arm: it is that the first look precedes the first
        // sleep, which is what the wait did before a watch existed and
        // must survive the rewiring.
        let b = Sandbox::new();
        let (s, session) = ticking(&b);
        s.reply(ID, "ok", &[], b"pong")
            .expect("the reply publishes");
        let mut tally = Tally::default();
        let got = wait_paced(
            &session,
            &just_sent(),
            Duration::from_secs(5),
            &woken(),
            &mut tally,
        )
        .expect("the wait reads");
        assert!(matches!(got, Outcome::Reply(_)), "{got:?}");
        assert_eq!(
            (tally.opens, tally.polls, tally.events),
            (0, 0, 0),
            "it never slept, so it never opened anything: {tally:?}"
        );
    }

    #[test]
    fn a_watch_that_reports_nothing_is_still_answered_by_the_poll() {
        let b = Sandbox::new();
        let (s, session) = ticking(&b);
        let sent = just_sent();
        let mut tally = Tally::default();
        let got = wait_while(
            Duration::from_millis(150),
            || {
                s.reply(ID, "ok", &[], b"a reply the watch never mentioned")
                    .expect("the reply publishes");
            },
            || {
                wait_paced(
                    &session,
                    &sent,
                    Duration::from_secs(5),
                    &unwatched(),
                    &mut tally,
                )
                .expect("the wait reads")
            },
        );
        let Outcome::Reply(envelope) = got else {
            panic!("a reply, not {got:?}");
        };
        assert_eq!(envelope.body, b"a reply the watch never mentioned");
        assert_eq!(tally.events, 0, "nothing reported anything: {tally:?}");
        // The count, not merely one: the reply lands 150 ms into a
        // five-second deadline, so a sleep bounded by the poll looks
        // about six times before it finds it and a sleep bounded by the
        // deadline looks once. One look would be satisfied by a wait
        // with no poll in it at all, which is the thing being checked.
        assert!(
            tally.polls >= 4,
            "polls: {}, wanted at least 4 — the sleep ran to the deadline rather than to the \
             25 ms poll, and the reply was found on the way out",
            tally.polls
        );
    }

    #[test]
    fn a_watch_that_will_not_open_leaves_the_wait_polling() {
        // The reply directory is gone, so `CreateFileW` refuses. A wait
        // whose watch will not open is still a wait: it polls, it reads
        // the table, and it hands back what the table said.
        let b = Sandbox::new();
        let (s, session) = ticking(&b);
        fs::remove_dir_all(s.res()).expect("the reply directory goes");
        let mut tally = Tally::default();
        let got = wait_paced(
            &session,
            &just_sent(),
            Duration::from_millis(120),
            &woken(),
            &mut tally,
        )
        .expect("a watch that will not open is not a failure of the wait");
        assert_eq!(pending_of(&got), ("menu", None), "{got:?}");
        assert_eq!(tally.opens, 0, "nothing opened: {tally:?}");
        assert_eq!(
            tally.deaf, 1,
            "and a filesystem that refused once is not asked again: {tally:?}"
        );
        assert!(tally.polls >= 1, "the wait went on polling: {tally:?}");
    }

    #[test]
    fn a_wait_that_timed_out_cancels_the_read_it_left_pending() {
        // The case the cancellation exists for: at the deadline a read is
        // certainly still outstanding, and the kernel holds a pointer
        // into the buffer about to be freed.
        //
        // This is not proof that no use-after-free is possible. Deleting
        // the cancellation and the drain and keeping the handle close
        // leaves every check about a held handle green, because closing
        // the handle does release the directory. What it proves is that
        // the cancellation ran and completed, which is the only thing
        // about it a test can see.
        let b = Sandbox::new();
        let (_s, session) = ticking(&b);
        let mut tally = Tally::default();
        let got = wait_paced(
            &session,
            &just_sent(),
            Duration::from_millis(120),
            &Pace::default(),
            &mut tally,
        )
        .expect("running out of time is not a failure");
        assert!(matches!(got, Outcome::Pending { .. }), "{got:?}");
        assert_eq!(tally.opens, 1, "a watch was opened: {tally:?}");
        assert_eq!(
            tally.drained(),
            Some(sys::ERROR_OPERATION_ABORTED),
            "the wait left a read pending and recorded no cancellation: drained: {:?}",
            tally.drained()
        );
    }

    #[test]
    fn a_wait_that_returned_holds_no_handle_on_the_reply_directory() {
        // A directory with a handle on it cannot be removed, so removing
        // it is how "no handle is held" is observed rather than asserted.
        // The control below is what says this is a real question on this
        // machine.
        let b = Sandbox::new();
        let (s, session) = ticking(&b);
        let mut tally = Tally::default();
        let got = wait_paced(
            &session,
            &just_sent(),
            Duration::from_millis(120),
            &Pace::default(),
            &mut tally,
        )
        .expect("the wait reads");
        assert!(matches!(got, Outcome::Pending { .. }), "{got:?}");
        assert_eq!(tally.opens, 1, "there was a handle to hold: {tally:?}");
        fs::remove_dir(s.res()).unwrap_or_else(|why| {
            panic!(
                "{} could not be removed once the wait returned: {why}",
                s.res().display()
            )
        });
    }

    #[test]
    fn a_held_handle_really_does_refuse_the_removal() {
        // The positive control, and the first thing to believe or
        // disbelieve in this file. If a directory this crate is watching
        // can be removed anyway, every check above about a handle not
        // being held proves nothing at all.
        let b = Sandbox::new();
        let (s, _session) = ticking(&b);
        let observer = sys::Changes::open(s.res()).expect("the directory opens");
        let why = fs::remove_dir(s.res()).expect_err("a watched directory does not go");
        assert_eq!(
            why.raw_os_error(),
            Some(32),
            "{} was removed while it was being watched, so nothing here is proved: {why}",
            s.res().display()
        );
        drop(observer);
        fs::remove_dir(s.res()).expect("and once the watch is gone it goes");
    }

    #[test]
    fn a_superseded_session_is_answered_without_the_directory_being_opened() {
        let b = Sandbox::new();
        let (mut s, session) = ticking(&b);
        s.stamp = format!("{}-restarted", s.stamp);
        s.handshake().expect("the new session's handshake");
        let mut tally = Tally::default();
        let got = wait_paced(
            &session,
            &just_sent(),
            Duration::from_secs(5),
            &Pace::default(),
            &mut tally,
        )
        .expect("the wait reads");
        assert!(matches!(got, Outcome::Superseded { .. }), "{got:?}");
        assert_eq!(
            tally.opens, 0,
            "opens: {}, wanted 0 — a session this wait reported superseded was watched",
            tally.opens
        );
        fs::remove_dir_all(s.res()).expect("and the next session could sweep it");
    }

    #[test]
    fn a_dead_session_is_answered_without_the_directory_being_opened() {
        let b = Sandbox::new();
        let mut s = standin(&b);
        let (_child, pid) = a_pid_that_has_exited();
        s.pid = pid;
        s.armed = true;
        let session = address(&s);
        s.beat(SystemTime::now() - Duration::from_secs(3600))
            .expect("a stale beat");
        let mut tally = Tally::default();
        let got = wait_paced(
            &session,
            &just_sent(),
            Duration::from_secs(5),
            &Pace::default(),
            &mut tally,
        )
        .expect("the wait reads");
        assert!(matches!(got, Outcome::Dead { .. }), "{got:?}");
        assert_eq!(
            tally.opens, 0,
            "opens: {}, wanted 0 — a session this wait reported dead was watched",
            tally.opens
        );
    }

    #[test]
    fn a_deadline_already_spent_opens_nothing() {
        let b = Sandbox::new();
        let (_s, session) = ticking(&b);
        let mut tally = Tally::default();
        let got = wait_paced(
            &session,
            &just_sent(),
            Duration::ZERO,
            &Pace::default(),
            &mut tally,
        )
        .expect("a wait of no time is still an outcome");
        assert!(matches!(got, Outcome::Pending { .. }), "{got:?}");
        assert_eq!(
            (tally.opens, tally.polls),
            (0, 0),
            "opens: {}, wanted 0 — a wait with no time to sleep opened a handle anyway",
            tally.opens
        );
    }

    #[test]
    fn a_foreign_reply_wakes_the_wait_and_the_wait_goes_on() {
        // Two replies land under one id, and the first is somebody else's
        // session. Waking on it costs the wait its one completion, so the
        // reply that follows is only ever seen by a wait that armed a
        // second read; with the poll an hour away there is nothing else
        // that could have found it.
        let b = Sandbox::new();
        let (s, session) = ticking(&b);
        let mine = s.stamp.clone();
        let sent = just_sent();
        let mut tally = Tally::default();
        let got = std::thread::scope(|scope| {
            scope.spawn(|| {
                let mut s = standin(&b);
                s.stamp = format!("{mine}-restarted");
                std::thread::sleep(Duration::from_millis(150));
                s.reply(ID, "ok", &[], b"not yours")
                    .expect("the foreign reply publishes");
                s.stamp = mine.clone();
                std::thread::sleep(Duration::from_millis(250));
                s.reply(ID, "ok", &[], b"pong").expect("and then mine");
            });
            wait_paced(
                &session,
                &sent,
                Duration::from_secs(5),
                &woken(),
                &mut tally,
            )
            .expect("a foreign reply is not a refusal")
        });
        let Outcome::Reply(envelope) = got else {
            panic!("the second reply, not {got:?}");
        };
        assert_eq!(envelope.body, b"pong");
        assert_eq!(
            tally.polls, 0,
            "the wait consumed its one event on a foreign reply and never re-armed: \
             events: {}, arms: {}, and the reply that followed was not woken on",
            tally.events, tally.arms
        );
        assert!(
            tally.events >= 2 && tally.arms >= 1,
            "both replies were woken on: {tally:?}"
        );
    }

    #[test]
    fn the_outcome_table_says_the_same_thing_woken_as_polled() {
        // The watch changes how the loop sleeps and nothing else. The
        // same session, waited on both ways, reads the same row.
        let b = Sandbox::new();
        let (_s, session) = ticking(&b);
        let mut watched = Tally::default();
        let mut polled = Tally::default();
        let a = wait_paced(
            &session,
            &just_sent(),
            Duration::from_millis(120),
            &Pace::default(),
            &mut watched,
        )
        .expect("the watched wait reads");
        let z = wait_paced(
            &session,
            &just_sent(),
            Duration::from_millis(120),
            &unwatched(),
            &mut polled,
        )
        .expect("the polled wait reads");
        assert_eq!(pending_of(&a), pending_of(&z), "{a:?} against {z:?}");
        assert_eq!(watched.opens, 1, "and one of them really watched");
        assert_eq!(polled.opens, 0, "while the other really did not");
    }
}
