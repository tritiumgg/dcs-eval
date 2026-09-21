//! The live ledger: one line of JSON per measured row, appended to
//! `live.jsonl` in the data directory and never rewritten.
//!
//! A phase run at the game appends what it measured and nothing else, and
//! the report reads the whole file back and takes the latest entry for each
//! row. Appending is the only write, so a later session adds to what an
//! earlier one took rather than replacing it, and a figure taken on one DCS
//! build is still there after the next.
//!
//! **A line that will not parse is handed back, never skipped.** The report
//! prints it as a problem. A reader that dropped a bad line would print the
//! row it held as unmeasured, and a measurement that happened would read as
//! one that did not.

use std::fs;
use std::io::{self, Write as _};
use std::time::SystemTime;

use serde_json::{Map, Value};

use crate::register::{DataDir, stamp};

/// One measured row: which row, what it came to in words, the figures
/// behind the words, and the session and scene it was taken in.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// When it was appended, as the run record stamps a line.
    pub at: String,
    /// The phase that took it: `rtt`, `dormant`, `scene` or `read`.
    pub phase: String,
    /// The row's key in the report's table.
    pub row: String,
    /// The label the maintainer gave the scene, where the phase takes one.
    pub scene: Option<String>,
    /// The executor session's stamp, which is what "one DCS session" means.
    pub stamp: String,
    /// `hook` or `export`.
    pub host: String,
    /// The DCS build the session reported, where it reported one.
    pub app_version: Option<String>,
    /// What the row prints.
    pub said: String,
    /// The numbers and raw answers behind `said`, kept so a later reader can
    /// recompute rather than re-parse prose.
    pub figures: Map<String, Value>,
}

impl Entry {
    /// An entry stamped now, with nothing in its figures yet.
    pub fn new(phase: &str, row: &str, session: &Session, said: impl Into<String>) -> Self {
        Self {
            at: stamp(SystemTime::now()),
            phase: phase.to_owned(),
            row: row.to_owned(),
            scene: session.scene.clone(),
            stamp: session.stamp.clone(),
            host: session.host.clone(),
            app_version: session.app_version.clone(),
            said: said.into(),
            figures: Map::new(),
        }
    }

    /// The same entry with one more figure.
    #[must_use]
    pub fn with(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.figures.insert(name.to_owned(), value.into());
        self
    }

    /// The entry as one line of JSON, without its newline.
    pub fn line(&self) -> String {
        let text = |v: &Option<String>| v.clone().map_or(Value::Null, Value::String);
        let mut row = Map::new();
        row.insert("at".to_owned(), Value::String(self.at.clone()));
        row.insert("phase".to_owned(), Value::String(self.phase.clone()));
        row.insert("row".to_owned(), Value::String(self.row.clone()));
        row.insert("scene".to_owned(), text(&self.scene));
        row.insert("stamp".to_owned(), Value::String(self.stamp.clone()));
        row.insert("host".to_owned(), Value::String(self.host.clone()));
        row.insert("app_version".to_owned(), text(&self.app_version));
        row.insert("said".to_owned(), Value::String(self.said.clone()));
        row.insert("figures".to_owned(), Value::Object(self.figures.clone()));
        Value::Object(row).to_string()
    }

    /// An entry read back from one line, or nothing where the line is not
    /// one this module wrote.
    pub fn parse(line: &str) -> Option<Self> {
        let Value::Object(row) = serde_json::from_str::<Value>(line).ok()? else {
            return None;
        };
        let text = |name: &str| row.get(name)?.as_str().map(str::to_owned);
        let maybe = |name: &str| match row.get(name)? {
            Value::Null => Some(None),
            Value::String(s) => Some(Some(s.clone())),
            _ => None,
        };
        Some(Self {
            at: text("at")?,
            phase: text("phase")?,
            row: text("row")?,
            scene: maybe("scene")?,
            stamp: text("stamp")?,
            host: text("host")?,
            app_version: maybe("app_version")?,
            said: text("said")?,
            figures: row.get("figures")?.as_object()?.clone(),
        })
    }
}

/// What every entry of one invocation shares: the session it spoke to and
/// the scene it was told it was in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub stamp: String,
    pub host: String,
    pub app_version: Option<String>,
    pub scene: Option<String>,
}

/// Append one entry, making the directory if it is not there.
///
/// One write of the line and its newline together, as the run record does,
/// so a line is whole or absent.
pub fn append(data: &DataDir, entry: &Entry) -> io::Result<()> {
    fs::create_dir_all(data.path().as_path())?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(data.live_path())?;
    file.write_all(format!("{}\n", entry.line()).as_bytes())
}

/// Every entry in the ledger, oldest first, and every line that is not one.
///
/// A ledger that was never written is no entries: nothing has been measured
/// yet, which is a fact. A ledger that exists and cannot be read is an
/// error, because "nothing measured" would be a claim this side cannot make.
pub fn read(data: &DataDir) -> io::Result<(Vec<Entry>, Vec<String>)> {
    let text = match fs::read_to_string(data.live_path()) {
        Ok(text) => text,
        Err(why) if why.kind() == io::ErrorKind::NotFound => String::new(),
        Err(why) => return Err(why),
    };
    let mut entries = Vec::new();
    let mut malformed = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        match Entry::parse(line) {
            Some(entry) => entries.push(entry),
            None => malformed.push(line.to_owned()),
        }
    }
    Ok((entries, malformed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Sandbox;

    fn data(b: &Sandbox) -> DataDir {
        DataDir::at(&b.dir("data"), &[]).expect("the data directory resolves")
    }

    fn session() -> Session {
        Session {
            stamp: "0000000001-abcd".to_owned(),
            host: "hook".to_owned(),
            app_version: Some("2.9.28.26385".to_owned()),
            scene: Some("menu".to_owned()),
        }
    }

    #[test]
    fn appends_one_line_per_entry_and_keeps_the_earlier() {
        let b = Sandbox::new();
        let data = data(&b);
        let first =
            Entry::new("rtt", "round_trip.hook.hook", &session(), "p50 1 ms").with("p50_ms", 1.0);
        let second = Entry::new("rtt", "round_trip.hook.hook", &session(), "p50 2 ms");
        append(&data, &first).expect("the first appends");
        append(&data, &second).expect("the second appends");
        let (entries, malformed) = read(&data).expect("the ledger reads");
        assert_eq!(entries, vec![first, second]);
        assert!(malformed.is_empty(), "{malformed:?}");
    }

    #[test]
    fn a_malformed_line_is_handed_back_not_skipped() {
        let b = Sandbox::new();
        let data = data(&b);
        let entry = Entry::new("dormant", "dormant.dormant", &session(), "0.0002 ms");
        append(&data, &entry).expect("the entry appends");
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(data.live_path())
            .expect("the ledger opens");
        file.write_all(b"{\"row\": \"half a line\n")
            .expect("the bad line lands");
        let (entries, malformed) = read(&data).expect("the ledger reads");
        assert_eq!(entries, vec![entry]);
        assert_eq!(malformed, vec!["{\"row\": \"half a line".to_owned()]);
    }

    #[test]
    fn a_ledger_never_written_is_no_entries() {
        let b = Sandbox::new();
        let (entries, malformed) = read(&data(&b)).expect("an absent ledger reads");
        assert!(entries.is_empty() && malformed.is_empty());
    }
}
