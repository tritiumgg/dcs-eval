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
//! A request name that is not UTF-16 is read lossily where the executor
//! reads bytes; no client mints one.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The envelope version the executor writes.
const PROTOCOL: &str = "2";

/// A request over this is refused unopened, as the executor refuses it.
const MAX_REQUEST_BYTES: u64 = 262_144;

/// The `states` header a `ping` carries, per host: the states that host
/// answers, in the order the executor declares them. Copied from what the
/// harness expects of the executor, so a client that reads the header
/// reads the real shape.
const HOOK_STATES: &str = "hook:carrier=local,returns=any,needs=always \
                           gui:carrier=dostring_in,returns=string,needs=menu \
                           scripting:carrier=dostring_in,returns=string,needs=menu \
                           mission:carrier=dostring_in,returns=string,needs=menu \
                           config:carrier=dostring_in,returns=string,needs=menu \
                           export:carrier=dostring_in,returns=string,needs=slot \
                           missionscripting:carrier=a_do_script,returns=string,needs=mission";
const EXPORT_STATES: &str = "export:carrier=local,returns=any,needs=always";

// ---- the envelope, this side's way -----------------------------------------

/// The bytes of an envelope in the stand-in's dialect: each header as
/// `name: value` ending in CRLF, a CRLF alone to end the block, then the
/// body byte for byte. A refusal covers what a value taken from a filename
/// can carry, a line break, a byte past ASCII or a leading blank, in the
/// executor's words and its order, and nothing more: every other header is
/// this module's own composition.
pub fn encode(headers: &[(&str, &str)], body: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(body.len() + 256);
    for &(name, value) in headers {
        if value.contains(['\r', '\n']) {
            return Err(format!("{name}: the value carries a CR or LF"));
        }
        if !value.is_ascii() {
            return Err(format!("{name}: the value is not ASCII"));
        }
        if value.starts_with([' ', '\t', '\u{0B}', '\u{0C}']) {
            return Err(format!(
                "{name}: the value begins with whitespace, which a reader strips"
            ));
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

// ---- the session, the tick and the reply -----------------------------------

/// The parts of a reply the tick composes for one request: the status, the
/// op's own headers after the seven every reply carries, and the body.
struct Answer {
    status: &'static str,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
}

impl Answer {
    fn refusal(status: &'static str, why: String) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: why.into_bytes(),
        }
    }
}

/// One stand-in executor session: a stamped directory under a root, a
/// tick counter, and the three values every reply names. The public fields
/// are a test's to set between ticks, the way a live session's phase
/// changes under a client.
pub struct Standin {
    /// `hook` or `export`, as the executor names its host.
    pub host: String,
    /// `menu` at load on the hook host and `loaded` on export, as the
    /// executor starts; a test moves it.
    pub phase: String,
    /// `<seconds>-<pid>`, minted at `open`, and the session directory's
    /// name.
    pub stamp: String,
    /// Ticks so far. Incremented as a tick begins, so the first reply of a
    /// session says `tick: 1`.
    pub tick: u64,
    session: PathBuf,
    req: PathBuf,
    res: PathBuf,
    arm: PathBuf,
}

impl Standin {
    /// A session opened under `root` for `host`: `<root>/<stamp>/req` and
    /// `res` made, the arm path named beside them and not made, since the
    /// executor never makes it either. The stamp is the executor's shape,
    /// the wall clock in seconds and this process's id.
    pub fn open(root: &Path, host: &str) -> io::Result<Self> {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let stamp = format!("{secs}-{}", std::process::id());
        let session = root.join(&stamp);
        let req = session.join("req");
        let res = session.join("res");
        fs::create_dir_all(&req)?;
        fs::create_dir_all(&res)?;
        // The executor's own branch: `menu` on the hook host and `loaded`
        // on any other, so a host spelt some third way lands where it
        // would there.
        let phase = if host == "hook" { "menu" } else { "loaded" };
        Ok(Self {
            host: host.to_owned(),
            phase: phase.to_owned(),
            stamp,
            tick: 0,
            arm: session.join("arm"),
            session,
            req,
            res,
        })
    }

    /// The session directory, `<root>/<stamp>`.
    pub fn session(&self) -> &Path {
        &self.session
    }

    /// Where a client publishes a request.
    pub fn req(&self) -> &Path {
        &self.req
    }

    /// Where a reply appears.
    pub fn res(&self) -> &Path {
        &self.res
    }

    /// The arm file's path, which a client makes and this side leaves.
    pub fn arm(&self) -> &Path {
        &self.arm
    }

    /// One frame: the counter up, the request directory listed, every
    /// name ending in exactly `.req` taken in name order and answered.
    /// The ids answered, in that order; a request that vanished between
    /// the listing and the read is not among them, and neither is one
    /// whose reply could not be written, which the executor counts and
    /// this side drops.
    pub fn tick(&mut self) -> Vec<String> {
        self.tick += 1;
        let mut names: Vec<String> = match fs::read_dir(&self.req) {
            Ok(listing) => listing
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".req"))
                .collect(),
            Err(_) => Vec::new(),
        };
        names.sort();
        let mut answered = Vec::new();
        for name in names {
            let id = name[..name.len() - 4].to_owned();
            let Some(answer) = self.answer(&self.req.join(&name)) else {
                continue;
            };
            let headers: Vec<(&str, &str)> = answer
                .headers
                .iter()
                .map(|(n, v)| (*n, v.as_str()))
                .collect();
            if self
                .reply(&id, answer.status, &headers, &answer.body)
                .is_ok()
            {
                answered.push(id);
            }
        }
        answered
    }

    /// The request at `path` taken and answered, the executor's way and in
    /// its words: the size read first and one over the limit refused
    /// unopened; the bytes read and the file removed before anything is
    /// done with them; then the envelope, `for`, `op` and the body checked
    /// in that order, and the op dispatched. `None` when the file was gone
    /// before it could be read, which the executor answers nothing to.
    fn answer(&self, path: &Path) -> Option<Answer> {
        let size = fs::metadata(path).ok()?.len();
        if size > MAX_REQUEST_BYTES {
            let _ = fs::remove_file(path);
            return Some(Answer::refusal(
                "bad-request",
                format!(
                    "the request is {size} bytes, over the {MAX_REQUEST_BYTES}-byte limit, \
                     and was not read"
                ),
            ));
        }
        let bytes = fs::read(path).ok()?;
        if let Err(why) = fs::remove_file(path) {
            return Some(Answer {
                status: "error",
                headers: vec![("stage", "bridge".to_owned())],
                body: format!(
                    "{} was read and could not be removed: {why}",
                    path.display()
                )
                .into_bytes(),
            });
        }
        let (headers, body) = match decode(&bytes) {
            Ok(decoded) => decoded,
            Err(why) => return Some(Answer::refusal("bad-request", why)),
        };
        if header(&headers, "for").is_none_or(str::is_empty) {
            return Some(Answer::refusal(
                "bad-request",
                "no for: the request does not name the session stamp it is for".to_owned(),
            ));
        }
        let op = match header(&headers, "op") {
            Some(op) if !op.is_empty() => op,
            _ => return Some(Answer::refusal("bad-request", "no op".to_owned())),
        };
        if op == "eval" && body.is_empty() {
            return Some(Answer::refusal(
                "bad-request",
                format!("the body is empty, and {op} runs it"),
            ));
        }
        Some(match op {
            "ping" => self.ping(),
            "eval" => Answer::refusal(
                "unsupported",
                "eval is declared and not yet served by this executor".to_owned(),
            ),
            other => Answer::refusal("bad-request", format!("unknown op: {other}")),
        })
    }

    /// The `ping` op as the executor answers it: `ok`, the host's states,
    /// no callback seen and none recorded, and `pong` for a body whatever
    /// the request carried.
    fn ping(&self) -> Answer {
        let states = if self.host == "export" {
            EXPORT_STATES
        } else {
            HOOK_STATES
        };
        Answer {
            status: "ok",
            headers: vec![
                ("states", states.to_owned()),
                ("last_callback", String::new()),
                ("callbacks", String::new()),
            ],
            body: b"pong".to_vec(),
        }
    }

    /// A reply to `id`: the seven headers the executor puts first, in its
    /// order, then `headers`, then `body`, encoded this side's way and
    /// published as `<res>/<id>.res` through `<id>.res.tmp` beside it. The
    /// refusal is the encoder's, or `<path>: <what the OS said>` as the
    /// executor spells one from the disk.
    pub fn reply(
        &self,
        id: &str,
        status: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> Result<(), String> {
        let tick = self.tick.to_string();
        let mut all = vec![
            ("status", status),
            ("protocol", PROTOCOL),
            ("host", self.host.as_str()),
            ("stamp", self.stamp.as_str()),
            ("phase", self.phase.as_str()),
            ("id", id),
            ("tick", tick.as_str()),
        ];
        all.extend_from_slice(headers);
        let bytes = encode(&all, body)?;
        let path = self.res.join(format!("{id}.res"));
        let tmp = self.res.join(format!("{id}.res.tmp"));
        let written = File::create(&tmp).and_then(|mut file| file.write_all(&bytes));
        let landed = written.and_then(|()| fs::rename(&tmp, &path));
        landed.map_err(|why| {
            let _ = fs::remove_file(&tmp);
            format!("{}: {why}", path.display())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Envelope, frame, parse};
    use crate::publish::send;
    use crate::testing::{Sandbox, entries, slurp};

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
        // A filename may begin with a blank, and the executor's framer
        // refuses one before the reader loses it.
        for id in [" a", "\ta", "\u{0B}a", "\u{0C}a"] {
            assert_eq!(
                encode(&[("id", id)], b"").expect_err(id),
                "id: the value begins with whitespace, which a reader strips",
                "{id:?}"
            );
        }
        assert_eq!(
            encode(&[("id", " \u{e9}")], b"").expect_err("two faults"),
            "id: the value is not ASCII",
            "checked in the executor's order"
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

    // ---- the session --------------------------------------------------------

    /// The reply's headers, in order: what every reply carries, and what
    /// a `ping` adds after it. The suite's own copy, kept apart from the
    /// module's.
    const HEAD: [&str; 7] = ["status", "protocol", "host", "stamp", "phase", "id", "tick"];
    const PING: [&str; 10] = [
        "status",
        "protocol",
        "host",
        "stamp",
        "phase",
        "id",
        "tick",
        "states",
        "last_callback",
        "callbacks",
    ];

    fn opened(b: &Sandbox, host: &str) -> Standin {
        Standin::open(&b.path, host).expect("the session opens")
    }

    /// A request sent the client's way: `send`, with `for` the stand-in's
    /// own stamp unless the caller spells the headers.
    fn sent(s: &Standin, id: &str, headers: &[(&str, &str)], body: &[u8]) {
        send(s.req(), s.arm(), id, headers, body).expect("the request sends");
    }

    fn pinged(s: &Standin, id: &str) {
        sent(s, id, &[("op", "ping"), ("for", s.stamp.as_str())], b"");
    }

    /// The reply to `id`, read back with the client's parser.
    fn read(s: &Standin, id: &str) -> Envelope {
        parse(&slurp(&s.res().join(format!("{id}.res")))).expect("the client reads the reply")
    }

    /// The names of the reply's headers, in wire order, against `want`.
    fn fields(e: &Envelope, want: &[&str], what: &str) {
        let got: Vec<&str> = e.headers.iter().map(|(n, _)| n).collect();
        assert_eq!(
            got, want,
            "{what}: every header is present, in the wire's order"
        );
    }

    /// A reply that is a refusal: its status and its message, with only the
    /// seven headers.
    fn refused(s: &Standin, id: &str, status: &str, why: &str) {
        let e = read(s, id);
        fields(&e, &HEAD, id);
        assert_eq!(e.headers.get("status"), Some(status), "{id}");
        assert_eq!(e.headers.get("id"), Some(id));
        assert_eq!(e.body, why.as_bytes(), "{id}: the body is the message");
    }

    #[test]
    fn open_makes_req_and_res_under_the_stamp_and_no_arm_file() {
        let b = Sandbox::new();
        let s = opened(&b, "hook");
        assert_eq!(
            entries(&b.path),
            s.stamp,
            "one session directory, named by the stamp"
        );
        assert_eq!(s.session(), b.join(&s.stamp));
        assert_eq!(entries(s.session()), "req res", "and no arm file");
        assert_eq!(s.req(), s.session().join("req"));
        assert_eq!(s.res(), s.session().join("res"));
        assert_eq!(s.arm(), s.session().join("arm"));
        let (secs, pid) = s.stamp.split_once('-').expect("seconds, a dash, the pid");
        assert!(
            secs.len() >= 10 && secs.bytes().all(|b| b.is_ascii_digit()),
            "{secs}"
        );
        assert_eq!(pid, std::process::id().to_string());
        assert_eq!(
            (s.host.as_str(), s.phase.as_str(), s.tick),
            ("hook", "menu", 0)
        );
        assert_eq!(opened(&Sandbox::new(), "export").phase, "loaded");
        assert_eq!(
            opened(&Sandbox::new(), "other").phase,
            "loaded",
            "the executor's branch: menu on hook, loaded on anything else"
        );
    }

    // ---- the round trip -----------------------------------------------------

    #[test]
    fn a_ping_sent_by_the_client_is_answered_on_the_tick_and_read_back() {
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        pinged(&s, "0000000001-abcd");
        assert!(s.arm().is_file(), "the client armed the session");
        assert_eq!(s.tick(), ["0000000001-abcd"]);
        assert_eq!(entries(s.req()), "", "the request is taken");
        assert_eq!(
            entries(s.res()),
            "0000000001-abcd.res",
            "the reply is under its final name and no .tmp is left"
        );
        assert!(
            s.arm().is_file(),
            "the arm file is left where the client put it"
        );
        let e = read(&s, "0000000001-abcd");
        fields(&e, &PING, "ping");
        let h = &e.headers;
        assert_eq!(h.get("status"), Some("ok"));
        assert_eq!(h.get("protocol"), Some("2"));
        assert_eq!(h.get("host"), Some("hook"));
        assert_eq!(h.get("stamp"), Some(s.stamp.as_str()));
        assert_eq!(h.get("phase"), Some("menu"));
        assert_eq!(h.get("id"), Some("0000000001-abcd"));
        assert_eq!(h.get("tick"), Some("1"), "the first tick of the session");
        assert_eq!(h.get("states"), Some(HOOK_STATES));
        assert_eq!(h.get("last_callback"), Some(""));
        assert_eq!(h.get("callbacks"), Some(""));
        assert_eq!(e.body, b"pong");
        // The bytes on the disk are the stand-in's dialect, not the client's.
        let bytes = slurp(&s.res().join("0000000001-abcd.res"));
        assert!(
            bytes.starts_with(b"status: ok\r\nprotocol: 2\r\n"),
            "{bytes:?}"
        );
        assert!(bytes.ends_with(b"\r\n\r\npong"));
    }

    #[test]
    fn several_in_one_tick_answered_in_name_order_and_a_quiet_tick_still_counts() {
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        pinged(&s, "0000000003-abcd");
        pinged(&s, "0000000001-zzzz");
        pinged(&s, "0000000002-abcd");
        fs::write(s.req().join("0000000009-abcd.req.tmp"), b"half").expect("a .tmp");
        fs::write(s.req().join("notes.txt"), b"x").expect("a stray file");
        assert_eq!(
            s.tick(),
            ["0000000001-zzzz", "0000000002-abcd", "0000000003-abcd"],
            "name order, not the order sent"
        );
        assert_eq!(
            entries(s.req()),
            "0000000009-abcd.req.tmp notes.txt",
            "only an exact .req is taken"
        );
        assert_eq!(
            entries(s.res()),
            "0000000001-zzzz.res 0000000002-abcd.res 0000000003-abcd.res"
        );
        for id in ["0000000001-zzzz", "0000000002-abcd", "0000000003-abcd"] {
            assert_eq!(
                read(&s, id).headers.get("tick"),
                Some("1"),
                "{id}: one tick shared"
            );
        }
        assert_eq!(
            s.tick(),
            Vec::<String>::new(),
            "a quiet tick answers nothing"
        );
        pinged(&s, "0000000004-abcd");
        assert_eq!(s.tick(), ["0000000004-abcd"]);
        assert_eq!(
            read(&s, "0000000004-abcd").headers.get("tick"),
            Some("3"),
            "the quiet tick counted"
        );
    }

    #[test]
    fn a_phase_moved_between_ticks_is_the_next_replys() {
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        pinged(&s, "0000000001-abcd");
        s.tick();
        s.phase = "mission".to_owned();
        pinged(&s, "0000000002-abcd");
        s.tick();
        assert_eq!(
            read(&s, "0000000001-abcd").headers.get("phase"),
            Some("menu")
        );
        assert_eq!(
            read(&s, "0000000002-abcd").headers.get("phase"),
            Some("mission")
        );
    }

    #[test]
    fn the_export_host_answers_with_its_own_states() {
        let b = Sandbox::new();
        let mut s = opened(&b, "export");
        pinged(&s, "0000000001-abcd");
        assert_eq!(s.tick(), ["0000000001-abcd"]);
        let e = read(&s, "0000000001-abcd");
        fields(&e, &PING, "export ping");
        assert_eq!(e.headers.get("host"), Some("export"));
        assert_eq!(e.headers.get("phase"), Some("loaded"));
        assert_eq!(e.headers.get("states"), Some(EXPORT_STATES));
    }

    // ---- refusals, in the executor's words ----------------------------------

    #[test]
    fn eval_is_unsupported_an_unknown_op_and_a_miscased_one_are_bad_requests() {
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        let stamp = s.stamp.clone();
        sent(
            &s,
            "0000000001-abcd",
            &[("op", "eval"), ("for", &stamp)],
            b"return 1",
        );
        sent(
            &s,
            "0000000002-abcd",
            &[("op", "Ping"), ("for", &stamp)],
            b"",
        );
        sent(
            &s,
            "0000000003-abcd",
            &[("op", "nope"), ("for", &stamp)],
            b"",
        );
        assert_eq!(s.tick().len(), 3, "every one is answered");
        refused(
            &s,
            "0000000001-abcd",
            "unsupported",
            "eval is declared and not yet served by this executor",
        );
        refused(&s, "0000000002-abcd", "bad-request", "unknown op: Ping");
        refused(&s, "0000000003-abcd", "bad-request", "unknown op: nope");
        assert_eq!(entries(s.req()), "", "each request is taken");
    }

    #[test]
    fn a_request_the_executor_would_not_admit_is_a_bad_request_and_taken() {
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        let stamp = s.stamp.clone();
        sent(&s, "0000000001-abcd", &[("op", "ping")], b"");
        sent(&s, "0000000002-abcd", &[("for", &stamp)], b"");
        sent(
            &s,
            "0000000003-abcd",
            &[("op", "eval"), ("for", &stamp)],
            b"",
        );
        fs::write(
            s.req().join("0000000004-abcd.req"),
            b"op: ping\nnocolon\n\n",
        )
        .expect("a bad line");
        fs::write(s.req().join("0000000005-abcd.req"), b"op: ping\n").expect("no blank line");
        sent(&s, "0000000006-abcd", &[("op", "ping"), ("for", "")], b"");
        assert_eq!(s.tick().len(), 6);
        assert_eq!(
            entries(s.req()),
            "",
            "every request is taken, admitted or not"
        );
        refused(
            &s,
            "0000000001-abcd",
            "bad-request",
            "no for: the request does not name the session stamp it is for",
        );
        refused(&s, "0000000002-abcd", "bad-request", "no op");
        refused(
            &s,
            "0000000003-abcd",
            "bad-request",
            "the body is empty, and eval runs it",
        );
        refused(
            &s,
            "0000000004-abcd",
            "bad-request",
            "line 2 is not a header: nocolon",
        );
        refused(
            &s,
            "0000000005-abcd",
            "bad-request",
            "the headers never end: no blank line before the bytes ran out, after 1 header lines",
        );
        refused(
            &s,
            "0000000006-abcd",
            "bad-request",
            "no for: the request does not name the session stamp it is for",
        );
    }

    #[test]
    fn a_request_over_the_limit_is_refused_unread_and_removed() {
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        let stamp = s.stamp.clone();
        let over = vec![b'x'; 262_145 - "op: ping\nfor: \n\n".len() - stamp.len()];
        sent(
            &s,
            "0000000001-abcd",
            &[("op", "ping"), ("for", &stamp)],
            &over,
        );
        assert_eq!(
            fs::metadata(s.req().join("0000000001-abcd.req"))
                .expect("the request")
                .len(),
            262_145
        );
        let at = vec![b'x'; 262_144 - "op: ping\nfor: \n\n".len() - stamp.len()];
        sent(
            &s,
            "0000000002-abcd",
            &[("op", "ping"), ("for", &stamp)],
            &at,
        );
        assert_eq!(s.tick().len(), 2);
        assert_eq!(entries(s.req()), "", "both are removed");
        refused(
            &s,
            "0000000001-abcd",
            "bad-request",
            "the request is 262145 bytes, over the 262144-byte limit, and was not read",
        );
        assert_eq!(
            read(&s, "0000000002-abcd").body,
            b"pong",
            "one at the limit is read whole"
        );
    }

    #[test]
    fn a_foreign_for_is_answered_as_the_executor_answers_it() {
        // The executor requires `for` and does not yet compare it with its
        // stamp; the fence that answers `stale-session` is a later task,
        // on both sides. Until then a foreign stamp is answered, and this
        // pins that the stand-in does what the executor does.
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        sent(
            &s,
            "0000000001-abcd",
            &[("op", "ping"), ("for", "1-1")],
            b"",
        );
        assert_eq!(s.tick(), ["0000000001-abcd"]);
        let e = read(&s, "0000000001-abcd");
        assert_eq!(e.headers.get("status"), Some("ok"));
        assert_eq!(
            e.headers.get("stamp"),
            Some(s.stamp.as_str()),
            "the reply names the real stamp"
        );
    }

    #[test]
    fn a_body_reaches_the_stand_in_byte_for_byte_and_a_ping_answers_pong_regardless() {
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        let body = b"\xcf\xf0\xe8\xe2\xe5\xf2\r\n\0\xff status: fake\n\n";
        sent(
            &s,
            "0000000001-abcd",
            &[("op", "ping"), ("for", &s.stamp.clone())],
            body,
        );
        let on_disk = slurp(&s.req().join("0000000001-abcd.req"));
        assert_eq!(
            decoded(&on_disk).1,
            body,
            "the decoder hands back the client's bytes"
        );
        assert_eq!(s.tick(), ["0000000001-abcd"]);
        assert_eq!(read(&s, "0000000001-abcd").body, b"pong");
    }

    #[test]
    fn reply_refuses_an_id_the_encoder_refuses_and_names_the_final_on_a_disk_refusal() {
        let b = Sandbox::new();
        let s = opened(&b, "hook");
        assert_eq!(
            s.reply("caf\u{e9}", "ok", &[], b"")
                .expect_err("a byte past ASCII in the id"),
            "id: the value is not ASCII"
        );
        assert_eq!(entries(s.res()), "", "nothing is written");
        let final_ = s.res().join("0000000001-abcd.res");
        fs::create_dir(&final_).expect("a directory at the final name");
        let why = s
            .reply("0000000001-abcd", "ok", &[], b"")
            .expect_err("the rename refuses");
        assert!(why.starts_with(&format!("{}: ", final_.display())), "{why}");
        assert_eq!(
            entries(s.res()),
            "0000000001-abcd.res",
            "and no .tmp survives"
        );
    }
}
