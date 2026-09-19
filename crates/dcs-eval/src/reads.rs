//! The game-state reads: which `DCS.*` calls this client will ever send,
//! and the shape each one is sent in.
//!
//! Every read here is authored by this side, not by the executor: the
//! executor evaluates what it is given and keeps no catalogue of calls,
//! so whether a read is safe to make is this crate's concern. The answer
//! taken is precedent rather than plausibility — the tier-1 five are the
//! ones ED's own hook script calls from the hook state, which is the
//! strongest evidence available that a hook may — and the answer to
//! everything else is a constant table: a call that is not in it is not a
//! game-state read at all, and an agent that wants one evaluates it under
//! its own name where a crash names it.
//!
//! The table is not the proof. Asserting that a list lacks a name is
//! circular, so the tests below that pin this table to the frozen text
//! prove exactly that and nothing about what is published; what is
//! published is proved at the publication seam and on the stand-in's own
//! ledger of the bytes that reached the disk.

use std::fmt;
use std::time::Duration;

use crate::pipeline::{PipeError, Pipeline, Spec};
use crate::protocol::Envelope;
use crate::readers::Handshake;
use crate::wait::{Outcome, PHASE_LOAD};

/// Which tier a read belongs to, and so whether it is sent by default.
///
/// Tier 2 is built and off. The four in it are present in the hook state
/// by the census but are called by ED only from `gui`, so there is no
/// hook-state precedent for any of them; they are enabled one at a time,
/// each measured alone under the probe supervisor on a live run, and
/// nothing in this crate or the binary flips the switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    One,
    Two,
}

/// One read: the name a caller asks for it under, the Lua expression the
/// chunk calls, and its tier.
///
/// The fields are private and [`READS`] is the only value of this type
/// anywhere, so no caller can assemble a read the table does not hold.
/// That is a structural argument and not a checkable one, which is why
/// the bytes are vetted again where they are published.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Read {
    key: &'static str,
    callee: &'static str,
    tier: Tier,
}

impl Read {
    /// The fact's own name, as the facts are named on this side.
    #[must_use]
    pub fn key(&self) -> &'static str {
        self.key
    }

    /// The Lua expression the chunk hands to `pcall`. It is a whole
    /// expression and not a bare function name, because one of the nine
    /// is not under `DCS` at all.
    #[must_use]
    pub fn callee(&self) -> &'static str {
        self.callee
    }

    /// Which tier it belongs to.
    #[must_use]
    pub fn tier(&self) -> Tier {
        self.tier
    }
}

/// Every read this client may send, in the order it sends them.
///
/// The five tier-1 entries come first and in the frozen document's own
/// order, which is the order ED's hook script makes them in.
const READS: [Read; 9] = [
    Read {
        key: "pause",
        callee: "DCS.getPause",
        tier: Tier::One,
    },
    Read {
        key: "mission_name",
        callee: "DCS.getMissionName",
        tier: Tier::One,
    },
    Read {
        key: "mission_file",
        callee: "DCS.getMissionFilename",
        tier: Tier::One,
    },
    Read {
        key: "model_time",
        callee: "DCS.getModelTime",
        tier: Tier::One,
    },
    Read {
        key: "sim_mode",
        callee: "DCS.getSimulatorMode",
        tier: Tier::One,
    },
    Read {
        key: "multiplayer",
        callee: "DCS.isMultiplayer",
        tier: Tier::Two,
    },
    Read {
        key: "server",
        callee: "DCS.isServer",
        tier: Tier::Two,
    },
    Read {
        key: "track",
        callee: "DCS.isTrackPlaying",
        tier: Tier::Two,
    },
    Read {
        key: "player_id",
        callee: "net.get_my_player_id",
        tier: Tier::Two,
    },
];

/// The three names this client never sends, whatever else happens.
///
/// One is a named suspect in a hook-state crash and the other two were in
/// the batch that crashed. They are stored bare, without the `DCS.`
/// prefix, for two reasons: the frozen text gives two of the three that
/// way, and a bare name matches every spelling a caller could reach the
/// call under.
///
/// This is a second gate and deliberately redundant with the table: it is
/// not known whether the batching was the hazard or the particular reads
/// were, so the rule is a constant list of what may be sent, and these
/// three are named again so that promoting one into the table is still
/// refused.
pub const NEVER: [&str; 3] = ["getMissionLoaded", "getPlayerUnitType", "getMissionTheatre"];

/// Which tiers a gather may send. The default is tier 1 alone and
/// [`Tiers::with_tier_two`] is the only route to the other; nothing
/// outside a test calls it, and no command-line flag reaches it yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tiers {
    tier_two: bool,
}

impl Tiers {
    /// Tier 1 and tier 2 both. It exists so the switch is built and
    /// testable; turning it on for real waits on a live run that has
    /// measured each tier-2 read alone.
    #[must_use]
    pub fn with_tier_two() -> Self {
        Self { tier_two: true }
    }

    /// Whether tier 2 is on.
    #[must_use]
    pub fn tier_two(self) -> bool {
        self.tier_two
    }
}

/// The reads `t` admits, in table order.
#[must_use]
pub fn listed(t: Tiers) -> Vec<&'static Read> {
    READS
        .iter()
        .filter(|r| r.tier == Tier::One || t.tier_two)
        .collect()
}

/// The chunk that makes one read, for the call expression `callee`.
///
/// It is the frozen document's own five lines with the callee
/// substituted, and it is substituted bare: `pcall(<callee>)` hands
/// `pcall` the function value, so the call happens inside the protection
/// and a raise comes back as `ok == false` rather than as an error of the
/// request.
///
/// Two consequences worth naming, because the next stage reads answers
/// off this grammar. `tostring` is applied to a scalar only — a table,
/// function, userdata or thread is reported by its type and never
/// stringified, since nothing on this side walks one — so the answer is
/// `<type>\t` with nothing after the tab. And indexing the callee happens
/// *outside* the `pcall`: on a host where the table it sits under is nil
/// the chunk raises before `pcall` is entered, and the executor answers
/// that as an error of the request, not as a read that threw. A read that
/// threw and a host that has no such table are different findings and
/// nothing here folds them together.
#[must_use]
pub fn chunk(callee: &str) -> Vec<u8> {
    format!(
        "local ok, v = pcall({callee})\n\
         if not ok then return 'error\\t' .. tostring(v) end\n\
         local t = type(v)\n\
         if t == 'table' or t == 'function' or t == 'userdata' or t == 'thread' \
         then return t .. '\\t' end\n\
         return t .. '\\t' .. tostring(v)\n"
    )
    .into_bytes()
}

/// Why a listed read was not published at all.
///
/// The two are kept apart, and the tier filter is applied first: a tier-2
/// read while the switch is off says so even during a load, because it
/// would not have been sent either way, and only a read the switch admits
/// can be held back by the load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotSent {
    /// Tier 2 is off, and this read is in it.
    TierTwoOff,
    /// The session is loading. Nothing answers during a load, so a
    /// request published into one would only wait.
    Loading,
}

impl fmt::Display for NotSent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TierTwoOff => write!(f, "tier 2 is off"),
            Self::Loading => write!(f, "the session is loading"),
        }
    }
}

