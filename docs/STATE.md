# Working state

**Last updated:** 2026-09-23

The handoff between sessions. Read it first; update it before a session ends,
not only when a task finishes. Stamp the date above each time; it carries a
date and nothing else, because what changed is what the sections below are for.

**This file is loaded cold every session, so its size is a tax on all of them.**
Each section has a line budget and `tools/statecheck.sh` enforces it. Over
budget, nothing is deleted — it moves. A completion older than the last few
goes to git log. A choice with reasoning behind it becomes a decision record.
A durable fact about the project belongs in `CLAUDE.md`. A resolved
carry-forward is just deleted. One or two lines per entry, never paragraphs.

---

## In progress

Nothing. T66 landed; T67 is next.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T66, the finished file**: `dcs_eval::shot_file` finds a capture under `.png`, `.jpg`, `.bmp` or the bare name, newest first, judges it whole by its format's own end, answers zero bytes `Empty`, and reads the size, walking a JPEG's segments past a thumbnail. Three controls, `shotfile/`, swept red.
- **T65, the screenshot name**: `dcs_eval::shot_name` checks a caller's name and refuses naming the character, or supplies `dcs-eval-YYYYMMDD-HHMMSS-mmm` off `GetLocalTime`, waiting out a clock that has not moved so two back to back differ. Two controls, `shotname/`, swept red.
- **`docs/specs/screenshot.md`, frozen**: a seventh tool and a fifth verb, the DCS facts with where each came from, crop and an inline image argued out, and §4 listing what only a live install can answer.

*The last three at most, one line each. Git log holds the rest.*

## Next

T67, the capture: the chunk for the `hook` state, the export host refused, the newness test against a clock read before publishing, and one wait over both phases, `cargo test -p dcs-eval -- screenshot`. Developer-only against the stand-in, and an agent sees its own result. It joins `shot_name::resolve` to `shot_file::find` and `examine`, and owns the newness test. Then T68 and T69 update the README, which still says six tools.

A release is still open: every task of the retired plan is done, but no workflow builds or publishes `dcs-mcp.exe` and the README's Install says to download it. The tag and the workflow are the maintainer's.

The MCP registration is swapped (2026-09-22): `dcs-eval` replaces `dcs-api-bridge` in
Claude Desktop's config and Claude Code's user scope, running a copy of the binary at
`%LOCALAPPDATA%\Programs\dcs-mcp\` so a rebuild is not blocked by the server's lock.

## After that

- **Milestone E**, the capability built: Stage 0 green and an MCP session listing seven tools.
  **Milestone F**, a capture looked at in a mission, at the menu and in the editor, and
  `screenshot.md` §4's open questions answered.
- **A to D are closed** — the wire, the protocol, the installer, and live in DCS. Figures in git
  log, the records, and `docs/PLAN-SHIPPED.md`.

## Carries forward

Things that must not be lost between sessions. Delete an entry when it is
resolved, and say where. Mark an entry only the maintainer can settle. Ten
entries at most: an eleventh means something here is finished, or belongs in
`docs/decisions/` or `CLAUDE.md` instead.

- **Nothing here has ever taken a screenshot.** Every DCS fact in `docs/specs/screenshot.md` §1 comes from the install, from files on one maintainer's disk and from an earlier project. Stage 1 is where this build sees one.
