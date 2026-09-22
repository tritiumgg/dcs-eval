//! The report in words, at two levels of detail (ADR 0032).
//!
//! One renderer for a person and an agent both. The summary is what a person
//! reads first: a verdict, then a row per part with a plain status word and,
//! where there is something to do, the command that does it. The full report
//! is the same summary with every problem's exact sentence and every fact of
//! the session under it, so an agent reading it loses nothing a person's
//! summary leaves out, and the two can never word the same finding two ways
//! at the top.

use std::path::Path;
use std::time::Duration;

use dcs_eval::status::{self, Age, BeatStatus, Process, SessionStatus, VersionCheck};

use super::{GateFile, Hook, Line, Problem, Report};

/// How much of the report to print.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    /// The verdict and a row per part: what `verify` prints.
    Summary,
    /// The summary, every problem's exact sentence, and every fact: what
    /// `verify --verbose`, `status` and `dcs_status` print.
    Full,
}

/// A row's status word. ASCII words and no symbols, so that a console on
/// an OEM code page shows them as written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Word {
    Ok,
    Problem,
    Waiting,
    Note,
}

impl Word {
    fn as_str(self) -> &'static str {
        match self {
            Word::Ok => "ok",
            Word::Problem => "PROBLEM",
            Word::Waiting => "waiting",
            Word::Note => "note",
        }
    }
}

/// The parts, in the order their rows are printed.
const PARTS: [&str; 5] = ["hook", "Export.lua", "DCS", "DCS version", "autoexec.cfg"];

/// One row of the summary.
struct Row {
    word: Word,
    part: &'static str,
    text: String,
    fix: Option<String>,
}

impl Row {
    fn new(word: Word, part: &'static str, text: impl Into<String>) -> Self {
        Row {
            word,
            part,
            text: text.into(),
            fix: None,
        }
    }

    fn fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }
}

/// The report at the level of detail asked for, without a trailing newline.
pub fn render(report: &Report, detail: Detail) -> String {
    let mut lines = vec![verdict(report), String::new()];
    lines.extend(printed(&rows(report)));
    match detail {
        Detail::Summary => {
            if !report.verified() {
                lines.push(String::new());
                lines.push("run dcs-mcp verify --verbose for the exact findings".to_owned());
            }
        }
        Detail::Full => {
            let exact: Vec<String> = report
                .problems
                .iter()
                .map(ToString::to_string)
                .chain(report.session.problems.iter().map(ToString::to_string))
                .collect();
            lines.extend(exactly(&exact));
            lines.push(String::new());
            lines.push("details".to_owned());
            lines.extend(
                details(report)
                    .iter()
                    .map(|(key, value)| detail_line(key, value)),
            );
        }
    }
    lines.join("\n")
}

/// The session alone, in full: what `status` prints when the install could
/// not be looked at and only the output directory is left to read.
pub fn render_session(session: &status::Status) -> String {
    let mut lines = printed(&dcs_rows(session, None, false));
    let exact: Vec<String> = session.problems.iter().map(ToString::to_string).collect();
    lines.extend(exactly(&exact));
    lines.push(String::new());
    lines.push("details".to_owned());
    lines.extend(
        session_details(session, None)
            .iter()
            .map(|(key, value)| detail_line(key, value)),
    );
    lines.join("\n")
}

/// Every problem sentence as it has always been worded, under a heading of
/// its own, or nothing where there are none.
fn exactly(sentences: &[String]) -> Vec<String> {
    if sentences.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![String::new(), "problems (exact)".to_owned()];
    lines.extend(sentences.iter().map(|s| format!("  problem: {s}")));
    lines
}

/// One line of `details`: the key in a column, then its value.
pub(super) fn detail_line(key: &str, value: &str) -> String {
    format!("  {key:<23}{value}")
}

/// The first line: `verified` or `not verified`, and the folder it was
/// taken against.
fn verdict(report: &Report) -> String {
    let variant = report.variant.display();
    let verified = report.verified();
    if verified {
        return format!("verified: {variant}");
    }
    let found = report.problems.len() + report.session.problems.len();
    let install_missing = matches!(report.hook, Hook::Absent)
        && report.line != Line::Once
        && report.problems.iter().all(|p| {
            matches!(
                p,
                Problem::HookAbsent { .. }
                    | Problem::ExportFileAbsent { .. }
                    | Problem::ExportLineAbsent { .. }
            )
        })
        && report
            .session
            .problems
            .iter()
            .all(|p| matches!(p, status::Problem::NotInstalled { .. }));
    let installed = report.problems.is_empty();
    let headline = match report.session.problems.as_slice() {
        _ if install_missing => format!("dcs-eval is not installed in {variant}"),
        [status::Problem::NotInstalled { .. }] if installed => {
            format!("DCS has not yet loaded the executor installed in {variant}")
        }
        [status::Problem::ProcessGone { .. }] if installed => {
            format!("DCS is not running, and the executor is installed in {variant}")
        }
        _ if found == 1 => format!("1 problem in {variant}"),
        _ => format!("{found} problems in {variant}"),
    };
    format!("not verified: {headline}")
}

