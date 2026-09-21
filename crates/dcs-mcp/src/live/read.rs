//! The read phase: the six reads the live run sent alone before every
//! game-state sent them, each sent alone, and whatever each came to written
//! down.
//!
//! Alone means the only request on the disk: no ping, no other read and no
//! probe beside it. A session is one executor stamp, which changes when DCS
//! restarts.
//!
//! Two ways in. `live read <key>` sends one read into a session that has had
//! none, which is the clean case: a session that goes down holding one
//! request was taken down by that request, and nothing earlier in it can
//! have set the crash up. `live read all` sends every read that has no
//! outcome yet in the scene `--label` names, one after another in one session, and stops at the first
//! that does not answer (ADR 0028). Sent in turn, each still alone on the
//! disk, a crash still names the read in flight; what a sequence cannot rule
//! out is damage an earlier read did that surfaced later, so the one that
//! stopped it is retested alone in a fresh session before it is believed.
//!
//! **This phase refuses a read only to keep a crash attributable; it bans
//! none.** Either way in is refused in a session that already had a read,
//! because a crash then could name neither. And a ledger that cannot be read
//! in full, or cannot be written, refuses the read too, because the phase
//! could not then prove the session had not had one, or could not make the
//! next run see this one.
//!
//! The read is written to the ledger as sent *before* it is published, and
//! its outcome is appended after. A read that was sent and never answered
//! still counts as sent: it is on the disk, and it will run. Written only
//! after the answer, a run stopped during the wait — interrupted, or killed
//! with a hung game — would leave no trace, and the next run would admit a
//! second read into the same session.

use std::io::Write;
use std::time::Duration;

use dcs_eval::readers::Handshake;
use dcs_eval::reads::{self, Answer, Unanswered};

use super::ledger::{self, Entry, Session};
use crate::register::DataDir;

/// Refuse a read in a session the ledger says already had one.
fn refuse_a_second(rows: &[Entry], stamp: &str) -> Result<(), String> {
    match rows.iter().find(|e| e.phase == "read" && e.stamp == stamp) {
        Some(e) => Err(format!(
            "this DCS session ({stamp}) already had an opt-in read, {} at {}; restart DCS \
             before the next, so a crash names one read",
            e.row, e.at
        )),
        None => Ok(()),
    }
}

/// The reads this phase sends, in the order the live run sent them. The
/// seventh, `mission_loaded`, crashed DCS and left the reads table, so it
/// can no longer be sent from here (ADR 0031).
const KEYS: [&str; 6] = [
    "multiplayer",
    "server",
    "track",
    "player_id",
    "player_unit_type",
    "mission_theatre",
];

/// The key, if it names one of the reads this phase sends.
fn opt_in(key: &str) -> Result<(), String> {
    if KEYS.contains(&key) {
        return Ok(());
    }
    Err(format!(
        "live read takes one of {}, not {key}",
        KEYS.join(", ")
    ))
}

/// What an answer says, in a row's words.
fn said(answer: &Answer) -> String {
    match answer {
        Answer::Value { lua_type, value } => match value {
            Some(value) => format!("answered: {lua_type} {value}"),
            None => format!("answered: a {lua_type}"),
        },
        Answer::Raised { message } => format!("raised inside its pcall: {message}"),
        Answer::Malformed { body } => {
            let text: String = String::from_utf8_lossy(body).chars().take(80).collect();
            format!("malformed: {text}")
        }
        Answer::Unanswered {
            why: Unanswered::Dead { id } | Unanswered::Superseded { id },
        } => format!(
            "session gone before it answered, id {id}: the executor's events log holds its \
             opening marker with no closing one"
        ),
        Answer::Unanswered {
            why: Unanswered::Pending { id, .. },
        } => format!("sent and not yet answered, id {id}; it counts as this session's read"),
        Answer::Unanswered { why } => why.to_string(),
        Answer::NotSent { why } => format!("not sent: {why}"),
    }
}

/// What the ledger says of a read between its sending and its answer, and
/// ever after if no answer is appended.
const SENT: &str = "sent, and no answer recorded: the instrument stopped before one came \
                    back; it counts as this session's read";

