//! The game state, derived: what the session is doing, on one axis at a
//! time, with every unknown naming its reason.
//!
//! This module owns the axes and nothing else. It sends nothing, reads no
//! file of its own until the one gathering entry at the bottom, and maps
//! the evidence `readers`, `status` and `reads` have already produced onto
//! values a reader can act on.
//!
//! Two rules hold over everything here, and decision record 0018 argues
//! both. Every `unknown` carries a reason, because "unknown because the
//! read raised", "unknown because the read was never sent" and "unknown
//! because there was no heartbeat to read" are three different things and
//! a reader must be able to tell which they have. And no axis is filled
//! from another axis's evidence: an axis may be *gated* by another, where
//! the vocabulary names the gate, and a gate may only select among the
//! gated axis's own values — including `n/a` and `unknown` — never supply
//! one.
//!
//! The deny below is the structural half of "no default arm". A match over
//! the evidence writes every arm out, so that adding a value to any of
//! these enums stops the build instead of falling quietly into a
//! catch-all that would fill an axis from something else.

#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use crate::wait::PHASE_LOAD;

/// Which host wrote the heartbeat being read.
///
/// The host is carried beside the phase and never dropped, because the two
/// hosts' phase vocabularies overlap: `sim` is a word in both. A phase word
/// on its own is therefore not enough to say what the session is doing, and
/// what may be asked of the session differs by host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Host {
    Hook,
    Export,
    /// A host spelt some third way. Kept as its own arm rather than folded
    /// into either, because a file naming a host this build does not know
    /// is a finding and not a default.
    Other(String),
}

impl Host {
    /// The host a heartbeat's `host` header names.
    #[must_use]
    pub fn named(word: &str) -> Self {
        match word {
            "hook" => Self::Hook,
            "export" => Self::Export,
            other => Self::Other(other.to_owned()),
        }
    }

    /// The word this host is written as, which is the word it was read as.
    #[must_use]
    pub fn word(&self) -> &str {
        match self {
            Self::Hook => "hook",
            Self::Export => "export",
            Self::Other(word) => word,
        }
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// A phase word, parsed against the vocabulary of the host that wrote it.
///
/// The two hosts' vocabularies are disjoint but for `sim`, and a word from
/// the wrong one is [`Self::Unrecognised`] rather than a phase this side
/// invented a meaning for: a hook heartbeat saying `stopped`, or an export
/// one saying `menu`, is a session doing something this build has no map
/// for, and saying so is the honest answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Menu,
    Load,
    Sim,
    Paused,
    Loaded,
    Stopped,
    /// A word the named host's vocabulary does not hold, kept verbatim so
    /// a reader sees what was actually written.
    Unrecognised(String),
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Menu => f.write_str("menu"),
            Self::Load => f.write_str("load"),
            Self::Sim => f.write_str("sim"),
            Self::Paused => f.write_str("paused"),
            Self::Loaded => f.write_str("loaded"),
            Self::Stopped => f.write_str("stopped"),
            Self::Unrecognised(word) => write!(f, "an unrecognised phase: {word}"),
        }
    }
}

/// The phase `word` means on `host`.
///
/// The load word is compared against the wait's own constant, so the word
/// that decides a load here and the word that skips the reads window
/// cannot come to be spelt differently. A host this build does not know
/// has no vocabulary at all, so every word from one is unrecognised.
#[must_use]
pub fn phase_of(host: &Host, word: &str) -> Phase {
    match host {
        Host::Hook => match word {
            "menu" => Phase::Menu,
            w if w == PHASE_LOAD => Phase::Load,
            "sim" => Phase::Sim,
            "paused" => Phase::Paused,
            other => Phase::Unrecognised(other.to_owned()),
        },
        Host::Export => match word {
            "loaded" => Phase::Loaded,
            "sim" => Phase::Sim,
            "stopped" => Phase::Stopped,
            other => Phase::Unrecognised(other.to_owned()),
        },
        Host::Other(_) => Phase::Unrecognised(word.to_owned()),
    }
}

/// One piece of evidence, as it was read, for printing beside the others
/// where they disagree or where nothing maps them.
///
/// A name and what it said, and no prose: the printing carries nothing an
/// axis has to agree with, so a reader sees the facts rather than this
/// side's reading of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    pub name: String,
    pub said: String,
}

impl Fact {
    /// A fact called `name` that said `said`.
    #[must_use]
    pub fn new(name: &str, said: impl Into<String>) -> Self {
        Self {
            name: name.to_owned(),
            said: said.into(),
        }
    }
}

impl fmt::Display for Fact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} said {}", self.name, self.said)
    }
}

/// Why an axis is unknown.
///
/// One enum over every axis, because a reason is not a value: it records
/// which evidence failed and how, and it never carries an axis's answer,
/// so it cannot become the route by which one axis hands another an
/// answer. The axis values stay in an enum each. Decision record 0018.
///
/// The arms are kept apart on purpose and none of them subsumes another.
/// "The read raised", "the read was never sent because tier 2 is off",
/// "there is no heartbeat", "the heartbeat would not parse" and "the
/// heartbeat is another session's" are five different findings, and an
/// agent reading one has to be able to tell which it has — two of them
/// say the answer is purchasable and the rest do not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// The read threw inside its own `pcall`, message verbatim. It is
    /// printed alone: nothing else joins it, because an errored read
    /// decides its axis and no other fact gets a say.
    Errored { message: String },
    /// The read was never published, and this is which of the two
    /// reasons.
    NotSent { why: crate::reads::NotSent },
    /// No `ok` reply came back for it.
    Unanswered { why: crate::reads::Unanswered },
    /// An `ok` reply that is not the read grammar: a finding about this
    /// build, never a fact about the game.
    Malformed { body: Vec<u8> },
    /// The read answered a type this axis cannot be made of.
    WrongType {
        lua_type: String,
        value: Option<String>,
    },
    /// Two or more facts contradict, and the disagreement is itself the
    /// finding: a build moved, or a read means something else here.
    Disagrees { facts: Vec<Fact> },
    /// The facts are all there and the vocabulary maps no value to this
    /// combination of them.
    Unmapped { facts: Vec<Fact> },
    /// No heartbeat file at all: nothing has armed, so nothing has
    /// written one.
    NoHeartbeat,
    /// A heartbeat that is there and would not parse.
    HeartbeatUnreadable { detail: String },
    /// A heartbeat carrying another session's stamp. Two installs are
    /// writing into one output directory, which says nothing about this
    /// session.
    ForeignHeartbeat { saw: String, wanted: String },
    /// A heartbeat from a host other than the one this output directory
    /// is for.
    WrongHost { saw: String, wanted: String },
    /// A file that is there and would not parse, naming it. The
    /// handshake's arm, kept apart from a heartbeat's.
    Unreadable {
        path: std::path::PathBuf,
        detail: String,
    },
    /// The named host wrote a phase word its vocabulary does not hold.
    PhaseUnrecognised { host: Host, phase: String },
    /// The export host: `DCS` is nil there, so the read this axis is made
    /// of cannot be made at all.
    NoReadPossible { host: Host },
    /// The axis this one is gated on is itself unknown. The gate selects
    /// among this axis's own values; it never supplies one.
    GateUnknown { gate: &'static str },
    /// The process id was never probed, so nothing was established about
    /// it. Kept apart from a probe that could not decide, which is a
    /// probe that ran.
    NotProbed,
}

impl fmt::Display for Why {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // The error and nothing else, which is the whole of what an
            // errored read sets its axis to.
            Self::Errored { message } => write!(f, "unknown: {message}"),
            Self::NotSent { why } => match why {
                // Spelt out here rather than taken from the reason's own
                // prose, because this exact line is what tells an agent
                // the answer is purchasable and how.
                crate::reads::NotSent::TierTwoOff => f.write_str("unknown (tier 2 off)"),
                crate::reads::NotSent::Loading => f.write_str("unknown (the session is loading)"),
            },
            Self::Unanswered { why } => write!(f, "unknown: no answer came back, {why}"),
            Self::Malformed { body } => write!(
                f,
                "unknown: the reply is not the read grammar, {} bytes of it",
                body.len()
            ),
            Self::WrongType { lua_type, value } => match value {
                Some(value) => write!(f, "unknown: the read answered the {lua_type} {value}"),
                None => write!(f, "unknown: the read answered a {lua_type}"),
            },
            Self::Disagrees { facts } => {
                write!(f, "unknown — the facts disagree: {}", joined(facts))
            }
            Self::Unmapped { facts } => write!(
                f,
                "unknown — no value is mapped to these facts: {}",
                joined(facts)
            ),
            Self::NoHeartbeat => f.write_str("unknown: no heartbeat has been written"),
            Self::HeartbeatUnreadable { detail } => {
                write!(f, "unknown: the heartbeat would not read, {detail}")
            }
            Self::ForeignHeartbeat { saw, wanted } => write!(
                f,
                "unknown: the heartbeat is another session's, stamped {saw} and not {wanted}"
            ),
            Self::WrongHost { saw, wanted } => write!(
                f,
                "unknown: the heartbeat is the {saw} host's and this is the {wanted} host's"
            ),
            Self::Unreadable { path, detail } => {
                write!(f, "unknown: {} would not read, {detail}", path.display())
            }
            Self::PhaseUnrecognised { host, phase } => write!(
                f,
                "unknown: the {host} host wrote a phase this build has no map for, {phase}"
            ),
            Self::NoReadPossible { host } => write!(
                f,
                "unknown: no read is possible on the {host} host, where DCS is nil"
            ),
            Self::GateUnknown { gate } => write!(f, "unknown: {gate} is itself unknown"),
            Self::NotProbed => f.write_str("unknown: the process id was never probed"),
        }
    }
}