/// Why no `ok` reply came back, kept as data rather than as prose.
///
/// The arms are here so that nothing downstream has to prefix-match a
/// sentence to tell one from another. A reply that came back and said
/// something other than `ok` is a fact about the carrier the request
/// named, and which thing it says is in the status and the stage; a
/// pending, a superseded session, a session that is gone and a window
/// that could not publish say nothing about a carrier at all. Rendering
/// any of it as a sentence is [`fmt::Display`]'s job, so a change to the
/// wording cannot change what a reader can tell apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unanswered {
    /// A reply came back and its status was not `ok`. Both fields are
    /// the headers verbatim, and `stage` is `None` where the reply
    /// carried none.
    NotOk {
        status: String,
        stage: Option<String>,
        detail: String,
    },
    /// The window gave up with the request still unanswered.
    Pending { id: String, phase: String },
    /// The session was superseded before it answered.
    Superseded { id: String },
    /// The session was gone before it answered.
    Dead { id: String },
    /// The window could not publish the request or could not read its
    /// reply.
    Window { detail: String },
    /// The window yielded fewer items than it was handed specs, so there
    /// was nothing to read for this one.
    Unyielded,
}

impl fmt::Display for Unanswered {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotOk {
                status,
                stage,
                detail,
            } => match stage {
                Some(stage) => write!(f, "{status} at {stage}: {detail}"),
                None => write!(f, "{status}: {detail}"),
            },
            Self::Pending { id, phase } => {
                write!(f, "{id} is still pending, the session in {phase}")
            }
            Self::Superseded { id } => {
                write!(f, "{id}: the session was superseded before it answered")
            }
            Self::Dead { id } => write!(f, "{id}: the session was gone before it answered"),
            Self::Window { detail } => write!(f, "{detail}"),
            Self::Unyielded => write!(f, "the window ended before this request was yielded"),
        }
    }
}

/// What one read came to. Five arms and no default: an errored read, an
/// absent read, a false read and a far end that has stopped speaking the
/// grammar are four different findings, and the derivation that reads
/// these must not be able to confuse them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// The chunk ran and returned. `value` is the scalar's text, or
    /// `None` where the type was one the chunk reports without
    /// stringifying.
    Value {
        lua_type: String,
        value: Option<String>,
    },
    /// The read threw inside its own `pcall`, message verbatim. Nothing
    /// else produces this arm, which is what makes an errored read its
    /// own axis rather than a kind of absence.
    Raised { message: String },
    /// An `ok` reply that is not the chunk's grammar. It is kept apart
    /// from a raise because a far end that has stopped speaking the
    /// grammar is a finding about this build, not a fact about the game.
    Malformed { body: Vec<u8> },
    /// Anything that is not an `ok` reply: a refusal with its status and
    /// stage, a pending, a session superseded or gone, or a window that
    /// could not publish or could not read. None of it says anything
    /// about the game, and which of them it was stays on the arm as
    /// data, because a refusal is evidence about the carrier and the
    /// rest is not.
    Unanswered { why: Unanswered },
    /// The read was never published.
    NotSent { why: NotSent },
}

/// The grammar's tag words that carry no text after the tab.
const OPAQUE: [&str; 4] = ["table", "function", "userdata", "thread"];

/// Every Lua type name, which is the whole of what a well-formed tag may
/// be besides `error`.
const TYPES: [&str; 8] = [
    "nil", "boolean", "number", "string", "table", "function", "userdata", "thread",
];

/// The `ok` reply a window's item carries, or why there is none.
///
/// Everything that rides a reads window goes through here — a read, the
/// ping, the probe — so that "no answer came back" is decided once and
/// in one shape, and each caller is left with only what an `ok` reply of
/// its own kind means.
fn replied(item: Result<Outcome, PipeError>) -> Result<Envelope, Unanswered> {
    let envelope = match item {
        Ok(Outcome::Reply(envelope)) => envelope,
        Ok(Outcome::Pending { id, phase, .. }) => return Err(Unanswered::Pending { id, phase }),
        Ok(Outcome::Superseded { id }) => return Err(Unanswered::Superseded { id }),
        Ok(Outcome::Dead { id }) => return Err(Unanswered::Dead { id }),
        Err(err) => {
            return Err(Unanswered::Window {
                detail: err.to_string(),
            });
        }
    };
    let status = envelope.headers.get("status").unwrap_or_default();
    if status == "ok" {
        return Ok(envelope);
    }
    let stage = envelope.headers.get("stage").unwrap_or_default();
    Err(Unanswered::NotOk {
        status: status.to_owned(),
        stage: (!stage.is_empty()).then(|| stage.to_owned()),
        detail: String::from_utf8_lossy(&envelope.body).into_owned(),
    })
}

/// What the pipeline's item for one read says that read came to.
///
/// It takes the whole item rather than the outcome alone, so the error
/// half — a spec that never reached the disk, a counter with nothing
/// left, a session that would not read — lands on `Unanswered` instead of
/// being dropped or unwrapped.
///
/// The body is split at the **first** tab and no other: a raised message
/// or a returned string may carry tabs and newlines of its own, and the
/// tail is passed through exactly as it came. `error` is not a Lua type
/// name, so a read that *returns* the string "error" is tagged `string`
/// and cannot be mistaken for one that threw.
#[must_use]
pub fn answer_of(item: Result<Outcome, PipeError>) -> Answer {
    let envelope = match replied(item) {
        Ok(envelope) => envelope,
        Err(why) => return Answer::Unanswered { why },
    };
    // The chunk always returns a string, so a reply that says it returned
    // anything else did not run the chunk this side built.
    if envelope.headers.get("result_type").unwrap_or_default() != "string" {
        return Answer::Malformed {
            body: envelope.body,
        };
    }
    let Ok(text) = std::str::from_utf8(&envelope.body) else {
        return Answer::Malformed {
            body: envelope.body,
        };
    };
    let Some((tag, rest)) = text.split_once('\t') else {
        return Answer::Malformed {
            body: envelope.body,
        };
    };
    if tag == "error" {
        return Answer::Raised {
            message: rest.to_owned(),
        };
    }
    if !TYPES.contains(&tag) {
        return Answer::Malformed {
            body: envelope.body,
        };
    }
    if OPAQUE.contains(&tag) {
        if !rest.is_empty() {
            // The chunk never stringifies one of these, so text after the
            // tab means something else wrote the body.
            return Answer::Malformed {
                body: envelope.body,
            };
        }
        return Answer::Value {
            lua_type: tag.to_owned(),
            value: None,
        };
    }
    Answer::Value {
        lua_type: tag.to_owned(),
        value: Some(rest.to_owned()),
    }
}

/// What the reachability probe came to.
///
/// The probe is not a read: it calls nothing, speaks no read grammar and
/// answers about the carrier rather than about the game, so it has
/// answers of its own rather than borrowing a read's. Routing it through
/// the read grammar would report its one success — a body of `ok`, which
/// carries no tab — as a body that is not a read's, so the outcome
/// meaning "the state is reachable" would arrive on the arm reserved for
/// a far end that has stopped speaking.
///
/// The three the frozen document names are here: `ok` is [`Self::Reachable`],
/// and `refused` and `invalid-state` are the status on
/// [`Unanswered::NotOk`], which is what lets a reader tell which of the
/// two it was without reading a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    /// The state answered the probe's own word, so it can be reached.
    Reachable,
    /// No `ok` reply came back. A refusal is the interesting one and
    /// carries its status; the rest says nothing about the carrier.
    Unanswered { why: Unanswered },
    /// An `ok` reply that is not the probe's word. Like a read's
    /// `Malformed`, this is a finding about this build and never a fact
    /// about the game.
    Malformed { body: Vec<u8> },
}

/// What the pipeline's item for the probe says the probe came to.
///
/// The probe returns one known string, so the test is equality against
/// it rather than a grammar: anything else is a far end answering
/// something this side did not ask for.
#[must_use]
pub fn probe_of(item: Result<Outcome, PipeError>) -> Probe {
    let envelope = match replied(item) {
        Ok(envelope) => envelope,
        Err(why) => return Probe::Unanswered { why },
    };
    if envelope.headers.get("result_type").unwrap_or_default() != "string"
        || envelope.body != PROBE_ANSWER
    {
        return Probe::Malformed {
            body: envelope.body,
        };
    }
    Probe::Reachable
}

