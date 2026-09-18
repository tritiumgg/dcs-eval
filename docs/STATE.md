# Working state

**Last updated:** 2026-09-17

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

Nothing. T26 landed; T27 has not started.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T26** — the events log and its markers: `events: 114 checks`. The load moves the last generation to `events.prev.log` and opens a new one with its banner; every dispatch is bracketed `B|<id>|<op>|<state>|<stamp>` and `O|<id>|<status>|<cpu_ms>` with the reply's own figures, a request refused before dispatch gets no pair, a field a client spells is cut at eighty bytes and stripped of the separator (ADR 0007), and the crossing into `missionscripting` is marked through `log.write` inside `mission`. The killer is read out of the file as the chunk that would have killed DCS sees it, by a reader kept in `tools/harness/crasher.lua`; thirteen mutations seen red.
- **T25** — the stamp fence: `fence: 583 checks`. A `for` that is not this session's stamp is `stale-session` before an op is looked for, echoing the stamp it named whole and uncapped (ADR 0006), with nothing compiled, run or sent to a state — foreign `mission` and `missionscripting` requests pin the send at one hop and at two, and each host's control names a state that host serves — for a request in the session directory at load and one published on a tick; the stand-in and the interop control carry the new reply; fifteen mutations seen red.
- **T24** — the instruction budget: `instr-budget: 2209 checks`. RUN source beside CONVERT sets a count hook in the chunk's state (`stage: budget`, position kept, spent stays spent, the executor's own gap ignored, a hook already set left alone as `none`); `max_instructions` read and clamped; `budget` after `chunkname`; the wrapper answers four fields (ADR 0005 supersedes 0003); bad figures stop the load; eight mutations seen red.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T27** — the dormant path: zero allocations and zero kernel entries
per frame, one `lfs.attributes` every `PROBE_EVERY` frames, as a byte-count
control. Done when `lua5.1 tools/harness.lua executor/dormant` shows 100,000
dormant ticks growing `collectgarbage('count')` by zero and 12,500 ticks
calling the stubbed `lfs.attributes` exactly 12,500 times, `lfs.dir` never;
mutation: reintroducing the per-call closure in the callback wrapper reddens
the byte count. Needs T04, T12.

**An agent verifies** it: the harness under `lua5.1.exe` on PATH under mise.

## After that

- **Stages 0 to 2 are closed, and Milestone A with them:** the wire proven off
  DCS, the stand-in, the interop and round-trip controls. T17 closed it.
- **Stage 3 is closed:** eval across the carriers with `<file>:47` true in
  every state, and the `a_do_script` shift reproduced.
- **Stage 4's path is closed:** `tick-budget: 524`, `instr-budget: 2209`,
  `fence: 583`, `events: 114`. Milestone B needs T53, Stages 5 and 6, the mutation sweep.
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
  carrier is proved against the incumbent's measured shift and `...`, not T49's.
  Unmeasured too: DCS's `os.clock` against the harness's, a count hook raising
  inside either carrier, a DCS state already holding one (`none`, ADR 0005),
  and what `dcs.log` renders around a crossing's markers, which the reader
  is written not to depend on (ADR 0007).
- **Declared before served, and absent before counted.** The handshake
  publishes `ops`, `states`, `eval` and the five figures from the first load,
  the two instruction figures provisional. No `state` is `bad-request`: the maintainer's call, 2026-09-11.
- **A path with a byte past ASCII stops the load.** Header values are ASCII,
  a user name past ASCII puts such a byte in every path the handshake names,
  and the executor refuses the file with the header named in `dcs.log`. The
  specification says nothing (`docs/audit.md`, Open). The client's parser
  refuses such a value as the executor does, the maintainer's call at T13;
  a spelling for such a path on the wire is the writer's side, unsettled.
- **The held sibling is a held file.** The sweep's "cannot be removed" path is
  proved with a file the suite keeps open; a client's `ReadDirectoryChangesW` handle is Stage 9's.
- **The client's half of the fence waits on T31,** whose row now names it: the
  executor fences a foreign `for` since T25, and discarding a reply with
  another session's `stamp`, with the round-trip's `superseded`, needs `collect`.