/// The one reading of the heartbeat every axis works from.
///
/// The stamp and host checks happen once, here, rather than in each axis,
/// so that no axis can come to read a file the others rejected. A
/// heartbeat that is not this session's yields no phase at all: it is
/// another install's file, and a phase taken off it would make an axis
/// definite out of evidence that is not this session's. `status` carries a
/// `belongs` flag and a wait takes its heartbeat through the same check,
/// for the same reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Beat {
    /// This session's, on the host this output directory is for.
    Ours {
        host: Host,
        phase: Phase,
        armed: bool,
        last_callback: Option<String>,
        callbacks: Vec<String>,
    },
    /// Another session's stamp: two installs writing into one output
    /// directory.
    Foreign { saw: String, wanted: String },
    /// Another host's file under this host's directory.
    WrongHost { saw: String, wanted: String },
    /// There and would not parse.
    Unreadable { detail: String },
}

impl Beat {
    /// The verdict on `beat`, read for the session stamped `stamp` on
    /// `host`.
    ///
    /// The stamp is checked before the host, and the order decides the
    /// case where both differ: a file another session wrote says nothing
    /// about this one, its host header included.
    #[must_use]
    pub fn verdict(beat: &crate::readers::Heartbeat, stamp: &str, host: &Host) -> Self {
        if beat.stamp != stamp {
            return Self::Foreign {
                saw: beat.stamp.clone(),
                wanted: stamp.to_owned(),
            };
        }
        if beat.host != host.word() {
            return Self::WrongHost {
                saw: beat.host.clone(),
                wanted: host.word().to_owned(),
            };
        }
        let host = Host::named(&beat.host);
        let phase = phase_of(&host, &beat.phase);
        Self::Ours {
            host,
            phase,
            armed: beat.armed,
            last_callback: beat.last_callback.clone(),
            callbacks: beat.callbacks.clone(),
        }
    }
}

/// Why this beat gives no phase, or `None` where it gives one.
///
/// Every axis that wants a phase asks here first, so the four ways there
/// is no usable heartbeat stay four reasons rather than collapsing into
/// one. It never produces a value, only a reason: an axis whose evidence
/// is missing is unknown, and which way it is missing is what a reader
/// needs.
#[must_use]
fn unusable(beat: Option<&Beat>) -> Option<Why> {
    match beat {
        Some(Beat::Ours { .. }) => None,
        Some(Beat::Foreign { saw, wanted }) => Some(Why::ForeignHeartbeat {
            saw: saw.clone(),
            wanted: wanted.clone(),
        }),
        Some(Beat::WrongHost { saw, wanted }) => Some(Why::WrongHost {
            saw: saw.clone(),
            wanted: wanted.clone(),
        }),
        Some(Beat::Unreadable { detail }) => Some(Why::HeartbeatUnreadable {
            detail: detail.clone(),
        }),
        None => Some(Why::NoHeartbeat),
    }
}

/// What the session is doing.
///
/// `MenuOrEditor` is one value and not two, and it is not `unknown`: the
/// axis knows the game is at one of the two, and nothing measured on this
/// build tells them apart. The value's name holds the indeterminacy rather
/// than picking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    Loading,
    Mission { name: String },
    MenuOrEditor,
    Unknown { why: Why },
}

impl fmt::Display for Activity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Loading => f.write_str("loading"),
            Self::Mission { name } => write!(f, "in a mission, {name}"),
            Self::MenuOrEditor => f.write_str(
                "at the main menu or in the mission editor (not distinguished on this build)",
            ),
            Self::Unknown { why } => write!(f, "{why}"),
        }
    }
}

/// What the session is doing, from the heartbeat verdict and then the
/// `mission_name` read — in that order, and never the other way.
///
/// The phase decides first and the read only sharpens it. That order is
/// the whole of why a load costs no round trip: a load is read off a file
/// already on the disk, and nothing answers during one anyway.
///
/// At the menu the answer is `menu-or-editor` **whatever `mission_name`
/// says**. What `DCS.getMissionName()` gives at the menu — nothing, or
/// still the last mission flown — has never been measured, so a name there
/// is not evidence about anything and certainly not evidence against a
/// phase that is measured.
///
/// On the export host no read is possible at all, for every phase word it
/// writes and `sim` included: the two hosts' vocabularies overlap on that
/// word, and `DCS` is nil on export, so an answer to the `mission_name`
/// read cannot have come from there. Keying this arm on the word rather
/// than on the host would make a mission out of a read that could not
/// exist.
#[must_use]
pub fn activity_of(beat: Option<&Beat>, mission_name: Option<&crate::reads::Answer>) -> Activity {
    if let Some(why) = unusable(beat) {
        return Activity::Unknown { why };
    }
    let Some(Beat::Ours { host, phase, .. }) = beat else {
        // `unusable` returned `None`, which it does for `Ours` alone.
        return Activity::Unknown {
            why: Why::NoHeartbeat,
        };
    };
    match host {
        Host::Export => Activity::Unknown {
            why: Why::NoReadPossible { host: host.clone() },
        },
        Host::Other(_) => Activity::Unknown {
            why: Why::PhaseUnrecognised {
                host: host.clone(),
                phase: phase.to_string(),
            },
        },
        Host::Hook => match phase {
            Phase::Load => Activity::Loading,
            Phase::Menu => Activity::MenuOrEditor,
            Phase::Sim | Phase::Paused => named_mission(phase, mission_name),
            // The hook host's vocabulary does not hold these two; a
            // heartbeat that writes one is saying something this build
            // has no map for.
            Phase::Loaded | Phase::Stopped | Phase::Unrecognised(_) => Activity::Unknown {
                why: Why::PhaseUnrecognised {
                    host: host.clone(),
                    phase: phase.to_string(),
                },
            },
        },
    }
}

/// The mission the `mission_name` read names, where the phase already
/// says the session is in one.
fn named_mission(phase: &Phase, mission_name: Option<&crate::reads::Answer>) -> Activity {
    use crate::reads::Answer;
    let unknown = |why| Activity::Unknown { why };
    match mission_name {
        Some(Answer::Value { lua_type, value }) if lua_type == "string" => match value {
            Some(name) if !name.is_empty() => Activity::Mission { name: name.clone() },
            // The phase says a mission and the read says there is no
            // name. Both facts are printed: the disagreement is itself
            // the finding.
            Some(_) => unknown(Why::Disagrees {
                facts: vec![
                    Fact::new("phase", phase.to_string()),
                    Fact::new("mission_name", "the empty string"),
                ],
            }),
            None => unknown(Why::WrongType {
                lua_type: lua_type.clone(),
                value: None,
            }),
        },
        Some(Answer::Value { lua_type, value }) => unknown(Why::WrongType {
            lua_type: lua_type.clone(),
            value: value.clone(),
        }),
        // Alone: an errored read decides its axis and no other fact gets
        // a say, the phase included.
        Some(Answer::Raised { message }) => unknown(Why::Errored {
            message: message.clone(),
        }),
        Some(Answer::Malformed { body }) => unknown(Why::Malformed { body: body.clone() }),
        Some(Answer::Unanswered { why }) => unknown(Why::Unanswered { why: why.clone() }),
        Some(Answer::NotSent { why }) => unknown(Why::NotSent { why: *why }),
        None => unknown(Why::Unanswered {
            why: crate::reads::Unanswered::Unyielded,
        }),
    }
}

/// Whether the simulation clock is stopped.
///
/// One fact and not two: a mission paused and a simulation paused are the
/// same stopped clock, and the game menu that stops it in single player is
/// a callback in the `ui` record rather than a state here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pause {
    Paused,
    Running,
    /// Outside a mission, where the question does not arise.
    NotApplicable,
    Unknown {
        why: Why,
    },
}

impl fmt::Display for Pause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // The basis is in the same line as the value, because the
            // read and the callback phase can disagree and a reader has
            // to know which one they are looking at.
            Self::Paused => f.write_str("paused (read)"),
            Self::Running => f.write_str("running (read)"),
            Self::NotApplicable => f.write_str("n/a"),
            Self::Unknown { why } => write!(f, "{why}"),
        }
    }
}

/// The pause axis: the value the read decided, the callback phase beside
/// it, and a note where the two disagree.
///
/// The phase is shown whether or not it agrees. A value with no phase
/// beside it would leave a reader unable to see the disagreement at all,
/// and the disagreement is the interesting part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PauseAxis {
    pub value: Pause,
    pub phase_callback: Option<Phase>,
    /// `Some` only where the read and the phase disagree, saying what the
    /// phase said. It is a note and never a resolution.
    pub note: Option<String>,
}

impl fmt::Display for PauseAxis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)?;
        if let Some(phase) = &self.phase_callback {
            write!(f, ", phase_callback {phase}")?;
        }
        if let Some(note) = &self.note {
            write!(f, " — {note}")?;
        }
        Ok(())
    }
}