/// Why a window of reads was not published at all.
///
/// Both refusals are decided before anything reaches the disk, and
/// neither subsumes the other: a name may be unlisted without being
/// forbidden, and a forbidden name promoted into the table would be
/// listed and must still be refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// One of the three names that are never sent appeared in a request
    /// about to be published.
    NeverSent { name: String },
    /// A request named a callee the constant list does not hold. The rule
    /// is a list of what may be sent, not a list of what may not.
    Unlisted { name: String },
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NeverSent { name } => write!(f, "gather refused: {name} is never sent"),
            Self::Unlisted { name } => write!(
                f,
                "gather refused: {name} is not a read on the constant list"
            ),
        }
    }
}

impl std::error::Error for Refused {}

/// What one gather came to: an answer per listed read, in list order,
/// and the two other things that rode the same window.
///
/// The ping and the probe are here because they cost the same wake as the
/// reads — a window is one quiet period whatever is in it — and a caller
/// that took them separately would pay for two. What either one *means*
/// is not decided here: the ping comes back as the envelope it was, and
/// the probe as its own answer, for whoever maps them onto an axis.
#[derive(Debug)]
pub struct Readings {
    entries: Vec<(&'static Read, Answer)>,
    ping: Option<Result<Envelope, Unanswered>>,
    probe: Option<Probe>,
}

impl Readings {
    /// The ping's reply, where one came back, and otherwise why none
    /// did. An `ok` reply carries the fresh `phase`, `tick` and callback
    /// headers.
    ///
    /// The reason survives rather than collapsing to the `None` a load
    /// gives, because a window that was never opened and a session that
    /// refused the ping are different findings and the axis built on
    /// this one must not confuse them.
    #[must_use]
    pub fn ping(&self) -> Option<Result<&Envelope, &Unanswered>> {
        self.ping.as_ref().map(|r| r.as_ref())
    }

    /// What the reachability probe came to, where one was sent.
    #[must_use]
    pub fn probe(&self) -> Option<&Probe> {
        self.probe.as_ref()
    }

