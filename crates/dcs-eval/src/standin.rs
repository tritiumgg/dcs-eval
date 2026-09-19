//! The stand-in: a test double that plays the executor's side of the wire
//! in this process, so a client control has something to drive that is not
//! the client's own serialiser turned around.
//!
//! It stands in for the executor's `tick` and `reply` in
//! `DcsEvalExecutor.lua`: an output directory with a session under
//! `rpc\<stamp>` carrying `req` and `res`, as the executor lays one out
//! when it falls back beside the output, a tick that lists requests and
//! answers them in name order, a
//! reply with the eight headers the executor puts first, the `ping` op,
//! and `eval` as far as its checks go: no chunk runs here, and one that
//! passes them is answered as one that returned nil — unless a test has
//! *told* it what to answer, which is still not running one: a script
//! matched on the request body substitutes a status, a `result_type` and
//! a body for the nil a chunk that ran nothing would give. What it answers, it
//! answers as the executor does, message for
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
//! arm file, which the executor does not have yet either.
//! Nor is the tick budget: the stand-in answers every request it lists in
//! one tick, and measures nothing, so every reply's `cpu_ms` is `0.000`.
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

/// The most a `chunkname` may be, as the executor bounds it.
const MAX_CHUNKNAME_BYTES: usize = 200;

/// The instruction count a chunk runs under when the request names none,
/// and the most a request may name, as the executor publishes them.
const INSTRUCTION_BUDGET: u64 = 1_000_000;
const INSTRUCTION_CEILING: u64 = 50_000_000;

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

/// The start of a value for a message, as the executor excerpts one: the
/// first 80 bytes and three dots where it is longer. Bytes, not characters,
/// because the executor counts bytes and a header value is ASCII by the
/// time either side reads it.
fn excerpt(value: &str) -> String {
    if value.len() > 80 {
        format!("{}...", &value[..80])
    } else {
        value.to_owned()
    }
}

/// Whether `needle` appears in `haystack`. A script is matched on the
/// request's body, which is Lua and need not be text this side can decode.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
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
/// op's own headers after the eight every reply carries, and the body.
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

/// One request as it was read off the disk, raw.
///
/// The bytes are what the file held, recorded before anything tries to
/// make sense of them, so a request that would not decode is still in the
/// ledger. That is the point of it: a check that sweeps what left the
/// client for a name it must never send has to see the headers too, and a
/// request the far end could not read is exactly the one a decoded record
/// would lose.
#[derive(Debug, Clone)]
pub struct Seen {
    pub bytes: Vec<u8>,
}

/// One answer a test has told this side to give: the first entry whose
/// `when` appears in the request body decides an `eval`.
struct Script {
    when: String,
    status: &'static str,
    result_type: String,
    body: Vec<u8>,
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
    /// Whether the session is armed, `false` at `open` as the executor
    /// loads dormant; a test moves it between beats.
    pub armed: bool,
    /// The process id the handshake names. A fixed 4242 at `open`, which
    /// on this host is nobody; a test whose client probes a real process,
    /// running or gone, sets it before the handshake is written.
    pub pid: u32,
    /// The wall-clock time of the last arm or disarm, display only and so
    /// a fixed spelling here: this side formats no clock and reads none.
    pub since: String,
    /// `<name>@<tick>` of the last callback other than the frame to fire,
    /// empty until one does.
    pub last_callback: String,
    /// The callbacks this session has seen, empty until one fires.
    pub callbacks: Vec<String>,
    output: PathBuf,
    session: PathBuf,
    req: PathBuf,
    res: PathBuf,
    arm: PathBuf,
    seen: Vec<Seen>,
    scripts: Vec<Script>,
}