/// Whether the clock is stopped, from the `getPause` read, gated on being
/// in a mission at all.
///
/// **The read wins and the disagreement is reported, in both directions.**
/// Where the read says paused and the callback phase says `sim`, or the
/// read says running and the phase says `paused`, the value is the read's
/// and the note says what the phase said. It is never resolved in the
/// callback's favour: DCS can begin a mission already paused without
/// firing either callback and can resume with no preceding pause, and this
/// executor's phase reports `sim` for a mission that began paused. Nothing
/// on this build has measured any of that, which is another reason not to
/// let the phase overrule a read.
///
/// The gate is `activity`, and it only ever selects among this axis's own
/// values: outside a mission the question does not arise, and where
/// `activity` is itself unknown this axis is unknown naming the gate.
/// `activity` never supplies a pause value — decision record 0018.
#[must_use]
pub fn pause_of(
    activity: &Activity,
    beat: Option<&Beat>,
    pause: Option<&crate::reads::Answer>,
) -> PauseAxis {
    use crate::reads::Answer;
    let phase = match beat {
        Some(Beat::Ours { phase, .. }) => Some(phase.clone()),
        Some(Beat::Foreign { .. } | Beat::WrongHost { .. } | Beat::Unreadable { .. }) | None => {
            None
        }
    };
    let settled = |value| PauseAxis {
        value,
        phase_callback: phase.clone(),
        note: None,
    };
    match activity {
        Activity::Loading | Activity::MenuOrEditor => return settled(Pause::NotApplicable),
        Activity::Unknown { .. } => {
            return settled(Pause::Unknown {
                why: Why::GateUnknown { gate: "activity" },
            });
        }
        Activity::Mission { .. } => {}
    }
    let value = match pause {
        Some(Answer::Value { lua_type, value }) if lua_type == "boolean" => {
            match value.as_deref() {
                Some("true") => Pause::Paused,
                Some("false") => Pause::Running,
                Some(other) => Pause::Unknown {
                    why: Why::WrongType {
                        lua_type: lua_type.clone(),
                        value: Some(other.to_owned()),
                    },
                },
                None => Pause::Unknown {
                    why: Why::WrongType {
                        lua_type: lua_type.clone(),
                        value: None,
                    },
                },
            }
        }
        Some(Answer::Value { lua_type, value }) => Pause::Unknown {
            why: Why::WrongType {
                lua_type: lua_type.clone(),
                value: value.clone(),
            },
        },
        // The error and nothing else. The phase is sitting right there
        // and it is `activity`'s evidence, not this axis's.
        Some(Answer::Raised { message }) => Pause::Unknown {
            why: Why::Errored {
                message: message.clone(),
            },
        },
        Some(Answer::Malformed { body }) => Pause::Unknown {
            why: Why::Malformed { body: body.clone() },
        },
        Some(Answer::Unanswered { why }) => Pause::Unknown {
            why: Why::Unanswered { why: why.clone() },
        },
        Some(Answer::NotSent { why }) => Pause::Unknown {
            why: Why::NotSent { why: *why },
        },
        None => Pause::Unknown {
            why: Why::Unanswered {
                why: crate::reads::Unanswered::Unyielded,
            },
        },
    };
    let note = if matches!(value, Pause::Paused) && matches!(phase, Some(Phase::Sim)) {
        Some("the callback phase says sim".to_owned())
    } else if matches!(value, Pause::Running) && matches!(phase, Some(Phase::Paused)) {
        Some("the callback phase says paused".to_owned())
    } else {
        None
    };
    PauseAxis {
        value,
        phase_callback: phase,
        note,
    }
}

/// What kind of session this is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionAxis {
    /// Joined to somebody else's server.
    Client,
    /// Reachable, with tier 2 off. Hosting from the client is
    /// indistinguishable from single player by reachability alone, so the
    /// value names both and says what would separate them.
    SingleOrHost,
    Single,
    Host,
    Unknown {
        why: Why,
    },
}

impl fmt::Display for SessionAxis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client => f.write_str("a client joined to a server (gui refused)"),
            Self::SingleOrHost => f.write_str("single player or host (tier 2 off)"),
            Self::Single => f.write_str("single player (tier 2)"),
            Self::Host => f.write_str("hosting (tier 2)"),
            Self::Unknown { why } => write!(f, "{why}"),
        }
    }
}

/// Whether a track is playing back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Track {
    Replay,
    Live,
    Unknown { why: Why },
}

impl fmt::Display for Track {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Replay => f.write_str("a track playing back (tier 2)"),
            Self::Live => f.write_str("live (tier 2)"),
            Self::Unknown { why } => write!(f, "{why}"),
        }
    }
}

/// The boolean a read answered, or why there is none.
///
/// Every arm is written out so that a read which raised, one that was
/// never sent and one that answered the wrong type stay three reasons.
fn bool_of(answer: Option<&crate::reads::Answer>) -> Result<bool, Why> {
    use crate::reads::Answer;
    match answer {
        Some(Answer::Value { lua_type, value }) if lua_type == "boolean" => {
            match value.as_deref() {
                Some("true") => Ok(true),
                Some("false") => Ok(false),
                Some(other) => Err(Why::WrongType {
                    lua_type: lua_type.clone(),
                    value: Some(other.to_owned()),
                }),
                None => Err(Why::WrongType {
                    lua_type: lua_type.clone(),
                    value: None,
                }),
            }
        }
        Some(Answer::Value { lua_type, value }) => Err(Why::WrongType {
            lua_type: lua_type.clone(),
            value: value.clone(),
        }),
        Some(Answer::Raised { message }) => Err(Why::Errored {
            message: message.clone(),
        }),
        Some(Answer::Malformed { body }) => Err(Why::Malformed { body: body.clone() }),
        Some(Answer::Unanswered { why }) => Err(Why::Unanswered { why: why.clone() }),
        Some(Answer::NotSent { why }) => Err(Why::NotSent { why: *why }),
        None => Err(Why::Unanswered {
            why: crate::reads::Unanswered::Unyielded,
        }),
    }
}

/// The status a probe that did not answer `ok` came back with, where it
/// was a reply that said something rather than no reply at all.
///
/// The status is read off the arm as a field, never off a formatted
/// sentence: `refused` has exactly one meaning on the wire — the state
/// could not be reached — and every other word means something else.
fn refusal_word(probe: &crate::reads::Probe) -> Option<&str> {
    match probe {
        crate::reads::Probe::Unanswered {
            why: crate::reads::Unanswered::NotOk { status, .. },
        } => Some(status),
        crate::reads::Probe::Unanswered { .. }
        | crate::reads::Probe::Reachable
        | crate::reads::Probe::Malformed { .. } => None,
    }
}

/// What kind of session this is, from the `gui` reachability probe and
/// then the tier-2 reads, gated on being in a mission.
///
/// **The probe is read first, and the tier-2 mapping is reached only where
/// the probe was reachable.** Without that ordering written down, a
/// `refused` probe with `isMultiplayer` true and `isServer` false has two
/// rows of the vocabulary claiming it at once. Both are this axis's own
/// evidence, so reading one before the other borrows nothing.
///
/// A probe that came back `refused` means one thing and one thing only:
/// `net.dostring_in` returned nil for that state. On a client joined to a
/// server that happens for every state while `hook` still answers, which
/// is what makes it evidence. Every other refusal word — `unsupported`
/// where eval is off, `bad-request`, a refusal at the mission hop,
/// `oversize`, `budget` — is a different finding and is unknown naming
/// itself, never `client`.
///
/// A `refused` probe at the menu is the second of the vocabulary's two
/// disagreement examples, and **it lands here and leaves `activity:
/// menu-or-editor` definite**. The probe is this axis's evidence; using it
/// to unknown `activity` would be exactly the borrowing decision record
/// 0018 forbids, however strong the instinct to unknown both.
#[must_use]
pub fn session_of(
    activity: &Activity,
    probe: Option<&crate::reads::Probe>,
    tiers: crate::reads::Tiers,
    multiplayer: Option<&crate::reads::Answer>,
    server: Option<&crate::reads::Answer>,
) -> SessionAxis {
    use crate::reads::Probe;
    let refused = probe.and_then(refusal_word) == Some("refused");
    match activity {
        Activity::MenuOrEditor if refused => {
            return SessionAxis::Unknown {
                why: Why::Disagrees {
                    facts: vec![
                        Fact::new("activity", "menu-or-editor"),
                        Fact::new("the gui probe", "refused, where the menu answers"),
                    ],
                },
            };
        }
        Activity::MenuOrEditor | Activity::Loading | Activity::Unknown { .. } => {
            return SessionAxis::Unknown {
                why: Why::GateUnknown { gate: "activity" },
            };
        }
        Activity::Mission { .. } => {}
    }
    match probe {
        // `refused` is the one refusal word that says the state could
        // not be reached, which is what makes it evidence about the
        // session rather than about the request.
        Some(_) if refused => SessionAxis::Client,
        Some(Probe::Unanswered { why }) => SessionAxis::Unknown {
            why: Why::Unanswered { why: why.clone() },
        },
        Some(Probe::Malformed { body }) => SessionAxis::Unknown {
            why: Why::Malformed { body: body.clone() },
        },
        Some(Probe::Reachable) => reachable_session(tiers, multiplayer, server),
        None => SessionAxis::Unknown {
            why: Why::Unanswered {
                why: crate::reads::Unanswered::Unyielded,
            },
        },
    }
}