/// Send one opt-in read, alone, if the ledger says this session has had
/// none, having first written it down as sent.
///
/// # Errors
///
/// The refusal, in words, where the key names no opt-in read, the session
/// already had one, or the ledger will not take the entry that says it was
/// sent. Nothing is published then.
pub(crate) fn run(
    h: &Handshake,
    session: &Session,
    key: &str,
    rows: Vec<Entry>,
    data: &DataDir,
    upto: Duration,
) -> Result<Vec<Entry>, String> {
    opt_in(key)?;
    let stamp = session.stamp.as_str();
    refuse_a_second(&rows, stamp)?;
    let (entry, _) = send(h, session, key, data, upto)?;
    Ok(vec![entry])
}

/// Whether the ledger already holds what `key` came to in `scene`, in any
/// session.
///
/// Scene by scene, because a read that answers in a settled mission may not
/// at the menu, where the objects it reads are not there, and the reason a
/// read is tested is to find that out. No label is a scene of its own.
///
/// The entry written before a read is sent is not an outcome, and neither
/// is one saying it was refused before the disk: a key with only those
/// never came to anything, and is sent again.
fn has_outcome(rows: &[Entry], key: &str, scene: Option<&str>) -> bool {
    let row = format!("read.{key}");
    rows.iter().any(|e| {
        e.phase == "read"
            && e.row == row
            && e.scene.as_deref() == scene
            && e.said != SENT
            && !e.said.starts_with("not sent:")
    })
}

/// Send every opt-in read with no outcome yet in this session's scene, one
/// after another in this session, each alone and each written down before and after, stopping at
/// the first that does not answer. The exit code: 0 when every read sent
/// answered, 1 when one did not.
///
/// A read that raises inside its pcall, or answers in a shape nobody
/// expected, answered: the session is up and the next is sent. One that
/// never answers stops the run, because the session may be gone, and a read
/// sent after it would be sent into a game nobody can vouch for.
///
/// # Errors
///
/// The refusal, in words, where the session already had a read, a read was
/// refused before the disk, or the ledger will not take an entry.
pub(crate) fn run_all(
    h: &Handshake,
    session: &Session,
    rows: &[Entry],
    data: &DataDir,
    upto: Duration,
    out: &mut dyn Write,
) -> Result<i32, String> {
    refuse_a_second(rows, &session.stamp)?;
    let todo: Vec<&str> = KEYS
        .into_iter()
        .filter(|key| !has_outcome(rows, key, session.scene.as_deref()))
        .collect();
    let say =
        |out: &mut dyn Write, line: String| writeln!(out, "{line}").map_err(|why| why.to_string());
    if todo.is_empty() {
        say(
            out,
            "every opt-in read already has an outcome in the ledger; nothing was sent".into(),
        )?;
        return Ok(0);
    }
    for key in todo {
        let (entry, answered) = send(h, session, key, data, upto)?;
        ledger::append(data, &entry).map_err(|why| {
            format!(
                "{}: {why}, after {} was sent and came to: {}",
                data.live_path().display(),
                entry.row,
                entry.said
            )
        })?;
        say(out, format!("{}: {}", entry.row, entry.said))?;
        if !answered {
            say(
                out,
                format!(
                    "stopped: {key} did not answer, so nothing after it was sent; restart DCS, \
                     confirm it alone with `live read {key}`, then `live read all` for the rest"
                ),
            )?;
            return Ok(1);
        }
    }
    Ok(0)
}

/// Send one opt-in read, alone, having first written it down as sent: what
/// it came to, and whether the session answered at all.
fn send(
    h: &Handshake,
    session: &Session,
    key: &str,
    data: &DataDir,
    upto: Duration,
) -> Result<(Entry, bool), String> {
    let row = format!("read.{key}");
    ledger::append(data, &Entry::new("read", &row, session, SENT)).map_err(|why| {
        format!(
            "{}: {why}, so the read was not sent: the next run could not see it",
            data.live_path().display()
        )
    })?;
    let answer = match reads::alone(h, key, upto) {
        Ok(answer) => answer,
        Err(why) => {
            // Refused before the disk, so the entry above overstates it, and
            // this one corrects what the report prints, best effort. Either
            // still refuses a second read in this session, which is a read
            // that would have been safe refused: the right way to err.
            let not_sent = Entry::new("read", &row, session, format!("not sent: {why}"));
            let _ = ledger::append(data, &not_sent);
            return Err(why.to_string());
        }
    };
    let answered = matches!(
        answer,
        Answer::Value { .. } | Answer::Raised { .. } | Answer::Malformed { .. }
    );
    Ok((Entry::new("read", &row, session, said(&answer)), answered))
}

