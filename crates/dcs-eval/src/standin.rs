//! The stand-in: a test double that plays the executor's side of the wire
//! in this process, so a client control has something to drive that is not
//! the client's own serialiser turned around.
//!
//! It stands in for the executor's `tick` and `reply` in
//! `DcsEvalExecutor.lua`: a session directory with `req` and `res` under a
//! stamp, a tick that lists requests and answers them in name order, a
//! reply with the seven headers the executor puts first, and the `ping`
//! op. What it answers, it answers as the executor does, message for
//! message, because a client that reads a refusal reads the executor's
//! words. Whether those are the executor's bytes is not this module's
//! claim: the interop and round-trip controls make it on the shipped Lua.
//!
//! Nothing here comes from `protocol` or `publish`. The encoder is the
//! point of the exercise, and it is kept apart on purpose: a client parser
//! tested only against `frame` would pass while both were wrong in the same
//! way. It writes a dialect no other producer emits, every header line
//! ending in CRLF, which both the executor's reader and the client's parser
//! accept and normalise, so the bytes are provably not `frame`'s and the
//! client's CRLF path is exercised by something other than a hand-made
//! fixture. The decoder is its own for the same reason, from the other
//! direction: the request half of a round trip would otherwise be the
//! client's `frame` read by the client's `parse`. The disk side is its own
//! too, so a reply reaches the client through code that is not `publish`.
//!
//! Not modelled, because no client control needs it: the held set for a
//! request that could not be removed, the count of replies that could not
//! be published and the `dcs.log` line for one, the dormant probe on the
//! arm file, which the executor does not have yet either, and the
//! handshake and heartbeat files, which arrive with the readers for them.

// ---- the envelope, this side's way -----------------------------------------

