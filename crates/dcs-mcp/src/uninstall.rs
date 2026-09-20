//! Taking back what was installed: the hook, the `Export.lua` line, and
//! the files that were moved aside to make room for them.
//!
//! Three rules, and the module is not much more than them.
//!
//! **Remove only what is ours.** The hook goes only when its hash is one
//! this project has shipped; any other hash is somebody else's file at our
//! name, and it is left standing and named rather than moved, parked or
//! deleted. The `Export.lua` line goes only by whole-line equality against
//! the line the installer wrote, marker and all — a match on a prefix
//! would take a hand-written `dofile` of the same hook out with it, and
//! the file is SRS's and Tacview's as much as ours.
//!
//! **Put back only what we displaced.** Every file the installer moved out
//! of the way went into the park store under its path relative to the
//! variant it came from, and a row in the register names where it was. The
//! two together are what a restore is driven by; decision record 0020 is
//! the third thing it needs, which is how a file that was *moved* out is
//! told from one that was only copied aside.
//!
//! **Write both down.** Every removal and every restore goes through a
//! register row, opened before the file is touched and marked after, so a
//! run killed in the middle leaves a `pending` row naming exactly what was
//! in flight.
//!
//! Nothing here deletes anything under the data directory, and nothing
//! here fails because a file is missing. An uninstall of a machine that
//! was never installed on is a report saying so, not a refusal.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use dcs_eval::paths::{self, Real};
use dcs_eval::sha256::{digest, hex};

use crate::export_line;
use crate::install::{self, Executor};
use crate::register::{Action, DataDir, RegisterError};

/// A file at the hook's name whose hash this project never shipped, so it
/// was left where it is. `Display` is the project's `<path>: <reason>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Left {
    pub file: PathBuf,
    pub sha256: String,
}

impl fmt::Display for Left {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: sha256 {} is not one this project has shipped, so the file is somebody else's \
             work and is left where it is",
            self.file.display(),
            self.sha256
        )
    }
}

/// What became of the `Export.lua` line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineOutcome {
    /// The line was taken out and the rest of the file written back as it
    /// was found, with a copy of that file parked first. `emptied` says
    /// the line was the whole of the file: what is left is a zero-byte
    /// `Export.lua`, kept rather than deleted, because nothing on disk
    /// records that this build was the one that created it and deleting
    /// somebody's file on a guess is the loss the park store exists
    /// against.
    Removed { parked: PathBuf, emptied: bool },
    /// There is an `Export.lua`, and no line of it is ours. Nothing was
    /// written.
    NotThere,
    /// There is no `Export.lua` at all.
    NoFile,
}

/// What an uninstall took back, what it would not touch, and what it put
/// where it found it.
#[derive(Clone, Debug)]
pub struct Removed {
    pub hook: Option<PathBuf>,
    pub left: Vec<Left>,
    pub line: LineOutcome,
    pub restored: Vec<PathBuf>,
}

/// Take `release` back out of `variant`.
///
/// The order is removals first and restores last, and it is not
/// arbitrary: a park is put back only where nothing occupies its path, so
/// the hook has to be gone before the file it displaced can come home.
pub fn uninstall(
    now: SystemTime,
    variant: &Real,
    data: &DataDir,
    release: &Executor<'_>,
) -> Result<Removed, RegisterError> {
    let register = data.register(Action::Uninstall);
    let mut left = Vec::new();

    // One read of the hooks directory, with every name compared case
    // folded, because that is how the placement found the name and how
    // Windows itself matches one.
    let hooks = variant.as_path().join("Scripts").join("Hooks");
    let mut found: Option<PathBuf> = None;
    match fs::read_dir(&hooks) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|why| disk(&hooks, why))?;
                if entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(release.name)
                {
                    found = Some(entry.path());
                }
            }
        }
        // Nothing has made it, so nothing of ours is in it.
        Err(why) if why.kind() == io::ErrorKind::NotFound => {}
        Err(why) => return Err(disk(&hooks, why)),
    }

    let mut hook = None;
    if let Some(file) = found {
        let bytes = fs::read(&file).map_err(|why| disk(&file, why))?;
        let sha = hex(&digest(&bytes));
        let ours = release.shipped.contains(&sha.as_str());
        if ours {
            let resolved = paths::resolve(&file)?;
            register.around(now, &resolved, &sha, || {
                fs::remove_file(resolved.as_path()).map_err(|why| RegisterError::Disk {
                    path: resolved.as_path().to_owned(),
                    why,
                })
            })?;
            hook = Some(resolved.into_path_buf());
        } else {
            // No row: the register records what this build moved, and this
            // is a file it refused to move. The report is where the fact
            // lives, which is the same place install puts a refusal.
            left.push(Left { file, sha256: sha });
        }
    }

    let line = remove_line(now, variant, &register)?;
    let restored = restore_parked(now, variant, data, &register)?;

    Ok(Removed {
        hook,
        left,
        line,
        restored,
    })
}

