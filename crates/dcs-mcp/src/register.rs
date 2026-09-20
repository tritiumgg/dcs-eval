//! The server's own data directory: what it wrote down before it moved
//! anything, and the copies it moved aside.
//!
//! Two things live here and they are one module because neither is useful
//! without the other. `install-register.tsv` is the record — a row appended
//! *before* a file is touched and marked once the touch succeeded — and
//! `parked\<utc>\` is the store, holding a displaced file under its path
//! relative to the write directory it came out of. A row with no park names
//! a move nothing can undo; a park with no row is a directory nobody can
//! read back.
//!
//! The ordering is the whole design. A row written after the move looks
//! identical in the finished state and differs only in what survives a
//! crash between the two: written first, a killed run leaves a `pending`
//! row naming exactly the file that was in flight, and whoever comes next
//! can see what happened. Written second, it leaves nothing. So the row
//! goes in first and is marked after, and the marking is a seek-and-
//! overwrite of a fixed-width field rather than a rewrite of the file:
//! nothing here is ever rewritten, truncated, reordered or removed.
//!
//! **Nothing under this directory is ever deleted, and nothing under it is
//! under either DCS tree.** A store that pruned itself would throw away the
//! one copy of a file somebody wants back, and a store inside `Saved Games`
//! would be swept up by the very uninstall it exists to survive. The second
//! is enforced: every way of naming the root ends in [`DataDir::at`], which
//! refuses one under any tree it is handed.

use std::fmt;
use std::fs;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use dcs_eval::paths::{self, PathError, Real};
use dcs_eval::sha256;

use crate::sys;

/// The width the status column is padded to, in bytes.
///
/// The field is fixed-width because it is overwritten in place, and it is
/// sized against every word this build will ever write there rather than
/// against the two the register starts out with — `uninstalled` arrives
/// with the uninstaller, and a field widened later would not fit the
/// registers already on disk.
const STATUS_WIDTH: usize = 11;

/// Every word that may stand in the status column, so the width above can
/// be proved against the whole set at compile time.
const STATUS_WORDS: [&str; 3] = ["pending", "installed", "uninstalled"];

const _: () = {
    let mut i = 0;
    while i < STATUS_WORDS.len() {
        assert!(
            STATUS_WORDS[i].len() <= STATUS_WIDTH,
            "a status word does not fit the column it is overwritten into"
        );
        i += 1;
    }
};

/// The status a row carries between being written and being marked.
const PENDING: &str = "pending";

/// How many directories one second's worth of parks may take before the
/// minting gives up. Far past anything an installer does in a second, and
/// a bound rather than a loop so a directory nothing can create does not
/// spin forever.
const PARKS_PER_SECOND: u32 = 1000;

/// Where this build keeps what is its own: the register and the park store.
///
/// Constructing one creates nothing. The directory is made the first time
/// something is written, so a verb that only reads — and there is one —
/// can hold a `DataDir` without leaving a directory behind on a machine
/// where nothing was ever installed.
#[derive(Clone, Debug)]
pub struct DataDir {
    root: Real,
}

impl DataDir {
    /// `<Local AppData>\dcs-mcp`, resolved, and refused if it lies under
    /// any of `trees`.
    ///
    /// The known folder is nobody's idea of a place to keep DCS, so the
    /// refusal is all but unreachable here — it takes a profile relocated
    /// on top of `Saved Games` to fire. It goes through [`DataDir::at`]
    /// anyway because an invariant held by only the override path is one
    /// the product does not have, and the shape of a machine is not this
    /// module's to predict.
    pub fn known(trees: &[&Real]) -> Result<Self, RegisterError> {
        let local = sys::local_app_data().map_err(RegisterError::NoKnownFolder)?;
        Self::at(&local.join("dcs-mcp"), trees)
    }

    /// A root the caller supplies — an override, or a fixture — refused if
    /// it lies under any of `trees`.
    ///
    /// The trees are passed in rather than looked up, because the caller is
    /// the one that already knows which DCS directories are in play, and
    /// because a test can then hand this a fixture pair. The comparison is
    /// [`Real::contains`], so a junction pointing into `Saved Games` is
    /// refused however it was spelled.
    pub fn at(path: &Path, trees: &[&Real]) -> Result<Self, RegisterError> {
        let root = paths::resolve(path)?;
        for tree in trees {
            if tree.contains(&root) {
                return Err(RegisterError::InsideDcsTree {
                    data: root,
                    tree: (*tree).clone(),
                });
            }
        }
        Ok(Self { root })
    }

