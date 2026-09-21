//! The report: every Stage 9 figure this instrument knows of, a line each,
//! measured or not.
//!
//! **The rows are a table in this file, never what the ledger happens to
//! hold.** A report built from the ledger would print only what was
//! measured, and a row nobody measured would vanish rather than say so,
//! which is the one failure an instrument must not have. So every row is
//! here whether or not the ledger has a figure for it, and a row without
//! one prints `unmeasured` and why. A few rows are not measurements at all
//! and print what they are instead: a figure that is zero by construction,
//! and the steps only a person at the game can take.
//!
//! Each row names the figure it is set against, taken from the incumbent's
//! own recorded runs. Where the incumbent recorded nothing, the row says so
//! rather than inventing a target.

use std::io::{self, Write};

use super::ledger::Entry;

/// The executor's states, per host, as the report lists them: the hook
/// host's seven in the order its handshake names them, then the export
/// host's one.
pub const STATES: [(&str, &str); 8] = [
    ("hook", "hook"),
    ("hook", "gui"),
    ("hook", "scripting"),
    ("hook", "mission"),
    ("hook", "config"),
    ("hook", "export"),
    ("hook", "missionscripting"),
    ("export", "export"),
];

/// What a row prints when the ledger holds no figure for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Absent {
    /// Nothing in this build takes the figure yet, and why.
    NotBuilt(&'static str),
    /// The phase that takes it has not been run.
    Run(&'static str),
    /// Not a measurement: what the row is, whole.
    Said(&'static str),
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Which ledger entries are this row's.
    pub key: String,
    /// What the row is called where it is printed.
    pub label: String,
    /// The recorded figure it is set against, or why there is none.
    pub baseline: &'static str,
    /// What it prints with no figure.
    pub absent: Absent,
}

fn row(
    key: impl Into<String>,
    label: impl Into<String>,
    baseline: &'static str,
    absent: Absent,
) -> Row {
    Row {
        key: key.into(),
        label: label.into(),
        baseline,
        absent,
    }
}

const ROUND_TRIP: &str =
    "the Node client's 30 ms p50 / 37 ms p95; S4's in-Lua 14 ms median / 30 ms p95";
const NO_COST: &str = "none recorded: the incumbent's replies carried no cost";
const NO_PER_TICK: &str = "none recorded";
const GENERATION: &str = "465 s: 13,605 replies at 34.2 ms, round trips alone";
const LISTING: &str =
    "0.098 ms per frame, the incumbent's listing (S4: one machine, DCS 2.9.28.26385, one session)";
const NONE: &str = "nothing recorded";

/// Every row, in the order the report prints them.
pub fn rows() -> Vec<Row> {
    let rtt = |host: &str| match host {
        "export" => Absent::Run("rtt --host export"),
        _ => Absent::Run("rtt"),
    };
    let mut rows = Vec::new();
    for (host, state) in STATES {
        rows.push(row(
            format!("round_trip.{host}.{state}"),
            format!("round trip, {state} ({host} host)"),
            ROUND_TRIP,
            rtt(host),
        ));
    }
    for (host, state) in STATES {
        rows.push(row(
            format!("cpu_ms.{host}.{state}"),
            format!("cpu_ms per reply, {state} ({host} host)"),
            NO_COST,
            rtt(host),
        ));
    }
    for (host, state) in STATES {
        rows.push(row(
            format!("per_tick.{host}.{state}"),
            format!("replies per tick at W=8, {state} ({host} host)"),
            NO_PER_TICK,
            rtt(host),
        ));
    }
    rows.push(row(
        "generation",
        "seven-state generation",
        GENERATION,
        rtt("hook"),
    ));

    let dormant = Absent::NotBuilt("no phase takes it yet");
    rows.push(row(
        "dormant.absent",
        "dormant cost, hook absent",
        LISTING,
        Absent::Said(
            "0 by construction, not a measurement: what DCS pays to call a registered hook \
             is the whole-frame row's",
        ),
    ));
    rows.push(row(
        "dormant.dormant",
        "dormant cost, installed-dormant",
        LISTING,
        dormant,
    ));
    rows.push(row(
        "dormant.armed_idle",
        "dormant cost, armed-idle",
        LISTING,
        dormant,
    ));
    rows.push(row(
        "dormant.listing",
        "the incumbent's listing, on this machine",
        LISTING,
        dormant,
    ));

    rows.push(row(
        "missionscripting.a_do_script",
        "missionscripting through a_do_script, a mission loaded",
        NONE,
        rtt("hook"),
    ));
    rows.push(row(
        "missionscripting.s17",
        "the s17 flag agreement",
        NONE,
        Absent::NotBuilt("the s17 fixture mission is not ported"),
    ));

    let read = Absent::NotBuilt("no phase takes it yet");
    for key in [
        "multiplayer",
        "server",
        "track",
        "player_id",
        "mission_loaded",
        "player_unit_type",
        "mission_theatre",
    ] {
        rows.push(row(
            format!("read.{key}"),
            format!("opt-in read {key}, sent alone"),
            "none: never sent from a hook",
            read,
        ));
    }

    let scene = Absent::NotBuilt("the scene phase is a follow-up");
    for label in ["menu", "editor", "mission"] {
        rows.push(row(
            format!("scene.sim_mode.{label}"),
            format!("sim_mode raw, {label}"),
            NONE,
            scene,
        ));
    }
    rows.push(row(
        "scene.editor_vs_menu",
        "editor told from the menu",
        NONE,
        scene,
    ));
    rows.push(row(
        "scene.mission_name.menu",
        "mission_name at the menu",
        NONE,
        scene,
    ));
    rows.push(row("scene.callbacks", "callbacks seen", NONE, scene));

    rows.push(row(
        "by_hand.cutover",
        "the cutover from the incumbent",
        NONE,
        Absent::Said(
            "by hand: the incumbent's hook and its Export.lua line removed before install; \
             nothing checks it",
        ),
    ));
    rows.push(row(
        "by_hand.permanent",
        "permanent installation",
        NONE,
        Absent::Said("by hand: install, fly dormant, survive a DCS update, verify green"),
    ));
    rows.push(row(
        "by_hand.whole_frame",
        "dormant cost, whole frame, three ways",
        LISTING,
        Absent::Said(
            "by hand: not taken by this instrument (ADR 0025); the permanent-installation \
             acceptance's \"no noticeable frame impact\" is the whole-frame judgement",
        ),
    ));
    rows
}

/// What a row with no figure says.
fn unmeasured(row: &Row) -> String {
    let said = match row.absent {
        Absent::NotBuilt(why) => format!("unmeasured: not built ({why})"),
        Absent::Run(phase) => format!("unmeasured: run `dcs-mcp live {phase}`"),
        Absent::Said(what) => what.to_owned(),
    };
    format!("{}: {said}; against {}", row.label, row.baseline)
}

/// What a row with a figure says, and where the figure came from.
fn measured(row: &Row, e: &Entry) -> String {
    let scene = e
        .scene
        .as_deref()
        .map(|s| format!("{s}, "))
        .unwrap_or_default();
    let version = e.app_version.as_deref().unwrap_or("DCS version unread");
    format!(
        "{}: {}; against {} [{scene}{version}, {} {}]",
        row.label, e.said, row.baseline, e.host, e.at
    )
}

/// The newest entry the ledger holds for a row.
fn latest<'a>(row: &Row, entries: &'a [Entry]) -> Option<&'a Entry> {
    entries.iter().rev().find(|e| e.row == row.key)
}

