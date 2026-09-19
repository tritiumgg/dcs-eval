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

#[cfg(test)]
#[allow(clippy::wildcard_enum_match_arm)]
mod game_state {
    use super::*;

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