/// The rows, a line each, with a row's `fix:` under it. A fix the next row
/// repeats is printed once, under the last of them.
fn printed(rows: &[Row]) -> Vec<String> {
    let mut lines = Vec::new();
    for (at, row) in rows.iter().enumerate() {
        lines.push(format!(
            "  {:<9}{:<14}{}",
            row.word.as_str(),
            row.part,
            row.text
        ));
        if let Some(fix) = &row.fix {
            let repeated = rows.get(at + 1).and_then(|next| next.fix.as_ref()) == Some(fix);
            if !repeated {
                lines.push(format!("{:11}fix: {fix}", ""));
            }
        }
    }
    lines
}

/// `path` under the variant folder, or whole where it lies outside it.
fn relative(variant: &Path, path: &Path) -> String {
    path.strip_prefix(variant)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// Every row of the summary, in the order of [`PARTS`].
fn rows(report: &Report) -> Vec<Row> {
    let variant = &report.variant;
    let name = variant
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| variant.display().to_string());
    let install = format!("dcs-mcp install --variant {name}");
    let rel = |path: &Path| relative(variant, path);

    let mut rows = Vec::new();
    if matches!(report.hook, Hook::Ours { current: true, .. }) {
        rows.push(Row::new(
            Word::Ok,
            "hook",
            format!("{}, this release", rel(&report.hook_path)),
        ));
    }
    if report.line == Line::Once {
        rows.push(Row::new(Word::Ok, "Export.lua", "loads the executor once"));
    }
    for problem in &report.problems {
        rows.push(match problem {
            Problem::HookAbsent { path } => {
                Row::new(Word::Problem, "hook", format!("{} is missing", rel(path)))
                    .fix(install.clone())
            }
            Problem::HookUnreadable { path, why } => Row::new(
                Word::Problem,
                "hook",
                format!("{} could not be read: {why}", rel(path)),
            ),
            Problem::HookNotOurs { path, .. } => Row::new(
                Word::Problem,
                "hook",
                format!("{} is not a file dcs-eval shipped", rel(path)),
            )
            .fix(format!(
                "dcs-mcp install --replace --variant {name}  (moves it aside first)"
            )),
            Problem::HookNotTheCurrentRelease { path, .. } => Row::new(
                Word::Problem,
                "hook",
                format!("{} is an older dcs-eval release", rel(path)),
            )
            .fix(install.clone()),
            Problem::OtherHook { path } => Row::new(
                Word::Problem,
                "hook",
                format!(
                    "{} is a second copy of the executor, and DCS loads both",
                    rel(path)
                ),
            )
            .fix(format!("delete {}", rel(path))),
            Problem::ExportFileAbsent { path } => Row::new(
                Word::Problem,
                "Export.lua",
                format!("{} is missing", rel(path)),
            )
            .fix(install.clone()),
            Problem::ExportLineAbsent { path } => Row::new(
                Word::Problem,
                "Export.lua",
                format!("{} does not load the executor", rel(path)),
            )
            .fix(install.clone()),
            Problem::ExportLineRepeated { path, count } => Row::new(
                Word::Problem,
                "Export.lua",
                format!("loads the executor {count} times"),
            )
            .fix(format!(
                "delete all but one of those lines from {}",
                rel(path)
            )),
            Problem::ExportFileUnreadable { path, why } => Row::new(
                Word::Problem,
                "Export.lua",
                format!("{} could not be read: {why}", rel(path)),
            ),
            Problem::AutoexecUnreadable { path, why } => Row::new(
                Word::Problem,
                "autoexec.cfg",
                format!("{} could not be read: {why}", rel(path)),
            ),
        });
    }
    rows.extend(dcs_rows(&report.session, Some(variant), installed(report)));
    if let Some(row) = version_row(&report.app_version) {
        rows.push(row);
    }
    rows.extend(gate_rows(report));
    // Stable, so the rows of one part keep the order they were found in.
    rows.sort_by_key(|row| PARTS.iter().position(|part| *part == row.part));
    rows
}

/// Whether the install half found nothing, which is when "start DCS" is
/// the next thing to do.
fn installed(report: &Report) -> bool {
    report.problems.is_empty()
}

