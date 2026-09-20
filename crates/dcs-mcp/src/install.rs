//! Placing the executor in `Scripts\Hooks\`: what is already there, whether
//! we may touch it, and how the bytes get to their final name.
//!
//! DCS loads every `.lua` in that directory at start-up, so the file this
//! writes is one the game runs. Two things follow, and they are the whole of
//! this module. The first is that the final name may never hold half a file:
//! a game started mid-write would load a truncated chunk, so the bytes are
//! written under a staging name in the same directory and renamed in, and a
//! rename within a directory is the one filesystem operation that either
//! happened or did not. The second is that whatever is already there has to
//! be identified before it is displaced — this project's own release, an
//! older one of ours, a stranger's file, or the executor of the project this
//! one replaces — because only the first two are ours to answer for.
//!
//! Nothing here records or moves anything itself. The register writes the row
//! before the write and marks it after, and the park store holds what was
//! displaced; both are `register`'s, and this module only decides what is
//! handed to them.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use dcs_eval::paths::{self, Real};
use dcs_eval::sha256::{digest, hex};

use crate::register::{Action, DataDir, RegisterError};

/// The leaf names the project this one replaces put in `Scripts\Hooks\`.
///
/// Its executor registers the same callbacks ours does, so a machine holding
/// both runs two of them and each event is handled twice. They are named
/// rather than matched by pattern: a pattern over `DcsApi*` would sweep up a
/// file somebody else happened to name that way, and this list is the set of
/// files one known project is known to have written.
const INCUMBENT: [&str; 2] = ["DcsApiEval.lua", "DcsApiExport.lua"];

/// The release being placed: the bytes, the name they go under, the hash of
/// those bytes, and every hash this project has shipped.
///
/// The release is a parameter and not read off `embed` inside the placement,
/// for the same reason `register::mint` takes its limit rather than reading
/// the constant: the real shipped list has one entry today, so an upgrade
/// whose old bytes genuinely differ from the new is unreachable without a
/// second release to upgrade from. Handing the release in is what makes that
/// case testable at all.
pub struct Executor<'a> {
    pub name: &'a str,
    pub bytes: &'a [u8],
    pub sha256: &'a str,
    pub shipped: &'a [&'a str],
}

impl Executor<'static> {
    /// The release this binary carries.
    pub fn embedded() -> Self {
        Self {
            name: crate::embed::EXECUTOR_FILE_NAME,
            bytes: crate::embed::EXECUTOR,
            sha256: crate::embed::EXECUTOR_SHA256,
            shipped: crate::embed::SHIPPED,
        }
    }
}

/// What was at the hook's name before this placement, as far as its hash can
/// say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Disposition {
    /// Nothing was there.
    Absent,
    /// A file whose hash this project has shipped: an older release, or this
    /// one placed again.
    Upgrade { sha256: String },
    /// A file whose hash this project never shipped, so somebody else wrote
    /// it.
    Foreign { sha256: String },
}

/// What a placement did: where the hook now is, what it displaced, and every
/// directory a displaced file was parked in.
#[derive(Clone, Debug)]
pub struct Placed {
    pub hook: PathBuf,
    pub disposition: Disposition,
    pub parked: Vec<PathBuf>,
}