    /// The directory itself.
    pub fn path(&self) -> &Real {
        &self.root
    }

    /// The register file. One per data directory, appended to forever.
    pub fn register_path(&self) -> PathBuf {
        self.root.as_path().join("install-register.tsv")
    }

    /// The directory each park mints a directory of its own inside.
    pub fn parked_root(&self) -> PathBuf {
        self.root.as_path().join("parked")
    }

    /// The directory a captured reply is kept in, one file per id.
    ///
    /// Beside the park store rather than inside it: a parked file is somebody
    /// else's, moved out of the way and owed back, and a captured reply is
    /// this build's own copy of something it was given.
    pub fn replies_root(&self) -> PathBuf {
        self.root.as_path().join("replies")
    }

    /// The register, to write one action's rows through.
    pub fn register(&self, action: Action) -> Register<'_> {
        Register { data: self, action }
    }

    /// Every row the register holds, oldest first. A register that has
    /// never been written is no rows rather than a failure: nothing has
    /// been installed yet, which is a fact and not a problem.
    ///
    /// A row is one line ending in a newline, so anything after the last
    /// newline is a write that did not finish — the very crash the writing
    /// order exists to survive. It is dropped rather than refused: a run
    /// killed mid-append would otherwise take every intact row before it
    /// down with the one that was in flight, which is the opposite of what
    /// a record is for. A line that is whole and still holds no five
    /// columns is corruption of another kind and is refused.
    pub fn rows(&self) -> Result<Vec<Row>, RegisterError> {
        let path = self.register_path();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(why) if why.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(why) => return Err(RegisterError::Disk { path, why }),
        };
        let whole = match text.rfind('\n') {
            Some(last) => &text[..=last],
            None => "",
        };
        whole
            .lines()
            .filter(|line| !line.is_empty())
            .map(Row::parse)
            .collect()
    }
}

/// What a row is about. The word is the register's second column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Install,
    Uninstall,
}

impl Action {
    /// The word written when the row goes in.
    pub fn word(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Uninstall => "uninstall",
        }
    }

    /// The word the status field is marked with once the action succeeded.
    pub fn done_word(self) -> &'static str {
        match self {
            Self::Install => "installed",
            Self::Uninstall => "uninstalled",
        }
    }
}

/// One line of the register, parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub stamp: String,
    pub action: String,
    pub path: PathBuf,
    pub sha256: String,
    /// Trimmed of the padding the column is written with.
    pub status: String,
}

impl Row {
    /// The five columns, taken from both ends inwards.
    ///
    /// Four of them — the stamp and the action from the left, the digest
    /// and the status from the right — are fixed shapes that hold no tab,
    /// so whatever is left in the middle is the path, whatever it holds.
    /// That is why no refusal guards a path that "would break the
    /// columns": Windows forbids every control character in a path
    /// component, a tab included, so the case cannot arise — and were it
    /// ever to, the split still lands the path in the path column. The one
    /// genuinely lossy step is upstream, where a path Windows will not
    /// spell back as UTF-8 prints with replacement characters; the park
    /// store, not this string, is what makes such a file recoverable.
    fn parse(line: &str) -> Result<Self, RegisterError> {
        let malformed = || RegisterError::MalformedRow {
            line: line.to_owned(),
        };
        let (stamp, rest) = line.split_once('\t').ok_or_else(malformed)?;
        let (action, rest) = rest.split_once('\t').ok_or_else(malformed)?;
        let (rest, status) = rest.rsplit_once('\t').ok_or_else(malformed)?;
        let (path, sha256) = rest.rsplit_once('\t').ok_or_else(malformed)?;
        Ok(Self {
            stamp: stamp.to_owned(),
            action: action.to_owned(),
            path: PathBuf::from(path),
            sha256: sha256.to_owned(),
            status: status.trim_end().to_owned(),
        })
    }
}

