//! The bytes of a file, as they go on the wire, and the name they go under.
//!
//! The other half of evaluating a file. `file` owns everything decided
//! before a byte is read and hands back an `Admitted`; this module takes one
//! and reads it: the byte-order mark, the shebang, the digest of what is
//! actually sent, and the request the caller publishes. Decision record 0015
//! holds the six readings behind it.
//!
//! The name is here too, because it is one string with two renderings. The
//! header carries the whole of it; what an error message shows is that name
//! put through Lua's own abbreviation, which cuts sooner than either frozen
//! document's "60 bytes" suggests and cuts at a different place for a
//! compile error than for a raise. A consumer showing a user an error beside
//! the file it came from wants both, so both are rendered here.

use std::fmt;
use std::fs;
use std::io::Read as _;

use crate::file::{Admitted, FileRefusal, Refusal};
use crate::paths::Real;
use crate::sha256;

/// The most a `chunkname` header may carry. The far end caps the value at
/// this, and this crate's framer caps no value at any length, so a deeply
/// nested resolved path would otherwise frame a request the far end throws
/// away.
pub const CHUNKNAME_MAX: usize = 200;

/// The buffer Lua fills when it puts a source name into a runtime message.
/// Named for `LUA_IDSIZE`, which is what it is; the renderer below spends
/// eight of it before it copies, so a raise abbreviates a path over 52
/// bytes and not over 60.
pub const RUNTIME_IDSIZE: usize = 60;

/// The buffer the lexer fills when it puts a source name into a compile
/// error, which is a different and larger one — so the same rule abbreviates
/// a compile error's name over 72 bytes instead.
pub const COMPILE_IDSIZE: usize = 80;

/// What `luaO_chunkid` reserves for the quotes and the ellipsis it may add:
/// `sizeof(" '...' ")`, which is what makes the cut eight bytes shorter than
/// the buffer.
const RESERVE: usize = 8;

/// The name a file's chunk is compiled under: `@` and the resolved path, so
/// that an error from any state reads `<path>:<line>:`.
///
/// One place builds it because the caller hands the same string twice — to
/// `file::check`, whose ceiling counts its bytes, and to the reader — and
/// because this is where the wire's 200-byte cap on the value is enforced.
///
/// The path is spelt as this crate resolves it, with backslashes: Lua treats
/// the name as opaque and a user recognises their own spelling.
pub fn chunkname(real: &Real) -> Result<String, NameTooLong> {
    let name = format!("@{real}");
    if name.len() > CHUNKNAME_MAX {
        return Err(NameTooLong {
            bytes: name.len(),
            limit: CHUNKNAME_MAX,
        });
    }
    Ok(name)
}

/// A resolved path too long to name a chunk with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameTooLong {
    pub bytes: usize,
    pub limit: usize,
}

impl std::fmt::Display for NameTooLong {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the chunk name is {} bytes and the wire takes {} at most; evaluate it from a \
             shorter path",
            self.bytes, self.limit
        )
    }
}

impl std::error::Error for NameTooLong {}

/// `name` as Lua renders it into a message buffer of `bufflen` bytes:
/// [`RUNTIME_IDSIZE`] for a raise, [`COMPILE_IDSIZE`] for a compile error.
///
/// This is `luaO_chunkid` for the two forms that reach this wire. A `@path`
/// that does not fit is shown as `...` and its tail, which keeps the line
/// number true and shortens the path; a `=name` is truncated from the front
/// instead. Anything else — the `[string "…"]` form — is returned unchanged,
/// because this client never sends one and a rendering nothing produces
/// would be a rule with no test behind it.
///
/// A consumer prints this beside the whole name, so a user can match the
/// abbreviated path in an error against the file they asked about.
pub fn chunkid(name: &str, bufflen: usize) -> String {
    let room = bufflen.saturating_sub(RESERVE);
    match name.as_bytes().first() {
        Some(b'=') => {
            let body = &name[1..];
            // The front, up to what is left of the buffer once its
            // terminator is counted.
            let keep = bufflen.saturating_sub(1).min(body.len());
            body[..floor_char(body, keep)].to_owned()
        }
        Some(b'@') => {
            let body = &name[1..];
            if body.len() <= room {
                body.to_owned()
            } else {
                let from = body.len() - room;
                format!("...{}", &body[ceil_char(body, from)..])
            }
        }
        _ => name.to_owned(),
    }
}

