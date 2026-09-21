//! How an answer is worded, in one place.
//!
//! Every answer the tools give is rendered by a function here. One module
//! rather than a paragraph in each tool body, because the questions this
//! settles — does a refusal read as a refusal rather than as an empty
//! result, does a `pending` name its id and its phase — are the same six
//! times over, and six copies of an answer is five chances to word one of
//! them differently.
//!
//! The command line prints what these functions render, rather than what a
//! formatter of its own would make of the same reply. That is the only way
//! the two can be held to the same wording, and it is what [`text`] is for:
//! the terminal shows the blocks the way a client shows them.

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

/// Every text block of an answer, joined the way a reader sees it.
///
/// A client shows the blocks it was handed one after another, and the command
/// line has one stream to print to — so this is what "the same words" means
/// for a terminal, and it is the one function that decides it.
pub fn text(answer: &CallToolResult) -> String {
    answer
        .content
        .iter()
        .filter_map(|block| block.as_text().map(|block| block.text.as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A reply's headers, a line each, a blank line, then its body.
///
/// The blank line is the wire's own framing, and it is always there, an
/// empty body included: without it the last header and a body that is one
/// line of `name: value` would read alike, and with it the first blank line
/// ends the headers however the body begins.
fn reply_lines(envelope: &Envelope) -> Vec<String> {
    let mut lines: Vec<String> = envelope
        .headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect();
    lines.push(String::new());
    lines.push(String::from_utf8_lossy(&envelope.body).into_owned());
    lines
}

/// The word a reply is headed with: its `status`, except where that is
/// `error`, when it is the `stage` that failed.
///
/// A deliberate second, independent read of those two headers. The read
/// grammar splits them too, for its own purposes and in its own shape; what
/// is wanted here is one word a reader can see at a glance, and tying the
/// rendering to a grammar type would make every later change to one a change
/// to the other. A reply carrying neither header is headed `error`, which
/// the verdict below refuses — an unreadable reply is not an answer.
fn word_of(envelope: &Envelope) -> &str {
    match envelope.headers.get("status") {
        Some("error") | None => envelope.headers.get("stage").unwrap_or("error"),
        Some(status) => status,
    }
}

/// Whether a reply is an answer or a refusal, and what a refusal says.
enum Verdict {
    Answered,
    Refused(&'static str),
}

/// What each word the executor can head a reply with comes to.
///
/// Only `ok` is an answer. Everything else is a refusal carrying a sentence
/// of its own, because a refusal rendered as a plain reply reads to a caller
/// as a successful call that returned nothing — which is the one failure this
/// whole function exists against. The catch-all is a refusal for that reason:
/// a word this build has never heard of is certainly not `ok`, and guessing
/// that it is turns an unknown failure into a silent empty answer.
fn verdict(word: &str) -> Verdict {
    match word {
        "ok" => Verdict::Answered,
        "no-mission" => Verdict::Refused("no mission is loaded, so the chunk never ran"),
        "stale-session" => Verdict::Refused("written for another session; it did not run"),
        "unsupported" => Verdict::Refused("this state does not offer what was asked"),
        "invalid-state" => Verdict::Refused("no such Lua state on this host"),
        "bad-request" => Verdict::Refused("the request was malformed; nothing ran"),
        "no-session" => Verdict::Refused("no executor session answered; nothing ran"),
        "refused" => Verdict::Refused("the executor refused the request"),
        "oversize" => Verdict::Refused("the result was refused whole, not cut"),
        "budget" => Verdict::Refused("the chunk ran past its instruction budget"),
        "compile" => Verdict::Refused("the chunk did not compile, so nothing ran"),
        "run" => Verdict::Refused("the chunk raised while it was running"),
        "dostring_in" => Verdict::Refused("the state refused the chunk before it ran"),
        "a_do_script" => Verdict::Refused("the mission state was never reached"),
        _ => Verdict::Refused("a refusal this build has no sentence for"),
    }
}

/// One reply off the wire, rendered.
///
/// An answer is headed `reply` and carries its headers and body. A refusal is
/// headed by the word that refused it, says in one line why, and is marked an
/// error — so it can never be mistaken for an answer that came back empty.
/// The reason goes before the headers, which is what keeps a refusal
/// non-empty however little the reply itself carried.
pub fn reply(envelope: &Envelope) -> CallToolResult {
    let word = word_of(envelope);
    match verdict(word) {
        Verdict::Answered => say("reply", reply_lines(envelope)),
        Verdict::Refused(why) => {
            let mut lines = vec![why.to_owned()];
            lines.extend(reply_lines(envelope));
            refuse(word, lines)
        }
    }
}

/// What a single-request window yielded, rendered.
pub fn answered(item: Option<Result<Outcome, PipeError>>) -> CallToolResult {
    match item {
        Some(Ok(Outcome::Reply(envelope))) => reply(&envelope),
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

#[cfg(test)]
mod tests {
    use super::*;
    use dcs_eval::protocol;

    /// A reply built from the bytes the executor would really write, framed
    /// and parsed back, so every word asserted below is read off the wire
    /// rather than off a struct assembled to suit the test.
    fn envelope(headers: &[(&str, &str)], body: &str) -> Envelope {
        let bytes = protocol::frame(headers, body.as_bytes()).expect("the headers frame");
        protocol::parse(&bytes).expect("what was framed parses back")
    }

    /// The head word of an answer: the first line of its text.
    fn head(answer: &CallToolResult) -> String {
        text(answer).lines().next().unwrap_or_default().to_owned()
    }

    /// The row this work is proved by. Every status and stage the executor
    /// can refuse under, each checked for the three things that separate a
    /// refusal from an empty answer: it is marked an error, it is headed by
    /// the word that refused it, and it says something past that word.
    #[test]
    fn every_refusing_status_reads_as_a_non_empty_refusal() {
        let cases: [(&str, Vec<(&str, &str)>); 12] = [
            ("no-mission", vec![("status", "no-mission")]),
            (
                "stale-session",
                vec![("status", "stale-session"), ("for", "0000000000-deadbeef")],
            ),
            ("unsupported", vec![("status", "unsupported")]),
            ("bad-request", vec![("status", "bad-request")]),
            (
                "invalid-state",
                vec![("status", "invalid-state"), ("stage", "eval")],
            ),
            ("refused", vec![("status", "refused")]),
            (
                "oversize",
                vec![
                    ("status", "error"),
                    ("stage", "oversize"),
                    ("result_bytes", "1048577"),
                ],
            ),
            (
                "budget",
                vec![
                    ("status", "error"),
                    ("stage", "budget"),
                    ("chunkname", "probe"),
                ],
            ),
            ("compile", vec![("status", "error"), ("stage", "compile")]),
            ("run", vec![("status", "error"), ("stage", "run")]),
            (
                "dostring_in",
                vec![("status", "error"), ("stage", "dostring_in")],
            ),
            (
                "a_do_script",
                vec![("status", "error"), ("stage", "a_do_script")],
            ),
        ];

        for (word, headers) in cases {
            let answer = reply(&envelope(&headers, "something went wrong"));
            assert_eq!(answer.is_error, Some(true), "{word} is marked an error");
            let rendered = text(&answer);
            assert_eq!(head(&answer), word, "{word} heads its own refusal");
            assert_ne!(
                rendered.trim(),
                word,
                "{word} says nothing but its own name"
            );
            let second = rendered.lines().nth(1).unwrap_or_default();
            assert!(
                !second.trim().is_empty(),
                "{word} is followed by a blank line rather than a reason"
            );
        }
    }

    /// The named mutation's target, on its own. A result over the ceiling was
    /// refused whole: nothing came back, and a caller told this as a plain
    /// reply would read it as a call that succeeded and returned nothing.
    #[test]
    fn an_oversize_reply_is_a_refusal_and_not_an_empty_answer() {
        let answer = reply(&envelope(
            &[
                ("status", "error"),
                ("stage", "oversize"),
                ("chunkname", "probe"),
                ("result_bytes", "1048577"),
            ],
            "the result is 1048577 bytes, over the ceiling",
        ));

        assert_eq!(answer.is_error, Some(true), "an oversize reply is an error");
        assert_eq!(head(&answer), "oversize");
        let rendered = text(&answer);
        assert!(
            rendered.contains("result_bytes: 1048577"),
            "the size the reply named survives: {rendered}"
        );
        assert!(
            rendered.contains("refused whole"),
            "the refusal says why in its own line: {rendered}"
        );
    }

    /// A word off the wire that this build has never heard of. It is still a
    /// refusal, because the alternative is a newer executor's failure reading
    /// here as a successful empty answer.
    #[test]
    fn a_status_word_this_build_does_not_know_is_still_a_refusal() {
        let answer = reply(&envelope(&[("status", "teapot")], ""));

        assert_eq!(answer.is_error, Some(true));
        assert_eq!(head(&answer), "teapot");
        assert!(
            text(&answer).lines().count() > 1,
            "an unknown refusal still says more than its own name"
        );
    }

    /// What says the three above are not passing on a renderer that refuses
    /// everything: a reply that really is an answer is not marked an error,
    /// is headed `reply`, and carries what the chunk returned.
    #[test]
    fn an_ok_reply_is_not_marked_as_an_error() {
        let answer = reply(&envelope(
            &[
                ("status", "ok"),
                ("result_type", "string"),
                ("chunkname", "probe"),
            ],
            "7",
        ));

        assert_ne!(answer.is_error, Some(true), "an ok reply is not an error");
        assert_eq!(head(&answer), "reply");
        let rendered = text(&answer);
        assert!(rendered.contains("result_type: string"), "{rendered}");
        assert!(
            rendered.ends_with('7'),
            "the body is the last line: {rendered}"
        );
    }

    /// The body is set off from the headers by one blank line, as it is on
    /// the wire, so the last header is never read as the first line of the
    /// body.
    #[test]
    fn an_ok_reply_has_a_blank_line_between_its_headers_and_its_body() {
        let answer = reply(&envelope(
            &[("status", "ok"), ("result_type", "string")],
            "7",
        ));

        assert_eq!(text(&answer), "reply\nstatus: ok\nresult_type: string\n\n7");
    }

    /// An empty body still gets the blank line, and the text ends on it: a
    /// reply that returned nothing is the headers and then nothing, not the
    /// headers alone.
    #[test]
    fn an_empty_body_ends_in_the_blank_line() {
        let answer = reply(&envelope(&[("status", "ok"), ("result_type", "nil")], ""));

        assert_eq!(text(&answer), "reply\nstatus: ok\nresult_type: nil\n\n");
    }

    /// A `pending` names what a caller needs to pick the reply up, and is not
    /// an error: nothing failed, the request is published, and the answer is
    /// collected later.
    #[test]
    fn a_pending_names_its_id_and_phase_and_is_not_an_error() {
        let answer = answered(Some(Ok(Outcome::Pending {
            id: "0000000001-abcd1234".to_owned(),
            phase: "load".to_owned(),
            flag: Some(Flag::Waking),
        })));

        assert_ne!(answer.is_error, Some(true), "a pending is not an error");
        assert_eq!(head(&answer), "pending");
        let rendered = text(&answer);
        assert!(rendered.contains("id: 0000000001-abcd1234"), "{rendered}");
        assert!(rendered.contains("phase: load"), "{rendered}");
        assert!(rendered.contains("waiting on:"), "{rendered}");
    }

    /// The same for the other way a `pending` is reached: a collect that
    /// found nothing under the id. It names a phase there too, because a
    /// caller deciding whether to wait again needs the same two facts
    /// whichever call produced the answer.
    #[test]
    fn a_pending_with_nothing_collected_still_names_a_phase() {
        let answer = pending(
            "0000000001-abcd1234",
            dcs_eval::wait::PHASE_UNKNOWN,
            None,
            Some("nothing has landed under that id yet"),
        );

        assert_ne!(answer.is_error, Some(true));
        let rendered = text(&answer);
        assert!(rendered.contains("id: 0000000001-abcd1234"), "{rendered}");
        assert!(rendered.contains("phase: unknown"), "{rendered}");
        assert!(rendered.contains("nothing has landed"), "{rendered}");
    }

    /// The two terminal outcomes: the request will never run, and saying so
    /// as a plain answer would leave a caller waiting on a reply that is not
    /// coming.
    #[test]
    fn a_superseded_and_a_dead_session_read_as_refusals() {
        let id = "0000000001-abcd1234";
        for (outcome, word) in [
            (Outcome::Superseded { id: id.to_owned() }, "stale-session"),
            (Outcome::Dead { id: id.to_owned() }, "no-session"),
        ] {
            let answer = answered(Some(Ok(outcome)));
            assert_eq!(answer.is_error, Some(true), "{word} is marked an error");
            assert_eq!(head(&answer), word);
            assert!(text(&answer).contains(id), "{word} names the id it refused");
        }
    }

    /// A window that ended without yielding anything at all. There is no
    /// reply to word, and the answer still has to read as a refusal rather
    /// than as an empty success.
    #[test]
    fn a_window_that_yielded_nothing_reads_as_a_refusal() {
        let answer = answered(None);

        assert_eq!(answer.is_error, Some(true));
        assert_eq!(head(&answer), "refused");
        let second = text(&answer).lines().nth(1).unwrap_or_default().to_owned();
        assert!(!second.trim().is_empty(), "it says why: {second}");
    }
}