#[cfg(test)]
mod tests {
    use crate::live::ledger;
    use crate::register::DataDir;
    use crate::serve::{Host, Options};
    use crate::testing::{Sandbox, driven, ticking};
    use dcs_eval::standin::Standin;

    fn standin(b: &Sandbox, stamp: &str) -> Standin {
        let opts = Options {
            saved_games: b.path.clone(),
            variant: "DCS".to_owned(),
            host: Host::Hook,
            data_dir: Some(b.dir("data")),
        };
        let mut s =
            Standin::open_stamped(&opts.output(), "hook", stamp).expect("the stand-in opens");
        ticking(&mut s);
        s
    }

    /// The ledger's entries.
    fn entries(b: &Sandbox) -> Vec<ledger::Entry> {
        let data = DataDir::at(&b.join("data"), &[]).expect("the data directory resolves");
        ledger::read(&data).expect("the ledger reads").0
    }

    /// `live read <key>` run against a stand-in: the exit code, what it
    /// printed, and the ledger after.
    fn read(b: &Sandbox, s: &mut Standin, key: &str) -> (i32, String, Vec<ledger::Entry>) {
        let line = line(b, key);
        let (code, shown) = driven(s, |out| {
            crate::live::run(line, out).expect("the line parses")
        });
        (code, shown, entries(b))
    }

    /// The command line that asks for `key`.
    fn line(b: &Sandbox, key: &str) -> Vec<String> {
        [
            "live",
            "read",
            key,
            "--wait-seconds",
            "5",
            "--saved-games",
            &b.path.to_string_lossy(),
            "--variant",
            "DCS",
            "--data-dir",
            &b.join("data").to_string_lossy(),
        ]
        .iter()
        .map(|w| (*w).to_owned())
        .collect()
    }

    #[test]
    fn a_second_opt_in_read_in_one_session_is_refused() {
        let b = Sandbox::new();
        let mut s = standin(&b, "0000000001-4242");
        let (first, _, _) = read(&b, &mut s, "multiplayer");
        assert_eq!(first, 0);
        let before = s.seen().len();
        let (second, shown, entries) = read(&b, &mut s, "server");
        assert_eq!(second, 1, "{shown}");
        assert!(shown.contains("already had an opt-in read"), "{shown}");
        assert_eq!(s.seen().len(), before, "the refused read reached the disk");
        assert_eq!(entries.len(), 2, "the first read, sent then answered");
    }

    #[test]
    fn a_new_session_admits_the_next_read() {
        let b = Sandbox::new();
        let mut s = standin(&b, "0000000001-4242");
        assert_eq!(read(&b, &mut s, "multiplayer").0, 0);
        drop(s);
        let mut s = standin(&b, "0000000002-4242");
        let (code, shown, entries) = read(&b, &mut s, "server");
        assert_eq!(code, 0, "{shown}");
        assert_eq!(entries.len(), 4);
    }

    #[test]
    fn the_read_is_the_only_request_published() {
        let b = Sandbox::new();
        let mut s = standin(&b, "0000000001-4242");
        s.script("DCS.getMissionTheatre", "ok", "string", b"string\tCaucasus");
        let (code, _, entries) = read(&b, &mut s, "mission_theatre");
        assert_eq!(code, 0);
        assert_eq!(s.seen().len(), 1, "the ledger holds {}", s.seen().len());
        assert_eq!(entries[1].row, "read.mission_theatre");
        assert_eq!(entries[1].said, "answered: string Caucasus");
    }

