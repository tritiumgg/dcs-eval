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

Nothing. Milestone D closed on 2026-09-22: T52 and T51 passed by hand at the maintainer's install, ADR 0035 holds what shipped unmeasured, ADR 0036 closes the non-ASCII path, ADR 0037 the `--out` path.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **Milestone D closed (ADR 0035)**: T52's cutover and T51's permanent-install acceptance done by hand on 2026-09-22; the "what only Stage 9 sees" list and the handshake's declared-before-served fact moved into the record; five resolved carries deleted: three into it, and two stale ones of Stage 7's closed with Milestone C (the held handle by T41's `watching` controls, the `Minter` by a fresh tag per tool call).
- **A path past ASCII is a shipped limit (ADR 0036)**: no wire spelling is built; the README says a user name past ASCII is unsupported and `docs/audit.md` points at the record.
- **The wait suite fails instead of hanging on a pid-for-stamp `decide`**: its unbounded-wait test runs on a thread with a 30 s bound, so `cargo test -p dcs-eval wait` reddens `wait/pid-for-stamp` (T54).

*The last three at most, one line each. Git log holds the rest.*

## Next

The one carry below, then a release: every plan task is done and every milestone closed, but no workflow builds or publishes `dcs-mcp.exe` and the README's Install says to download it. The tag and the workflow are the maintainer's.

The MCP registration is swapped (2026-09-22): `dcs-eval` replaces `dcs-api-bridge` in
Claude Desktop's config and Claude Code's user scope, running a copy of the binary at
`%LOCALAPPDATA%\Programs\dcs-mcp\` so a rebuild is not blocked by the server's lock.

## After that

- **Every milestone is closed.** A: the wire proven off DCS (Stages 0–2). B: the full protocol
  and its controls (3–6). C: installable and serving, 26 controls seen red (7–8). D: measured
  live (ADR 0031), the cutover and the permanent install by hand, what shipped unmeasured
  written down (ADR 0035). Figures in git log and the records.

## Carries forward

Things that must not be lost between sessions. Delete an entry when it is
resolved, and say where. Mark an entry only the maintainer can settle. Ten
entries at most: an eleventh means something here is finished, or belongs in
`docs/decisions/` or `CLAUDE.md` instead.

- **`game-state` wording, two rough edges.** A nil read prints `player_unit_type: nil nil`
  (type and value both); the pause axis at the menu heads the answer with `unknown: activity
  said menu, and this is answered only in a mission`. Plain words for both. Maintainer's, 2026-09-22.
