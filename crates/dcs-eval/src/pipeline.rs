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
    /// A spec that did not reach the disk, in `send`'s own words. The
    /// window refills behind it: a spec the disk refused was never one of
    /// the W published.
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
    /// The ids published and not yet taken off the front, lowest first.
    flight: VecDeque<Sent>,
    /// A refusal met while refilling before a yield, held until the yield
    /// is out of the way.
    refusal: Option<PipeError>,
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
            refusal: None,
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

    /// Publish until the window is full or there is nothing left to
    /// publish. A refusal stops this call and leaves the rest where they
    /// are; the next one carries on, so a refused spec costs its own slot
    /// and not the window's depth.
    fn fill(&mut self) -> Result<(), PipeError> {
        while self.gone.is_none() && self.flight.len() < self.depth {
            let Some(spec) = self.specs.pop_front() else {
                return Ok(());
            };
            let Some(id) = self.minter.mint() else {
                return Err(PipeError::Exhausted);
            };
            let headers: Vec<(&str, &str)> = spec
                .headers
                .iter()
                .map(|(n, v)| (n.as_str(), v.as_str()))
                .collect();
            match send(&self.req, &self.arm, &id, &headers, &spec.body) {
                Ok(sent) => self.flight.push_back(sent),
                Err(err) => return Err(PipeError::Send(err)),
            }
        }
        Ok(())
    }

    /// One neighbour of a head that came back terminal: collected once,
    /// because a reply that landed before the session died is a real
    /// answer to a real request and reporting a uniform death over it
    /// would throw it away. What has not answered carries the head's own
    /// word.
    fn next_neighbour(&mut self, gone: Gone) -> Option<Result<Outcome, PipeError>> {
        let sent = self.flight.pop_front()?;
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
        // A refusal met while refilling behind the last yield comes out
        // before anything else is published, so refusals are yielded in
        // the order they happened.
        if let Some(err) = self.refusal.take() {
            return Some(Err(err));
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
        let Some(head) = self.flight.front().cloned() else {
            self.done = true;
            return None;
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
        if let Err(err) = self.fill() {
            self.refusal = Some(err);
        }
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
    use std::time::{Instant, SystemTime};

    /// Long enough that a busy box delays a test rather than turning a
    /// reply into a `pending` and reddening a check about ordering for a
    /// reason that has nothing to do with ordering.
    const UPTO: Duration = Duration::from_secs(5);

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
