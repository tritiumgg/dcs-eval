# Working state

**Last updated:** 2026-09-11

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

Nothing. T17 is on its branch, waiting on its pull request; T18 has not started.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T17** — the round-trip control: `e2e: 1 check` under cargo, `e2e: 10 checks` under the harness.
  `crates/dcs-eval/src/e2e.rs` spawns `lua5.1.exe` on `tools/harness/executor/e2e.lua` over a box in `DCS_EVAL_E2E`, sends a `ping` through the client's `send` into the live tick and reads `pong` back; a CRLF in `frame` and a suite that stops ticking seen red, the second in 20 s, not a hang.
- **T16** — the interop control: `interop: 4 checks` under cargo. `crates/dcs-eval/src/interop.rs`
  spawns `lua5.1.exe` on `tools/harness/executor/interop.lua` over a box in `DCS_EVAL_INTEROP`; the handshake, a `ping` and an `eval` reply parse, the stand-in matches; the `frame` and empty-value mutations seen red.
- **T15** — the stand-in: `standin: 19 checks` under cargo. `crates/dcs-eval/src/standin.rs`
  behind the `standin` feature, its encoder a CRLF dialect and its decoder and disk side its own; the client's `send` round-trips a `ping`; the frame-swap mutation seen red.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T18** — the `hook` eval carrier: `loadstring` plus `setfenv` into the
host `_G`, with a true `chunkname`. Done when `lua5.1 tools/harness.lua
executor/eval-hook` shows a raise on line 47 reported as `<name>:47`; mutation:
one line prepended to the body before compiling reddens the line-truth check.
Needs T12. Serving `eval` flips two pinned answers: the interop control's
`unsupported` case becomes an empty body, and the stand-in's `eval` answer
follows the executor's (`standin.rs`, `interop.rs`).

**An agent verifies** it: the harness under `lua5.1.exe` on PATH under mise,
and `cargo test -p dcs-eval interop standin` for the two flips.

## After that

- **Stages 0 to 2 are closed, and Milestone A with them:** the wire proven off
  DCS, the stand-in, the interop and round-trip controls. T17 closed it.
- **Stage 3 (T18–T22) is eval and line-truth**, Milestone B: the one op that
  runs anything, across the four carriers, with `<file>:47` true in every state.
- **Stage 9** is the critical path and cannot be shortened by parallel effort.
  Everything provable off DCS is proved before it.

## Carries forward

Things that must not be lost between sessions. Delete an entry when it is
resolved, and say where. Mark an entry only the maintainer can settle. Ten
entries at most: an eleventh means something here is finished, or belongs in
`docs/decisions/` or `CLAUDE.md` instead.

- **Cutover — the incumbent goes before this executor installs.**
  `dcs-api-bridge`'s `DcsApiEval.lua` sits in the same `Scripts\Hooks\`; ADR
  0002 is why this one is not a near-identical name. Two hooks on one
  transport root is what T52 proves cannot happen.
- **Maintainer decision — when the MCP registration is swapped.** Claude Code
  still points at `dcs-api-bridge`; the swap strands any session mid-task, so
  it happens at Milestone C and not before. ADR 0001.
- **The 0.098 ms dormant baseline is the incumbent's, on one machine.** T48
  compares against it; measured on DCS 2.9.28.26385, one session, so a figure
  that disagrees on other hardware is a new measurement, not a regression.
- **Two gaps T06 left.** The hook guard's swallow path has no seam until a
  raising stub sits on the frame path. Whether the export state survives
  between missions is unmeasured; the load sentinel covers both, Stage 9 settles it.
- **Declared before served, and absent before counted.** The handshake
  publishes `ops`, `states`, `eval` and the five figures from the first load,
  the two instruction figures provisional; the task that lands each reads its
  constant. `eval` is answered `unsupported` until T18, and the interop control
  reads it so; when T18 serves it, that case flips to an empty body, the stub's
  `net.dostring_in` answer. `cpu_ms` is absent from replies until T23.
- **A path with a byte past ASCII stops the load.** Header values are ASCII,
  a user name past ASCII puts such a byte in every path the handshake names,
  and the executor refuses the file with the header named in `dcs.log`. The
  specification says nothing (`docs/audit.md`, Open). The client's parser
  refuses such a value as the executor does, the maintainer's call at T13;
  a spelling for such a path on the wire is the writer's side, unsettled.
- **The held sibling is a held file.** The sweep's "cannot be removed" path is
  proved with a file the suite keeps open; a client's directory handle from
  `ReadDirectoryChangesW` is only seen at Stage 9, with the real client.
- **A wrong `for` is answered.** `admit` passes a wrong stamp through and the
  tick answers it; `executor/ping` pins that until the fence (T25) answers
  `stale-session`, and the round-trip's `superseded` mutation waits on it and on the client's wait (T31).
- **`docs/PLAN.md`'s DR-1 and DR-2 stay the record** for one repository and
  Windows as the target; a copy in `docs/decisions/` would be a second place
  to keep in step.
