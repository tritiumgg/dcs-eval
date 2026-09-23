//! The read-and-eval verbs, from a terminal.
//!
//! Five words — `status`, `ping`, `game-state`, `eval`, over a chunk or over
//! a file, and `screenshot` — and every one of them is a line over the
//! same function the matching tool is a line over. Nothing here decides what
//! an answer says; `wording` does, and this module prints what it rendered.
//! A second formatter here would be a second vocabulary, and the two would
//! drift the first time either was changed.
//!
//! Two flags keep the reply: `--out <path>` writes it where the caller asked,
//! and `--capture` keeps a copy under this build's own data directory. Both
//! write **the bytes the executor published**, taken off the reply file
//! rather than re-encoded from what was parsed out of it — so "verbatim"
//! means the wire's own bytes, header line endings and all.
//!
//! Where no single reply came off the wire — a `pending`, a refusal raised
//! here, the two verbs that never publish — neither flag writes anything at
//! all. Not an empty file: a zero-byte `--out` is indistinguishable from a
//! reply that carried nothing, and a caller reading one back would take a
//! request still in flight for a measured empty answer.
//!
//! `screenshot` keeps something else. Its reply is one line saying where the
//! write directory is, which nobody wants a copy of; the thing the call went
//! and got is the picture. So its `--out` copies the file the capture found,
//! byte for byte, and writes nothing at all for any answer but `ok` — and its
//! `--capture` is refused, because the reply it would keep is not the point.
//!
//! The install verbs are not here: they are `installer`'s, which finds
//! `Saved Games` rather than being given it, and a word neither module knows
//! is refused by name rather than half-answered.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::serve::{self, Host, Serve, host_of};
use crate::tools::{self, Answered, Reply};
use crate::wording;

/// The usage line, which is also the list of what this module answers to.
pub const USAGE: &str = "usage: dcs-mcp status | ping | game-state \
     | eval <state> (<code> | --file <path>)\n       \
     --saved-games <dir> --variant <name> [--host hook|export]\n       \
     [--wait-seconds <n>] [--max-instructions <n>] [--chunkname <name>]\n       \
     [--out <path>] [--capture] [--data-dir <dir>]\n       \
     dcs-mcp screenshot [--name <name>] [--wait-seconds <n>] [--out <path>]\n       \
     --saved-games <dir> --variant <name> [--host hook|export]";

/// What was asked for. One of five, and never a word the installer owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verb {
    Status,
    Ping,
    GameState,
    Eval,
    Screenshot,
}

impl Verb {
    /// The word it is spelt with, for a refusal to name it back.
    fn word(self) -> &'static str {
        match self {
            Verb::Status => "status",
            Verb::Ping => "ping",
            Verb::GameState => "game-state",
            Verb::Eval => "eval",
            Verb::Screenshot => "screenshot",
        }
    }

    /// Whether one reply comes off the wire for this verb.
    ///
    /// `status` reads local files and `game-state` runs a window of reads
    /// whose answer is a summary rather than one reply, so neither has bytes
    /// to keep — which is why the two flags that keep them are refused for
    /// those verbs rather than accepted and quietly doing nothing.
    fn publishes(self) -> bool {
        matches!(self, Verb::Ping | Verb::Eval)
    }
}

/// The verb a word names, or nothing where it names none.
fn verb_of(word: &str) -> Option<Verb> {
    match word {
        "status" => Some(Verb::Status),
        "ping" => Some(Verb::Ping),
        "game-state" => Some(Verb::GameState),
        "eval" => Some(Verb::Eval),
        "screenshot" => Some(Verb::Screenshot),
        _ => None,
    }
}

/// Whether this module is the one that answers a word.
pub fn takes(word: &str) -> bool {
    verb_of(word).is_some()
}

/// One command line, read.
struct Parsed {
    verb: Verb,
    opts: serve::Options,
    wait_seconds: Option<u64>,
    max_instructions: Option<u64>,
    chunkname: Option<String>,
    /// Which Lua state an `eval` runs in. Empty for the other three.
    state: String,
    /// The chunk itself, where one was given on the line.
    code: String,
    /// The file the chunk is read from instead.
    file: Option<String>,
    /// The name a screenshot is written under. Judged by the capture, which
    /// refuses a name that breaks the rule before anything is published.
    name: Option<String>,
    out: Option<PathBuf>,
    capture: bool,
}

/// Fill a slot that has not been filled, or name the flag that filled it.
///
/// The same rule the serve flags keep, for the same reason: no flag here
/// repeats, and the one that takes a list takes it comma-separated in one
/// value, so a flag given twice is a line with two opinions, and last-wins
/// would act on one of them silently.
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