impl Standin {
    /// A session opened for `host` with `root` as the output directory:
    /// `<root>/rpc/<stamp>/req` and `res` made, the arm path named beside
    /// them and not made, since the executor never makes it either. The
    /// layout is the executor's own fallback, where the session sits under
    /// `rpc` beside the output rather than in the temp directory, so a
    /// client reading this side's handshake reads paths shaped like the
    /// ones a real session names. The stamp is the executor's shape, the
    /// wall clock in seconds and this process's id.
    pub fn open(root: &Path, host: &str) -> io::Result<Self> {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let stamp = format!("{secs}-{}", std::process::id());
        let session = root.join("rpc").join(&stamp);
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
            armed: false,
            pid: 4242,
            since: "2026-09-19 11:03:07".to_owned(),
            last_callback: String::new(),
            callbacks: Vec::new(),
            arm: session.join("arm"),
            output: root.to_owned(),
            session,
            req,
            res,
            seen: Vec::new(),
            scripts: Vec::new(),
        })
    }

    /// The output directory, where the handshake and the heartbeat land.
    pub fn output(&self) -> &Path {
        &self.output
    }

    /// The session directory, `<output>/rpc/<stamp>`.
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

    /// Every request this side has read off the disk, in the order it
    /// read them, raw.
    pub fn seen(&self) -> &[Seen] {
        &self.seen
    }

    /// Answer the next `eval` whose body holds `when` with `status`, a
    /// `result_type` and `body`, rather than as one that returned nil.
    ///
    /// It is matched after the shape refusals an executor makes before it
    /// looks at a state's carrier, and *before* the carrier is looked up.
    /// The order is the whole use of it: placed after the carrier, a
    /// script could never give a `gui` eval anything but `unsupported`,
    /// and a control that needs to see a client handle some other answer
    /// from a state this host declares but does not serve would have no
    /// way to stage one.
    pub fn script(&mut self, when: &str, status: &'static str, result_type: &str, body: &[u8]) {
        self.scripts.push(Script {
            when: when.to_owned(),
            status,
            result_type: result_type.to_owned(),
            body: body.to_vec(),
        });
    }

    /// One frame: the counter up, the request directory listed, every
    /// name ending in exactly `.req` taken in name order and answered.
    /// The ids answered, in that order; a request that vanished between
    /// the listing and the read is not among them, and neither is one
    /// whose reply could not be written, which the executor counts and
    /// this side drops.
    pub fn tick(&mut self) -> Vec<String> {
        self.tick_with(|_| {})
    }

    /// One frame whose listing `pick` may reorder or shorten before any of
    /// it is answered. It is handed the ids the executor would answer —
    /// the exact `.req` names in sorted order, each without its suffix —
    /// and whatever it leaves in the vector is answered in the order it
    /// left them.
    ///
    /// A live executor never does this: it answers in sorted order until
    /// its tick budget is spent. The hook is here because a client that
    /// claims to yield replies in id order can only be held to it by a
    /// session that answers in some other order, and a fixture that waits
    /// for replies to happen to arrive out of order proves nothing. What
    /// `pick` drops is left on the disk for a later frame, which is also
    /// what a spent budget does, so a test can hold one request back
    /// without inventing a state the executor cannot be in.
    pub fn tick_with(&mut self, pick: impl FnOnce(&mut Vec<String>)) -> Vec<String> {
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
        let mut ids: Vec<String> = names
            .iter()
            .map(|name| name[..name.len() - 4].to_owned())
            .collect();
        pick(&mut ids);
        let mut answered = Vec::new();
        for id in ids {
            let name = format!("{id}.req");
            // The path is hoisted because answering records the request
            // in the ledger, so it borrows this side mutably and cannot
            // also be reading `self.req` for its argument.
            let path = self.req.join(&name);
            let Some(answer) = self.answer(&path) else {
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
    /// done with them; then the envelope, `for` present, `for` this
    /// session's, `op` and the body checked in that order, and the op
    /// dispatched. `None` when the file was gone before it could be read,
    /// which the executor answers nothing to.
    fn answer(&mut self, path: &Path) -> Option<Answer> {
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
        // Recorded here, before the decode: what the ledger is for is
        // saying what really reached the disk, and a request that would
        // not decode is one nothing else would ever show.
        self.seen.push(Seen {
            bytes: bytes.clone(),
        });
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
        match header(&headers, "for") {
            None | Some("") => {
                return Some(Answer::refusal(
                    "bad-request",
                    "no for: the request does not name the session stamp it is for".to_owned(),
                ));
            }
            // The fence, in the executor's place and its words: a request
            // for another session is answered on its stamp alone, before
            // an op is looked for, and the stamp it named comes back so a
            // client can see which session it addressed.
            Some(named) if named != self.stamp => {
                return Some(Answer {
                    status: "stale-session",
                    headers: vec![("for", named.to_owned())],
                    body: format!(
                        "for: {} is not this session's stamp, {}: the request was \
                         written for another session and was not run",
                        excerpt(named),
                        self.stamp
                    )
                    .into_bytes(),
                });
            }
            Some(_) => {}
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
            "eval" => self.eval(&headers, &body),
            other => Answer::refusal("bad-request", format!("unknown op: {other}")),
        })
    }

    /// The `eval` op as far as the executor's own checks go, word for
    /// word: a request must name its state, spelt as a name; a chunkname
    /// over 200 bytes is refused and one absent is `=dcs-eval`; a
    /// `max_instructions` that is not digits is refused, one absent is the
    /// default, one over the ceiling is held to it, and `0` is unbounded,
    /// which the reply's `budget` says as `none`; a state
    /// this host does not declare is `unsupported`, and so is one whose
    /// carrier is not built. Past the checks the stand-in runs nothing,
    /// because there is no Lua here to run it, and answers every chunk as
    /// the executor answers one that returned nil: `ok`, `result_type:
    /// nil`, the chunkname it was compiled under, the budget it ran
    /// under, and an empty body. A
    /// control that needs a value back is a control on the shipped Lua.
    /// An install with eval disabled is not modelled: every stand-in has
    /// it on, so the empty-body refusal above is unconditional here.
    fn eval(&self, headers: &[(String, String)], body: &[u8]) -> Answer {
        let state = match header(headers, "state") {
            Some(state) if !state.is_empty() => state,
            _ => {
                return Answer::refusal(
                    "bad-request",
                    "no state: the request does not name the state to run in".to_owned(),
                );
            }
        };
        let mut chars = state.chars();
        let named = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !named {
            return Answer::refusal(
                "bad-request",
                format!("state: {state} is not [A-Za-z][A-Za-z0-9_]*"),
            );
        }
        let chunkname = match header(headers, "chunkname") {
            Some(name) if !name.is_empty() => name,
            _ => "=dcs-eval",
        };
        if chunkname.len() > MAX_CHUNKNAME_BYTES {
            return Answer::refusal(
                "bad-request",
                format!(
                    "chunkname: {} bytes, over the {MAX_CHUNKNAME_BYTES}-byte limit",
                    chunkname.len()
                ),
            );
        }
        let count = match header(headers, "max_instructions") {
            None | Some("") => INSTRUCTION_BUDGET,
            Some(count) if count.bytes().all(|b| b.is_ascii_digit()) => count
                .parse::<u64>()
                .map_or(INSTRUCTION_CEILING, |n| n.min(INSTRUCTION_CEILING)),
            Some(count) => {
                let dots = if count.len() > 80 { "..." } else { "" };
                return Answer::refusal(
                    "bad-request",
                    format!(
                        "max_instructions: {}{dots} is not a non-negative integer",
                        &count[..count.len().min(80)]
                    ),
                );
            }
        };
        let budget = if count == 0 {
            "none".to_owned()
        } else {
            format!("instructions={count}")
        };
        // A script sits here, after every refusal an executor decides on
        // the request's own shape and before the state's carrier is
        // looked up, so a test can stage an answer for a state this host
        // declares and does not serve.
        if let Some(scripted) = self
            .scripts
            .iter()
            .find(|s| contains(body, s.when.as_bytes()))
        {
            let mut headers = Vec::new();
            if !scripted.result_type.is_empty() {
                headers.push(("result_type", scripted.result_type.clone()));
                headers.push(("chunkname", chunkname.to_owned()));
                headers.push(("budget", budget));
            }
            return Answer {
                status: scripted.status,
                headers,
                body: scripted.body.clone(),
            };
        }
        let states = if self.host == "export" {
            EXPORT_STATES
        } else {
            HOOK_STATES
        };
        let looked_up = if state == "server" {
            "scripting"
        } else {
            state
        };
        let carrier = states
            .split(' ')
            .find_map(|entry| entry.strip_prefix(&format!("{looked_up}:carrier=")))
            .map(|rest| rest.split(',').next().unwrap_or(rest));
        match carrier {
            None => Answer::refusal(
                "unsupported",
                format!("{state} is not a state this host serves"),
            ),
            Some("local") => Answer {
                status: "ok",
                headers: vec![
                    ("result_type", "nil".to_owned()),
                    ("chunkname", chunkname.to_owned()),
                    ("budget", budget),
                ],
                body: Vec::new(),
            },
            Some(_) => Answer::refusal(
                "unsupported",
                format!("{state} is declared and not yet served by this executor"),
            ),
        }
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

    /// The handshake, `<output>/executor.txt`: the twenty-seven headers the
    /// executor writes at load, in its order, naming this session's own
    /// paths. Every one is written on every load there — `ALLOW_EVAL`
    /// moves the values of `eval` and `ops` and never their presence — so
    /// every one is written here too.
    ///
    /// The four fields the executor writes as the literal `ABSENT` where
    /// its read did not answer are given present values instead, so what a
    /// client reads off this side exercises the resolving path; a fixture
    /// wanting `ABSENT` builds it by hand.
    pub fn handshake(&self) -> Result<(), String> {
        let path = |p: &Path| p.display().to_string();
        let transport = path(&self.session);
        let req = path(&self.req);
        let res = path(&self.res);
        let arm = path(&self.arm);
        let output = path(&self.output);
        let tempdir = path(&self.output.join("tmp"));
        let guard = path(&self.output.join("install_guard.txt"));
        let states = self.states();
        let pid = self.pid.to_string();
        let headers = [
            ("executor", "dcs-eval"),
            ("protocol", PROTOCOL),
            ("host", self.host.as_str()),
            ("stamp", self.stamp.as_str()),
            ("pid", pid.as_str()),
            ("started", "2026-09-19 11:03:07"),
            ("transport", transport.as_str()),
            ("req", req.as_str()),
            ("res", res.as_str()),
            ("arm", arm.as_str()),
            ("output", output.as_str()),
            ("eval", "allowed"),
            ("ops", "ping,eval"),
            ("states", states),
            ("namespace", "DcsEval"),
            ("source", "DcsEvalExecutor.lua"),
            ("lfs_tempdir", tempdir.as_str()),
            ("transport_source", "fallback: beside the output"),
            ("install_guard", guard.as_str()),
            ("tick_budget_ms", "8"),
            ("instruction_budget", "1000000"),
            ("instruction_ceiling", "50000000"),
            ("probe_every", "8"),
            ("quiet_s", "3"),
            ("app_version", "2.9.10.1234"),
            ("max_request_bytes", "262144"),
            ("max_result_bytes", "65536"),
        ];
        let bytes = encode(&headers, b"")?;
        self.land(&self.output.join("executor.txt"), &bytes)
    }

    /// The heartbeat, `<output>/heartbeat.txt`: the ten headers this
    /// session keeps, in the executor's order, with the file's modification
    /// time set to `at` after it lands, since the write would otherwise
    /// stamp it now. The time is the caller's every time, because the age
    /// a client derives comes from this stamp and from nothing in the
    /// file's own text.
    pub fn beat(&self, at: SystemTime) -> Result<(), String> {
        let transport = self.session.display().to_string();
        let ticks = self.tick.to_string();
        let callbacks = self.callbacks.join(",");
        let headers = [
            ("protocol", PROTOCOL),
            ("host", self.host.as_str()),
            ("stamp", self.stamp.as_str()),
            ("transport", transport.as_str()),
            ("phase", self.phase.as_str()),
            ("armed", if self.armed { "yes" } else { "no" }),
            ("since", self.since.as_str()),
            ("ticks", ticks.as_str()),
            ("last_callback", self.last_callback.as_str()),
            ("callbacks", callbacks.as_str()),
        ];
        let bytes = encode(&headers, b"")?;
        let path = self.output.join("heartbeat.txt");
        self.land(&path, &bytes)?;
        File::options()
            .write(true)
            .open(&path)
            .and_then(|file| file.set_modified(at))
            .map_err(|why| format!("{}: {why}", path.display()))
    }

    /// The `states` header for this host, as the executor declares them.
    fn states(&self) -> &'static str {
        if self.host == "export" {
            EXPORT_STATES
        } else {
            HOOK_STATES
        }
    }

    /// One file published the way this side publishes every file: written
    /// beside itself as `.tmp` and renamed over, so a reader never meets a
    /// half-written one.
    fn land(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        let mut name = path.file_name().unwrap_or_default().to_owned();
        name.push(".tmp");
        let tmp = path.with_file_name(name);
        let written = File::create(&tmp).and_then(|mut file| file.write_all(bytes));
        let landed = written.and_then(|()| fs::rename(&tmp, path));
        landed.map_err(|why| {
            let _ = fs::remove_file(&tmp);
            format!("{}: {why}", path.display())
        })
    }

    /// A reply to `id`: the eight headers the executor puts first, in its
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
            ("cpu_ms", "0.000"),
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

    use std::time::Duration;

    fn encoded(headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
        encode(headers, body).expect("the envelope encodes")
    }

    fn decoded(bytes: &[u8]) -> Decoded {
        decode(bytes).expect("the envelope decodes")
    }

    /// The executor's reply shape with a caller's header after it.
    const REPLY: [(&str, &str); 9] = [
        ("status", "ok"),
        ("protocol", "2"),
        ("host", "hook"),
        ("stamp", "0000000001-abcd"),
        ("phase", "menu"),
        ("id", "0000000001-abcd"),
        ("tick", "0"),
        ("cpu_ms", "0.000"),
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
    const HEAD: [&str; 8] = [
        "status", "protocol", "host", "stamp", "phase", "id", "tick", "cpu_ms",
    ];
    const FENCED: [&str; 9] = [
        "status", "protocol", "host", "stamp", "phase", "id", "tick", "cpu_ms", "for",
    ];
    const PING: [&str; 11] = [
        "status",
        "protocol",
        "host",
        "stamp",
        "phase",
        "id",
        "tick",
        "cpu_ms",
        "states",
        "last_callback",
        "callbacks",
    ];
    const EVAL: [&str; 11] = [
        "status",
        "protocol",
        "host",
        "stamp",
        "phase",
        "id",
        "tick",
        "cpu_ms",
        "result_type",
        "chunkname",
        "budget",
    ];

    fn opened(b: &Sandbox, host: &str) -> Standin {
        Standin::open(&b.path, host).expect("the session opens")
    }

    /// A request sent the client's way: `send`, with `for` the stand-in's
    /// own stamp unless the caller spells the headers.
    fn sent(s: &Standin, id: &str, headers: &[(&str, &str)], body: &[u8]) {
        let _ = send(s.req(), s.arm(), id, headers, body).expect("the request sends");
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
    /// eight headers.
    fn refused(s: &Standin, id: &str, status: &str, why: &str) {
        let e = read(s, id);
        fields(&e, &HEAD, id);
        assert_eq!(e.headers.get("status"), Some(status), "{id}");
        assert_eq!(e.headers.get("id"), Some(id));
        assert_eq!(e.body, why.as_bytes(), "{id}: the body is the message");
    }

    #[test]
    fn the_ledger_holds_a_request_that_would_not_decode() {
        // The bytes are recorded before the decode, so the one request
        // nothing else can show is in the ledger too. A sweep for a name
        // that must never be sent depends on that: a forbidden name in a
        // header of a request the far end could not read would otherwise
        // be invisible.
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        fs::write(
            s.req().join("0000000001-abcd.req"),
            b"not an envelope at all",
        )
        .expect("the request is written");
        s.tick();
        assert_eq!(s.seen().len(), 1, "the ledger holds the request");
        assert_eq!(s.seen()[0].bytes, b"not an envelope at all");
        refused(
            &s,
            "0000000001-abcd",
            "bad-request",
            "the headers never end: no blank line before the bytes ran out, \
             after 0 header lines",
        );
    }

    #[test]
    fn a_scripted_eval_comes_back_with_the_status_and_body_it_was_given() {
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        let stamp = s.stamp.clone();
        s.script("getPause", "ok", "string", b"boolean\ttrue");
        sent(
            &s,
            "0000000001-abcd",
            &[("op", "eval"), ("for", &stamp), ("state", "hook")],
            b"return DCS.getPause()",
        );
        s.tick();
        let e = read(&s, "0000000001-abcd");
        assert_eq!(e.headers.get("status"), Some("ok"));
        assert_eq!(e.headers.get("result_type"), Some("string"));
        assert_eq!(e.body, b"boolean\ttrue");
    }

    #[test]
    fn a_script_can_give_a_gui_eval_a_status_the_carrier_check_would_not() {
        // The seam this is here to hold open: `gui` is declared and not
        // served, so without a script no client control could ever see a
        // `gui` eval answered any other way.
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        let stamp = s.stamp.clone();
        s.script("return 'ok'", "refused", "", b"eval is off in this install");
        sent(
            &s,
            "0000000001-abcd",
            &[("op", "eval"), ("for", &stamp), ("state", "gui")],
            b"return 'ok'",
        );
        s.tick();
        refused(
            &s,
            "0000000001-abcd",
            "refused",
            "eval is off in this install",
        );
    }

    #[test]
    fn an_unscripted_gui_eval_is_still_unsupported() {
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        let stamp = s.stamp.clone();
        s.script("some other body", "refused", "", b"never matched");
        sent(
            &s,
            "0000000001-abcd",
            &[("op", "eval"), ("for", &stamp), ("state", "gui")],
            b"return 'ok'",
        );
        s.tick();
        refused(
            &s,
            "0000000001-abcd",
            "unsupported",
            "gui is declared and not yet served by this executor",
        );
    }

    #[test]
    fn an_unscripted_local_eval_still_answers_as_one_that_returned_nil() {
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        let stamp = s.stamp.clone();
        sent(
            &s,
            "0000000001-abcd",
            &[("op", "eval"), ("for", &stamp), ("state", "hook")],
            b"return 1",
        );
        s.tick();
        let e = read(&s, "0000000001-abcd");
        assert_eq!(e.headers.get("status"), Some("ok"));
        assert_eq!(e.headers.get("result_type"), Some("nil"));
        assert_eq!(e.body, b"");
    }

    #[test]
    fn open_makes_req_and_res_under_the_stamp_and_no_arm_file() {
        let b = Sandbox::new();
        let s = opened(&b, "hook");
        assert_eq!(s.output(), b.path, "the root is the output directory");
        assert_eq!(entries(&b.path), "rpc", "the transport root beside it");
        assert_eq!(
            entries(&b.join("rpc")),
            s.stamp,
            "one session directory, named by the stamp"
        );
        assert_eq!(s.session(), b.join("rpc").join(&s.stamp));
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

    /// The twenty-seven handshake headers, in the order the executor's
    /// `handshake` writes them. `app_version` sits between `quiet_s` and
    /// the two byte limits, which is where the writer puts it and not
    /// where the specification's table would suggest.
    const HANDSHAKE: [&str; 27] = [
        "executor",
        "protocol",
        "host",
        "stamp",
        "pid",
        "started",
        "transport",
        "req",
        "res",
        "arm",
        "output",
        "eval",
        "ops",
        "states",
        "namespace",
        "source",
        "lfs_tempdir",
        "transport_source",
        "install_guard",
        "tick_budget_ms",
        "instruction_budget",
        "instruction_ceiling",
        "probe_every",
        "quiet_s",
        "app_version",
        "max_request_bytes",
        "max_result_bytes",
    ];

    #[test]
    fn the_handshake_lands_whole_under_the_output() {
        let b = Sandbox::new();
        let s = opened(&b, "hook");
        s.handshake().expect("the handshake publishes");
        assert_eq!(
            entries(s.output()),
            "executor.txt rpc",
            "under its final name and no .tmp left behind"
        );
        let (headers, body) = decoded(&slurp(&s.output().join("executor.txt")));
        let names: Vec<&str> = headers.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, HANDSHAKE, "every header, in the writer's order");
        assert!(body.is_empty(), "the handshake carries no body");
        let field = |name: &str| header(&headers, name).expect(name).to_owned();
        assert_eq!(field("executor"), "dcs-eval");
        assert_eq!(field("protocol"), "2");
        assert_eq!(field("host"), "hook");
        assert_eq!(field("stamp"), s.stamp);
        assert_eq!(field("transport"), s.session().display().to_string());
        assert_eq!(field("req"), s.req().display().to_string());
        assert_eq!(field("res"), s.res().display().to_string());
        assert_eq!(field("arm"), s.arm().display().to_string());
        assert_eq!(field("output"), s.output().display().to_string());
        assert_eq!(field("transport_source"), "fallback: beside the output");
        assert_eq!(field("states"), HOOK_STATES);
        assert_eq!(
            opened(&Sandbox::new(), "export").states(),
            EXPORT_STATES,
            "and the other host's states"
        );
    }

    #[test]
    fn the_handshake_names_the_pid_the_session_was_given() {
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        assert_eq!(s.pid, 4242, "the default, which is nobody on this host");
        s.handshake().expect("the default publishes");
        let read = |s: &Standin| {
            let (headers, _) = decoded(&slurp(&s.output().join("executor.txt")));
            header(&headers, "pid").expect("pid").to_owned()
        };
        assert_eq!(read(&s), "4242");
        s.pid = std::process::id();
        s.handshake().expect("the second handshake publishes");
        assert_eq!(
            read(&s),
            std::process::id().to_string(),
            "a session may name a process a client can really probe"
        );
    }

    /// The ten heartbeat headers, in the order the executor writes them.
    const BEAT: [&str; 10] = [
        "protocol",
        "host",
        "stamp",
        "transport",
        "phase",
        "armed",
        "since",
        "ticks",
        "last_callback",
        "callbacks",
    ];

    /// The beat's headers, read back off the disk.
    fn beaten(s: &Standin, at: SystemTime) -> Vec<(String, String)> {
        s.beat(at).expect("the beat publishes");
        let (headers, body) = decoded(&slurp(&s.output().join("heartbeat.txt")));
        assert!(body.is_empty(), "the heartbeat carries no body");
        headers
    }

    #[test]
    fn the_beat_lands_under_the_output_with_the_time_it_was_given() {
        let b = Sandbox::new();
        let s = opened(&b, "hook");
        let at = SystemTime::now() - Duration::from_secs(90);
        let headers = beaten(&s, at);
        assert_eq!(
            entries(s.output()),
            "heartbeat.txt rpc",
            "under its final name and no .tmp left behind"
        );
        let names: Vec<&str> = headers.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, BEAT, "every header, in the writer's order");
        let field = |name: &str| header(&headers, name).expect(name).to_owned();
        assert_eq!(field("protocol"), "2");
        assert_eq!(field("host"), "hook");
        assert_eq!(field("stamp"), s.stamp);
        assert_eq!(field("transport"), s.session().display().to_string());
        assert_eq!(field("phase"), "menu");
        assert_eq!(field("armed"), "no", "a session loads dormant");
        assert_eq!(field("since"), s.since);
        assert_eq!(field("ticks"), "0");
        assert_eq!(field("last_callback"), "", "none has fired");
        assert_eq!(field("callbacks"), "");
        // Within a second, not equal: a volume whose timestamps are coarse
        // will not give back the instant it was handed.
        let modified = fs::metadata(s.output().join("heartbeat.txt"))
            .and_then(|m| m.modified())
            .expect("the beat has a modification time");
        let off = modified
            .duration_since(at)
            .or_else(|_| at.duration_since(modified))
            .expect("one is after the other");
        assert!(off < Duration::from_secs(1), "{off:?} off the time given");
    }

    #[test]
    fn armed_and_the_phase_moved_between_beats_are_the_next_beats() {
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        let now = SystemTime::now();
        assert_eq!(header(&beaten(&s, now), "armed"), Some("no"));
        s.armed = true;
        s.phase = "mission".to_owned();
        s.since = "2026-09-19 11:05:00".to_owned();
        s.tick = 7;
        s.last_callback = "onSimulationStart@3".to_owned();
        s.callbacks = vec![
            "onSimulationStart".to_owned(),
            "onMissionLoadEnd".to_owned(),
        ];
        let headers = beaten(&s, now);
        let field = |name: &str| header(&headers, name).expect(name).to_owned();
        assert_eq!(field("armed"), "yes");
        assert_eq!(field("phase"), "mission");
        assert_eq!(field("since"), "2026-09-19 11:05:00");
        assert_eq!(field("ticks"), "7");
        assert_eq!(field("last_callback"), "onSimulationStart@3");
        assert_eq!(
            field("callbacks"),
            "onSimulationStart,onMissionLoadEnd",
            "joined with commas, as the executor concatenates them"
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
    fn a_tick_answers_in_the_order_its_pick_left() {
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        for id in ["0000000001-abcd", "0000000002-abcd", "0000000003-abcd"] {
            pinged(&s, id);
        }
        let answered = s.tick_with(|ids| {
            assert_eq!(
                ids,
                &["0000000001-abcd", "0000000002-abcd", "0000000003-abcd"],
                "the pick is handed sorted ids with the suffix off"
            );
            ids.reverse();
        });
        assert_eq!(
            answered,
            ["0000000003-abcd", "0000000002-abcd", "0000000001-abcd"],
            "answered backwards, which no live frame does"
        );
        assert_eq!(entries(s.req()), "", "and every request is still taken");
    }

    #[test]
    fn what_a_pick_drops_is_left_for_the_next_tick() {
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        pinged(&s, "0000000001-abcd");
        pinged(&s, "0000000002-abcd");
        assert_eq!(
            s.tick_with(|ids| ids.truncate(1)),
            ["0000000001-abcd"],
            "only what the pick left"
        );
        assert_eq!(
            entries(s.req()),
            "0000000002-abcd.req",
            "the other is untouched on the disk"
        );
        assert_eq!(entries(s.res()), "0000000001-abcd.res");
        assert_eq!(s.tick(), ["0000000002-abcd"], "and the next tick takes it");
        assert_eq!(
            read(&s, "0000000002-abcd").headers.get("tick"),
            Some("2"),
            "on the tick that answered it"
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
    fn an_unknown_op_and_a_miscased_one_are_bad_requests() {
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        let stamp = s.stamp.clone();
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
        assert_eq!(s.tick().len(), 2, "every one is answered");
        refused(&s, "0000000002-abcd", "bad-request", "unknown op: Ping");
        refused(&s, "0000000003-abcd", "bad-request", "unknown op: nope");
        assert_eq!(entries(s.req()), "", "each request is taken");
    }

    #[test]
    fn an_eval_is_answered_as_a_chunk_that_returned_nil_and_refused_as_the_executor_refuses() {
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        let stamp = s.stamp.clone();
        let long = "@".to_owned() + &"x".repeat(200);
        let digits = "-".to_owned() + &"1".repeat(100);
        let cases: [(&str, &[(&str, &str)]); 13] = [
            ("0000000001-abcd", &[("state", "hook")]),
            (
                "0000000002-abcd",
                &[("state", "hook"), ("chunkname", "@x.lua")],
            ),
            ("0000000003-abcd", &[]),
            ("0000000004-abcd", &[("state", "9x")]),
            ("0000000005-abcd", &[("state", "nope")]),
            ("0000000006-abcd", &[("state", "gui")]),
            ("0000000007-abcd", &[("state", "server")]),
            (
                "0000000008-abcd",
                &[("state", "hook"), ("chunkname", &long)],
            ),
            (
                "0000000009-abcd",
                &[("state", "hook"), ("max_instructions", "0")],
            ),
            (
                "0000000010-abcd",
                &[
                    ("state", "hook"),
                    ("max_instructions", "99999999999999999999999"),
                ],
            ),
            (
                "0000000011-abcd",
                &[("state", "hook"), ("max_instructions", "")],
            ),
            (
                "0000000012-abcd",
                &[("state", "hook"), ("max_instructions", "1.5")],
            ),
            (
                "0000000013-abcd",
                &[("state", "hook"), ("max_instructions", &digits)],
            ),
        ];
        for (id, extra) in cases {
            let mut headers = vec![("op", "eval"), ("for", stamp.as_str())];
            headers.extend_from_slice(extra);
            sent(&s, id, &headers, b"return 1");
        }
        assert_eq!(s.tick().len(), 13, "every one is answered");
        for (id, name, budget) in [
            ("0000000001-abcd", "=dcs-eval", "instructions=1000000"),
            ("0000000002-abcd", "@x.lua", "instructions=1000000"),
            ("0000000009-abcd", "=dcs-eval", "none"),
            ("0000000010-abcd", "=dcs-eval", "instructions=50000000"),
            ("0000000011-abcd", "=dcs-eval", "instructions=1000000"),
        ] {
            let e = read(&s, id);
            fields(&e, &EVAL, id);
            assert_eq!(e.headers.get("status"), Some("ok"), "{id}");
            assert_eq!(e.headers.get("result_type"), Some("nil"), "{id}");
            assert_eq!(e.headers.get("chunkname"), Some(name), "{id}");
            assert_eq!(e.headers.get("budget"), Some(budget), "{id}");
            assert_eq!(e.body, b"", "{id}: the stand-in runs nothing");
        }
        refused(
            &s,
            "0000000012-abcd",
            "bad-request",
            "max_instructions: 1.5 is not a non-negative integer",
        );
        refused(
            &s,
            "0000000013-abcd",
            "bad-request",
            &format!(
                "max_instructions: -{}... is not a non-negative integer",
                "1".repeat(79)
            ),
        );
        refused(
            &s,
            "0000000003-abcd",
            "bad-request",
            "no state: the request does not name the state to run in",
        );
        refused(
            &s,
            "0000000004-abcd",
            "bad-request",
            "state: 9x is not [A-Za-z][A-Za-z0-9_]*",
        );
        refused(
            &s,
            "0000000005-abcd",
            "unsupported",
            "nope is not a state this host serves",
        );
        refused(
            &s,
            "0000000006-abcd",
            "unsupported",
            "gui is declared and not yet served by this executor",
        );
        refused(
            &s,
            "0000000007-abcd",
            "unsupported",
            "server is declared and not yet served by this executor",
        );
        refused(
            &s,
            "0000000008-abcd",
            "bad-request",
            "chunkname: 201 bytes, over the 200-byte limit",
        );
        let export = Sandbox::new();
        let mut x = opened(&export, "export");
        sent(
            &x,
            "0000000001-abcd",
            &[("op", "eval"), ("for", &x.stamp.clone()), ("state", "hook")],
            b"return 1",
        );
        x.tick();
        refused(
            &x,
            "0000000001-abcd",
            "unsupported",
            "hook is not a state this host serves",
        );
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
    fn a_foreign_for_is_fenced_out_as_the_executor_fences_it() {
        // The fence: a `for` that is not this session's stamp is answered
        // `stale-session` before the op is looked for, so an `eval` whose
        // body would have run does not reach one. A long stamp comes back
        // whole in the header and excerpted in the message, the way the
        // executor splits the two.
        let b = Sandbox::new();
        let mut s = opened(&b, "hook");
        let long = "9".repeat(100);
        sent(
            &s,
            "0000000001-abcd",
            &[("op", "ping"), ("for", "1-1")],
            b"",
        );
        sent(
            &s,
            "0000000002-abcd",
            &[("op", "eval"), ("for", &long), ("state", "hook")],
            b"return 1",
        );
        sent(&s, "0000000003-abcd", &[("for", "1-1")], b"");
        // The byte the excerpt turns on, in both directions: eighty is not
        // longer than eighty and comes back whole, eighty-one is cut. The
        // shipped Lua is pinned at the same two bytes by `executor/fence`,
        // because a stamp of three bytes or of a hundred cannot tell a cut
        // at eighty from a cut past it, and the two dialects would then be
        // free to read the rule differently.
        let eighty = "8".repeat(80);
        let past = "7".repeat(81);
        sent(&s, "0000000004-abcd", &[("for", &eighty)], b"");
        sent(&s, "0000000005-abcd", &[("for", &past)], b"");
        // The echo is uncapped on this side too. Nothing above is long
        // enough to say so: a cap at the two hundred bytes a `chunkname` is
        // capped at would leave every stamp here whole, and `executor/fence`
        // pins the shipped Lua at three hundred for the same reason.
        let uncapped = "6".repeat(300);
        sent(&s, "0000000006-abcd", &[("for", &uncapped)], b"");
        assert_eq!(s.tick().len(), 6);

        let e = read(&s, "0000000001-abcd");
        assert_eq!(e.headers.get("status"), Some("stale-session"));
        assert_eq!(
            e.headers.get("stamp"),
            Some(s.stamp.as_str()),
            "the reply names the real stamp"
        );
        fields(&e, &FENCED, "the eight, then the stamp the request named");
        assert_eq!(e.headers.get("for"), Some("1-1"));
        assert_eq!(
            e.body,
            format!(
                "for: 1-1 is not this session's stamp, {}: the request was written \
                 for another session and was not run",
                s.stamp
            )
            .into_bytes()
        );

        let e = read(&s, "0000000002-abcd");
        assert_eq!(e.headers.get("status"), Some("stale-session"));
        assert_eq!(
            e.headers.get("for"),
            Some(long.as_str()),
            "the header carries the stamp whole"
        );
        assert!(
            e.body
                .starts_with(format!("for: {}...", "9".repeat(80)).as_bytes()),
            "and the message excerpts it at 80 bytes"
        );

        let e = read(&s, "0000000003-abcd");
        assert_eq!(
            e.headers.get("status"),
            Some("stale-session"),
            "the stamp is judged before the op, so a foreign request with none is foreign"
        );

        let e = read(&s, "0000000004-abcd");
        assert_eq!(e.headers.get("for"), Some(eighty.as_str()));
        assert_eq!(
            e.body,
            format!(
                "for: {eighty} is not this session's stamp, {}: the request was written \
                 for another session and was not run",
                s.stamp
            )
            .into_bytes(),
            "eighty bytes are not longer than eighty, so the message carries them whole"
        );

        let e = read(&s, "0000000005-abcd");
        assert_eq!(e.headers.get("for"), Some(past.as_str()));
        assert_eq!(
            e.body,
            format!(
                "for: {}... is not this session's stamp, {}: the request was written \
                 for another session and was not run",
                "7".repeat(80),
                s.stamp
            )
            .into_bytes(),
            "eighty-one are, so the message is cut to eighty and three dots"
        );

        let e = read(&s, "0000000006-abcd");
        assert_eq!(
            e.headers.get("for"),
            Some(uncapped.as_str()),
            "three hundred bytes come back whole, uncapped at two hundred"
        );
        assert!(
            e.body
                .starts_with(format!("for: {}...", "6".repeat(80)).as_bytes()),
            "while the message still excerpts at eighty"
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
