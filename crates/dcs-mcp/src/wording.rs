//! How an answer is worded, in one place.
//!
//! Every answer the tools give is rendered by a function here. One module
//! rather than a paragraph in each tool body, because the questions this
//! settles — does a refusal read as a refusal rather than as an empty
//! result, does a `pending` name its id and its phase — are the same for
//! every tool, and a copy of an answer per tool is a chance to word one of
//! them differently.
//!
//! The command line prints what these functions render, rather than what a
//! formatter of its own would make of the same reply. That is the only way
//! the two can be held to the same wording, and it is what [`text`] is for:
//! the terminal shows the blocks the way a client shows them.

use dcs_eval::pipeline::PipeError;
use dcs_eval::protocol::Envelope;
use dcs_eval::screenshot::{Capture, CaptureError};
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
    let mut lines = pending_lines(id, phase, flag);
    if let Some(note) = note {
        lines.push(note.to_owned());
    }
    say("pending", lines)
}

/// The lines every `pending` opens with, a capture's included.
fn pending_lines(id: &str, phase: &str, flag: Option<Flag>) -> Vec<String> {
    let mut lines = vec![format!("id: {id}"), format!("phase: {phase}")];
    if let Some(flag) = flag {
        lines.push(format!("waiting on: {flag:?}"));
    }
    lines
}

