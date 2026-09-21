//! The installer's verbs, from a terminal.
//!
//! Nothing here decides what a placement, a removal or a report is. Those are
//! `install`, `export_line`, `uninstall` and `verify`, each tested on its own;
//! a verb here is the order their steps run in and the words that say what
//! each one did. So a verb holds no rule a test of the library would miss.
//!
//! The installer never prompts. A question is asked by refusing: which
//! variant, when `Saved Games` holds more than one, and whether to move a
//! file this project did not ship. The refusal exits 1, names what it found
//! and the flag that answers it, and writes nothing first (ADR 0024). The
//! verbs look at no hook but our own, because the modules they call look at
//! none (ADR 0022).
//!
//! Everything a verb answers goes to stdout, the build's release line first,
//! so an agent reads one stream. A line that will not parse is the caller's
//! to print, with the usage, and exit 2.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use dcs_eval::paths::Real;

use crate::embed;
use crate::export_line::{self, Outcome};
use crate::install::{self, Disposition, Executor, Placed};
use crate::locate::{LocateError, SavedGames, Variant};
use crate::register::DataDir;
use crate::serve::{self, Host, host_of};
use crate::uninstall::{self, LineOutcome, Removed};
use crate::verify;

/// The usage line, which is also the list of what this module answers to.
pub const USAGE: &str = "usage: dcs-mcp install | verify | uninstall\n       \
     [--saved-games <dir>] [--variant <name>] [--replace] [--host hook|export]\n       \
     [--data-dir <dir>]";

/// What was asked for. One of three, and never a word `cli` owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verb {
    Install,
    Verify,
    Uninstall,
}

impl Verb {
    /// The word it is spelt with, for a refusal to name it back.
    fn word(self) -> &'static str {
        match self {
            Verb::Install => "install",
            Verb::Verify => "verify",
            Verb::Uninstall => "uninstall",
        }
    }
}

/// The verb a word names, or nothing where it names none.
fn verb_of(word: &str) -> Option<Verb> {
    match word {
        "install" => Some(Verb::Install),
        "verify" => Some(Verb::Verify),
        "uninstall" => Some(Verb::Uninstall),
        _ => None,
    }
}

/// Whether this module is the one that answers a word.
pub fn takes(word: &str) -> bool {
    verb_of(word).is_some()
}

/// One command line, read. Nothing in it is resolved yet: a line is read
/// whole before anything on disk is looked at, so a line that will not parse
/// never reaches the known folder.
struct Parsed {
    verb: Verb,
    saved_games: Option<PathBuf>,
    variant: Option<String>,
    replace: bool,
    host: Host,
    data_dir: Option<PathBuf>,
}

/// Fill a slot that has not been filled, or name the flag that filled it.
fn once<T>(slot: &mut Option<T>, flag: &str, value: T) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!("{flag} is given twice"));
    }
    *slot = Some(value);
    Ok(())
}

/// Read a command line, the verb first.
///
/// Each verb takes only the flags it acts on, and refuses the rest by name
/// rather than accepting and ignoring them: `--replace` is `install`'s alone,
/// `--host` is `verify`'s, and `verify` reads no register, so it takes no
/// `--data-dir`.
fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Parsed, String> {
    let mut args = args.into_iter();
    let word = args.next().ok_or_else(|| "a verb is wanted".to_owned())?;
    let verb = verb_of(&word).ok_or_else(|| format!("dcs-mcp does not take {word}"))?;

    let mut saved_games = None;
    let mut variant = None;
    let mut host = None;
    let mut data_dir = None;
    let mut replace = false;

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
            // Refused by name rather than as an unknown flag, because the
            // installer's own line once listed it. There is one question it
            // would answer, and it answers every one added later too
            // (ADR 0024).
            "--yes" => {
                return Err(format!(
                    "{} does not take --yes: the one question install asks is whether to move \
                     a file it did not ship, and --replace is its answer",
                    verb.word()
                ));
            }
            "--replace" if verb == Verb::Install => {
                if replace {
                    return Err("--replace is given twice".to_owned());
                }
                replace = true;
            }
            "--host" if verb == Verb::Verify => {
                let given = value("--host")?;
                let picked = host_of(&given)
                    .ok_or_else(|| format!("--host is hook or export, not {given}"))?;
                once(&mut host, "--host", picked)?
            }
            "--data-dir" if verb == Verb::Verify => {
                return Err("verify does not take --data-dir: it reads no register".to_owned());
            }
            "--data-dir" => once(
                &mut data_dir,
                "--data-dir",
                PathBuf::from(value("--data-dir")?),
            )?,
            other => return Err(format!("{} does not take {other}", verb.word())),
        }
    }

    Ok(Parsed {
        verb,
        saved_games,
        variant,
        replace,
        host: host.unwrap_or(Host::Hook),
        data_dir,
    })
}

