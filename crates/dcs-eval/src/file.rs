//! What evaluating a file refuses, and what it learns about it before a byte
//! is read.
//!
//! The judging is here; the reading is in [`crate::source`]. One refusal type
//! spans both halves, because a caller asking "may I evaluate this file"
//! wants one answer and one sentence: which side of the split decided it is
//! this module's business and not the caller's.
//!
//! Where a path lies is not judged. Anything this process can open, the hook
//! state's own `io` can open too, so a rule about location refuses the tool
//! and protects nothing; decision record 0026 says why. What is judged is
//! what can be known without reading a byte: that the path names a regular
//! file, and that its size plus the bytes of the request's own header block
//! does not exceed the request ceiling the handshake published. Both come
//! off one stat, before the file is opened.

use std::fmt;
use std::fs;
use std::io;

use crate::paths::Real;
use crate::protocol::{self, FrameError};
use crate::readers::Handshake;

/// A file that may be read, and what was learned about it without reading
/// it.
///
/// `size` is what the stat said and nothing more: the file may have grown
/// since, and whoever reads the bytes owns that gap. There is a second gap
/// of the same kind in front of it: the path was resolved before it was
/// judged, and a leaf that did not exist then had nothing of its own to
/// follow, so a junction or a symlink put there afterwards is one this
/// judgement never saw. Whoever opens the path owns that gap too, and the
/// reader closes it by judging the handle it opened rather than the path it
/// was given.
///
/// `headroom` is `max_request_bytes` less the header block less `size` —
/// the bytes a reader still has in hand once this file's bytes are in the
/// envelope — defined here once so nobody has to derive it a second time.
///
/// The fields are private for the reason [`Real`]'s is: [`check`] is then
/// the only thing that can make one, so a reader taking an `Admitted` has
/// the judgement in its signature rather than in a note asking its callers
/// to have run one. A public field would let any caller assemble a path
/// nothing judged and a size that came from nowhere.
///
/// `block` is the framed header block, kept rather than recomputed: the
/// reader sends these very bytes, so the envelope it publishes is the one
/// whose length was counted against the ceiling and there is no second set
/// of headers to disagree with the measured one.
///
/// `chunkname` is that block's own `chunkname` value, lifted out of the
/// very headers the block was framed from, or `None` where the caller sent
/// no such header. The reader records the name the far end will compile
/// under, and it has to read it off what is being sent: a name derived a
/// second time from the path would be a second answer, and the record would
/// then be free to name a chunk the wire does not carry.
#[derive(Clone)]
pub struct Admitted {
    path: Real,
    size: u64,
    headroom: u64,
    block: Vec<u8>,
    chunkname: Option<String>,
}

// The block is bytes nobody wants in a failure message, so it is shown as a
// length. Everything else is what a test that went red needs to read.
impl fmt::Debug for Admitted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Admitted")
            .field("path", &self.path.to_string())
            .field("size", &self.size)
            .field("headroom", &self.headroom)
            .field("header_bytes", &self.block.len())
            .finish()
    }
}

impl Admitted {
    /// The resolved path that was judged, which is the one to open: a
    /// reader that re-spells it has stepped outside what was checked.
    pub fn path(&self) -> &Real {
        &self.path
    }

    /// What the stat said, in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// What is left of the request ceiling once the header block and a file
    /// of `size` are in the envelope.
    pub fn headroom(&self) -> u64 {
        self.headroom
    }

    /// The framed header block the ceiling was measured against, which the
    /// reader puts the body behind. Crate-private: it is an internal hand-off
    /// between the two halves and not a thing a caller assembles.
    pub(crate) fn block(&self) -> &[u8] {
        &self.block
    }

    /// The `chunkname` the block carries, or `None` where it carries none.
    /// Crate-private for the reason `block` is: it is the other half of the
    /// same hand-off.
    pub(crate) fn chunkname(&self) -> Option<&str> {
        self.chunkname.as_deref()
    }
}

