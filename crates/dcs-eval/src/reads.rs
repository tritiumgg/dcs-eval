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

#[cfg(test)]
mod game_reads {
    use super::*;

    /// The five tier-1 callees, spelt as the frozen text spells them.
    fn tier_one() -> Vec<&'static str> {
        listed(Tiers::default())
            .iter()
            .map(|r| r.callee())
            .collect()
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
