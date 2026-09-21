//! The dormant phase: what a frame costs while nobody is asking, taken as
//! the cost of that frame's own operations, inside DCS (ADR 0025).
//!
//! It sends one chunk, five times, each request alone, to the host's own
//! local state: the executor times the stat a dormant frame makes, the
//! listing an armed-idle frame makes, and the floor of the callback wrapper,
//! against an empty call it subtracts. The median of the five is what the
//! rows print. The chunk holds the game for about a quarter of a second each
//! time, which is why the README says to run this at the menu or paused.
//!
//! The chunk is sent unbounded. Its loops end on its own clock or its own
//! cap, and a count hook would sit on the very path being timed. With every
//! loop run to its cap the chunk comes to about the executor's own ceiling,
//! so a budget would not have bounded it much tighter; the ADR holds the
//! count.

use std::time::Duration;

use dcs_eval::pipeline::{Pipeline, Spec};
use dcs_eval::readers::Handshake;
use dcs_eval::wait::Outcome;

use super::ledger::{Entry, Session};

/// The chunk, as it is sent.
const CHUNK: &str = include_str!("dormant.lua");

/// How many times it is sent, for a median with a spread behind it.
const SAMPLES: usize = 5;

/// The incumbent's per-frame listing, in milliseconds, which every row here
/// is set against.
const BASELINE_MS: f64 = 0.098;

/// One answer of the chunk, in microseconds per call.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Probe {
    pub absent: f64,
    pub present: f64,
    pub list: f64,
    pub pcall: f64,
    pub empty: f64,
    /// The loops the cap ended before the clock did, or `none`.
    pub capped: String,
}

/// The chunk's answer read, or nothing where it is not the chunk's grammar:
/// five `<name>_us <number>` lines in order, then `capped <names>`.
pub(crate) fn parse(body: &str) -> Option<Probe> {
    let mut lines = body.lines();
    let mut figure = |name: &str| -> Option<f64> {
        let (tag, value) = lines.next()?.split_once(' ')?;
        (tag == name).then(|| value.parse().ok()).flatten()
    };
    let probe = Probe {
        absent: figure("absent_us")?,
        present: figure("present_us")?,
        list: figure("list_us")?,
        pcall: figure("pcall_us")?,
        empty: figure("empty_us")?,
        capped: String::new(),
    };
    let capped = lines.next()?.strip_prefix("capped ")?.to_owned();
    if lines.next().is_some() {
        return None;
    }
    Some(Probe { capped, ..probe })
}

/// The per-frame figures one probe gives, in microseconds: the dormant
/// frame, which is the wrapper floor and one absent stat every
/// `probe_every` frames, and the armed-idle frame's listing alone. Each is
/// net of the empty call's cost.
pub(crate) fn per_frame(p: &Probe, probe_every: u64) -> (f64, f64) {
    let wrapper = p.pcall - p.empty;
    let stat = p.absent - p.empty;
    let dormant = wrapper + stat / probe_every.max(1) as f64;
    (dormant, p.list - p.empty)
}