/// The DCS rows: one for the process, and one for each finding about the
/// session that the process row does not already say.
fn dcs_rows(session: &status::Status, variant: Option<&Path>, installed: bool) -> Vec<Row> {
    let rel = |path: &Path| match variant {
        Some(variant) => relative(variant, path),
        None => path.display().to_string(),
    };
    let restart = "start DCS, then run dcs-mcp verify again";
    let mut rows = Vec::new();
    match &session.session {
        None => {
            if session
                .problems
                .iter()
                .any(|p| matches!(p, status::Problem::NotInstalled { .. }))
            {
                let loaded = Row::new(Word::Waiting, "DCS", "has not loaded the executor yet");
                rows.push(if installed {
                    loaded.fix(restart)
                } else {
                    loaded
                });
            }
        }
        Some(s) => {
            rows.push(match &s.process {
                Process::Running => Row::new(
                    Word::Ok,
                    "DCS",
                    format!("running (process {}){}", s.pid, activity(s, session)),
                ),
                Process::Exited => Row::new(
                    Word::Waiting,
                    "DCS",
                    format!("not running (it last ran as process {})", s.pid),
                )
                .fix(restart),
                Process::Undecided { why } => Row::new(
                    Word::Problem,
                    "DCS",
                    format!(
                        "whether process {} is running could not be established: {why}",
                        s.pid
                    ),
                ),
            });
            if !s.eval {
                rows.push(Row::new(
                    Word::Note,
                    "DCS",
                    "the executor was loaded with evaluation off, so it runs no Lua",
                ));
            }
        }
    }
    let mut two_writers = false;
    for problem in &session.problems {
        let row = match problem {
            // Said by the process row already.
            status::Problem::NotInstalled { .. }
            | status::Problem::ProcessGone { .. }
            | status::Problem::ProcessUndecided { .. } => continue,
            status::Problem::ForeignStamp { .. }
            | status::Problem::ForeignTransport { .. }
            | status::Problem::ForeignHost { .. } => {
                // One cause, however many of its three marks it left.
                if two_writers {
                    continue;
                }
                two_writers = true;
                Row::new(
                    Word::Problem,
                    "DCS",
                    "another copy of the executor has written to the same folder since \
                     this one started: two copies of DCS, or of the executor, share this \
                     Saved Games folder",
                )
            }
            status::Problem::HandshakeUnreadable { path, why }
            | status::Problem::HeartbeatUnreadable { path, why } => Row::new(
                Word::Problem,
                "DCS",
                format!("{} would not read: {why}", rel(path)),
            ),
            status::Problem::ArmUndecided { why, .. } => Row::new(
                Word::Problem,
                "DCS",
                format!("whether anything has woken it could not be established: {why}"),
            ),
            status::Problem::TempdirDisagrees { executor, client } => Row::new(
                Word::Problem,
                "DCS",
                format!(
                    "uses the temp folder {executor}, which is not this user's ({client}); \
                     replies may not reach dcs-mcp"
                ),
            ),
            status::Problem::Unwritable(why) => Row::new(
                Word::Problem,
                "DCS",
                format!("names a folder dcs-mcp will not write requests into: {why}"),
            ),
            status::Problem::Unresolved { name, named, why } => Row::new(
                Word::Problem,
                "DCS",
                format!("the {name} it reported, {named}, would not resolve: {why}"),
            ),
        };
        rows.push(row);
    }
    rows
}

/// What a running session has been doing, as far as its heartbeat says.
///
/// A dormant heartbeat's age is when it went quiet and never whether it
/// lives (ADR 0012): the process row says it is running, and the age reads
/// only as when it was last used.
fn activity(s: &SessionStatus, session: &status::Status) -> String {
    match &s.beat {
        Some(beat) if beat.belongs => match beat.age {
            Age::Ticking(_) => format!(", active {}", place(&beat.phase)),
            Age::Dormant(quiet) => format!(
                ", idle; last used {} ago {}",
                ago(quiet),
                place(&beat.phase)
            ),
        },
        // Another session's, which a problem row names.
        Some(_) => String::new(),
        None if session
            .problems
            .iter()
            .any(|p| matches!(p, status::Problem::HeartbeatUnreadable { .. })) =>
        {
            String::new()
        }
        None => ", not used yet since it started".to_owned(),
    }
}

/// Where a phase puts the game, in words. A phase this build has no words
/// for is printed as it came.
fn place(phase: &str) -> String {
    match phase {
        "menu" => "outside a mission".to_owned(),
        "load" => "loading a mission".to_owned(),
        "sim" => "in a mission".to_owned(),
        "paused" => "in a paused mission".to_owned(),
        "loaded" => "with a mission loaded".to_owned(),
        "stopped" => "after the mission stopped".to_owned(),
        other => format!("in phase {other}"),
    }
}