/// Read a command line, the verb first.
fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Parsed, String> {
    let mut args = args.into_iter();
    let word = args.next().ok_or_else(|| "a verb is wanted".to_owned())?;
    let verb = verb_of(&word).ok_or_else(|| format!("dcs-mcp does not take {word}"))?;

    let mut saved_games = None;
    let mut variant = None;
    let mut host = None;
    let mut wait_seconds = None;
    let mut max_instructions = None;
    let mut chunkname = None;
    let mut file = None;
    let mut name = None;
    let mut out = None;
    let mut data_dir = None;
    let mut capture = false;
    let mut loose: Vec<String> = Vec::new();

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
            "--wait-seconds" => {
                let given = value("--wait-seconds")?;
                once(
                    &mut wait_seconds,
                    "--wait-seconds",
                    number("--wait-seconds", &given)?,
                )?
            }
            "--max-instructions" => {
                let given = value("--max-instructions")?;
                once(
                    &mut max_instructions,
                    "--max-instructions",
                    number("--max-instructions", &given)?,
                )?
            }
            "--chunkname" => once(&mut chunkname, "--chunkname", value("--chunkname")?)?,
            "--file" => once(&mut file, "--file", value("--file")?)?,
            "--name" => once(&mut name, "--name", value("--name")?)?,
            "--out" => once(&mut out, "--out", PathBuf::from(value("--out")?))?,
            "--data-dir" => once(
                &mut data_dir,
                "--data-dir",
                PathBuf::from(value("--data-dir")?),
            )?,
            "--capture" => {
                if capture {
                    return Err("--capture is given twice".to_owned());
                }
                capture = true;
            }
            other if other.starts_with("--") => {
                return Err(format!("{} does not take {other}", verb.word()));
            }
            other => loose.push(other.to_owned()),
        }
    }

    // A screenshot's `--out` keeps the picture, so the flag is its own; what
    // it will not take is a copy of the reply, which is one line saying where
    // the write directory is.
    if verb == Verb::Screenshot && capture {
        return Err(
            "screenshot does not take --capture: its reply only says where the \
             write directory is, and --out <path> keeps the picture"
                .to_owned(),
        );
    }
    // A verb with no single reply behind it cannot keep one. Refused here
    // rather than accepted and silently writing nothing, which would read as
    // a reply that was empty.
    if verb != Verb::Screenshot && !verb.publishes() {
        if out.is_some() {
            return Err(format!(
                "{} answers without one reply off the wire, so it does not take --out",
                verb.word()
            ));
        }
        if capture {
            return Err(format!(
                "{} answers without one reply off the wire, so it does not take --capture",
                verb.word()
            ));
        }
    }
    // `--data-dir` is not guarded by `--capture`. It names the directory this
    // build keeps its own files in, and every evaluation appends a line to
    // the run record there whether or not a reply is being kept — so a line
    // that moves the directory and captures nothing has moved something real.

    let mut state = String::new();
    let mut code = String::new();
    match verb {
        Verb::Eval => {
            if name.is_some() {
                return Err("eval does not take --name".to_owned());
            }
            let mut words = loose.into_iter();
            state = words.next().ok_or("eval wants a state to run in")?;
            match (&file, words.next()) {
                (Some(_), Some(extra)) => {
                    return Err(format!(
                        "eval reads the chunk from --file, not from {extra}"
                    ));
                }
                (Some(_), None) => {}
                (None, Some(given)) => code = given,
                (None, None) => return Err("eval wants a chunk, or --file <path>".to_owned()),
            }
            if let Some(extra) = words.next() {
                return Err(format!("eval does not take {extra}"));
            }
            // The chunkname of a file is the file, worked out where the bytes
            // are read; a second one given here would name something else.
            if file.is_some() && chunkname.is_some() {
                return Err("--chunkname names a chunk given on the line, not a file".to_owned());
            }
        }
        other => {
            // The chunk a screenshot runs is this build's own and the same
            // every time, so there is nothing for a budget to bound.
            if other == Verb::Screenshot && max_instructions.is_some() {
                return Err("screenshot does not take --max-instructions".to_owned());
            }
            if other != Verb::Screenshot && name.is_some() {
                return Err(format!("{} does not take --name", other.word()));
            }
            if file.is_some() {
                return Err(format!("{} does not take --file", other.word()));
            }
            if chunkname.is_some() {
                return Err(format!("{} does not take --chunkname", other.word()));
            }
            if let Some(extra) = loose.first() {
                return Err(format!("{} does not take {extra}", other.word()));
            }
        }
    }

    Ok(Parsed {
        verb,
        opts: serve::Options {
            saved_games: saved_games.ok_or("a verb wants --saved-games <dir>")?,
            variant: variant.ok_or("a verb wants --variant <name>")?,
            host: host.unwrap_or(Host::Hook),
            data_dir,
        },
        wait_seconds,
        max_instructions,
        chunkname,
        state,
        code,
        file,
        name,
        out,
        capture,
    })
}

/// The bytes the executor published for this answer, where a flag asked for
/// them and exactly one reply came off the wire.
///
/// Nothing at all where neither flag was given, because the bytes are read
/// off the disk by the asking and a caller that is not keeping the reply
/// would be paying for a copy of it to be thrown away. Nothing either where
/// no single reply came off the wire, which is what the two keeping flags are
/// guarded by.
fn written_bytes(answered: &Answered, wanted: bool) -> Option<Result<Reply, String>> {
    if !wanted {
        return None;
    }
    answered.published()
}

