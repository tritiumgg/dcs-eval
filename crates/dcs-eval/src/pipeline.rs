//! A window of requests kept published, and their replies yielded in id
//! order.
//!
//! The throughput case is a driver that publishes the same chunk again and
//! again — paging a walk forward — and the slow way to do it is publish,
//! wait, collect, publish. The executor already answers everything it
//! finds in a frame, in sorted name order, so a client that keeps W
//! requests on the disk at once gets W answers out of one frame instead of
//! W frames. That is all a pipeline is here: a window of **W published
//! requests**, refilled as each one is taken off the front, and the front
//! is always the lowest id in flight.
//!
//! Ordering is the client's promise and not the executor's. The executor
//! answers a frame's worth in sorted order, but a request published after
//! a frame began waits for the next one, so replies land on the disk in
//! whatever order the frames fell — and a client that yielded them as they
//! appeared would hand a driver page 4 before page 2. So the head of the
//! window is waited on by name, and nothing behind it is yielded first
//! however long ago it landed. `id.rs` is the other half of that promise:
//! a zero-padded counter is what makes "lowest id" and "published first"
//! the same thing.
//!
//! What a window does when the head gives an answer that is not a reply —
//! ran out of time, could not be published at all, came back terminal, or
//! could not be read — is decided in decision record 0013, and every
//! branch below points at it.

use std::collections::VecDeque;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use crate::id::Minter;
use crate::publish::{SendError, Sent, send};
use crate::readers::Handshake;
use crate::wait::{Collected, Outcome, Session, WaitError, collect, wait};

