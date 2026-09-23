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

Nothing. T69 landed, and with it Stage 0; T70 is next.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T69, the verb**: `dcs-mcp screenshot [--name] [--wait-seconds] [--out]` over `tools::screenshot`, whose `Answered` now carries the path an `ok` found. `--out` copies that file and writes nothing for any other answer; `--capture` refused by name; exits 0 (`pending`, `not-written` too), 1, and 2 through the binary. Three controls, `shotcli/`, swept red; the README documents the verb.
- **T68, the seventh tool**: `dcs_screenshot` is registered, listed and answered over MCP through `tools::screenshot`, worded by `wording::capture`: `ok` with path, format, bytes, width and height; `pending` and `not-written` naming directory and name, neither marked an error; a `pending` with no directory says why (ADR 0039). The export host is refused before a session is sought. Two controls, `tools/`, swept red; the README says seven tools.
- **T67, the capture**: `dcs_eval::screenshot::capture` publishes `DCS.makeScreenShot(<name>) return lfs.writedir()` for the `hook` state, refuses the export host unpublished, and watches `ScreenShots` over what the reply leaves of one wait. New means stamped at or after a reading of the files' own clock (ADR 0038); a `pending` names its directory off the handshake (ADR 0039). Five controls, `screenshot/`, swept red.

*The last three at most, one line each. Git log holds the rest.*

## Next

T70, the first live capture in a mission, at the menu and in the editor: reported format and size against the file, and the ask-to-whole spread over ten captures, in a decision record. Only a person at a live install verifies it: an agent drives the calls, the maintainer sets each scene and looks. The registered server's copy under `%LOCALAPPDATA%\Programs\dcs-mcp\` predates `dcs_screenshot`; refresh it first.

A release is still open: every task of the retired plan is done, but no workflow builds or publishes `dcs-mcp.exe` and the README's Install says to download it. The tag and the workflow are the maintainer's.

The MCP registration is swapped (2026-09-22): `dcs-eval` replaces `dcs-api-bridge` in
Claude Desktop's config and Claude Code's user scope, running a copy of the binary at
`%LOCALAPPDATA%\Programs\dcs-mcp\` so a rebuild is not blocked by the server's lock.

## After that

- **Milestone F**, a capture looked at in a mission, at the menu and in the editor (T70), and
  `screenshot.md` §4's open questions answered (T71). **Milestone E** closes with this PR:
  Stage 0's command green and seven tools listed.
- **A to D are closed** — the wire, the protocol, the installer, and live in DCS. Figures in git
  log, the records, and `docs/PLAN-SHIPPED.md`.

## Carries forward

Things that must not be lost between sessions. Delete an entry when it is
resolved, and say where. Mark an entry only the maintainer can settle. Ten
entries at most: an eleventh means something here is finished, or belongs in
`docs/decisions/` or `CLAUDE.md` instead.

- **Nothing here has ever taken a screenshot.** Every DCS fact in `docs/specs/screenshot.md` §1 comes from the install, from files on one maintainer's disk and from an earlier project. Stage 1 is where this build sees one.
