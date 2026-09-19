//! A path the client will compare: resolved against the filesystem first,
//! then tested for containment at a segment boundary.
//!
//! Everything the client refuses to write to, or read from, is decided by
//! asking whether a path lies under a root — the install, the write
//! directory, the `Logs` tree inside it. A textual answer to that question is
//! wrong on Windows in three ways at once, and each of them has let a path
//! through somewhere before: `C:\PROGRA~1\...` is the install spelled short,
//! a junction inside a permitted directory is the install wearing a
//! permitted name, and a prefix test on bytes puts `...\LogsX` under
//! `...\Logs`. So the two halves here are separate on purpose: [`resolve`]
//! turns a path into what the filesystem says it really is, and only a
//! [`Real`] has a [`contains`](Real::contains), which compares at a segment
//! boundary and nowhere else.
//!
//! This mirrors the executor's own containment, which folds case, collapses
//! `.` and `..`, and requires the same boundary — but which cannot resolve
//! anything, because nothing in a DCS Lua state expands a short name or
//! follows a junction. That is why the resolving is the client's job: the
//! executor trusts the paths the handshake names, and this is the side that
//! earns that trust. What the executor refuses, this refuses too.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, MAIN_SEPARATOR_STR, Path, PathBuf, Prefix};

/// Why a path could not be resolved, and which path it was about. `Display`
/// is `<path>: <reason>`, the shape the rest of the client's refusals take,
/// so one line reads the same wherever it was raised.
#[derive(Debug)]
pub struct PathError {
    /// The path as it was handed in, not as far as resolution got: the
    /// caller recognises what it passed.
    pub path: PathBuf,
    pub kind: PathErrorKind,
}

#[derive(Debug)]
pub enum PathErrorKind {
    /// Not absolute: no drive, or a drive with nothing anchoring it. Such a
    /// path resolves against something this process happens to be sitting
    /// on — its current directory, or the current directory of whichever
    /// drive was named — and a containment check on an accident is not a
    /// check. `\Users\...` is refused for the same reason `..\x` is: the
    /// drive it lands on is not written down anywhere.
    Relative,
    /// No ancestor of it exists, so the filesystem has nothing to say about
    /// what it would really be. The error is the one from the last
    /// candidate tried, which is the anchor itself.
    Unresolvable(io::Error),
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.path.display();
        match &self.kind {
            PathErrorKind::Relative => write!(
                f,
                "{path}: is relative, and would resolve against wherever this process is sitting"
            ),
            PathErrorKind::Unresolvable(source) => write!(f, "{path}: {source}"),
        }
    }
}

impl std::error::Error for PathError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            PathErrorKind::Relative => None,
            PathErrorKind::Unresolvable(source) => Some(source),
        }
    }
}

/// A path the filesystem has been asked about: short names expanded,
/// junctions and symlinks followed, `.` and `..` gone, and no `\\?\` on the
/// front. Only [`resolve`] makes one, which is the whole point — a
/// containment test that cannot be handed an unresolved path cannot be
/// fooled by a spelling.
///
/// Equality is the bytes, not a fold: two `Real`s differing only in the case
/// of a tail that does not exist yet are not equal, while
/// [`contains`](Real::contains) would still put each inside the other. The
/// comparison that guards anything is `contains`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Real(PathBuf);

impl Real {
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }

    /// Whether `other` is this path or lies under it, at a segment
    /// boundary. Case is folded across ASCII and no further: that is what
    /// the executor's own check folds, and a header value carrying a byte
    /// past ASCII stops its load long before either side compares
    /// anything. A root already ending in a separator — a drive, which
    /// holds everything on it — is not given a second one, which would
    /// spell a prefix no path has.
    ///
    /// The fold is over the encoded bytes rather than a lossy string, so
    /// two paths that are not the same path cannot become one by way of a
    /// replacement character.
    pub fn contains(&self, other: &Real) -> bool {
        let root = folded(&self.0);
        let path = folded(&other.0);
        if path == root {
            return true;
        }
        let mut boundary = root;
        if boundary.last() != Some(&SEPARATOR) {
            boundary.push(SEPARATOR);
        }
        path.starts_with(&boundary[..])
    }
}

impl fmt::Display for Real {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.display().fmt(f)
    }
}

