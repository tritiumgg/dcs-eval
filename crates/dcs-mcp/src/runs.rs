//! The run record: one line of JSON per evaluation, appended where this
//! build keeps its own files.
//!
//! A result nobody can reproduce is worth little, so every chunk this server
//! sends into the game leaves behind the two facts that would let somebody
//! run it again: where the bytes came from and what they hashed to. The line
//! is appended before the reply is rendered, so a caller reading an answer
//! knows the record for it is already on the disk.
//!
//! **The digest is taken, never computed here.** `dcs-eval`'s reader hashes
//! the bytes it read off the file, and [`Chunk::File`] carries that reader's
//! own record. A digest computed in this module could only be over the bytes
//! that were *sent*, which is a different claim wearing the same name: it
//! would agree with the file in every case except the one a provenance
//! record exists for, a reader that handed on something other than what it
//! read. Decision record 0021 holds the rest of that argument.
//!
//! There is one writer, and this is it. A second place that appended to the
//! record would be a second opinion about what a run is.

use std::fs;
use std::io;
use std::io::Write as _;
use std::time::SystemTime;

use dcs_eval::pipeline::PipeError;
use dcs_eval::source::Source;
use dcs_eval::wait::Outcome;
use serde_json::{Map, Value};

use crate::register::{DataDir, stamp};
use crate::serve::Host;

/// What ran: a chunk the caller spelt out, or a file this server read.
pub(crate) enum Chunk<'a> {
    /// A chunk given on the line or in a call's arguments. It has no path
    /// and no digest: there is no reader's record to take one from, and a
    /// digest computed here would be this module hashing what it sent.
    Inline {
        chunkname: Option<&'a str>,
        bytes: usize,
    },
    /// A file, with the reader's own record of the bytes it read.
    File(&'a Source),
}

/// One evaluation, as much of it as is known before the request goes out.
pub(crate) struct Run<'a> {
    pub host: Host,
    pub state: &'a str,
    /// The executor session the request was fenced with.
    pub stamp: &'a str,
    /// The instruction budget, where the call named one: the number asked
    /// for, and not the `instructions=<n>` a header spells it with. Decision
    /// record 0021 says why the record keeps a count rather than the wire's
    /// wording of it.
    pub budget: Option<&'a str>,
    pub chunk: Chunk<'a>,
}

/// A string where there is one, and JSON's `null` where there is not.
fn text(value: Option<&str>) -> Value {
    value.map_or(Value::Null, |value| Value::String(value.to_owned()))
}

/// A header read as a number, and `null` where it is absent or will not
/// parse. A header the far end spelt in a way this build does not expect is
/// recorded as missing rather than as a zero somebody could average.
fn number<T>(value: Option<&str>) -> Value
where
    T: std::str::FromStr + Into<Value>,
{
    value
        .and_then(|value| value.parse::<T>().ok())
        .map_or(Value::Null, Into::into)
}