/// The register, bound to the action whose rows are going in.
pub struct Register<'a> {
    data: &'a DataDir,
    action: Action,
}

impl Register<'_> {
    /// Write the row, do the thing, mark the row.
    ///
    /// The ordering is structural rather than remembered: a caller cannot
    /// move a file "and also" record it, because the recording is what
    /// calls the move. `moving` returns whatever the caller wants back —
    /// the park store returns the directory it minted — and an error from
    /// it is returned as it stands, with the row left `pending`, which is
    /// the record of a move that did not complete.
    pub fn around<T>(
        &self,
        now: SystemTime,
        file: &Real,
        sha256: &str,
        moving: impl FnOnce() -> Result<T, RegisterError>,
    ) -> Result<T, RegisterError> {
        let at = self.append(now, file, sha256)?;
        let done = moving()?;
        self.mark(at, self.action.done_word())?;
        Ok(done)
    }

    /// Append one `pending` row and answer where its status field begins.
    fn append(&self, now: SystemTime, file: &Real, sha256: &str) -> Result<u64, RegisterError> {
        let root = self.data.path().as_path();
        fs::create_dir_all(root).map_err(|why| RegisterError::Disk {
            path: root.to_owned(),
            why,
        })?;
        let path = self.data.register_path();
        let head = format!("{}\t{}\t{file}\t{sha256}\t", stamp(now), self.action.word());
        let line = format!("{head}{:<STATUS_WIDTH$}\n", PENDING);
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|why| RegisterError::Disk {
                path: path.clone(),
                why,
            })?;
        let at = f
            .seek(SeekFrom::End(0))
            .map_err(|why| RegisterError::Disk {
                path: path.clone(),
                why,
            })?;
        f.write_all(line.as_bytes())
            .map_err(|why| RegisterError::Disk { path, why })?;
        Ok(at + head.len() as u64)
    }

    /// Overwrite the status field at `at`, and nothing else.
    fn mark(&self, at: u64, word: &str) -> Result<(), RegisterError> {
        let path = self.data.register_path();
        let mut f = fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .map_err(|why| RegisterError::Disk {
                path: path.clone(),
                why,
            })?;
        f.seek(SeekFrom::Start(at))
            .map_err(|why| RegisterError::Disk {
                path: path.clone(),
                why,
            })?;
        f.write_all(format!("{word:<STATUS_WIDTH$}").as_bytes())
            .map_err(|why| RegisterError::Disk { path, why })
    }

    /// A directory of its own under the park store, made, together with
    /// the place inside it that `relative` names.
    ///
    /// Answers the minted directory and the destination, in that order:
    /// the first is what a caller hands back as the record of the park,
    /// the second is where the bytes go.
    fn minted(
        &self,
        now: SystemTime,
        relative: &Path,
    ) -> Result<(PathBuf, PathBuf), RegisterError> {
        let parked = self.data.parked_root();
        fs::create_dir_all(&parked).map_err(|why| RegisterError::Disk {
            path: parked.clone(),
            why,
        })?;
        let dir = mint(&parked, &stamp(now), PARKS_PER_SECOND)?;
        let destination = dir.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|why| RegisterError::Disk {
                path: parent.to_owned(),
                why,
            })?;
        }
        Ok((dir, destination))
    }

    /// Move `file` out of `variant` into a directory of its own under the
    /// park store, and answer that directory.
    ///
    /// Everything that touches the filesystem happens inside the closure,
    /// so the row precedes all of it, and the destination is decided where
    /// the move is rather than handed down from a caller that guessed.
    /// The path under the park directory is `file`'s own path relative to
    /// the variant, which is what makes the copy recoverable without a
    /// second record saying where it came from.
    pub fn park(
        &self,
        now: SystemTime,
        variant: &Real,
        file: &Path,
    ) -> Result<PathBuf, RegisterError> {
        let (source, relative) = relative_to(variant, file)?;
        let bytes = fs::read(source.as_path()).map_err(|why| RegisterError::Disk {
            path: source.as_path().to_owned(),
            why,
        })?;
        let sha = sha256::hex(&sha256::digest(&bytes));
        self.around(now, &source, &sha, || {
            let (dir, destination) = self.minted(now, &relative)?;
            move_file(source.as_path(), &destination)?;
            Ok(dir)
        })
    }

    /// Copy `file` aside into a directory of its own under the park store,
    /// leaving the original where it is, and answer that directory.
    ///
    /// It copies rather than moves because what it parks is a file that is
    /// about to be *edited* in place, which has to stay where it is. And
    /// it writes no row of its own: it is called from inside a row the
    /// caller already opened, and a second row for one action would say
    /// two things happened where one did.
    pub fn copy_aside(
        &self,
        now: SystemTime,
        variant: &Real,
        file: &Path,
    ) -> Result<PathBuf, RegisterError> {
        let (source, relative) = relative_to(variant, file)?;
        let (dir, destination) = self.minted(now, &relative)?;
        fs::copy(source.as_path(), &destination).map_err(|why| RegisterError::Disk {
            path: destination.clone(),
            why,
        })?;
        Ok(dir)
    }

    /// Move the file at `parked` back out of the store to `destination`,
    /// inside a row of its own.
    ///
    /// It is a move, and that is the half of it worth saying. A copy would
    /// leave the store holding a file it has already given back, and the
    /// next run to look would hand that copy out a second time — over
    /// whatever is at the destination by then, which by that point is the
    /// very file this one restored. The minted directory itself is left
    /// standing, empty: nothing under the data directory is ever deleted,
    /// and an empty directory is the record that something came out of it.
    ///
    /// The digest on the row is of the bytes going back, so the row says
    /// what was put there rather than what was displaced — which is the
    /// same rule the install rows follow, read from the other end.
    pub fn restore(
        &self,
        now: SystemTime,
        parked: &Path,
        destination: &Real,
    ) -> Result<(), RegisterError> {
        let bytes = fs::read(parked).map_err(|why| RegisterError::Disk {
            path: parked.to_owned(),
            why,
        })?;
        let sha = sha256::hex(&sha256::digest(&bytes));
        self.around(now, destination, &sha, || {
            if let Some(parent) = destination.as_path().parent() {
                fs::create_dir_all(parent).map_err(|why| RegisterError::Disk {
                    path: parent.to_owned(),
                    why,
                })?;
            }
            move_file(parked, destination.as_path())
        })
    }
}

