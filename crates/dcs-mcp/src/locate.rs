//! Where `Saved Games` is, which `DCS*` variant under it the installer means,
//! and which trees a variant may not be in.
//!
//! Three separate questions, kept separate on purpose. The shell answers the
//! first, because the folder can be relocated and a path built out of the
//! profile then names a directory nothing is in. The second has no answer
//! this code may invent: a machine carrying stable and open beta side by side
//! is an ambiguity, and an ambiguity is put to whoever is running the
//! installer, never settled by taking the first entry. The third is a
//! refusal — a variant that resolves under the DCS install, or out of
//! `Saved Games` altogether, is the wrong tree however it was spelled.
//!
//! Nothing here asks, prompts, picks or writes. It reports what is there and
//! why a thing is refused; the asking is the installer's verb, and the root
//! comes in as an argument so a test can hand it a fixture directory.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use dcs_eval::paths::{self, PathError, Real};

use crate::sys;

/// A directory that holds DCS write directories: what the shell called
/// `Saved Games`, or whatever a caller passed instead.
#[derive(Clone, Debug)]
pub struct SavedGames {
    root: Real,
}

/// One `DCS*` directory under the root, with the name it is spelled with on
/// this machine and the path the filesystem says it really is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub name: String,
    pub path: Real,
}

impl SavedGames {
    /// The shell's answer, resolved.
    pub fn known() -> Result<Self, LocateError> {
        let path = sys::saved_games().map_err(LocateError::NoKnownFolder)?;
        Self::at(&path)
    }

    /// A root the caller supplies — the `--saved-games` override, or a
    /// fixture. The caller supplying the root is how the rest of this build
    /// is arranged; nothing here goes looking on its own.
    pub fn at(path: &Path) -> Result<Self, LocateError> {
        Ok(Self {
            root: paths::resolve(path)?,
        })
    }

    /// Every `DCS*` directory directly under the root, sorted by folded
    /// name so that a refusal naming them prints the same order twice.
    ///
    /// The prefix is matched with ASCII case folded, because Windows does:
    /// `dcs.openbeta` is the same directory as `DCS.openbeta` and a match
    /// that missed it would report an ambiguity as a single variant.
    pub fn variants(&self) -> Result<Vec<Variant>, LocateError> {
        let mut found = Vec::new();
        let entries =
            std::fs::read_dir(self.root.as_path()).map_err(|why| LocateError::Unreadable {
                root: self.root.as_path().to_owned(),
                why,
            })?;
        for entry in entries {
            let entry = entry.map_err(|why| LocateError::Unreadable {
                root: self.root.as_path().to_owned(),
                why,
            })?;
            // A name Windows will not spell back as UTF-8 keeps its entry
            // rather than being skipped, and the replacement characters it
            // prints with are accepted. Skipping it would drop a directory
            // out of the count, and the count is what makes two variants an
            // ambiguity rather than a silent pick; a name that prints oddly
            // costs a confusing line, which is the smaller harm. The path
            // is unaffected — it comes from the entry, not from this.
            let name = entry.file_name().to_string_lossy().into_owned();
            let bytes = name.as_bytes();
            if bytes.len() < 3 || !bytes[..3].eq_ignore_ascii_case(b"DCS") {
                continue;
            }
            // A stat rather than the entry's own file type, so a junction
            // standing where a write directory should be is judged by what
            // it points at — which is the case the containment rule below
            // exists for.
            if !entry.path().is_dir() {
                continue;
            }
            found.push(Variant {
                path: paths::resolve(&entry.path())?,
                name,
            });
        }
        found.sort_by_key(|v| v.name.to_ascii_lowercase());
        Ok(found)
    }