/// Print every row, then every entry no row owns, then every ledger line
/// that would not parse.
pub fn print(entries: &[Entry], malformed: &[String], out: &mut dyn Write) -> io::Result<()> {
    let rows = rows();
    for row in rows.iter() {
        match latest(row, entries) {
            Some(e) => writeln!(out, "{}", measured(row, e))?,
            None => writeln!(out, "{}", unmeasured(row))?,
        }
    }
    // A figure the table has no row for is still printed. It is not dropped
    // any more than an unmeasured row is.
    for e in entries
        .iter()
        .filter(|e| rows.iter().all(|r| r.key != e.row))
    {
        writeln!(out, "{} (no row in the table): {}", e.row, e.said)?;
    }
    for line in malformed {
        writeln!(out, "problem: a ledger line that is not an entry: {line}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::ledger::Session;

    fn printed(entries: &[Entry], malformed: &[String]) -> String {
        let mut out = Vec::new();
        print(entries, malformed, &mut out).expect("the report prints");
        String::from_utf8(out).expect("the report is text")
    }

    fn entry(row: &str, said: &str) -> Entry {
        let session = Session {
            stamp: "0000000001-abcd".to_owned(),
            host: "hook".to_owned(),
            app_version: Some("2.9.28.26385".to_owned()),
            scene: Some("menu".to_owned()),
        };
        Entry::new("rtt", row, &session, said)
    }

    #[test]
    fn every_row_prints_over_an_empty_ledger() {
        let text = printed(&[], &[]);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 47, "the report printed {} lines", lines.len());
        for row in rows() {
            assert!(
                lines
                    .iter()
                    .any(|l| l.starts_with(&format!("{}: ", row.label))),
                "no line for {}",
                row.label
            );
        }
        for line in lines {
            assert!(
                line.contains("unmeasured")
                    || line.contains("not a measurement")
                    || line.contains("by hand"),
                "a line with no figure claims one: {line}"
            );
        }
    }

    #[test]
    fn a_measured_round_trip_prints_against_the_30_ms_baseline() {
        let text = printed(
            &[entry("round_trip.hook.hook", "p50 12.0 ms, p95 20.0 ms")],
            &[],
        );
        let line = text
            .lines()
            .find(|l| l.starts_with("round trip, hook (hook host): "))
            .expect("the row prints");
        assert!(line.contains("p50 12.0 ms"), "{line}");
        assert!(line.contains("30 ms p50"), "{line}");
        assert!(!line.contains("unmeasured"), "{line}");
    }

    #[test]
    fn the_latest_entry_for_a_row_wins() {
        let text = printed(
            &[
                entry("generation", "older figure"),
                entry("generation", "newer figure"),
            ],
            &[],
        );
        let line = text
            .lines()
            .find(|l| l.starts_with("seven-state generation: "))
            .expect("the row prints");
        assert!(line.contains("newer figure"), "{line}");
    }

    #[test]
    fn a_malformed_ledger_line_is_printed_as_a_problem() {
        let text = printed(&[], &["{half".to_owned()]);
        assert!(
            text.lines()
                .any(|l| l == "problem: a ledger line that is not an entry: {half"),
            "{text}"
        );
    }

    #[test]
    fn an_entry_no_row_owns_is_printed_rather_than_dropped() {
        let text = printed(&[entry("round_trip.hook.server", "p50 1.0 ms")], &[]);
        assert!(
            text.contains("round_trip.hook.server (no row in the table): p50 1.0 ms"),
            "{text}"
        );
    }
}
