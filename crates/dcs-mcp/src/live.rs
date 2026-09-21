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

use std::io::Write;
use std::path::PathBuf;

use crate::serve::{self, Host, Serve, host_of};
use crate::tools;

/// The usage line, which is also the list of phases this verb answers to.
pub const USAGE: &str = "usage: dcs-mcp live report\n       \
     --saved-games <dir> --variant <name> [--host hook|export] [--data-dir <dir>]";

/// Which phase a line asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Report,
}

/// The phase a word names, or nothing where it names none.
fn phase_of(word: &str) -> Option<Phase> {
    match word {
        "report" => Some(Phase::Report),
        _ => None,
    }
}

/// One command line, read.
struct Parsed {
    phase: Phase,
    opts: serve::Options,
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

/// Read a command line, `live` and the phase first.
fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Parsed, String> {
    let mut args = args.into_iter().peekable();
    if args.peek().is_some_and(|word| word == "live") {
        args.next();
    }
    let word = args
        .next()
        .ok_or_else(|| "live wants a phase: report".to_owned())?;
    let phase = phase_of(&word).ok_or_else(|| format!("live does not take {word}"))?;

    let mut saved_games = None;
    let mut variant = None;
    let mut host = None;
    let mut data_dir = None;
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
    })
}

/// Run one phase and print what it came to.
///
/// The exit code: 0 for a phase that did what it was asked, 1 for one that
/// refused or found a problem, and 2 — the caller's, from the `Err` here —
/// for a line that would not parse.
pub fn run<I: IntoIterator<Item = String>>(args: I, out: &mut dyn Write) -> Result<i32, String> {
    let parsed = parse(args)?;
    let serve = Serve::new(parsed.opts);
    let data = match tools::data_dir(&serve) {
        Ok(data) => data,
        Err(why) => {
            writeln!(out, "refused: {why}").map_err(|why| why.to_string())?;
            return Ok(1);
        }
    };
    match parsed.phase {
        Phase::Report => {
            let (entries, malformed) = match ledger::read(&data) {
                Ok(read) => read,
                Err(why) => {
                    writeln!(out, "refused: {}: {why}", data.live_path().display())
                        .map_err(|why| why.to_string())?;
                    return Ok(1);
                }
            };
            report::print(&entries, &malformed, out).map_err(|why| why.to_string())?;
            Ok(i32::from(!malformed.is_empty()))
        }
    }
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