    #[test]
    fn the_read_is_on_the_ledger_before_it_is_on_the_disk() {
        use std::time::{Duration, Instant};
        let b = Sandbox::new();
        let mut s = standin(&b, "0000000001-4242");
        let line = line(&b, "server");
        let files = |s: &Standin| std::fs::read_dir(s.req()).map_or(0, Iterator::count);
        let before = files(&s);
        std::thread::scope(|scope| {
            let running =
                scope.spawn(|| crate::live::run(line, &mut Vec::new()).expect("the line parses"));
            let deadline = Instant::now() + Duration::from_secs(10);
            // Nothing is answered yet: the run is held in its wait, as one
            // interrupted there would be.
            while files(&s) == before {
                assert!(Instant::now() < deadline, "the read never reached the disk");
                std::thread::sleep(Duration::from_millis(5));
            }
            let held = entries(&b);
            assert_eq!(held.len(), 1, "{held:?}");
            assert_eq!(held[0].row, "read.server");
            assert!(
                held[0].said.starts_with("sent, and no answer"),
                "{}",
                held[0].said
            );
            while !running.is_finished() && Instant::now() < deadline {
                s.tick();
                std::thread::sleep(Duration::from_millis(25));
            }
            assert_eq!(running.join().expect("the phase does not panic"), 0);
        });
        assert_eq!(entries(&b).len(), 2, "the outcome is appended after");
    }

    #[test]
    fn a_key_it_does_not_send_is_refused_by_name() {
        let b = Sandbox::new();
        let mut s = standin(&b, "0000000001-4242");
        let (code, shown, entries) = read(&b, &mut s, "pause");
        assert_eq!(code, 1);
        assert!(shown.contains("live read takes one of"), "{shown}");
        assert!(s.seen().is_empty() && entries.is_empty());
    }

    #[test]
    fn a_gone_session_is_recorded_with_its_id() {
        let b = Sandbox::new();
        let mut s = standin(&b, "0000000001-4242");
        // A process id nobody is running, and a heartbeat that says dormant,
        // so the wait probes the process and finds it gone.
        s.pid = u32::MAX - 1;
        s.armed = false;
        s.handshake().expect("the handshake publishes");
        s.beat(std::time::SystemTime::now())
            .expect("the heartbeat publishes");
        // Run here with nothing ticking: a stand-in ticked alongside would
        // answer the read before the wait found the process gone.
        let code = crate::live::run(line(&b, "track"), &mut Vec::new()).expect("the line parses");
        assert_eq!(code, 0);
        let entries = entries(&b);
        assert_eq!(entries.len(), 2);
        assert!(
            entries[1]
                .said
                .starts_with("session gone before it answered, id "),
            "{}",
            entries[1].said
        );
    }

    /// The rows the ledger holds an entry for, in the order they landed,
    /// once each.
    fn rows_seen(entries: &[ledger::Entry]) -> Vec<String> {
        let mut rows: Vec<String> = Vec::new();
        for e in entries {
            if !rows.contains(&e.row) {
                rows.push(e.row.clone());
            }
        }
        rows
    }

    #[test]
    fn all_sends_every_opt_in_read_in_turn_each_alone() {
        let b = Sandbox::new();
        let mut s = standin(&b, "0000000001-4242");
        let (code, shown, entries) = read(&b, &mut s, "all");
        assert_eq!(code, 0, "{shown}");
        assert_eq!(
            s.seen().len(),
            6,
            "one request per read, and nothing beside"
        );
        assert_eq!(
            rows_seen(&entries),
            [
                "read.multiplayer",
                "read.server",
                "read.track",
                "read.player_id",
                "read.player_unit_type",
                "read.mission_theatre",
            ],
            "in the order the live run sent them"
        );
        assert_eq!(
            entries.len(),
            12,
            "each written down as sent, then as answered"
        );
    }