impl AsRef<Path> for Real {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

const SEPARATOR: u8 = b'\\';

fn folded(path: &Path) -> Vec<u8> {
    path.as_os_str().as_encoded_bytes().to_ascii_lowercase()
}

/// What `path` really is, or why the filesystem could not say.
///
/// A path names something that does not exist yet — a reply about to be
/// written, an output directory about to be made — far more often than not,
/// and `fs::canonicalize` answers only about something that does. So the
/// nearest existing ancestor is resolved and the rest of the path is put
/// back on: the part that exists is what a short name or a junction could
/// have been hiding in, and the part that does not cannot hide anything,
/// because there is nothing there to hide behind.
///
/// `.` and `..` are collapsed before any of that, which is what Windows
/// itself does with them — it reads `..` against the spelling, not against
/// where a junction in the spelling points. `..` never eats the anchor:
/// `C:\..\Program Files` is `C:\Program Files` there and here both, and a
/// resolver that let it climb past the drive would hand the install check a
/// path that walks out of its own root.
pub fn resolve(path: &Path) -> Result<Real, PathError> {
    if !path.is_absolute() {
        return Err(PathError {
            path: path.to_owned(),
            kind: PathErrorKind::Relative,
        });
    }
    let mut head = collapse(path);
    let mut tail: Vec<OsString> = Vec::new();
    loop {
        match fs::canonicalize(&head) {
            Ok(real) => {
                let mut out = plain(&real);
                for name in tail.iter().rev() {
                    out.push(name);
                }
                return Ok(Real(out));
            }
            Err(source) => {
                let Some(name) = head.file_name().map(ToOwned::to_owned) else {
                    // The anchor itself: there is nothing left to climb to.
                    return Err(PathError {
                        path: path.to_owned(),
                        kind: PathErrorKind::Unresolvable(source),
                    });
                };
                tail.push(name);
                head.pop();
            }
        }
    }
}

/// `.` dropped and `..` resolved against the segment before it, with the
/// anchor — the drive and its root — never popped.
fn collapse(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    let mut depth = 0usize;
    for part in path.components() {
        match part {
            Component::Prefix(_) | Component::RootDir => out.push(part.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if depth > 0 {
                    out.pop();
                    depth -= 1;
                }
            }
            Component::Normal(name) => {
                out.push(name);
                depth += 1;
            }
        }
    }
    out
}

/// A canonical path without its `\\?\`. The prefix is how the resolved path
/// comes back and it is a real part of the path, but it is not the spelling
/// anything else in this project uses: the executor's paths come out of
/// `lfs` without one, the handshake carries none, and a refusal quoting one
/// would name a path the user does not recognise as theirs. A verbatim
/// prefix that is not a drive or a UNC share — a volume GUID — is left as
/// it is, because dropping it would name a different path rather than the
/// same one spelled plainly.
fn plain(path: &Path) -> PathBuf {
    let mut parts = path.components();
    let Some(Component::Prefix(prefix)) = parts.next() else {
        return path.to_owned();
    };
    let mut out = match prefix.kind() {
        Prefix::VerbatimDisk(letter) => {
            let mut anchor = PathBuf::from(format!("{}:", char::from(letter)));
            anchor.push(MAIN_SEPARATOR_STR);
            anchor
        }
        Prefix::VerbatimUNC(server, share) => {
            let mut anchor = OsString::from(r"\\");
            anchor.push(server);
            anchor.push(r"\");
            anchor.push(share);
            PathBuf::from(anchor)
        }
        _ => return path.to_owned(),
    };
    for part in parts {
        if let Component::Normal(name) = part {
            out.push(name);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Sandbox, junction, short_name};

    use std::fs;

    /// The sandbox, resolved: the host's temp directory is itself often
    /// spelled short, so every expectation here is against what the
    /// filesystem says the box is, never against the path that made it.
    fn box_root(b: &Sandbox) -> Real {
        resolve(&b.path).expect("the box resolves")
    }

    /// The drive a resolved path is on, spelled `C:\`: the root that holds
    /// everything on the volume, and the anchor `..` may not climb past.
    fn anchor(real: &Real) -> PathBuf {
        let drive = real
            .as_path()
            .components()
            .next()
            .expect("a resolved path has an anchor");
        let mut out = PathBuf::from(drive.as_os_str());
        out.push(MAIN_SEPARATOR_STR);
        out
    }

    /// A drive letter no volume is mounted on, for the one path that must
    /// not resolve at any depth. Loud rather than skipped when every letter
    /// is taken: a control that quietly does not run is worse than none.
    fn free_drive() -> PathBuf {
        for letter in ('D'..='Z').rev() {
            let root = PathBuf::from(format!("{letter}:\\"));
            if fs::metadata(&root).is_err() {
                return root;
            }
        }
        panic!("every drive letter from D to Z is mounted, so nothing here is unresolvable");
    }

    // ---- what resolution refuses ------------------------------------------

    #[test]
    fn a_relative_path_is_refused_by_shape() {
        for spelling in ["logs\\x", ".\\x", "..\\x"] {
            let err = resolve(Path::new(spelling)).expect_err("relative is refused");
            assert!(matches!(err.kind, PathErrorKind::Relative), "{spelling}");
            assert_eq!(
                err.path,
                PathBuf::from(spelling),
                "the refusal names it back"
            );
            assert!(
                err.to_string().contains(spelling),
                "and the line carries it: {err}"
            );
        }
    }

    #[test]
    fn a_path_anchored_to_no_drive_is_relative_here() {
        // `C:x` is the current directory of drive C, and `\x` is the root
        // of whichever drive this process is on. Both name a different file
        // depending on where the client was started.
        for spelling in ["C:x", "\\x", "\\Users\\public"] {
            let err = resolve(Path::new(spelling)).expect_err("refused");
            assert!(matches!(err.kind, PathErrorKind::Relative), "{spelling}");
        }
    }

    #[test]
    fn a_path_no_part_of_which_resolves_is_refused() {
        let path = free_drive().join("nowhere").join("x.res");
        let err = resolve(&path).expect_err("nothing to resolve against");
        assert!(matches!(err.kind, PathErrorKind::Unresolvable(_)), "{err}");
        assert_eq!(err.path, path, "the refusal names the path handed in");
    }

    // ---- what resolution answers ------------------------------------------

    #[test]
    fn resolution_answers_without_the_verbatim_prefix() {
        let b = Sandbox::new();
        let real = box_root(&b);
        let text = real.to_string();
        assert!(!text.starts_with(r"\\?\"), "no verbatim prefix: {text}");
        assert!(
            text.as_bytes()[1..].starts_with(b":\\"),
            "a drive and its root: {text}"
        );
        assert!(real.as_path().is_dir(), "and it still names the box");
    }

    #[test]
    fn resolution_expands_an_8_3_short_name() {
        let b = Sandbox::new();
        let long = b.join("Saved Games Like This");
        fs::create_dir(&long).expect("the long-named directory");
        let short = short_name(&long);
        assert_eq!(
            resolve(&short).expect("the short spelling resolves"),
            resolve(&long).expect("the long spelling resolves"),
            "one directory, one answer"
        );
    }

    #[test]
    fn resolution_follows_a_junction() {
        let b = Sandbox::new();
        let target = b.join("elsewhere");
        fs::create_dir(&target).expect("the target");
        let link = b.join("link");
        junction(&link, &target);
        assert_eq!(
            resolve(&link).expect("the junction resolves"),
            resolve(&target).expect("the target resolves"),
            "the junction is not a place of its own"
        );
    }

    #[test]
    fn resolution_puts_back_a_tail_that_does_not_exist_yet() {
        let b = Sandbox::new();
        let long = b.join("Saved Games Like This");
        fs::create_dir(&long).expect("the long-named directory");
        let short = short_name(&long);
        let wanted = short
            .join("Logs")
            .join("DcsEval")
            .join("0000000001-abcd.req");
        let real = resolve(&wanted).expect("the unmade path resolves");
        assert!(!real.as_path().exists(), "nothing was created");
        assert_eq!(
            real.as_path(),
            resolve(&long)
                .expect("the existing part")
                .as_path()
                .join("Logs")
                .join("DcsEval")
                .join("0000000001-abcd.req"),
            "the part that exists is resolved, the part that does not is reattached"
        );
    }

    #[test]
    fn resolution_collapses_dot_and_dot_dot() {
        let b = Sandbox::new();
        let root = box_root(&b);
        fs::create_dir(b.join("Logs")).expect("Logs");
        let wound = b
            .join("Logs")
            .join("..")
            .join(".")
            .join("Logs")
            .join("..")
            .join("Config");
        assert_eq!(
            resolve(&wound).expect("it resolves"),
            resolve(&root.as_path().join("Config")).expect("the plain spelling"),
            "`Logs\\..\\Config` is a path about Config"
        );
    }

    #[test]
    fn dot_dot_never_climbs_past_the_anchor() {
        let b = Sandbox::new();
        let root = box_root(&b);
        let depth = root.as_path().components().count();
        let mut climbing = root.as_path().to_owned();
        for _ in 0..depth + 4 {
            climbing.push("..");
        }
        climbing.push("Windows");
        let anchored = anchor(&root).join("Windows");
        assert_eq!(
            resolve(&climbing).expect("it resolves"),
            resolve(&anchored).expect("the plain spelling"),
            "the climb stops at the drive, as Windows stops it"
        );
    }

    // ---- containment ------------------------------------------------------

    #[test]
    fn a_root_holds_itself() {
        let b = Sandbox::new();
        let root = box_root(&b);
        assert!(root.contains(&root), "a root is inside itself");
    }

    #[test]
    fn a_drive_root_holds_everything_on_it() {
        // The one root already ending in the boundary. A check that appends
        // a second separator spells `C:\\`, which no path starts with, and
        // this goes red.
        let b = Sandbox::new();
        let inside = box_root(&b);
        let root = resolve(&anchor(&inside)).expect("the drive root");
        assert!(root.contains(&inside), "the drive holds the box");
    }

    #[test]
    fn containment_stops_at_a_segment_boundary() {
        let b = Sandbox::new();
        fs::create_dir(b.join("Logs")).expect("Logs");
        fs::create_dir(b.join("LogsX")).expect("a sibling sharing the bytes");
        let logs = resolve(&b.join("Logs")).expect("Logs resolves");
        let sibling = resolve(&b.join("LogsX").join("x.res")).expect("the sibling resolves");
        assert!(
            !logs.contains(&sibling),
            "a byte prefix is not containment: {sibling} is not under {logs}"
        );
        let under = resolve(&b.join("Logs").join("x.res")).expect("under Logs");
        assert!(logs.contains(&under), "and what is under it, is");
    }

    #[test]
    fn containment_folds_ascii_case() {
        let b = Sandbox::new();
        fs::create_dir(b.join("Logs")).expect("Logs");
        let logs = resolve(&b.join("Logs")).expect("Logs resolves");
        // A tail that does not exist keeps the case it was spelled with,
        // which is exactly where an unfolded comparison would go wrong.
        let shouted = resolve(&b.join("Logs").join("DCSEVAL").join("X.RES")).expect("resolves");
        assert!(logs.contains(&shouted), "{shouted} is under {logs}");
    }

    #[test]
    fn containment_sees_through_a_laundering_junction() {
        // The attack: a link inside a permitted directory whose target is
        // outside it. Textually the path is under the permitted root; the
        // file it names is not.
        let b = Sandbox::new();
        let permitted = b.join("Logs");
        fs::create_dir(&permitted).expect("the permitted root");
        let outside = b.join("Config");
        fs::create_dir(&outside).expect("the place it really goes");
        let link = permitted.join("link");
        junction(&link, &outside);
        let root = resolve(&permitted).expect("the root resolves");
        let laundered = resolve(&link.join("autoexec.cfg")).expect("the path resolves");
        assert!(
            !root.contains(&laundered),
            "the junction does not launder {laundered} into {root}"
        );
        assert!(
            resolve(&outside)
                .expect("the target resolves")
                .contains(&laundered),
            "it is under what the junction points at"
        );
    }

    #[test]
    fn containment_sees_through_an_8_3_short_spelling() {
        // The other direction: a path that textually misses a forbidden
        // root because the root is spelled short. Resolution puts it back
        // inside, which is what a guard on the install needs.
        let b = Sandbox::new();
        let forbidden = b.join("Program Files Like This");
        fs::create_dir(&forbidden).expect("the forbidden root");
        let short = short_name(&forbidden);
        let root = resolve(&forbidden).expect("the root resolves");
        let sneaking = short.join("bin").join("x.exe");
        assert!(
            !folded(&sneaking).starts_with(&folded(root.as_path())[..]),
            "the short spelling misses the root textually, which is the attack"
        );
        let real = resolve(&sneaking).expect("it resolves");
        assert!(root.contains(&real), "{real} is inside {root} after all");
    }
}