/// `file`, resolved, with its path relative to `variant` — or a refusal
/// naming both, where it does not lie under the variant at all.
///
/// The relative half is the path a parked copy takes under the directory
/// minted for it, which is what makes the copy recoverable without a
/// second record saying where it came from.
fn relative_to(variant: &Real, file: &Path) -> Result<(Real, PathBuf), RegisterError> {
    let source = paths::resolve(file)?;
    let astray = || RegisterError::NotUnderVariant {
        file: source.clone(),
        variant: variant.clone(),
    };
    // The rule is `contains`, which folds case and compares at a
    // segment boundary; `strip_prefix` is the byte-exact half and can
    // only disagree with it on a path the resolver did not produce.
    // Both refuse the same way, so neither can let one through.
    if !variant.contains(&source) {
        return Err(astray());
    }
    let relative = source
        .as_path()
        .strip_prefix(variant.as_path())
        .map_err(|_| astray())?
        .to_owned();
    Ok((source, relative))
}

/// A directory named for `stamp` that did not exist a moment ago.
///
/// Two parks in the same second are not a clock problem and are not solved
/// with a finer clock: whatever the resolution, two can share it. The
/// filesystem arbitrates instead — `create_dir` fails when the name is
/// taken, and the next suffix is tried — so the second park lands in a
/// directory of its own without either park having to know about the
/// other.
///
/// `limit` is how many names that second may take, handed in rather than
/// read off the constant so that giving up is reachable in a test without
/// minting a thousand directories to get there.
fn mint(parked: &Path, stamp: &str, limit: u32) -> Result<PathBuf, RegisterError> {
    for n in 1..=limit {
        let dir = parked.join(if n == 1 {
            stamp.to_owned()
        } else {
            format!("{stamp}-{n}")
        });
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(why) if why.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(why) => return Err(RegisterError::Disk { path: dir, why }),
        }
    }
    Err(RegisterError::Crowded {
        stamp: stamp.to_owned(),
        limit,
    })
}