/// The middle value.
fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// One request's answer, or what came back instead, in a row's words.
fn sent(h: &Handshake, state: &str, upto: Duration) -> Result<Probe, String> {
    let spec = Spec::new(
        &[
            ("op", "eval"),
            ("for", h.stamp.as_str()),
            ("state", state),
            ("chunkname", "=dcs-eval live dormant"),
            ("max_instructions", "0"),
        ],
        CHUNK.as_bytes(),
    );
    match Pipeline::over(h, vec![spec], 1, upto).next() {
        Some(Ok(Outcome::Reply(envelope))) => {
            let status = envelope.headers.get("status").unwrap_or_default();
            let body = String::from_utf8_lossy(&envelope.body).into_owned();
            if status != "ok" {
                let stage = envelope
                    .headers
                    .get("stage")
                    .map(|s| format!(" at {s}"))
                    .unwrap_or_default();
                return Err(format!("refused: {status}{stage}: {body}"));
            }
            parse(&body).ok_or_else(|| {
                let excerpt: String = body.chars().take(80).collect();
                format!("malformed: {excerpt}")
            })
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

/// Whether a per-frame figure holds the baseline, in words.
fn against(ms: f64) -> &'static str {
    if ms <= BASELINE_MS {
        "at or below the 0.098 ms baseline"
    } else {
        "above the 0.098 ms baseline, which reopens the choice of transport"
    }
}

/// The three rows this phase takes.
pub(crate) fn run(h: &Handshake, session: &Session, upto: Duration) -> Vec<Entry> {
    // The host's own local state is the one named for the host.
    let state = session.host.as_str();
    let mut probes = Vec::new();
    let mut stopped = None;
    for _ in 0..SAMPLES {
        match sent(h, state, upto) {
            Ok(p) => probes.push(p),
            Err(why) => {
                stopped = Some(why);
                break;
            }
        }
    }
    let entry = |row: &str, said: String| Entry::new("dormant", row, session, said);
    if probes.is_empty() {
        let why = stopped.unwrap_or_default();
        return ["dormant.dormant", "dormant.armed_idle", "dormant.listing"]
            .iter()
            .map(|row| entry(row, why.clone()))
            .collect();
    }
    let frames: Vec<(f64, f64)> = probes.iter().map(|p| per_frame(p, h.probe_every)).collect();
    let dormant_us = median(frames.iter().map(|f| f.0).collect());
    let idle_us = median(frames.iter().map(|f| f.1).collect());
    let absent_us = median(probes.iter().map(|p| p.absent - p.empty).collect());
    let wrapper_us = median(probes.iter().map(|p| p.pcall - p.empty).collect());
    let present_us = median(probes.iter().map(|p| p.present - p.empty).collect());
    let mut tail = format!("; median of {}", probes.len());
    let capped: Vec<&str> = probes
        .iter()
        .map(|p| p.capped.as_str())
        .filter(|c| *c != "none")
        .collect();
    if !capped.is_empty() {
        tail.push_str(&format!("; capped before the clock: {}", capped.join(" ")));
    }
    if let Some(why) = &stopped {
        tail.push_str(&format!("; stopped: {why}"));
    }
    let (dormant_ms, idle_ms) = (dormant_us / 1000.0, idle_us / 1000.0);
    vec![
        entry(
            "dormant.dormant",
            format!(
                "{dormant_ms:.4} ms per frame, {}: the wrapper floor {wrapper_us:.3} µs and the \
                 absent stat {absent_us:.3} µs every {} frames{tail}",
                against(dormant_ms),
                h.probe_every
            ),
        )
        .with("ms", dormant_ms)
        .with("wrapper_us", wrapper_us)
        .with("absent_us", absent_us)
        .with("probe_every", h.probe_every),
        entry(
            "dormant.armed_idle",
            format!(
                "{idle_ms:.4} ms per frame, {}: the listing alone, the armed frame's clock \
                 reads and tables untimed{tail}",
                against(idle_ms)
            ),
        )
        .with("ms", idle_ms),
        entry(
            "dormant.listing",
            format!(
                "{idle_ms:.4} ms per listing here, where S4 measured 0.098 ms; the stat on a \
                 present file {present_us:.3} µs{tail}"
            ),
        )
        .with("ms", idle_ms)
        .with("present_us", present_us),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::ledger;
    use crate::register::DataDir;
    use crate::serve::{Host, Options};
    use crate::testing::{Sandbox, driven, ticking};
    use dcs_eval::standin::Standin;

    const BODY: &str =
        "absent_us 0.9\npresent_us 1.1\nlist_us 98.1\npcall_us 0.2\nempty_us 0.1\ncapped none";

    #[test]
    fn parses_the_probe_grammar() {
        assert_eq!(
            parse(BODY),
            Some(Probe {
                absent: 0.9,
                present: 1.1,
                list: 98.1,
                pcall: 0.2,
                empty: 0.1,
                capped: "none".to_owned(),
            })
        );
        assert_eq!(parse("absent_us 0.9"), None);
        assert_eq!(parse(&format!("{BODY}\nmore")), None);
    }

    #[test]
    fn dormant_per_frame_is_the_wrapper_plus_the_absent_stat_over_probe_every() {
        let p = parse(BODY).expect("the grammar");
        let (dormant, idle) = per_frame(&p, 8);
        assert!((dormant - 0.2).abs() < 1e-9, "{dormant}");
        assert!((idle - 98.0).abs() < 1e-9, "{idle}");
        assert_eq!(format!("{:.4}", dormant / 1000.0), "0.0002");
    }

    /// `live dormant` run against a stand-in, what it printed, and the
    /// ledger after.
    fn dormant(b: &Sandbox, s: &mut Standin) -> (i32, Vec<ledger::Entry>) {
        let line: Vec<String> = [
            "live",
            "dormant",
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
        let (code, _) = driven(s, |out| {
            crate::live::run(line, out).expect("the line parses")
        });
        let data = DataDir::at(&b.join("data"), &[]).expect("the data directory resolves");
        (code, ledger::read(&data).expect("the ledger reads").0)
    }

    fn standin(b: &Sandbox) -> Standin {
        let opts = Options {
            saved_games: b.path.clone(),
            variant: "DCS".to_owned(),
            host: Host::Hook,
            data_dir: Some(b.dir("data")),
        };
        let mut s = Standin::open(&opts.output(), "hook").expect("the stand-in opens");
        ticking(&mut s);
        s
    }

    #[test]
    fn five_answers_give_the_three_rows() {
        let b = Sandbox::new();
        let mut s = standin(&b);
        s.script("absent_us", "ok", "string", BODY.as_bytes());
        let (code, entries) = dormant(&b, &mut s);
        assert_eq!(code, 0);
        assert_eq!(
            s.seen().len(),
            5,
            "the chunk was sent {} times",
            s.seen().len()
        );
        let rows: Vec<&str> = entries.iter().map(|e| e.row.as_str()).collect();
        assert_eq!(
            rows,
            ["dormant.dormant", "dormant.armed_idle", "dormant.listing"]
        );
        // The stand-in's handshake says probe_every 8: the wrapper floor
        // 0.1 µs plus the absent stat 0.8 µs over 8 frames is 0.2 µs, where
        // one frame in one would be 0.9 µs.
        assert!(
            entries[0].said.starts_with("0.0002 ms per frame"),
            "{}",
            entries[0].said
        );
        assert_eq!(entries[0].figures["probe_every"], 8);
        assert!(
            entries[1].said.starts_with("0.0980 ms per frame"),
            "{}",
            entries[1].said
        );
    }

    #[test]
    fn a_body_off_the_grammar_is_recorded_malformed() {
        let b = Sandbox::new();
        let mut s = standin(&b);
        s.script("absent_us", "ok", "string", b"nonsense");
        let (_, entries) = dormant(&b, &mut s);
        assert_eq!(entries.len(), 3);
        for e in &entries {
            assert_eq!(e.said, "malformed: nonsense", "{}", e.row);
        }
        assert_eq!(s.seen().len(), 1, "a malformed answer stops the phase");
    }

    #[test]
    fn the_probe_is_sent_unbounded_and_publishes_no_read() {
        let b = Sandbox::new();
        let mut s = standin(&b);
        s.script("absent_us", "ok", "string", BODY.as_bytes());
        dormant(&b, &mut s);
        assert!(!s.seen().is_empty());
        for seen in s.seen() {
            let text = String::from_utf8_lossy(&seen.bytes);
            assert!(
                text.lines().any(|l| l.trim_end() == "max_instructions: 0"),
                "{text}"
            );
            assert!(
                !text.contains("DCS.") && !text.contains("net."),
                "the probe sent a read: {text}"
            );
        }
    }
}
