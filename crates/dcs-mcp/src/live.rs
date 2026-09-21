//! The live-run instrument: the measurements only a running game can give,
//! taken one phase at a time and kept in a ledger of their own.
//!
//! It is a verb of this binary rather than a script or a second executable.
//! A script driving the command line would start a process per request, so
//! every round trip it timed would include a process start-up; and a second
//! executable is a second thing to package for no gain over a verb that
//! already parses where the install is. The figures have to be taken again
//! after every DCS update, so the instrument ships to users with the rest.

// The append-only record of what each phase measured.
pub mod ledger;

// Every row the instrument knows of, printed measured or not.
pub mod report;

// What a frame costs while nobody is asking, timed inside DCS.
mod dormant;

// Round trips, the executor's own cost of each, and replies per frame.
mod rtt;

// One opt-in read, alone, once per DCS session.
mod read;

use std::io::Write;
use std::path::PathBuf;

use crate::register::DataDir;
use crate::serve::{self, Host, Serve, host_of};
use crate::tools;
use ledger::{Entry, Session};

/// The usage line, which is also the list of phases this verb answers to.
pub const USAGE: &str = "usage: dcs-mcp live dormant | rtt | read <key> | report\n       \
     --saved-games <dir> --variant <name> [--host hook|export] [--data-dir <dir>]\n       \
     [--wait-seconds <n>] [--count <n>] [--label <word>]";

/// How many requests `rtt` times per state, each way, unless told otherwise.
const DEFAULT_COUNT: usize = 200;

/// Which phase a line asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Dormant,
    Rtt,
    Read,
    Report,
}

/// The phase a word names, or nothing where it names none.
fn phase_of(word: &str) -> Option<Phase> {
    match word {
        "dormant" => Some(Phase::Dormant),
        "rtt" => Some(Phase::Rtt),
        "read" => Some(Phase::Read),
        "report" => Some(Phase::Report),
        _ => None,
    }
}

/// One command line, read.
struct Parsed {
    phase: Phase,
    opts: serve::Options,
    wait_seconds: Option<u64>,
    count: usize,
    /// The scene the maintainer says the game is in, recorded on each entry.
    label: Option<String>,
    /// The opt-in read `read` sends. Empty for the other phases.
    key: String,
}

/// Fill a slot that has not been filled, or name the flag that filled it.
/// The rule every other verb keeps: a flag given twice has two opinions, and
/// last-wins would act on one of them silently.
fn once<T>(slot: &mut Option<T>, flag: &str, value: T) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!("{flag} is given twice"));
    }
    *slot = Some(value);
    Ok(())
}

/// A count, refused by name rather than defaulted where it will not parse.
fn number(flag: &str, given: &str) -> Result<u64, String> {
    given
        .parse()
        .map_err(|_| format!("{flag} wants a whole number, not {given}"))
}

/// Read a command line, `live` and the phase first.
fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Parsed, String> {
    let mut args = args.into_iter().peekable();
    if args.peek().is_some_and(|word| word == "live") {
        args.next();
    }
    let word = args
        .next()
        .ok_or_else(|| "live wants a phase: dormant, rtt, read or report".to_owned())?;
    let phase = phase_of(&word).ok_or_else(|| format!("live does not take {word}"))?;

    let mut saved_games = None;
    let mut variant = None;
    let mut host = None;
    let mut data_dir = None;
    let mut wait_seconds = None;
    let mut count = None;
    let mut label = None;
    let mut key = None;
    while let Some(arg) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .ok_or_else(|| format!("{name} wants a value after it"))
        };
        match arg.as_str() {
            "--saved-games" => once(
                &mut saved_games,
                "--saved-games",
                PathBuf::from(value("--saved-games")?),
            )?,
            "--variant" => once(&mut variant, "--variant", value("--variant")?)?,
            // The dormant rows are the hook's frame, and the opt-in reads are
            // the hook state's: sent to the export host, a read comes back
            // unsupported and a dormant figure would overwrite the hook's
            // row under a label that still names the hook.
            "--host" if matches!(phase, Phase::Dormant | Phase::Read) => {
                return Err(format!(
                    "live {word} measures the hook host only and takes no --host; \
                     --host is for rtt"
                ));
            }
            "--host" => {
                let given = value("--host")?;
                let picked = host_of(&given)
                    .ok_or_else(|| format!("--host is hook or export, not {given}"))?;
                once(&mut host, "--host", picked)?
            }
            "--data-dir" => once(
                &mut data_dir,
                "--data-dir",
                PathBuf::from(value("--data-dir")?),
            )?,
            "--wait-seconds" if phase != Phase::Report => {
                let given = value("--wait-seconds")?;
                once(
                    &mut wait_seconds,
                    "--wait-seconds",
                    number("--wait-seconds", &given)?,
                )?
            }
            "--count" if phase == Phase::Rtt => {
                let given = value("--count")?;
                let n = number("--count", &given)?;
                if n == 0 {
                    return Err("--count wants at least 1".to_owned());
                }
                once(&mut count, "--count", n)?
            }
            "--label" if phase != Phase::Report => once(&mut label, "--label", value("--label")?)?,
            given if phase == Phase::Read && !given.starts_with("--") => {
                once(&mut key, "the read's key", given.to_owned())?
            }
            other => return Err(format!("live {word} does not take {other}")),
        }
    }
    Ok(Parsed {
        phase,
        opts: serve::Options {
            saved_games: saved_games.ok_or("live wants --saved-games <dir>")?,
            variant: variant.ok_or("live wants --variant <name>")?,
            host: host.unwrap_or(Host::Hook),
            data_dir,
        },
        wait_seconds,
        count: count.map_or(DEFAULT_COUNT, |n| usize::try_from(n).unwrap_or(usize::MAX)),
        label,
        key: match (phase, key) {
            (Phase::Read, None) => return Err("live read wants the key of one opt-in read".into()),
            (_, key) => key.unwrap_or_default(),
        },
    })
}

