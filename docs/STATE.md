# Working state

**Last updated:** 2026-09-13

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

Nothing. T20 landed; T21 has not started.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T20** — the `net.dostring_in` carrier: `dostring: 778 checks`. The body crosses as a `%q` literal in a wrapper compiled in-state under `chunkname`, run in that state's `_G`, converted there by one source string both carriers use (ADR 0003), answered as three fields; `nil` is `refused`, the literal `invalid-state`, an unshaped string `error`/`dostring_in`; line 47 true through the wrapper; six mutations seen red.
- **T19** — result conversion and the reply ceiling: `result: 553 checks`. `number` prints `%.14g`, widened to `%.17g` where it does not read back, `inf`/`-inf`/`nan` by name; `answer` refuses a body over `MAX_RESULT_BYTES`, returned or raised, as `stage: oversize` with `result_bytes`; five mutations seen red.
- **T18** — the local eval carrier: `eval-hook: 583 checks`. `loadstring(body, chunkname)` with nothing prepended, `setfenv` into the host `_G`, `hook` and `export` served in place; a raise on line 47 reads `<name>:47:` under every spelling of `chunkname`; the prepend mutation reads 48.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T21** — the `missionscripting` two-hop door: `a_do_script` from the
`mission` state, its args as `%q` literals, the return-shift correction,
string-only conversion, and a `door-shut` status with no mission loaded. Done
when `lua5.1 tools/harness.lua executor/door` shows a value crossing as a
string only, the slot-2 read, and `door-shut`; mutation: a table returned
across the door reddens the string-only backstop. Needs T20. Start from
`OPS.eval`, where `a_do_script` still answers "not yet served"; the far chunk
is `wrapper` one hop further, `CONVERT` with it; the model lacks `a_do_script`.

**An agent verifies** it: the harness under `lua5.1.exe` on PATH under mise.

## After that

- **Stages 0 to 2 are closed, and Milestone A with them:** the wire proven off
  DCS, the stand-in, the interop and round-trip controls. T17 closed it.
- **Stage 3 (T18–T22) is eval and line-truth**, Milestone B: the one op that
  runs anything, across the four carriers, with `<file>:47` true in every
  state. T18 built the local carrier; T20 and T21 build the other three.
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
  `setfenv` and `_G`, is unmeasured; the model's stub still evaluates nothing.
- **Declared before served, and absent before counted.** The handshake
  publishes `ops`, `states`, `eval` and the five figures from the first load,
  the two instruction figures provisional; the task that lands each reads its
  constant. `eval` serves every state but `missionscripting`, "not yet served"
  until T21. No `cpu_ms` until T23, no `budget` until T24. No `state` is
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