/// `from` moved to `to`, across volumes if it has to be.
///
/// `rename` is the whole of it when both ends are on one volume, and a
/// user whose `Saved Games` sits on another drive from their profile is
/// the case it is not: Windows refuses a rename across volumes, so the
/// bytes are copied and the original removed. The remove is what makes
/// this a move and not a copy, and it is the last thing done.
fn move_file(from: &Path, to: &Path) -> Result<(), RegisterError> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to).map_err(|why| RegisterError::Disk {
        path: to.to_owned(),
        why,
    })?;
    fs::remove_file(from).map_err(|why| RegisterError::Disk {
        path: from.to_owned(),
        why,
    })
}

/// `now` as `YYYYMMDDTHHMMSSZ`, UTC, to the second.
///
/// The basic ISO form, without the separators, because the same string
/// names a directory and Windows takes no colon in a name. A time before
/// the epoch is not a thing this build writes, and one handed in anyway
/// stamps as the epoch rather than failing a park over a clock.
fn stamp(now: SystemTime) -> String {
    let secs = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let (days, rest) = ((secs / 86_400) as i64, secs % 86_400);
    let (y, m, d) = civil_from_days(days);
    let (h, mi, s) = (rest / 3_600, (rest % 3_600) / 60, rest % 60);
    format!("{y:04}{m:02}{d:02}T{h:02}{mi:02}{s:02}Z")
}

/// The year, month and day `days` after 1970-01-01, in the proleptic
/// Gregorian calendar.
///
/// Howard Hinnant's `civil_from_days`, which counts from an era beginning
/// on 1 March so that the leap day is the last day of a year and the
/// month-length table becomes a single division. Written out rather than
/// taken from a crate: this crate's dependencies are the MCP SDK and its
/// runtime, and a date library for one format string would be a third.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Shift the epoch to 0000-03-01, the start of an era.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    // Day of era, in [0, 146096]: 400 years is exactly 146097 days.
    let doe = z - era * 146_097;
    // Year of era, in [0, 399], by removing the leap days already passed.
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    // Day of that year, counting from 1 March.
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    // The March-based month, in [0, 11]: month lengths repeat on a
    // five-month cycle from March, which this division walks.
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Why nothing was written down, or why what was written down could not be
/// read back. `Display` is the project's `<path>: <reason>`.
#[derive(Debug)]
pub enum RegisterError {
    NoKnownFolder(io::Error),
    Path(PathError),
    InsideDcsTree { data: Real, tree: Real },
    NotUnderVariant { file: Real, variant: Real },
    Disk { path: PathBuf, why: io::Error },
    MalformedRow { line: String },
    Crowded { stamp: String, limit: u32 },
}

impl From<PathError> for RegisterError {
    fn from(why: PathError) -> Self {
        Self::Path(why)
    }
}

impl fmt::Display for RegisterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoKnownFolder(why) => write!(f, "Local AppData: {why}"),
            Self::Path(why) => why.fmt(f),
            Self::InsideDcsTree { data, tree } => write!(
                f,
                "{data}: lies under the DCS directory at {tree}, and what is parked there has to \
                 outlive it"
            ),
            Self::NotUnderVariant { file, variant } => write!(
                f,
                "{file}: is not under {variant}, so there is no path relative to it to park it by"
            ),
            Self::Disk { path, why } => write!(f, "{}: {why}", path.display()),
            Self::MalformedRow { line } => {
                write!(f, "the register holds a row of no five columns: {line}")
            }
            Self::Crowded { stamp, limit } => write!(
                f,
                "{stamp}: that second already holds {limit} parked directories"
            ),
        }
    }
}