/// A span in the largest unit that keeps it readable.
fn ago(span: Duration) -> String {
    let secs = span.as_secs();
    match secs {
        0..60 => format!("{secs} s"),
        60..3600 => format!("{} min", secs / 60),
        _ => format!("{} h {} min", secs / 3600, secs % 3600 / 60),
    }
}

/// The DCS build, where the session said one. Never a problem: a patched
/// game is the ordinary state of an install.
fn version_row(check: &VersionCheck) -> Option<Row> {
    let (word, text) = match check {
        VersionCheck::Unmeasured { running: Some(r) } => (Word::Note, r.clone()),
        VersionCheck::Unmeasured { running: None } | VersionCheck::Unreadable { .. } => {
            return None;
        }
        VersionCheck::Same { build } => (
            Word::Ok,
            format!("{build}, the build dcs-eval was tested on"),
        ),
        VersionCheck::Differs { running, measured } => (
            Word::Note,
            format!("{running} (dcs-eval was tested on {measured})"),
        ),
    };
    Some(Row::new(word, "DCS version", text))
}

/// A note per policy-gate key, as it is written, or one saying the file is
/// not there. A file that would not read is a problem row instead.
fn gate_rows(report: &Report) -> Vec<Row> {
    let gate = &report.gate;
    match &gate.file {
        GateFile::Absent => vec![Row::new(
            Word::Note,
            "autoexec.cfg",
            "not there (normal: DCS writes it only when an option changes)",
        )],
        GateFile::Read => [
            ("net.allow_unsafe_api", &gate.unsafe_api),
            ("net.allow_dostring_in", &gate.dostring_in),
        ]
        .into_iter()
        .map(|(key, value)| {
            let text = match value {
                Some(value) => format!("{key} = {value}"),
                None => format!("{key} is not set"),
            };
            Row::new(Word::Note, "autoexec.cfg", text)
        })
        .collect(),
        GateFile::Unreadable { .. } => Vec::new(),
    }
}

/// Every fact of the report, a key and a value each, in the order `details`
/// prints them.
fn details(report: &Report) -> Vec<(&'static str, String)> {
    let gate = &report.gate;
    let mut facts = vec![
        ("release", report.release.clone()),
        ("variant", report.variant.display().to_string()),
        (
            "hook",
            format!(
                "{}, {}",
                report.hook_path.display(),
                hook_said(&report.hook)
            ),
        ),
        (
            "Export.lua",
            format!(
                "{}, {}",
                report.export_path.display(),
                line_said(&report.line)
            ),
        ),
        (
            "autoexec.cfg",
            format!("{}, {}", gate.path.display(), gate_file(gate)),
        ),
        ("net.allow_unsafe_api", written(&gate.unsafe_api)),
        ("net.allow_dostring_in", written(&gate.dostring_in)),
    ];
    facts.extend(session_details(&report.session, Some(&report.app_version)));
    facts
}

/// Every fact of the session, a key and a value each. `app_version` is the
/// check the report made where there is one, and the session's own where
/// there is not.
///
/// Both structures are taken apart field by field, so that a field added to
/// either does not compile until it is given a line here.
fn session_details(
    session: &status::Status,
    app_version: Option<&VersionCheck>,
) -> Vec<(&'static str, String)> {
    let mut facts = vec![("output", session.output.display().to_string())];
    let Some(s) = &session.session else {
        facts.push(("session", "none published".to_owned()));
        if let Some(check) = app_version {
            facts.push(("app_version", check.to_string()));
        }
        return facts;
    };
    let SessionStatus {
        host,
        stamp,
        pid,
        started,
        process,
        transport,
        eval,
        arm_file,
        app_version: own,
        tempdir,
        beat,
        leftover,
    } = s;
    facts.push(("session", format!("{stamp}, started {started}")));
    facts.push(("host", host.clone()));
    facts.push(("process", format!("{pid}, {process}")));
    facts.push(("transport", transport.to_string()));
    facts.push(("eval", if *eval { "on" } else { "off" }.to_owned()));
    facts.push(("arm file", arm_file.to_string()));
    facts.push(("app_version", app_version.unwrap_or(own).to_string()));
    facts.push(("temp folder", tempdir.to_string()));
    match (beat, leftover) {
        (Some(beat), _) => {
            let BeatStatus {
                belongs,
                host,
                transport,
                phase,
                armed,
                since,
                ticks,
                last_callback,
                callbacks,
                age,
            } = beat;
            let callbacks = if callbacks.is_empty() {
                "none".to_owned()
            } else {
                callbacks.join(",")
            };
            facts.push((
                "heartbeat",
                format!(
                    "phase {phase}, {} since {since}, {age}, {ticks} ticks, last callback {}, \
                     callbacks {callbacks}, {}",
                    if *armed { "armed" } else { "not armed" },
                    last_callback.as_deref().unwrap_or("none"),
                    if *belongs {
                        "this session's"
                    } else {
                        "another session's"
                    },
                ),
            ));
            facts.push(("heartbeat host", host.clone()));
            facts.push(("heartbeat transport", transport.to_string()));
        }
        (None, Some(stamp)) => facts.push((
            "heartbeat",
            format!(
                "none written this session, so nothing has armed it; the file there is \
                 {stamp}'s, from before this session loaded"
            ),
        )),
        (None, None) => facts.push((
            "heartbeat",
            "none written, so nothing has armed it".to_owned(),
        )),
    }
    facts
}

