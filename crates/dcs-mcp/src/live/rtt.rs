//! The round-trip phase: how long a request takes, what the executor says
//! handling it cost, and how many replies one frame gives when the window is
//! full, for every state the session serves.
//!
//! It sends `return 1` and nothing else. The chunk is the least a request can
//! ask, so what is timed is the transport and the dispatch and not a chunk's
//! work, and no read of the game rides along with it. A trailing comment names
//! the state, so each body is distinct and a stand-in can answer one state
//! without matching another.
//!
//! Per state: one request first to wake a dormant executor, not counted; then
//! `count` requests one after another, each timed from before it is published
//! to its parsed reply; then `count` more through one window eight deep, whose
//! replies are counted against the distinct `tick` values they carry. A state
//! stops at its first answer that is not `ok`, and nothing more is sent into
//! it: a refusal is that state's finding, and pressing on would only repeat
//! it.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use dcs_eval::pipeline::{DEFAULT_DEPTH, Pipeline, Spec};
use dcs_eval::protocol::Envelope;
use dcs_eval::readers::Handshake;
use dcs_eval::wait::Outcome;

use super::ledger::{Entry, Session};

/// Replies in one seven-state generation, from the incumbent's own capture.
const GENERATION_REPLIES: f64 = 13_605.0;

/// The states a session serves, read off its handshake's `states` line:
/// entries separated by whitespace, each a name then a colon.
fn states(h: &Handshake) -> Vec<String> {
    h.states
        .split_whitespace()
        .filter_map(|entry| entry.split(':').next())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The one request this phase sends to `state`.
fn spec(h: &Handshake, state: &str) -> Spec {
    let body = format!("return 1 -- live rtt [{state}]");
    Spec::new(
        &[
            ("op", "eval"),
            ("for", h.stamp.as_str()),
            ("state", state),
            ("chunkname", "=dcs-eval live rtt"),
        ],
        body.as_bytes(),
    )
}

/// The `ok` reply a window item carries, or what came back instead, in the
/// words a row prints.
fn ok(item: Option<Result<Outcome, dcs_eval::pipeline::PipeError>>) -> Result<Envelope, String> {
    match item {
        Some(Ok(Outcome::Reply(envelope))) => {
            let status = envelope.headers.get("status").unwrap_or_default();
            if status == "ok" {
                return Ok(envelope);
            }
            let stage = envelope
                .headers
                .get("stage")
                .map(|s| format!(" at {s}"))
                .unwrap_or_default();
            Err(format!("refused: {status}{stage}"))
        }
        Some(Ok(Outcome::Pending { id, .. })) => {
            Err(format!("pending: {id} did not answer within the wait"))
        }
        Some(Ok(Outcome::Superseded { id } | Outcome::Dead { id })) => {
            Err(format!("session gone before it answered, id {id}"))
        }
        Some(Err(why)) => Err(why.to_string()),
        None => Err("the window yielded nothing".to_owned()),
    }
}

/// The value at percentile `p` of `sorted`, by nearest rank: the smallest
/// value at least `p` percent of the sample is at or below.
pub(crate) fn nearest_rank(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

/// Sorted, for the percentiles.
fn sorted(mut v: Vec<f64>) -> Vec<f64> {
    v.sort_by(f64::total_cmp);
    v
}

/// What one state came to.
struct Measured {
    /// Client-side round trips, milliseconds.
    trips: Vec<f64>,
    /// The executor's own `cpu_ms`, one per sequential reply.
    cpu: Vec<f64>,
    /// Replies and distinct ticks through the full window, and its wall time.
    window: Option<(usize, usize, Duration)>,
    /// Why the state stopped early, where it did.
    stopped: Option<String>,
}

/// Measure one state.
fn measure(h: &Handshake, state: &str, count: usize, upto: Duration) -> Measured {
    let mut m = Measured {
        trips: Vec::new(),
        cpu: Vec::new(),
        window: None,
        stopped: None,
    };
    // The wake, not counted: a dormant executor takes up to a probe interval
    // to notice the arm file, and that is not a round trip.
    if let Err(why) = ok(Pipeline::over(h, vec![spec(h, state)], 1, upto).next()) {
        m.stopped = Some(why);
        return m;
    }
    for _ in 0..count {
        let began = Instant::now();
        let reply = ok(Pipeline::over(h, vec![spec(h, state)], 1, upto).next());
        let took = began.elapsed();
        match reply {
            Ok(envelope) => {
                m.trips.push(took.as_secs_f64() * 1000.0);
                if let Some(cpu) = envelope.headers.get("cpu_ms").and_then(|c| c.parse().ok()) {
                    m.cpu.push(cpu);
                }
            }
            Err(why) => {
                m.stopped = Some(format!("{why}, after {} replies", m.trips.len()));
                return m;
            }
        }
    }
    let specs = (0..count).map(|_| spec(h, state)).collect();
    let began = Instant::now();
    let mut ticks = BTreeSet::new();
    let mut replies = 0;
    for item in Pipeline::over(h, specs, DEFAULT_DEPTH, upto) {
        match ok(Some(item)) {
            Ok(envelope) => {
                replies += 1;
                ticks.insert(envelope.headers.get("tick").unwrap_or_default().to_owned());
            }
            Err(why) => {
                m.stopped = Some(format!("{why}, after {replies} replies in the window"));
                return m;
            }
        }
    }
    m.window = Some((replies, ticks.len(), began.elapsed()));
    m
}

/// Every row the phase takes, measured over every state the session serves.
pub(crate) fn run(h: &Handshake, session: &Session, count: usize, upto: Duration) -> Vec<Entry> {
    let host = session.host.as_str();
    let mut entries = Vec::new();
    for state in states(h) {
        let m = measure(h, &state, count, upto);
        let key = |what: &str| format!("{what}.{host}.{state}");
        let entry = |what: &str, said: String| Entry::new("rtt", &key(what), session, said);
        let trips = sorted(m.trips.clone());
        let cpu = sorted(m.cpu.clone());
        let stop = m.stopped.clone();

        entries.push(match (&stop, trips.is_empty()) {
            (Some(why), true) => entry("round_trip", why.clone()),
            _ => {
                let (p50, p95) = (nearest_rank(&trips, 50.0), nearest_rank(&trips, 95.0));
                let mut said = format!(
                    "p50 {p50:.1} ms, p95 {p95:.1} ms over {} sequential `return 1`",
                    trips.len()
                );
                if let Some(why) = &stop {
                    said.push_str(&format!("; stopped: {why}"));
                }
                entry("round_trip", said)
                    .with("p50_ms", p50)
                    .with("p95_ms", p95)
                    .with("n", trips.len())
            }
        });
        entries.push(match (&stop, cpu.is_empty()) {
            (Some(why), true) => entry("cpu_ms", why.clone()),
            (None, true) => entry("cpu_ms", "no reply carried a cpu_ms".to_owned()),
            _ => {
                let (p50, p95) = (nearest_rank(&cpu, 50.0), nearest_rank(&cpu, 95.0));
                entry(
                    "cpu_ms",
                    format!(
                        "p50 {p50:.3} ms, p95 {p95:.3} ms over {} replies: `return 1`, the \
                         handling floor; a consumer's page is the consumer's figure, unmeasured",
                        cpu.len()
                    ),
                )
                .with("p50_ms", p50)
                .with("p95_ms", p95)
                .with("n", cpu.len())
            }
        });
        let window = m.window.map(|(replies, ticks, wall)| {
            let per_tick = replies as f64 / ticks.max(1) as f64;
            let per_second = replies as f64 / wall.as_secs_f64().max(f64::EPSILON);
            (replies, ticks, per_tick, per_second)
        });
        entries.push(match (window, &stop) {
            (Some((replies, ticks, per_tick, per_second)), _) => entry(
                "per_tick",
                format!(
                    "{per_tick:.2} replies per tick ({replies} over {ticks} ticks), \
                     {per_second:.1} replies/s"
                ),
            )
            .with("replies", replies)
            .with("ticks", ticks)
            .with("per_tick", per_tick)
            .with("per_second", per_second),
            (None, Some(why)) => entry("per_tick", why.clone()),
            (None, None) => entry("per_tick", "the window was not run".to_owned()),
        });

        if host == "hook" && state == "missionscripting" {
            let said = match (&stop, trips.is_empty()) {
                (Some(why), true) => why.clone(),
                _ => format!(
                    "answered through a_do_script, p50 {:.1} ms",
                    nearest_rank(&trips, 50.0)
                ),
            };
            entries.push(Entry::new(
                "rtt",
                "missionscripting.a_do_script",
                session,
                said,
            ));
        }
        if host == "hook"
            && state == "hook"
            && let Some((_, _, _, per_second)) = window
        {
            let seconds = GENERATION_REPLIES / per_second;
            entries.push(
                Entry::new(
                    "rtt",
                    "generation",
                    session,
                    format!(
                        "projected floor {seconds:.0} s: 13,605 `return 1` round trips at the \
                         hook state's W=8 throughput of {per_second:.1} replies/s; only the hook \
                         state is measured against a seven-state total, and a consumer's real \
                         generation is unmeasured"
                    ),
                )
                .with("seconds", seconds)
                .with("per_second", per_second),
            );
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::ledger;
    use crate::register::DataDir;
    use crate::serve::{Host, Options};
    use crate::testing::{Sandbox, driven, ticking};
    use dcs_eval::standin::Standin;

    #[test]
    fn nearest_rank_percentiles() {
        let v: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(nearest_rank(&v, 50.0), 50.0);
        assert_eq!(nearest_rank(&v, 95.0), 95.0);
        assert_eq!(nearest_rank(&[7.0], 95.0), 7.0);
    }

    fn opts(b: &Sandbox) -> Options {
        Options {
            saved_games: b.path.clone(),
            variant: "DCS".to_owned(),
            host: Host::Hook,
            data_dir: Some(b.dir("data")),
        }
    }

    /// `live rtt` run to the end against a stand-in, and the ledger after.
    fn rtt(b: &Sandbox, s: &mut Standin, count: &str) -> (i32, String, Vec<Entry>) {
        let line: Vec<String> = [
            "live",
            "rtt",
            "--count",
            count,
            "--wait-seconds",
            "5",
            "--saved-games",
            &b.path.to_string_lossy(),
            "--variant",
            "DCS",
            "--data-dir",
            &b.join("data").to_string_lossy(),
        ]
        .iter()
        .map(|w| (*w).to_owned())
        .collect();
        let (code, shown) = driven(s, |out| {
            crate::live::run(line, out).expect("the line parses")
        });
        let data = DataDir::at(&b.join("data"), &[]).expect("the data directory resolves");
        let (entries, malformed) = ledger::read(&data).expect("the ledger reads");
        assert!(malformed.is_empty(), "{malformed:?}");
        (code, shown, entries)
    }

    fn standin(b: &Sandbox) -> Standin {
        let mut s = Standin::open(&opts(b).output(), "hook").expect("the stand-in opens");
        ticking(&mut s);
        s
    }

    fn said<'a>(entries: &'a [Entry], row: &str) -> &'a str {
        entries
            .iter()
            .find(|e| e.row == row)
            .map(|e| e.said.as_str())
            .unwrap_or_else(|| panic!("no entry for {row}"))
    }

    #[test]
    fn a_row_per_state_the_session_answers() {
        let b = Sandbox::new();
        let mut s = standin(&b);
        for state in [
            "gui",
            "scripting",
            "mission",
            "config",
            "export",
            "missionscripting",
        ] {
            s.script(&format!("[{state}]"), "ok", "number", b"1");
        }
        let (code, _, entries) = rtt(&b, &mut s, "3");
        assert_eq!(code, 0);
        for state in [
            "hook",
            "gui",
            "scripting",
            "mission",
            "config",
            "export",
            "missionscripting",
        ] {
            let trip = said(&entries, &format!("round_trip.hook.{state}"));
            assert!(trip.starts_with("p50 "), "{state}: {trip}");
            // The stand-in stamps every reply `cpu_ms: 0.000`, so each of
            // the three sequential replies carries one.
            let row = format!("cpu_ms.hook.{state}");
            let cpu = said(&entries, &row);
            assert!(cpu.starts_with("p50 "), "{state}: {cpu}");
            let n = entries
                .iter()
                .find(|e| e.row == row)
                .map(|e| &e.figures["n"]);
            assert_eq!(n, Some(&serde_json::Value::from(3)), "{state}");
            let tick = said(&entries, &format!("per_tick.hook.{state}"));
            assert!(tick.contains("replies per tick"), "{state}: {tick}");
        }
        assert!(said(&entries, "missionscripting.a_do_script").starts_with("answered"));
        assert!(said(&entries, "generation").starts_with("projected floor"));
    }

    #[test]
    fn a_state_stops_at_its_first_refusal() {
        let b = Sandbox::new();
        let mut s = standin(&b);
        s.script(
            "[missionscripting]",
            "no-mission",
            "",
            b"no mission is loaded",
        );
        let (_, _, entries) = rtt(&b, &mut s, "3");
        let sent = s
            .seen()
            .iter()
            .filter(|seen| String::from_utf8_lossy(&seen.bytes).contains("[missionscripting]"))
            .count();
        assert_eq!(sent, 1, "the refused state was sent {sent} requests");
        assert_eq!(
            said(&entries, "missionscripting.a_do_script"),
            "refused: no-mission"
        );
        assert_eq!(
            said(&entries, "round_trip.hook.missionscripting"),
            "refused: no-mission"
        );
    }

    #[test]
    fn replies_per_tick_counts_distinct_ticks() {
        let b = Sandbox::new();
        let mut s = standin(&b);
        let (_, _, entries) = rtt(&b, &mut s, "8");
        let e = entries
            .iter()
            .find(|e| e.row == "per_tick.hook.hook")
            .expect("the hook state's row");
        let per_tick = e.figures["per_tick"].as_f64().expect("a figure");
        assert!(per_tick > 1.0 && per_tick <= 8.0, "{per_tick}");
    }

    #[test]
    fn rtt_publishes_no_read() {
        let b = Sandbox::new();
        let mut s = standin(&b);
        rtt(&b, &mut s, "2");
        assert!(!s.seen().is_empty());
        for seen in s.seen() {
            let text = String::from_utf8_lossy(&seen.bytes);
            assert!(
                !text.contains("DCS.") && !text.contains("net."),
                "rtt sent a read: {text}"
            );
        }
    }
}