/// Take the one line out of `variant`'s `Export.lua`, parking the file as
/// found before the rest of it is written back.
fn remove_line(
    now: SystemTime,
    variant: &Real,
    register: &crate::register::Register<'_>,
) -> Result<LineOutcome, RegisterError> {
    let spelled = export_line::path(variant);
    let file = paths::resolve(&spelled)?;
    let found = match fs::read(file.as_path()) {
        Ok(bytes) => bytes,
        Err(why) if why.kind() == io::ErrorKind::NotFound => return Ok(LineOutcome::NoFile),
        Err(why) => {
            return Err(RegisterError::Disk {
                path: file.as_path().to_owned(),
                why,
            });
        }
    };
    let Some(kept) = export_line::without(&found) else {
        return Ok(LineOutcome::NotThere);
    };
    let emptied = kept.is_empty();
    // The digest is of the file as found, which is the file the parked
    // copy holds — the row and the park then name the same bytes.
    let sha = hex(&digest(&found));
    // The leaf and its directory come from the path the appending module
    // spells, rather than being written out a second time here.
    let directory = spelled
        .parent()
        .expect("the export file's path names a directory");
    let leaf = spelled
        .file_name()
        .expect("the export file's path ends in its name")
        .to_string_lossy()
        .into_owned();
    let parked = register.around(now, &file, &sha, || {
        let parked = register.copy_aside(now, variant, file.as_path())?;
        install::staged(directory, &leaf, &kept, |_| Ok(()))?;
        Ok(parked)
    })?;
    Ok(LineOutcome::Removed { parked, emptied })
}

/// Put back every file the installer moved out of `variant`, and answer
/// where each one went.
///
/// A park is paired with the row that made it by three things: the row's
/// stamp, which names the directories the park could be in; the row's path
/// relative to the variant, which names the file inside one of them; and
/// the destination being *absent*, which is what tells a file that was
/// moved out from one that was only copied aside. Decision record 0020
/// holds that argument. The absence is read per row at the moment the row
/// is reached rather than once at the start, so a row whose file another
/// row has just restored sees it there and leaves it alone.
fn restore_parked(
    now: SystemTime,
    variant: &Real,
    data: &DataDir,
    register: &crate::register::Register<'_>,
) -> Result<Vec<PathBuf>, RegisterError> {
    let mut restored = Vec::new();
    for row in data.rows()? {
        if row.action != Action::Install.word() {
            continue;
        }
        let destination = paths::resolve(&row.path)?;
        if !variant.contains(&destination) || destination.as_path().exists() {
            continue;
        }
        let Ok(relative) = destination.as_path().strip_prefix(variant.as_path()) else {
            continue;
        };
        // The minting allocates `<stamp>`, `<stamp>-2`, … with no gaps and
        // nothing ever removes one, so walking until a name is not there
        // walks exactly that second's parks, oldest first.
        let mut n = 1u32;
        loop {
            let dir = data.parked_root().join(if n == 1 {
                row.stamp.clone()
            } else {
                format!("{}-{n}", row.stamp)
            });
            if !dir.is_dir() {
                break;
            }
            let candidate = dir.join(relative);
            if candidate.is_file() {
                register.restore(now, &candidate, &destination)?;
                restored.push(destination.into_path_buf());
                break;
            }
            n += 1;
        }
    }
    Ok(restored)
}

