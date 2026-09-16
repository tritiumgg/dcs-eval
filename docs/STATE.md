# Working state

**Last updated:** 2026-09-16

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

Nothing. T21 landed; T22 has not started.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T21** — the mission door: `door: 706 checks`. `net.dostring_in` carries `DOOR` into `mission` with the body and `chunkname` as `%q` literals; `DOOR` reads `a_do_script` at use (`door-shut` without one) and passes them to the constant `FAR`, the wrapper reading them from `...` and ending `, 0`; slot 2 is read, a non-string there refused `stage: door` naming both slots; every reply carries `carrier`/`via`; eight mutations seen red.
- **T20** — the `net.dostring_in` carrier: `dostring: 778 checks`. The body crosses as a `%q` literal in a wrapper compiled in-state under `chunkname`, run in that state's `_G`, converted there by one source string both carriers use (ADR 0003), answered as three fields; `nil` is `refused`, the literal `invalid-state`, an unshaped string `error`/`dostring_in`; line 47 true through the wrapper; six mutations seen red.
- **T19** — result conversion and the reply ceiling: `result: 553 checks`. `number` prints `%.14g`, widened to `%.17g` where it does not read back, `inf`/`-inf`/`nan` by name; `answer` refuses a body over `MAX_RESULT_BYTES`, returned or raised, as `stage: oversize` with `result_bytes`; five mutations seen red.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T22** — the `a_do_script` off-by-one fixture under reference `lua5.1`.
Done when `lua5.1 tools/harness.lua executor/door-shift` exercises the shift
and asserts the correction; mutation: removing the correction reddens it, a
lone value dropped. Needs T21. The correction is `FAR`'s trailing `, 0` and
`DOOR`'s slot-2 read; `executor/door`'s `door()` already models the shift and
reddens on that mutation, so this suite isolates the fixture the incumbent
kept in `dcs-api/tools/probes/tabledepth_test.lua`.

**An agent verifies** it: the harness under `lua5.1.exe` on PATH under mise.

## After that

- **Stages 0 to 2 are closed, and Milestone A with them:** the wire proven off
  DCS, the stand-in, the interop and round-trip controls. T17 closed it.
- **Stage 3 (T18–T22) is eval and line-truth**, Milestone B: the one op that
  runs anything, across the four carriers, with `<file>:47` true in every
  state. T18, T20 and T21 built them; T22 closes the stage.
- **Stage 9** is the critical path and cannot be shortened by parallel effort.
  Everything provable off DCS is proved before it.

## Carries forward

Things that must not be lost between sessions. Delete an entry when it is
resolved, and say where. Mark an entry only the maintainer can settle. Ten
entries at most: an eleventh means something here is finished, or belongs in
`docs/decisions/` or `CLAUDE.md` instead.

- **Cutover — the incumbent goes before this executor installs.**
  `dcs-api-bridge`'s `DcsApiEval.lua` sits in the same `Scripts\Hooks\`; ADR
  0002 is why the names differ; T52 proves two hooks on one root cannot happen.
- **Maintainer decision — when the MCP registration is swapped.** Claude Code
  still points at `dcs-api-bridge`; the swap strands any session mid-task, so
  it happens at Milestone C and not before. ADR 0001.
- **The 0.098 ms dormant baseline is the incumbent's, on one machine.** T48
  compares against it; measured on DCS 2.9.28.26385, one session, so a figure
  that disagrees on other hardware is a new measurement, not a regression.
- **What only Stage 9 sees.** The hook guard's swallow path has no seam until
  a raising stub sits on the frame path; whether the export state survives
  between missions is unmeasured, and the load sentinel covers both. The
  wrapper is proved under the suite's own carrier alone: what DCS does with
  one that raises inside `net.dostring_in`, and whether every state has
  `setfenv` and `_G`, is unmeasured; the model's stubs evaluate nothing. The
  door is proved against the incumbent's measured shift and `...`, not T49's.
- **Declared before served, and absent before counted.** The handshake
  publishes `ops`, `states`, `eval` and the five figures from the first load,
  the two instruction figures provisional; the task that lands each reads its
  constant. No `cpu_ms` until T23, no `budget` until T24. No `state` is
  `bad-request`: the maintainer's call, 2026-09-11.
- **A path with a byte past ASCII stops the load.** Header values are ASCII,
  a user name past ASCII puts such a byte in every path the handshake names,
  and the executor refuses the file with the header named in `dcs.log`. The
  specification says nothing (`docs/audit.md`, Open). The client's parser
  refuses such a value as the executor does, the maintainer's call at T13;
  a spelling for such a path on the wire is the writer's side, unsettled.
- **The held sibling is a held file.** The sweep's "cannot be removed" path is
  proved with a file the suite keeps open; a client's `ReadDirectoryChangesW` handle is Stage 9's.
- **A wrong `for` is answered.** `admit` passes a wrong stamp through and the
  tick answers it; `executor/ping` pins that until the fence (T25) answers
  `stale-session`, and the round-trip's `superseded` mutation waits on it and on the client's wait (T31).
- **`docs/PLAN.md`'s DR-1 and DR-2 stay the record** for one repository and
  Windows as the target; a copy in `docs/decisions/` is a second place to keep in step.