/// Whether `real` names a file that would fit, deciding it before anything
/// opens it.
///
/// The parameter is a [`Real`] rather than a path because only
/// [`crate::paths::resolve`] can make one. The chunk is named after the
/// resolved path, the run record names it, and the stat here has to measure
/// that same object; a type that cannot be handed an unresolved spelling
/// keeps the three from coming apart. It also means the caller has the
/// resolved path in hand before it builds `headers`, which matters because
/// one of those headers names the resolved path and its bytes are among the
/// ones counted here.
///
/// `headers` is the exact set the caller will send, and the count is taken
/// by framing them with an empty body rather than estimated: the executor
/// stats the whole request file, so every header line and the blank line
/// ending the block counts against the ceiling. The ceiling itself comes off
/// the handshake, which is why this takes one rather than a number a caller
/// could have invented.
///
/// The order is frame, stat, compare, and the one stat answers twice: what
/// the path names and how long it is. Nothing that reads the file runs
/// before the stat, and the framer's own refusal is raised before the stat
/// because a request that cannot be written is not a question about this
/// file at all.
pub fn check(
    h: &Handshake,
    headers: &[(&str, &str)],
    real: &Real,
) -> Result<Admitted, FileRefusal> {
    let block = protocol::frame(headers, b"").map_err(|source| FileRefusal {
        path: real.clone(),
        kind: Refusal::Frame(source),
    })?;
    let header_bytes = block.len() as u64;
    // Off the same slice the block was framed from, in the same breath, so
    // the name kept beside the block is the name inside it. `frame` refuses
    // a repeated name however it is spelt, so there is one line to find.
    let chunkname = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("chunkname"))
        .map(|(_, value)| (*value).to_owned());
    let stat = fs::metadata(real.as_path()).map_err(|source| FileRefusal {
        path: real.clone(),
        kind: Refusal::Stat(source),
    })?;
    // The same stat says what the path names, and that answer belongs on
    // this side of the split for the reason the size does: it is knowable
    // without reading a byte. A directory admitted here would be a
    // judgement that something can be sent as a chunk handed to a reader
    // that can only fail to open it, and the failure would arrive as the
    // operating system's own words about a path this module had already
    // said yes to — a refusal owed by the guard, paid by whoever came
    // next. Anything that is neither a regular file nor a directory gets
    // its own answer rather than being folded into one of them: on this
    // host that is a device, a pipe or a socket, none of which has a size a
    // stat can be believed about or an end a read is sure to reach, so the
    // ceiling above would be measuring nothing. A link is not a third
    // answer, because `real` is resolved and the stat follows what is left.
    let file_type = stat.file_type();
    if !file_type.is_file() {
        return Err(FileRefusal {
            path: real.clone(),
            kind: if file_type.is_dir() {
                Refusal::Directory
            } else {
                Refusal::NotAFile
            },
        });
    }
    let size = stat.len();
    // The sum, never the limit less the header block: a header block at or
    // past the limit makes that subtraction saturate to nothing and then
    // admits an empty file whose framed request is already over.
    let total = size.saturating_add(header_bytes);
    if total > h.max_request_bytes {
        return Err(FileRefusal {
            path: real.clone(),
            kind: Refusal::Oversize {
                size,
                header_bytes,
                limit: h.max_request_bytes,
            },
        });
    }
    Ok(Admitted {
        path: real.clone(),
        size,
        headroom: h.max_request_bytes - total,
        block,
        chunkname,
    })
}

/// A path that will not be read, and why. `Display` is `<path>: <reason>`,
/// the shape every refusal in this client takes.
///
/// No variant carries anything read out of the file: a refusal is decided on
/// the stat and the handshake, so it has no bytes to leak.
#[derive(Debug)]
pub struct FileRefusal {
    pub path: Real,
    pub kind: Refusal,
}