/// Whether a leading byte-order mark was there and taken off. Renders as the
/// record's own word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bom {
    None,
    Stripped,
}

/// Whether a first line beginning `#` was there and emptied. Two enums and
/// not one shared marker: the two words that are not `none` are different
/// words, and the record prints them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shebang {
    None,
    Blanked,
}

impl fmt::Display for Bom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Bom::None => "none",
            Bom::Stripped => "stripped",
        })
    }
}

impl fmt::Display for Shebang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Shebang::None => "none",
            Shebang::Blanked => "blanked",
        })
    }
}

/// A file read and ready to send: the request bytes, and the provenance of
/// the chunk inside them.
///
/// One value rather than two, because the record is about the very bytes the
/// request carries and a second value would be a second chance to describe
/// bytes that are not these. Nothing is written to disk here: the run record
/// also carries a timestamp, an id, a stamp and everything that comes off
/// the reply, none of which this crate knows, so rendering the line belongs
/// to whoever holds all of it. What this crate owns is the six fields the
/// source itself answers for.
pub struct Source {
    path: Real,
    chunkname: String,
    request: Vec<u8>,
    body_at: usize,
    sha256: [u8; 32],
    bom: Bom,
    shebang: Shebang,
}

impl Source {
    /// The resolved path the bytes came from.
    pub fn path(&self) -> &Real {
        &self.path
    }

    /// The name the chunk is compiled under, whole. What a message shows of
    /// it is [`chunkid`] of this.
    pub fn chunkname(&self) -> &str {
        &self.chunkname
    }

    /// The whole request: the header block `check` measured, then the body.
    pub fn request(&self) -> &[u8] {
        &self.request
    }

    /// The chunk as it will be compiled, which is what the digest is over.
    pub fn body(&self) -> &[u8] {
        &self.request[self.body_at..]
    }

    /// The digest of [`Source::body`].
    pub fn sha256(&self) -> &[u8; 32] {
        &self.sha256
    }

    /// The digest as the record prints it.
    pub fn sha256_hex(&self) -> String {
        sha256::hex(&self.sha256)
    }

    pub fn bom(&self) -> Bom {
        self.bom
    }

    pub fn shebang(&self) -> Shebang {
        self.shebang
    }
}

impl fmt::Debug for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Source")
            .field("path", &self.path.to_string())
            .field("chunkname", &self.chunkname)
            .field("bytes", &self.body().len())
            .field("sha256", &self.sha256_hex())
            .field("bom", &self.bom.to_string())
            .field("shebang", &self.shebang.to_string())
            .finish()
    }
}

