# Working state

**Last updated:** 2026-09-22

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

Nothing. The screenshot capability is specified and planned; T65 is where building starts.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **`docs/specs/screenshot.md`, frozen**: a seventh tool and a fifth verb, the DCS facts with where each came from, crop and an inline image argued out, and §4 listing what only a live install can answer.
- **The plan is new**: the finished one is retired at `docs/PLAN-SHIPPED.md`, `docs/PLAN.md` carries seven tasks in two stages from T65, and `tools/sweep-cover.sh` reads both — the current plan owes an entry for a row it marks `**Done**` in its task cell, a retired plan across Stage 0–8, and an ID two plans share is refused.
- **The frozen guard lets a specification be born**: a path under `docs/specs/` that does not exist passes once, and is refused every write after.

*The last three at most, one line each. Git log holds the rest.*

## Next

T65, the screenshot name: validation and the supplied default, `cargo test -p dcs-eval -- shot_name`. Developer-only, and an agent sees its own result. Then T66 and T67; T68 and T69 update the README, which still says six tools.

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