/// What the tier-2 reads say, where the `gui` state answered the probe.
///
/// `isMultiplayer` true with `isServer` false is a combination the
/// vocabulary maps to nothing, so it is unknown naming both reads.
/// Agreeing with the probe there would be a guess wearing a
/// corroboration: the probe already said what it had to say, and this is
/// a second reading that does not fit.
fn reachable_session(
    tiers: crate::reads::Tiers,
    multiplayer: Option<&crate::reads::Answer>,
    server: Option<&crate::reads::Answer>,
) -> SessionAxis {
    if !tiers.tier_two() {
        return SessionAxis::SingleOrHost;
    }
    let multi = match bool_of(multiplayer) {
        Ok(multi) => multi,
        Err(why) => return SessionAxis::Unknown { why },
    };
    if !multi {
        return SessionAxis::Single;
    }
    let serving = match bool_of(server) {
        Ok(serving) => serving,
        Err(why) => return SessionAxis::Unknown { why },
    };
    if serving {
        SessionAxis::Host
    } else {
        SessionAxis::Unknown {
            why: Why::Unmapped {
                facts: vec![
                    Fact::new("multiplayer", "true"),
                    Fact::new("server", "false"),
                ],
            },
        }
    }
}

/// Whether a track is playing, from the tier-2 read alone.
///
/// No gate: the vocabulary gives this row none. With tier 2 off the read
/// is never sent, and the answer says so in the one line that tells an
/// agent the answer is purchasable and how.
#[must_use]
pub fn track_of(track: Option<&crate::reads::Answer>) -> Track {
    use crate::reads::Answer;
    match track {
        Some(Answer::Value { lua_type, value }) if lua_type == "boolean" => {
            match value.as_deref() {
                Some("true") => Track::Replay,
                Some("false") => Track::Live,
                Some(other) => Track::Unknown {
                    why: Why::WrongType {
                        lua_type: lua_type.clone(),
                        value: Some(other.to_owned()),
                    },
                },
                None => Track::Unknown {
                    why: Why::WrongType {
                        lua_type: lua_type.clone(),
                        value: None,
                    },
                },
            }
        }
        Some(Answer::Value { lua_type, value }) => Track::Unknown {
            why: Why::WrongType {
                lua_type: lua_type.clone(),
                value: value.clone(),
            },
        },
        Some(Answer::Raised { message }) => Track::Unknown {
            why: Why::Errored {
                message: message.clone(),
            },
        },
        Some(Answer::Malformed { body }) => Track::Unknown {
            why: Why::Malformed { body: body.clone() },
        },
        Some(Answer::Unanswered { why }) => Track::Unknown {
            why: Why::Unanswered { why: why.clone() },
        },
        // Which unknown this is matters: tier 2 off is purchasable and
        // the rest are not.
        Some(Answer::NotSent { why }) => Track::Unknown {
            why: Why::NotSent { why: *why },
        },
        None => Track::Unknown {
            why: Why::Unanswered {
                why: crate::reads::Unanswered::Unyielded,
            },
        },
    }
}

/// The handshake, as it was found.
///
/// Three answers and not two, because an absent handshake and one that
/// would not parse are different findings: the first says nothing has ever
/// loaded here, and the second says something did and this build could not
/// read what it wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    Read(Box<crate::readers::Handshake>),
    /// Not there, which the filesystem said in those words.
    Missing,
    /// There and would not parse.
    Unreadable {
        path: std::path::PathBuf,
        detail: String,
    },
}

/// Whether the process the handshake named is there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessAxis {
    Running,
    Gone,
    /// No handshake at all: nothing has ever loaded here.
    NeverRan,
    Unknown {
        why: Why,
    },
}

impl fmt::Display for ProcessAxis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Running => f.write_str("running"),
            Self::Gone => f.write_str("gone"),
            Self::NeverRan => f.write_str("never ran"),
            Self::Unknown { why } => write!(f, "{why}"),
        }
    }
}

/// Whether the process is there, from the handshake and the process
/// probe.
///
/// `never-ran` is the handshake **missing** and nothing else. One that is
/// there and would not parse is unknown naming the file: something loaded
/// and wrote it, which is the opposite of never having run.
///
/// A probe that could not decide is unknown and never `gone`. A handle
/// that would not open says nothing about whether the process is there,
/// and `gone` is a claim that the session will never answer again. That is
/// the reading `status` takes and decision record 0012 argues.
///
/// The ambiguity the vocabulary names under this row — two output
/// directories under two `Saved Games` trees, reported with both and never
/// picked between — is not decided here. This is handed one already-chosen
/// output directory; the ambiguity belongs where the directory is chosen.
#[must_use]
pub fn process_of(found: &Found, probe: Option<&crate::status::Process>) -> ProcessAxis {
    match found {
        Found::Missing => ProcessAxis::NeverRan,
        Found::Unreadable { path, detail } => ProcessAxis::Unknown {
            why: Why::Unreadable {
                path: path.clone(),
                detail: detail.clone(),
            },
        },
        Found::Read(_) => match probe {
            Some(crate::status::Process::Running) => ProcessAxis::Running,
            Some(crate::status::Process::Exited) => ProcessAxis::Gone,
            Some(crate::status::Process::Undecided { why }) => ProcessAxis::Unknown {
                why: Why::Unreadable {
                    path: std::path::PathBuf::from("the process id"),
                    detail: why.clone(),
                },
            },
            None => ProcessAxis::Unknown {
                why: Why::NotProbed,
            },
        },
    }
}

/// Whether the executor is answering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeAxis {
    Dormant,
    Armed,
    Waking,
    Stalled,
    Superseded,
    Unknown { why: Why },
}

impl fmt::Display for BridgeAxis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dormant => f.write_str("dormant"),
            Self::Armed => f.write_str("armed"),
            Self::Waking => f.write_str("waking"),
            Self::Stalled => f.write_str("stalled"),
            Self::Superseded => f.write_str("superseded"),
            Self::Unknown { why } => write!(f, "{why}"),
        }
    }
}

/// Whether the executor is answering, from the ping the reads window
/// carried and, where no window opened, the heartbeat's own word.
///
/// **This is not the wait's table re-implemented.** That table decides
/// about a request that was sent, and the ping is that request: its
/// outcome already carries the verdict, run once by the wait, and it is
/// read off the arm here rather than derived again. On a load no window
/// opens and there is no request at all, so the axis falls back to the
/// heartbeat's `armed` word alone. Saying that plainly is more use than
/// claiming the table was implemented.
///
/// A pending with no flag is unknown, not armed. The wait leaves the flag
/// off on three branches — armed and fresh, and either branch while
/// loading — so a flagless pending is three readings at once and proves
/// none of them.
///
/// A dormant heartbeat's age is never read as staleness. A dormant
/// session stops rewriting the file by design, so the age says when it
/// went quiet and nothing about whether it lives; the verdict this reads
/// carries no age at all, which is where that rule sits.
#[must_use]
pub fn bridge_of(
    ping: Option<&Result<crate::protocol::Envelope, crate::reads::Unanswered>>,
    beat: Option<&Beat>,
) -> BridgeAxis {
    use crate::reads::Unanswered;
    use crate::wait::Flag;
    match ping {
        // The reply came back and said `ok`, which is what makes it a
        // reply at all here: the session answered this tick.
        Some(Ok(_)) => BridgeAxis::Armed,
        Some(Err(Unanswered::Superseded { .. })) => BridgeAxis::Superseded,
        Some(Err(Unanswered::Pending {
            flag: Some(Flag::Waking),
            ..
        })) => BridgeAxis::Waking,
        Some(Err(Unanswered::Pending {
            flag: Some(Flag::Stalled),
            ..
        })) => BridgeAxis::Stalled,
        Some(Err(why)) => BridgeAxis::Unknown {
            why: Why::Unanswered { why: why.clone() },
        },
        // No window opened, so nothing was asked and the file is all
        // there is.
        None => match beat {
            Some(Beat::Ours { armed: true, .. }) => BridgeAxis::Armed,
            Some(Beat::Ours { armed: false, .. }) => BridgeAxis::Dormant,
            Some(Beat::Foreign { saw, wanted }) => BridgeAxis::Unknown {
                why: Why::ForeignHeartbeat {
                    saw: saw.clone(),
                    wanted: wanted.clone(),
                },
            },
            Some(Beat::WrongHost { saw, wanted }) => BridgeAxis::Unknown {
                why: Why::WrongHost {
                    saw: saw.clone(),
                    wanted: wanted.clone(),
                },
            },
            Some(Beat::Unreadable { detail }) => BridgeAxis::Unknown {
                why: Why::HeartbeatUnreadable {
                    detail: detail.clone(),
                },
            },
            None => BridgeAxis::Unknown {
                why: Why::NoHeartbeat,
            },
        },
    }
}