    /// The one variant the installer means, or why there is not one.
    ///
    /// `wanted` is the answer to the ambiguity — `--variant`, or whatever
    /// the caller asked — and it filters before anything is counted, so
    /// naming one of two is not an ambiguity at all. With no answer in
    /// hand, two variants are refused: which one is meant is a question for
    /// whoever runs the installer, and taking the first would be answering
    /// it for them — into the one directory this build then writes to.
    pub fn target(
        &self,
        wanted: Option<&str>,
        install: Option<&Real>,
    ) -> Result<Variant, LocateError> {
        let all = self.variants()?;
        let mut found = match wanted {
            Some(name) => {
                let picked: Vec<Variant> = all
                    .iter()
                    .filter(|v| v.name.eq_ignore_ascii_case(name))
                    .cloned()
                    .collect();
                if picked.is_empty() {
                    return Err(LocateError::NoSuchVariant {
                        root: self.root.clone(),
                        wanted: name.to_owned(),
                        variants: all,
                    });
                }
                picked
            }
            None => all,
        };
        if found.is_empty() {
            return Err(LocateError::NoVariant {
                root: self.root.clone(),
            });
        }
        if found.len() > 1 {
            return Err(LocateError::Ambiguous {
                root: self.root.clone(),
                variants: found,
            });
        }
        self.judged(found.remove(0), install)
    }

    /// The wrong-tree rules, in the order the client's own file judgement
    /// takes them: the install first, because a caller whose roots are wide
    /// enough to cover it should be told which rule really refused, and
    /// containment last.
    ///
    /// Both are decided on the resolved path, which is what makes a
    /// junction out of `Saved Games` refusable: textually it is under the
    /// root, and the directory it names is not.
    fn judged(&self, variant: Variant, install: Option<&Real>) -> Result<Variant, LocateError> {
        if let Some(install) = install
            && install.contains(&variant.path)
        {
            return Err(LocateError::InsideInstall {
                variant: variant.path,
                install: install.clone(),
            });
        }
        if !self.root.contains(&variant.path) {
            return Err(LocateError::OutsideSavedGames {
                variant: variant.path,
                root: self.root.clone(),
            });
        }
        Ok(variant)
    }
}

/// Why the installer has no variant to work on. `Display` is the project's
/// `<path>: <reason>`, so one line reads the same wherever it was raised.
#[derive(Debug)]
pub enum LocateError {
    NoKnownFolder(io::Error),
    Unreadable {
        root: PathBuf,
        why: io::Error,
    },
    Path(PathError),
    NoVariant {
        root: Real,
    },
    Ambiguous {
        root: Real,
        variants: Vec<Variant>,
    },
    NoSuchVariant {
        root: Real,
        wanted: String,
        variants: Vec<Variant>,
    },
    InsideInstall {
        variant: Real,
        install: Real,
    },
    OutsideSavedGames {
        variant: Real,
        root: Real,
    },
}

impl From<PathError> for LocateError {
    fn from(why: PathError) -> Self {
        Self::Path(why)
    }
}

/// The variants' names on one line, which is what a question with both
/// answers in it needs.
fn names(variants: &[Variant]) -> String {
    variants
        .iter()
        .map(|v| v.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

impl fmt::Display for LocateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoKnownFolder(why) => write!(f, "Saved Games: {why}"),
            Self::Unreadable { root, why } => write!(f, "{}: {why}", root.display()),
            Self::Path(why) => why.fmt(f),
            Self::NoVariant { root } => {
                write!(f, "{root}: holds no DCS write directory")
            }
            Self::Ambiguous { root, variants } => write!(
                f,
                "{root}: holds {} DCS write directories ({}), and which one is meant is a \
                 question rather than a guess",
                variants.len(),
                names(variants)
            ),
            Self::NoSuchVariant {
                root,
                wanted,
                variants,
            } if variants.is_empty() => {
                // A name asked for in a root holding nothing reaches this
                // arm rather than `NoVariant`, because the filter runs
                // before anything is counted. Without this the line would
                // offer "only " and then stop.
                write!(
                    f,
                    "{root}: holds no {wanted}, and no DCS write directory at all"
                )
            }
            Self::NoSuchVariant {
                root,
                wanted,
                variants,
            } => write!(f, "{root}: holds no {wanted}, only {}", names(variants)),
            Self::InsideInstall { variant, install } => write!(
                f,
                "{variant}: lies under the DCS install at {install}, which is read-only, always"
            ),
            Self::OutsideSavedGames { variant, root } => {
                write!(
                    f,
                    "{variant}: is not under {root}, whatever it is spelled as"
                )
            }
        }
    }
}