/// Read an admitted file into the bytes that go on the wire, and the record
/// of what they are.
///
/// There is no `headers` parameter. The framer writes the header lines, a
/// blank line, then the body verbatim, so the block `check` framed is the
/// same block whatever the body turns out to be; this uses that block rather
/// than framing a second one. A reader that took headers could be handed a
/// different set from the one the ceiling was measured against, and the
/// absence of the parameter is what makes that impossible rather than merely
/// checked.
///
/// The order is open, read, size, mark, shebang, empty, hash. The size is
/// compared against the raw bytes, before either rule shortens them, because
/// the question is what the file turned out to be. The mark comes off before
/// the shebang is looked for, so a file that is a mark followed by `#!` still
/// has its first line blanked.
pub fn read(admitted: &Admitted) -> Result<Source, FileRefusal> {
    let refusal = |kind| FileRefusal {
        path: admitted.path().clone(),
        kind,
    };
    // The name first, because it is a refusal this file's bytes cannot
    // change and there is no reason to read them to reach it.
    let chunkname = chunkname(admitted.path()).map_err(|source| refusal(Refusal::Name(source)))?;
    let mut file = open(admitted)?;
    let mut bytes = Vec::with_capacity(admitted.size() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|source| refusal(Refusal::Read(source)))?;
    let read = bytes.len() as u64;
    if read > admitted.size() + admitted.headroom() {
        return Err(refusal(Refusal::Grew {
            read,
            admitted: admitted.size(),
            headroom: admitted.headroom(),
        }));
    }

    let mut body = &bytes[..];
    let mut bom = Bom::None;
    if let Some(rest) = body.strip_prefix(BOM) {
        body = rest;
        bom = Bom::Stripped;
    }

    let mut shebang = Shebang::None;
    let mut body = body.to_vec();
    if body.first() == Some(&b'#') {
        // The line's content goes; its own terminator stays as it was. A
        // `\n` supplied over a line that ended `\r\n` would be the
        // conversion this reader exists not to make, and dropping the
        // terminator with the line would move every line number below it.
        match body.iter().position(|&b| b == b'\n') {
            Some(nl) => {
                // `\r\n` keeps both bytes, `\n` keeps the one.
                let keep_from = if nl > 0 && body[nl - 1] == b'\r' {
                    nl - 1
                } else {
                    nl
                };
                body.drain(..keep_from);
                shebang = Shebang::Blanked;
            }
            // A `#` line with nothing behind it is the whole file, so
            // blanking it leaves nothing; the refusal below says so.
            None => {
                body.clear();
                shebang = Shebang::Blanked;
            }
        }
    }

    if body.is_empty() {
        return Err(refusal(Refusal::Empty { bom }));
    }

    let sha256 = sha256::digest(&body);
    let mut request = admitted.block().to_vec();
    let body_at = request.len();
    request.extend_from_slice(&body);
    Ok(Source {
        path: admitted.path().clone(),
        chunkname,
        request,
        body_at,
        sha256,
        bom,
        shebang,
    })
}

const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Open the admitted path and judge the handle rather than the path.
///
/// `check` resolved the path and then judged it, and a leaf that did not
/// exist at the resolve had nothing of its own to follow; anything put there
/// since is a leaf that judgement never saw. So the open refuses to follow a
/// reparse point, and what comes back is asked what it is: a link, a
/// directory or something that is neither is refused on the handle, in the
/// same words the stat would have used for the last two.
///
/// This does not prove it is the *same* file — that wants an identity the
/// standard library does not expose — and decision record 0015 says so.
fn open(admitted: &Admitted) -> Result<fs::File, FileRefusal> {
    use std::os::windows::fs::OpenOptionsExt;

    let refusal = |kind| FileRefusal {
        path: admitted.path().clone(),
        kind,
    };
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(admitted.path().as_path())
        .map_err(|source| refusal(Refusal::Open(source)))?;
    let file_type = file
        .metadata()
        .map_err(|source| refusal(Refusal::Stat(source)))?
        .file_type();
    if file_type.is_symlink() {
        return Err(refusal(Refusal::Relinked));
    }
    if file_type.is_dir() {
        return Err(refusal(Refusal::Directory));
    }
    if !file_type.is_file() {
        return Err(refusal(Refusal::NotAFile));
    }
    Ok(file)
}

/// Open what the name points at and not what it points to. Standard
/// library, not a new declared symbol, so ADR 0011's set is untouched.
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

