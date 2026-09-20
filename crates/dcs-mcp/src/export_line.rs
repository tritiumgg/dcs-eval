//! The one line appended to `Export.lua`, the marker that makes its
//! removal exact, and the copy parked before it goes in.
//!
//! `Export.lua` is not this build's file. SRS, Tacview and whatever else
//! the user has installed each keep a line in it, and several of them were
//! there first. So the only write this module makes is an *append*: the
//! bytes already in the file are never read back out and written again,
//! which makes "no other byte changed" a property of the write rather than
//! something a test has to keep watch over. The copy taken before the
//! append is the second half of that — whatever else goes wrong, the file
//! as it was found is still on disk somewhere the user can reach.
//!
//! The appended line ends in a marker, and that is the whole reason the
//! marker exists: removing the line later is a whole-line exact match on
//! this one string, so a marker on the end is what tells the line apart
//! from a hand-written `dofile` of the same hook.
//!
//! Everything here is bytes. An `Export.lua` edited by hand on a machine
//! set to a Windows codepage can hold bytes that are not UTF-8 at all, and
//! a round trip through `String` would either refuse such a file or spell
//! it back with replacement characters — a file this build had no business
//! rewriting in the first place.

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::SystemTime;

use dcs_eval::paths::{self, Real};
use dcs_eval::sha256;

use crate::register::{Action, DataDir, RegisterError};

/// What the appended line ends in, and what a later removal matches a
/// whole line against.
pub const MARKER: &str = "-- dcs-mcp";

/// The line itself, spelled out rather than built, because it is the text
/// a later uninstall compares byte for byte: a line assembled from parts
/// can be assembled two ways, and the two would not match each other.
///
/// The hook's own leaf name is written here and in the embedding module,
/// which cannot see each other — the embedding is part of the binary and
/// not of this library — so renaming the hook means editing both.
pub const LINE: &str = "dofile(lfs.writedir() .. 'Scripts/Hooks/DcsEvalExecutor.lua') -- dcs-mcp";

/// Where `Export.lua` is under a write directory. It may not be there.
pub fn path(variant: &Real) -> PathBuf {
    variant.as_path().join("Scripts").join("Export.lua")
}

/// How many of `bytes`' lines are the line.
///
/// One trailing carriage return is stripped before the comparison, so an
/// editor that converted the whole file to CRLF after the line went in
/// cannot make this build think the line is missing and add a second one.
///
/// Shared with the verification that reports a duplicated line, so that the
/// count which decides whether a line goes in is the count a later report
/// takes of the same file. Two counters would answer the same question
/// twice and could come to answer it differently.
pub(crate) fn occurrences(bytes: &[u8]) -> usize {
    bytes
        .split(|b| *b == b'\n')
        .filter(|line| {
            let line: &[u8] = match line.split_last() {
                Some((last, head)) if *last == b'\r' => head,
                _ => line,
            };
            line == LINE.as_bytes()
        })
        .count()
}

/// `bytes` with every line that *is* the line taken out, terminator and
/// all, or `None` where no line was.
///
/// Never a rebuild. Each surviving line is copied back as the bytes it
/// was, so a CRLF stays a CRLF, a byte that is not UTF-8 stays that byte,
/// and a file with no final newline does not gain one on the way through.
/// A `split` and a `join` would decide all three of those, and would
/// decide them for a file this build has no business reformatting.
///
/// The comparison strips one trailing carriage return, the same way
/// [`occurrences`] does, so a file an editor converted to CRLF after the
/// line went in is still a file the line can be taken out of. And it is a
/// whole line that is compared, never a prefix of one: the marker on the
/// end is what tells our line from a hand-written `dofile` of the same
/// hook, and a match that stopped short of it would eat the neighbour.
pub fn without(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut kept = Vec::with_capacity(bytes.len());
    let mut found = false;
    let mut start = 0;
    while start < bytes.len() {
        let end = match bytes[start..].iter().position(|b| *b == b'\n') {
            Some(at) => start + at + 1,
            None => bytes.len(),
        };
        let whole = &bytes[start..end];
        let content: &[u8] = match whole.split_last() {
            Some((last, head)) if *last == b'\n' => head,
            _ => whole,
        };
        let content: &[u8] = match content.split_last() {
            Some((last, head)) if *last == b'\r' => head,
            _ => content,
        };
        if content == LINE.as_bytes() {
            found = true;
        } else {
            kept.extend_from_slice(whole);
        }
        start = end;
    }
    found.then_some(kept)
}