/// What was at the hook's name, as `details` says it.
fn hook_said(hook: &Hook) -> String {
    match hook {
        Hook::Absent => "not there".to_owned(),
        Hook::Ours {
            sha256,
            current: true,
        } => format!("ours, sha256 {sha256}, the release this binary carries"),
        Hook::Ours {
            sha256,
            current: false,
        } => format!("ours, sha256 {sha256}, an older release"),
        Hook::Foreign { sha256 } => format!("sha256 {sha256}, never shipped by us"),
        Hook::Unreadable { why } => format!("could not be read: {why}"),
    }
}

/// How many lines load the executor, as `details` says it.
fn line_said(line: &Line) -> String {
    match line {
        Line::Absent => "no line loading the executor".to_owned(),
        Line::Once => "one line loading the executor".to_owned(),
        Line::Repeated { count } => format!("{count} lines loading the executor"),
        Line::NoFile => "not there".to_owned(),
        Line::Unreadable { why } => format!("would not read: {why}"),
    }
}

/// How the policy-gate file itself read, in one phrase.
fn gate_file(gate: &super::Gate) -> String {
    match &gate.file {
        GateFile::Absent => "not there, so neither key is set".to_owned(),
        GateFile::Read => "read".to_owned(),
        GateFile::Unreadable { why } => format!("would not read: {why}"),
    }
}

