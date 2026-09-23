//! The name a capture is asked for under: a caller's, checked and passed
//! through untouched, or one this crate supplies from the local clock.
//!
//! The name is all a caller can know in advance about the file DCS will
//! write, so it is the one thing this crate is careful with. DCS takes
//! whatever it is given and does not say what it did with it: a dot in the
//! name leaves a zero-byte file with no picture in it, and a separator
//! sends the file somewhere nobody has looked. So a caller's name is held
//! to letters, digits, `_` and `-`, one to sixty-four of them, and a name
//! that breaks the rule is refused with the character that broke it. It is
//! never repaired, because a repaired name puts a different path in the
//! answer from the one that was asked for.
//!
//! An empty name is refused too rather than sent. DCS names an empty
//! request itself, to the second, and the only way to find that file
//! afterwards is to list the directory and guess — which is the work a
//! named capture exists to remove. A caller who has no name gives none,
//! and gets [`supplied`].

use std::fmt;
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use crate::sys::{self, LocalTime};

/// The most characters a caller's name may have.
pub const LONGEST: usize = 64;

/// Why a caller's name was refused. Each says which character broke the
/// rule and where it sits, counting from one, so a caller can find it
/// without counting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameRefusal {
    /// No characters at all.
    Empty,
    /// A character outside `A-Z a-z 0-9 _ -`.
    Character { at: usize, character: char },
    /// The sixty-fifth character, which is one too many whatever it is.
    TooLong { character: char },
}

/// A character as a refusal names it: itself in quotes where it prints,
/// its code point where it would not show on the page.
fn shown(character: char) -> String {
    if character.is_control() || character.is_whitespace() {
        format!("U+{:04X}", u32::from(character))
    } else {
        format!("'{character}'")
    }
}

impl fmt::Display for NameRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(
                f,
                "the name is empty; a name is one to {LONGEST} characters of A-Z, a-z, 0-9, _ \
                 and -, and a capture asked for without one is named by the tool"
            ),
            Self::Character { at, character } => write!(
                f,
                "character {at} of the name is {}, and a name is only A-Z, a-z, 0-9, _ and -",
                shown(*character)
            ),
            Self::TooLong { character } => write!(
                f,
                "character {} of the name is {}, and a name is at most {LONGEST} characters",
                LONGEST + 1,
                shown(*character)
            ),
        }
    }
}

impl std::error::Error for NameRefusal {}

/// Whether a character may appear in a name.
fn allowed(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '-'
}

/// A caller's name held to the rule, and handed back exactly as given when
/// it keeps it. The first character to break the rule, reading from the
/// left, is the one the refusal names.
pub fn check(name: &str) -> Result<&str, NameRefusal> {
    if name.is_empty() {
        return Err(NameRefusal::Empty);
    }
    for (index, character) in name.chars().enumerate() {
        if !allowed(character) {
            return Err(NameRefusal::Character {
                at: index + 1,
                character,
            });
        }
        if index == LONGEST {
            return Err(NameRefusal::TooLong { character });
        }
    }
    Ok(name)
}

/// The name a capture goes out under: the caller's, checked, or the
/// tool's own where the caller gave none.
pub fn resolve(given: Option<&str>) -> Result<String, NameRefusal> {
    match given {
        Some(name) => check(name).map(str::to_owned),
        None => Ok(supplied()),
    }
}

/// The last clock reading a supplied name was taken from, in this
/// process.
static LAST: Mutex<Option<LocalTime>> = Mutex::new(None);

/// A name of the tool's own: `dcs-eval-YYYYMMDD-HHMMSS-mmm` in local time.
/// It sorts by the time it names, it says where it came from, and it
/// keeps the rule a caller's name is held to.
///
/// Two names taken back to back in this process never match. The local
/// clock moves on the system timer's tick, which can be fifteen
/// milliseconds, so two readings a moment apart often agree; a reading
/// equal to the last one issued is not used, and the call waits for the
/// clock to move instead. The name stays the time it was taken, and the
/// wait is at most a tick.
#[must_use]
pub fn supplied() -> String {
    supplied_from(&LAST, sys::local_now, || {
        thread::sleep(Duration::from_millis(1))
    })
}

/// [`supplied`] with the clock, the nap and the memory of the last reading
/// handed in, so a test can drive a clock that stands still.
fn supplied_from(
    last: &Mutex<Option<LocalTime>>,
    mut now: impl FnMut() -> LocalTime,
    mut nap: impl FnMut(),
) -> String {
    let mut last = last.lock().unwrap_or_else(PoisonError::into_inner);
    let mut reading = now();
    while *last == Some(reading) {
        nap();
        reading = now();
    }
    *last = Some(reading);
    rendered(reading)
}