/// `Saved Games` and the one variant in it this line means, or the refusal
/// that says why there is not one.
///
/// The install is passed as unknown: nothing on this line names it, so the
/// rule that fires is the containment one, and a variant anywhere but under
/// `Saved Games` is refused however it is spelled.
fn located(parsed: &Parsed) -> Result<(SavedGames, Variant), String> {
    let root = match &parsed.saved_games {
        Some(path) => SavedGames::at(path),
        None => SavedGames::known(),
    }
    .map_err(|why| asked(&why))?;
    let variant = root
        .target(parsed.variant.as_deref(), None)
        .map_err(|why| asked(&why))?;
    Ok((root, variant))
}

/// A locate refusal in words, with the flag that answers it where it is a
/// question rather than a fault.
fn asked(why: &LocateError) -> String {
    match why {
        LocateError::Ambiguous { .. } => {
            format!("{why}; name one with --variant <name>; the installer asks no other way")
        }
        _ => why.to_string(),
    }
}

/// The data directory this line names, or the known one, judged against the
/// variant by the one rule every other verb judges it by. Nothing is made:
/// a refused directory leaves nothing behind.
fn data_for(parsed: &Parsed, variant: &Variant) -> Result<DataDir, String> {
    match &parsed.data_dir {
        Some(path) => DataDir::at(path, &[&variant.path]),
        None => DataDir::known(&[&variant.path]),
    }
    .map_err(|why| why.to_string())
}

/// A verb that stops, in the one shape each of them stops in.
fn stopped(out: &mut dyn Write, head: &str, why: &str) -> io::Result<i32> {
    writeln!(out, "{head}: {why}")?;
    Ok(1)
}

/// Place the hook, then the line, then say what happens next.
///
/// A refused hook stops the run before `Export.lua` is read. A line that
/// fails after the hook went in is reported after what was done, and nothing
/// is rolled back: the hook is in the register, and `uninstall` takes it out.
fn installing(parsed: &Parsed, out: &mut dyn Write, exe: &Path) -> io::Result<i32> {
    const HEAD: &str = "not installed";
    writeln!(out, "{}", embed::release_line())?;
    let (root, variant) = match located(parsed) {
        Ok(found) => found,
        Err(why) => return stopped(out, HEAD, &why),
    };
    writeln!(out, "variant: {}", variant.path)?;
    let data = match data_for(parsed, &variant) {
        Ok(data) => data,
        Err(why) => return stopped(out, HEAD, &why),
    };

    let now = SystemTime::now();
    let replace = parsed.replace;
    let placed =
        match install::place_hook(now, &variant.path, &data, &Executor::embedded(), replace) {
            Ok(placed) => placed,
            Err(why) => return stopped(out, HEAD, &why.to_string()),
        };
    writeln!(out, "{}", said_hook(&placed))?;
    let line = match export_line::ensure(&variant.path, &data, now) {
        Ok(line) => line,
        Err(why) => return stopped(out, HEAD, &why.to_string()),
    };
    writeln!(
        out,
        "{}",
        said_line(&line, &export_line::path(&variant.path))
    )?;
    writeln!(out, "installed")?;

    // The `serve` line names the root as it resolved and the variant as it is
    // spelled on disk, never the name as typed and never the variant's
    // parent: `serve` joins the two itself, and a junction moves the parent.
    let data_named = named_data(parsed, &data);
    writeln!(out)?;
    writeln!(
        out,
        "next: DCS loads Scripts\\Hooks\\ when it starts and Export.lua when a mission starts, \
         so the executor answers after DCS next starts."
    )?;
    writeln!(
        out,
        "then: dcs-mcp verify --saved-games \"{}\" --variant \"{}\"",
        root.root(),
        variant.name
    )?;
    writeln!(out, "register the server with your MCP client:")?;
    writeln!(
        out,
        "{}",
        registration(exe, root.root(), &variant.name, data_named)
    )?;
    Ok(0)
}