/// The largest index at or below `at` that begins a character, so a cut
/// through a multi-byte path does not panic. Lua counts bytes and does not
/// care; Rust's slicing does.
fn floor_char(s: &str, at: usize) -> usize {
    let mut at = at.min(s.len());
    while at > 0 && !s.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The smallest index at or above `at` that begins a character.
fn ceil_char(s: &str, at: usize) -> usize {
    let mut at = at.min(s.len());
    while at < s.len() && !s.is_char_boundary(at) {
        at += 1;
    }
    at
}

#[cfg(test)]
mod file_source {
    use super::*;
    use crate::file::{Roots, check};
    use crate::protocol;
    use crate::readers::Handshake;
    use crate::standin::Standin;
    use crate::testing::{Sandbox, real, slurp};

    use std::io;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// Compile a file's bytes under a given name and say what Lua said.
    ///
    /// The body is read in binary mode on purpose: a text-mode read would
    /// convert the CRLF this module exists to pass through, and the driver
    /// would then be testing its own reading rather than the wire's bytes.
    const DRIVER: &str = r#"
local path, name = ...
local f = assert(io.open(path, "rb"))
local body = f:read("*a")
f:close()
local chunk, err = loadstring(body, name)
if not chunk then
  io.write("compile\t", err, "\n")
else
  local ok, msg = pcall(chunk)
  if ok then io.write("ok\t\n") else io.write("run\t", tostring(msg), "\n") end
end
"#;

    /// What Lua said about `body` under `name`: the word `compile`, `run` or
    /// `ok`, and the message.
    fn lua(b: &Sandbox, body: &[u8], name: &str) -> (String, String) {
        let driver = b.join("driver.lua");
        fs::write(&driver, DRIVER).expect("the driver is written");
        let chunk = b.join("chunk.bin");
        fs::write(&chunk, body).expect("the body is written");
        let out = Command::new("lua5.1.exe")
            .arg(&driver)
            .arg(&chunk)
            .arg(name)
            .output();
        let out = match out {
            Ok(out) => out,
            Err(e) if e.kind() == io::ErrorKind::NotFound => panic!(
                "no lua5.1.exe on PATH: build it with `mise run lua-build`, then run cargo \
                 under mise, `mise exec -- cargo test -p dcs-eval file_source`"
            ),
            Err(e) => panic!("lua5.1.exe did not start: {e}"),
        };
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "the driver did not run ({}):\n{stdout}{stderr}",
            out.status
        );
        let (kind, msg) = stdout
            .trim_end_matches(['\r', '\n'])
            .split_once('\t')
            .unwrap_or_else(|| panic!("the driver said {stdout:?}"));
        (kind.to_owned(), msg.to_owned())
    }

    /// A `@` name whose path is exactly `len` bytes.
    fn name_of(len: usize) -> String {
        format!("@{}", "p".repeat(len))
    }

    // ---- the name ---------------------------------------------------------

    #[test]
    fn a_chunkname_is_an_at_sign_and_the_resolved_path() {
        let b = Sandbox::new();
        let path = b.join("probe.lua");
        fs::write(&path, b"return 1\n").expect("the file is written");
        let real = real(&path);
        assert_eq!(
            chunkname(&real).expect("a short path names a chunk"),
            format!("@{real}")
        );
    }

    #[test]
    fn a_chunkname_past_two_hundred_bytes_is_refused() {
        // The framer caps no value at any length, so this is the only thing
        // between a deep path and a header the far end throws away.
        let b = Sandbox::new();
        let mut dir = b.path.clone();
        while dir.as_os_str().len() < CHUNKNAME_MAX {
            dir = dir.join("a-directory-with-a-name-of-its-own");
        }
        fs::create_dir_all(&dir).expect("the deep directory is made");
        let path = dir.join("probe.lua");
        fs::write(&path, b"return 1\n").expect("the file is written");
        let err = chunkname(&real(&path)).expect_err("too long to name");
        assert!(err.bytes > CHUNKNAME_MAX, "{err}");
        assert_eq!(err.limit, CHUNKNAME_MAX);
        assert!(err.to_string().contains("200"), "{err}");
    }

    #[test]
    fn a_chunkname_of_exactly_two_hundred_bytes_is_not() {
        // The boundary is the far end's own `at most 200 bytes`, so the
        // fixture is a path padded to land the name on it exactly.
        let b = Sandbox::new();
        let here = real(&b.path).to_string().len();
        let filler = CHUNKNAME_MAX - 1 - here - "\\".len() * 2 - "probe.lua".len();
        let dir = b.path.join("d".repeat(filler));
        fs::create_dir_all(&dir).expect("the padded directory is made");
        let path = dir.join("probe.lua");
        fs::write(&path, b"return 1\n").expect("the file is written");
        let name = chunkname(&real(&path)).expect("exactly at the limit");
        assert_eq!(name.len(), CHUNKNAME_MAX, "{name}");
    }

    #[test]
    fn chunkid_abbreviates_a_path_over_fifty_two_bytes() {
        // 52 is the cut, not 60: the buffer is 60 and eight of it is spent
        // before anything is copied.
        let whole = "p".repeat(52);
        assert_eq!(chunkid(&name_of(52), RUNTIME_IDSIZE), whole);
        assert_eq!(chunkid(&name_of(53), RUNTIME_IDSIZE), format!("...{whole}"));
        assert_eq!(chunkid(&name_of(90), RUNTIME_IDSIZE), format!("...{whole}"));
    }

    #[test]
    fn chunkid_abbreviates_a_compile_error_name_over_seventy_two_bytes() {
        let whole = "p".repeat(72);
        assert_eq!(chunkid(&name_of(72), COMPILE_IDSIZE), whole);
        assert_eq!(chunkid(&name_of(73), COMPILE_IDSIZE), format!("...{whole}"));
    }

    #[test]
    fn chunkid_truncates_an_equals_name() {
        assert_eq!(chunkid("=short", RUNTIME_IDSIZE), "short");
        let long = format!("={}", "q".repeat(100));
        assert_eq!(
            chunkid(&long, RUNTIME_IDSIZE),
            "q".repeat(RUNTIME_IDSIZE - 1)
        );
    }

    #[test]
    fn chunkid_leaves_a_name_of_neither_form_alone() {
        assert_eq!(chunkid("plain", RUNTIME_IDSIZE), "plain");
    }

    #[test]
    fn what_lua_prints_for_a_long_name_is_what_chunkid_says() {
        // The non-circular pin: the renderer against the interpreter, at
        // lengths either side of both thresholds.
        let b = Sandbox::new();
        let body = b"\nlocal a = 1\nerror('boom')\n";
        for len in [50usize, 52, 53, 66, 72, 73, 90] {
            let name = name_of(len);
            let (kind, msg) = lua(&b, body, &name);
            assert_eq!(kind, "run", "the body raises: {msg}");
            let want = format!("{}:3: boom", chunkid(&name, RUNTIME_IDSIZE));
            assert_eq!(msg, want, "a raise under a {len}-byte path");
        }
    }

    #[test]
    fn a_compile_error_abbreviates_later_than_a_raise() {
        // At a length between the two thresholds — which a resolved Windows
        // path routinely is — the compile error shows the whole path and the
        // raise does not. A fixture that proved the line number with a
        // compile error would therefore stop testing abbreviation at all.
        let b = Sandbox::new();
        let len = 66;
        let name = name_of(len);
        let (kind, msg) = lua(&b, b"\nlocal x =\n", &name);
        assert_eq!(kind, "compile", "the body will not parse: {msg}");
        assert!(
            msg.starts_with(&"p".repeat(len)),
            "the compile error carries the whole path: {msg}"
        );
        let (kind, msg) = lua(&b, b"\nlocal a = 1\nerror('boom')\n", &name);
        assert_eq!(kind, "run", "and the raise does not: {msg}");
        assert!(msg.starts_with("..."), "{msg}");
    }

    // ---- the bytes --------------------------------------------------------

    /// A box with a project root that is allowed and a session that has
    /// published, so the ceiling comes off a handshake rather than out of a
    /// constant.
    struct Scene {
        /// Held, not read: dropping it takes the fixtures with it, so the
        /// scene has to outlive every path below.
        _b: Sandbox,
        project: Real,
        h: Handshake,
    }

    fn scene() -> Scene {
        let b = Sandbox::new();
        let project = b.join("project");
        fs::create_dir_all(&project).expect("the project root is made");
        let session = b.join("session");
        fs::create_dir_all(&session).expect("the session root is made");
        let s = Standin::open(&session, "hook").expect("the session opens");
        s.handshake().expect("the handshake publishes");
        let bytes = slurp(&s.output().join("executor.txt"));
        let h =
            Handshake::from_bytes(Path::new("executor.txt"), &bytes).expect("the handshake reads");
        Scene {
            _b: b,
            project: real(&project),
            h,
        }
    }

    impl Scene {
        fn roots(&self) -> Roots {
            Roots::new(std::slice::from_ref(&self.project), &[], None).expect("the roots resolve")
        }

        /// A file under the allowed root, written and put through `check`.
        /// The headers are the ones a caller really sends, the name among
        /// them, so the block the reader reuses is a realistic one.
        fn admit(&self, name: &str, bytes: &[u8]) -> crate::file::Admitted {
            let path = self.write(name, bytes);
            let real = real(&path);
            let headers = self.headers(&real);
            let refs: Vec<(&str, &str)> = headers
                .iter()
                .map(|(n, v)| (n.as_str(), v.as_str()))
                .collect();
            check(&self.roots(), &self.h, &refs, &real).expect("the file is admitted")
        }

        fn headers(&self, real: &Real) -> Vec<(String, String)> {
            vec![
                ("op".to_owned(), "eval".to_owned()),
                ("state".to_owned(), "hook".to_owned()),
                (
                    "chunkname".to_owned(),
                    chunkname(real).expect("the name fits"),
                ),
            ]
        }

        fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.project.as_path().join(name);
            fs::write(&path, bytes).expect("the fixture is written");
            path
        }
    }

    /// A fixture chosen for the byte-for-byte control: `\r\n` endings, a
    /// lone `\r`, a `\0` and a byte past ASCII, so a conversion or a lossy
    /// round trip on the way in shows rather than hides.
    const AWKWARD: &[u8] =
        b"-- one\r\n-- two\rstill two\r\nlocal z = '\x00\xC3\xA9'\r\nreturn z\r\n";

    #[test]
    fn a_plain_file_is_shipped_byte_for_byte() {
        let s = scene();
        let admitted = s.admit("awkward.lua", AWKWARD);
        let source = read(&admitted).expect("the file reads");
        assert_eq!(source.body(), AWKWARD, "the bytes are not the file's");
    }

    #[test]
    fn the_record_says_none_when_there_was_no_bom_and_no_shebang() {
        let s = scene();
        let source = read(&s.admit("plain.lua", b"return 1\n")).expect("the file reads");
        assert_eq!(source.bom(), Bom::None);
        assert_eq!(source.shebang(), Shebang::None);
        assert_eq!(source.bom().to_string(), "none");
        assert_eq!(source.shebang().to_string(), "none");
    }

    #[test]
    fn a_bom_is_stripped_and_said_so() {
        let s = scene();
        let source = read(&s.admit("bom.lua", b"\xEF\xBB\xBFreturn 1\n")).expect("the file reads");
        assert_eq!(source.body(), b"return 1\n");
        assert_eq!(source.bom(), Bom::Stripped);
        assert_eq!(source.bom().to_string(), "stripped");
    }

    #[test]
    fn a_shebang_is_blanked_and_its_terminator_kept() {
        // The line's text goes and its own `\r\n` stays, so the body is the
        // file less exactly the text.
        let s = scene();
        let file = b"#!/usr/bin/env lua\r\nreturn 1\r\n";
        let source = read(&s.admit("she.lua", file)).expect("the file reads");
        assert_eq!(source.body(), b"\r\nreturn 1\r\n");
        assert_eq!(
            source.body().len(),
            file.len() - "#!/usr/bin/env lua".len(),
            "only the text is gone"
        );
        assert_eq!(source.shebang(), Shebang::Blanked);
        assert_eq!(source.shebang().to_string(), "blanked");
    }

    #[test]
    fn a_shebang_ending_in_a_bare_newline_keeps_that_one_byte() {
        let s = scene();
        let source = read(&s.admit("she-lf.lua", b"#!lua\nreturn 1\n")).expect("the file reads");
        assert_eq!(source.body(), b"\nreturn 1\n");
    }

    #[test]
    fn a_bom_before_a_shebang_still_blanks_the_shebang() {
        // The ordering, stated once: the mark comes off first, so the `#`
        // is a first byte when the shebang rule looks at it.
        let s = scene();
        let source =
            read(&s.admit("both.lua", b"\xEF\xBB\xBF#!lua\r\nreturn 1\r\n")).expect("it reads");
        assert_eq!(source.body(), b"\r\nreturn 1\r\n");
        assert_eq!(source.bom(), Bom::Stripped);
        assert_eq!(source.shebang(), Shebang::Blanked);
    }

    #[test]
    fn a_shebang_with_no_terminator_leaves_nothing() {
        let s = scene();
        let err = read(&s.admit("only-she.lua", b"#!lua")).expect_err("nothing is left");
        assert!(
            matches!(err.kind, Refusal::Empty { bom: Bom::None }),
            "{err:?}"
        );
    }

    #[test]
    fn a_file_that_is_only_a_bom_is_refused() {
        let s = scene();
        let err = read(&s.admit("just-bom.lua", b"\xEF\xBB\xBF")).expect_err("nothing is left");
        assert!(
            matches!(err.kind, Refusal::Empty { bom: Bom::Stripped }),
            "{err:?}"
        );
        assert!(err.to_string().contains("byte-order mark"), "{err}");
    }

    #[test]
    fn an_empty_file_is_refused_in_different_words() {
        let s = scene();
        let err = read(&s.admit("empty.lua", b"")).expect_err("nothing to evaluate");
        assert!(
            matches!(err.kind, Refusal::Empty { bom: Bom::None }),
            "{err:?}"
        );
        assert!(!err.to_string().contains("byte-order mark"), "{err}");
    }

    #[test]
    fn a_file_whose_first_hash_is_not_on_line_one_is_left_alone() {
        // `#` is a Lua operator, so only the first line is a shebang.
        let s = scene();
        let file = b"local t = {}\n#t\n";
        let source = read(&s.admit("hash.lua", file)).expect("the file reads");
        assert_eq!(source.body(), file);
        assert_eq!(source.shebang(), Shebang::None);
    }

    #[test]
    fn the_hash_is_over_what_is_sent_and_not_over_the_file() {
        // Both readings in one place, so neither can drift alone: a plain
        // file agrees with `sha256sum`, and a file the two rules changed
        // does not — it agrees with the digest of the body instead.
        let s = scene();
        let plain = s.write("plain-hash.lua", b"return 1\n");
        let source = read(&s.admit("plain-hash.lua", b"return 1\n")).expect("it reads");
        assert_eq!(
            source.sha256_hex(),
            crate::testing::sha256sum(&plain),
            "a file with neither rule applied hashes as the tool says"
        );

        let bytes = b"\xEF\xBB\xBF#!lua\r\nreturn 1\r\n";
        let marked = s.write("marked.lua", bytes);
        let source = read(&s.admit("marked.lua", bytes)).expect("it reads");
        assert_eq!(
            source.sha256_hex(),
            sha256::hex(&sha256::digest(source.body())),
            "the digest is the body's"
        );
        assert_ne!(
            source.sha256_hex(),
            crate::testing::sha256sum(&marked),
            "and not the file's, which is why the record carries bom and shebang"
        );
    }

    #[test]
    fn the_request_is_the_block_check_measured_and_then_the_body() {
        let s = scene();
        let path = s.write("req.lua", b"return 1\n");
        let real = real(&path);
        let headers = s.headers(&real);
        let refs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(n, v)| (n.as_str(), v.as_str()))
            .collect();
        let admitted = check(&s.roots(), &s.h, &refs, &real).expect("admitted");
        let source = read(&admitted).expect("it reads");
        let envelope = protocol::parse(source.request()).expect("the request parses");
        let sent: Vec<(String, String)> = envelope
            .headers
            .iter()
            .map(|(n, v)| (n.to_owned(), v.to_owned()))
            .collect();
        assert_eq!(sent, headers, "the headers are the ones check was given");
        assert_eq!(envelope.body, b"return 1\n", "and the body is the chunk");
    }

    #[test]
    fn a_file_exactly_at_the_ceiling_is_sent_whole() {
        let s = scene();
        let path = s.project.as_path().join("exact.lua");
        let real_guess = path.clone();
        // The header block depends on the name, which depends on the path,
        // so the block is measured on the path the file will have.
        fs::write(&real_guess, b"x").expect("a placeholder so the path resolves");
        let real = real(&real_guess);
        let headers = s.headers(&real);
        let refs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(n, v)| (n.as_str(), v.as_str()))
            .collect();
        let block = protocol::frame(&refs, b"")
            .expect("the headers frame")
            .len() as u64;
        let size = s.h.max_request_bytes - block;
        fs::write(&real_guess, vec![b'-'; size as usize]).expect("the fixture is written");
        let admitted = check(&s.roots(), &s.h, &refs, &real).expect("exactly at the ceiling");
        assert_eq!(admitted.headroom(), 0, "nothing is left over");
        let source = read(&admitted).expect("and it is read whole");
        assert_eq!(source.body().len() as u64, size, "nothing is truncated");
        assert_eq!(
            source.request().len() as u64,
            s.h.max_request_bytes,
            "and the request is the ceiling exactly"
        );
        assert_eq!(
            source.body(),
            fs::read(&real_guess).expect("the file reads")
        );
    }

    #[test]
    fn a_file_that_grew_after_the_stat_is_refused_naming_the_three_figures() {
        let s = scene();
        let admitted = s.admit("growing.lua", b"return 1\n");
        // Past the ceiling, not merely past the stat: a file with headroom
        // that grew into it is still sendable.
        let big = vec![b'x'; (s.h.max_request_bytes + 1) as usize];
        fs::write(admitted.path().as_path(), &big).expect("the file grows");
        let err = read(&admitted).expect_err("it grew past the ceiling");
        let line = err.to_string();
        assert!(matches!(err.kind, Refusal::Grew { .. }), "{err:?}");
        assert!(line.contains(&big.len().to_string()), "the read: {line}");
        assert!(
            line.contains(&admitted.size().to_string()),
            "the stat: {line}"
        );
        assert!(
            line.contains(&admitted.headroom().to_string()),
            "the headroom: {line}"
        );
    }

    #[test]
    fn a_file_removed_between_the_check_and_the_read_is_refused() {
        let s = scene();
        let admitted = s.admit("vanishing.lua", b"return 1\n");
        fs::remove_file(admitted.path().as_path()).expect("the file goes");
        let err = read(&admitted).expect_err("there is nothing to open");
        assert!(matches!(err.kind, Refusal::Open(_)), "{err:?}");
    }

    #[test]
    fn a_leaf_swapped_for_a_junction_is_refused_on_the_open() {
        // A directory junction, not a file symlink: a symlink here needs
        // elevation this build does not run with, so the `Relinked` arm is
        // unproven and decision record 0015 says so.
        //
        // The refusal observed on this host is the open's own — the flag
        // that stops the junction being followed leaves a handle this open
        // cannot have, and Windows answers "Access is denied." The
        // assertion is on what was seen rather than on what was hoped for;
        // what it holds either way is the thing that matters, that a leaf
        // swapped between the judgement and the read does not come back as
        // bytes.
        let s = scene();
        let admitted = s.admit("swapped.lua", b"return 1\n");
        let elsewhere = s.project.as_path().join("elsewhere");
        fs::create_dir_all(&elsewhere).expect("somewhere to point at");
        fs::remove_file(admitted.path().as_path()).expect("the file goes");
        crate::testing::junction(admitted.path().as_path(), &elsewhere);
        let err = read(&admitted).expect_err("a swapped leaf is not read");
        assert!(matches!(err.kind, Refusal::Open(_)), "{err:?}");
    }
}