/// What [`ensure`] found and what it did about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// There was no `Export.lua`, so one was written holding the line and
    /// nothing else. Nothing was displaced, so nothing was parked, and
    /// nothing is written to the register either: the register records
    /// what was displaced, and here nothing was.
    ///
    /// So this answer is the only place the fact lives. A removal that
    /// takes the line back out of a file created this way leaves an empty
    /// `Export.lua` that only this build ever had a use for, and no row on
    /// disk says so — whoever builds the removal decides what to do about
    /// that, from an empty file rather than from a record.
    Created,
    /// The line was appended to a file that was already there, after a
    /// copy of it was parked. `newline_added` says whether the file was
    /// missing its final newline and was given one, so that the line
    /// landed on a line of its own instead of on the end of somebody
    /// else's.
    Appended {
        parked: PathBuf,
        newline_added: bool,
    },
    /// The line was already in the file. Nothing was written.
    AlreadyThere,
}

/// Put the line in `variant`'s `Export.lua`, once.
///
/// The register's row goes in before the copy is parked and is marked
/// after the append, so a run killed between the two leaves a `pending`
/// row naming the file and a copy of it that can be put back.
pub fn ensure(variant: &Real, data: &DataDir, now: SystemTime) -> Result<Outcome, RegisterError> {
    let resolved = paths::resolve(&path(variant))?;
    let disk = |why: io::Error| RegisterError::Disk {
        path: resolved.as_path().to_owned(),
        why,
    };
    let found = match fs::read(resolved.as_path()) {
        Ok(bytes) => bytes,
        Err(why) if why.kind() == io::ErrorKind::NotFound => {
            if let Some(parent) = resolved.as_path().parent() {
                fs::create_dir_all(parent).map_err(|why| RegisterError::Disk {
                    path: parent.to_owned(),
                    why,
                })?;
            }
            let mut whole = LINE.as_bytes().to_vec();
            whole.push(b'\n');
            fs::write(resolved.as_path(), &whole).map_err(disk)?;
            return Ok(Outcome::Created);
        }
        Err(why) => return Err(disk(why)),
    };
    if occurrences(&found) > 0 {
        return Ok(Outcome::AlreadyThere);
    }
    // A file whose last line has no newline on it: the line would join
    // onto the end of that line, which is one line neither this build nor
    // whoever wrote the other one would ever match again.
    let newline_added = !found.is_empty() && !found.ends_with(b"\n");
    let sha = sha256::hex(&sha256::digest(&found));
    let register = data.register(Action::Install);
    let parked = register.around(now, &resolved, &sha, || {
        let parked = register.copy_aside(now, variant, resolved.as_path())?;
        let mut tail = Vec::new();
        if newline_added {
            tail.push(b'\n');
        }
        tail.extend_from_slice(LINE.as_bytes());
        tail.push(b'\n');
        // Opened for append and written once. The prefix is never read
        // back into this process and never written out again, so there is
        // no path by which a byte of it could change.
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(resolved.as_path())
            .map_err(disk)?;
        f.write_all(&tail).map_err(disk)?;
        Ok(parked)
    })?;
    Ok(Outcome::Appended {
        parked,
        newline_added,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;
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
    /// holding one variant, and the data directory beside it rather than
    /// inside, which is the arrangement `DataDir::at` refuses to violate.
    fn fixture() -> (Sandbox, Real, DataDir) {
        let b = Sandbox::new();
        let saved = real(&b.dir("saved"));
        let variant = real(&b.dir("saved/DCS.openbeta"));
        let data = DataDir::at(&b.join("data"), &[&saved]).expect("beside is not inside");
        (b, variant, data)
    }

    /// An `Export.lua` of the awkward kind: a CRLF line, a blank line, a
    /// byte that is not UTF-8 at all — `0x92`, a Windows-1252 quote — and
    /// somebody else's `dofile` line.
    const AWKWARD: &[u8] =
        b"-- Tacview\r\nlocal Tacview = 1\n\n-- SRS \x92 export\ndofile(lfs.writedir()..'Scripts/Hooks/SRS.lua')\n";

    #[test]
    fn install_twice_leaves_one_dofile_line() {
        let (b, variant, data) = fixture();
        let file = b.join("saved/DCS.openbeta/Scripts/Export.lua");
        put(&file, AWKWARD);

        let first = ensure(&variant, &data, an_instant()).expect("the line goes in");
        assert!(
            matches!(first, Outcome::Appended { .. }),
            "the first call appends: {first:?}"
        );
        let once = fs::read(&file).expect("the file reads");

        let again = ensure(&variant, &data, an_instant()).expect("the second call looks");
        assert_eq!(
            again,
            Outcome::AlreadyThere,
            "the line it wrote is the line it finds"
        );
        let twice = fs::read(&file).expect("the file reads");
        assert_eq!(occurrences(&twice), 1, "one dofile line, not two");
        assert_eq!(twice, once, "and the second call wrote nothing at all");
    }

    #[test]
    fn a_file_with_no_trailing_newline_gains_one_rather_than_a_joined_line() {
        let (b, variant, data) = fixture();
        let found = &AWKWARD[..AWKWARD.len() - 1];
        let file = b.join("saved/DCS.openbeta/Scripts/Export.lua");
        put(&file, found);

        let outcome = ensure(&variant, &data, an_instant()).expect("the line goes in");
        assert!(
            matches!(
                outcome,
                Outcome::Appended {
                    newline_added: true,
                    ..
                }
            ),
            "the missing newline is noticed: {outcome:?}"
        );

        let after = fs::read(&file).expect("the file reads");
        assert_eq!(&after[..found.len()], found, "the prefix is untouched");
        let lines: Vec<&[u8]> = after
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .collect();
        let tail = &lines[lines.len() - 2..];
        assert_eq!(
            tail[0], b"dofile(lfs.writedir()..'Scripts/Hooks/SRS.lua')",
            "somebody else's last line is still a line of its own"
        );
        assert_eq!(tail[1], LINE.as_bytes(), "and ours is the line after it");
        assert!(
            !lines.iter().any(|line| line
                .starts_with(b"dofile(lfs.writedir()..'Scripts/Hooks/SRS.lua')")
                && line.ends_with(MARKER.as_bytes())),
            "and no line is the two of them joined together"
        );
    }

    #[test]
    fn an_absent_export_file_is_created_carrying_the_line() {
        let (b, variant, data) = fixture();
        let file = b.join("saved/DCS.openbeta/Scripts/Export.lua");
        assert!(!file.exists(), "nothing there, not even the directory");

        let outcome = ensure(&variant, &data, an_instant()).expect("the file is made");
        assert_eq!(outcome, Outcome::Created);
        assert_eq!(
            fs::read(&file).expect("the file reads"),
            [LINE.as_bytes(), b"\n"].concat(),
            "the line and a newline, and nothing else"
        );
        assert!(
            data.rows().expect("the register reads").is_empty(),
            "nothing was displaced, so nothing was recorded or parked"
        );
    }

    #[test]
    fn every_other_byte_of_the_file_is_the_byte_it_was() {
        let (b, variant, data) = fixture();
        let file = b.join("saved/DCS.openbeta/Scripts/Export.lua");
        put(&file, AWKWARD);

        ensure(&variant, &data, an_instant()).expect("the line goes in");

        let after = fs::read(&file).expect("the file reads");
        assert_eq!(
            &after[..AWKWARD.len()],
            AWKWARD,
            "down to the CRLF and the byte that is not UTF-8"
        );
        assert_eq!(
            &after[AWKWARD.len()..],
            [LINE.as_bytes(), b"\n"].concat().as_slice(),
            "and what was added is exactly one line"
        );
    }

    #[test]
    fn the_file_as_found_is_parked_before_the_line_goes_in() {
        let (b, variant, data) = fixture();
        let file = b.join("saved/DCS.openbeta/Scripts/Export.lua");
        put(&file, AWKWARD);
        let resolved = real(&file);

        ensure(&variant, &data, an_instant()).expect("the line goes in");

        let rows = data.rows().expect("the register reads");
        assert_eq!(rows.len(), 1, "one action, one row");
        assert_eq!(rows[0].action, "install");
        assert_eq!(rows[0].path, resolved.as_path());
        assert_eq!(
            rows[0].sha256,
            sha256::hex(&sha256::digest(AWKWARD)),
            "the file as found, not as left"
        );
        assert_eq!(rows[0].status, "installed");

        let mut minted: Vec<PathBuf> = fs::read_dir(data.parked_root())
            .expect("the park store is there")
            .map(|e| e.expect("the entry reads").path())
            .collect();
        assert_eq!(minted.len(), 1, "one park");
        let dir = minted.pop().expect("the one park");
        assert_eq!(
            fs::read(dir.join("Scripts").join("Export.lua")).expect("the parked copy"),
            AWKWARD,
            "the bytes as they were found"
        );
        assert!(
            file.exists(),
            "and the original is still where DCS looks for it"
        );
    }

    #[test]
    fn the_line_carries_the_marker_that_makes_its_removal_an_exact_match() {
        assert!(
            LINE.ends_with(MARKER),
            "a removal matches a whole line, and the marker is what tells \
             this line from a hand-written one: {LINE}"
        );
        assert!(
            LINE.contains("DcsEvalExecutor.lua"),
            "and it loads the hook this build installs: {LINE}"
        );
        assert!(!LINE.contains('\n'), "one line: {LINE}");
    }

    #[test]
    fn a_crlf_terminated_copy_of_the_line_is_removed_with_its_crlf() {
        let before = b"-- Tacview\r\n".to_vec();
        let after = b"local Tacview = 1\n".to_vec();
        let whole = [
            before.clone(),
            LINE.as_bytes().to_vec(),
            b"\r\n".to_vec(),
            after.clone(),
        ]
        .concat();

        assert_eq!(
            without(&whole).expect("the line is in there"),
            [before, after].concat(),
            "the line goes, and the carriage return it ended in goes with it"
        );
    }

    #[test]
    fn a_file_holding_no_such_line_answers_that_there_was_none() {
        assert_eq!(
            without(AWKWARD),
            None,
            "nothing of ours is in it, so there is nothing to write back"
        );
        let handwritten = "dofile(lfs.writedir() .. 'Scripts/Hooks/DcsEvalExecutor.lua')\n";
        assert_eq!(
            without(handwritten.as_bytes()),
            None,
            "and a hand-written dofile of the same hook is not our line: {handwritten}"
        );
    }

    #[test]
    fn a_crlf_terminated_copy_of_the_line_is_still_the_line() {
        let (b, variant, data) = fixture();
        let file = b.join("saved/DCS.openbeta/Scripts/Export.lua");
        let crlf = format!("-- Tacview\r\n{LINE}\r\n");
        put(&file, crlf.as_bytes());

        let outcome = ensure(&variant, &data, an_instant()).expect("the file is looked at");
        assert_eq!(
            outcome,
            Outcome::AlreadyThere,
            "an editor converting the file does not earn it a second line"
        );
        assert_eq!(
            fs::read(&file).expect("the file reads"),
            crlf.as_bytes(),
            "and nothing was written"
        );
    }
}