/// Report the installation and the session, and write nothing.
///
/// The report opens with the release line itself, so it is not printed here
/// a second time; a variant that cannot be found prints it, and then why.
fn verifying(parsed: &Parsed, out: &mut dyn Write) -> io::Result<i32> {
    let (_, variant) = match located(parsed) {
        Ok(found) => found,
        Err(why) => {
            writeln!(out, "{}", embed::release_line())?;
            return stopped(out, "not verified", &why);
        }
    };
    let report = verify::verify(
        &variant.path,
        &serve::output_in(variant.path.as_path(), parsed.host),
    );
    writeln!(out, "{report}")?;
    Ok(i32::from(!report.verified()))
}

/// Take back what `install` put in, and put back what it moved aside.
///
/// A file at our name that we did not ship is left and named, and that is a
/// report rather than a failure: the verb did everything it may do.
fn uninstalling(parsed: &Parsed, out: &mut dyn Write) -> io::Result<i32> {
    const HEAD: &str = "not uninstalled";
    writeln!(out, "{}", embed::release_line())?;
    let (_, variant) = match located(parsed) {
        Ok(found) => found,
        Err(why) => return stopped(out, HEAD, &why),
    };
    writeln!(out, "variant: {}", variant.path)?;
    let data = match data_for(parsed, &variant) {
        Ok(data) => data,
        Err(why) => return stopped(out, HEAD, &why),
    };
    let removed = match uninstall::uninstall(
        SystemTime::now(),
        &variant.path,
        &data,
        &Executor::embedded(),
    ) {
        Ok(removed) => removed,
        Err(why) => return stopped(out, HEAD, &why.to_string()),
    };
    for line in said_removed(&removed, &export_line::path(&variant.path)) {
        writeln!(out, "{line}")?;
    }
    writeln!(out, "uninstalled")?;
    Ok(0)
}

/// The parked directories, on one line.
fn listed(dirs: &[PathBuf]) -> String {
    dirs.iter()
        .map(|dir| dir.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// What the placement found at the hook's name, and what it did about it.
fn said_hook(placed: &Placed) -> String {
    let hook = placed.hook.display();
    match &placed.disposition {
        Disposition::Absent => format!("hook: {hook}: placed; nothing was there"),
        Disposition::Upgrade { sha256 } => format!(
            "hook: {hook}: placed over sha256 {sha256}, a release of ours, parked in {}",
            listed(&placed.parked)
        ),
        Disposition::Foreign { sha256 } => format!(
            "hook: {hook}: placed over sha256 {sha256}, never shipped by us, parked in {} \
             because --replace said so",
            listed(&placed.parked)
        ),
    }
}

/// What became of `Export.lua`.
fn said_line(outcome: &Outcome, file: &Path) -> String {
    let file = file.display();
    match outcome {
        Outcome::Created => format!("Export.lua: {file}: created, holding the one line"),
        Outcome::Appended {
            parked,
            newline_added,
        } => {
            let mut said = format!(
                "Export.lua: {file}: the line appended, the file as found parked in {}",
                parked.display()
            );
            if *newline_added {
                said.push_str("; a newline added first where the file lacked one");
            }
            said
        }
        Outcome::AlreadyThere => {
            format!("Export.lua: {file}: the line was already there; nothing written")
        }
    }
}

/// What an uninstall took, what it left, and what it put back, a line each.
fn said_removed(removed: &Removed, file: &Path) -> Vec<String> {
    let mut said = Vec::new();
    match &removed.hook {
        Some(hook) => said.push(format!("hook: {}: removed", hook.display())),
        None if removed.left.is_empty() => said.push("hook: not there".to_owned()),
        None => {}
    }
    for left in &removed.left {
        said.push(format!("left: {left}"));
    }
    let file = file.display();
    said.push(match &removed.line {
        LineOutcome::Removed { parked, emptied } => {
            let mut line = format!(
                "Export.lua: {file}: the line taken out, the file as found parked in {}",
                parked.display()
            );
            if *emptied {
                line.push_str("; nothing else was in it, so it is left empty");
            }
            line
        }
        LineOutcome::NotThere => format!("Export.lua: {file}: no line of ours in it"),
        LineOutcome::NoFile => "Export.lua: not there".to_owned(),
    });
    for restored in &removed.restored {
        said.push(format!("restored: {}", restored.display()));
    }
    said
}

/// The data directory the `serve` line names: the one this line named, and
/// none when it named none, because `serve` finds the known one by itself
/// and a path spelled into a client's config stops following it.
fn named_data<'a>(parsed: &Parsed, data: &'a DataDir) -> Option<&'a Real> {
    parsed.data_dir.as_ref().map(|_| data.path())
}