/// The one line this evaluation is recorded as.
///
/// Pure: it decides nothing about where the line goes and touches no file,
/// so what the record says can be read off one call. `at` is the instant the
/// line is stamped with, and `item` is what the single-request window
/// yielded — `None` where it yielded nothing at all.
pub(crate) fn line(
    run: &Run<'_>,
    at: SystemTime,
    item: Option<&Result<Outcome, PipeError>>,
) -> String {
    let source = match &run.chunk {
        Chunk::File(source) => Some(*source),
        Chunk::Inline { .. } => None,
    };
    let sha256 = source.map(|source| source.sha256_hex());

    // Everything that comes off the reply. A run that was published but not
    // answered records what it is — pending, superseded, dead, or an error
    // the pipeline raised — and leaves the reply's own fields out rather
    // than filling them with a value nothing measured.
    let (id, status, stage, cpu_ms, tick) = match item {
        Some(Ok(Outcome::Reply(envelope))) => (
            text(envelope.headers.get("id")),
            text(envelope.headers.get("status")),
            text(envelope.headers.get("stage")),
            number::<f64>(envelope.headers.get("cpu_ms")),
            number::<u64>(envelope.headers.get("tick")),
        ),
        Some(Ok(outcome @ Outcome::Pending { .. })) => (
            text(Some(outcome.id())),
            text(Some("pending")),
            Value::Null,
            Value::Null,
            Value::Null,
        ),
        Some(Ok(outcome @ Outcome::Superseded { .. })) => (
            text(Some(outcome.id())),
            text(Some("superseded")),
            Value::Null,
            Value::Null,
            Value::Null,
        ),
        Some(Ok(outcome @ Outcome::Dead { .. })) => (
            text(Some(outcome.id())),
            text(Some("dead")),
            Value::Null,
            Value::Null,
            Value::Null,
        ),
        Some(Err(_)) => (
            Value::Null,
            text(Some("error")),
            Value::Null,
            Value::Null,
            Value::Null,
        ),
        None => (
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
        ),
    };

    let mut row = Map::new();
    row.insert("ts".to_owned(), Value::String(stamp(at)));
    row.insert("id".to_owned(), id);
    row.insert("stamp".to_owned(), Value::String(run.stamp.to_owned()));
    row.insert("host".to_owned(), Value::String(run.host.word().to_owned()));
    row.insert("state".to_owned(), Value::String(run.state.to_owned()));
    row.insert(
        "source".to_owned(),
        Value::String(match run.chunk {
            Chunk::File(_) => "file".to_owned(),
            Chunk::Inline { .. } => "chunk".to_owned(),
        }),
    );
    row.insert(
        "path".to_owned(),
        // The path as this build resolved it, backslashes and all. A second
        // spelling here would be a path the reader never opened.
        source.map_or(Value::Null, |source| {
            Value::String(source.path().to_string())
        }),
    );
    row.insert("sha256".to_owned(), text(sha256.as_deref()));
    row.insert(
        "bytes".to_owned(),
        Value::from(match &run.chunk {
            Chunk::File(source) => source.body().len(),
            Chunk::Inline { bytes, .. } => *bytes,
        }),
    );
    row.insert(
        "chunkname".to_owned(),
        match &run.chunk {
            Chunk::File(source) => text(Some(source.chunkname())),
            Chunk::Inline { chunkname, .. } => text(*chunkname),
        },
    );
    row.insert(
        "bom".to_owned(),
        source.map_or(Value::Null, |source| {
            Value::String(source.bom().to_string())
        }),
    );
    row.insert(
        "shebang".to_owned(),
        source.map_or(Value::Null, |source| {
            Value::String(source.shebang().to_string())
        }),
    );
    row.insert("status".to_owned(), status);
    row.insert("stage".to_owned(), stage);
    row.insert("cpu_ms".to_owned(), cpu_ms);
    row.insert("tick".to_owned(), tick);
    row.insert("budget".to_owned(), text(run.budget));
    Value::Object(row).to_string()
}