/// What one capture came to, rendered.
///
/// `ok` carries the file and what its header says, a line each. A refusal is
/// headed by the word that refused it and marked an error, whether the word
/// is this build's own or the executor's, which is passed through the reply
/// renderer unchanged. `pending` and `not-written` are the two that are not
/// refusals, and neither is marked one: the first is a reply still to come,
/// the second a reply that came while the file did not, and the file may yet
/// land. Both carry the directory and the name rather than a path, because
/// the extension is the user's setting and nothing has read it yet.
pub fn capture(outcome: Result<Capture, CaptureError>) -> CallToolResult {
    let capture = match outcome {
        Ok(capture) => capture,
        Err(why) => return refuse("refused", vec![why.to_string()]),
    };
    match capture {
        Capture::Ok { path, picture } => say(
            "ok",
            vec![
                format!("path: {}", path.display()),
                format!("format: {}", picture.format.extension()),
                format!("bytes: {}", picture.bytes),
                format!("width: {}", picture.width),
                format!("height: {}", picture.height),
            ],
        ),
        Capture::Pending {
            id,
            phase,
            flag,
            dir,
            name,
        } => {
            let mut lines = pending_lines(&id, &phase, flag);
            lines.push(match dir {
                Some(dir) => format!("directory: {}", dir.display()),
                // Decision record 0039: the directory is read off where the
                // session's output sits, and only this executor's layout
                // says anything about that.
                None => "directory: not known; the session's output is not where this \
                         executor puts its own, so the write directory cannot be read off it"
                    .to_owned(),
            });
            lines.push(format!("name: {name}"));
            lines.push(
                "the executor has not answered yet; collect the id for the directory, \
                 then look there for the name"
                    .to_owned(),
            );
            say("pending", lines)
        }
        Capture::NotWritten { dir, name } => {
            let lines = vec![
                format!("directory: {}", dir.display()),
                format!("name: {name}"),
                "the executor answered and no whole file under the name landed inside the \
                 wait; it may still land, and where nothing renders it never will"
                    .to_owned(),
            ];
            say("not-written", lines)
        }
        Capture::Empty { path } => refuse(
            "empty",
            vec![
                format!("path: {}", path.display()),
                "a file landed under the name and was still zero bytes when the wait ended, \
                 which is what an abandoned capture leaves"
                    .to_owned(),
            ],
        ),
        Capture::BadName(why) => refuse("bad-request", vec![why.to_string()]),
        Capture::Unsupported { host } => refuse(
            "unsupported",
            vec![format!(
                "the capture runs in the hook state, which the {host} host cannot reach; \
                 ask host hook"
            )],
        ),
        Capture::Replied(envelope) => reply(&envelope),
        Capture::Malformed(envelope) => {
            let mut lines =
                vec!["the reply is not the directory the capture's chunk returns".to_owned()];
            lines.extend(reply_lines(&envelope));
            refuse("refused", lines)
        }
        Capture::Superseded { id } => answered(Some(Ok(Outcome::Superseded { id }))),
        Capture::Dead { id } => answered(Some(Ok(Outcome::Dead { id }))),
    }
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

    /// A capture that found its file: headed `ok`, not an error, and each
    /// of the five facts on a line of its own.
    #[test]
    fn a_capture_found_is_ok_with_its_five_facts() {
        use dcs_eval::shot_file::{Format, Picture};
        let answer = capture(Ok(Capture::Ok {
            path: "C:\\Saved Games\\DCS\\ScreenShots\\shot.png".into(),
            picture: Picture {
                format: Format::Png,
                bytes: 1234,
                width: 37,
                height: 23,
            },
        }));

        assert_ne!(answer.is_error, Some(true), "an ok capture is not an error");
        assert_eq!(
            text(&answer),
            "ok\npath: C:\\Saved Games\\DCS\\ScreenShots\\shot.png\nformat: png\n\
             bytes: 1234\nwidth: 37\nheight: 23"
        );
    }

    /// The two answers that are not refusals. Each names the directory and
    /// the name rather than a path, and neither is marked an error.
    #[test]
    fn a_capture_pending_or_not_written_is_not_an_error() {
        let dir = std::path::PathBuf::from("C:\\Saved Games\\DCS\\ScreenShots");
        let pending = capture(Ok(Capture::Pending {
            id: "0000000001-abcd1234".to_owned(),
            phase: "menu".to_owned(),
            flag: None,
            dir: Some(dir.clone()),
            name: "shot".to_owned(),
        }));
        let not_written = capture(Ok(Capture::NotWritten {
            dir,
            name: "shot".to_owned(),
        }));

        for (answer, word) in [(pending, "pending"), (not_written, "not-written")] {
            let rendered = text(&answer);
            assert_ne!(answer.is_error, Some(true), "{word} is not an error");
            assert_eq!(head(&answer), word, "{rendered}");
            assert!(
                rendered.contains("directory: C:\\Saved Games\\DCS\\ScreenShots"),
                "{rendered}"
            );
            assert!(rendered.contains("name: shot"), "{rendered}");
        }
    }

    /// A `pending` whose directory could not be read off the session says
    /// so, rather than dropping the line or printing an empty path.
    #[test]
    fn a_capture_pending_with_no_directory_says_why() {
        let answer = capture(Ok(Capture::Pending {
            id: "0000000001-abcd1234".to_owned(),
            phase: "load".to_owned(),
            flag: Some(Flag::Waking),
            dir: None,
            name: "shot".to_owned(),
        }));

        assert_ne!(answer.is_error, Some(true));
        let rendered = text(&answer);
        assert!(rendered.contains("id: 0000000001-abcd1234"), "{rendered}");
        assert!(rendered.contains("phase: load"), "{rendered}");
        assert!(rendered.contains("directory: not known; "), "{rendered}");
        assert!(rendered.contains("name: shot"), "{rendered}");
    }

    /// Every capture refusal this build words itself: marked an error,
    /// headed by its word, and saying why on the line after it.
    #[test]
    fn a_capture_refusal_is_headed_by_its_word() {
        use dcs_eval::shot_name::NameRefusal;
        let cases = [
            (
                capture(Ok(Capture::Empty {
                    path: "C:\\ScreenShots\\shot".into(),
                })),
                "empty",
                "path: C:\\ScreenShots\\shot",
            ),
            (
                capture(Ok(Capture::BadName(NameRefusal::Character {
                    at: 2,
                    character: '.',
                }))),
                "bad-request",
                "'.'",
            ),
            (
                capture(Ok(Capture::Unsupported {
                    host: "export".to_owned(),
                })),
                "unsupported",
                "ask host hook",
            ),
            (
                capture(Ok(Capture::Malformed(envelope(
                    &[("status", "ok"), ("result_type", "nil")],
                    "",
                )))),
                "refused",
                "not the directory",
            ),
            (
                capture(Ok(Capture::Superseded {
                    id: "0000000001-abcd1234".to_owned(),
                })),
                "stale-session",
                "0000000001-abcd1234",
            ),
            (
                capture(Ok(Capture::Dead {
                    id: "0000000001-abcd1234".to_owned(),
                })),
                "no-session",
                "0000000001-abcd1234",
            ),
        ];
        for (answer, word, says) in cases {
            let rendered = text(&answer);
            assert_eq!(
                answer.is_error,
                Some(true),
                "{word} is an error: {rendered}"
            );
            assert_eq!(head(&answer), word, "{rendered}");
            assert!(rendered.contains(says), "{word} says {says:?}: {rendered}");
        }
    }

    /// The executor's own refusal of the chunk is its word, not this
    /// build's: a chunk that raised reads as `run`, as it would from an eval.
    #[test]
    fn a_capture_the_executor_refused_is_passed_through() {
        let answer = capture(Ok(Capture::Replied(envelope(
            &[("status", "error"), ("stage", "run")],
            "attempt to call a nil value",
        ))));

        assert_eq!(answer.is_error, Some(true));
        assert_eq!(head(&answer), "run");
        assert!(text(&answer).ends_with("attempt to call a nil value"));
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