    #[test]
    fn all_stops_at_the_first_read_that_does_not_answer() {
        let b = Sandbox::new();
        let mut s = standin(&b, "0000000001-4242");
        // A process nobody is running and a dormant heartbeat: the first
        // read finds the session gone, as one that took DCS down would.
        s.pid = u32::MAX - 1;
        s.armed = false;
        s.handshake().expect("the handshake publishes");
        s.beat(std::time::SystemTime::now())
            .expect("the heartbeat publishes");
        let mut shown = Vec::new();
        let code = crate::live::run(line(&b, "all"), &mut shown).expect("the line parses");
        let shown = String::from_utf8_lossy(&shown);
        assert_eq!(code, 1, "{shown}");
        assert!(
            shown.contains("stopped: multiplayer did not answer"),
            "{shown}"
        );
        let entries = entries(&b);
        assert_eq!(
            rows_seen(&entries),
            ["read.multiplayer"],
            "nothing was sent after the read that did not answer"
        );
        assert!(
            entries[1]
                .said
                .starts_with("session gone before it answered"),
            "{}",
            entries[1].said
        );
    }

    #[test]
    fn all_skips_every_read_that_already_came_to_something() {
        let b = Sandbox::new();
        let mut s = standin(&b, "0000000001-4242");
        assert_eq!(read(&b, &mut s, "multiplayer").0, 0);
        drop(s);
        let mut s = standin(&b, "0000000002-4242");
        let (code, shown, entries) = read(&b, &mut s, "all");
        assert_eq!(code, 0, "{shown}");
        assert_eq!(s.seen().len(), 5, "multiplayer was not sent again");
        assert_eq!(entries.len(), 12);
        let (again, shown, _) = {
            drop(s);
            let mut s = standin(&b, "0000000003-4242");
            let got = read(&b, &mut s, "all");
            assert!(
                s.seen().is_empty(),
                "a read went out with nothing left to send"
            );
            got
        };
        assert_eq!(again, 0, "{shown}");
        assert!(shown.contains("nothing was sent"), "{shown}");
    }

    #[test]
    fn all_runs_again_in_a_scene_it_has_not_been_run_in() {
        let b = Sandbox::new();
        let labelled = |key: &str, label: &str| {
            let mut words = line(&b, key);
            words.extend(["--label".to_owned(), label.to_owned()]);
            words
        };
        let mut s = standin(&b, "0000000001-4242");
        let (code, shown) = driven(&mut s, |out| {
            crate::live::run(labelled("all", "menu"), out).expect("the line parses")
        });
        assert_eq!(code, 0, "{shown}");
        assert_eq!(s.seen().len(), 6);
        drop(s);

        let mut s = standin(&b, "0000000002-4242");
        let (code, shown) = driven(&mut s, |out| {
            crate::live::run(labelled("all", "mission"), out).expect("the line parses")
        });
        assert_eq!(code, 0, "{shown}");
        assert_eq!(
            s.seen().len(),
            6,
            "a result at the menu says nothing of a mission"
        );
        drop(s);

        let mut s = standin(&b, "0000000003-4242");
        let (code, shown) = driven(&mut s, |out| {
            crate::live::run(labelled("all", "mission"), out).expect("the line parses")
        });
        assert_eq!(code, 0, "{shown}");
        assert!(s.seen().is_empty(), "the mission's reads were sent twice");
        assert!(shown.contains("nothing was sent"), "{shown}");
    }

    #[test]
    fn all_is_refused_in_a_session_that_already_had_a_read() {
        let b = Sandbox::new();
        let mut s = standin(&b, "0000000001-4242");
        assert_eq!(read(&b, &mut s, "multiplayer").0, 0);
        let before = s.seen().len();
        let (code, shown, entries) = read(&b, &mut s, "all");
        assert_eq!(code, 1, "{shown}");
        assert!(shown.contains("already had an opt-in read"), "{shown}");
        assert_eq!(s.seen().len(), before, "a read reached the disk");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn an_unreadable_ledger_refuses_the_read() {
        let b = Sandbox::new();
        let mut s = standin(&b, "0000000001-4242");
        let data = DataDir::at(&b.join("data"), &[]).expect("the data directory resolves");
        std::fs::write(data.live_path(), b"{half a line\n").expect("the bad ledger lands");
        let (code, shown, _) = read(&b, &mut s, "player_id");
        assert_eq!(code, 1);
        assert!(shown.contains("cannot prove"), "{shown}");
        assert!(
            s.seen().is_empty(),
            "a read went out over a ledger nobody could read"
        );
    }
}