/// Put `release` at `<variant>\Scripts\Hooks\<name>`, answering for whatever
/// was there first.
///
/// `replace` is the user saying they know a file they did not put there is
/// about to be moved aside. It is consulted in one place — the refusals near
/// the top — and never again: below them, what is parked is decided by what
/// was found and nothing else, so an upgrade of our own file needs no answer
/// and a stranger's file cannot be parked without one.
pub fn place_hook(
    now: SystemTime,
    variant: &Real,
    data: &DataDir,
    release: &Executor<'_>,
    replace: bool,
) -> Result<Placed, InstallError> {
    let hooks = variant.as_path().join("Scripts").join("Hooks");

    // One read of the directory, and every name compared with case folded.
    // Windows matches names that way, so a hook written back as
    // `dcsevalexecutor.lua` is the same file to the game and has to be the
    // same file here; reaching ours by `join` while folding only the
    // incumbent's names would leave the two halves disagreeing. Only this
    // directory is looked at, deliberately: a wider sweep for stray
    // `DcsEval*`/`DcsApi*` files anywhere under the variant is what `verify`
    // is for, and doing it here would make placing a hook depend on the whole
    // tree being tidy.
    let mut ours: Option<PathBuf> = None;
    let mut incumbent: Vec<PathBuf> = Vec::new();
    match fs::read_dir(&hooks) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|why| disk(&hooks, why))?;
                let leaf = entry.file_name().to_string_lossy().into_owned();
                if leaf.eq_ignore_ascii_case(release.name) {
                    ours = Some(entry.path());
                } else if INCUMBENT.iter().any(|name| leaf.eq_ignore_ascii_case(name)) {
                    incumbent.push(entry.path());
                }
            }
        }
        // A directory that is not there is an empty one. DCS makes it itself
        // only when something has been put in it, so its absence is the
        // ordinary case on a fresh install and not a failure.
        Err(why) if why.kind() == io::ErrorKind::NotFound => {}
        Err(why) => return Err(disk(&hooks, why)),
    }
    // So that the refusal below names the two files in the same order every
    // time, whatever order the filesystem hands them back in.
    incumbent.sort();

    let disposition = match &ours {
        None => Disposition::Absent,
        Some(hook) => {
            let bytes = fs::read(hook).map_err(|why| disk(hook, why))?;
            let sha = hex(&digest(&bytes));
            if release.shipped.contains(&sha.as_str()) {
                Disposition::Upgrade { sha256: sha }
            } else {
                Disposition::Foreign { sha256: sha }
            }
        }
    };

    // Both refusals happen before anything on disk is touched — before even
    // the Hooks directory is made. A refusal that left a directory behind is
    // not a refusal that changed nothing, and changing nothing is the whole
    // of what it promises.
    if !replace {
        if let (Some(hook), Disposition::Foreign { sha256: hash }) = (&ours, &disposition) {
            return Err(InstallError::Foreign {
                file: hook.clone(),
                hash: hash.clone(),
            });
        }
        if !incumbent.is_empty() {
            return Err(InstallError::Incumbent { files: incumbent });
        }
    }

    fs::create_dir_all(&hooks).map_err(|why| disk(&hooks, why))?;

    let register = data.register(Action::Install);
    let mut parked = Vec::new();
    // The incumbent's files first, so that if anything below fails the
    // machine is left with one executor rather than two.
    for file in &incumbent {
        parked.push(register.park(now, variant, file)?);
    }
    if let Some(hook) = &ours {
        parked.push(register.park(now, variant, hook)?);
    }

    let hook = paths::resolve(&hooks.join(release.name)).map_err(RegisterError::from)?;
    register.around(now, &hook, release.sha256, || {
        staged(&hooks, release.name, release.bytes, |_| Ok(()))
    })?;

    Ok(Placed {
        hook: hook.into_path_buf(),
        disposition,
        parked,
    })
}

