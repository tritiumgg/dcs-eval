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

Nothing. T13 is on its branch, waiting on its pull request; T14 has not started.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T13** — the client's envelope: `protocol: 35 checks` under cargo. `frame` and `parse`
  in `crates/dcs-eval`, cases mirroring the harness, refusals in the executor's words; both named mutations seen red.
- **T12** — the tick and `ping`: `ping: 305 checks`, both hosts, under a sandbox. The
  frame lists `req`, admits in name order, dispatches through `ops`; `tick` on every reply; the missing-`status` mutation seen red.
- **T11** — the handshake: `handshake: 320 checks`, both hosts, under a sandbox. Every
  field in the table's order, `protocol: 2`, by rename before registration, a stale one replaced; the two named mutations and `protocol: 1` seen red.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T14** — the client's send in `crates/dcs-eval`: a request published by
rename, and the arm file made when absent, never removed. Done when
`cargo test -p dcs-eval publish` shows a request landing only under its final
name and the arm file created when absent; mutation: a client that removes
the arm file reddens the never-removes check. Needs T13. The disk discipline
to mirror is the executor's `publish`; the arm file and what it means are
`spec.sh find BRIDGE "arm file"`; the bytes come from `protocol::frame`.

**An agent verifies** it: cargo runs the tests over a temporary directory; a
real executor reads the file at T17.

## After that

- **Stages 0 and 1 are closed.** Stage 2 (T12–T17) is `ping` and the wire
  proven: the client half in Rust, the stand-in, and the interop controls.
- **Milestone A** is Stages 0–2: the wire proven off DCS, the interop control
  first and the stand-in second. The CI interop job arrives with T15.
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
  constant. `eval` is answered `unsupported` until T18 serves it; `cpu_ms`
  is absent from replies until T23. The interop control at T16 reads none of
  this as a defect.
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
  `stale-session`.
- **`docs/PLAN.md`'s DR-1 and DR-2 stay the record** for one repository and
  Windows as the target; a copy in `docs/decisions/` would be a second place
  to keep in step.
