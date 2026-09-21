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
            format!("installed in {variant}, and DCS has not loaded it yet")
        }
        [status::Problem::ProcessGone { .. }] if installed => {
            format!("installed in {variant}, and DCS is not running")
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