#[derive(Debug)]
pub enum Refusal {
    /// Too big for one request. Every figure here comes off the stat and
    /// the handshake; none of them was derived from a byte of the file.
    Oversize {
        size: u64,
        header_bytes: u64,
        limit: u64,
    },
    /// The request's own headers could not be written, so there was no
    /// point asking how big the file is.
    Frame(FrameError),
    /// The file could not be stated. Nothing was read.
    Stat(io::Error),
    /// A directory. Within the ceiling, since a directory stats at no
    /// length, and still not a chunk.
    Directory,
    /// Neither a regular file nor a directory, so nothing whose length the
    /// stat settles and nothing a read is sure to reach the end of.
    NotAFile,
    /// The resolved path is too long to name the chunk with. Refused here
    /// rather than sent, because the far end caps the header and this
    /// crate's framer caps no value at any length.
    Name(crate::source::NameTooLong),
    /// The leaf is a link or a junction now, and was not when the path was
    /// resolved. The resolver follows what is there at the time, so this is
    /// something put in the way since — refused rather than followed,
    /// because nothing judged where it leads.
    Relinked,
    /// The file could not be opened, although the stat had answered about
    /// it. Between the two somebody may have removed it, replaced it, or
    /// taken a hold that denies this process's reads.
    Open(io::Error),
    /// The file opened and then would not read to the end.
    Read(io::Error),
    /// The file grew past the ceiling between the stat and the read. The
    /// figures are all three of them, because "too big" without the one it
    /// was measured against a moment ago reads as a contradiction.
    ///
    /// `read` is a floor and not the file's length: the reader stops one
    /// byte past the ceiling, so a file that grew without bound is refused
    /// without being buffered, and how far past it went is not known here.
    Grew {
        read: u64,
        admitted: u64,
        headroom: u64,
    },
    /// Nothing left to evaluate once the two rules had run. The far end
    /// answers an empty body `bad-request`, and a refusal issued here can
    /// say which kind of empty it was.
    Empty { bom: crate::source::Bom },
}

impl FileRefusal {
    /// The refusal without the path in front of it, so two refusals about
    /// different paths can be compared for having given the same reason.
    pub fn reason(&self) -> String {
        match &self.kind {
            Refusal::Oversize {
                size,
                header_bytes,
                limit,
            } => format!(
                "is {size} bytes and the request's headers are {header_bytes} more, over the \
                 {limit}-byte limit the handshake published; split the file, or dofile it from a \
                 one-line chunk in a state that has io"
            ),
            Refusal::Frame(source) => format!("the request's headers were refused: {source}"),
            Refusal::Stat(source) => format!("could not be stated: {source}"),
            Refusal::Directory => {
                "is a directory, and a directory is not a chunk to evaluate".to_owned()
            }
            Refusal::NotAFile => {
                "is neither a file nor a directory, and only a file is evaluated from here"
                    .to_owned()
            }
            Refusal::Relinked => {
                "became a link after it was resolved, and where a link leads is not what was \
                 judged"
                    .to_owned()
            }
            Refusal::Name(source) => source.to_string(),
            Refusal::Open(source) => format!("could not be opened: {source}"),
            Refusal::Read(source) => format!("could not be read to the end: {source}"),
            Refusal::Grew {
                read,
                admitted,
                headroom,
            } => format!(
                "was {admitted} bytes when it was checked, with {headroom} to spare, and read at \
                 least {read}; it grew past the request ceiling while it was being read"
            ),
            Refusal::Empty { bom } => match bom {
                crate::source::Bom::Stripped => {
                    "was a byte-order mark and nothing else, so it holds no chunk to evaluate"
                        .to_owned()
                }
                crate::source::Bom::None => "holds no chunk to evaluate".to_owned(),
            },
        }
    }
}

impl fmt::Display for FileRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.reason())
    }
}

impl std::error::Error for FileRefusal {}

#[cfg(test)]
mod file_refusals {
    use super::*;
    use crate::standin::Standin;
    use crate::testing::{Sandbox, held, real, slurp, with};

    use std::fs;
    use std::path::{Path, PathBuf};

    /// A directory under the box, made, and resolved.
    fn dir(b: &Sandbox, name: &str) -> Real {
        let path = b.path.join(name);
        fs::create_dir_all(&path).expect("the directory is made");
        real(&path)
    }

