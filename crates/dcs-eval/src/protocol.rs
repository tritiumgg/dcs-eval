//! The envelope: what a request, a reply, the handshake and the heartbeat
//! all look like on the disk, and how the client writes one and reads one
//! back.
//!
//! An envelope is header lines, one blank line, then a body that is
//! everything after it byte for byte. It needs no quoting rule, because the
//! body is the only place a newline may appear, and that holds only while a
//! writer refuses a header value carrying one rather than escaping it: a
//! value with a newline inside would end its own line early and the rest
//! would be read as a header nobody wrote. So a bad header is refused and
//! nothing is written.
//!
//! The other end of the wire is the executor, whose `frame` and `parse` in
//! `DcsEvalExecutor.lua` define the bytes. Everything here is checked
//! against what those two produce and accept, case by case, and the
//! refusals spell their reasons the way the executor spells them, so a log
//! line from either end reads the same.
//!
//! The body is bytes and is never decoded. DCS's own strings mix UTF-8 and
//! cp1251, and a chunk may return either; a body that went through a
//! string type on the way in would come out as something else.

use std::fmt;

/// The envelope version. The executor writes `protocol: 2` in the handshake,
/// the heartbeat and every reply, and a reader refuses a mismatch outright.
pub const PROTOCOL: u32 = 2;

/// Why `frame` wrote nothing. `Display` spells each the way the executor's
/// `frame` does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// A name outside `[A-Za-z0-9_-]+`, counted from 1 where it sits in the
    /// list.
    Name { index: usize, name: String },
    /// A name already written, compared without regard to case.
    Repeated { name: String },
    /// A value carrying a CR or an LF.
    LineBreak { name: String },
    /// A value with a byte past ASCII.
    NotAscii { name: String },
    /// A value beginning with whitespace, which a reader strips and would
    /// lose.
    LeadingBlank { name: String },
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name { index, name } => {
                write!(f, "header {index}: the name {name} is not [A-Za-z0-9_-]+")
            }
            Self::Repeated { name } => write!(f, "{name}: repeated"),
            Self::LineBreak { name } => write!(f, "{name}: the value carries a CR or LF"),
            Self::NotAscii { name } => write!(f, "{name}: the value is not ASCII"),
            Self::LeadingBlank { name } => {
                write!(
                    f,
                    "{name}: the value begins with whitespace, which a reader strips"
                )
            }
        }
    }
}

impl std::error::Error for FrameError {}

/// The bytes a header name is made of. Spelt out rather than taken from a
/// character class, so the rule is the same on every host and in every
/// locale, as the executor spells it.
fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// The whitespace a reader drops from the front of a value, and so the
/// whitespace a writer refuses there.
fn is_leading_blank(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | 0x0B | 0x0C)
}