/// A clock reading written as a supplied name.
fn rendered(reading: LocalTime) -> String {
    let LocalTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        millisecond,
    } = reading;
    format!("dcs-eval-{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}-{millisecond:03}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::HashSet;

    fn at(second: u16, millisecond: u16) -> LocalTime {
        LocalTime {
            year: 2026,
            month: 9,
            day: 3,
            hour: 7,
            minute: 5,
            second,
            millisecond,
        }
    }

    /// The refusal a name earns, as the caller would read it.
    fn refused(name: &str) -> String {
        check(name)
            .expect_err(&format!("{name:?} should be refused"))
            .to_string()
    }

    #[test]
    fn a_dot_is_refused_and_named() {
        let why = refused("verify_A01_shot9_c_plus0.3s");
        assert!(why.contains("character 25"), "{why}");
        assert!(why.contains("'.'"), "{why}");
    }

    #[test]
    fn a_backslash_is_refused_and_named() {
        let why = refused(r"shots\one");
        assert!(why.contains("character 6"), "{why}");
        assert!(why.contains(r"'\'"), "{why}");
    }

    #[test]
    fn a_forward_slash_is_refused_and_named() {
        let why = refused("shots/one");
        assert!(why.contains("character 6"), "{why}");
        assert!(why.contains("'/'"), "{why}");
    }

    #[test]
    fn an_empty_name_is_refused_as_empty() {
        let why = refused("");
        assert!(why.contains("the name is empty"), "{why}");
    }

    #[test]
    fn a_sixty_fifth_character_is_refused_and_named() {
        let name = format!("{}Z", "a".repeat(LONGEST));
        let why = refused(&name);
        assert!(why.contains("character 65"), "{why}");
        assert!(why.contains("'Z'"), "{why}");
        assert!(why.contains("at most 64"), "{why}");
    }

    #[test]
    fn sixty_four_characters_is_a_name() {
        let name = "a".repeat(LONGEST);
        assert_eq!(check(&name), Ok(name.as_str()));
    }

    #[test]
    fn a_character_that_would_not_show_is_named_by_its_code_point() {
        for (name, code) in [
            ("a b", "U+0020"),
            ("a\tb", "U+0009"),
            ("a\u{7f}b", "U+007F"),
        ] {
            let why = refused(name);
            assert!(why.contains(code), "{name:?}: {why}");
        }
    }

    #[test]
    fn a_letter_outside_ascii_is_refused_and_named() {
        let why = refused("caf\u{e9}");
        assert!(why.contains("character 4"), "{why}");
        assert!(why.contains("'\u{e9}'"), "{why}");
    }

    #[test]
    fn every_character_the_rule_admits_is_admitted() {
        let every = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
        assert_eq!(check(every), Ok(every));
    }

    #[test]
    fn a_callers_name_comes_back_unchanged() {
        assert_eq!(
            resolve(Some("Mig-29_Cockpit")),
            Ok("Mig-29_Cockpit".to_owned())
        );
        assert_eq!(
            resolve(Some("a.b")),
            Err(NameRefusal::Character {
                at: 2,
                character: '.'
            })
        );
    }

    #[test]
    fn no_name_given_is_a_supplied_one() {
        let name = resolve(None).expect("a supplied name");
        assert!(name.starts_with("dcs-eval-"), "{name}");
    }

    #[test]
    fn a_supplied_name_is_dated_to_the_millisecond() {
        assert_eq!(rendered(at(9, 42)), "dcs-eval-20260903-070509-042");
    }

    #[test]
    fn a_supplied_name_keeps_the_callers_rule() {
        let name = supplied();
        assert_eq!(check(&name), Ok(name.as_str()));
        assert_eq!(name.len(), "dcs-eval-YYYYMMDD-HHMMSS-mmm".len(), "{name}");
    }

    #[test]
    fn names_taken_in_immediate_succession_differ() {
        // The real clock, as fast as the calls will go. A name that lost
        // its milliseconds would repeat here, because ten names taken back
        // to back land in one second or, at worst, straddle two.
        let names: Vec<String> = (0..10).map(|_| supplied()).collect();
        let distinct: HashSet<&String> = names.iter().collect();
        assert_eq!(distinct.len(), names.len(), "{names:?}");
    }

    #[test]
    fn a_clock_that_has_not_moved_is_waited_for() {
        // A clock that reads the same millisecond three times running, the
        // way the system timer's tick makes it read, and then moves.
        let last = Mutex::new(None);
        let readings = [at(9, 42), at(9, 42), at(9, 42), at(9, 58)];
        let next = Cell::new(0);
        let naps = Cell::new(0);
        let now = || {
            let reading = readings[next.get()];
            next.set(next.get() + 1);
            reading
        };
        let first = supplied_from(&last, now, || naps.set(naps.get() + 1));
        let second = supplied_from(&last, now, || naps.set(naps.get() + 1));
        assert_eq!(first, "dcs-eval-20260903-070509-042");
        assert_eq!(second, "dcs-eval-20260903-070509-058");
        assert_eq!(naps.get(), 2, "one nap per reading that had not moved");
    }
}