    /// A file at `path` with `bytes` in it, resolved.
    fn file(path: &PathBuf, bytes: &[u8]) -> Real {
        fs::create_dir_all(path.parent().expect("a parent")).expect("the parent is made");
        fs::write(path, bytes).expect("the file is written");
        real(path)
    }

    /// The usual shape: a project directory, a write directory with a
    /// `Config` in it, an install-shaped tree, and a place that is none of
    /// them — the places a location rule would once have told apart.
    struct Box_ {
        b: Sandbox,
        project: Real,
        writedir: Real,
        install: Real,
        outside: Real,
    }

    fn scene() -> Box_ {
        let b = Sandbox::new();
        let project = dir(&b, "project");
        let writedir = dir(&b, "Saved Games\\DCS");
        let install = dir(&b, "install");
        let outside = dir(&b, "elsewhere");
        dir(&b, "Saved Games\\DCS\\Config");
        Box_ {
            b,
            project,
            writedir,
            install,
            outside,
        }
    }

    // ---- where the path lies ----------------------------------------------

    #[test]
    fn admits_a_file_wherever_it_lies() {
        // A write directory's `Config`, an install's `Scripts` and a place
        // that is neither: once refused by three different rules, now each
        // judged by its size alone. A fourth sits beside the install guard
        // the handshake names, the one install path `check` is handed, so a
        // refusal re-derived from the handshake cannot stay green here.
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let crate::readers::Diagnostic::Real(guard) = &h.install_guard else {
            panic!("the stand-in's handshake names an install guard that resolves")
        };
        let beside_the_guard = guard
            .as_path()
            .parent()
            .expect("the guard has a parent")
            .join("guarded.lua");
        let places = [
            beside_the_guard,
            s.writedir.as_path().join("Config").join("network.vault"),
            s.install
                .as_path()
                .join("Scripts")
                .join("MissionScripting.lua"),
            s.outside.as_path().join("x.lua"),
        ];
        for path in places {
            let real = file(&path, b"return 1\n");
            let ok = check(&h, HEADERS, &real).expect("admitted wherever it lies");
            assert_eq!(ok.size(), 9, "the size is what the stat said");
        }
    }

    // ---- the ceiling ------------------------------------------------------

    /// A handshake from the stand-in's own bytes. The ceiling has to come
    /// off one of these rather than out of a constant, which is why every
    /// test below needs a session to have published.
    fn handshake_bytes(b: &Sandbox) -> Vec<u8> {
        let root = b.path.join("session");
        fs::create_dir_all(&root).expect("the session root");
        let s = Standin::open(&root, "hook").expect("the session opens");
        s.handshake().expect("the handshake publishes");
        slurp(&s.output().join("executor.txt"))
    }

    fn handshake(bytes: &[u8]) -> Handshake {
        Handshake::from_bytes(Path::new("executor.txt"), bytes).expect("the handshake reads")
    }

    /// A plausible header set for a request. What is in it does not matter;
    /// how many bytes it frames to does.
    const HEADERS: &[(&str, &str)] = &[("op", "eval"), ("state", "hook")];

    fn block_len(headers: &[(&str, &str)]) -> u64 {
        protocol::frame(headers, b"")
            .expect("the headers frame")
            .len() as u64
    }

    /// A file of exactly `size` bytes under the project directory, resolved.
    fn sized(s: &Box_, name: &str, size: u64) -> Real {
        file(
            &s.project.as_path().join(name),
            &vec![b'x'; size as usize][..],
        )
    }

    #[test]
    fn refuses_a_file_one_byte_over_the_file_ceiling_naming_the_limit_and_the_size() {
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let header_bytes = block_len(HEADERS);
        let size = h.max_request_bytes - header_bytes + 1;
        let real = sized(&s, "big.lua", size);
        let err = check(&h, HEADERS, &real).expect_err("one byte over");
        assert!(
            matches!(err.kind, Refusal::Oversize { .. }),
            "over the ceiling: {err}"
        );
        let line = err.to_string();
        assert!(line.contains(&size.to_string()), "names the size: {line}");
        assert!(
            line.contains(&h.max_request_bytes.to_string()),
            "names the limit: {line}"
        );
    }