/// The client configuration that starts `serve` against this install.
///
/// Built as JSON rather than spelled into a string, because a Windows path
/// is full of backslashes and a hand-escaped one is the kind of wrong that
/// shows only on somebody else's machine.
fn registration(exe: &Path, root: &Real, variant: &str, data: Option<&Real>) -> String {
    let mut args = vec![
        "serve".to_owned(),
        "--saved-games".to_owned(),
        root.to_string(),
        "--variant".to_owned(),
        variant.to_owned(),
    ];
    if let Some(data) = data {
        args.push("--data-dir".to_owned());
        args.push(data.to_string());
    }
    let snippet = serde_json::json!({
        "mcpServers": {
            "dcs-eval": {
                "command": exe.display().to_string(),
                "args": args,
            }
        }
    });
    serde_json::to_string_pretty(&snippet).expect("a JSON value of strings serialises")
}

/// Run one command line, the verb first.
///
/// The exit code: 0 for `installed`, `verified` or `uninstalled`, 1 for a
/// refusal, a failure on disk or a report that is not `verified`, and 2 —
/// the caller's, from the `Err` here — for a line that would not parse.
pub fn run<I: IntoIterator<Item = String>>(
    args: I,
    out: &mut dyn Write,
    exe: &Path,
) -> Result<i32, String> {
    let parsed = parse(args)?;
    let done = match parsed.verb {
        Verb::Install => installing(&parsed, out, exe),
        Verb::Verify => verifying(&parsed, out),
        Verb::Uninstall => uninstalling(&parsed, out),
    };
    done.map_err(|why| why.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use dcs_eval::paths;
    use dcs_eval::sha256::{digest, hex};

    use crate::testing::{Sandbox, a_live_session, snapshot};

    /// The executable the snippet names. Never run; only printed.
    const EXE: &str = r"C:\tools\dcs-mcp.exe";

    /// A file with known bytes, and whatever directories it needs.
    fn put(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().expect("a file has a parent"))
            .expect("the directories are made");
        fs::write(path, bytes).expect("the file is written");
    }

    /// One line run against the box: its `saved` as `Saved Games` and, for
    /// the verbs that take one, its `data` as the data directory. Neither the
    /// real `Saved Games` nor the real data directory is ever reached.
    fn ran(b: &Sandbox, verb: &str, more: &[&str]) -> (i32, String) {
        let mut line = vec![
            verb.to_owned(),
            "--saved-games".to_owned(),
            b.join("saved").display().to_string(),
        ];
        if verb != "verify" {
            line.push("--data-dir".to_owned());
            line.push(b.join("data").display().to_string());
        }
        line.extend(more.iter().map(|word| (*word).to_owned()));
        let mut sink: Vec<u8> = Vec::new();
        let code = run(line, &mut sink, Path::new(EXE)).expect("the line parses");
        (code, String::from_utf8(sink).expect("the output is UTF-8"))
    }

    /// Why a line would not parse, where it would not.
    fn refused(line: &[&str]) -> String {
        match parse(line.iter().map(|word| (*word).to_owned())) {
            Ok(_) => panic!("{line:?} parsed"),
            Err(why) => why,
        }
    }

    fn real(path: &Path) -> Real {
        paths::resolve(path).expect("the path resolves")
    }

    fn last(shown: &str) -> &str {
        shown.lines().last().expect("something was printed")
    }

    fn scripts(b: &Sandbox, variant: &str) -> PathBuf {
        b.join("saved").join(variant).join("Scripts")
    }

    fn hook(b: &Sandbox, variant: &str) -> PathBuf {
        scripts(b, variant)
            .join("Hooks")
            .join(embed::EXECUTOR_FILE_NAME)
    }

    fn times_the_line_is_in(file: &Path) -> usize {
        fs::read_to_string(file)
            .expect("Export.lua reads")
            .lines()
            .filter(|line| *line == export_line::LINE)
            .count()
    }

    /// The maintainer's own machine: three variants, and each holding an
    /// `Export.lua` so that none of them is the obvious one.
    fn three_variants() -> Sandbox {
        let b = Sandbox::new();
        for name in ["DCS", "DCS_F4E", "DCS_OH58D"] {
            put(&scripts(&b, name).join("Export.lua"), b"-- Tacview\n");
        }
        b
    }

    fn one_variant() -> Sandbox {
        let b = Sandbox::new();
        b.dir("saved/DCS.openbeta");
        b
    }

    const FOREIGN: &[u8] = b"-- somebody else's executor\n";

    #[test]
    fn three_variants_are_refused_every_one_named_and_nothing_written() {
        let b = three_variants();
        let before = snapshot(&b.path);
        let (code, shown) = ran(&b, "install", &[]);
        assert_eq!(code, 1, "{shown}");
        for name in [
            "DCS,",
            "DCS_F4E",
            "DCS_OH58D",
            "--variant",
            "not installed:",
        ] {
            assert!(shown.contains(name), "{name} is not named: {shown}");
        }
        assert_eq!(snapshot(&b.path), before, "the refusal wrote something");
    }

    #[test]
    fn a_named_variant_among_three_is_the_only_one_touched() {
        let b = three_variants();
        let f4e = snapshot(&b.join("saved/DCS_F4E"));
        let oh58d = snapshot(&b.join("saved/DCS_OH58D"));
        for typed in ["DCS", "dcs"] {
            let (code, shown) = ran(&b, "install", &["--variant", typed]);
            assert_eq!(code, 0, "{shown}");
            let then = shown
                .lines()
                .find(|line| line.starts_with("then: "))
                .expect("a then: line");
            assert!(then.ends_with(" --variant \"DCS\""), "{then}");
            assert!(shown.contains(r#""DCS""#), "{shown}");
            assert!(!shown.contains(r#""dcs""#), "{shown}");
        }
        let placed = fs::read(hook(&b, "DCS")).expect("the hook is there");
        assert_eq!(hex(&digest(&placed)), embed::EXECUTOR_SHA256);
        assert_eq!(
            times_the_line_is_in(&scripts(&b, "DCS").join("Export.lua")),
            1
        );
        assert_eq!(snapshot(&b.join("saved/DCS_F4E")), f4e);
        assert_eq!(snapshot(&b.join("saved/DCS_OH58D")), oh58d);
    }

    #[test]
    fn one_variant_needs_no_name() {
        let b = one_variant();
        let (code, shown) = ran(&b, "install", &[]);
        assert_eq!(code, 0, "{shown}");
        assert!(shown.contains(r#""--variant""#), "{shown}");
        assert!(shown.contains(r#""DCS.openbeta""#), "{shown}");
    }

    #[test]
    fn a_foreign_hook_is_refused_without_replace_and_nothing_written() {
        let b = one_variant();
        put(&hook(&b, "DCS.openbeta"), FOREIGN);
        put(&scripts(&b, "DCS.openbeta").join("Export.lua"), b"-- SRS\n");
        let before = snapshot(&b.path);
        let (code, shown) = ran(&b, "install", &["--variant", "DCS.openbeta"]);
        assert_eq!(code, 1, "{shown}");
        assert!(shown.contains("--replace"), "{shown}");
        assert!(last(&shown).starts_with("not installed: "), "{shown}");
        assert_eq!(snapshot(&b.path), before, "the refusal wrote something");
    }

    #[test]
    fn replace_parks_the_foreign_hook_and_installs() {
        let b = one_variant();
        put(&hook(&b, "DCS.openbeta"), FOREIGN);
        put(&scripts(&b, "DCS.openbeta").join("Export.lua"), b"-- SRS\n");
        let (code, shown) = ran(&b, "install", &["--variant", "DCS.openbeta", "--replace"]);
        assert_eq!(code, 0, "{shown}");
        assert!(
            shown.contains("never shipped by us") && shown.contains("because --replace said so"),
            "{shown}"
        );
        let placed = fs::read(hook(&b, "DCS.openbeta")).expect("the hook is there");
        assert_eq!(hex(&digest(&placed)), embed::EXECUTOR_SHA256);
        let parked: Vec<Vec<u8>> = fs::read_dir(b.join("data/parked"))
            .expect("the park store lists")
            .filter_map(Result::ok)
            .map(|dir| {
                dir.path()
                    .join("Scripts/Hooks")
                    .join(embed::EXECUTOR_FILE_NAME)
            })
            .filter(|file| file.is_file())
            .map(|file| fs::read(file).expect("the parked copy reads"))
            .collect();
        assert_eq!(parked, vec![FOREIGN.to_vec()]);
    }

    #[test]
    fn yes_is_refused_and_points_at_replace() {
        let why = refused(&["install", "--yes"]);
        assert!(why.contains("--yes") && why.contains("--replace"), "{why}");
    }

    #[test]
    fn install_says_what_happens_next_and_how_to_register() {
        let b = one_variant();
        let (code, shown) = ran(&b, "install", &["--variant", "DCS.openbeta"]);
        assert_eq!(code, 0, "{shown}");
        let root = real(&b.join("saved"));
        let data = real(&b.join("data"));
        assert!(shown.contains("\ninstalled\n"), "{shown}");
        assert!(shown.contains("\nnext: "), "{shown}");
        let then =
            format!("then: dcs-mcp verify --saved-games \"{root}\" --variant \"DCS.openbeta\"");
        assert!(shown.lines().any(|line| line == then), "{then}\n{shown}");

        let (_, json) = shown
            .split_once("register the server with your MCP client:\n")
            .expect("the snippet is introduced");
        let snippet: serde_json::Value = serde_json::from_str(json).expect("the snippet parses");
        let server = &snippet["mcpServers"]["dcs-eval"];
        assert_eq!(server["command"], EXE);
        assert_eq!(
            server["args"],
            serde_json::json!([
                "serve",
                "--saved-games",
                root.to_string(),
                "--variant",
                "DCS.openbeta",
                "--data-dir",
                data.to_string(),
            ])
        );
    }

    #[test]
    fn the_snippet_names_a_data_directory_only_when_the_line_did() {
        let b = one_variant();
        b.dir("data");
        let variant = real(&b.join("saved/DCS.openbeta"));
        let data = DataDir::at(&b.join("data"), &[&variant]).expect("the data directory is taken");
        let given = b.join("data").display().to_string();
        let named = parse(["install", "--data-dir", given.as_str()].map(str::to_owned))
            .expect("the line parses");
        assert_eq!(named_data(&named, &data), Some(data.path()));
        let unnamed = parse(["install".to_owned()]).expect("the line parses");
        assert_eq!(named_data(&unnamed, &data), None);
    }

    #[test]
    fn install_twice_is_one_hook_and_one_line() {
        let b = one_variant();
        let (code, shown) = ran(&b, "install", &["--variant", "DCS.openbeta"]);
        assert_eq!(code, 0, "{shown}");
        let (code, shown) = ran(&b, "install", &["--variant", "DCS.openbeta"]);
        assert_eq!(code, 0, "{shown}");
        assert!(
            shown.contains("the line was already there; nothing written"),
            "{shown}"
        );
        assert!(shown.contains("a release of ours"), "{shown}");
        let hooks: Vec<String> = fs::read_dir(scripts(&b, "DCS.openbeta").join("Hooks"))
            .expect("the hooks directory lists")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(hooks, vec![embed::EXECUTOR_FILE_NAME.to_owned()]);
        assert_eq!(
            times_the_line_is_in(&scripts(&b, "DCS.openbeta").join("Export.lua")),
            1
        );
    }

    #[test]
    fn verify_exits_one_when_anything_is_found() {
        let b = one_variant();
        let before = snapshot(&b.path);
        let (code, shown) = ran(&b, "verify", &["--variant", "DCS.openbeta"]);
        assert_eq!(code, 1, "{shown}");
        assert!(last(&shown).starts_with("not verified"), "{shown}");
        assert_eq!(
            shown.lines().next(),
            Some(embed::release_line().as_str()),
            "{shown}"
        );
        assert_eq!(
            shown.matches(&embed::release_line()).count(),
            1,
            "the release line is printed once: {shown}"
        );
        assert_eq!(snapshot(&b.path), before, "verify wrote something");
    }

    #[test]
    fn verify_exits_nought_on_a_healthy_install_with_a_session() {
        let b = one_variant();
        let (code, shown) = ran(&b, "install", &["--variant", "DCS.openbeta"]);
        assert_eq!(code, 0, "{shown}");
        let variant = real(&b.join("saved/DCS.openbeta"));
        let _ex = a_live_session(&serve::output_in(variant.as_path(), Host::Hook));
        let (code, shown) = ran(&b, "verify", &["--variant", "DCS.openbeta"]);
        assert_eq!(code, 0, "{shown}");
        assert_eq!(last(&shown), "verified", "{shown}");
    }

    #[test]
    fn verify_reads_the_session_of_the_host_it_is_given() {
        let b = one_variant();
        let (code, shown) = ran(&b, "install", &["--variant", "DCS.openbeta"]);
        assert_eq!(code, 0, "{shown}");
        let variant = real(&b.join("saved/DCS.openbeta"));
        let _ex = a_live_session(&serve::output_in(variant.as_path(), Host::Export));
        let (code, shown) = ran(
            &b,
            "verify",
            &["--variant", "DCS.openbeta", "--host", "export"],
        );
        assert_eq!(code, 0, "{shown}");
        assert_eq!(last(&shown), "verified", "{shown}");
        let (code, shown) = ran(&b, "verify", &["--variant", "DCS.openbeta"]);
        assert_eq!(code, 1, "the hook's session is not there: {shown}");
    }

    #[test]
    fn uninstall_puts_the_scripts_tree_back() {
        let b = one_variant();
        put(
            &scripts(&b, "DCS.openbeta").join("Export.lua"),
            b"-- Tacview\n",
        );
        put(
            &scripts(&b, "DCS.openbeta").join("Hooks/Other.lua"),
            b"-- other\n",
        );
        let before = snapshot(&scripts(&b, "DCS.openbeta"));
        let (code, shown) = ran(&b, "install", &["--variant", "DCS.openbeta"]);
        assert_eq!(code, 0, "{shown}");
        let (code, shown) = ran(&b, "uninstall", &["--variant", "DCS.openbeta"]);
        assert_eq!(code, 0, "{shown}");
        assert_eq!(last(&shown), "uninstalled", "{shown}");
        assert!(shown.contains(": removed\n"), "{shown}");
        assert_eq!(snapshot(&scripts(&b, "DCS.openbeta")), before);
    }

    #[test]
    fn flags_a_verb_does_not_take_are_refused_by_name() {
        for (line, named) in [
            (&["install", "--host", "hook"][..], "--host"),
            (
                &["install", "--variant", "a", "--variant", "b"][..],
                "--variant",
            ),
            (&["install", "extra"][..], "extra"),
            (&["install", "--variant"][..], "--variant"),
            (&["verify", "--replace"][..], "--replace"),
            (&["uninstall", "--replace"][..], "--replace"),
            (&["uninstall", "--host", "hook"][..], "--host"),
            (&["verify", "--data-dir", "x"][..], "--data-dir"),
        ] {
            let why = refused(line);
            assert!(why.contains(named), "{line:?}: {why}");
        }
    }

    #[test]
    fn uninstall_names_a_foreign_hook_and_leaves_it() {
        let b = one_variant();
        put(&hook(&b, "DCS.openbeta"), FOREIGN);
        let (code, shown) = ran(&b, "uninstall", &["--variant", "DCS.openbeta"]);
        assert_eq!(code, 0, "{shown}");
        assert!(
            shown.lines().any(|line| line.starts_with("left: ")),
            "{shown}"
        );
        assert_eq!(
            fs::read(hook(&b, "DCS.openbeta")).expect("still there"),
            FOREIGN
        );
    }
}