/// Append one line to the record, making the directory if it is not there.
///
/// One write of the line and its newline together, so a line is whole or
/// absent: a reader of this file takes a line at a time, and half a line is
/// worse than none.
pub(crate) fn append(data: &DataDir, line: &str) -> io::Result<()> {
    fs::create_dir_all(data.path().as_path())?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(data.runs_path())?;
    file.write_all(format!("{line}\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serve::{Options, Serve};
    use crate::testing::{Sandbox, published, ran, ticking};
    use dcs_eval::file::{self, Roots};
    use dcs_eval::paths;
    use dcs_eval::protocol;
    use dcs_eval::source;
    use dcs_eval::standin::Standin;
    use std::fs;
    use std::time::{Duration, UNIX_EPOCH};

    fn opts(box_: &Sandbox) -> Options {
        Options {
            saved_games: box_.path.clone(),
            variant: "DCS.openbeta".to_owned(),
            host: Host::Hook,
            data_dir: Some(box_.join("data")),
        }
    }

    /// The flags every verb needs, the data directory among them.
    fn args(box_: &Sandbox, more: &[&str]) -> Vec<String> {
        let mut all: Vec<String> = more.iter().map(|word| (*word).to_owned()).collect();
        all.push("--saved-games".to_owned());
        all.push(box_.path.to_string_lossy().into_owned());
        all.push("--variant".to_owned());
        all.push("DCS.openbeta".to_owned());
        all.push("--data-dir".to_owned());
        all.push(box_.join("data").to_string_lossy().into_owned());
        all
    }

    /// Every line the record holds, parsed.
    fn rows(data: &std::path::Path) -> Vec<Value> {
        let text = fs::read_to_string(data.join("runs.jsonl")).expect("the record reads");
        text.lines()
            .map(|line| serde_json::from_str(line).expect("each line is one JSON object"))
            .collect()
    }

    /// The wiring, end to end: a chunk really ran, and the record says so
    /// under the id the executor answered with.
    ///
    /// The id is compared against the reply the stand-in published rather
    /// than against a literal, so a record filled in from what was asked for
    /// rather than from what came back would not agree.
    #[test]
    fn an_inline_eval_records_one_line_naming_the_reply() {
        let box_ = Sandbox::new();
        let mut s = Standin::open(&opts(&box_).output(), "hook").expect("the stand-in opens");
        ticking(&mut s);
        s.script("marker", "ok", "string", b"42");
        let data = box_.join("data");

        let (code, shown) = ran(
            &mut s,
            args(
                &box_,
                &["eval", "hook", "return marker", "--wait-seconds", "10"],
            ),
        );
        assert_eq!(code, 0, "an answered eval is not an error: {shown}");

        let recorded = rows(&data);
        assert_eq!(recorded.len(), 1, "one evaluation, one line: {recorded:?}");
        let row = &recorded[0];
        assert_eq!(row["source"], "chunk", "{row}");
        assert_eq!(row["path"], Value::Null, "a chunk has no path: {row}");
        assert_eq!(
            row["sha256"],
            Value::Null,
            "and no digest, because no reader took one: {row}"
        );
        assert_eq!(row["host"], "hook", "{row}");
        assert_eq!(row["state"], "hook", "{row}");
        assert_eq!(row["status"], "ok", "{row}");
        assert_eq!(row["stamp"], s.stamp.as_str(), "{row}");
        assert_eq!(row["bytes"], 13, "the chunk's own length: {row}");

        let reply = protocol::parse(&published(&s)).expect("the published reply parses");
        assert_eq!(
            row["id"].as_str(),
            Some(reply.headers.get("id").expect("the reply carries an id")),
            "the line names the id the reply came back under: {row}"
        );
        let ts = row["ts"].as_str().expect("the line is stamped");
        assert!(
            ts.len() == 16 && ts.ends_with('Z') && ts.as_bytes()[8] == b'T',
            "stamped the way this build writes an instant: {ts}"
        );

        // A second chunk into the same directory. The record is appended to,
        // not rewritten: a writer that truncated would leave one line behind
        // and every assertion above it would still hold.
        let first = row.clone();
        let again = "return marker, 2";
        let (code, shown) = ran(
            &mut s,
            args(&box_, &["eval", "hook", again, "--wait-seconds", "10"]),
        );
        assert_eq!(code, 0, "the second eval is answered too: {shown}");
        let recorded = rows(&data);
        assert_eq!(
            recorded.len(),
            2,
            "two evaluations, two lines: {recorded:?}"
        );
        assert_eq!(
            recorded[0], first,
            "the first line is where it was, unaltered"
        );
        assert_eq!(
            recorded[1]["bytes"],
            Value::from(again.len()),
            "and the second line is the second chunk's: {}",
            recorded[1]
        );
    }

    /// The provenance claim, and what the mutation for this row breaks: the
    /// digest on the line is the one the reader took over the bytes it read.
    ///
    /// A file eval cannot be driven from the command line yet — no root can
    /// be allowed, so every path is refused above the reader — so this walks
    /// the same three calls `tools::eval_file` walks, in the same order, and
    /// hands the result to the same `line`. The reply it is paired with
    /// carries a body the file does not contain, so a digest recomputed from
    /// what came back cannot coincide with the file's.
    #[test]
    fn a_file_eval_records_the_path_and_the_readers_own_hash() {
        let box_ = Sandbox::new();
        let opts = opts(&box_);
        let probe = box_.join("probe.lua");
        fs::write(&probe, b"return 1\n").expect("the probe is written");
        let mut s = Standin::open(&opts.output(), "hook").expect("the stand-in opens");
        ticking(&mut s);

        let serve = Serve::new(opts);
        let client = serve.client_at(Host::Hook).expect("the session is found");
        let real = paths::resolve(&probe).expect("the probe resolves");
        let roots = Roots::new(
            &[paths::resolve(&box_.path).expect("the box resolves")],
            &[],
            None,
        )
        .expect("the roots are built");
        let chunkname = source::chunkname(&real).expect("the name fits");
        let headers = vec![
            ("op", "eval"),
            ("for", client.handshake().stamp.as_str()),
            ("state", "hook"),
            ("chunkname", chunkname.as_str()),
        ];
        let admitted = file::check(&roots, client.handshake(), &headers, &real)
            .expect("the probe is admitted");
        let source = source::read(&admitted).expect("the probe reads");

        // A reply whose body is deliberately not the file's, so the two
        // digests can never coincide.
        let framed = protocol::frame(
            &[
                ("status", "ok"),
                ("id", "0000000042-k3Jd"),
                ("tick", "7"),
                ("cpu_ms", "0.410"),
                ("result_type", "string"),
            ],
            b"nil",
        )
        .expect("the reply frames");
        let envelope = protocol::parse(&framed).expect("the reply parses");
        let item: Option<Result<Outcome, PipeError>> = Some(Ok(Outcome::Reply(envelope)));

        let data = crate::tools::data_dir(&serve).expect("the data directory resolves");
        let run = Run {
            host: Host::Hook,
            state: "hook",
            stamp: client.handshake().stamp.as_str(),
            budget: None,
            chunk: Chunk::File(&source),
        };
        let at = UNIX_EPOCH + Duration::from_secs(1_760_000_000);
        append(&data, &line(&run, at, item.as_ref())).expect("the line is appended");

        let rows = rows(&box_.join("data"));
        assert_eq!(rows.len(), 1, "one evaluation, one line: {rows:?}");
        let row = &rows[0];
        assert_eq!(
            row["sha256"], "0805bfdc02e872ed322a4a4e440ae985f3a4335f4dd59f3d373dfaf2a68a4a3c",
            "the digest is the reader's, over `return 1` and a newline — not \
             over the reply's body: {row}"
        );
        assert_eq!(row["source"], "file", "{row}");
        assert_eq!(row["path"], real.to_string(), "{row}");
        assert_eq!(row["chunkname"], chunkname.as_str(), "{row}");
        assert_eq!(row["bytes"], 9, "{row}");
        assert_eq!(row["bom"], "none", "{row}");
        assert_eq!(row["shebang"], "none", "{row}");
        assert_eq!(row["status"], "ok", "{row}");
        assert_eq!(row["id"], "0000000042-k3Jd", "{row}");
        assert_eq!(row["tick"], 7, "{row}");
        assert_eq!(row["cpu_ms"], 0.41, "{row}");
        assert_eq!(row["ts"], "20251009T085320Z", "{row}");
    }

    /// The other half of the rule: a file eval refused before a byte of it
    /// was read leaves no line at all.
    ///
    /// The absence is only worth something if the writer is wired in, so the
    /// same data directory is then used by an inline eval that does run —
    /// and the record appears. One test, because the two halves are the same
    /// claim: a line means a chunk reached the executor.
    #[test]
    fn a_file_eval_refused_before_a_byte_is_read_records_nothing() {
        let box_ = Sandbox::new();
        let mut s = Standin::open(&opts(&box_).output(), "hook").expect("the stand-in opens");
        ticking(&mut s);
        let probe = box_.join("probe.lua");
        fs::write(&probe, b"return 1\n").expect("the probe is written");
        let data = box_.join("data");

        let mut sink: Vec<u8> = Vec::new();
        let code = crate::cli::run(
            args(
                &box_,
                &[
                    "eval",
                    "hook",
                    "--file",
                    &probe.to_string_lossy(),
                    "--wait-seconds",
                    "0",
                ],
            ),
            &mut sink,
        )
        .expect("the line parses");
        let shown = String::from_utf8_lossy(&sink).trim_end().to_owned();
        assert_eq!(code, 1, "the path was refused: {shown}");
        assert_eq!(
            shown.lines().next(),
            Some("refused"),
            "and the answer says so: {shown}"
        );
        assert!(
            !data.exists(),
            "nothing was read, so nothing was recorded — and the data \
             directory is made by the writing, so an absent one is the proof"
        );

        s.script("marker", "ok", "string", b"42");
        let (code, shown) = ran(
            &mut s,
            args(
                &box_,
                &["eval", "hook", "return marker", "--wait-seconds", "10"],
            ),
        );
        assert_eq!(code, 0, "an answered eval is not an error: {shown}");
        assert_eq!(
            rows(&data).len(),
            1,
            "a chunk that did run recorded one line, so the absence above \
             was a record withheld rather than a writer never wired in"
        );
    }
}