impl std::error::Error for RegisterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoKnownFolder(why) | Self::Disk { why, .. } => Some(why),
            Self::Path(why) => Some(why),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    use crate::testing::Sandbox;

    /// Every expectation compares resolved paths, never the spelling that
    /// made them: the host's temp directory is usually spelled short.
    fn real(path: &Path) -> Real {
        paths::resolve(path).expect("the path resolves")
    }

    /// A file with known bytes, and whatever directories it needs.
    fn put(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().expect("a file has a parent"))
            .expect("the directories are made");
        fs::write(path, bytes).expect("the file is written");
    }

    /// One instant, used wherever the test does not care which.
    fn an_instant() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_760_000_000)
    }

    /// A sandbox laid out the way the installer meets one: a `Saved Games`
    /// holding one variant, and the data directory **beside** it rather
    /// than inside. The two being siblings is the arrangement the refusal
    /// below exists to hold, and it is also what keeps a fixture tree that
    /// something else asserts is clean free of this store's writes.
    fn fixture() -> (Sandbox, Real, DataDir) {
        let b = Sandbox::new();
        let saved = real(&b.dir("saved"));
        let variant = real(&b.dir("saved/DCS.openbeta"));
        let data = DataDir::at(&b.join("data"), &[&saved]).expect("beside is not inside");
        (b, variant, data)
    }

    #[test]
    fn the_row_goes_in_before_the_move_and_is_marked_after() {
        let (b, _variant, data) = fixture();
        let bytes = b"-- an executor\n";
        let hook = b.join("saved/DCS.openbeta/Scripts/Hooks/DcsEvalExecutor.lua");
        put(&hook, bytes);
        let hook = real(&hook);
        let sha = sha256::hex(&sha256::digest(bytes));
        let now = an_instant();

        // The assertions that matter are made from *inside* the closure,
        // at the moment of the move. Read afterwards, a row written before
        // and a row written after look exactly alike.
        let seen = data
            .register(Action::Install)
            .around(now, &hook, &sha, || {
                let rows = data.rows().expect("the register reads");
                assert_eq!(rows.len(), 1, "one row, already there");
                let row = &rows[0];
                assert_eq!(row.stamp, stamp(now));
                assert_eq!(row.action, "install");
                assert_eq!(row.path, hook.as_path());
                assert_eq!(row.sha256, sha);
                assert_eq!(row.status, PENDING, "and not yet marked");
                Ok(fs::read(data.register_path()).expect("the bytes the move saw"))
            })
            .expect("the move succeeds");

        let after = fs::read(data.register_path()).expect("the bytes afterwards");
        let rows = data.rows().expect("the register reads");
        assert_eq!(
            rows.len(),
            1,
            "the register only ever grows, and nothing grew"
        );
        assert_eq!(rows[0].status, "installed");
        // The marking is a seek and an overwrite: every byte outside the
        // status field is the byte the closure saw.
        let mut expected = seen.clone();
        let at = expected.len() - 1 - STATUS_WIDTH;
        expected[at..at + STATUS_WIDTH]
            .copy_from_slice(format!("{:<STATUS_WIDTH$}", "installed").as_bytes());
        assert_eq!(after, expected, "only the status field changed");
    }

    #[test]
    fn a_move_that_fails_leaves_its_row_pending() {
        let (b, _variant, data) = fixture();
        let hook = b.join("saved/DCS.openbeta/Scripts/Hooks/DcsEvalExecutor.lua");
        put(&hook, b"-- an executor\n");
        let hook = real(&hook);
        let err = data
            .register(Action::Install)
            .around(an_instant(), &hook, "0", || {
                Err::<(), _>(RegisterError::Disk {
                    path: hook.as_path().to_owned(),
                    why: io::Error::other("the move did not happen"),
                })
            })
            .expect_err("the closure refused, so the call does");
        assert!(matches!(err, RegisterError::Disk { .. }), "{err}");
        let rows = data.rows().expect("the register reads");
        assert_eq!(
            rows.len(),
            1,
            "the row stays: it is the record of the attempt"
        );
        assert_eq!(rows[0].status, PENDING, "and stays pending");
    }

    #[test]
    fn a_parked_file_is_recoverable_from_its_path_relative_to_the_variant() {
        let (b, variant, data) = fixture();
        let bytes = b"-- somebody else's hook\n";
        let file = b.join("saved/DCS.openbeta/Scripts/Hooks/DcsEvalExecutor.lua");
        put(&file, bytes);
        let resolved = real(&file);

        let dir = data
            .register(Action::Install)
            .park(an_instant(), &variant, &file)
            .expect("the file parks");

        let landed = dir
            .join("Scripts")
            .join("Hooks")
            .join("DcsEvalExecutor.lua");
        assert_eq!(
            fs::read(&landed).expect("the parked copy is there"),
            bytes,
            "under its own path relative to the variant, byte for byte"
        );
        assert!(!file.exists(), "moved, not copied: {}", file.display());

        let rows = data.rows().expect("the register reads");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].path,
            resolved.as_path(),
            "the resolved path, not the spelling"
        );
        assert_eq!(rows[0].sha256, sha256::hex(&sha256::digest(bytes)));
        assert_eq!(rows[0].status, "installed");
    }

    #[test]
    fn a_copy_aside_leaves_the_original_where_it_is() {
        let (b, variant, data) = fixture();
        let bytes = b"-- somebody else's export line\n";
        let file = b.join("saved/DCS.openbeta/Scripts/Export.lua");
        put(&file, bytes);

        let dir = data
            .register(Action::Install)
            .copy_aside(an_instant(), &variant, &file)
            .expect("the file is copied aside");

        assert_eq!(
            fs::read(dir.join("Scripts").join("Export.lua")).expect("the parked copy is there"),
            bytes
        );
        assert_eq!(
            fs::read(&file).expect("and so is the original"),
            bytes,
            "copied, not moved: a file about to be edited stays where it is"
        );
        assert!(
            data.rows().expect("the register reads").is_empty(),
            "and the copy writes no row: the caller's row is the one row"
        );
    }

    #[test]
    fn two_parks_in_one_second_land_in_distinct_directories() {
        let (b, variant, data) = fixture();
        let one = b.join("saved/DCS.openbeta/Scripts/Hooks/DcsEvalExecutor.lua");
        let two = b.join("saved/DCS.openbeta/Scripts/Hooks/DcsApiEval.lua");
        put(&one, b"-- the first\n");
        put(&two, b"-- the second\n");
        // The same instant, handed in twice. Nothing sleeps and nothing
        // reads a clock, so this is the collision every time rather than
        // whenever the machine is fast enough.
        let now = an_instant();
        let register = data.register(Action::Install);
        let first = register.park(now, &variant, &one).expect("the first parks");
        let second = register
            .park(now, &variant, &two)
            .expect("the second parks");

        assert_ne!(first, second, "two parks, two directories");
        for dir in [&first, &second] {
            let name = dir
                .file_name()
                .expect("a minted directory has a name")
                .to_string_lossy()
                .into_owned();
            assert!(
                name.starts_with(&stamp(now)),
                "both are inside the one second: {name}"
            );
        }
        assert_eq!(
            fs::read(
                first
                    .join("Scripts")
                    .join("Hooks")
                    .join("DcsEvalExecutor.lua")
            )
            .expect("the first copy"),
            b"-- the first\n"
        );
        assert_eq!(
            fs::read(second.join("Scripts").join("Hooks").join("DcsApiEval.lua"))
                .expect("the second copy"),
            b"-- the second\n"
        );
    }

    #[test]
    fn a_data_directory_under_a_dcs_tree_is_refused_naming_both() {
        let b = Sandbox::new();
        let saved = real(&b.dir("saved"));
        let inside = b.join("saved/dcs-mcp");
        let err = DataDir::at(&inside, &[&saved])
            .expect_err("what is parked has to outlive what it was parked out of");
        assert!(matches!(err, RegisterError::InsideDcsTree { .. }), "{err}");
        let line = err.to_string();
        assert!(
            line.contains(&saved.to_string()),
            "the line names the tree: {line}"
        );
        assert!(
            line.contains("dcs-mcp"),
            "and the data directory it refused: {line}"
        );
    }

    #[test]
    fn a_file_outside_the_variant_has_no_relative_path_to_park_by() {
        let (b, variant, data) = fixture();
        let stray = b.join("elsewhere/DcsEvalExecutor.lua");
        put(&stray, b"-- not in the variant\n");
        let err = data
            .register(Action::Install)
            .park(an_instant(), &variant, &stray)
            .expect_err("that file is not the variant's");
        assert!(
            matches!(err, RegisterError::NotUnderVariant { .. }),
            "{err}"
        );
        assert!(
            stray.exists(),
            "and a refusal moves nothing: {}",
            stray.display()
        );
    }

    #[test]
    fn a_row_torn_off_by_a_crash_does_not_hide_the_rows_before_it() {
        let (b, _variant, data) = fixture();
        let hook = b.join("saved/DCS.openbeta/Scripts/Hooks/DcsEvalExecutor.lua");
        put(&hook, b"-- an executor\n");
        let hook = real(&hook);
        data.register(Action::Install)
            .around(an_instant(), &hook, "0", || Ok(()))
            .expect("the row goes in");

        // What a run killed part-way through an append leaves behind: a
        // line with no newline on the end of it.
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(data.register_path())
            .expect("the register opens");
        f.write_all(b"20251009T085320Z\tinstall\tC:\\half")
            .expect("half a row is written");
        drop(f);

        let rows = data.rows().expect("the intact rows still read back");
        assert_eq!(rows.len(), 1, "the torn tail is dropped, not the rest");
        assert_eq!(rows[0].status, "installed");

        // A line that is whole and still short of five columns is not a
        // torn write, and is refused rather than skipped.
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(data.register_path())
            .expect("the register opens");
        f.write_all(b"\n").expect("the tear is closed off");
        drop(f);
        let err = data
            .rows()
            .expect_err("a whole row of three columns is not a row");
        assert!(matches!(err, RegisterError::MalformedRow { .. }), "{err}");
    }

    #[test]
    fn an_uninstall_row_carries_the_uninstall_words() {
        let (b, _variant, data) = fixture();
        let hook = b.join("saved/DCS.openbeta/Scripts/Hooks/DcsEvalExecutor.lua");
        put(&hook, b"-- an executor\n");
        let hook = real(&hook);
        data.register(Action::Uninstall)
            .around(an_instant(), &hook, "0", || Ok(()))
            .expect("the removal is recorded");
        let rows = data.rows().expect("the register reads");
        assert_eq!(rows[0].action, "uninstall");
        assert_eq!(
            rows[0].status, "uninstalled",
            "the longest word the column is sized for, written into it"
        );
    }

    #[test]
    fn a_second_that_has_run_out_of_names_is_refused_naming_the_stamp() {
        let b = Sandbox::new();
        let parked = b.dir("parked");
        let stamp = "20251009T085320Z";
        // Two names, both taken by minting them, so the third has nowhere
        // to go. The real bound is far larger and is the same arithmetic.
        mint(&parked, stamp, 2).expect("the first name is free");
        mint(&parked, stamp, 2).expect("the second name is free");
        let err = mint(&parked, stamp, 2).expect_err("and there is no third");
        assert!(matches!(err, RegisterError::Crowded { .. }), "{err}");
        let line = err.to_string();
        assert!(line.contains(stamp), "the line names the second: {line}");
        assert!(line.contains('2'), "and how many it tried: {line}");
    }

    #[test]
    fn the_stamp_is_utc_to_the_second() {
        for (secs, expected) in [
            (0, "19700101T000000Z"),
            // The day after a leap day in a year divisible by 100 and by
            // 400 both: the boundary the civil-from-days arithmetic gets
            // wrong when it is wrong at all.
            (951_868_800, "20000301T000000Z"),
            (1_760_000_000, "20251009T085320Z"),
        ] {
            assert_eq!(stamp(UNIX_EPOCH + Duration::from_secs(secs)), expected);
        }
    }

    #[test]
    fn the_known_data_directory_is_under_local_app_data() {
        // The one test here that touches the real machine. Like the
        // locator's own, it says the wiring is present and the answer is
        // one the client can resolve — not that the path is right for any
        // particular host, which nothing in a fixture could show.
        let local = sys::local_app_data().expect("the shell says where Local AppData is");
        assert!(local.is_absolute(), "an absolute path: {}", local.display());
        // No trees to compare against: the refusal has its own test, and
        // this one is about the known folder being reachable at all.
        let data = DataDir::known(&[]).expect("and the data directory resolves under it");
        assert_eq!(*data.path(), real(&local.join("dcs-mcp")));
    }
}