/// A key as it is written, or the fact that it is not written at all.
fn written(value: &Option<String>) -> String {
    match value {
        Some(text) => format!("{text}, as written"),
        None => "not set in the file".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use dcs_eval::paths::Real;
    use dcs_eval::standin::Standin;

    use crate::export_line;
    use crate::testing::Sandbox;
    use crate::verify::tests::{
        a_release, an_instant, fixture, install_files, installed, put, real,
    };
    use crate::verify::verify_at;

    /// The report of a fixture as it stands, against the test release.
    fn report_of(variant: &Real, output: &Path) -> Report {
        let (release, _older, _current) = a_release();
        verify_at(variant, output, &release, None, an_instant())
    }

    fn summary(report: &Report) -> String {
        render(report, Detail::Summary)
    }

    /// The rows every healthy fixture shares under the verdict: our release,
    /// the one line, and the two policy keys as the fixture writes them.
    const HOOK_OK: &str =
        "  ok       hook          Scripts\\Hooks\\DcsEvalExecutor.lua, this release";
    const LINE_OK: &str = "  ok       Export.lua    loads the executor once";
    const GATE: &str = "  note     autoexec.cfg  net.allow_unsafe_api = true\n  \
                        note     autoexec.cfg  net.allow_dostring_in = {\"mission\", \"server\"}";
    const VERSION: &str = "  note     DCS version   2.9.10.1234";
    const VERBOSE: &str = "run dcs-mcp verify --verbose for the exact findings";

    #[test]
    fn a_healthy_install_is_verified_on_the_first_line_and_a_row_per_part() {
        let (_b, variant, output) = fixture();
        let _ex = installed(&variant, &output);

        let report = report_of(&variant, &output);

        assert_eq!(
            summary(&report),
            format!(
                "verified: {}\n\n{HOOK_OK}\n{LINE_OK}\n  ok       DCS           running \
                 (process {}), not used yet since it started\n{VERSION}\n{GATE}",
                variant.as_path().display(),
                std::process::id()
            )
        );
    }

    #[test]
    fn nothing_installed_says_so_and_names_the_one_command() {
        let (_b, variant, output) = fixture();

        let report = report_of(&variant, &output);

        let want = [
            format!(
                "not verified: dcs-eval is not installed in {}",
                variant.as_path().display()
            ),
            String::new(),
            "  PROBLEM  hook          Scripts\\Hooks\\DcsEvalExecutor.lua is missing".to_owned(),
            "  PROBLEM  Export.lua    Scripts\\Export.lua is missing".to_owned(),
            "           fix: dcs-mcp install --variant DCS.openbeta".to_owned(),
            "  waiting  DCS           has not loaded the executor yet".to_owned(),
            "  note     autoexec.cfg  not there (normal: DCS writes it only when an option \
             changes)"
                .to_owned(),
            String::new(),
            VERBOSE.to_owned(),
        ];
        assert_eq!(summary(&report), want.join("\n"));
    }

    /// The state straight after `install`: not verified, and exit 1, but
    /// `waiting` and not a problem, with the one thing to do.
    #[test]
    fn installed_but_not_started_is_waiting_and_not_verified() {
        let (_b, variant, output) = fixture();
        install_files(&variant);

        let report = report_of(&variant, &output);

        assert!(!report.verified());
        let want = [
            format!(
                "not verified: DCS has not yet loaded the executor installed in {}",
                variant.as_path().display()
            ),
            String::new(),
            HOOK_OK.to_owned(),
            LINE_OK.to_owned(),
            "  waiting  DCS           has not loaded the executor yet".to_owned(),
            "           fix: start DCS, then run dcs-mcp verify again".to_owned(),
            GATE.to_owned(),
            String::new(),
            VERBOSE.to_owned(),
        ];
        assert_eq!(summary(&report), want.join("\n"));
    }

    #[test]
    fn a_dcs_that_has_exited_is_waiting_on_a_restart() {
        let (_b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        let mut report = report_of(&variant, &output);
        let session = report.session.session.as_mut().expect("a session");
        session.process = Process::Exited;
        let pid = session.pid;
        report.session.problems = vec![status::Problem::ProcessGone { pid }];

        let summary = summary(&report);

        assert!(
            summary.starts_with(&format!(
                "not verified: DCS is not running, and the executor is installed in {}\n",
                variant.as_path().display()
            )),
            "{summary}"
        );
        assert!(
            summary.contains(&format!(
                "  waiting  DCS           not running (it last ran as process {pid})\n           \
                 fix: start DCS, then run dcs-mcp verify again\n"
            )),
            "{summary}"
        );
    }

    #[test]
    fn every_phase_either_host_writes_has_words() {
        for (phase, words) in [
            ("menu", "outside a mission"),
            ("load", "loading a mission"),
            ("sim", "in a mission"),
            ("paused", "in a paused mission"),
            ("loaded", "with a mission loaded"),
            ("stopped", "after the mission stopped"),
            ("mission", "in phase mission"),
        ] {
            assert_eq!(place(phase), words, "{phase}");
        }
    }

    #[test]
    fn a_session_in_use_says_what_the_game_is_doing_and_never_that_quiet_is_dead() {
        let (_b, variant, output) = fixture();
        let mut ex = installed(&variant, &output);
        ex.armed = true;
        ex.phase = "sim".to_owned();
        ex.beat(an_instant()).expect("the heartbeat is published");
        let mut report = report_of(&variant, &output);
        let pid = std::process::id();

        let active = summary(&report);
        assert!(
            active.contains(&format!(
                "  ok       DCS           running (process {pid}), active in a mission\n"
            )),
            "{active}"
        );

        let beat = report
            .session
            .session
            .as_mut()
            .and_then(|s| s.beat.as_mut())
            .expect("the heartbeat is read");
        beat.phase = "menu".to_owned();
        beat.age = Age::Dormant(Duration::from_secs(856));
        let idle = summary(&report);
        assert!(
            idle.contains(&format!(
                "  ok       DCS           running (process {pid}), idle; last used 14 min ago \
                 outside a mission\n"
            )),
            "{idle}"
        );
    }

    /// Three install findings at once, each a row with the command that
    /// clears it and no hash in sight.
    #[test]
    fn every_install_finding_is_a_row_with_its_fix() {
        let (_b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        let hooks = variant.as_path().join("Scripts").join("Hooks");
        put(&hooks.join("DcsEvalExecutor.lua"), b"-- somebody else's\n");
        put(
            &hooks.join("DcsEvalExecutor.old.lua"),
            b"-- a copy left behind\n",
        );
        put(
            &export_line::path(&variant),
            format!("{}\n{}\n", export_line::LINE, export_line::LINE).as_bytes(),
        );

        let summary = summary(&report_of(&variant, &output));

        let want = [
            format!(
                "not verified: 3 problems in {}",
                variant.as_path().display()
            ),
            String::new(),
            "  PROBLEM  hook          Scripts\\Hooks\\DcsEvalExecutor.lua is not a file dcs-eval \
             shipped"
                .to_owned(),
            "           fix: dcs-mcp install --replace --variant DCS.openbeta  (moves it aside \
             first)"
                .to_owned(),
            "  PROBLEM  hook          Scripts\\Hooks\\DcsEvalExecutor.old.lua is a second copy of \
             the executor, and DCS loads both"
                .to_owned(),
            "           fix: delete Scripts\\Hooks\\DcsEvalExecutor.old.lua".to_owned(),
            "  PROBLEM  Export.lua    loads the executor 2 times".to_owned(),
            "           fix: delete all but one of those lines from Scripts\\Export.lua".to_owned(),
            "  ok       DCS           ".to_owned(),
        ];
        assert!(summary.starts_with(&want.join("\n")), "{summary}");
        assert!(!summary.contains("sha256"), "{summary}");
    }

    #[test]
    fn an_older_release_of_ours_is_a_problem_with_install_as_its_fix() {
        let (_b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        let hook = variant
            .as_path()
            .join("Scripts")
            .join("Hooks")
            .join("DcsEvalExecutor.lua");
        put(&hook, b"-- an older release\n");

        let summary = summary(&report_of(&variant, &output));

        assert!(
            summary.contains(
                "  PROBLEM  hook          Scripts\\Hooks\\DcsEvalExecutor.lua is an older \
                 dcs-eval release\n           fix: dcs-mcp install --variant DCS.openbeta\n"
            ),
            "{summary}"
        );
    }

    /// A second writer leaves up to three marks, and they are one cause: one
    /// row, and every exact sentence still under the full report.
    #[test]
    fn two_writers_are_one_row_and_every_exact_sentence() {
        let (_b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        let mut report = report_of(&variant, &output);
        let here = real(variant.as_path());
        report.session.problems = vec![
            status::Problem::ForeignStamp {
                saw: "1790020166-15404".to_owned(),
                wanted: "1790027086-25924".to_owned(),
            },
            status::Problem::ForeignTransport {
                saw: here.clone(),
                wanted: here,
            },
        ];

        let summary = summary(&report);
        assert_eq!(
            summary.matches("another copy of the executor").count(),
            1,
            "{summary}"
        );
        let full = render(&report, Detail::Full);
        for problem in &report.session.problems {
            assert!(
                full.contains(&format!("\n  problem: {problem}\n")),
                "{full}"
            );
        }
    }

    #[test]
    fn a_temp_folder_outside_this_users_is_a_problem_with_no_fix() {
        let (b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        let mut report = report_of(&variant, &output);
        let executor = real(&b.dir("elsewhere"));
        let client = real(&std::env::temp_dir());
        report.session.problems = vec![status::Problem::TempdirDisagrees {
            executor: executor.clone(),
            client: client.clone(),
        }];

        let summary = summary(&report);

        assert!(
            summary.contains(&format!(
                "  PROBLEM  DCS           uses the temp folder {executor}, which is not this \
                 user's ({client}); replies may not reach dcs-mcp\n"
            )),
            "{summary}"
        );
        assert!(!summary.contains("fix:"), "{summary}");
    }

    #[test]
    fn a_key_the_file_does_not_set_is_a_row_saying_so() {
        let (_b, variant, output) = fixture();
        let _ex = installed(&variant, &output);
        put(
            &variant.as_path().join("Config").join("autoexec.cfg"),
            b"net.allow_unsafe_api = true\n",
        );

        let summary = summary(&report_of(&variant, &output));

        assert!(
            summary.ends_with(
                "  note     autoexec.cfg  net.allow_unsafe_api = true\n  \
                 note     autoexec.cfg  net.allow_dostring_in is not set"
            ),
            "{summary}"
        );
    }

    /// The full report is the summary with more under it: the same first
    /// lines, every problem's sentence as it was always worded and in the
    /// order found, then the facts.
    #[test]
    fn the_full_report_is_the_summary_then_every_exact_sentence_then_details() {
        let (_b, variant, output) = fixture();

        let report = report_of(&variant, &output);
        let full = render(&report, Detail::Full);
        let summary = summary(&report);

        let rows = summary
            .strip_suffix(&format!("\n\n{VERBOSE}"))
            .expect("a report not verified ends in the pointer to --verbose");
        assert!(full.starts_with(rows), "{full}");
        let exact: Vec<String> = report
            .problems
            .iter()
            .map(ToString::to_string)
            .chain(report.session.problems.iter().map(ToString::to_string))
            .map(|sentence| format!("  problem: {sentence}"))
            .collect();
        assert_eq!(exact.len(), 3);
        assert!(
            full.contains(&format!(
                "\n\nproblems (exact)\n{}\n\ndetails\n",
                exact.join("\n")
            )),
            "{full}"
        );
        assert!(!full.contains("--verbose"), "{full}");
    }

    /// Every fact the report carries is a line of `details`. The session and
    /// its heartbeat are taken apart field by field, so a field added to
    /// either fails to compile here until it is given a line.
    #[test]
    fn full_report_carries_every_session_fact() {
        let (_b, variant, output) = fixture();
        let mut ex = installed(&variant, &output);
        ex.armed = true;
        ex.phase = "sim".to_owned();
        ex.last_callback = "onSimulationFrame".to_owned();
        ex.callbacks = vec![
            "onSimulationStart".to_owned(),
            "onSimulationFrame".to_owned(),
        ];
        ex.tick = 5065;
        ex.beat(an_instant()).expect("the heartbeat is published");

        let report = report_of(&variant, &output);
        let full = render(&report, Detail::Full);
        let line = |key: &str, value: &str| {
            assert!(
                full.lines().any(|l| l == detail_line(key, value)),
                "no `{key}` line reading {value}:\n{full}"
            );
        };

        line("release", &report.release);
        line("variant", &report.variant.display().to_string());
        let Hook::Ours { sha256, .. } = &report.hook else {
            panic!("{:?}", report.hook)
        };
        line(
            "hook",
            &format!(
                "{}, ours, sha256 {sha256}, the release this binary carries",
                report.hook_path.display()
            ),
        );
        line(
            "Export.lua",
            &format!(
                "{}, one line loading the executor",
                report.export_path.display()
            ),
        );
        line(
            "autoexec.cfg",
            &format!("{}, read", report.gate.path.display()),
        );
        line("net.allow_unsafe_api", "true, as written");
        line(
            "net.allow_dostring_in",
            "{\"mission\", \"server\"}, as written",
        );

        let status::Status {
            output: out,
            session,
            problems,
        } = &report.session;
        line("output", &out.display().to_string());
        assert!(problems.is_empty(), "{problems:?}");
        let SessionStatus {
            host,
            stamp,
            pid,
            started,
            process,
            transport,
            eval,
            arm_file,
            app_version,
            tempdir,
            beat,
            leftover,
        } = session.as_ref().expect("a session");
        line("session", &format!("{stamp}, started {started}"));
        line("host", host);
        line("process", &format!("{pid}, {process}"));
        line("transport", &transport.to_string());
        line("eval", if *eval { "on" } else { "off" });
        line("arm file", &arm_file.to_string());
        assert_eq!(
            app_version, &report.app_version,
            "one check, measured the same"
        );
        line("app_version", &app_version.to_string());
        line("temp folder", &tempdir.to_string());
        assert_eq!(
            leftover, &None,
            "the relaunch test holds the leftover's line"
        );

        let BeatStatus {
            belongs,
            host: beat_host,
            transport: beat_transport,
            phase,
            armed,
            since,
            ticks,
            last_callback,
            callbacks,
            age,
        } = beat.as_ref().expect("the heartbeat is read");
        assert!(*belongs && *armed);
        assert_eq!((*ticks, callbacks.len()), (5065, 2), "the fixture's own");
        line(
            "heartbeat",
            &format!(
                "phase {phase}, armed since {since}, {age}, {ticks} ticks, last callback {}, \
                 callbacks {}, this session's",
                last_callback.as_deref().expect("a last callback"),
                callbacks.join(",")
            ),
        );
        line("heartbeat host", beat_host);
        line("heartbeat transport", &beat_transport.to_string());
    }

    /// `status` over an install it could not look at still words the
    /// session the one way, with no Debug syntax in it.
    #[test]
    fn the_session_alone_renders_its_rows_problems_and_facts() {
        let b = Sandbox::new();
        let output = b.dir("out");
        let _ex = Standin::open(&output, "hook").expect("the stand-in opens");
        let session = status::status_at(&output, an_instant());

        let rendered = render_session(&session);

        assert!(
            rendered.contains(&detail_line("output", &output.display().to_string())),
            "{rendered}"
        );
        let exact: Vec<String> = session
            .problems
            .iter()
            .map(|p| format!("  problem: {p}"))
            .collect();
        assert!(!exact.is_empty(), "nothing published a handshake");
        assert!(rendered.contains(&exact.join("\n")), "{rendered}");
        assert!(!rendered.contains("Some("), "{rendered}");
    }
}
