# Working state

**Last updated:** 2026-09-19

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

Nothing. T30 landed; T31 has not started.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T30** — client path containment: `paths: 15 checks`. `resolve` refuses anything not absolute — `C:x` and `\x` included, because the drive they land on is an accident of where the client was started — then collapses `.` and `..` without letting `..` climb past the drive, canonicalizes the nearest existing ancestor, drops the `\\?\`, and reattaches the tail that does not exist yet. Only a `Real` has `contains`, which folds ASCII case over the encoded bytes and matches at a segment boundary. Proved: a short spelling and a junction each resolving to what they name, a laundering junction inside a permitted root not laundering anything, a short-spelled root still holding what sneaks in under it, `LogsX` not under `Logs`, a drive root holding everything on it, and a path no part of which resolves refused. Both mutations were seen red — resolution left textual reddens six, the boundary dropped reddens the sibling. An agent verified every claim itself on this machine under `mise exec -- cargo test`; the sandbox's short-name and junction helpers panic rather than skip where the volume cannot make one. It did not see, and does not claim, that a junction DCS would put in the way behaves as the one the test makes.
- **T29** — the heartbeat writer: `heartbeat: 300 checks`. `<output>\heartbeat.txt` is one envelope carrying `protocol`, `host`, `stamp`, `transport`, `phase`, `armed`, `since`, `ticks`, `last_callback` and `callbacks` (ADR 0010 names the four of the specification's table this session does not keep, and why). It is written at every arm, at every disarm, on every phase change and every 2 s while armed, off the one `os.time` an armed frame now reads for both the beat and the quiet window (ADR 0009, which narrows ADR 0008's cost sentence and leaves its decision standing; `executor/arming`'s "reads no wall clock" check became "reads it exactly once"). A dormant frame writes nothing at all, and every phase change also appends `phase|<stamp>|<from>|<to>|<tick>` to `events.log`, whose first field is neither `B` nor `O`. Proved for both hosts: zero writes across 160 dormant frames spanning 160 intervals, one per transition, one per change and none for a callback repeating its phase, one per interval idle and busy alike, a transition and a due beat on one frame writing one file, and a hand-armed session's first armed frame answering without a raise. The mutation was seen red — a dormant path that keeps beating writes 160 where the suite wants zero. An agent verified every claim itself under the pinned lua5.1.5: the suite drives the frame, fakes the wall clock through a wrapper on `env.os.time` and counts the writes through one on `env.io.open` rather than trusting a counter the executor keeps about itself. It did not see, and does not claim, what one `os.time` per armed frame costs at DCS's frame rate, whether DCS's `os.time` and `os.date` behave in every state as they do here, or how often a beat lands on a loading screen; all Stage 9's.
- **T28** — arming and disarming: `arming: 95 checks`. The load is asleep; the arm file a client writes beside its request wakes it within `PROBE_EVERY`+1 frames, and an armed frame that has listed nothing for `QUIET_S` seconds of `os.time` (ADR 0008) disarms in the order that makes the race unwinnable — arm file removed, one more listing, recreate and stay awake if it holds a request, otherwise record and sleep. Proved: a request published after that listing's snapshot answered, one published before it keeping the executor awake, an abandoned arm file costing one quiet period, a global in a target state outliving a sleep, and every arm and disarm an `events.log` line whose first field is neither `B` nor `O`. The mutation was seen red — the removal below the listing strands the request. An agent verified every claim itself under the pinned lua5.1.5 on this machine: the suite drives the frame, fakes the wall clock through a wrapper on `env.os.time`, counts and hooks the listings through one on `env.lfs.dir`, and reads the log and the arm file off a sandbox. It did not see, and does not claim, what a quiet period costs at DCS's frame rate or whether `os.time` behaves in every DCS state as it does here; both are Stage 9's.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T31** — the handshake and heartbeat readers, and the stand-in
publishing both. Done when `cargo test -p dcs-eval readers` prints its count:
a handshake missing `stamp`, one whose `protocol` is not `2`, one carrying a
byte past ASCII, and a heartbeat whose `armed` is neither `yes` nor `no` each
refused naming the field, the age off the file's mtime and `since`
display-only; mutation: an absent `armed` defaulted to `no` reddens the
tri-state check. Needs T13 and T29, both landed.

**An agent verifies** it: `cargo test` under mise on this machine.

## After that

- **Stages 0 to 2 are closed, and Milestone A with them:** the wire proven off
  DCS, the stand-in, the interop and round-trip controls. T17 closed it.
- **Stage 3 is closed:** eval across the carriers with `<file>:47` true in
  every state, and the `a_do_script` shift reproduced.
- **Stage 4's path is closed and Stage 5 has opened:** `tick-budget: 524`, `instr-budget: 2209`, `fence: 583`, `events: 114`, `dormant: 40`, `arming: 95`, `heartbeat: 300`. Stage 6 has opened on the client side: `paths: 15` under `cargo test`.
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
- **The disarm owes a sweep, and no row asks for one.** Its heartbeat half is
  built (T29); the sweep on that same disarm is unclaimed.
- **The held sibling is a held file.** The sweep's "cannot be removed" path is proved
  with a file the suite keeps open; the client's own handle is T56's, unheld at T41.
- **The client's half of the fence waits on T54,** whose row now names it: the
  executor fences a foreign `for` since T25, and discarding a reply with
  another session's `stamp`, with the round-trip's `superseded`, needs `collect`.
  Both now have a heartbeat to read (ADR 0010), absent until the first arm.