/// Whether `id` is one plain name, and so a thing a file can be called.
///
/// The id is the executor's, off the wire, and a capture is about to make a
/// filename out of it. One carrying a separator or a `..` would put the copy
/// somewhere the caller never named, so it is refused here rather than
/// joined and trusted.
fn one_name(id: &str) -> bool {
    let mut parts = Path::new(id).components();
    matches!(parts.next(), Some(std::path::Component::Normal(_))) && parts.next().is_none()
}

/// Write `bytes` at `path`, naming the path and what the OS said.
fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|why| format!("{}: {why}", path.display()))
}

/// Keep the reply where the flags asked for it.
///
/// The first failure stops the rest. A caller that asked for two copies and
/// got one is told which one did not land, and a second sentence about the
/// other would not change what it has to do about it.
fn keep(reply: &Reply, serve: &Serve, parsed: &Parsed) -> Result<(), String> {
    if let Some(path) = &parsed.out {
        write_bytes(path, &reply.bytes)?;
    }
    if parsed.capture {
        if !one_name(&reply.id) {
            return Err(format!(
                "the reply came back under {}, which is not a name a capture can be written under",
                reply.id
            ));
        }
        // The same resolver the run record goes through, so a `--data-dir`
        // moves both or neither, and the containment rule the install paths
        // are judged by is the one a capture is judged by too.
        let data = tools::data_dir(serve).map_err(|why| why.to_string())?;
        let root = data.replies_root();
        std::fs::create_dir_all(&root).map_err(|why| format!("{}: {why}", root.display()))?;
        write_bytes(&root.join(format!("{}.res", reply.id)), &reply.bytes)?;
    }
    Ok(())
}