impl std::error::Error for LocateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoKnownFolder(why) | Self::Unreadable { why, .. } => Some(why),
            Self::Path(why) => Some(why),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fresh directory under the host's temp directory, gone when the
    /// test ends. `dcs_eval::testing` is the crate's own test module and
    /// is not reachable from here, so this is the same shape written
    /// again rather than shared: two other lanes are editing this crate,
    /// and whoever needs it second is the one who should lift it out.
    struct Sandbox {
        path: PathBuf,
    }

    impl Sandbox {
        fn new() -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("dcs-mcp-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("the box is made");
            Self { path }
        }

        fn join(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }

        fn dir(&self, name: &str) -> PathBuf {
            let path = self.join(name);
            fs::create_dir_all(&path).expect("the directory is made");
            path
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    /// A directory junction at `link` onto `target`. Junctions need no
    /// privilege, which is why the wrong-tree controls use one. It panics
    /// with `mklink`'s own words rather than skipping: a control about a
    /// junction that quietly had none to follow proves nothing.
    fn junction(link: &Path, target: &Path) {
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("cmd")
            .raw_arg(format!(
                "/C mklink /J \"{}\" \"{}\"",
                link.display(),
                target.display()
            ))
            .output()
            .expect("cmd runs");
        assert!(
            out.status.success(),
            "mklink /J {} {}: {} {}",
            link.display(),
            target.display(),
            String::from_utf8_lossy(&out.stdout).trim(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        assert!(link.is_dir(), "the junction is there: {}", link.display());
    }

    /// Every expectation compares resolved paths, never the spelling that
    /// made them: the host's temp directory is usually spelled short.
    fn real(path: &Path) -> Real {
        paths::resolve(path).expect("the path resolves")
    }

    #[test]
    fn two_variants_are_an_ambiguity_carrying_both() {
        let b = Sandbox::new();
        b.dir("DCS");
        b.dir("DCS.openbeta");
        let sg = SavedGames::at(&b.path).expect("the root resolves");
        let err = sg
            .target(None, None)
            .expect_err("two variants are not a target");
        let LocateError::Ambiguous { variants, .. } = &err else {
            panic!("the refusal is the ambiguity, not {err}");
        };
        assert_eq!(variants.len(), 2, "both are reported: {err}");
        assert_eq!(
            variants.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(),
            vec!["DCS", "DCS.openbeta"],
            "in a stable order"
        );
        let line = err.to_string();
        assert!(line.contains("DCS.openbeta"), "the line names both: {line}");
        assert!(
            line.contains("DCS,"),
            "the line names the other one too: {line}"
        );
    }

    #[test]
    fn a_single_variant_is_the_target() {
        let b = Sandbox::new();
        let wanted = b.dir("DCS.openbeta");
        b.dir("Other");
        // A file whose name begins DCS: the match is on directories.
        fs::write(b.join("DCS.txt"), b"not a write directory").expect("the decoy file");
        // An unrelated install, to show the wrong-tree guard does not fire
        // on every call — only on a variant that is really in it.
        let install = real(&b.dir("install"));
        let sg = SavedGames::at(&b.path).expect("the root resolves");
        let target = sg
            .target(None, Some(&install))
            .expect("one variant is the target");
        assert_eq!(target.path, real(&wanted));
        assert_eq!(target.name, "DCS.openbeta");
    }

    #[test]
    fn a_lowercase_spelling_is_still_a_variant() {
        let b = Sandbox::new();
        let wanted = b.dir("dcs.openbeta");
        let sg = SavedGames::at(&b.path).expect("the root resolves");
        let target = sg.target(None, None).expect("Windows folds the case");
        assert_eq!(target.path, real(&wanted));
    }

    #[test]
    fn an_empty_root_is_refused_naming_it() {
        let b = Sandbox::new();
        b.dir("Other");
        let sg = SavedGames::at(&b.path).expect("the root resolves");
        let err = sg.target(None, None).expect_err("nothing to install into");
        assert!(matches!(err, LocateError::NoVariant { .. }), "{err}");
        assert!(
            err.to_string().contains(&real(&b.path).to_string()),
            "the line names the root: {err}"
        );
    }

    #[test]
    fn a_named_variant_settles_the_ambiguity() {
        let b = Sandbox::new();
        b.dir("DCS");
        let wanted = b.dir("DCS.openbeta");
        let sg = SavedGames::at(&b.path).expect("the root resolves");
        let target = sg
            .target(Some("DCS.openbeta"), None)
            .expect("the answer came in, so there is no question left");
        assert_eq!(target.path, real(&wanted));
    }

    #[test]
    fn a_named_variant_that_is_not_there_is_refused_listing_what_is() {
        let b = Sandbox::new();
        b.dir("DCS");
        b.dir("DCS.openbeta");
        let sg = SavedGames::at(&b.path).expect("the root resolves");
        let err = sg
            .target(Some("DCS.dedicated"), None)
            .expect_err("no such variant");
        assert!(matches!(err, LocateError::NoSuchVariant { .. }), "{err}");
        let line = err.to_string();
        for named in ["DCS.dedicated", "DCS.openbeta", "DCS,"] {
            assert!(line.contains(named), "the line names {named}: {line}");
        }
    }

    #[test]
    fn a_named_variant_in_an_empty_root_offers_no_empty_list() {
        let b = Sandbox::new();
        b.dir("Other");
        let sg = SavedGames::at(&b.path).expect("the root resolves");
        // The filter runs before anything is counted, so naming a variant
        // in a root that holds none is this refusal and not `NoVariant`.
        let err = sg.target(Some("DCS"), None).expect_err("no such variant");
        assert!(matches!(err, LocateError::NoSuchVariant { .. }), "{err}");
        let line = err.to_string();
        assert!(
            !line.contains("only"),
            "a refusal with nothing to list does not offer a list: {line}"
        );
        assert!(
            line.contains("DCS"),
            "it still names what was asked for: {line}"
        );
    }

    #[test]
    fn a_variant_under_the_install_is_refused_naming_the_install() {
        let b = Sandbox::new();
        let install = b.dir("install");
        let inside = b.dir("install/DCS");
        let saved = b.dir("saved");
        junction(&saved.join("DCS"), &inside);
        // The attack, spelled out: textually the variant is under the
        // root the caller allowed.
        assert!(
            saved.join("DCS").starts_with(&saved),
            "the spelling is inside Saved Games, which is what makes this worth refusing"
        );
        let sg = SavedGames::at(&saved).expect("the root resolves");
        let err = sg
            .target(None, Some(&real(&install)))
            .expect_err("the install is read-only, always");
        assert!(matches!(err, LocateError::InsideInstall { .. }), "{err}");
        assert!(
            err.to_string().contains(&real(&install).to_string()),
            "the line names the install: {err}"
        );
    }

    #[test]
    fn a_variant_resolving_out_of_saved_games_is_refused() {
        let b = Sandbox::new();
        let elsewhere = b.dir("elsewhere");
        let saved = b.dir("saved");
        junction(&saved.join("DCS"), &elsewhere);
        let sg = SavedGames::at(&saved).expect("the root resolves");
        // No install is passed, so the rule that fired is the containment
        // one and nothing else.
        let err = sg
            .target(None, None)
            .expect_err("that is not in Saved Games");
        assert!(
            matches!(err, LocateError::OutsideSavedGames { .. }),
            "{err}"
        );
        assert!(
            err.to_string().contains(&real(&saved).to_string()),
            "the line names the root it is not under: {err}"
        );
    }

    #[test]
    fn the_known_root_is_what_the_shell_answered() {
        // The one test here that touches the real machine — every other
        // one runs against a fixture root, so this is the whole of what
        // `cargo test -p dcs-mcp locate` asks of the host: that the known
        // folder resolves at all.
        //
        // It asserts shape and agreement, and neither is a proof that the
        // path came from the shell: on a host whose known folder has never
        // been relocated the answer is indistinguishable from the string
        // this must never build out of the profile, and `known()` is `at()`
        // over `sys::saved_games()` by construction, so it would agree with
        // that call whatever the call returned. What actually holds the
        // property is that the declaration in `sys` is the only source the
        // locator has. This says the wiring is present and the answer is
        // one the client can resolve.
        let path = sys::saved_games().expect("the shell says where Saved Games is");
        assert!(path.is_absolute(), "an absolute path: {}", path.display());
        let sg = SavedGames::known().expect("and the locator resolves what it said");
        assert_eq!(sg.root, real(&path));
    }
}