/// Write `bytes` into `dir` under `name`, through a staging file in that same
/// directory.
///
/// The staging file is a sibling of the destination and never anywhere else.
/// A rename within one directory is atomic; a rename across volumes is a copy
/// followed by a delete, which is exactly the half-written window the staging
/// exists to close — so a `.tmp` in the system temp directory would undo the
/// whole thing on any machine whose `Saved Games` is on another drive.
///
/// `between` runs after the bytes are on disk and before the rename, and its
/// reason is the one `register::around`'s closure has: written first or
/// written last, the finished state is identical, and the only observable
/// difference is what exists between the two steps. A check that looks
/// afterwards cannot tell the orderings apart, so this is the seam it looks
/// through.
///
/// The uninstaller writes `Export.lua` back through this too. That file is
/// not one DCS loads at start-up, so the window it closes there is narrower,
/// but the reasoning is the same one and a second copy of it would be a
/// second thing to keep true.
pub(crate) fn staged(
    dir: &Path,
    name: &str,
    bytes: &[u8],
    between: impl FnOnce(&Path) -> Result<(), RegisterError>,
) -> Result<(), RegisterError> {
    let staging = dir.join(format!("{name}.tmp"));
    let done = fs::write(&staging, bytes)
        .map_err(|why| RegisterError::Disk {
            path: staging.clone(),
            why,
        })
        .and_then(|()| between(&staging))
        .and_then(|()| {
            let final_name = dir.join(name);
            fs::rename(&staging, &final_name).map_err(|why| RegisterError::Disk {
                path: final_name,
                why,
            })
        });
    if done.is_err() {
        // A staging file left behind would be loaded by nothing — DCS reads
        // `.lua` — but it would be read by the next `verify` as a stray file,
        // so it goes.
        let _ = fs::remove_file(&staging);
    }
    done
}

/// A disk failure, in the one shape the register already spells it in.
fn disk(path: &Path, why: io::Error) -> InstallError {
    InstallError::Register(RegisterError::Disk {
        path: path.to_owned(),
        why,
    })
}

/// Why the hook was not placed. `Display` is the project's `<path>: <reason>`.
///
/// Path failures and disk failures are `RegisterError`'s words rather than a
/// second set of variants saying the same thing: the register already spells
/// both, and one more spelling of "the disk said no" is one more thing that
/// can drift.
#[derive(Debug)]
pub enum InstallError {
    Foreign { file: PathBuf, hash: String },
    Incumbent { files: Vec<PathBuf> },
    Register(RegisterError),
}

impl From<RegisterError> for InstallError {
    fn from(why: RegisterError) -> Self {
        Self::Register(why)
    }
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Foreign { file, hash } => write!(
                f,
                "{}: sha256 {hash} is not one this project has shipped, so the file is somebody \
                 else's work; --replace parks it and puts ours in its place",
                file.display()
            ),
            Self::Incumbent { files } => write!(
                f,
                "{}: a second executor DCS would run beside ours, registering its callbacks \
                 twice; --replace parks it",
                files
                    .iter()
                    .map(|file| file.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Register(why) => why.fmt(f),
        }
    }
}