    #[test]
    fn admits_a_file_exactly_at_the_file_ceiling() {
        // The file's ceiling is the request ceiling less the header block,
        // and such a file frames to exactly the request ceiling, which the
        // executor's own `size > MAX_REQUEST_BYTES` accepts.
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let header_bytes = block_len(HEADERS);
        let size = h.max_request_bytes - header_bytes;
        let real = sized(&s, "exact.lua", size);
        let ok = check(&h, HEADERS, &real).expect("exactly at the ceiling");
        assert_eq!(ok.size(), size, "the size is what the stat said");
        assert_eq!(ok.headroom(), 0, "and nothing is left over");
    }

    #[test]
    fn the_header_block_counts_toward_the_ceiling() {
        // One file, two header sets one byte apart. The executor stats the
        // whole request file, so the envelope's own bytes are part of what
        // has to fit.
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let short: &[(&str, &str)] = &[("op", "eval"), ("state", "hook")];
        let long: &[(&str, &str)] = &[("op", "eval"), ("state", "hooks")];
        assert_eq!(
            block_len(long),
            block_len(short) + 1,
            "the two blocks differ by one byte"
        );
        let size = h.max_request_bytes - block_len(long) + 1;
        let real = sized(&s, "edge.lua", size);
        check(&h, short, &real).expect("it fits under the shorter block");
        let err = check(&h, long, &real).expect_err("and not under the longer one");
        assert!(matches!(err.kind, Refusal::Oversize { .. }), "{err}");
    }

    #[test]
    fn the_ceiling_is_the_handshakes_own_and_not_the_default() {
        let s = scene();
        let h = handshake(&with(&handshake_bytes(&s.b), "max_request_bytes", "1024"));
        assert_eq!(h.max_request_bytes, 1024, "the session says 1024");
        let real = sized(&s, "two-thousand.lua", 2_000);
        let err = check(&h, HEADERS, &real).expect_err("over this session's ceiling");
        let line = err.to_string();
        assert!(line.contains("1024"), "the session's figure: {line}");
        assert!(line.contains("2000"), "and the file's: {line}");
    }

    #[test]
    fn a_header_block_at_the_limit_refuses_even_an_empty_file() {
        // The case the limit-less-the-header spelling gets wrong: that
        // subtraction saturates to nothing and then admits a file whose
        // framed request is already over.
        let s = scene();
        let header_bytes = block_len(HEADERS);
        let limit = (header_bytes - 1).to_string();
        let h = handshake(&with(&handshake_bytes(&s.b), "max_request_bytes", &limit));
        let real = sized(&s, "empty.lua", 0);
        let err = check(&h, HEADERS, &real).expect_err("the headers alone are over");
        assert!(matches!(err.kind, Refusal::Oversize { .. }), "{err}");
    }

    #[test]
    fn refuses_headers_the_framer_would_refuse_before_the_stat() {
        // The path is one that is not there, so an implementation that
        // stated first would answer about the stat instead.
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let real = real(&s.project.as_path().join("absent.lua"));
        let bad: &[(&str, &str)] = &[("op", "eval\nstate: hook")];
        let err = check(&h, bad, &real).expect_err("the framer refuses");
        assert!(matches!(err.kind, Refusal::Frame(_)), "{err}");
    }

    #[test]
    fn refuses_a_chunkname_carrying_a_byte_past_ascii_before_the_stat() {
        // A resolved path with a byte past ASCII cannot go on the wire at
        // all, and that is settled before the file is asked about.
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let real = real(&s.project.as_path().join("absent.lua"));
        let bad: &[(&str, &str)] = &[("chunkname", "@C:\\Users\\Ünter\\x.lua")];
        let err = check(&h, bad, &real).expect_err("the value is not ASCII");
        assert!(matches!(err.kind, Refusal::Frame(_)), "{err}");
    }