    /// Every read and its answer, in the order the table lists them.
    #[must_use]
    pub fn entries(&self) -> &[(&'static Read, Answer)] {
        &self.entries
    }

    /// The answer for one fact, by its key.
    #[must_use]
    pub fn of(&self, key: &str) -> Option<&Answer> {
        self.entries
            .iter()
            .find(|(r, _)| r.key() == key)
            .map(|(_, a)| a)
    }

    /// The reads that were never published, and why each one was not.
    #[must_use]
    pub fn skipped(&self) -> Vec<(&'static Read, NotSent)> {
        self.entries
            .iter()
            .filter_map(|(r, a)| match a {
                Answer::NotSent { why } => Some((*r, *why)),
                _ => None,
            })
            .collect()
    }
}

/// The call expression a read chunk hands to `pcall`, where the body is
/// one of this module's chunks.
fn callee_of(body: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(body).ok()?;
    let rest = text.split_once("pcall(")?.1;
    let (callee, _) = rest.split_once(')')?;
    Some(callee.to_owned())
}

/// Refuse to publish anything carrying a name that is never sent, or an
/// `eval` whose callee the constant list does not hold.
///
/// Both layers are here on purpose. `Read`'s fields are private and the
/// table is the only value of that type, so production cannot assemble an
/// unlisted read — but it is the specs that reach the disk, and a guard
/// nothing can drive proves nothing, so the check sits on the bytes at the
/// publication seam where a test can hand it a spec the table could not
/// have produced. Its cost is a substring scan of a few short strings once
/// per window.
///
/// The two refusals are independent, and that is the point of having two.
/// The list is an allowlist because it is not known whether the batching
/// was the hazard or the particular reads were; the three never-names are
/// checked separately so that one promoted *into* the table is still
/// refused, which an allowlist alone would wave through.
///
/// The never scan is a substring match and not a parse, which is sound
/// only while no name the table holds contains one of the three. That is
/// what the collision test above keeps true.
pub(crate) fn vet(specs: &[Spec]) -> Result<(), Refused> {
    for spec in specs {
        for never in NEVER {
            let folded = never.to_ascii_lowercase();
            let in_headers = spec
                .headers
                .iter()
                .any(|(_, v)| v.to_ascii_lowercase().contains(&folded));
            let in_body = String::from_utf8_lossy(&spec.body)
                .to_ascii_lowercase()
                .contains(&folded);
            if in_headers || in_body {
                return Err(Refused::NeverSent {
                    name: never.to_owned(),
                });
            }
        }
    }
    for spec in specs {
        let op = spec
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("op"))
            .map(|(_, v)| v.as_str());
        if op != Some("eval") {
            continue;
        }
        // The allowlist falls on anything shaped like a read: a chunk
        // that protects a call. The window carries other evals — a
        // reachability probe that calls nothing — and they are held to
        // the never rule above like everything else, but they are not
        // reads and have no callee to look up.
        let Some(callee) = callee_of(&spec.body) else {
            continue;
        };
        if !READS.iter().any(|r| r.callee() == callee) {
            return Err(Refused::Unlisted { name: callee });
        }
    }
    Ok(())
}

/// The chunkname a read is compiled under, which names the call so that a
/// crash, a raise and a log line all say which read it was.
fn chunkname(callee: &str) -> String {
    format!("=dcs-eval read {callee}")
}

/// The chunk the reachability probe evaluates: it calls nothing and
/// names nothing, so what it answers is about whether the state can be
/// reached at all.
const PROBE: &[u8] = b"return 'ok'";

/// What the probe's chunk returns when the state ran it, which is the
/// whole of what a reachable answer may be.
const PROBE_ANSWER: &[u8] = b"ok";

/// Publish `specs` over one window and hand back what each came to, in
/// the order they were given.
///
/// This is the only route from this module to the disk, and it is a seam
/// of its own so that what is about to be published can be checked as
/// bytes rather than as the table they came from. It yields the window's
/// own items rather than answers, because not everything that rides the
/// window is a read: what each item means is the caller's to decide.
pub(crate) fn publish_reads(
    h: &Handshake,
    specs: Vec<Spec>,
    depth: usize,
    upto: Duration,
) -> Result<Vec<Result<Outcome, PipeError>>, Refused> {
    vet(&specs)?;
    Ok(Pipeline::over(h, specs, depth, upto).collect())
}

/// Every listed read this session will answer, in list order.
///
/// A load publishes nothing: nothing answers during one, so a request
/// written into it would only wait. The tier filter runs first and the
/// load skip second, so a tier-2 read with the switch off says so even
/// during a load — it would not have been sent either way.
///
/// The window is as deep as there are reads, because the whole point of
/// the window here is that the five share one tick rather than costing a
/// wake each.
pub fn gather(
    h: &Handshake,
    phase: &str,
    tiers: Tiers,
    upto: Duration,
) -> Result<Readings, Refused> {
    let reads = listed(tiers);
    let mut entries: Vec<(&'static Read, Answer)> = Vec::with_capacity(READS.len());
    for r in READS.iter() {
        if r.tier() == Tier::Two && !tiers.tier_two() {
            entries.push((
                r,
                Answer::NotSent {
                    why: NotSent::TierTwoOff,
                },
            ));
        }
    }
    // The word is the wait's own, so the half that decides a load and the
    // half that skips the window cannot come to spell it differently.
    if phase == PHASE_LOAD {
        for r in &reads {
            entries.push((
                r,
                Answer::NotSent {
                    why: NotSent::Loading,
                },
            ));
        }
        entries.sort_by_key(|(r, _)| order_of(r));
        return Ok(Readings {
            entries,
            ping: None,
            probe: None,
        });
    }
    // The ping first, then the reads, then the probe: one window, one
    // wake, one quiet period for all three.
    let mut specs: Vec<Spec> = vec![Spec::new(&[("op", "ping"), ("for", h.stamp.as_str())], b"")];
    specs.extend(reads.iter().map(|r| {
        // `for` is written here because a window adds nothing on the
        // way out, the session stamp included.
        let name = chunkname(r.callee());
        Spec::new(
            &[
                ("op", "eval"),
                ("for", h.stamp.as_str()),
                ("state", "hook"),
                ("chunkname", name.as_str()),
            ],
            &chunk(r.callee()),
        )
    }));
    specs.push(Spec::new(
        &[
            ("op", "eval"),
            ("for", h.stamp.as_str()),
            ("state", "gui"),
            ("chunkname", "=dcs-eval probe"),
        ],
        PROBE,
    ));
    let depth = specs.len().max(1);
    let mut items = publish_reads(h, specs, depth, upto)?.into_iter();
    let ping = Some(items.next().map_or(Err(Unanswered::Unyielded), replied));
    for r in &reads {
        let answer = items.next().map_or(
            Answer::Unanswered {
                why: Unanswered::Unyielded,
            },
            answer_of,
        );
        entries.push((r, answer));
    }
    let probe = items.next().map(probe_of);
    entries.sort_by_key(|(r, _)| order_of(r));
    Ok(Readings {
        entries,
        ping,
        probe,
    })
}

/// Where a read sits in the table, so the entries come back in list order
/// whichever branch put each one there.
fn order_of(read: &Read) -> usize {
    READS
        .iter()
        .position(|r| r.key() == read.key())
        .unwrap_or(usize::MAX)
}

#[cfg(test)]
mod game_reads {
    use super::*;
    use crate::protocol::{frame, parse};
    use crate::standin::Standin;
    use crate::testing::Sandbox;
    use std::fs;
    use std::path::Path;
    use std::time::{Instant, SystemTime};

    /// The five tier-1 callees, spelt as the frozen text spells them.
    fn tier_one() -> Vec<&'static str> {
        listed(Tiers::default())
            .iter()
            .map(|r| r.callee())
            .collect()
    }

    /// The chunk as a string, which is how every assertion about it reads
    /// better than a byte slice does.
    fn text(callee: &str) -> String {
        String::from_utf8(chunk(callee)).expect("the chunk is ASCII")
    }

    #[test]
    fn the_chunk_for_getpause_is_the_one_the_document_gives() {
        // The five lines as the frozen text prints them, written out here
        // rather than composed, so the composition has something to be
        // wrong against.
        let wanted = concat!(
            "local ok, v = pcall(DCS.getPause)\n",
            "if not ok then return 'error\\t' .. tostring(v) end\n",
            "local t = type(v)\n",
            "if t == 'table' or t == 'function' or t == 'userdata' or t == 'thread' ",
            "then return t .. '\\t' end\n",
            "return t .. '\\t' .. tostring(v)\n",
        );
        let got = text("DCS.getPause");
        assert_eq!(
            got, wanted,
            "the chunk does not match the one the document gives"
        );
    }

    #[test]
    fn every_listed_read_has_exactly_one_pcall() {
        for r in listed(Tiers::with_tier_two()) {
            assert_eq!(
                text(r.callee()).matches("pcall(").count(),
                1,
                "the chunk for {} does not hold exactly one pcall",
                r.callee()
            );
        }
    }

    #[test]
    fn a_chunk_names_its_own_callee_and_no_other_dcs_name() {
        for r in listed(Tiers::with_tier_two()) {
            let body = text(r.callee());
            for other in listed(Tiers::with_tier_two()) {
                if other.callee() == r.callee() {
                    continue;
                }
                assert!(
                    !body.contains(other.callee()),
                    "the chunk for {} also names {}",
                    r.callee(),
                    other.callee()
                );
            }
        }
    }

    /// What the chunk prints when the reference interpreter runs it over
    /// a stub called `stub`, whose definition is `def`.
    ///
    /// The chunk is loaded from a long bracket rather than a file so that
    /// what runs is the bytes this module built, with nothing in between
    /// that could normalise them. The interpreter is the pinned 5.1.5, the
    /// one every Lua task in this tree depends on; a missing one fails
    /// rather than skips, because a suite that skipped its only
    /// end-to-end check would say nothing and say it in green.
    #[cfg(windows)]
    fn over_a_stub(def: &str, callee: &str) -> String {
        use std::process::Command;
        let b = crate::testing::Sandbox::new();
        let driver = b.join("driver.lua");
        let mut script = def.to_owned();
        script.push_str("\nlocal f = assert(loadstring([==[\n");
        script.push_str(&text(callee));
        script.push_str("]==]))\nio.write(f())\n");
        std::fs::write(&driver, script).expect("the driver is written");
        let out = match Command::new("lua5.1.exe").arg(&driver).output() {
            Ok(out) => out,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => panic!(
                "no lua5.1.exe on PATH: build it with `mise run lua-build`, then run cargo \
                 under mise, `mise exec -- cargo test -p dcs-eval game_reads`"
            ),
            Err(e) => panic!("lua5.1.exe did not start: {e}"),
        };
        assert!(
            out.status.success(),
            "the chunk did not run to the end ({}):\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    #[test]
    #[cfg(windows)]
    fn a_chunk_over_a_stub_returning_true_prints_boolean_true() {
        assert_eq!(
            over_a_stub("stub = function() return true end", "stub"),
            "boolean\ttrue"
        );
    }

    #[test]
    #[cfg(windows)]
    fn a_chunk_over_a_stub_that_raises_prints_error_and_the_message() {
        // The message is the interpreter's, position prefix and all, and
        // it is passed through verbatim: what is pinned here is the tag
        // and that the raise did not escape the chunk.
        let got = over_a_stub("stub = function() error('boom') end", "stub");
        let (tag, rest) = got.split_once('\t').expect("the answer carries a tab");
        assert_eq!(tag, "error");
        assert!(rest.ends_with("boom"), "the message was {rest}");
    }

    #[test]
    #[cfg(windows)]
    fn a_chunk_over_a_stub_returning_a_table_prints_the_type_and_nothing_else() {
        // A table is reported by type and never stringified, so there is
        // nothing after the tab — not an address, which would differ on
        // every run and say nothing about the game.
        assert_eq!(
            over_a_stub("stub = function() return {} end", "stub"),
            "table\t"
        );
    }

    /// A reply as the pipeline hands one over: the status, the
    /// `result_type` and the body, which is all `answer_of` reads.
    fn reply(status: &str, result_type: &str, body: &[u8]) -> Result<Outcome, PipeError> {
        let mut lines = vec![("status", status), ("id", "0000000001-ab")];
        if !result_type.is_empty() {
            lines.push(("result_type", result_type));
        }
        Ok(Outcome::Reply(envelope(&lines, body)))
    }

    /// An envelope built the way one really arrives: framed and parsed,
    /// so a fixture cannot say something the wire could not.
    fn envelope(headers: &[(&str, &str)], body: &[u8]) -> Envelope {
        let bytes = frame(headers, body).expect("the envelope frames");
        parse(&bytes).expect("the envelope parses")
    }

    /// An `ok` reply carrying `body`, which is the ordinary case.
    fn ok(body: &[u8]) -> Answer {
        answer_of(reply("ok", "string", body))
    }

    #[test]
    fn a_boolean_false_is_a_value_and_not_an_absence() {
        // The four arms this stage exists to keep apart, in one place:
        // a false read, an errored read, an absent read and a read that
        // never went.
        let f = ok(b"boolean\tfalse");
        assert_eq!(
            f,
            Answer::Value {
                lua_type: "boolean".to_owned(),
                value: Some("false".to_owned()),
            }
        );
        let raised = ok(b"error\tattempt to call a nil value");
        let unanswered = answer_of(Err(PipeError::Exhausted));
        let not_sent = Answer::NotSent {
            why: NotSent::TierTwoOff,
        };
        assert!(matches!(raised, Answer::Raised { .. }), "{raised:?}");
        assert!(
            matches!(unanswered, Answer::Unanswered { .. }),
            "{unanswered:?}"
        );
        assert_ne!(f, raised);
        assert_ne!(f, unanswered);
        assert_ne!(f, not_sent);
        assert_ne!(raised, unanswered);
        assert_ne!(raised, not_sent);
        assert_ne!(unanswered, not_sent);
    }

    #[test]
    fn an_errored_read_carries_its_message_verbatim() {
        assert_eq!(
            ok(b"error\t[string \"=dcs-eval read DCS.getPause\"]:1: boom"),
            Answer::Raised {
                message: "[string \"=dcs-eval read DCS.getPause\"]:1: boom".to_owned(),
            }
        );
    }

    #[test]
    fn a_raised_message_carrying_a_tab_keeps_it_because_the_split_is_at_the_first() {
        assert_eq!(
            ok(b"error\tone\ttwo\nthree"),
            Answer::Raised {
                message: "one\ttwo\nthree".to_owned(),
            }
        );
    }

    #[test]
    fn a_read_that_returned_the_word_error_is_a_string_and_not_a_raise() {
        // `error` is not a Lua type name, so the tag says which of the
        // two this is and the value never has to.
        assert_eq!(
            ok(b"string\terror"),
            Answer::Value {
                lua_type: "string".to_owned(),
                value: Some("error".to_owned()),
            }
        );
    }

    #[test]
    fn a_table_is_reported_by_type_with_no_text() {
        assert_eq!(
            ok(b"table\t"),
            Answer::Value {
                lua_type: "table".to_owned(),
                value: None,
            }
        );
    }

    #[test]
    fn a_table_with_a_tail_is_malformed_because_the_chunk_never_stringifies_one() {
        assert_eq!(
            ok(b"table\ttable: 0x00a1b2c3"),
            Answer::Malformed {
                body: b"table\ttable: 0x00a1b2c3".to_vec(),
            }
        );
    }

    #[test]
    fn a_body_with_no_tab_is_malformed() {
        assert_eq!(
            ok(b"boolean true"),
            Answer::Malformed {
                body: b"boolean true".to_vec(),
            }
        );
    }

    #[test]
    fn an_unknown_type_word_is_malformed_and_not_a_value() {
        assert_eq!(
            ok(b"integer\t7"),
            Answer::Malformed {
                body: b"integer\t7".to_vec(),
            }
        );
    }

    #[test]
    fn an_ok_reply_whose_result_type_is_not_string_is_malformed() {
        assert_eq!(
            answer_of(reply("ok", "nil", b"boolean\ttrue")),
            Answer::Malformed {
                body: b"boolean\ttrue".to_vec(),
            }
        );
    }

    #[test]
    fn the_empty_body_of_a_nil_reply_is_malformed_and_not_a_nil_value() {
        // A read chunk always returns a string, so an empty body is a far
        // end that has stopped speaking the grammar and not a game fact.
        assert_eq!(
            answer_of(reply("ok", "nil", b"")),
            Answer::Malformed { body: Vec::new() }
        );
        assert_eq!(
            answer_of(reply("ok", "string", b"")),
            Answer::Malformed { body: Vec::new() }
        );
    }

    #[test]
    fn an_error_status_is_unanswered_because_a_host_without_dcs_raises_outside_the_pcall() {
        // The chunk indexes the callee before `pcall` is entered, so on a
        // host where that table is nil the executor answers `error` with a
        // stage. That is a request that did not run, not a read that threw.
        let got = answer_of(reply(
            "error",
            "",
            b"attempt to index global 'DCS' (a nil value)",
        ));
        assert_eq!(
            got,
            Answer::Unanswered {
                why: Unanswered::NotOk {
                    status: "error".to_owned(),
                    stage: None,
                    detail: "attempt to index global 'DCS' (a nil value)".to_owned(),
                }
            }
        );
    }

    #[test]
    fn a_refusal_keeps_its_status_and_stage_as_data_and_not_as_a_sentence() {
        // The derivation built on this has to tell a refusal from a
        // pending without reading prose, so the status and the stage are
        // fields. The sentence is rendered from them and is nobody's
        // input.
        let got = answer_of(Ok(Outcome::Reply(envelope(
            &[("status", "refused"), ("stage", "eval")],
            b"gui is declared and not yet served by this executor",
        ))));
        let Answer::Unanswered { why } = got else {
            panic!("wanted Unanswered, got {got:?}");
        };
        let Unanswered::NotOk {
            status,
            stage,
            detail,
        } = &why
        else {
            panic!("wanted NotOk, got {why:?}");
        };
        assert_eq!(status, "refused");
        assert_eq!(stage.as_deref(), Some("eval"));
        assert!(detail.contains("not yet served"), "{detail}");
        assert_eq!(
            why.to_string(),
            "refused at eval: gui is declared and not yet served by this executor"
        );
    }

    #[test]
    fn a_pipeline_error_is_unanswered_and_not_a_missing_read() {
        // The error half of the window's item has nowhere else to go, and
        // dropping it would turn a window that could not publish into a
        // read that was never listed. It is its own arm, so a reader
        // cannot take it for a carrier that refused.
        let got = answer_of(Err(PipeError::Exhausted));
        let Answer::Unanswered {
            why: Unanswered::Window { detail },
        } = got
        else {
            panic!("wanted Unanswered::Window, got {got:?}");
        };
        assert!(detail.contains("ten-digit seq"), "{detail}");
    }

    #[test]
    fn a_pending_and_a_dead_session_are_unanswered_each_on_its_own_arm() {
        let pending = answer_of(Ok(Outcome::Pending {
            id: "0000000001-ab".to_owned(),
            phase: "menu".to_owned(),
            flag: None,
        }));
        assert_eq!(
            pending,
            Answer::Unanswered {
                why: Unanswered::Pending {
                    id: "0000000001-ab".to_owned(),
                    phase: "menu".to_owned(),
                }
            }
        );
        let dead = answer_of(Ok(Outcome::Dead {
            id: "0000000001-ab".to_owned(),
        }));
        assert_eq!(
            dead,
            Answer::Unanswered {
                why: Unanswered::Dead {
                    id: "0000000001-ab".to_owned(),
                }
            }
        );
        let superseded = answer_of(Ok(Outcome::Superseded {
            id: "0000000001-ab".to_owned(),
        }));
        assert_ne!(superseded, dead, "a superseded session is not a dead one");
    }

    #[test]
    fn every_reason_no_answer_came_back_is_its_own_arm() {
        // The whole point of keeping these apart: no two of them are
        // equal, so a reader cannot land on one thinking it is another.
        let all = [
            Unanswered::NotOk {
                status: "refused".to_owned(),
                stage: Some("eval".to_owned()),
                detail: "no".to_owned(),
            },
            Unanswered::NotOk {
                status: "invalid-state".to_owned(),
                stage: Some("eval".to_owned()),
                detail: "no".to_owned(),
            },
            Unanswered::Pending {
                id: "0000000001-ab".to_owned(),
                phase: "menu".to_owned(),
            },
            Unanswered::Superseded {
                id: "0000000001-ab".to_owned(),
            },
            Unanswered::Dead {
                id: "0000000001-ab".to_owned(),
            },
            Unanswered::Window {
                detail: "no window".to_owned(),
            },
            Unanswered::Unyielded,
        ];
        for (i, one) in all.iter().enumerate() {
            for (j, other) in all.iter().enumerate() {
                assert_eq!(i == j, one == other, "{one:?} against {other:?}");
                assert_eq!(
                    i == j,
                    one.to_string() == other.to_string(),
                    "{one} against {other}"
                );
            }
        }
    }

    /// Long enough that a busy box delays a test rather than turning a
    /// reply into a `pending` and reddening a check about what was
    /// published for a reason that has nothing to do with publishing.
    const UPTO: Duration = Duration::from_secs(5);

    /// A stand-in that looks alive, and the handshake a client reads off
    /// it.
    fn ticking(b: &Sandbox) -> (Standin, Handshake) {
        let mut s = Standin::open(&b.join("dcs"), "hook").expect("the stand-in opens");
        s.pid = std::process::id();
        s.armed = true;
        s.handshake().expect("the handshake publishes");
        s.beat(SystemTime::now()).expect("a fresh beat");
        let h = Handshake::read(&s.output().join("executor.txt")).expect("the handshake reads");
        (s, h)
    }

    /// Poll `dir` until at least `want` files with `suffix` are there,
    /// panicking naming what it saw: every caller is gating a tick on it
    /// and a gate that quietly opened proves nothing.
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

    /// How many requests in the ledger carry `needle`. Every count here
    /// is by predicate and never by total, so a later window that adds a
    /// request of its own does not redden a check about the reads.
    fn carrying(s: &Standin, needle: &str) -> usize {
        s.seen()
            .iter()
            .filter(|seen| String::from_utf8_lossy(&seen.bytes).contains(needle))
            .count()
    }

    /// The `.req` names in `dir`.
    fn published(dir: &Path) -> usize {
        fs::read_dir(dir)
            .expect("the request directory lists")
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".req"))
            .count()
    }

    /// One gather against a stand-in that answers the whole window in one
    /// tick, and how many ticks it took.
    fn gathered(
        s: &mut Standin,
        h: &Handshake,
        phase: &str,
        tiers: Tiers,
        want: usize,
    ) -> Readings {
        std::thread::scope(|scope| {
            let ticker = scope.spawn(|| {
                until(s.req(), ".req", want, UPTO);
                s.tick();
            });
            let readings = gather(h, phase, tiers, UPTO);
            ticker.join().expect("the ticker finishes");
            readings
        })
        .expect("the gather is not refused")
    }

    #[test]
    fn each_read_is_its_own_request() {
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        gathered(&mut s, &h, "menu", Tiers::default(), 7);
        for callee in tier_one() {
            assert_eq!(
                carrying(&s, &chunkname(callee)),
                1,
                "the ledger does not hold exactly one eval naming {callee}"
            );
        }
        // The window's size is this test's subject, so it counts a total:
        // five reads, the ping and the probe.
        assert_eq!(
            carrying(&s, "=dcs-eval read "),
            5,
            "the ledger holds {} eval requests naming a read and should hold 5, one per read",
            carrying(&s, "=dcs-eval read ")
        );
        assert_eq!(
            s.seen().len(),
            7,
            "the ledger holds {} requests and should hold 7: five reads, a ping and a probe",
            s.seen().len()
        );
    }

    #[test]
    fn the_five_reads_share_one_tick() {
        // The window's whole purpose here: five reads, one wake. The
        // ticker waits for all five to be on the disk before it answers
        // anything, so a client that published them one at a time would
        // never let it past the wait.
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        gathered(&mut s, &h, "menu", Tiers::default(), 7);
        assert_eq!(s.tick, 1, "the session answered over {} ticks", s.tick);
    }

    #[test]
    fn every_read_is_sent_to_the_hook_state() {
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        gathered(&mut s, &h, "menu", Tiers::default(), 7);
        assert_eq!(carrying(&s, "state: hook"), 5);
        assert_eq!(carrying(&s, "=dcs-eval read "), 5);
        // The probe is the one eval that goes elsewhere, and the ping
        // names no state at all.
        assert_eq!(carrying(&s, "state: gui"), 1);
    }

    #[test]
    fn every_published_body_holds_exactly_one_pcall() {
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        gathered(&mut s, &h, "menu", Tiers::default(), 7);
        let mut reads = 0;
        for (n, seen) in s.seen().iter().enumerate() {
            let text = String::from_utf8_lossy(&seen.bytes);
            if !text.contains("=dcs-eval read ") {
                continue;
            }
            reads += 1;
            let calls = text.matches("pcall(").count();
            assert_eq!(
                calls,
                1,
                "body {} holds {calls} pcall calls and should hold 1",
                n + 1
            );
        }
        assert_eq!(reads, 5, "the control: {reads} read bodies were looked at");
    }

    #[test]
    fn the_readings_come_back_in_the_list_order() {
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let readings = gathered(&mut s, &h, "menu", Tiers::default(), 7);
        let keys: Vec<&str> = readings.entries().iter().map(|(r, _)| r.key()).collect();
        assert_eq!(
            keys,
            vec![
                "pause",
                "mission_name",
                "mission_file",
                "model_time",
                "sim_mode",
                "multiplayer",
                "server",
                "track",
                "player_id"
            ]
        );
    }

    #[test]
    fn a_loading_phase_publishes_nothing_at_all() {
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let readings = gather(&h, PHASE_LOAD, Tiers::default(), UPTO).expect("not refused");
        assert_eq!(
            published(s.req()),
            0,
            "the request directory holds {} files and nothing answers during a load",
            published(s.req())
        );
        s.tick();
        assert_eq!(s.seen().len(), 0, "the ledger holds a request");
        for r in listed(Tiers::default()) {
            assert_eq!(
                readings.of(r.key()),
                Some(&Answer::NotSent {
                    why: NotSent::Loading
                }),
                "{}",
                r.key()
            );
        }
    }

    #[test]
    fn a_read_whose_reply_is_scripted_comes_back_as_a_value() {
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        s.script("DCS.getPause", "ok", "string", b"boolean\ttrue");
        let readings = gathered(&mut s, &h, "menu", Tiers::default(), 7);
        assert_eq!(
            readings.of("pause"),
            Some(&Answer::Value {
                lua_type: "boolean".to_owned(),
                value: Some("true".to_owned()),
            })
        );
        // The other four are unscripted, so the stand-in answers them as
        // chunks that returned nil — which is not this grammar, and says
        // so rather than inventing a value.
        assert!(
            matches!(readings.of("sim_mode"), Some(Answer::Malformed { .. })),
            "{:?}",
            readings.of("sim_mode")
        );
    }

    #[test]
    fn tier_two_is_not_published_while_the_switch_is_off() {
        // The absence and the control are one assertion: a gather that
        // published nothing would fail the five before it could pass the
        // four, so the absence cannot be true vacuously.
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        gathered(&mut s, &h, "menu", Tiers::default(), 7);
        for callee in tier_one() {
            assert_eq!(
                carrying(&s, callee),
                1,
                "the control: no request names {callee}"
            );
        }
        for (n, seen) in s.seen().iter().enumerate() {
            let text = String::from_utf8_lossy(&seen.bytes);
            for off in [
                "isMultiplayer",
                "isServer",
                "isTrackPlaying",
                "get_my_player_id",
            ] {
                assert!(
                    !text.contains(off),
                    "request {} carries {off} and tier 2 is off",
                    n + 1
                );
            }
        }
    }

    #[test]
    fn a_tier_two_read_with_the_switch_off_is_not_sent_rather_than_unanswered() {
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let readings = gathered(&mut s, &h, "menu", Tiers::default(), 7);
        for key in ["multiplayer", "server", "track", "player_id"] {
            assert_eq!(
                readings.of(key),
                Some(&Answer::NotSent {
                    why: NotSent::TierTwoOff
                }),
                "{key}"
            );
        }
        assert_eq!(readings.skipped().len(), 4);
        // One entry per read and no more. Without this the check is a
        // ceiling alone: a gather that both marked a tier-2 read not sent
        // *and* published it would leave two entries under one key, and
        // the lookup above would find the first and pass.
        assert_eq!(
            readings.entries().len(),
            9,
            "the readings hold {} entries and there are nine reads",
            readings.entries().len()
        );
        for r in listed(Tiers::with_tier_two()) {
            assert_eq!(
                readings
                    .entries()
                    .iter()
                    .filter(|(e, _)| e.key() == r.key())
                    .count(),
                1,
                "{} has more than one answer",
                r.key()
            );
        }
    }

    #[test]
    fn a_tier_two_read_is_not_sent_during_a_load_either_and_says_the_switch_not_the_load() {
        // The tier filter runs first: a read the switch would not have
        // sent is not held back by the load, because it would not have
        // gone either way.
        let b = Sandbox::new();
        let (_s, h) = ticking(&b);
        let readings = gather(&h, PHASE_LOAD, Tiers::default(), UPTO).expect("not refused");
        assert_eq!(
            readings.of("multiplayer"),
            Some(&Answer::NotSent {
                why: NotSent::TierTwoOff
            })
        );
        assert_eq!(
            readings.of("pause"),
            Some(&Answer::NotSent {
                why: NotSent::Loading
            })
        );
    }

    #[test]
    fn the_switch_exists_and_sends_nine_when_it_is_asked_for() {
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        gathered(&mut s, &h, "menu", Tiers::with_tier_two(), 11);
        for r in listed(Tiers::with_tier_two()) {
            assert_eq!(
                carrying(&s, &chunkname(r.callee())),
                1,
                "no request names {}",
                r.callee()
            );
        }
        assert_eq!(
            s.seen().len(),
            11,
            "nine reads, the ping and the probe: {} requests",
            s.seen().len()
        );
    }

    #[test]
    fn the_default_tiers_value_is_tier_one_alone() {
        assert!(!Tiers::default().tier_two());
        assert!(Tiers::with_tier_two().tier_two());
    }

    /// A read spec as `gather` builds one, for any callee at all — which
    /// is how a test reaches the seam with something the table could not
    /// have produced. It goes through `publish_reads`, the production
    /// path, and there is no test-only constructor anywhere.
    fn spec_for(h: &Handshake, callee: &str) -> Spec {
        let name = chunkname(callee);
        Spec::new(
            &[
                ("op", "eval"),
                ("for", h.stamp.as_str()),
                ("state", "hook"),
                ("chunkname", name.as_str()),
            ],
            &chunk(callee),
        )
    }

    #[test]
    fn a_forbidden_name_is_refused_before_anything_reaches_the_disk() {
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let got = publish_reads(&h, vec![spec_for(&h, "DCS.getMissionLoaded")], 1, UPTO);
        assert_eq!(
            got.expect_err("wanted Refused, got Ok(..)"),
            Refused::NeverSent {
                name: "getMissionLoaded".to_owned()
            }
        );
        assert_eq!(
            published(s.req()),
            0,
            "the request directory holds {} files and should hold none",
            published(s.req())
        );
        s.tick();
        assert_eq!(s.seen().len(), 0, "the ledger holds a request");
    }

    #[test]
    fn a_name_that_is_merely_unlisted_is_refused_too_because_the_rule_is_a_list_and_not_a_blocklist()
     {
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let got = publish_reads(&h, vec![spec_for(&h, "DCS.getUnitType")], 1, UPTO);
        assert_eq!(
            got.expect_err("wanted Refused, got Ok(..)"),
            Refused::Unlisted {
                name: "DCS.getUnitType".to_owned()
            }
        );
        assert_eq!(published(s.req()), 0);
        s.tick();
        assert_eq!(s.seen().len(), 0, "the ledger holds a request");
    }

    #[test]
    fn a_forbidden_name_in_a_chunkname_is_refused_as_one_in_a_body_is() {
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let spec = Spec::new(
            &[
                ("op", "eval"),
                ("for", h.stamp.as_str()),
                ("state", "hook"),
                ("chunkname", "=dcs-eval read DCS.getMissionTheatre"),
            ],
            &chunk("DCS.getPause"),
        );
        assert_eq!(
            publish_reads(&h, vec![spec], 1, UPTO).expect_err("wanted Refused, got Ok(..)"),
            Refused::NeverSent {
                name: "getMissionTheatre".to_owned()
            }
        );
        assert_eq!(published(s.req()), 0);
        s.tick();
        assert_eq!(s.seen().len(), 0, "the ledger holds a request");
    }

    #[test]
    fn a_forbidden_name_is_refused_even_when_it_is_in_the_read_table() {
        // The never gate is independent of the list: a name promoted into
        // the table would pass an allowlist, and this is the check that
        // would still refuse it. The spec here is byte for byte what a
        // tenth table entry would produce.
        let b = Sandbox::new();
        let (_s, h) = ticking(&b);
        let got = publish_reads(&h, vec![spec_for(&h, "DCS.getMissionLoaded")], 1, UPTO);
        assert_eq!(
            got.expect_err("wanted Refused::NeverSent, got Ok(())"),
            Refused::NeverSent {
                name: "getMissionLoaded".to_owned()
            },
            "the refusal must be the never gate's and not the list's"
        );
    }

    #[test]
    fn no_request_of_a_full_gather_carries_a_never_sent_name() {
        // The sweep: what actually reached the disk, headers included,
        // with tier 2 on so every read the client can ever send is in it.
        // The positive control is in the same test and counted by
        // predicate, so a gather that published nothing fails here first.
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        gathered(&mut s, &h, "menu", Tiers::with_tier_two(), 11);
        for r in listed(Tiers::with_tier_two()) {
            assert_eq!(
                carrying(&s, &chunkname(r.callee())),
                1,
                "the control: no request names {}",
                r.callee()
            );
        }
        for (n, seen) in s.seen().iter().enumerate() {
            let text = String::from_utf8_lossy(&seen.bytes).to_ascii_lowercase();
            for never in NEVER {
                assert!(
                    !text.contains(&never.to_ascii_lowercase()),
                    "request {} of the ledger carries a name that is never sent: {never}\n{}",
                    n + 1,
                    String::from_utf8_lossy(&seen.bytes)
                );
            }
        }
    }

    #[test]
    fn the_ping_and_the_probe_share_the_reads_tick() {
        // A window is one wake whatever is in it, so the ping and the
        // probe cost nothing beyond the reads. The ticker waits for all
        // seven before it answers anything.
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let readings = gathered(&mut s, &h, "menu", Tiers::default(), 7);
        assert_eq!(s.tick, 1, "the session answered over {} ticks", s.tick);
        let ping = readings
            .ping()
            .expect("a ping was sent")
            .expect("the ping answered");
        assert_eq!(ping.body, b"pong");
        assert!(readings.probe().is_some(), "the probe answered");
    }

    #[test]
    fn a_ping_that_left_no_envelope_says_why_and_does_not_look_like_a_load() {
        // A window that was never opened and a session that did not
        // answer the ping both leave no envelope, and only one of them is
        // a fact about the session. The stamp is bent so the fence
        // answers every request in the window, the ping included.
        let b = Sandbox::new();
        let (mut s, mut h) = ticking(&b);
        h.stamp.push_str("-not-this-session");
        let readings = gathered(&mut s, &h, "menu", Tiers::default(), 7);
        let Some(Err(why)) = readings.ping() else {
            panic!("wanted a ping that says why, got {:?}", readings.ping());
        };
        assert!(
            matches!(why, Unanswered::Superseded { .. }),
            "the fence answers a bent stamp on the stamp alone: {why:?}"
        );
        // The other half of the distinction, from the branch that sends
        // no ping at all.
        let loaded = gather(&h, PHASE_LOAD, Tiers::default(), UPTO).expect("not refused");
        assert!(loaded.ping().is_none(), "a load asks nothing");
    }

    #[test]
    fn a_probe_the_state_answered_is_reachable_and_not_a_body_this_side_cannot_read() {
        // The success path, which is the one the read grammar would have
        // mangled: `ok` carries no tab, so a probe routed through a
        // read's answer would come back as a far end that has stopped
        // speaking rather than as a state that can be reached.
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        s.script("return 'ok'", "ok", "string", b"ok");
        let readings = gathered(&mut s, &h, "menu", Tiers::default(), 7);
        assert_eq!(readings.probe(), Some(&Probe::Reachable));
    }

    #[test]
    fn a_probe_the_state_refused_keeps_the_status_that_says_which_refusal() {
        // The stand-in declares `gui` and does not serve it, which is the
        // refusal this arm exists for. What matters downstream is the
        // status as a field: the two refusals the document names are told
        // apart by it and by nothing else.
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let readings = gathered(&mut s, &h, "menu", Tiers::default(), 7);
        let Some(Probe::Unanswered {
            why: Unanswered::NotOk { status, .. },
        }) = readings.probe()
        else {
            panic!("wanted a refusal, got {:?}", readings.probe());
        };
        assert_eq!(status, "unsupported");
    }

    #[test]
    fn a_probe_answering_anything_but_its_own_word_is_malformed() {
        assert_eq!(
            probe_of(reply("ok", "string", b"nope")),
            Probe::Malformed {
                body: b"nope".to_vec()
            }
        );
        assert_eq!(
            probe_of(reply("ok", "nil", b"")),
            Probe::Malformed { body: Vec::new() }
        );
        assert_eq!(
            probe_of(reply("ok", "string", PROBE_ANSWER)),
            Probe::Reachable
        );
    }

    #[test]
    fn an_invalid_state_probe_is_told_from_a_refused_one_without_reading_prose() {
        // T36 puts `session: client` on a `refused` probe and nothing on
        // an `invalid-state` one, so these two must not share an arm.
        let refused = probe_of(Ok(Outcome::Reply(envelope(
            &[("status", "refused"), ("stage", "eval")],
            b"no",
        ))));
        let invalid = probe_of(Ok(Outcome::Reply(envelope(
            &[("status", "invalid-state"), ("stage", "eval")],
            b"no",
        ))));
        assert_ne!(refused, invalid);
        let Probe::Unanswered {
            why: Unanswered::NotOk { status, .. },
        } = &refused
        else {
            panic!("wanted a refusal, got {refused:?}");
        };
        assert_eq!(status, "refused");
    }

    #[test]
    fn the_probe_is_the_one_request_that_does_not_go_to_hook() {
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        gathered(&mut s, &h, "menu", Tiers::default(), 7);
        assert_eq!(carrying(&s, "state: gui"), 1);
        assert_eq!(carrying(&s, "state: hook"), 5);
        assert_eq!(carrying(&s, "op: ping"), 1);
    }

    #[test]
    fn the_probe_body_carries_no_dcs_name() {
        // The probe asks whether the state can be reached at all, so it
        // calls nothing: what it answers is about the carrier and never
        // about the game.
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        gathered(&mut s, &h, "menu", Tiers::default(), 7);
        let probe = s
            .seen()
            .iter()
            .map(|seen| String::from_utf8_lossy(&seen.bytes).into_owned())
            .find(|text| text.contains("=dcs-eval probe"))
            .expect("the probe is in the ledger");
        assert!(probe.ends_with("return 'ok'"), "{probe}");
        assert!(!probe.contains("DCS."), "{probe}");
        assert!(!probe.contains("pcall"), "{probe}");
    }

    #[test]
    fn a_loading_phase_sends_no_ping_and_no_probe_either() {
        let b = Sandbox::new();
        let (mut s, h) = ticking(&b);
        let readings = gather(&h, PHASE_LOAD, Tiers::default(), UPTO).expect("not refused");
        assert_eq!(
            published(s.req()),
            0,
            "the request directory holds {} files and nothing answers during a load",
            published(s.req())
        );
        s.tick();
        assert_eq!(s.seen().len(), 0, "the ledger holds a request");
        assert!(readings.ping().is_none());
        assert!(readings.probe().is_none());
    }

    #[test]
    fn the_tier_one_list_is_the_five_ed_calls_from_a_hook() {
        assert_eq!(
            tier_one(),
            vec![
                "DCS.getPause",
                "DCS.getMissionName",
                "DCS.getMissionFilename",
                "DCS.getModelTime",
                "DCS.getSimulatorMode",
            ]
        );
        let keys: Vec<&str> = listed(Tiers::default()).iter().map(|r| r.key()).collect();
        assert_eq!(
            keys,
            vec![
                "pause",
                "mission_name",
                "mission_file",
                "model_time",
                "sim_mode"
            ]
        );
    }

    #[test]
    fn tier_two_is_the_four_and_is_off_unless_asked_for() {
        assert_eq!(listed(Tiers::default()).len(), 5);
        let all = listed(Tiers::with_tier_two());
        assert_eq!(all.len(), 9);
        let extra: Vec<&str> = all
            .iter()
            .filter(|r| r.tier() == Tier::Two)
            .map(|r| r.callee())
            .collect();
        assert_eq!(
            extra,
            vec![
                "DCS.isMultiplayer",
                "DCS.isServer",
                "DCS.isTrackPlaying",
                "net.get_my_player_id",
            ]
        );
    }

    #[test]
    fn the_one_tier_two_callee_that_is_not_a_dcs_name_is_the_net_one() {
        // The chunk builder must never prepend `DCS.`, and this is the
        // entry that says why.
        let odd: Vec<&str> = listed(Tiers::with_tier_two())
            .iter()
            .map(|r| r.callee())
            .filter(|c| !c.starts_with("DCS."))
            .collect();
        assert_eq!(odd, vec!["net.get_my_player_id"]);
    }

    #[test]
    fn the_never_list_holds_the_suspect_and_the_two_from_its_batch() {
        assert_eq!(
            NEVER.to_vec(),
            vec!["getMissionLoaded", "getPlayerUnitType", "getMissionTheatre"]
        );
    }

    #[test]
    fn no_callee_sits_on_two_lists() {
        for r in READS {
            assert_eq!(
                READS.iter().filter(|o| o.callee == r.callee).count(),
                1,
                "{} appears more than once",
                r.callee
            );
            assert_eq!(
                READS.iter().filter(|o| o.key == r.key).count(),
                1,
                "the key {} appears more than once",
                r.key
            );
        }
    }

    #[test]
    fn no_listed_callee_contains_a_never_sent_name() {
        // The never gate matches bare names as substrings rather than
        // parsing them, which is only sound while no name it may send
        // contains one of them.
        for r in READS {
            for never in NEVER {
                assert!(
                    !r.callee
                        .to_ascii_lowercase()
                        .contains(&never.to_ascii_lowercase()),
                    "{} contains the never-sent name {never}, and the substring gate would \
                     refuse a read the table holds",
                    r.callee
                );
            }
        }
    }
}