/// The bytes of an envelope: each header as `name: value` on its own line,
/// in the order given, then one blank line, then `body` byte for byte.
///
/// `headers` is a list rather than a map because the order is the wire's:
/// the executor writes `status` first so a reader with one line has the
/// verdict, and a request's order is whatever the caller chose. A value is
/// text the caller has already spelt, a number included, so what goes on
/// the wire is visible where it is written.
///
/// A header the executor's `parse` would refuse is refused here, before
/// anything is produced, and the checks run in the executor's order so the
/// same bad header draws the same reason from both ends. The body is not
/// inspected.
pub fn frame(headers: &[(&str, &str)], body: &[u8]) -> Result<Vec<u8>, FrameError> {
    let mut out = Vec::new();
    let mut seen: Vec<&str> = Vec::with_capacity(headers.len());
    for (i, &(name, value)) in headers.iter().enumerate() {
        if name.is_empty() || !name.bytes().all(is_name_byte) {
            return Err(FrameError::Name {
                index: i + 1,
                name: name.to_owned(),
            });
        }
        if seen.iter().any(|s| s.eq_ignore_ascii_case(name)) {
            return Err(FrameError::Repeated {
                name: name.to_owned(),
            });
        }
        seen.push(name);
        if value.bytes().any(|b| b == b'\r' || b == b'\n') {
            return Err(FrameError::LineBreak {
                name: name.to_owned(),
            });
        }
        if !value.is_ascii() {
            return Err(FrameError::NotAscii {
                name: name.to_owned(),
            });
        }
        if value.bytes().next().is_some_and(is_leading_blank) {
            return Err(FrameError::LeadingBlank {
                name: name.to_owned(),
            });
        }
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(b": ");
        out.extend_from_slice(value.as_bytes());
        out.push(b'\n');
    }
    out.push(b'\n');
    out.extend_from_slice(body);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bytes `frame` writes for headers it accepts.
    fn framed(headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
        frame(headers, body).expect("the envelope frames")
    }

    /// The reason `frame` refused, as the executor would spell it.
    fn refused(headers: &[(&str, &str)]) -> (FrameError, String) {
        let err = frame(headers, b"").expect_err("the header is refused");
        let why = err.to_string();
        (err, why)
    }

    // ---- the envelope -----------------------------------------------------

    #[test]
    fn frame_one_header_the_blank_line_the_body() {
        assert_eq!(
            framed(&[("status", "ok")], b"hello"),
            b"status: ok\n\nhello"
        );
    }

    #[test]
    fn frame_keeps_the_order_and_an_empty_body_ends_at_the_blank_line() {
        assert_eq!(
            framed(&[("status", "ok"), ("id", "1-a"), ("tick", "7")], b""),
            b"status: ok\nid: 1-a\ntick: 7\n\n"
        );
    }

    #[test]
    fn frame_no_headers_is_the_blank_line_and_the_body() {
        assert_eq!(framed(&[], b"x"), b"\nx");
        assert_eq!(framed(&[], b""), b"\n");
    }

    #[test]
    fn frame_body_passes_verbatim() {
        // CRLF, NUL, bytes past ASCII and a line shaped like a header: the
        // body is never inspected.
        let body = b"line\r\nnul\0x\xff\x80 status: fake\n";
        let mut want = b"status: ok\n\n".to_vec();
        want.extend_from_slice(body);
        assert_eq!(framed(&[("status", "ok")], body), want);
    }

    #[test]
    fn frame_keeps_a_tab_inside_a_value() {
        assert_eq!(
            framed(&[("chunkname", "a\tb")], b""),
            b"chunkname: a\tb\n\n"
        );
    }

    #[test]
    fn frame_name_may_carry_case_an_underscore_and_a_dash() {
        assert_eq!(
            framed(&[("Status", "ok"), ("cpu_ms", "0.5"), ("x-y", "1")], b""),
            b"Status: ok\ncpu_ms: 0.5\nx-y: 1\n\n"
        );
    }

    #[test]
    fn frame_the_executors_reply_shape() {
        // The seven headers the executor puts before a caller's, then the
        // caller's, then the framer suite's body, byte for byte.
        let protocol = PROTOCOL.to_string();
        let headers = [
            ("status", "ok"),
            ("protocol", protocol.as_str()),
            ("host", "hook"),
            ("stamp", "0000000001-abcd"),
            ("phase", "menu"),
            ("id", "0000000001-abcd"),
            ("tick", "0"),
            ("result_type", "string"),
        ];
        let body = b"\xd0\xbf\xd1\x80\0\xff caf\xe9\r\nstatus: fake\n";
        let mut want =
            b"status: ok\nprotocol: 2\nhost: hook\nstamp: 0000000001-abcd\nphase: menu\n\
                         id: 0000000001-abcd\ntick: 0\nresult_type: string\n\n"
                .to_vec();
        want.extend_from_slice(body);
        assert_eq!(framed(&headers, body), want);
    }

    // ---- refusals ---------------------------------------------------------

    #[test]
    fn frame_refuses_an_lf_in_a_value_the_injection_guard() {
        // Written, `ok\nstatus: fake` would be read as two headers, the
        // second one nobody wrote. Refused, not escaped, and nothing
        // produced: `Err` carries no bytes.
        let (err, why) = refused(&[("x", "a\nb")]);
        assert_eq!(err, FrameError::LineBreak { name: "x".into() });
        assert_eq!(why, "x: the value carries a CR or LF");
        assert!(frame(&[("status", "ok\nstatus: fake")], b"").is_err());
    }

    #[test]
    fn frame_refuses_a_cr_in_a_value() {
        let (err, why) = refused(&[("x", "a\rb")]);
        assert_eq!(err, FrameError::LineBreak { name: "x".into() });
        assert_eq!(why, "x: the value carries a CR or LF");
    }

    #[test]
    fn frame_refuses_a_crlf_at_the_end_of_a_value() {
        let (err, _) = refused(&[("x", "a\r\n")]);
        assert_eq!(err, FrameError::LineBreak { name: "x".into() });
    }

    #[test]
    fn frame_refuses_a_byte_past_ascii() {
        let (err, why) = refused(&[("x", "caf\u{e9}")]);
        assert_eq!(err, FrameError::NotAscii { name: "x".into() });
        assert_eq!(why, "x: the value is not ASCII");
    }

    #[test]
    fn frame_refuses_a_value_beginning_with_whitespace() {
        // A reader strips leading blanks, so a value written with one would
        // come back without it.
        for value in [" a", "\ta", "\u{0B}a", "\u{0C}a"] {
            let (err, why) = refused(&[("x", value)]);
            assert_eq!(
                err,
                FrameError::LeadingBlank { name: "x".into() },
                "{value:?}"
            );
            assert_eq!(
                why,
                "x: the value begins with whitespace, which a reader strips"
            );
        }
    }

    #[test]
    fn frame_refuses_a_name_outside_the_alphabet() {
        for name in ["", "a b", "a:b", "caf\u{e9}"] {
            let (err, why) = refused(&[(name, "a")]);
            assert_eq!(
                err,
                FrameError::Name {
                    index: 1,
                    name: name.into()
                },
                "{name:?}"
            );
            assert!(why.starts_with("header 1: the name "), "{why}");
            assert!(why.ends_with(" is not [A-Za-z0-9_-]+"), "{why}");
        }
    }

    #[test]
    fn frame_counts_a_bad_name_where_it_sits() {
        let (err, why) = refused(&[("ok", "1"), ("a b", "a")]);
        assert_eq!(
            err,
            FrameError::Name {
                index: 2,
                name: "a b".into()
            }
        );
        assert_eq!(why, "header 2: the name a b is not [A-Za-z0-9_-]+");
    }

    #[test]
    fn frame_refuses_a_name_repeated_in_another_case() {
        let (err, why) = refused(&[("id", "1"), ("ID", "2")]);
        assert_eq!(err, FrameError::Repeated { name: "ID".into() });
        assert_eq!(why, "ID: repeated");
    }

    #[test]
    fn frame_checks_in_the_executors_order() {
        // A header wrong in more than one way draws the reason the executor
        // would give: the name before the value, and a repeat before what
        // the repeated value carries.
        let (err, _) = refused(&[("a b", "x\ny")]);
        assert!(matches!(err, FrameError::Name { .. }));
        let (err, _) = refused(&[("a", "1"), ("A", "x\ny")]);
        assert!(matches!(err, FrameError::Repeated { .. }));
        let (err, _) = refused(&[("a", "\u{e9}\n")]);
        assert!(matches!(err, FrameError::LineBreak { .. }));
        let (err, _) = refused(&[("a", " \u{e9}")]);
        assert!(matches!(err, FrameError::NotAscii { .. }));
    }
}
