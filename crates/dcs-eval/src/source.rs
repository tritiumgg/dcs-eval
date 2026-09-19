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

use crate::paths::Real;

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
    use crate::testing::{Sandbox, real};

    use std::fs;
    use std::io;
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
}