/// The bytes of an envelope in the stand-in's dialect: each header as
/// `name: value` ending in CRLF, a CRLF alone to end the block, then the
/// body byte for byte. A refusal covers what a value taken from a filename
/// can carry, a line break or a byte past ASCII, and nothing more: every
/// other header is this module's own composition.
pub fn encode(headers: &[(&str, &str)], body: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(body.len() + 256);
    for &(name, value) in headers {
        if value.contains(['\r', '\n']) {
            return Err(format!("{name}: the value carries a CR or LF"));
        }
        if !value.is_ascii() {
            return Err(format!("{name}: the value is not ASCII"));
        }
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(b": ");
        out.extend_from_slice(value.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(body);
    Ok(out)
}

/// The headers of a request as read, in wire order, and its body.
type Decoded = (Vec<(String, String)>, Vec<u8>);

/// The value under `name` in a decoded header list, without regard to
/// case.
fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// An envelope read the executor's way: a line ends at LF and loses one CR
/// before it, the first empty line ends the headers, a header is a name of
/// `[A-Za-z0-9_-]` up to the first colon and everything after it with
/// leading blanks dropped, and the body is the rest, untouched. The
/// refusals are worded as the executor words them.
fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut rest = bytes;
    let mut count = 0;
    while let Some(nl) = rest.iter().position(|&b| b == b'\n') {
        let (line, after) = (&rest[..nl], &rest[nl + 1..]);
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            return Ok((headers, after.to_vec()));
        }
        count += 1;
        let colon = line
            .iter()
            .take_while(|&&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            .count();
        if colon == 0 || line.get(colon) != Some(&b':') {
            let shown: String = line.iter().take(80).map(|&b| b as char).collect();
            let dots = if line.len() > 80 { "..." } else { "" };
            return Err(format!("line {count} is not a header: {shown}{dots}"));
        }
        let name: String = line[..colon].iter().map(|&b| b as char).collect();
        let value = &line[colon + 1..];
        let blanks = value
            .iter()
            .take_while(|&&b| matches!(b, b' ' | b'\t' | 0x0B | 0x0C))
            .count();
        let value = &value[blanks..];
        if value.contains(&b'\r') {
            return Err(format!("line {count}: {name}: the value carries a CR"));
        }
        if !value.is_ascii() {
            return Err(format!("line {count}: {name}: the value is not ASCII"));
        }
        if header(&headers, &name).is_some() {
            return Err(format!("line {count}: {name}: repeated"));
        }
        headers.push((name, value.iter().map(|&b| b as char).collect()));
        rest = after;
    }
    Err(format!(
        "the headers never end: no blank line before the bytes ran out, after {count} header lines"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{frame, parse};

    fn encoded(headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
        encode(headers, body).expect("the envelope encodes")
    }

    fn decoded(bytes: &[u8]) -> Decoded {
        decode(bytes).expect("the envelope decodes")
    }

    /// The executor's reply shape with a caller's header after it.
    const REPLY: [(&str, &str); 8] = [
        ("status", "ok"),
        ("protocol", "2"),
        ("host", "hook"),
        ("stamp", "0000000001-abcd"),
        ("phase", "menu"),
        ("id", "0000000001-abcd"),
        ("tick", "0"),
        ("result_type", "string"),
    ];

    // ---- the encoder ------------------------------------------------------

    #[test]
    fn encode_one_header_the_blank_line_the_body() {
        assert_eq!(
            encoded(&[("status", "ok")], b"hello"),
            b"status: ok\r\n\r\nhello"
        );
    }

    #[test]
    fn encode_keeps_the_order_and_no_headers_is_the_blank_line() {
        assert_eq!(
            encoded(&[("status", "ok"), ("id", "1-a"), ("tick", "7")], b""),
            b"status: ok\r\nid: 1-a\r\ntick: 7\r\n\r\n"
        );
        assert_eq!(encoded(&[], b"x"), b"\r\nx");
    }

    #[test]
    fn encode_body_passes_verbatim() {
        let body = b"line\r\nnul\0x\xff\x80 status: fake\n\n";
        let mut want = b"status: ok\r\n\r\n".to_vec();
        want.extend_from_slice(body);
        assert_eq!(encoded(&[("status", "ok")], body), want);
    }

    #[test]
    fn encode_refuses_what_a_filename_can_carry_in_the_executors_words() {
        assert_eq!(
            encode(&[("id", "a\nb")], b"").expect_err("an LF"),
            "id: the value carries a CR or LF"
        );
        assert_eq!(
            encode(&[("id", "a\rb")], b"").expect_err("a CR"),
            "id: the value carries a CR or LF"
        );
        assert_eq!(
            encode(&[("id", "caf\u{e9}")], b"").expect_err("a byte past ASCII"),
            "id: the value is not ASCII"
        );
    }

    #[test]
    fn encoder_is_not_the_clients() {
        // The property the stand-in exists for. The same headers and body
        // through both encoders: the bytes differ, the stand-in's header
        // block ends every line in CRLF where the client's ends none, and
        // the client's parser reads the two as one envelope. An encoder
        // swapped for `frame` goes red on the first two.
        let body = b"\xcf\xf0\xe8\xe2\xe5\xf2\r\nstatus: fake\n";
        let ours = encoded(&REPLY, body);
        let theirs = frame(&REPLY, body).expect("the client frames");
        assert_ne!(ours, theirs, "the bytes are not the client's");
        let block = |bytes: &[u8]| {
            let end = bytes
                .windows(2)
                .position(|w| w == b"\n\n" || w == b"\n\r")
                .expect("a blank line");
            bytes[..end + 1].to_vec()
        };
        let ours_block = block(&ours);
        let theirs_block = block(&theirs);
        assert_eq!(
            ours_block.iter().filter(|&&b| b == b'\n').count(),
            REPLY.len()
        );
        assert_eq!(
            ours_block.iter().filter(|&&b| b == b'\r').count(),
            REPLY.len(),
            "every header line of the stand-in's ends in CRLF"
        );
        assert_eq!(
            theirs_block.iter().filter(|&&b| b == b'\r').count(),
            0,
            "and none of the client's does"
        );
        let read = parse(&ours).expect("the client reads the stand-in's bytes");
        assert_eq!(read, parse(&theirs).expect("and its own"));
        assert_eq!(read.body, body);
        assert_eq!(read.headers.iter().collect::<Vec<_>>(), REPLY);
    }

    // ---- the decoder ------------------------------------------------------

    #[test]
    fn decode_an_lf_block_and_a_crlf_block_alike_the_body_verbatim() {
        let chunk = b"return 'a\r\nb'\n\n\xcf\xf0\0\xff for: fake\n";
        let mut lf = b"op: eval\nfor: 1-2\n\n".to_vec();
        lf.extend_from_slice(chunk);
        let mut crlf = b"op: eval\r\nfor: 1-2\r\n\r\n".to_vec();
        crlf.extend_from_slice(chunk);
        let (headers, body) = decoded(&lf);
        assert_eq!(
            headers,
            [
                ("op".to_string(), "eval".to_string()),
                ("for".to_string(), "1-2".to_string())
            ]
        );
        assert_eq!(body, chunk);
        assert_eq!(decoded(&crlf), decoded(&lf));
        assert_eq!(decoded(b"\nx"), (vec![], b"x".to_vec()));
    }

    #[test]
    fn decode_drops_leading_blanks_and_looks_names_up_without_regard_to_case() {
        let (headers, body) = decoded(b"Op:ping\nFOR:   x \nc:\tC:\\y\nd:\n\n");
        assert_eq!(header(&headers, "op"), Some("ping"));
        assert_eq!(header(&headers, "for"), Some("x "), "trailing blanks kept");
        assert_eq!(
            header(&headers, "C"),
            Some("C:\\y"),
            "the first colon splits"
        );
        assert_eq!(header(&headers, "d"), Some(""));
        assert_eq!(header(&headers, "e"), None);
        assert_eq!(body, b"");
    }

    #[test]
    fn decode_refuses_in_the_executors_words() {
        let cases: [(&[u8], &str); 6] = [
            (
                b"for: x\n",
                "the headers never end: no blank line before the bytes ran out, after 1 header lines",
            ),
            (b"for: x\nnocolon\n\n", "line 2 is not a header: nocolon"),
            (b" for: x\n\n", "line 1 is not a header:  for: x"),
            (b"for: a\rb\n\n", "line 1: for: the value carries a CR"),
            (b"for: caf\xe9\n\n", "line 1: for: the value is not ASCII"),
            (b"for: x\nop: a\nFOR: y\n\n", "line 3: FOR: repeated"),
        ];
        for (bytes, why) in cases {
            assert_eq!(decode(bytes).expect_err(why), why, "{bytes:?}");
        }
        let mut long = vec![b'x'; 200];
        long.extend_from_slice(b"\n\n");
        assert_eq!(
            decode(&long).expect_err("a long line"),
            format!("line 1 is not a header: {}...", "x".repeat(80))
        );
    }
}