    #[test]
    fn a_stat_that_fails_is_named() {
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let real = real(&s.project.as_path().join("absent.lua"));
        let err = check(&h, HEADERS, &real).expect_err("nothing to state");
        assert!(matches!(err.kind, Refusal::Stat(_)), "{err}");
        assert!(err.reason().starts_with("could not be stated"), "{err}");
    }

    // ---- what the path names ----------------------------------------------

    #[test]
    fn refuses_a_directory() {
        // A directory stats at no length, so the ceiling has nothing to
        // refuse it with. The stat that measured it is what knows better.
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let real = real(&s.project.as_path().join("subdir"));
        fs::create_dir_all(real.as_path()).expect("the directory is made");
        let err = check(&h, HEADERS, &real).expect_err("a directory is not a chunk");
        assert!(matches!(err.kind, Refusal::Directory), "{err}");
        assert_eq!(
            err.reason(),
            "is a directory, and a directory is not a chunk to evaluate"
        );
        assert!(err.to_string().starts_with(&real.to_string()), "{err}");
    }

    #[test]
    fn admits_an_ordinary_file() {
        // The other half of the pair: the rule is about what the path
        // names.
        let s = scene();
        let h = handshake(&handshake_bytes(&s.b));
        let real = file(&s.project.as_path().join("ordinary.lua"), b"return 1\n");
        let ok = check(&h, HEADERS, &real).expect("an ordinary file is admitted");
        assert_eq!(ok.size(), 9, "the size is what the stat said");
    }

    #[test]
    fn the_oversize_refusal_offers_the_two_ways_out() {
        let s = scene();
        let h = handshake(&with(&handshake_bytes(&s.b), "max_request_bytes", "1024"));
        let real = sized(&s, "too-big.lua", 2_000);
        let line = check(&h, HEADERS, &real)
            .expect_err("over the ceiling")
            .to_string();
        assert!(line.contains("split the file"), "{line}");
        assert!(line.contains("dofile"), "{line}");
    }

    // ---- before the file is opened ----------------------------------------
    //
    // Each fixture is held open with nothing shared, so this process cannot
    // read it. A check that read first and refused afterwards cannot pass
    // here: it would fail on the read. The stat still answers under such a
    // hold, so these are specific to reading rather than reddening anything
    // that merely touches the file.

    /// The bytes every held fixture carries, and the token no refusal may
    /// repeat back.
    const SECRET: &[u8] = b"local password = 'hunter2'\n";

    #[test]
    fn a_held_file_over_the_ceiling_is_refused_without_being_opened() {
        // A file too big for one request is refused on the stat. A read
        // placed ahead of it would buffer the file for a request that cannot
        // be sent, and here would fail on the hold instead.
        let s = scene();
        let h = handshake(&with(&handshake_bytes(&s.b), "max_request_bytes", "1024"));
        let path = s.outside.as_path().join("secret.lua");
        let real = file(&path, &SECRET.repeat(64));
        let hold = held(&path);
        let err = check(&h, HEADERS, &real).expect_err("refused");
        drop(hold);
        assert!(matches!(err.kind, Refusal::Oversize { .. }), "{err}");
    }

    #[test]
    fn no_refusal_carries_a_byte_of_the_file_or_an_open_error() {
        // A refusal that leaked the first line would hand back what it
        // declined to send; one that leaked the open error would be an
        // admission that the file was opened. The fixture is the vault's own
        // path, so the most sensitive-looking place is the one proved not to
        // leak.
        let s = scene();
        let h = handshake(&with(&handshake_bytes(&s.b), "max_request_bytes", "1024"));
        let path = s.writedir.as_path().join("Config").join("network.vault");
        let real = file(&path, &SECRET.repeat(64));
        let hold = held(&path);
        let err = check(&h, HEADERS, &real).expect_err("refused");
        drop(hold);
        assert!(matches!(err.kind, Refusal::Oversize { .. }), "{err}");
        let line = err.to_string();
        assert!(
            !line.contains("password") && !line.contains("hunter2"),
            "the refusal repeats the file back: {line}"
        );
        assert!(
            !line.contains("another process") && !line.contains("os error"),
            "the refusal admits the file was opened: {line}"
        );
    }
}