/// One request a pipeline will publish: the headers as the caller wants
/// them on the wire, and the body.
///
/// Nothing is added on the way out, the session stamp under `for`
/// included — that is `publish::send`'s promise and this keeps it, so
/// what lands is what the caller asked for and a request that names no
/// session is refused by the executor rather than quietly fixed here.
#[derive(Debug, Clone)]
pub struct Spec {
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Spec {
    /// A spec from borrowed headers, which is how most callers have them.
    #[must_use]
    pub fn new(headers: &[(&str, &str)], body: &[u8]) -> Self {
        Self {
            headers: headers
                .iter()
                .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
                .collect(),
            body: body.to_vec(),
        }
    }
}

/// Why a drain yielded something other than an outcome.
#[derive(Debug)]
pub enum PipeError {
    /// The counter has run past ten digits and there is no id left to
    /// mint. Nothing was published and nothing was consumed.
    Exhausted,
    /// A spec that did not reach the disk, in `send`'s own words. It is
    /// yielded in that spec's own place and the window is not short a
    /// request for it: a spec that never reached the disk was never one
    /// of the W published.
    Send(SendError),
    /// The session could not be read. It ends the drain: the same
    /// unreadable file would fail the same way for ever.
    Wait(WaitError),
}

impl fmt::Display for PipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exhausted => write!(f, "the minter has no ten-digit seq left"),
            Self::Send(err) => write!(f, "{err}"),
            Self::Wait(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for PipeError {}

/// Which terminal answer the head of a dead window gave, so every
/// neighbour can be reported in the same word with its own id.
///
/// A restart and a process that is gone are two different findings and
/// the frozen table names both; reporting a neighbour `superseded`
/// because the head was superseded is right, and reporting one
/// `superseded` because a neighbour of *something* terminal is always
/// superseded would tell a caller a new session had started when this
/// client had seen no such thing. Record 0013 is the argument.
#[derive(Debug, Clone, Copy)]
enum Gone {
    Superseded,
    Dead,
}

impl Gone {
    /// The head's own kind, where it had one.
    fn of(outcome: &Outcome) -> Option<Self> {
        match outcome {
            Outcome::Superseded { .. } => Some(Self::Superseded),
            Outcome::Dead { .. } => Some(Self::Dead),
            _ => None,
        }
    }

    /// The same finding about `id`.
    fn about(self, id: &str) -> Outcome {
        let id = id.to_owned();
        match self {
            Self::Superseded => Outcome::Superseded { id },
            Self::Dead => Outcome::Dead { id },
        }
    }
}

/// What one spec became on its way out: a request on the disk, or a
/// refusal that never reached it.
///
/// A refusal sits in the queue rather than being yielded the moment it
/// happens, and that is the whole of what keeps the drain in order.
/// Publication runs ahead of consumption — by W, that being the point —
/// so a spec refused while the window was being refilled is refused long
/// before the replies in front of it are yielded; announcing it there and
/// then would put it ahead of requests that were published first. Held
/// here it comes out where the caller put it.
///
/// A refusal is not one of the W published requests either, so it does
/// not count towards the window's depth. A run of specs the framer
/// refuses would otherwise serialise the drain behind requests that were
/// never sent.
#[derive(Debug)]
enum Slot {
    Sent(Sent),
    Refused(SendError),
}

/// A window of requests over one session, drained by iterating it.
///
/// Each `next()` publishes up to the window's depth, waits on the lowest
/// id in flight, and yields what that one request came to. It is an
/// [`Iterator`], so a caller writes `for outcome in pipeline` and gets
/// one item per spec — a reply, a `pending` the caller may collect later,
/// a terminal finding, or an error.
pub struct Pipeline {
    session: Session,
    req: PathBuf,
    arm: PathBuf,
    minter: Minter,
    depth: usize,
    upto: Duration,
    /// The specs not yet published, in the order they were given.
    specs: VecDeque<Spec>,
    /// What each spec taken off `specs` became, in the order they were
    /// taken: an id published and not yet yielded, or a refusal waiting
    /// its turn.
    flight: VecDeque<Slot>,
    /// Set once the head came back terminal: publication stops and every
    /// id still in flight is collected once and then reported in this
    /// word.
    gone: Option<Gone>,
    /// Set once nothing more will ever be yielded, so the iterator is
    /// fused rather than re-waiting a head it already gave up on.
    done: bool,
}

impl Pipeline {
    /// A window `depth` deep over the session `h` describes, publishing
    /// `specs` in order and waiting at most `upto` on each head.
    ///
    /// `depth` is the count of requests kept published; a depth of zero is
    /// a depth of one, because a window that publishes nothing is not a
    /// window but a stall, and refusing it would make a caller's
    /// arithmetic — W from a config, a division, a subtraction — into an
    /// error path for no gain.
    #[must_use]
    pub fn over(h: &Handshake, specs: Vec<Spec>, depth: usize, upto: Duration) -> Self {
        Self::over_with(h, Minter::new(), specs, depth, upto)
    }

    /// The same, with the minter named. A caller resuming a counter across
    /// a restart passes its own; so does a test that wants to know the ids
    /// before they are minted.
    #[must_use]
    pub fn over_with(
        h: &Handshake,
        minter: Minter,
        specs: Vec<Spec>,
        depth: usize,
        upto: Duration,
    ) -> Self {
        Self {
            session: Session::addressed(h),
            req: h.req.as_path().to_owned(),
            arm: h.arm.as_path().to_owned(),
            minter,
            depth: depth.max(1),
            upto,
            specs: specs.into(),
            flight: VecDeque::new(),
            gone: None,
            done: false,
        }
    }

    /// How many specs the drain never published. A terminal outcome stops
    /// publication, and this is what is left to retry against whatever
    /// session comes next.
    #[must_use]
    pub fn unsent(&self) -> usize {
        self.specs.len()
    }

    /// Those same specs, back in the caller's hands. A caller that gave
    /// the window its specs by value cannot retry what it cannot get hold
    /// of, and retrying against a new session is the whole answer to a
    /// session that went away.
    #[must_use]
    pub fn into_unsent(self) -> Vec<Spec> {
        self.specs.into()
    }

    /// How many requests are on the disk right now. A refused slot is not
    /// one of them, which is what keeps a refusal from costing the window
    /// a place.
    fn published(&self) -> usize {
        self.flight
            .iter()
            .filter(|slot| matches!(slot, Slot::Sent(_)))
            .count()
    }

    /// Publish until the window is full or there is nothing left to
    /// publish. A spec that refuses takes its place in the queue as a
    /// refusal and the filling goes on, so the window is W deep whatever
    /// any one spec did.
    fn fill(&mut self) -> Result<(), PipeError> {
        while self.gone.is_none() && self.published() < self.depth {
            if self.specs.is_empty() {
                return Ok(());
            }
            // The id is minted before the spec is taken, so a minter with
            // nothing left leaves the spec where it was: `unsent` still
            // counts it, and a caller can retry it under a fresh minter.
            let Some(id) = self.minter.mint() else {
                return Err(PipeError::Exhausted);
            };
            let spec = self.specs.pop_front().expect("the queue was just read");
            let headers: Vec<(&str, &str)> = spec
                .headers
                .iter()
                .map(|(n, v)| (n.as_str(), v.as_str()))
                .collect();
            self.flight.push_back(
                match send(&self.req, &self.arm, &id, &headers, &spec.body) {
                    Ok(sent) => Slot::Sent(sent),
                    Err(err) => Slot::Refused(err),
                },
            );
        }
        Ok(())
    }

    /// One neighbour of a head that came back terminal: collected once,
    /// because a reply that landed before the session died is a real
    /// answer to a real request and reporting a uniform death over it
    /// would throw it away. What has not answered carries the head's own
    /// word.
    fn next_neighbour(&mut self, gone: Gone) -> Option<Result<Outcome, PipeError>> {
        let sent = match self.flight.pop_front()? {
            Slot::Sent(sent) => sent,
            Slot::Refused(err) => return Some(Err(PipeError::Send(err))),
        };
        match collect(&self.session, sent.id()) {
            Ok(Collected::Reply(envelope)) => Some(Ok(Outcome::Reply(envelope))),
            Ok(_) => Some(Ok(gone.about(sent.id()))),
            Err(err) => {
                self.done = true;
                Some(Err(PipeError::Wait(err.into())))
            }
        }
    }
}

impl Iterator for Pipeline {
    type Item = Result<Outcome, PipeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        if let Err(err) = self.fill() {
            return Some(Err(err));
        }
        if let Some(gone) = self.gone {
            let neighbour = self.next_neighbour(gone);
            if neighbour.is_none() {
                self.done = true;
            }
            return neighbour;
        }
        let head = match self.flight.front() {
            Some(Slot::Sent(sent)) => sent.clone(),
            // A refusal in its own place: yielded where the caller put
            // the spec, and the window behind it was never short a
            // request, so there is nothing to refill.
            Some(Slot::Refused(_)) => {
                let Some(Slot::Refused(err)) = self.flight.pop_front() else {
                    unreachable!("the front was just read as a refusal")
                };
                return Some(Err(PipeError::Send(err)));
            }
            None => {
                self.done = true;
                return None;
            }
        };
        let outcome = match wait(&self.session, &head, self.upto) {
            Ok(outcome) => outcome,
            // Nothing to retry against: the same unreadable file fails the
            // same way for ever, so the drain ends here rather than
            // handing the caller an unbounded stream of one error.
            Err(err) => {
                self.done = true;
                return Some(Err(PipeError::Wait(err)));
            }
        };
        self.flight.pop_front();
        if let Some(gone) = Gone::of(&outcome) {
            // Nothing more is published into a session that is gone. What
            // is still in flight is collected on the calls after this one.
            self.gone = Some(gone);
            return Some(Ok(outcome));
        }
        // Refilled before the yield, not after. Between two `next()` calls
        // the caller owns the time — it is doing whatever it asked for the
        // reply for — and a window refilled on the way back in would be W
        // deep only while nothing was being done with the replies, which
        // is the one moment it does not matter.
        // The one thing `fill` can refuse is a minter with no ten-digit
        // seq left, and that leaves its spec where it was, so the next
        // call refuses again in the same words with nothing lost.
        let _ = self.fill();
        Some(Ok(outcome))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::standin::Standin;
    use crate::testing::{Sandbox, entries};
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Instant, SystemTime};

    /// Long enough that a busy box delays a test rather than turning a
    /// reply into a `pending` and reddening a check about ordering for a
    /// reason that has nothing to do with ordering.
    const UPTO: Duration = Duration::from_secs(5);

    /// How long a census waits for the window to reach the depth it
    /// expects. It is generous because a census that gave up early would
    /// redden a check about the window's depth on a box that was merely
    /// busy; a client that really does hold the wrong number of requests
    /// makes the wait cost its full length once per reading and then says
    /// what it saw.
    const CENSUS: Duration = Duration::from_secs(5);

    /// A stand-in that looks alive: this process's id, armed, and a fresh
    /// beat, so the table reads `pending` while a fixture takes its time.
    fn ticking(b: &Sandbox) -> (Standin, Handshake) {
        let mut s = Standin::open(&b.join("dcs"), "hook").expect("the stand-in opens");
        s.pid = std::process::id();
        s.armed = true;
        s.handshake().expect("the handshake publishes");
        s.beat(SystemTime::now()).expect("a fresh beat");
        let h = Handshake::read(&s.output().join("executor.txt")).expect("the handshake reads");
        (s, h)
    }

    /// `n` pings for this session, which is what the stand-in answers and
    /// what a window of them is made of.
    fn pings(s: &Standin, n: usize) -> Vec<Spec> {
        (0..n)
            .map(|_| Spec::new(&[("op", "ping"), ("for", &s.stamp)], b""))
            .collect()
    }

    /// The ids of a drain's items, in the order they came out.
    fn ids(got: &[Result<Outcome, PipeError>]) -> Vec<String> {
        got.iter()
            .map(|item| match item {
                Ok(outcome) => outcome.id().to_owned(),
                Err(err) => format!("error: {err}"),
            })
            .collect()
    }

    /// The `.req` names in `dir`, which is the window as the session sees
    /// it rather than as the client's own bookkeeping claims.
    fn in_flight(dir: &Path) -> usize {
        fs::read_dir(dir)
            .expect("the request directory lists")
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".req"))
            .count()
    }

    /// Poll `dir` until exactly `want` requests are published, or give up.
    /// What it last saw comes back either way, so a timeout is an
    /// assertion about a number rather than a hang.
    ///
    /// It is sound only where the client blocks on each head until the
    /// session answers it, which is what makes the directory's contents
    /// and the window the same thing. A fixture that leaves a request
    /// unanswered has to prove its window some other way: the abandoned
    /// `.req` stays on the disk for ever and the client runs on without
    /// waiting, so what is counted here would be a number neither side
    /// agrees about.
    fn census(dir: &Path, want: usize, upto: Duration) -> usize {
        let deadline = Instant::now() + upto;
        loop {
            let saw = in_flight(dir);
            if saw == want || Instant::now() >= deadline {
                return saw;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Poll `dir` until at least `want` files with `suffix` are there.
    /// Panics naming what it saw, because every caller is gating disorder
    /// on it and a gate that quietly opened proves nothing.
    fn until(dir: &Path, suffix: &str, want: usize, upto: Duration) {
        let deadline = Instant::now() + upto;
        loop {
            let saw = fs::read_dir(dir)
                .expect("the directory lists")
                .filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().ends_with(suffix))
                .count();
            if saw >= want {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "waited for {want} {suffix} files in {} and saw {saw}",
                dir.display()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn pipeline_yields_every_reply_in_id_order_when_the_standin_answers_in_order() {
        // The baseline. It is not the control: a session answering in
        // sorted order satisfies a client that yields in arrival order
        // too, which is exactly why the backwards fixture below exists.
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let specs = pings(&s, 4);
        let minter = Minter::seeded(11);
        let tag = minter.tag().to_owned();
        let got: Vec<_> = std::thread::scope(|scope| {
            let answered = scope.spawn(|| {
                until(s.req(), ".req", 4, UPTO);
                s.tick()
            });
            let drained: Vec<_> = Pipeline::over_with(&h, minter, specs, 4, UPTO).collect();
            let answered = answered.join().expect("the ticker finishes");
            assert_eq!(answered.len(), 4, "one frame answered all four");
            drained
        });
        assert_eq!(
            ids(&got),
            (1..=4)
                .map(|n| format!("{n:010}-{tag}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn pipeline_yields_in_id_order_against_a_standin_that_answers_backwards() {
        // The ordering control, and the one thing it must not do is let
        // the replies arrive in order by accident. So disorder is gated on
        // the disk rather than timed: the ticker waits for the whole
        // window to be published before it answers anything, answers 4, 3
        // and 2, and then waits for all three replies to be on the disk
        // plus a further pause four times the client's poll before it
        // answers 1. The state "the last three replies are there and the
        // first is not" therefore lasts long enough that a client polling
        // at 25 ms cannot miss it, and a client that yielded what it found
        // would yield id 4 first.
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let specs = pings(&s, 4);
        let minter = Minter::seeded(11);
        let tag = minter.tag().to_owned();
        let (got, answered) = std::thread::scope(|scope| {
            let answered = scope.spawn(|| {
                until(s.req(), ".req", 4, UPTO);
                let mut order = Vec::new();
                for _ in 0..3 {
                    order.extend(s.tick_with(|listed| {
                        let last = listed.pop();
                        listed.clear();
                        listed.extend(last);
                    }));
                }
                until(s.res(), ".res", 3, UPTO);
                std::thread::sleep(Duration::from_millis(100));
                order.extend(s.tick());
                order
            });
            let drained: Vec<_> = Pipeline::over_with(&h, minter, specs, 4, UPTO).collect();
            (drained, answered.join().expect("the ticker finishes"))
        });
        let ascending: Vec<String> = (1..=4).map(|n| format!("{n:010}-{tag}")).collect();
        let mut descending = ascending.clone();
        descending.reverse();
        assert_eq!(ids(&got), ascending, "yielded in id order");
        assert_eq!(
            answered, descending,
            "and the fixture really did answer them backwards"
        );
    }

    #[test]
    fn a_window_of_nothing_is_a_window_of_one() {
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let specs = pings(&s, 2);
        let mut p = Pipeline::over_with(&h, Minter::seeded(5), specs, 0, UPTO);
        assert_eq!(census(s.req(), 0, Duration::from_millis(50)), 0);
        let first = std::thread::scope(|scope| {
            let ticker = scope.spawn(|| {
                until(s.req(), ".req", 1, UPTO);
                let one = in_flight(s.req());
                s.tick();
                one
            });
            let first = p.next().expect("a first item");
            assert_eq!(
                ticker.join().expect("the ticker finishes"),
                1,
                "one at a time, never nought"
            );
            first
        });
        assert!(
            matches!(first, Ok(Outcome::Reply(_))),
            "{:?}",
            first.map(|o| o.id().to_owned())
        );
        assert_eq!(
            in_flight(s.req()),
            1,
            "the second is published behind the yield, and only the second"
        );
        assert_eq!(p.unsent(), 0, "so nothing is left unpublished");
    }

    #[test]
    fn the_window_stays_w_deep_while_specs_remain() {
        // The window is counted from the session's side, never from the
        // client's own bookkeeping: what is proved is how many requests
        // were on the disk at once. Three deep over nine specs, one
        // answered per frame, and the reading before each frame must be
        // three until there are fewer than three specs left — a flat run
        // of 3s is exactly what a batch cannot produce. A batch of three
        // publishes three, waits for all three, and only then publishes
        // the next three, so its second reading is 2 and no refill comes
        // until the third is consumed.
        //
        // This depends on the refill happening before the yield: a client
        // that refilled on its way back in would be two deep for as long
        // as the caller held the reply.
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let specs = pings(&s, 9);
        let census = std::thread::scope(|scope| {
            let ticker = scope.spawn(|| {
                let mut readings = Vec::new();
                for step in 0..9 {
                    readings.push(census(s.req(), (9 - step).min(3), CENSUS));
                    s.tick_with(|listed| listed.truncate(1));
                }
                readings
            });
            let drained: Vec<_> = Pipeline::over_with(&h, Minter::seeded(5), specs, 3, UPTO)
                .map(|item| item.is_ok())
                .collect();
            assert_eq!(drained, vec![true; 9], "every spec came back");
            ticker.join().expect("the ticker finishes")
        });
        assert_eq!(census, vec![3, 3, 3, 3, 3, 3, 3, 2, 1]);
    }

    #[test]
    fn the_window_never_holds_more_than_w() {
        // The other side of the same number, and it needs its own run: a
        // client that published all nine at once would satisfy the flat
        // run of 3s above vacuously, since a reading of "at least three"
        // is what that one polls for. Here each reading is taken after the
        // window has had time to settle, so a flood has finished flooding
        // by the time it is counted.
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let specs = pings(&s, 9);
        let most = std::thread::scope(|scope| {
            let ticker = scope.spawn(|| {
                let mut most = 0;
                for step in 0..9 {
                    census(s.req(), (9 - step).min(3), CENSUS);
                    std::thread::sleep(Duration::from_millis(50));
                    most = most.max(in_flight(s.req()));
                    s.tick_with(|listed| listed.truncate(1));
                }
                most
            });
            let drained = Pipeline::over_with(&h, Minter::seeded(5), specs, 3, UPTO).count();
            assert_eq!(drained, 9, "every spec came back");
            ticker.join().expect("the ticker finishes")
        });
        assert!(most <= 3, "saw {most} requests in req/, wanted at most 3");
    }

    #[test]
    fn a_refused_spec_does_not_shrink_the_window() {
        // The refusal is the framer's, met before the disk is touched: a
        // header value with a byte past ASCII is one the executor's own
        // parser would not take, so this side will not write it. Five
        // specs, the second of them unframeable, three deep. What is
        // proved is that the refusal comes out second — where the caller
        // put it, not when the client happened to meet it — and that the
        // window was three published requests deep all the same, which it
        // would not be if a spec that never reached the disk held a place
        // in it.
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let mut specs = pings(&s, 5);
        specs[1]
            .headers
            .push(("note".to_owned(), "a\u{e9}".to_owned()));
        let minter = Minter::seeded(11);
        let tag = minter.tag().to_owned();
        let (got, census) = std::thread::scope(|scope| {
            let ticker = scope.spawn(|| {
                let mut readings = Vec::new();
                for want in [3, 3, 2, 1] {
                    readings.push(census(s.req(), want, CENSUS));
                    s.tick_with(|listed| listed.truncate(1));
                }
                readings
            });
            let drained: Vec<_> = Pipeline::over_with(&h, minter, specs, 3, UPTO).collect();
            (drained, ticker.join().expect("the ticker finishes"))
        });
        assert_eq!(got.len(), 5, "one item per spec");
        assert!(
            matches!(got[1], Err(PipeError::Send(SendError::Frame(_)))),
            "the second is the refusal: {:?}",
            ids(&got)
        );
        for (at, item) in got.iter().enumerate() {
            if at != 1 {
                assert!(
                    matches!(item, Ok(Outcome::Reply(_))),
                    "{at}: {:?}",
                    ids(&got)
                );
            }
        }
        assert_eq!(
            ids(&got)
                .into_iter()
                .enumerate()
                .filter(|(at, _)| *at != 1)
                .map(|(_, id)| id)
                .collect::<Vec<_>>(),
            [1, 3, 4, 5].map(|n| format!("{n:010}-{tag}")).to_vec(),
            "the refused spec spent id 2 and the rest kept their order"
        );
        assert_eq!(
            census,
            vec![3, 3, 2, 1],
            "three deep across the refusal, and only then draining out"
        );
    }

    /// A session restarted under the window: the thread waits for the
    /// whole window to be published, answers whatever `pick` keeps, and
    /// then publishes a handshake on a new stamp. The `.req` files it
    /// leaves are the ones the new session will never list.
    fn restart_under(s: &mut Standin, pick: impl FnOnce(&mut Vec<String>), answered: usize) {
        until(s.req(), ".req", 3, UPTO);
        s.tick_with(pick);
        if answered > 0 {
            until(s.res(), ".res", answered, UPTO);
        }
        s.stamp = format!("{}-restarted", s.stamp);
        s.handshake().expect("the new session's handshake");
    }

    #[test]
    fn a_stamp_change_mid_window_supersedes_every_request_in_flight() {
        // Pipelined requests share a frame, so a request that takes DCS
        // down takes its neighbours' replies with it. Nothing is ticked
        // here: the window is published, the stamp changes, and all three
        // ids come back with the one word, in id order like any other
        // yield.
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let specs = pings(&s, 6);
        let minter = Minter::seeded(11);
        let tag = minter.tag().to_owned();
        let req = s.req().to_owned();
        let mut p = Pipeline::over_with(&h, minter, specs, 3, UPTO);
        let got: Vec<_> = std::thread::scope(|scope| {
            scope.spawn(|| restart_under(&mut s, |listed| listed.clear(), 0));
            p.by_ref().collect()
        });
        assert_eq!(
            ids(&got),
            (1..=3)
                .map(|n| format!("{n:010}-{tag}"))
                .collect::<Vec<_>>()
        );
        for item in &got {
            assert!(
                matches!(item, Ok(Outcome::Superseded { .. })),
                "{:?}",
                ids(&got)
            );
        }
        assert_eq!(
            entries(&req),
            (1..=3)
                .map(|n| format!("{n:010}-{tag}.req"))
                .collect::<Vec<_>>()
                .join(" "),
            "the three lie where the new session will never list them"
        );
        assert_eq!(p.unsent(), 3, "and the other three were never published");
        assert_eq!(p.into_unsent().len(), 3, "and come back to be retried");
    }

    #[test]
    fn nothing_more_is_published_once_the_session_is_gone() {
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let specs = pings(&s, 6);
        let req = s.req().to_owned();
        let got = std::thread::scope(|scope| {
            scope.spawn(|| restart_under(&mut s, |listed| listed.clear(), 0));
            Pipeline::over_with(&h, Minter::seeded(11), specs, 3, UPTO).count()
        });
        assert_eq!(got, 3, "three items, one per request in flight");
        assert_eq!(
            in_flight(&req),
            3,
            "three requests were published into that session and no more"
        );
    }

    #[test]
    fn a_reply_that_landed_before_the_kill_is_still_yielded() {
        // A reply that reached the disk before the restart is a real
        // answer to a real request. Each id still in flight is collected
        // once before it is reported dead, which is the only reason this
        // comes back as a reply rather than a third `superseded`.
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let specs = pings(&s, 3);
        let minter = Minter::seeded(11);
        let tag = minter.tag().to_owned();
        let got: Vec<_> = std::thread::scope(|scope| {
            scope.spawn(|| {
                restart_under(
                    &mut s,
                    |listed| listed.retain(|id| id.starts_with("0000000002")),
                    1,
                );
            });
            Pipeline::over_with(&h, minter, specs, 3, UPTO).collect()
        });
        assert_eq!(
            ids(&got),
            (1..=3)
                .map(|n| format!("{n:010}-{tag}"))
                .collect::<Vec<_>>()
        );
        assert!(matches!(got[0], Ok(Outcome::Superseded { .. })), "the head");
        assert!(
            matches!(got[1], Ok(Outcome::Reply(_))),
            "the one that was answered first"
        );
        assert!(
            matches!(got[2], Ok(Outcome::Superseded { .. })),
            "and the one that never was"
        );
    }

    /// Short on purpose, and only where the timeout is the subject: a head
    /// nothing will ever answer.
    const SOON: Duration = Duration::from_millis(750);

    #[test]
    fn a_reply_that_never_comes_yields_pending_in_its_place() {
        // Nothing ticks, so the one request is still queued when the time
        // runs out. Running out of time is not a failure and not a death:
        // the id comes back with the phase, and the caller may collect it
        // later. The drain is bounded rather than collected, because a
        // client that held the slot would yield the same `pending` for
        // ever and a hang is not a red test.
        let b = Sandbox::new();
        let (s, h) = ticking(&b);
        let specs = pings(&s, 1);
        let minter = Minter::seeded(11);
        let tag = minter.tag().to_owned();
        let got: Vec<_> = Pipeline::over_with(&h, minter, specs, 2, SOON)
            .take(3)
            .collect();
        assert_eq!(ids(&got), [format!("0000000001-{tag}")]);
        let Ok(Outcome::Pending { phase, flag, .. }) = &got[0] else {
            panic!("pending, not {:?}", ids(&got));
        };
        assert_eq!((phase.as_str(), *flag), ("menu", None));
    }

    #[test]
    fn the_window_refills_behind_a_pending_reply() {
        // Two deep over four specs, with the session answering every
        // request except the first — one request queued behind a spent
        // tick budget, which is the ordinary reason a head goes quiet.
        //
        // The proof here is the yield, not a count on the disk: the
        // abandoned request's `.req` is still lying there and nothing but
        // the session will remove it, so a census would be counting the
        // window plus a ghost. What is asserted instead is that the third
        // and fourth specs were published and answered at all — a client
        // that kept the quiet request's slot would publish neither, and
        // would hand back the same `pending` four times over, which is
        // why the drain is bounded rather than collected.
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let specs = pings(&s, 4);
        let minter = Minter::seeded(11);
        let tag = minter.tag().to_owned();
        let quiet = format!("0000000001-{tag}");
        let drained = AtomicBool::new(false);
        let got: Vec<_> = std::thread::scope(|scope| {
            scope.spawn(|| {
                while !drained.load(Ordering::Relaxed) {
                    s.tick_with(|listed| listed.retain(|id| !id.starts_with("0000000001")));
                    std::thread::sleep(Duration::from_millis(5));
                }
            });
            let got: Vec<_> = Pipeline::over_with(&h, minter, specs, 2, SOON)
                .take(4)
                .collect();
            drained.store(true, Ordering::Relaxed);
            got
        });
        assert_eq!(
            ids(&got),
            (1..=4)
                .map(|n| format!("{n:010}-{tag}"))
                .collect::<Vec<_>>()
        );
        assert!(
            matches!(got[0], Ok(Outcome::Pending { .. })),
            "the one nothing answered"
        );
        for (at, item) in got.iter().enumerate().skip(1) {
            assert!(matches!(item, Ok(Outcome::Reply(_))), "{at}: {item:?}");
        }
        assert_eq!(
            entries(s.req()),
            format!("{quiet}.req"),
            "and the quiet one is still lying where it was published"
        );
    }

    #[test]
    fn an_empty_spec_list_publishes_nothing_and_yields_nothing() {
        let b = Sandbox::new();
        let (s, h) = ticking(&b);
        let mut p = Pipeline::over_with(&h, Minter::seeded(5), Vec::new(), 8, UPTO);
        assert!(p.next().is_none(), "nothing to drain");
        assert!(p.next().is_none(), "and it stays none");
        assert_eq!(entries(s.req()), "", "nothing was published");
        assert_eq!(p.unsent(), 0);
    }

    #[test]
    fn a_wait_that_cannot_read_the_session_ends_the_drain() {
        // The handshake removed before the first wait: the table cannot be
        // read, which is a failure to read a file rather than a verdict
        // about the request. Yielded once and never again, because
        // re-waiting the same head would hand a caller looping over this
        // an unbounded stream of one error.
        let b = Sandbox::new();
        let (s, h) = ticking(&b);
        let specs = pings(&s, 3);
        let mut p = Pipeline::over_with(&h, Minter::seeded(5), specs, 3, UPTO);
        fs::remove_file(s.output().join("executor.txt")).expect("the handshake goes");
        let first = p.next().expect("one item");
        assert!(
            matches!(first, Err(PipeError::Wait(_))),
            "{:?}",
            first.map(|o| o.id().to_owned())
        );
        assert!(p.next().is_none(), "the drain ends there");
        assert!(p.next().is_none(), "and stays ended");
    }
}