/// Run one command line and print what it came to.
///
/// The exit code: 0 for an answer or a `pending` — and for a screenshot's
/// `not-written`, which is not marked an error — 1 where the answer is
/// marked an error or a file the caller asked for could not be written, and
/// 2 — the caller's, from the `Err` here — for a line that would not parse.
pub fn run<I: IntoIterator<Item = String>>(args: I, out: &mut dyn Write) -> Result<i32, String> {
    let parsed = parse(args)?;
    let serve = Serve::new(parsed.opts.clone());
    let upto = tools::waiting(parsed.wait_seconds);
    // The host is the flag's, already in the options. A verb has no host word
    // of its own: there is one install per command line.
    let answered = match parsed.verb {
        Verb::Status => tools::status(&serve, None),
        Verb::Ping => tools::ping(&serve, None, upto),
        Verb::GameState => tools::game_state(&serve, None, upto),
        Verb::Eval => match &parsed.file {
            Some(path) => tools::eval_file(
                &serve,
                None,
                &parsed.state,
                path,
                parsed.max_instructions,
                upto,
            ),
            None => tools::eval(
                &serve,
                None,
                &parsed.state,
                &parsed.code,
                parsed.chunkname.as_deref(),
                parsed.max_instructions,
                upto,
            ),
        },
        Verb::Screenshot => tools::screenshot(&serve, None, parsed.name.as_deref(), upto),
    };

    let mut code = i32::from(answered.answer.is_error == Some(true));
    // The bytes the executor published, where a flag asked to keep them and
    // one reply came off the wire; nothing at all otherwise. A caller that
    // asked for neither file is not told a reply could not be read back,
    // because it was never read: the answer it wanted arrived, and a failure
    // reported over it would be a failure at nothing it asked for.
    let published = written_bytes(&answered, parsed.out.is_some() || parsed.capture);
    match published {
        Some(Ok(reply)) => {
            if let Err(why) = keep(&reply, &serve, &parsed) {
                // Stderr, so that what a caller reads off stdout stays the
                // words the tool would have given and nothing else.
                eprintln!("{why}");
                code = 1;
            }
        }
        Some(Err(why)) => {
            eprintln!("the reply arrived and could not be read back: {why}");
            code = 1;
        }
        None => {}
    }
    // A screenshot's `--out`, which the step above keeps nothing for: its
    // answer carries no reply to point at, only the file a capture found.
    if let (Verb::Screenshot, Some(to)) = (parsed.verb, &parsed.out) {
        match answered.shot() {
            // Copied off the file the capture judged whole, not rebuilt from
            // anything read out of it, so the copy is the picture DCS wrote.
            Some(shot) => {
                if let Err(why) = std::fs::copy(shot, to) {
                    eprintln!(
                        "{} could not be copied to {}: {why}",
                        shot.display(),
                        to.display()
                    );
                    code = 1;
                }
            }
            // Not a failure: a `pending` or a `not-written` is an answer, and
            // said so above. Stderr only says why the path was left alone,
            // so an empty or stale file is never taken for this capture.
            None => eprintln!(
                "{} was not written: only an ok has a picture to copy",
                to.display()
            ),
        }
    }
    let shown = wording::text(&answered.answer);
    writeln!(out, "{shown}").map_err(|why| why.to_string())?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serve::Options;
    use crate::testing::{Sandbox, published, ran, ticking};
    use dcs_eval::paths;
    use dcs_eval::protocol;
    use dcs_eval::standin::Standin;
    use std::fs;

    /// A reply body with a CRLF, a byte that is not UTF-8 and a NUL in it, so
    /// that "verbatim" is a claim something could fail.
    const BODY: &[u8] = b"first\r\nsecond\n\xff\x00tail";

    fn opts(box_: &Sandbox) -> Options {
        Options {
            saved_games: box_.path.clone(),
            variant: "DCS.openbeta".to_owned(),
            host: Host::Hook,
            data_dir: Some(box_.join("data")),
        }
    }

    /// The flags every verb needs, then the words the case adds.
    ///
    /// `--data-dir` is among them, and not optional: every evaluation appends
    /// a line to the run record under it, so a line without one would write
    /// into the machine's own data directory.
    fn args<I>(box_: &Sandbox, more: I) -> Vec<String>
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let mut all: Vec<String> = Vec::new();
        for word in more {
            all.push(word.as_ref().to_owned());
        }
        all.push("--saved-games".to_owned());
        all.push(box_.path.to_string_lossy().into_owned());
        all.push("--variant".to_owned());
        all.push("DCS.openbeta".to_owned());
        all.push("--data-dir".to_owned());
        all.push(box_.join("data").to_string_lossy().into_owned());
        all
    }

    /// A line-by-line rendering of what two texts disagree about, empty when
    /// they agree. Printed by the assertion that takes it, so a failure says
    /// which line moved rather than only that something did.
    ///
    /// Split on `\n` alone, never with `str::lines`, which drops a `\r`
    /// before each newline: the stand-in ends its header lines with one, so
    /// two texts differing only there would be unequal and yet show no line
    /// that moved, and an assertion on the diff being empty would pass them.
    fn diff(want: &str, got: &str) -> String {
        if want == got {
            return String::new();
        }
        let mut out = String::new();
        let mut want = want.split('\n');
        let mut got = got.split('\n');
        loop {
            match (want.next(), got.next()) {
                (None, None) => break,
                (a, b) if a == b => {}
                (a, b) => {
                    if let Some(a) = a {
                        out.push_str(&format!("- {a}\n"));
                    }
                    if let Some(b) = b {
                        out.push_str(&format!("+ {b}\n"));
                    }
                }
            }
        }
        out
    }

    /// The row this work is proved by, and both halves of it are here on
    /// purpose: they watch the same run from two sides.
    ///
    /// The `--out` half is not compared against a re-encoding of the reply.
    /// The stand-in writes CRLF header lines, deliberately and unlike
    /// anything `frame` produces, so a `--out` that reframed what it parsed
    /// would fail the CRLF assertion while a hand-built expectation would
    /// have agreed with it — which is the way a verbatim test goes quietly
    /// vacuous.
    ///
    /// The wording half diffs one envelope rendered twice rather than two
    /// live runs, because a request id carries a per-minter tag and a reply
    /// carries the tick it was answered on: two live calls can never be
    /// byte-identical, and the question the row asks is which function did
    /// the rendering.
    #[test]
    fn cli_one_reply_is_written_verbatim_and_worded_as_the_tool_words_it() {
        let box_ = Sandbox::new();
        let mut s = Standin::open(&opts(&box_).output(), "hook").expect("the stand-in opens");
        ticking(&mut s);
        s.script("marker", "ok", "string", BODY);
        let out = box_.join("reply.out");

        let (code, shown) = ran(
            &mut s,
            args(
                &box_,
                &[
                    "eval",
                    "hook",
                    "return marker",
                    "--wait-seconds",
                    "10",
                    "--out",
                    &out.to_string_lossy(),
                ],
            ),
        );
        assert_eq!(code, 0, "an answered eval is not an error: {shown}");

        let written = fs::read(&out).expect("--out wrote the reply");
        assert_eq!(
            written,
            published(&s),
            "--out wrote the bytes the executor published"
        );
        assert!(
            written.windows(2).any(|pair| pair == b"\r\n"),
            "the wire's own header endings survived, so this is not a re-encoding"
        );
        assert!(
            written.ends_with(BODY),
            "the body survived byte for byte, NUL and all"
        );

        let envelope = protocol::parse(&written).expect("what was written parses as a reply");
        let by_the_tool = wording::text(&wording::reply(&envelope));
        assert_eq!(
            diff(&by_the_tool, &shown),
            "",
            "the terminal printed what the tool renders"
        );
        assert_eq!(
            shown.lines().next(),
            Some("reply"),
            "an answered reply is headed `reply`: {shown}"
        );
        assert!(
            shown.contains("result_type: string"),
            "and carries the reply's own headers, so two empty renderings \
             could not have agreed: {shown}"
        );
    }

    /// The other half of the keeping rule: a reply really did come back, and
    /// `--capture` kept it under the data directory this line named.
    ///
    /// `--out` is deliberately not given. The verbatim test above watches
    /// that flag, and a line carrying both would let a capture that never
    /// wrote anything hide behind the file the other flag wrote.
    #[test]
    fn cli_capture_keeps_the_reply_under_the_data_dir() {
        let box_ = Sandbox::new();
        let mut s = Standin::open(&opts(&box_).output(), "hook").expect("the stand-in opens");
        ticking(&mut s);
        s.script("marker", "ok", "string", BODY);
        // The directory `args` already points every line at, which is also
        // where the run record goes; `--data-dir` is not repeated here,
        // because a flag given twice is refused.
        let data = box_.join("data");

        let (code, shown) = ran(
            &mut s,
            args(
                &box_,
                &[
                    "eval",
                    "hook",
                    "return marker",
                    "--wait-seconds",
                    "10",
                    "--capture",
                ],
            ),
        );
        assert_eq!(code, 0, "an answered eval is not an error: {shown}");

        let mut kept: Vec<PathBuf> = fs::read_dir(data.join("replies"))
            .expect("the capture directory was made")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        assert_eq!(kept.len(), 1, "one reply was captured: {kept:?}");
        let copy = kept.pop().expect("the one capture");
        assert_eq!(
            copy.extension().and_then(|ext| ext.to_str()),
            Some("res"),
            "under the name the wire gave it: {}",
            copy.display()
        );
        assert_eq!(
            fs::read(&copy).expect("the capture reads"),
            published(&s),
            "--capture kept the bytes the executor published"
        );
    }

    /// An id the executor never could have meant, refused rather than joined.
    /// It is the one place a filename on this side is built out of the wire.
    #[test]
    fn cli_a_reply_id_that_is_not_a_name_captures_nowhere() {
        assert!(one_name("abc123"), "a plain id is a name");
        for wrong in ["..", "a/b", "a\\b", "", ".", "C:/tmp/x"] {
            assert!(
                !one_name(wrong),
                "`{wrong}` is not a name a capture may be written under"
            );
        }
    }

    /// The capture rule, and the mutation's target. Nothing is ticked, so the
    /// wait runs out and the answer is a `pending` — which has no reply
    /// behind it and must therefore leave no file behind either.
    #[test]
    fn cli_a_pending_writes_no_file_at_all() {
        let box_ = Sandbox::new();
        let mut s = Standin::open(&opts(&box_).output(), "hook").expect("the stand-in opens");
        ticking(&mut s);
        let out = box_.join("nothing.out");
        let data = box_.join("data");

        let mut sink: Vec<u8> = Vec::new();
        let code = run(
            args(
                &box_,
                &[
                    "eval",
                    "hook",
                    "return 1",
                    "--wait-seconds",
                    "0",
                    "--out",
                    &out.to_string_lossy(),
                    "--capture",
                ],
            ),
            &mut sink,
        )
        .expect("the line parses");
        let shown = String::from_utf8_lossy(&sink).trim_end().to_owned();

        assert_eq!(code, 0, "a pending is not a failure: {shown}");
        assert_eq!(
            shown.lines().next(),
            Some("pending"),
            "the wait ran out and the answer says so: {shown}"
        );
        assert!(
            !out.exists(),
            "no reply came, so no file was written — an empty one would read \
             back as a measured empty answer"
        );
        let kept: Vec<PathBuf> = fs::read_dir(data.join("replies"))
            .map(|listing| {
                listing
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .collect()
            })
            .unwrap_or_default();
        assert!(kept.is_empty(), "nothing was captured either: {kept:?}");
    }

    /// Every verb reaches a body and answers something, and the two refusals
    /// after them are what say the four above are not vacuous.
    #[test]
    fn cli_every_read_verb_answers() {
        let box_ = Sandbox::new();
        let mut s = Standin::open(&opts(&box_).output(), "hook").expect("the stand-in opens");
        ticking(&mut s);
        let chunk = box_.join("chunk.lua");
        fs::write(&chunk, b"return 1\n").expect("the chunk is written");
        let kept = box_.join("kept.out");

        // Every wait is zero: nothing ticks the stand-in here, and a verb
        // that waited would only wait.
        let cases: [(&[&str], &str); 6] = [
            (&["status"], "status"),
            (&["ping", "--wait-seconds", "0"], "pending"),
            (&["game-state", "--wait-seconds", "0"], ""),
            (
                &["eval", "hook", "return 1", "--wait-seconds", "0"],
                "pending",
            ),
            (
                &["eval", "hook", "--file", "", "--wait-seconds", "0"],
                "pending",
            ),
            (
                &["eval", "hook", "--file", "<dir>", "--wait-seconds", "0"],
                "refused",
            ),
        ];
        for (words, head) in cases {
            let path = chunk.to_string_lossy().into_owned();
            let dir = box_.path.to_string_lossy().into_owned();
            // The file cases' placeholders: an empty one is the chunk's real
            // path, published and not waited on, and `<dir>` is a directory,
            // which the stat refuses before anything opens it.
            let mut line: Vec<String> = words
                .iter()
                .map(|word| match *word {
                    "" => path.clone(),
                    "<dir>" => dir.clone(),
                    _ => (*word).to_owned(),
                })
                .collect();
            // The file cases are given somewhere to write, so that "it kept
            // nothing" is a claim about a pending and a refusal rather than
            // about a flag nobody passed.
            if line.iter().any(|word| word == "--file") {
                line.push("--out".to_owned());
                line.push(kept.to_string_lossy().into_owned());
            }
            let mut sink: Vec<u8> = Vec::new();
            run(args(&box_, &line), &mut sink).unwrap_or_else(|why| panic!("{line:?}: {why}"));
            let shown = String::from_utf8_lossy(&sink).trim_end().to_owned();
            assert!(!shown.is_empty(), "{line:?} answered nothing at all");
            if !head.is_empty() {
                assert_eq!(
                    shown.lines().next(),
                    Some(head),
                    "{line:?} is headed `{head}`: {shown}"
                );
            }
        }
        assert!(
            !kept.exists(),
            "neither a pending nor a refused eval --file published a reply, so \
             neither kept anything"
        );

        let mut sink: Vec<u8> = Vec::new();
        let unknown = run(args(&box_, &["reflect"]), &mut sink)
            .expect_err("a verb this module does not own is refused");
        assert!(unknown.contains("reflect"), "it names the word: {unknown}");
        let mut sink: Vec<u8> = Vec::new();
        let flag = run(args(&box_, &["status", "--loud"]), &mut sink)
            .expect_err("a flag nothing takes is refused");
        assert!(flag.contains("--loud"), "it names the flag: {flag}");
    }

    /// A file read from inside the write directory's own `Config`, the place
    /// a location rule would most plainly have refused, runs and answers,
    /// and the answer names the file and what its bytes hashed to.
    #[test]
    fn cli_eval_file_reads_a_file_from_wherever_it_lies() {
        let box_ = Sandbox::new();
        let mut s = Standin::open(&opts(&box_).output(), "hook").expect("the stand-in opens");
        ticking(&mut s);
        let probe = box_.join("DCS.openbeta").join("Config").join("probe.lua");
        fs::create_dir_all(probe.parent().expect("a parent")).expect("Config is made");
        fs::write(&probe, b"return marker\n").expect("the probe is written");
        s.script("marker", "ok", "string", b"42");

        let (code, shown) = ran(
            &mut s,
            args(
                &box_,
                [
                    "eval",
                    "hook",
                    "--file",
                    &probe.to_string_lossy(),
                    "--wait-seconds",
                    "10",
                ],
            ),
        );
        assert_eq!(code, 0, "an answered eval is not an error: {shown}");
        assert_eq!(shown.lines().next(), Some("reply"), "{shown}");
        let real = paths::resolve(&probe).expect("the probe resolves");
        assert!(
            shown.contains(&format!("source: {real}")),
            "the answer names the file it read: {shown}"
        );
        // Computed outside the build, with `sha256sum`, over `return marker`
        // and a newline, so it is independent of the reader.
        assert!(
            shown.contains(
                "sha256: 7d72111abefa97d806630e7151bbfc34c1d60b0dacabaf49eb83bc906701fb0f"
            ),
            "and what its bytes hashed to: {shown}"
        );
    }

    /// A flag that keeps a reply, on a verb that has no reply to keep. It is
    /// refused by name rather than accepted and quietly writing nothing,
    /// which is the same trap the `pending` rule is against.
    #[test]
    fn cli_out_is_refused_for_a_verb_with_no_single_reply() {
        let box_ = Sandbox::new();
        let out = box_.join("never.out");

        let mut sink: Vec<u8> = Vec::new();
        let why = run(
            args(&box_, &["status", "--out", &out.to_string_lossy()]),
            &mut sink,
        )
        .expect_err("status does not take --out");
        assert!(
            why.contains("status") && why.contains("--out"),
            "the refusal names the verb and the flag: {why}"
        );

        let mut sink: Vec<u8> = Vec::new();
        let why = run(args(&box_, &["game-state", "--capture"]), &mut sink)
            .expect_err("game-state does not take --capture");
        assert!(
            why.contains("game-state") && why.contains("--capture"),
            "the refusal names the verb and the flag: {why}"
        );
        assert!(!out.exists(), "a refused line wrote nothing");
    }

    /// A whole picture, the one the client crate's own capture tests stage.
    const PNG: &[u8] = include_bytes!("../../dcs-eval/fixtures/shot/picture.png");

    /// The write directory the stand-in's session lies under, which is what
    /// the capture's chunk answers with.
    fn writedir(box_: &Sandbox) -> PathBuf {
        box_.join("DCS.openbeta")
    }

    /// A ticking session that answers the capture's chunk with the write
    /// directory as `lfs.writedir()` spells it, trailing separator and all,
    /// and the `ScreenShots` directory under it made ready.
    fn shooting(box_: &Sandbox) -> Standin {
        let mut s = Standin::open(&opts(box_).output(), "hook").expect("the stand-in opens");
        ticking(&mut s);
        let answer = format!("{}\\", writedir(box_).display());
        s.script("makeScreenShot", "ok", "string", answer.as_bytes());
        fs::create_dir_all(writedir(box_).join("ScreenShots")).expect("ScreenShots is made");
        s
    }

    /// One `screenshot` line run to completion. The request is answered once
    /// it is on the disk, and where `lands` names a file, the picture is
    /// written there straight after — so it is stamped after the request
    /// and is this capture's, as a file DCS writes would be.
    fn shot(s: &mut Standin, line: Vec<String>, lands: Option<&Path>) -> (i32, String) {
        let (code, sink) = std::thread::scope(|scope| {
            let running = scope.spawn(|| {
                let mut sink: Vec<u8> = Vec::new();
                let code = run(line, &mut sink).expect("the line parses");
                (code, sink)
            });
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let mut answered = false;
            while !running.is_finished() && std::time::Instant::now() < deadline {
                let asked = fs::read_dir(s.req())
                    .map(|listing| {
                        listing
                            .filter_map(Result::ok)
                            .any(|e| e.file_name().to_string_lossy().ends_with(".req"))
                    })
                    .unwrap_or(false);
                if asked && !answered {
                    s.tick();
                    if let Some(path) = lands {
                        fs::write(path, PNG).expect("the picture is written");
                    }
                    answered = true;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            running.join().expect("the verb does not panic")
        });
        let shown = String::from_utf8_lossy(&sink).trim_end().to_owned();
        (code, shown)
    }

    /// The three flags, read into the slots the verb runs on, and each one
    /// refused when it is given a second time.
    #[test]
    fn screenshot_cli_flags_are_parsed_and_each_refused_twice_over() {
        let box_ = Sandbox::new();
        let out = box_.join("kept.png");
        let out = out.to_string_lossy().into_owned();
        let parsed = parse(args(
            &box_,
            [
                "screenshot",
                "--name",
                "shot",
                "--wait-seconds",
                "3",
                "--out",
                &out,
            ],
        ))
        .expect("the line parses");
        assert_eq!(parsed.verb, Verb::Screenshot);
        assert_eq!(parsed.name.as_deref(), Some("shot"));
        assert_eq!(parsed.wait_seconds, Some(3));
        assert_eq!(parsed.out, Some(PathBuf::from(&out)));

        for (flag, first, second) in [
            ("--name", "one", "two"),
            ("--wait-seconds", "1", "2"),
            ("--out", "a.png", "b.png"),
        ] {
            let why = match parse(args(&box_, ["screenshot", flag, first, flag, second])) {
                Ok(_) => panic!("{flag} given twice is refused"),
                Err(why) => why,
            };
            assert!(
                why.contains(flag) && why.contains("twice"),
                "the refusal names {flag} and says it came twice: {why}"
            );
        }
    }

    /// `--capture` refused by name, and the flags that belong to a chunk of
    /// the caller's refused as well: the chunk a capture runs is this
    /// build's own.
    #[test]
    fn screenshot_cli_capture_is_refused_by_name() {
        let box_ = Sandbox::new();
        let mut sink: Vec<u8> = Vec::new();
        let why = run(args(&box_, ["screenshot", "--capture"]), &mut sink)
            .expect_err("screenshot does not take --capture");
        assert!(
            why.contains("screenshot") && why.contains("--capture"),
            "the refusal names the verb and the flag: {why}"
        );
        for words in [
            &["screenshot", "--file", "x.lua"][..],
            &["screenshot", "--chunkname", "x"],
            &["screenshot", "--max-instructions", "5"],
            &["screenshot", "shot"],
            &["ping", "--name", "shot"],
            &["eval", "hook", "return 1", "--name", "shot"],
        ] {
            let flag = words[words.len() - 2..]
                .iter()
                .find(|word| word.starts_with("--"))
                .unwrap_or(&words[words.len() - 1]);
            let why = run(args(&box_, words), &mut sink)
                .expect_err("a word the verb does not take is refused");
            assert!(
                why.contains(flag),
                "{words:?} is refused naming {flag}: {why}"
            );
        }
        assert!(sink.is_empty(), "a refused line printed no answer");
    }

    /// `--out` after an `ok`: the file the capture found, copied byte for
    /// byte, and the answer printed as the tool words it.
    #[test]
    fn screenshot_cli_out_is_the_file_byte_for_byte() {
        let box_ = Sandbox::new();
        let mut s = shooting(&box_);
        let found = writedir(&box_).join("ScreenShots").join("shot.png");
        let out = box_.join("kept.png");

        let (code, shown) = shot(
            &mut s,
            args(
                &box_,
                [
                    "screenshot",
                    "--name",
                    "shot",
                    "--wait-seconds",
                    "10",
                    "--out",
                    &out.to_string_lossy(),
                ],
            ),
            Some(&found),
        );
        assert_eq!(code, 0, "an ok is not an error: {shown}");
        assert_eq!(shown.lines().next(), Some("ok"), "{shown}");
        assert!(
            shown.contains(&format!("path: {}", found.display())),
            "the answer names the file it found: {shown}"
        );
        // Compared whole and reported by size, so a failure does not print a
        // picture's worth of bytes twice over.
        let copy = fs::read(&out).expect("--out wrote the copy");
        let picture = fs::read(&found).expect("the capture reads");
        assert!(
            copy == picture,
            "the copy is the file DCS wrote, byte for byte: {} bytes against {}",
            copy.len(),
            picture.len()
        );
    }

    /// `--out` after a `not-written`: the executor answered and no file came,
    /// so the path is left exactly as it was, which here is not there.
    #[test]
    fn screenshot_cli_not_written_writes_nothing_at_out() {
        let box_ = Sandbox::new();
        let mut s = shooting(&box_);
        let out = box_.join("never.png");

        let (_, shown) = shot(
            &mut s,
            args(
                &box_,
                [
                    "screenshot",
                    "--name",
                    "shot",
                    "--wait-seconds",
                    "1",
                    "--out",
                    &out.to_string_lossy(),
                ],
            ),
            None,
        );
        assert_eq!(shown.lines().next(), Some("not-written"), "{shown}");
        assert!(
            !out.exists(),
            "no picture came, so nothing was written where one would be read back"
        );
    }

    /// The binary's exit codes, as `run` hands them back: 0 for an answer,
    /// `pending` and `not-written` among them, whether or not `--out` was
    /// given; 1 for a refusal and for a copy that could not be written. The
    /// third, 2, is the binary's for a line `run` refuses to parse, and is
    /// watched on the real process beside the installer's.
    #[test]
    fn screenshot_cli_exits_0_for_an_answer_and_1_for_a_refusal_or_a_failed_copy() {
        // An `ok`, whose copy lands.
        let box_ = Sandbox::new();
        let mut s = shooting(&box_);
        let found = writedir(&box_).join("ScreenShots").join("shot.png");
        let out = box_.join("kept.png").to_string_lossy().into_owned();
        let line = [
            "screenshot",
            "--name",
            "shot",
            "--wait-seconds",
            "10",
            "--out",
            &out,
        ];
        let (code, shown) = shot(&mut s, args(&box_, line), Some(&found));
        assert_eq!((code, shown.lines().next()), (0, Some("ok")), "{shown}");

        // The same `ok`, with a copy that cannot land.
        let box_ = Sandbox::new();
        let mut s = shooting(&box_);
        let found = writedir(&box_).join("ScreenShots").join("shot.png");
        let out = box_.join("missing").join("kept.png");
        let out = out.to_string_lossy().into_owned();
        let line = [
            "screenshot",
            "--name",
            "shot",
            "--wait-seconds",
            "10",
            "--out",
            &out,
        ];
        let (code, shown) = shot(&mut s, args(&box_, line), Some(&found));
        assert_eq!(
            (code, shown.lines().next()),
            (1, Some("ok")),
            "the capture came and the copy failed: {shown}"
        );

        // A `not-written`, with `--out`: an answer, and nothing to copy.
        let box_ = Sandbox::new();
        let mut s = shooting(&box_);
        let out = box_.join("never.png").to_string_lossy().into_owned();
        let line = [
            "screenshot",
            "--name",
            "shot",
            "--wait-seconds",
            "1",
            "--out",
            &out,
        ];
        let (code, shown) = shot(&mut s, args(&box_, line), None);
        assert_eq!(
            (code, shown.lines().next()),
            (0, Some("not-written")),
            "a capture not written is an answer: {shown}"
        );

        // A `pending`, with `--out`: nothing ticks, so the wait runs out.
        let box_ = Sandbox::new();
        let _s = shooting(&box_);
        let out = box_.join("never.png").to_string_lossy().into_owned();
        let line = ["screenshot", "--wait-seconds", "0", "--out", &out];
        let mut sink: Vec<u8> = Vec::new();
        let code = run(args(&box_, line), &mut sink).expect("the line parses");
        let shown = String::from_utf8_lossy(&sink).trim_end().to_owned();
        assert_eq!(
            (code, shown.lines().next()),
            (0, Some("pending")),
            "{shown}"
        );

        // Two refusals, neither publishing anything: a name that breaks the
        // rule, and the host that cannot reach the capture. The session is
        // there, so neither is a `no-session` standing in for it.
        for (words, head) in [
            (&["screenshot", "--name", "a.b"][..], "bad-request"),
            (&["screenshot", "--host", "export"], "unsupported"),
        ] {
            let box_ = Sandbox::new();
            let _s = shooting(&box_);
            let mut sink: Vec<u8> = Vec::new();
            let code = run(args(&box_, words), &mut sink).expect("the line parses");
            let shown = String::from_utf8_lossy(&sink).trim_end().to_owned();
            assert_eq!((code, shown.lines().next()), (1, Some(head)), "{shown}");
        }
    }

    /// The naming rule, held rather than described: the filtered command this
    /// file is proved by selects by substring, so a test here under another
    /// name would be neither run by it nor missed by it.
    #[test]
    fn cli_names_every_test_here() {
        let source = include_str!("cli.rs");
        let mut lines = source.lines().enumerate();
        while let Some((number, line)) = lines.next() {
            let marker = line.trim();
            if marker != concat!("#[", "test]") && marker != concat!("#[", "tokio::test]") {
                continue;
            }
            let declared = lines
                .by_ref()
                .map(|(_, next)| next.trim_start())
                .find(|next| next.contains("fn "))
                .unwrap_or_else(|| {
                    panic!("the test marker on line {} declares nothing", number + 1)
                });
            let name = declared
                .split("fn ")
                .nth(1)
                .and_then(|rest| rest.split(['(', '<']).next())
                .expect("the declaration names the function");
            // The screenshot verb's tests are proved by a filter of their
            // own, and their prefix carries `cli_` inside it, so the filter
            // the rest of this file is proved by selects them as well.
            assert!(
                name.starts_with("cli_") || name.starts_with("screenshot_cli_"),
                "the test marked on line {} is named `{name}`, which the filtered \
                 command this file is proved by would not select",
                number + 1
            );
        }
    }
}