impl std::error::Error for InstallError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Register(why) => Some(why),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::{Duration, UNIX_EPOCH};

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

    /// A release with two shipped hashes, so that "ours, older" and "ours,
    /// current" are both reachable. The embedded list has one entry, which
    /// makes an upgrade from genuinely different bytes unreachable through it.
    fn a_release() -> (Executor<'static>, String, String) {
        // Leaked so the `Executor` can be `'static` and the hashes can still
        // be computed rather than written down; the process is a test binary
        // and this happens a handful of times.
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
            older.to_owned(),
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

    #[test]
    fn an_absent_hook_is_placed_into_a_hooks_directory_that_is_made() {
        let (_b, variant, data, hooks) = fixture();
        let (release, _older, current) = a_release();
        assert!(!hooks.exists(), "nothing has made it yet");

        let placed = place_hook(an_instant(), &variant, &data, &release, false)
            .expect("nothing was in the way");

        assert_eq!(placed.disposition, Disposition::Absent);
        assert!(placed.parked.is_empty(), "nothing was displaced");
        assert_eq!(
            fs::read(hooks.join("DcsEvalExecutor.lua")).expect("the hook is there"),
            CURRENT
        );
        assert_eq!(
            leaves(&hooks),
            vec!["DcsEvalExecutor.lua".to_owned()],
            "the staging file was renamed, not left beside its destination"
        );

        let rows = data.rows().expect("the register reads");
        assert_eq!(rows.len(), 1, "one file was written, so one row");
        assert_eq!(rows[0].path, placed.hook);
        assert_eq!(rows[0].sha256, current);
        assert_eq!(rows[0].status, "installed");
    }

    #[test]
    fn the_placement_puts_the_bytes_down_under_the_staging_name_first() {
        let (_b, variant, data, hooks) = fixture();
        let (release, _older, _current) = a_release();
        // A directory cannot be written to as a file, so occupying the staging
        // name with one makes the staged write fail and the placement with it.
        // That is the only difference a placement writing straight to the name
        // DCS loads would not feel: it would sail past this and succeed, which
        // is what ties this call to the staging the module promises rather
        // than to whatever its final state happens to look like.
        let squatter = hooks.join("DcsEvalExecutor.lua.tmp");
        fs::create_dir_all(&squatter).expect("the staging name is occupied");

        let err = place_hook(an_instant(), &variant, &data, &release, false)
            .expect_err("the bytes had nowhere to be staged");

        assert!(
            matches!(err, InstallError::Register(RegisterError::Disk { .. })),
            "{err}"
        );
        assert!(
            !hooks.join("DcsEvalExecutor.lua").exists(),
            "the name DCS loads was written anyway, so the staging was skipped"
        );
    }

    #[test]
    fn placing_twice_leaves_one_hook_file() {
        let (_b, variant, data, hooks) = fixture();
        let (release, _older, current) = a_release();

        place_hook(an_instant(), &variant, &data, &release, false).expect("the first placement");
        // No `--replace`, and none is owed: what is there is our own release.
        let again = place_hook(an_instant(), &variant, &data, &release, false)
            .expect("an upgrade of our own file answers for itself");

        assert_eq!(
            again.disposition,
            Disposition::Upgrade {
                sha256: current.clone()
            }
        );
        assert_eq!(
            leaves(&hooks),
            vec!["DcsEvalExecutor.lua".to_owned()],
            "one hook file, not two and not a leftover staging file"
        );
        assert_eq!(again.parked.len(), 1, "the first copy was moved aside");
        assert_eq!(
            fs::read(
                again.parked[0]
                    .join("Scripts")
                    .join("Hooks")
                    .join("DcsEvalExecutor.lua")
            )
            .expect("the parked copy"),
            CURRENT
        );
        assert_eq!(
            fs::read(hooks.join("DcsEvalExecutor.lua")).expect("the hook"),
            CURRENT
        );
    }

    #[test]
    fn the_hook_never_appears_half_written() {
        let (_b, _variant, _data, hooks) = fixture();
        fs::create_dir_all(&hooks).expect("the directory is made");
        let final_name = hooks.join("DcsEvalExecutor.lua");

        // The assertions that matter are made from *inside* the closure, at
        // the moment the bytes are on disk and before the rename. Read
        // afterwards, a file staged and renamed and a file written straight
        // to its final name look exactly alike.
        let err = staged(&hooks, "DcsEvalExecutor.lua", CURRENT, |staging| {
            assert_eq!(
                staging.parent(),
                Some(hooks.as_path()),
                "staged where it will be renamed, never on another volume"
            );
            assert_eq!(
                fs::read(staging).expect("the staging file"),
                CURRENT,
                "the bytes are all there before the rename"
            );
            assert!(
                !final_name.exists(),
                "the name DCS loads holds nothing until the rename: {}",
                final_name.display()
            );
            Err(RegisterError::Disk {
                path: staging.to_owned(),
                why: io::Error::other("the rename did not happen"),
            })
        })
        .expect_err("the closure refused, so the call does");
        assert!(matches!(err, RegisterError::Disk { .. }), "{err}");

        assert!(!final_name.exists(), "and still holds nothing");
        assert!(
            leaves(&hooks).is_empty(),
            "the staging file was cleared away: {:?}",
            leaves(&hooks)
        );
    }

    #[test]
    fn a_foreign_hook_is_refused_and_named_without_replace() {
        let (_b, variant, data, hooks) = fixture();
        let (release, _older, _current) = a_release();
        let stranger = b"-- somebody else's hook\n";
        let hook = hooks.join("DcsEvalExecutor.lua");
        put(&hook, stranger);

        let err = place_hook(an_instant(), &variant, &data, &release, false)
            .expect_err("a file this project never shipped is not ours to move");

        assert!(matches!(err, InstallError::Foreign { .. }), "{err}");
        let said = err.to_string();
        assert!(said.contains("DcsEvalExecutor.lua"), "no path: {said}");
        assert!(
            said.contains(&hex(&digest(stranger))),
            "no hash of what is there: {said}"
        );

        assert_eq!(
            fs::read(&hook).expect("the stranger's file"),
            stranger,
            "untouched, byte for byte"
        );
        assert_eq!(
            leaves(&hooks),
            vec!["DcsEvalExecutor.lua".to_owned()],
            "and nothing was staged beside it"
        );
        assert!(
            !data.register_path().exists(),
            "a refusal writes no row: {}",
            data.register_path().display()
        );
        assert!(
            !data.parked_root().exists(),
            "and parks nothing: {}",
            data.parked_root().display()
        );
    }

    #[test]
    fn a_foreign_hook_is_parked_when_replace_answers_for_it() {
        let (_b, variant, data, hooks) = fixture();
        let (release, _older, _current) = a_release();
        let stranger = b"-- somebody else's hook\n";
        let hook = hooks.join("DcsEvalExecutor.lua");
        put(&hook, stranger);

        let placed = place_hook(an_instant(), &variant, &data, &release, true)
            .expect("--replace is the answer the refusal asked for");

        assert_eq!(
            placed.disposition,
            Disposition::Foreign {
                sha256: hex(&digest(stranger))
            }
        );
        assert_eq!(placed.parked.len(), 1);
        assert_eq!(
            fs::read(
                placed.parked[0]
                    .join("Scripts")
                    .join("Hooks")
                    .join("DcsEvalExecutor.lua")
            )
            .expect("the stranger's file is recoverable"),
            stranger
        );
        assert_eq!(fs::read(&hook).expect("ours is in its place"), CURRENT);
    }

    #[test]
    fn a_hook_this_project_shipped_is_replaced_and_its_bytes_parked() {
        let (_b, variant, data, hooks) = fixture();
        let (release, older, current) = a_release();
        let hook = hooks.join("DcsEvalExecutor.lua");
        put(&hook, OLDER);

        let placed = place_hook(an_instant(), &variant, &data, &release, false)
            .expect("an older release of ours needs no answer");

        assert_eq!(
            placed.disposition,
            Disposition::Upgrade {
                sha256: older.clone()
            }
        );
        assert_eq!(placed.parked.len(), 1);
        assert_eq!(
            fs::read(
                placed.parked[0]
                    .join("Scripts")
                    .join("Hooks")
                    .join("DcsEvalExecutor.lua")
            )
            .expect("the older bytes are recoverable"),
            OLDER
        );
        assert_eq!(fs::read(&hook).expect("the new release"), CURRENT);

        let rows = data.rows().expect("the register reads");
        assert_eq!(rows.len(), 2, "the park's row, then the write's");
        assert_eq!(rows[0].sha256, older, "what was moved aside");
        assert_eq!(rows[1].path, placed.hook);
        assert_eq!(rows[1].sha256, current, "what was put there");
        for row in &rows {
            assert_eq!(row.action, "install");
            assert_eq!(row.status, "installed");
        }
    }

    #[test]
    fn the_incumbent_s_two_files_are_named_as_a_second_executor() {
        let (_b, variant, data, hooks) = fixture();
        let (release, _older, _current) = a_release();
        put(&hooks.join("DcsApiEval.lua"), b"-- the prior project\n");
        put(&hooks.join("DcsApiExport.lua"), b"-- the prior project\n");

        let err = place_hook(an_instant(), &variant, &data, &release, false)
            .expect_err("two executors is not something to do quietly");

        assert!(matches!(err, InstallError::Incumbent { .. }), "{err}");
        let said = err.to_string();
        for name in INCUMBENT {
            assert!(said.contains(name), "{name} is not named: {said}");
        }
        assert!(
            said.contains("twice"),
            "the reason is not said, only the files: {said}"
        );

        assert_eq!(
            leaves(&hooks),
            vec!["DcsApiEval.lua".to_owned(), "DcsApiExport.lua".to_owned()],
            "both still there, and ours was not written beside them"
        );
        assert!(
            !data.register_path().exists(),
            "a refusal writes no row: {}",
            data.register_path().display()
        );
    }

    #[test]
    fn the_incumbent_s_two_files_are_parked_with_replace() {
        let (_b, variant, data, hooks) = fixture();
        let (release, _older, _current) = a_release();
        put(&hooks.join("DcsApiEval.lua"), b"-- the eval half\n");
        put(&hooks.join("DcsApiExport.lua"), b"-- the export half\n");

        let placed = place_hook(an_instant(), &variant, &data, &release, true)
            .expect("--replace answers for them");

        assert_eq!(
            placed.disposition,
            Disposition::Absent,
            "ours was not there"
        );
        assert_eq!(placed.parked.len(), 2);
        let recovered: Vec<Vec<u8>> = placed
            .parked
            .iter()
            .zip(["DcsApiEval.lua", "DcsApiExport.lua"])
            .map(|(dir, name)| {
                fs::read(dir.join("Scripts").join("Hooks").join(name))
                    .unwrap_or_else(|why| panic!("{name} is not recoverable: {why}"))
            })
            .collect();
        assert_eq!(
            recovered,
            vec![
                b"-- the eval half\n".to_vec(),
                b"-- the export half\n".to_vec()
            ],
            "each under its own path relative to the variant"
        );
        assert_eq!(
            leaves(&hooks),
            vec!["DcsEvalExecutor.lua".to_owned()],
            "one executor left, and it is ours"
        );
    }

    #[test]
    fn an_incumbent_file_is_recognised_whatever_case_it_is_spelled_in() {
        let (_b, variant, data, hooks) = fixture();
        let (release, _older, _current) = a_release();
        // Windows matches names with case folded, so this is the same file to
        // the game as the one spelled the way the prior project wrote it.
        put(&hooks.join("DCSAPIEXPORT.LUA"), b"-- shouted\n");

        let err = place_hook(an_instant(), &variant, &data, &release, false)
            .expect_err("the spelling is not what makes it a second executor");
        assert!(matches!(err, InstallError::Incumbent { .. }), "{err}");
    }

    #[test]
    fn our_own_hook_is_recognised_whatever_case_it_is_spelled_in() {
        let (_b, variant, data, hooks) = fixture();
        let (release, older, _current) = a_release();
        // Reached by `join` rather than by folding, a hook written back under
        // another spelling would look absent: nothing would be parked, the
        // disposition would say there had been nothing there, and the rename
        // would land beside a file DCS also loads.
        put(&hooks.join("dcsevalexecutor.lua"), OLDER);

        let placed = place_hook(an_instant(), &variant, &data, &release, false)
            .expect("an older release of ours, shouted or not");

        assert_eq!(placed.disposition, Disposition::Upgrade { sha256: older });
        assert_eq!(placed.parked.len(), 1, "the old copy was moved aside");
        assert_eq!(
            leaves(&hooks).len(),
            1,
            "one hook file: {:?}",
            leaves(&hooks)
        );
        assert_eq!(
            fs::read(hooks.join("DcsEvalExecutor.lua")).expect("the new release"),
            CURRENT
        );
    }

    #[test]
    fn the_embedded_release_is_what_the_installer_places_by_default() {
        let release = Executor::embedded();
        assert_eq!(release.name, crate::embed::EXECUTOR_FILE_NAME);
        assert_eq!(release.bytes, crate::embed::EXECUTOR);
        assert_eq!(release.sha256, crate::embed::EXECUTOR_SHA256);
        assert!(
            release.shipped.contains(&release.sha256),
            "the release the installer would place is not one it would call ours"
        );
    }
}
