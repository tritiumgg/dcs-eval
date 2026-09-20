//! The executor the binary carries, the hash of those bytes, and the list of
//! every hash this project has embedded.
//!
//! A user downloads one file. The Lua the installer places in `Scripts\Hooks\`
//! therefore travels inside the binary rather than beside it, and the hash of
//! those bytes travels with it, because every later verb is a comparison: the
//! file on disk either is this release, is one of ours from before, or belongs
//! to somebody else.
//!
//! The hash is a hand-written literal and not computed from the bytes at
//! startup. A computed hash can never disagree with itself, so it would record
//! nothing — the whole point of writing it down is that the tests below can
//! catch a written-down value that has gone stale against the file.
//!
//! `SHIPPED` holds the current digest as a literal entry of its own rather
//! than referring to the constant. The installer recognises a hook file as
//! this project's by finding its hash in that list, and that is what tells an
//! upgrade from a stranger's file; a list defined as `&[EXECUTOR_SHA256]`
//! would recognise the current release and nothing else while looking
//! perfectly correct.
//!
//! The bytes are hashed exactly as committed. Line-ending conversion is turned
//! off tree-wide for this reason: a CR in the executor means the recorded hash
//! is a hash of bytes nobody committed, which passes on the machine that
//! converted them and fails everywhere else. That is a legible failure here
//! rather than a mystery in CI.

/// The executor as committed, carried in the binary.
pub const EXECUTOR: &[u8] = include_bytes!("../../../executor/DcsEvalExecutor.lua");

/// The leaf name the installer writes into `Scripts\Hooks\`.
pub const EXECUTOR_FILE_NAME: &str = "DcsEvalExecutor.lua";

/// The SHA-256 of `EXECUTOR`, written by hand so that it can be found wrong.
pub const EXECUTOR_SHA256: &str =
    "2a66399b06e4c14141179e3769d6180e9bf2e85ae27047a901c27d5f083ad871";

/// Every set of bytes this project has ever embedded, newest last.
///
/// A change to the executor updates `EXECUTOR_SHA256` and appends a line here.
/// An entry is never removed and never reordered: a user's already-installed
/// file has to stay recognisable as ours however old it is, and an entry that
/// left this list would turn one of our own releases into a stranger's file.
pub const SHIPPED: &[&str] = &[
    // The executor of the first release, and what this binary carries today.
    "2a66399b06e4c14141179e3769d6180e9bf2e85ae27047a901c27d5f083ad871",
];

/// Whether a hook file with this hash is one this project put there.
///
/// The installer does not call this: its shipped list is a parameter, so that
/// an upgrade from genuinely different bytes is reachable in a test while the
/// real list has one entry. This is the same question asked of the list this
/// binary actually carries, for the verbs that have no release handed to them.
pub fn is_shipped(hash: &str) -> bool {
    SHIPPED.contains(&hash)
}

/// The build this binary carries, as one line: the release a report names.
///
/// Composed here and deliberately not printed from `main`, whose stdout
/// carries protocol frames and nothing else. The verbs that emit it are
/// `verify` and the command line.
pub fn release_line() -> String {
    release_line_of(EXECUTOR_FILE_NAME, EXECUTOR_SHA256)
}

/// The same line about a release handed in rather than the embedded one.
///
/// A report is taken against whichever release its caller named, and a
/// headline composed from the constants regardless would name this build's
/// hash above a comparison made against another one's — the one place a
/// wrong hash goes unnoticed, because it is the line that looks right.
pub fn release_line_of(name: &str, sha256: &str) -> String {
    format!(
        "dcs-mcp {} · executor {name} sha256 {sha256}",
        env!("CARGO_PKG_VERSION"),
    )
}

#[cfg(test)]
mod embedded {
    use super::*;
    use dcs_eval::sha256::{digest, hex};
    use std::fs;
    use std::path::{Path, PathBuf};

    /// The checkout root: two above this crate's manifest.
    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("the crate sits two below the root")
            .to_path_buf()
    }

    /// The executor as it sits in the repository, read as bytes and never as
    /// a string: a decoded-and-re-encoded copy is not the file.
    fn on_disk() -> Vec<u8> {
        let p = root().join("executor").join(EXECUTOR_FILE_NAME);
        fs::read(&p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
    }

    #[test]
    fn the_embedded_bytes_are_the_repository_s_executor() {
        // Compared as hex so a mismatch prints two readable lines rather than
        // ninety-odd kilobytes of Lua.
        assert_eq!(
            hex(&digest(EXECUTOR)),
            hex(&digest(&on_disk())),
            "the embedded copy is not the repository's executor"
        );
    }

    #[test]
    fn the_recorded_hash_is_the_hash_of_the_file_on_disk() {
        assert_eq!(
            hex(&digest(&on_disk())),
            EXECUTOR_SHA256,
            "the executor changed: update EXECUTOR_SHA256 and append the new hash to SHIPPED"
        );
    }

    #[test]
    fn the_embedded_release_s_hash_is_in_the_shipped_list() {
        assert!(
            is_shipped(EXECUTOR_SHA256),
            "the embedded release's hash is not in the shipped list: {EXECUTOR_SHA256}"
        );
    }

    #[test]
    fn every_shipped_hash_is_sixty_four_lower_case_hex_characters() {
        for h in SHIPPED {
            assert_eq!(h.len(), 64, "not a SHA-256: {h}");
            assert!(
                h.bytes().all(|b| b.is_ascii_hexdigit()),
                "not hexadecimal: {h}"
            );
            assert_eq!(
                *h,
                h.to_ascii_lowercase(),
                "not lower case, so it will never equal a digest this build prints: {h}"
            );
        }
        // The list is short enough that a nested scan is the clearest way to
        // say it: a duplicate means a release was recorded twice.
        for (i, a) in SHIPPED.iter().enumerate() {
            for b in &SHIPPED[i + 1..] {
                assert_ne!(a, b, "the shipped list carries {a} twice");
            }
        }
    }

    #[test]
    fn the_executor_carries_no_carriage_return() {
        assert!(
            !EXECUTOR.contains(&b'\r'),
            "a CR in the executor means line-ending conversion ran: \
             the recorded hash is a hash of bytes nobody committed"
        );
    }

    #[test]
    fn a_hash_this_project_never_embedded_is_not_shipped() {
        assert!(
            !is_shipped(&"0".repeat(64)),
            "a stranger's file was called ours"
        );
    }

    #[test]
    fn the_release_line_names_the_executor_build() {
        let line = release_line();
        assert!(
            line.contains(env!("CARGO_PKG_VERSION")),
            "no version: {line}"
        );
        assert!(
            line.contains(EXECUTOR_FILE_NAME),
            "no executor name: {line}"
        );
        assert!(line.contains(EXECUTOR_SHA256), "no hash: {line}");
    }
}