/// A disk failure, in the one shape the register already spells it in.
fn disk(path: &Path, why: io::Error) -> RegisterError {
    RegisterError::Disk {
        path: path.to_owned(),
        why,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::{Duration, UNIX_EPOCH};

    use crate::install::place_hook;
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

    /// A sandbox laid out the way the installer left one: a `Saved Games`
    /// holding one variant, the data directory **beside** it rather than
    /// inside, and the `Scripts\Hooks` path not yet made.
    fn fixture() -> (Sandbox, Real, DataDir, PathBuf) {
        let b = Sandbox::new();
        let saved = real(&b.dir("saved"));
        let variant = real(&b.dir("saved/DCS.openbeta"));
        let data = DataDir::at(&b.join("data"), &[&saved]).expect("beside is not inside");
        let hooks = variant.as_path().join("Scripts").join("Hooks");
        (b, variant, data, hooks)
    }

    const OLDER: &[u8] = b"-- an older release\n";
    const CURRENT: &[u8] = b"-- this release\n";
    const STRANGER: &[u8] = b"-- somebody else's hook\n";

    /// A release with two shipped hashes, so that "ours, older" and "ours,
    /// current" are both reachable. The embedded list has one entry, which
    /// makes the pair unreachable through it.
    fn a_release() -> (Executor<'static>, String) {
        // Leaked so the `Executor` can be `'static` and the hashes can
        // still be computed rather than written down; the process is a
        // test binary and this happens a handful of times.
        let older = Box::leak(hex(&digest(OLDER)).into_boxed_str());
        let current = Box::leak(hex(&digest(CURRENT)).into_boxed_str());
        let shipped: &'static [&'static str] =
            Box::leak(vec![&*older, &*current].into_boxed_slice());
        (
            Executor {
                name: "DcsEvalExecutor.lua",
                bytes: CURRENT,
                sha256: current,
                shipped,
            },
            current.to_owned(),
        )
    }

    /// Every leaf name in `dir`, sorted, or none if it is not there.
    fn leaves(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .map(|entry| {
                entry
                    .expect("the entry reads")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    /// Every file under `dir`, by path and bytes, sorted — the whole tree
    /// in one value, so two of them can be compared outright.
    fn tree(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_owned()];
        while let Some(next) = stack.pop() {
            for entry in fs::read_dir(&next).into_iter().flatten() {
                let entry = entry.expect("the entry reads");
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let bytes = fs::read(&path).expect("the file reads");
                    out.push((path, bytes));
                }
            }
        }
        out.sort();
        out
    }

    /// An `Export.lua` of the awkward kind: a CRLF line, a blank line, a
    /// byte that is not UTF-8 at all — `0x92`, a Windows-1252 quote —
    /// somebody else's `dofile` line, and a hand-written `dofile` of *our
    /// own hook* carrying no marker. That last line is the one a match on
    /// anything less than a whole line would eat.
    const AWKWARD: &[u8] = b"-- Tacview\r\nlocal Tacview = 1\n\n-- SRS \x92 export\ndofile(lfs.writedir()..'Scripts/Hooks/SRS.lua')\ndofile(lfs.writedir() .. 'Scripts/Hooks/DcsEvalExecutor.lua')\n";

    /// What a later installer appends after ours has been and gone.
    const AFTER_US: &[u8] = b"-- SRS, added after us\n";

    #[test]
    fn the_export_line_is_removed_by_exact_match_with_its_neighbours_byte_identical() {
        let (b, variant, data, _hooks) = fixture();
        let (release, _current) = a_release();
        let file = b.join("saved/DCS.openbeta/Scripts/Export.lua");
        put(&file, AWKWARD);
        export_line::ensure(&variant, &data, an_instant()).expect("the line goes in");
        // A line somebody else appended after the install, which is what
        // makes this check able to see a restore putting the parked copy
        // back over the file the removal just wrote.
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .expect("the file opens");
        io::Write::write_all(&mut f, AFTER_US).expect("the later line goes in");
        drop(f);

        let removed = uninstall(an_instant(), &variant, &data, &release).expect("it comes out");

        assert_eq!(
            fs::read(&file).expect("the file reads"),
            [AWKWARD, AFTER_US].concat(),
            "the file as it was found, down to the CRLF, the byte that is not UTF-8 and the \
             hand-written dofile, plus what went in after us"
        );
        assert!(
            matches!(removed.line, LineOutcome::Removed { emptied: false, .. }),
            "one line out of a file that still holds plenty: {:?}",
            removed.line
        );
    }

    #[test]
    fn a_hook_whose_hash_is_not_one_we_shipped_is_left_in_place_and_named() {
        let (_b, variant, data, hooks) = fixture();
        let (release, _current) = a_release();
        let hook = hooks.join("DcsEvalExecutor.lua");
        put(&hook, STRANGER);

        let removed = uninstall(an_instant(), &variant, &data, &release).expect("it looks");

        assert_eq!(
            fs::read(&hook).expect("the file is still there"),
            STRANGER,
            "a file this project never shipped is not this project's to remove"
        );
        assert!(removed.hook.is_none(), "nothing of ours was taken back");
        assert_eq!(
            removed.left.len(),
            1,
            "and the one file is named: {removed:?}"
        );
        let said = removed.left[0].to_string();
        assert!(
            said.contains("DcsEvalExecutor.lua") && said.contains(&hex(&digest(STRANGER))),
            "the report names the file and the hash that spared it: {said}"
        );
        assert!(
            data.rows()
                .expect("the register reads")
                .iter()
                .all(|row| row.action != "uninstall"),
            "and nothing was written down, because nothing was moved"
        );
    }

    #[test]
    fn a_hook_this_project_shipped_is_removed_and_its_row_says_uninstalled() {
        let (_b, variant, data, hooks) = fixture();
        let (release, current) = a_release();
        let placed =
            place_hook(an_instant(), &variant, &data, &release, false).expect("it goes in");

        let removed = uninstall(an_instant(), &variant, &data, &release).expect("it comes out");

        assert_eq!(removed.hook.as_deref(), Some(placed.hook.as_path()));
        assert!(
            leaves(&hooks).is_empty(),
            "the hook is gone and nothing is left beside it: {:?}",
            leaves(&hooks)
        );
        let rows = data.rows().expect("the register reads");
        let last = rows.last().expect("the removal wrote one");
        assert_eq!(last.action, "uninstall");
        assert_eq!(last.status, "uninstalled");
        assert_eq!(last.path, placed.hook);
        assert_eq!(last.sha256, current, "the bytes that were taken away");
    }

    #[test]
    fn a_parked_file_is_restored_to_its_original_path_and_the_park_is_emptied() {
        let (_b, variant, data, hooks) = fixture();
        let (release, _current) = a_release();
        let hook = hooks.join("DcsEvalExecutor.lua");
        put(&hook, STRANGER);
        let placed = place_hook(an_instant(), &variant, &data, &release, true)
            .expect("--replace answers for the stranger");
        let park = placed
            .parked
            .first()
            .expect("the stranger was parked")
            .clone();

        let removed = uninstall(an_instant(), &variant, &data, &release).expect("it comes out");

        assert_eq!(
            fs::read(&hook).expect("the file is back"),
            STRANGER,
            "the file that was there before us, back at its own name"
        );
        assert!(
            !park
                .join("Scripts")
                .join("Hooks")
                .join("DcsEvalExecutor.lua")
                .exists(),
            "the park still holds the file it gave back"
        );
        assert!(
            park.exists(),
            "the minted directory stays — nothing under the data directory is deleted"
        );
        assert_eq!(removed.restored, vec![real(&hook).into_path_buf()]);
    }

    #[test]
    fn an_export_file_left_empty_by_the_removal_is_kept_and_named() {
        let (b, variant, data, _hooks) = fixture();
        let (release, _current) = a_release();
        let file = b.join("saved/DCS.openbeta/Scripts/Export.lua");
        assert_eq!(
            export_line::ensure(&variant, &data, an_instant()).expect("the file is made"),
            export_line::Outcome::Created,
            "nothing was there, so the file holds our line and nothing else"
        );

        let removed = uninstall(an_instant(), &variant, &data, &release).expect("it comes out");

        assert!(file.exists(), "the file is kept, not deleted");
        assert_eq!(
            fs::read(&file).expect("the file reads"),
            Vec::<u8>::new(),
            "and it is empty, because our line was the whole of it"
        );
        assert!(
            matches!(removed.line, LineOutcome::Removed { emptied: true, .. }),
            "and the report says so, because nothing on disk does: {:?}",
            removed.line
        );
    }

    #[test]
    fn a_second_uninstall_changes_nothing() {
        let (b, variant, data, hooks) = fixture();
        let (release, _current) = a_release();
        let hook = hooks.join("DcsEvalExecutor.lua");
        put(&hook, STRANGER);
        put(&b.join("saved/DCS.openbeta/Scripts/Export.lua"), AWKWARD);
        place_hook(an_instant(), &variant, &data, &release, true).expect("it goes in");
        export_line::ensure(&variant, &data, an_instant()).expect("the line goes in");

        uninstall(an_instant(), &variant, &data, &release).expect("the first one comes out");
        let after = tree(variant.as_path());
        let rows = data.rows().expect("the register reads").len();

        uninstall(an_instant(), &variant, &data, &release).expect("the second one looks");

        assert_eq!(
            tree(variant.as_path()),
            after,
            "the second pass found nothing of ours and wrote nothing"
        );
        assert_eq!(
            data.rows().expect("the register reads").len(),
            rows,
            "and it moved nothing, so it recorded nothing"
        );
    }
}