/// Append each entry to the ledger and print it, stopping at the first that
/// will not append: a figure printed and not kept is one the report will
/// call unmeasured.
fn recorded(entries: &[Entry], data: &DataDir, out: &mut dyn Write) -> Result<i32, String> {
    for e in entries {
        if let Err(why) = ledger::append(data, e) {
            writeln!(out, "refused: {}: {why}", data.live_path().display())
                .map_err(|why| why.to_string())?;
            return Ok(1);
        }
        writeln!(out, "{}: {}", e.row, e.said).map_err(|why| why.to_string())?;
    }
    Ok(0)
}

/// Run one phase and print what it came to.
///
/// The exit code: 0 for a phase that did what it was asked, 1 for one that
/// refused or found a problem, and 2 — the caller's, from the `Err` here —
/// for a line that would not parse.
pub fn run<I: IntoIterator<Item = String>>(args: I, out: &mut dyn Write) -> Result<i32, String> {
    let parsed = parse(args)?;
    let serve = Serve::new(parsed.opts.clone());
    let refused = |out: &mut dyn Write, why: String| {
        writeln!(out, "refused: {why}")
            .map(|()| 1)
            .map_err(|why| why.to_string())
    };
    let data = match tools::data_dir(&serve) {
        Ok(data) => data,
        Err(why) => return refused(out, why.to_string()),
    };
    if parsed.phase == Phase::Report {
        let (entries, malformed) = match ledger::read(&data) {
            Ok(read) => read,
            Err(why) => return refused(out, format!("{}: {why}", data.live_path().display())),
        };
        report::print(&entries, &malformed, out).map_err(|why| why.to_string())?;
        return Ok(i32::from(!malformed.is_empty()));
    }
    let client = match serve.client() {
        Ok(client) => client,
        Err(why) => return refused(out, format!("no session: {why}")),
    };
    let h = client.handshake();
    let session = Session {
        stamp: h.stamp.clone(),
        host: h.host.clone(),
        app_version: h.app_version.clone(),
        scene: parsed.label.clone(),
    };
    let upto = tools::waiting(parsed.wait_seconds);
    let entries = match parsed.phase {
        Phase::Dormant => dormant::run(h, &session, upto),
        Phase::Rtt => rtt::run(h, &session, parsed.count, upto),
        Phase::Read => {
            // Every line of the ledger, or no read: a line that will not
            // parse may be the read this session already had.
            let rows = match ledger::read(&data) {
                Ok((rows, malformed)) if malformed.is_empty() => rows,
                Ok(_) => {
                    return refused(
                        out,
                        format!(
                            "{} holds a line that is not an entry, so it cannot prove this \
                             session has had no opt-in read",
                            data.live_path().display()
                        ),
                    );
                }
                Err(why) => {
                    return refused(
                        out,
                        format!(
                            "{}: {why}, so it cannot prove this session has had no opt-in read",
                            data.live_path().display()
                        ),
                    );
                }
            };
            match read::run(h, &session, &parsed.key, rows, &data, upto) {
                Ok(entries) => entries,
                Err(why) => return refused(out, why),
            }
        }
        Phase::Report => Vec::new(),
    };
    recorded(&entries, &data, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Sandbox;

    fn line(b: &Sandbox, words: &[&str]) -> Vec<String> {
        let mut all: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
        for word in [
            "--saved-games",
            &b.path.to_string_lossy(),
            "--variant",
            "DCS",
            "--data-dir",
            &b.join("data").to_string_lossy(),
        ] {
            all.push(word.to_owned());
        }
        all
    }

    #[test]
    fn an_unknown_phase_is_refused_by_name() {
        let b = Sandbox::new();
        let err = run(line(&b, &["live", "census"]), &mut Vec::new())
            .expect_err("an unknown phase will not parse");
        assert_eq!(err, "live does not take census");
    }

    #[test]
    fn a_flag_given_twice_is_refused() {
        let b = Sandbox::new();
        let mut words = line(&b, &["live", "report"]);
        words.push("--variant".to_owned());
        words.push("DCS".to_owned());
        let err = run(words, &mut Vec::new()).expect_err("a repeat will not parse");
        assert_eq!(err, "--variant is given twice");
    }

    #[test]
    fn dormant_and_read_refuse_another_host() {
        let b = Sandbox::new();
        for words in [
            &["live", "dormant", "--host", "export"][..],
            &["live", "read", "server", "--host", "export"][..],
        ] {
            let err = run(line(&b, words), &mut Vec::new()).expect_err("--host will not parse");
            assert!(err.contains("measures the hook host only"), "{err}");
        }
    }

    #[test]
    fn the_report_over_a_fresh_data_directory_prints_every_row() {
        let b = Sandbox::new();
        b.dir("data");
        let mut out = Vec::new();
        let code = run(line(&b, &["live", "report"]), &mut out).expect("the line parses");
        assert_eq!(code, 0);
        let text = String::from_utf8(out).expect("the report is text");
        assert_eq!(text.lines().count(), report::rows().len(), "{text}");
    }
}