/// The facts, one after another, for a line that prints all of them.
fn joined(facts: &[Fact]) -> String {
    facts
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
#[allow(clippy::wildcard_enum_match_arm)]
mod game_state {
    use super::*;
    use crate::reads;

    /// A heartbeat verdict for this session on `host`, with the phase
    /// word as the host would write it.
    fn ours(host: Host, phase: &str, armed: bool) -> Beat {
        let parsed = phase_of(&host, phase);
        Beat::Ours {
            host,
            phase: parsed,
            armed,
            last_callback: None,
            callbacks: Vec::new(),
        }
    }

    /// A read that answered with the string `s`.
    fn said(s: &str) -> reads::Answer {
        reads::Answer::Value {
            lua_type: "string".to_owned(),
            value: Some(s.to_owned()),
        }
    }

    /// A read that answered with the boolean `b`.
    fn told(b: bool) -> reads::Answer {
        reads::Answer::Value {
            lua_type: "boolean".to_owned(),
            value: Some(b.to_string()),
        }
    }

    /// A read that threw inside its own `pcall`.
    fn raised() -> reads::Answer {
        reads::Answer::Raised {
            message: "attempt to call a nil value".to_owned(),
        }
    }

    #[test]
    fn every_phase_word_the_two_hosts_use_parses() {
        for (host, word, wanted) in [
            (Host::Hook, "menu", Phase::Menu),
            (Host::Hook, "load", Phase::Load),
            (Host::Hook, "sim", Phase::Sim),
            (Host::Hook, "paused", Phase::Paused),
            (Host::Export, "loaded", Phase::Loaded),
            (Host::Export, "sim", Phase::Sim),
            (Host::Export, "stopped", Phase::Stopped),
        ] {
            assert_eq!(phase_of(&host, word), wanted, "{host} said {word}");
        }
    }

    #[test]
    fn a_word_from_the_other_hosts_vocabulary_is_unrecognised() {
        // The two vocabularies overlap on `sim` alone, so a word is not
        // a phase on its own: it is a phase on a host.
        for (host, word) in [
            (Host::Hook, "loaded"),
            (Host::Hook, "stopped"),
            (Host::Export, "menu"),
            (Host::Export, "load"),
            (Host::Export, "paused"),
        ] {
            assert_eq!(
                phase_of(&host, word),
                Phase::Unrecognised(word.to_owned()),
                "{host} said {word} and it was taken for a phase"
            );
        }
        // And the one word both hosts hold is a phase on both.
        assert_eq!(phase_of(&Host::Hook, "sim"), Phase::Sim);
        assert_eq!(phase_of(&Host::Export, "sim"), Phase::Sim);
    }

    #[test]
    fn a_host_this_build_does_not_know_has_no_vocabulary() {
        let host = Host::named("mission");
        assert_eq!(host, Host::Other("mission".to_owned()));
        assert_eq!(
            phase_of(&host, "sim"),
            Phase::Unrecognised("sim".to_owned()),
            "an unknown host's word was read as a phase"
        );
    }

    #[test]
    fn an_unrecognised_phase_keeps_its_own_text() {
        let got = phase_of(&Host::Hook, "briefing");
        assert_eq!(got, Phase::Unrecognised("briefing".to_owned()));
        assert!(
            got.to_string().contains("briefing"),
            "the word was lost: {got}"
        );
    }

    #[test]
    fn the_load_word_is_the_waits_own() {
        // Not the literal `load`: the constant, so that the word which
        // decides a load here and the word which skips the reads window
        // cannot come to be spelt differently.
        assert_eq!(phase_of(&Host::Hook, PHASE_LOAD), Phase::Load);
    }

    #[test]
    fn a_load_phase_is_loading() {
        assert_eq!(
            activity_of(Some(&ours(Host::Hook, "load", true)), None),
            Activity::Loading
        );
    }

    #[test]
    fn a_load_phase_needs_no_mission_name_at_all() {
        // The phase is read off a file already on the disk, so the answer
        // is the same whatever the read says or does not say — which is
        // what makes a load cost no round trip.
        let beat = ours(Host::Hook, "load", true);
        for name in [
            None,
            Some(said("Caucasus TvT")),
            Some(said("")),
            Some(raised()),
        ] {
            assert_eq!(
                activity_of(Some(&beat), name.as_ref()),
                Activity::Loading,
                "a load was decided by the mission_name read"
            );
        }
    }

    #[test]
    fn a_sim_phase_with_a_named_mission_is_a_mission() {
        assert_eq!(
            activity_of(
                Some(&ours(Host::Hook, "sim", true)),
                Some(&said("Caucasus TvT"))
            ),
            Activity::Mission {
                name: "Caucasus TvT".to_owned()
            }
        );
    }

    #[test]
    fn a_paused_phase_with_a_named_mission_is_a_mission() {
        assert_eq!(
            activity_of(
                Some(&ours(Host::Hook, "paused", true)),
                Some(&said("Caucasus TvT"))
            ),
            Activity::Mission {
                name: "Caucasus TvT".to_owned()
            }
        );
    }

    #[test]
    fn a_sim_phase_with_an_empty_mission_name_is_unknown_and_shows_both() {
        let got = activity_of(Some(&ours(Host::Hook, "sim", true)), Some(&said("")));
        let Activity::Unknown {
            why: Why::Disagrees { facts },
        } = &got
        else {
            panic!("wanted a disagreement, got {got:?}");
        };
        assert_eq!(facts.len(), 2, "both facts are shown: {facts:?}");
        let said = got.to_string();
        assert!(said.contains("phase said sim"), "{said}");
        assert!(said.contains("mission_name said"), "{said}");
    }

    #[test]
    fn a_sim_phase_with_an_errored_mission_name_is_unknown_naming_the_error_alone() {
        // Alone: no phase beside it, no gate, nothing. An errored read
        // decides its axis and no other fact gets a say.
        let got = activity_of(Some(&ours(Host::Hook, "sim", true)), Some(&raised()));
        assert_eq!(
            got,
            Activity::Unknown {
                why: Why::Errored {
                    message: "attempt to call a nil value".to_owned()
                }
            }
        );
        assert_eq!(got.to_string(), "unknown: attempt to call a nil value");
    }

    #[test]
    fn a_sim_phase_with_an_unanswered_mission_name_is_unknown_and_not_errored() {
        let answer = reads::Answer::Unanswered {
            why: reads::Unanswered::Unyielded,
        };
        let got = activity_of(Some(&ours(Host::Hook, "sim", true)), Some(&answer));
        assert_eq!(
            got,
            Activity::Unknown {
                why: Why::Unanswered {
                    why: reads::Unanswered::Unyielded
                }
            },
            "a read that got no answer was read as one that raised"
        );
    }

    #[test]
    fn a_sim_phase_with_a_non_string_mission_name_is_unknown_naming_the_type() {
        let got = activity_of(Some(&ours(Host::Hook, "sim", true)), Some(&told(true)));
        assert_eq!(
            got,
            Activity::Unknown {
                why: Why::WrongType {
                    lua_type: "boolean".to_owned(),
                    value: Some("true".to_owned()),
                }
            }
        );
    }

    #[test]
    fn a_mission_name_at_the_menu_is_never_evidence_against_menu_or_editor() {
        // What `DCS.getMissionName()` gives at the menu has never been
        // measured — nothing, or the last mission flown — so a name there
        // is not evidence about anything.
        let beat = ours(Host::Hook, "menu", true);
        for name in [
            None,
            Some(said("Caucasus TvT")),
            Some(said("")),
            Some(raised()),
            Some(told(false)),
        ] {
            assert_eq!(
                activity_of(Some(&beat), name.as_ref()),
                Activity::MenuOrEditor,
                "the menu was argued out of by the mission_name read"
            );
        }
    }

    #[test]
    fn the_export_hosts_sim_admits_no_read_and_is_never_a_mission() {
        // `sim` is a word in both hosts' vocabularies, and on the export
        // host `DCS` is nil, so no read is possible there at all. An arm
        // keyed on the word rather than the host would make a mission out
        // of an answer that could not have come from that host.
        for word in ["loaded", "sim", "stopped"] {
            let got = activity_of(
                Some(&ours(Host::Export, word, true)),
                Some(&said("Caucasus TvT")),
            );
            assert_eq!(
                got,
                Activity::Unknown {
                    why: Why::NoReadPossible { host: Host::Export }
                },
                "the export host's {word} was read as {got:?}"
            );
        }
    }

    #[test]
    fn another_sessions_heartbeat_is_never_this_sessions_phase() {
        // The purest form of an axis filled from evidence that is not its
        // own: another install writing into the same output directory
        // would otherwise make this session's activity definite.
        let b = crate::testing::Sandbox::new();
        let mut s =
            crate::standin::Standin::open(&b.join("dcs"), "hook").expect("the stand-in opens");
        s.phase = "sim".to_owned();
        s.armed = true;
        s.beat(std::time::SystemTime::now()).expect("a beat lands");
        let hb = crate::readers::Heartbeat::read(&s.output().join("heartbeat.txt"))
            .expect("the heartbeat reads");

        let mine = Beat::verdict(&hb, &s.stamp, &Host::Hook);
        assert_eq!(
            activity_of(Some(&mine), Some(&said("Caucasus TvT"))),
            Activity::Mission {
                name: "Caucasus TvT".to_owned()
            },
            "the positive control: this session's own beat does decide"
        );

        let theirs = Beat::verdict(&hb, "1757160000-31244", &Host::Hook);
        assert_eq!(
            theirs,
            Beat::Foreign {
                saw: s.stamp.clone(),
                wanted: "1757160000-31244".to_owned(),
            }
        );
        assert_eq!(
            activity_of(Some(&theirs), Some(&said("Caucasus TvT"))),
            Activity::Unknown {
                why: Why::ForeignHeartbeat {
                    saw: s.stamp.clone(),
                    wanted: "1757160000-31244".to_owned(),
                }
            },
            "another session's phase was read as this session's"
        );
    }

    #[test]
    fn a_heartbeat_from_the_other_host_is_not_this_directorys() {
        let b = crate::testing::Sandbox::new();
        let mut s =
            crate::standin::Standin::open(&b.join("dcs"), "export").expect("the stand-in opens");
        s.phase = "sim".to_owned();
        s.beat(std::time::SystemTime::now()).expect("a beat lands");
        let hb = crate::readers::Heartbeat::read(&s.output().join("heartbeat.txt"))
            .expect("the heartbeat reads");
        let got = Beat::verdict(&hb, &s.stamp, &Host::Hook);
        assert_eq!(
            got,
            Beat::WrongHost {
                saw: "export".to_owned(),
                wanted: "hook".to_owned(),
            }
        );
    }

    #[test]
    fn an_unrecognised_phase_is_unknown_carrying_the_word() {
        let got = activity_of(Some(&ours(Host::Hook, "briefing", true)), None);
        assert_eq!(
            got,
            Activity::Unknown {
                why: Why::PhaseUnrecognised {
                    host: Host::Hook,
                    phase: "an unrecognised phase: briefing".to_owned(),
                }
            }
        );
        assert!(got.to_string().contains("briefing"), "{got}");
    }

    #[test]
    fn no_heartbeat_is_unknown_and_not_the_menu() {
        // Nothing on the disk is not the same as a session at the menu,
        // and the difference is the whole of what the reason is for.
        let got = activity_of(None, Some(&said("Caucasus TvT")));
        assert_eq!(
            got,
            Activity::Unknown {
                why: Why::NoHeartbeat
            }
        );
        assert_ne!(got, Activity::MenuOrEditor);
    }

    #[test]
    fn a_heartbeat_that_would_not_parse_is_its_own_reason() {
        let beat = Beat::Unreadable {
            detail: "no blank line before the bytes ran out".to_owned(),
        };
        let got = activity_of(Some(&beat), None);
        assert_eq!(
            got,
            Activity::Unknown {
                why: Why::HeartbeatUnreadable {
                    detail: "no blank line before the bytes ran out".to_owned()
                }
            },
            "an unreadable heartbeat was taken for an absent one"
        );
    }

    /// A session in a mission whose phase word is `phase`, and the
    /// activity that goes with it.
    fn in_mission(phase: &str) -> (Beat, Activity) {
        let beat = ours(Host::Hook, phase, true);
        let activity = activity_of(Some(&beat), Some(&said("Caucasus TvT")));
        assert_eq!(
            activity,
            Activity::Mission {
                name: "Caucasus TvT".to_owned()
            },
            "the fixture is not in a mission"
        );
        (beat, activity)
    }

    #[test]
    fn a_true_read_is_paused() {
        let (beat, activity) = in_mission("paused");
        let got = pause_of(&activity, Some(&beat), Some(&told(true)));
        assert_eq!(got.value, Pause::Paused);
        assert!(got.to_string().starts_with("paused (read)"), "{got}");
    }

    #[test]
    fn a_false_read_is_running() {
        let (beat, activity) = in_mission("sim");
        let got = pause_of(&activity, Some(&beat), Some(&told(false)));
        assert_eq!(got.value, Pause::Running);
    }

    #[test]
    fn a_paused_read_against_a_sim_phase_is_paused_read_with_the_phase_noted() {
        let (beat, activity) = in_mission("sim");
        let got = pause_of(&activity, Some(&beat), Some(&told(true)));
        assert_eq!(got.value, Pause::Paused, "the read did not win");
        assert_eq!(got.phase_callback, Some(Phase::Sim));
        let note = got.note.as_deref().expect("a disagreement is noted");
        assert!(
            note.contains("sim"),
            "the note does not say what the phase said: {note}"
        );
    }

    #[test]
    fn a_running_read_against_a_paused_phase_is_running_with_the_phase_noted() {
        let (beat, activity) = in_mission("paused");
        let got = pause_of(&activity, Some(&beat), Some(&told(false)));
        assert_eq!(got.value, Pause::Running, "the read did not win");
        assert_eq!(got.phase_callback, Some(Phase::Paused));
        assert!(got.note.is_some(), "the disagreement was not noted");
    }

    #[test]
    fn the_phase_never_resolves_the_disagreement_in_its_own_favour() {
        // Both directions, because a resolution that only ran one way
        // would leave the other test green. DCS can begin a mission
        // already paused without firing either callback, and can resume
        // with no preceding pause.
        for (phase, read, wanted) in [
            ("sim", true, Pause::Paused),
            ("paused", false, Pause::Running),
        ] {
            let (beat, activity) = in_mission(phase);
            let got = pause_of(&activity, Some(&beat), Some(&told(read)));
            assert_eq!(
                got.value, wanted,
                "the {phase} phase overruled a read of {read}"
            );
            assert!(got.note.is_some(), "and said nothing about disagreeing");
        }
    }

    #[test]
    fn an_agreeing_phase_leaves_no_note_and_is_still_shown() {
        for (phase, read, wanted) in [("paused", true, Phase::Paused), ("sim", false, Phase::Sim)] {
            let (beat, activity) = in_mission(phase);
            let got = pause_of(&activity, Some(&beat), Some(&told(read)));
            assert_eq!(got.note, None, "an agreement was noted as a disagreement");
            assert_eq!(
                got.phase_callback,
                Some(wanted),
                "the phase is shown whether or not it agrees"
            );
        }
    }

    #[test]
    fn pause_is_n_a_at_the_menu() {
        let beat = ours(Host::Hook, "menu", true);
        let activity = activity_of(Some(&beat), None);
        let got = pause_of(&activity, Some(&beat), Some(&told(true)));
        assert_eq!(
            got.value,
            Pause::NotApplicable,
            "the question does not arise outside a mission"
        );
    }

    #[test]
    fn pause_is_n_a_during_a_load() {
        let beat = ours(Host::Hook, "load", true);
        let activity = activity_of(Some(&beat), None);
        let got = pause_of(&activity, Some(&beat), None);
        assert_eq!(got.value, Pause::NotApplicable);
    }

    #[test]
    fn pause_is_unknown_naming_the_gate_where_activity_is_unknown() {
        // The gate selects among this axis's own values and never
        // supplies one: `activity` unknown makes `pause` unknown, and
        // says which gate it was.
        let got = pause_of(
            &Activity::Unknown {
                why: Why::NoHeartbeat,
            },
            None,
            Some(&told(true)),
        );
        assert_eq!(
            got.value,
            Pause::Unknown {
                why: Why::GateUnknown { gate: "activity" }
            }
        );
    }

    #[test]
    fn an_errored_pause_read_is_unknown_naming_the_error() {
        let (beat, activity) = in_mission("paused");
        let got = pause_of(&activity, Some(&beat), Some(&raised()));
        assert_eq!(
            got.value,
            Pause::Unknown {
                why: Why::Errored {
                    message: "attempt to call a nil value".to_owned()
                }
            },
            "the getPause read raised, so pause is unknown and the phase is not an answer"
        );
    }

    #[test]
    fn a_non_boolean_pause_read_is_unknown_naming_the_type() {
        let (beat, activity) = in_mission("sim");
        let got = pause_of(&activity, Some(&beat), Some(&said("yes")));
        assert_eq!(
            got.value,
            Pause::Unknown {
                why: Why::WrongType {
                    lua_type: "string".to_owned(),
                    value: Some("yes".to_owned()),
                }
            }
        );
    }

    #[test]
    fn an_unanswered_pause_read_is_unknown_and_not_errored() {
        let (beat, activity) = in_mission("sim");
        let answer = reads::Answer::Unanswered {
            why: reads::Unanswered::Unyielded,
        };
        let got = pause_of(&activity, Some(&beat), Some(&answer));
        assert_eq!(
            got.value,
            Pause::Unknown {
                why: Why::Unanswered {
                    why: reads::Unanswered::Unyielded
                }
            }
        );
    }

    /// A probe that came back with the refusal word `status`.
    fn refused_with(status: &str) -> reads::Probe {
        reads::Probe::Unanswered {
            why: reads::Unanswered::NotOk {
                status: status.to_owned(),
                stage: Some("eval".to_owned()),
                detail: "no".to_owned(),
            },
        }
    }

    #[test]
    fn a_refused_gui_probe_in_a_mission_is_session_client() {
        let (_, activity) = in_mission("sim");
        let got = session_of(
            &activity,
            Some(&refused_with("refused")),
            reads::Tiers::default(),
            None,
            None,
        );
        assert_eq!(got, SessionAxis::Client);
    }

    #[test]
    fn a_refused_probe_wins_over_the_tier_two_reads() {
        // Two rows of the vocabulary claim this one at once, and the
        // order is written down: the probe is read first.
        let (_, activity) = in_mission("sim");
        let got = session_of(
            &activity,
            Some(&refused_with("refused")),
            reads::Tiers::with_tier_two(),
            Some(&told(true)),
            Some(&told(false)),
        );
        assert_eq!(
            got,
            SessionAxis::Client,
            "the tier-2 reads overruled the probe"
        );
    }

    #[test]
    fn a_reachable_probe_with_tier_two_off_is_single_or_host() {
        let (_, activity) = in_mission("sim");
        let got = session_of(
            &activity,
            Some(&reads::Probe::Reachable),
            reads::Tiers::default(),
            None,
            None,
        );
        assert_eq!(got, SessionAxis::SingleOrHost);
        assert!(got.to_string().contains("tier 2 off"), "{got}");
    }

    #[test]
    fn an_invalid_state_probe_is_unknown_and_not_client() {
        let (_, activity) = in_mission("sim");
        let got = session_of(
            &activity,
            Some(&refused_with("invalid-state")),
            reads::Tiers::default(),
            None,
            None,
        );
        assert_ne!(got, SessionAxis::Client);
        assert!(
            got.to_string().contains("invalid-state"),
            "the word was lost: {got}"
        );
    }

    #[test]
    fn a_refusal_that_is_not_the_probes_is_unknown_and_not_client() {
        // `unsupported` where eval is off, a bad request, a refusal at
        // the mission hop: each says something about the request and
        // nothing about whether this session is a client.
        let (_, activity) = in_mission("sim");
        for word in ["unsupported", "bad-request", "oversize", "budget"] {
            let got = session_of(
                &activity,
                Some(&refused_with(word)),
                reads::Tiers::default(),
                None,
                None,
            );
            assert_ne!(got, SessionAxis::Client, "{word} was read as a client");
            assert!(got.to_string().contains(word), "{word} was lost: {got}");
        }
    }

    #[test]
    fn a_refused_probe_at_the_menu_is_unknown_and_the_facts_disagree() {
        let beat = ours(Host::Hook, "menu", true);
        let activity = activity_of(Some(&beat), None);
        let got = session_of(
            &activity,
            Some(&refused_with("refused")),
            reads::Tiers::default(),
            None,
            None,
        );
        let SessionAxis::Unknown {
            why: Why::Disagrees { facts },
        } = &got
        else {
            panic!("wanted a disagreement, got {got:?}");
        };
        assert_eq!(facts.len(), 2, "both facts are shown: {facts:?}");
        assert!(got.to_string().contains("the facts disagree"), "{got}");
    }

    #[test]
    fn a_refused_probe_at_the_menu_leaves_activity_alone() {
        // The probe is this axis's evidence. Unknowning `activity` with
        // it would be filling one axis from another's evidence, however
        // strong the instinct to unknown both.
        let beat = ours(Host::Hook, "menu", true);
        let activity = activity_of(Some(&beat), None);
        assert_eq!(activity, Activity::MenuOrEditor);
        let _ = session_of(
            &activity,
            Some(&refused_with("refused")),
            reads::Tiers::default(),
            None,
            None,
        );
        assert_eq!(
            activity_of(Some(&beat), None),
            Activity::MenuOrEditor,
            "the probe moved the activity axis"
        );
    }

    #[test]
    fn session_outside_a_mission_is_unknown_naming_the_gate() {
        let beat = ours(Host::Hook, "load", true);
        let activity = activity_of(Some(&beat), None);
        let got = session_of(
            &activity,
            Some(&reads::Probe::Reachable),
            reads::Tiers::default(),
            None,
            None,
        );
        assert_eq!(
            got,
            SessionAxis::Unknown {
                why: Why::GateUnknown { gate: "activity" }
            }
        );
    }

    #[test]
    fn tier_two_on_maps_multiplayer_false_to_single() {
        let (_, activity) = in_mission("sim");
        let got = session_of(
            &activity,
            Some(&reads::Probe::Reachable),
            reads::Tiers::with_tier_two(),
            Some(&told(false)),
            Some(&told(false)),
        );
        assert_eq!(got, SessionAxis::Single);
    }

    #[test]
    fn tier_two_on_maps_multiplayer_and_server_to_host() {
        let (_, activity) = in_mission("sim");
        let got = session_of(
            &activity,
            Some(&reads::Probe::Reachable),
            reads::Tiers::with_tier_two(),
            Some(&told(true)),
            Some(&told(true)),
        );
        assert_eq!(got, SessionAxis::Host);
    }

    #[test]
    fn a_combination_the_document_does_not_map_is_unknown_naming_both_reads() {
        // Multiplayer and not the server, with the gui state answering.
        // The vocabulary maps it to nothing; agreeing with the probe
        // here would be a guess wearing a corroboration.
        let (_, activity) = in_mission("sim");
        let got = session_of(
            &activity,
            Some(&reads::Probe::Reachable),
            reads::Tiers::with_tier_two(),
            Some(&told(true)),
            Some(&told(false)),
        );
        assert_eq!(
            got,
            SessionAxis::Unknown {
                why: Why::Unmapped {
                    facts: vec![
                        Fact::new("multiplayer", "true"),
                        Fact::new("server", "false"),
                    ]
                }
            }
        );
    }

    #[test]
    fn an_errored_tier_two_read_is_unknown_naming_the_error() {
        let (_, activity) = in_mission("sim");
        for (multi, server) in [(raised(), told(true)), (told(true), raised())] {
            let got = session_of(
                &activity,
                Some(&reads::Probe::Reachable),
                reads::Tiers::with_tier_two(),
                Some(&multi),
                Some(&server),
            );
            assert_eq!(
                got,
                SessionAxis::Unknown {
                    why: Why::Errored {
                        message: "attempt to call a nil value".to_owned()
                    }
                }
            );
        }
    }

    #[test]
    fn track_is_unknown_tier_2_off_when_the_switch_is_off() {
        // What a gather hands back for a tier-2 read with the switch off,
        // taken from the gather's own arm rather than composed here.
        let answer = reads::Answer::NotSent {
            why: reads::NotSent::TierTwoOff,
        };
        let got = track_of(Some(&answer));
        assert_eq!(
            got,
            Track::Unknown {
                why: Why::NotSent {
                    why: reads::NotSent::TierTwoOff
                }
            }
        );
        assert_eq!(got.to_string(), "unknown (tier 2 off)");
    }

    #[test]
    fn track_reads_replay_and_live_when_the_switch_is_on() {
        assert_eq!(track_of(Some(&told(true))), Track::Replay);
        assert_eq!(track_of(Some(&told(false))), Track::Live);
    }

    #[test]
    fn an_errored_track_read_is_unknown_naming_the_error() {
        assert_eq!(
            track_of(Some(&raised())),
            Track::Unknown {
                why: Why::Errored {
                    message: "attempt to call a nil value".to_owned()
                }
            }
        );
    }

    #[test]
    fn track_takes_no_gate_from_any_other_axis() {
        // The vocabulary gives this row no gate, so nothing but the read
        // reaches it: the same answer whatever the session is doing. The
        // signature is the check — there is nowhere to put an activity.
        let answer = told(true);
        assert_eq!(track_of(Some(&answer)), Track::Replay);
    }

    /// A handshake read off a stand-in's own output directory.
    fn found(s: &crate::standin::Standin) -> Found {
        let h = crate::readers::Handshake::read(&s.output().join("executor.txt"))
            .expect("the handshake reads");
        Found::Read(Box::new(h))
    }

    /// A ping that came back and said `ok`.
    fn answered_ping() -> Result<crate::protocol::Envelope, reads::Unanswered> {
        Ok(crate::protocol::parse(b"status: ok\n\n").expect("the reply parses"))
    }

    /// A ping still pending, with the wait's flag.
    fn pending_ping(
        flag: Option<crate::wait::Flag>,
    ) -> Result<crate::protocol::Envelope, reads::Unanswered> {
        Err(reads::Unanswered::Pending {
            id: "0000000001-ab".to_owned(),
            phase: "sim".to_owned(),
            flag,
        })
    }

    #[test]
    fn a_missing_handshake_is_never_ran() {
        assert_eq!(process_of(&Found::Missing, None), ProcessAxis::NeverRan);
    }

    #[test]
    fn an_unreadable_handshake_is_unknown_and_not_never_ran() {
        // Something loaded and wrote it, which is the opposite of never
        // having run.
        let got = process_of(
            &Found::Unreadable {
                path: std::path::PathBuf::from("executor.txt"),
                detail: "no blank line before the bytes ran out".to_owned(),
            },
            None,
        );
        assert_ne!(got, ProcessAxis::NeverRan);
        assert!(got.to_string().contains("executor.txt"), "{got}");
    }

    #[test]
    fn an_undecided_pid_probe_is_unknown_and_never_gone() {
        let b = crate::testing::Sandbox::new();
        let s = crate::standin::Standin::open(&b.join("dcs"), "hook").expect("it opens");
        s.handshake().expect("the handshake publishes");
        let probe = crate::status::Process::Undecided {
            why: "Access is denied.".to_owned(),
        };
        let got = process_of(&found(&s), Some(&probe));
        assert_ne!(got, ProcessAxis::Gone, "a refused handle was read as gone");
        assert!(got.to_string().contains("Access is denied."), "{got}");
    }

    #[test]
    fn a_running_pid_is_running() {
        let b = crate::testing::Sandbox::new();
        let s = crate::standin::Standin::open(&b.join("dcs"), "hook").expect("it opens");
        s.handshake().expect("the handshake publishes");
        assert_eq!(
            process_of(&found(&s), Some(&crate::status::Process::Running)),
            ProcessAxis::Running
        );
    }

    #[test]
    fn a_dormant_heartbeat_is_dormant() {
        // No window opened, so nothing was asked and the file is all
        // there is.
        assert_eq!(
            bridge_of(None, Some(&ours(Host::Hook, "menu", false))),
            BridgeAxis::Dormant
        );
        assert_eq!(
            bridge_of(None, Some(&ours(Host::Hook, "menu", true))),
            BridgeAxis::Armed
        );
    }

    #[test]
    fn a_dormant_heartbeats_age_is_not_read_as_staleness() {
        // A dormant session stops rewriting the file by design, so the
        // age says when it went quiet and nothing about whether it
        // lives. The verdict this axis reads carries no age at all,
        // which is where that rule sits rather than in a rule to
        // remember.
        let b = crate::testing::Sandbox::new();
        let mut s = crate::standin::Standin::open(&b.join("dcs"), "hook").expect("it opens");
        s.armed = false;
        let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(36_000);
        s.beat(long_ago).expect("a beat lands");
        let hb = crate::readers::Heartbeat::read(&s.output().join("heartbeat.txt"))
            .expect("the heartbeat reads");
        let beat = Beat::verdict(&hb, &s.stamp, &Host::Hook);
        assert_eq!(
            bridge_of(None, Some(&beat)),
            BridgeAxis::Dormant,
            "a ten-hour-old dormant beat was read as stale rather than dormant"
        );
    }

    #[test]
    fn a_superseded_ping_is_superseded() {
        let ping = Err(reads::Unanswered::Superseded {
            id: "0000000001-ab".to_owned(),
        });
        assert_eq!(bridge_of(Some(&ping), None), BridgeAxis::Superseded);
    }

    #[test]
    fn a_waking_pending_is_waking() {
        assert_eq!(
            bridge_of(Some(&pending_ping(Some(crate::wait::Flag::Waking))), None),
            BridgeAxis::Waking
        );
    }

    #[test]
    fn a_stalled_pending_is_stalled() {
        assert_eq!(
            bridge_of(Some(&pending_ping(Some(crate::wait::Flag::Stalled))), None),
            BridgeAxis::Stalled
        );
    }

    #[test]
    fn a_flagless_pending_is_unknown_and_not_armed() {
        // The wait leaves the flag off on three branches — armed and
        // fresh, and either branch while loading — so a flagless pending
        // is three readings at once and is proof of none of them.
        let got = bridge_of(
            Some(&pending_ping(None)),
            Some(&ours(Host::Hook, "sim", true)),
        );
        assert_ne!(
            got,
            BridgeAxis::Armed,
            "a flagless pending was read as armed"
        );
        assert!(matches!(got, BridgeAxis::Unknown { .. }), "{got:?}");
    }

    #[test]
    fn a_ping_that_answered_is_armed() {
        let ping = answered_ping();
        assert_eq!(bridge_of(Some(&ping), None), BridgeAxis::Armed);
    }

    #[test]
    fn the_derivation_and_status_read_one_set_of_files_alike() {
        // The obligation is that these two do not disagree about what the
        // same files mean. A process id nobody on this host holds is the
        // case where a disagreement would bite.
        let b = crate::testing::Sandbox::new();
        let s = crate::standin::Standin::open(&b.join("dcs"), "hook").expect("it opens");
        s.handshake().expect("the handshake publishes");
        let report = crate::status::status(s.output());
        let session = report.session.as_ref().expect("a session is reported");
        let got = process_of(&found(&s), Some(&session.process));
        let wanted = match &session.process {
            crate::status::Process::Running => ProcessAxis::Running,
            crate::status::Process::Exited => ProcessAxis::Gone,
            crate::status::Process::Undecided { .. } => {
                assert!(matches!(got, ProcessAxis::Unknown { .. }), "{got:?}");
                return;
            }
        };
        assert_eq!(
            got, wanted,
            "status said {} and the axis said {got}",
            session.process
        );
    }

    #[test]
    fn the_derivation_and_status_agree_a_foreign_heartbeat_is_not_ours() {
        // Where the obligation actually bites: `status` flags a foreign
        // stamp and the derivation must not read the phase off it anyway.
        let b = crate::testing::Sandbox::new();
        let mut s = crate::standin::Standin::open(&b.join("dcs"), "hook").expect("it opens");
        s.handshake().expect("the handshake publishes");
        let mine = s.stamp.clone();
        s.stamp = "1757160001-9999".to_owned();
        s.armed = true;
        s.phase = "sim".to_owned();
        s.beat(std::time::SystemTime::now()).expect("a beat lands");

        let report = crate::status::status(s.output());
        let beat_status = report
            .session
            .as_ref()
            .and_then(|session| session.beat.as_ref())
            .expect("a heartbeat is reported");
        assert!(
            !beat_status.belongs,
            "status did not flag the foreign stamp"
        );

        let hb = crate::readers::Heartbeat::read(&s.output().join("heartbeat.txt"))
            .expect("the heartbeat reads");
        let verdict = Beat::verdict(&hb, &mine, &Host::Hook);
        let got = activity_of(Some(&verdict), Some(&said("Caucasus TvT")));
        assert_eq!(
            got,
            Activity::Unknown {
                why: Why::ForeignHeartbeat {
                    saw: "1757160001-9999".to_owned(),
                    wanted: mine.clone(),
                }
            },
            "status says belongs: false and the derivation read the phase anyway"
        );
        assert_eq!(
            bridge_of(None, Some(&verdict)),
            BridgeAxis::Unknown {
                why: Why::ForeignHeartbeat {
                    saw: "1757160001-9999".to_owned(),
                    wanted: mine,
                }
            },
            "another session's armed word was read as this session's"
        );
    }

    #[test]
    fn a_raised_read_renders_as_unknown_naming_the_error_verbatim() {
        // Verbatim and alone: the message as the interpreter wrote it,
        // with no phase, no gate and no other fact alongside it.
        let why = Why::Errored {
            message: "attempt to call a nil value".to_owned(),
        };
        assert_eq!(why.to_string(), "unknown: attempt to call a nil value");
    }

    #[test]
    fn tier_two_off_renders_exactly_unknown_tier_2_off() {
        // The exact line, because it is what tells an agent the answer is
        // purchasable and how. A near miss would read as an ordinary
        // unknown.
        let why = Why::NotSent {
            why: reads::NotSent::TierTwoOff,
        };
        assert_eq!(why.to_string(), "unknown (tier 2 off)");
    }

    #[test]
    fn a_load_skip_and_a_tier_two_skip_do_not_render_alike() {
        // Both are reads that were never sent, and one of them is
        // purchasable by turning a switch on while the other is not.
        let load = Why::NotSent {
            why: reads::NotSent::Loading,
        };
        let tier = Why::NotSent {
            why: reads::NotSent::TierTwoOff,
        };
        assert_ne!(load, tier);
        assert_ne!(load.to_string(), tier.to_string());
        assert!(!load.to_string().contains("tier 2"), "{load}");
    }

    #[test]
    fn the_three_ways_there_is_no_phase_do_not_render_alike() {
        // Absent, unreadable and another session's are three findings:
        // the first is the ordinary state after a load, the second is a
        // file this build could not parse, and the third is two installs
        // writing into one directory.
        let all = [
            Why::NoHeartbeat,
            Why::HeartbeatUnreadable {
                detail: "no blank line before the bytes ran out".to_owned(),
            },
            Why::ForeignHeartbeat {
                saw: "1757160001-9999".to_owned(),
                wanted: "1757160000-31244".to_owned(),
            },
        ];
        for (i, one) in all.iter().enumerate() {
            for (j, other) in all.iter().enumerate() {
                assert_eq!(i == j, one == other, "{one:?} against {other:?}");
                assert_eq!(i == j, one.to_string() == other.to_string(), "{one}");
            }
        }
    }

    #[test]
    fn a_disagreement_prints_every_fact() {
        let why = Why::Disagrees {
            facts: vec![
                Fact::new("phase", "sim"),
                Fact::new("mission_name", "the empty string"),
            ],
        };
        let said = why.to_string();
        assert!(said.contains("the facts disagree"), "{said}");
        assert!(said.contains("phase said sim"), "{said}");
        assert!(
            said.contains("mission_name said the empty string"),
            "{said}"
        );
    }

    /// Every reason, one of each arm, for the sweep below and for the
    /// tests that want a reason to hand.
    fn every_reason() -> Vec<Why> {
        vec![
            Why::Errored {
                message: "attempt to call a nil value".to_owned(),
            },
            Why::NotSent {
                why: reads::NotSent::TierTwoOff,
            },
            Why::NotSent {
                why: reads::NotSent::Loading,
            },
            Why::Unanswered {
                why: reads::Unanswered::Unyielded,
            },
            Why::Malformed {
                body: b"not a read".to_vec(),
            },
            Why::WrongType {
                lua_type: "number".to_owned(),
                value: Some("42".to_owned()),
            },
            Why::WrongType {
                lua_type: "table".to_owned(),
                value: None,
            },
            Why::Disagrees {
                facts: vec![Fact::new("phase", "sim")],
            },
            Why::Unmapped {
                facts: vec![Fact::new("multiplayer", "true")],
            },
            Why::NoHeartbeat,
            Why::HeartbeatUnreadable {
                detail: "short".to_owned(),
            },
            Why::ForeignHeartbeat {
                saw: "1757160001-9999".to_owned(),
                wanted: "1757160000-31244".to_owned(),
            },
            Why::WrongHost {
                saw: "export".to_owned(),
                wanted: "hook".to_owned(),
            },
            Why::Unreadable {
                path: std::path::PathBuf::from("executor.txt"),
                detail: "short".to_owned(),
            },
            Why::PhaseUnrecognised {
                host: Host::Hook,
                phase: "briefing".to_owned(),
            },
            Why::NoReadPossible { host: Host::Export },
            Why::GateUnknown { gate: "activity" },
            Why::NotProbed,
        ]
    }

    #[test]
    fn every_reason_is_distinct_in_value_and_in_prose() {
        // In value *and* in prose: two reasons that print alike leave a
        // reader unable to tell them apart, which is the whole of what
        // carrying a reason is for.
        let all = every_reason();
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

    #[test]
    fn every_reason_says_unknown() {
        for why in every_reason() {
            assert!(
                why.to_string().starts_with("unknown"),
                "a reason that does not say unknown: {why}"
            );
        }
    }

    #[test]
    fn no_two_phases_render_alike() {
        let all = [
            Phase::Menu,
            Phase::Load,
            Phase::Sim,
            Phase::Paused,
            Phase::Loaded,
            Phase::Stopped,
            Phase::Unrecognised("briefing".to_owned()),
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
}
