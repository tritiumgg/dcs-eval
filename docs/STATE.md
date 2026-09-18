# Working state

**Last updated:** 2026-09-18

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

Nothing. T28 landed; T29 has not started.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T28** — arming and disarming: `arming: 95 checks`. The load is asleep; the arm file a client writes beside its request wakes it within `PROBE_EVERY`+1 frames, and an armed frame that has listed nothing for `QUIET_S` seconds of `os.time` (ADR 0008) disarms in the order that makes the race unwinnable — arm file removed, one more listing, recreate and stay awake if it holds a request, otherwise record and sleep. Proved: a request published after that listing's snapshot answered, one published before it keeping the executor awake, an abandoned arm file costing one quiet period, a global in a target state outliving a sleep, and every arm and disarm an `events.log` line whose first field is neither `B` nor `O`. The mutation was seen red — the removal below the listing strands the request. An agent verified every claim itself under the pinned lua5.1.5 on this machine: the suite drives the frame, fakes the wall clock through a wrapper on `env.os.time`, counts and hooks the listings through one on `env.lfs.dir`, and reads the log and the arm file off a sandbox. It did not see, and does not claim, what a quiet period costs at DCS's frame rate or whether `os.time` behaves in every DCS state as it does here; both are Stage 9's.
- **T27** — the dormant path: `dormant: 40 checks`. A frame that is not armed advances its counter and returns — no clock read, no listing, no table, no record — and probes the arm file through an `lfs.attributes` held from the load one frame in eight and on no other; a hundred thousand such frames grow `collectgarbage('count')` by zero once the callback wrapper's one-off 0.797 KB is paid, `lfs.dir` and `os.clock` are never called, and a probe that answers arms the frame again, so the path is escapable. An agent verified every claim itself under the pinned lua5.1.5 on this machine; the per-frame cost inside DCS is not claimed and stays T48's row, which now carries the baseline and its caveat.
- **T26** — the events log and its markers: `events: 114 checks`. The load moves the last generation to `events.prev.log` and opens a new one with its banner; every dispatch is bracketed `B|<id>|<op>|<state>|<stamp>` and `O|<id>|<status>|<cpu_ms>` with the reply's own figures, a request refused before dispatch gets no pair, a field a client spells is cut at eighty bytes and stripped of the separator (ADR 0007), and the crossing into `missionscripting` is marked through `log.write` inside `mission`. The killer is read out of the file as the chunk that would have killed DCS sees it, by a reader kept in `tools/harness/crasher.lua`; thirteen mutations seen red.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T29** — the heartbeat writer: `armed`, `since`, `phase`, `ticks`,
`last_callback`, `callbacks`, event-driven while dormant and every 2 s while
armed. Done when `executor/heartbeat` shows it written at every arm, disarm
and phase change and never per dormant frame, with every phase change also
appended to `events.log` under a first field that is neither `B` nor `O` (ADR
0007); mutation: a dormant executor that keeps writing every 2 s reddens the
dormant-write-count check. Needs T28.

**An agent verifies** it: the harness under `lua5.1.exe` on PATH under mise.

## After that

- **Stages 0 to 2 are closed, and Milestone A with them:** the wire proven off
  DCS, the stand-in, the interop and round-trip controls. T17 closed it.
- **Stage 3 is closed:** eval across the carriers with `<file>:47` true in
  every state, and the `a_do_script` shift reproduced.
- **Stage 4's path is closed and Stage 5 has opened:** `tick-budget: 524`, `instr-budget: 2209`, `fence: 583`, `events: 114`, `dormant: 40`, `arming: 95`.
  Milestone B needs T53, the rest of Stages 5 and 6, the mutation sweep.
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
- **What only Stage 9 sees.** The load sentinel covers two: the hook guard's
  swallow path, which has no seam until a raising stub sits on the frame path,
  and whether the export state survives between missions. The wrapper is proved
  under the suite's own carrier alone — what DCS does with one raising inside
  `net.dostring_in`, and whether every state has `setfenv` and `_G`, is
  unmeasured, the model's stubs evaluating nothing — and the carrier against the
  incumbent's measured shift and `...`, not T49's. Unmeasured too: DCS's
  `os.clock` and `os.time` against the harness's, a count hook raising inside
  either carrier or already held by a state (`none`, ADR 0005), and what
  `dcs.log` renders around a crossing's markers, which the reader ignores (ADR
  0007).
- **Declared before served, and absent before counted.** The handshake
  publishes `ops`, `states`, `eval` and the five figures from the first load,
  the two instruction figures provisional. No `state` is `bad-request`: the maintainer's call, 2026-09-11.
- **A path with a byte past ASCII stops the load.** Header values are ASCII, so
  a user name past ASCII puts one in every path the handshake names and the load
  refuses, naming the header in `dcs.log` (`docs/audit.md`, Open: the
  specification says nothing). The client's parser refuses such a value as the
  executor does, the maintainer's call at T13; a spelling for such a path on the
  wire is the writer's side, unsettled.
- **The disarm owes a sweep, and no row asks for one.** T29 owns only the
  heartbeat half of the transition the specification also sweeps once on.
- **The held sibling is a held file.** The sweep's "cannot be removed" path is
  proved with a file the suite keeps open; a client's `ReadDirectoryChangesW` handle is Stage 9's.
- **The client's half of the fence waits on T31,** whose row now names it: the
  executor fences a foreign `for` since T25, and discarding a reply with
  another session's `stamp`, with the round-trip's `superseded`, needs `collect`.
