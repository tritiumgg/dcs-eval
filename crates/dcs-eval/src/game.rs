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
        }
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
