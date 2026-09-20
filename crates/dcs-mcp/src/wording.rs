//! How an answer is worded, in one place.
//!
//! Every answer the tools give, and every one the command line prints, is
//! rendered by a function here. One module rather than a paragraph in each
//! tool body, because the questions this settles — does a refusal read as a
//! refusal rather than as an empty result, does a `pending` name its id and
//! its phase — are the same six times over, and six copies of an answer is
//! five chances to word one of them differently.

use dcs_eval::pipeline::PipeError;
use dcs_eval::protocol::Envelope;
use dcs_eval::wait::{Flag, Outcome};
use rmcp::model::{CallToolResult, ContentBlock};

/// An answer: a status word, then a line each.
///
/// Deliberately plain. A client shows the text it is given, so the first
/// line is the one word a reader needs and everything after it is detail.
pub fn say(status: &str, lines: Vec<String>) -> CallToolResult {
    let mut text = status.to_owned();
    for line in lines {
        text.push('\n');
        text.push_str(&line);
    }
    CallToolResult::success(vec![ContentBlock::text(text)])
}

/// The same, for an answer the caller did not get.
///
/// Marked as an error on the result rather than raised as one: the request
/// was routed and ran, and a protocol error would be rendered opaquely by the
/// caller's client, which is exactly where the reason would be lost. A raised
/// error is kept for arguments that will not parse, which the macro layer
/// raises before any of this runs.
pub fn refuse(status: &str, lines: Vec<String>) -> CallToolResult {
    let mut answer = say(status, lines);
    answer.is_error = Some(true);
    answer
}

/// A reply's headers, a line each, then its body.
pub fn reply_lines(envelope: &Envelope) -> Vec<String> {
    let mut lines: Vec<String> = envelope
        .headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect();
    lines.push(String::from_utf8_lossy(&envelope.body).into_owned());
    lines
}

/// What a single-request window yielded, rendered.
pub fn answered(item: Option<Result<Outcome, PipeError>>) -> CallToolResult {
    match item {
        Some(Ok(Outcome::Reply(envelope))) => say("reply", reply_lines(&envelope)),
        Some(Ok(Outcome::Pending { id, phase, flag })) => pending(&id, &phase, flag, None),
        Some(Ok(Outcome::Superseded { id })) => refuse(
            "stale-session",
            vec![
                format!("id: {id}"),
                "DCS restarted; the request did not run and will not".to_owned(),
            ],
        ),
        Some(Ok(Outcome::Dead { id })) => refuse(
            "no-session",
            vec![
                format!("id: {id}"),
                "the process the handshake named is gone".to_owned(),
            ],
        ),
        Some(Err(why)) => refuse("refused", vec![why.to_string()]),
        None => refuse(
            "refused",
            vec!["the window yielded nothing at all".to_owned()],
        ),
    }
}

/// A `pending`: the id to collect under and the phase the session was in.
///
/// Never marked as an error, and that is the point of it having a function
/// of its own. Nothing failed — the request is published, the executor has
/// it, and the reply is picked up later — so a client that branches on the
/// error flag must not be told to give up here.
pub fn pending(id: &str, phase: &str, flag: Option<Flag>, note: Option<&str>) -> CallToolResult {
    let mut lines = vec![format!("id: {id}"), format!("phase: {phase}")];
    if let Some(flag) = flag {
        lines.push(format!("waiting on: {flag:?}"));
    }
    if let Some(note) = note {
        lines.push(note.to_owned());
    }
    say("pending", lines)
}
