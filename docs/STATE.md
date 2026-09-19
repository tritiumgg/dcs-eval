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

Nothing. T53 landed; T57 has not started.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T53** — the two whole echoes cut at eighty: `ping: 336`, `eval-hook: 672`, `standin` 34, harness 8545. Comparing the dialects first turned up a third: the stand-in echoed a malformed `state` whole where the Lua already cut it. All five mutations seen red. The pull request holds the rest.
- **T36** — the game-state derivation: `game_state` prints 92, the crate 448. Six axes and the `ui` record, every `unknown` naming its reason, no axis filled from another's evidence (ADR 0018). The axes the first live run is meant to fill in are left undecided on purpose. All eight mutations seen red — one only after the sweep was widened to spoil every arm rather than one in seven.
- **T35** — the game-state reads: `game_reads` prints 53, the crate 354. Nine reads in a constant table, one `pcall` each, tier 2 built and off; an answer has five arms so an errored, absent, false and malformed read stay distinct (ADR 0017). `vet` refuses a never-sent or unlisted name at the publication seam. All twelve mutations seen red, three saying something other than what was predicted.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T57** — the mutation sweep, scripted: one runner that applies each
control's named mutation to a copy, runs the one command that must redden,
restores and reports. Done when `sh tools/sweep.sh` prints one row per control
and exits non-zero if any control stayed green or any file did not come back
identical; a mutation that no longer applies is reported unperformed, never
skipped silently, and restoring with `git checkout` reddens the tree-clean
check.

**An agent verifies** it here: every control it drives already runs off DCS.

## After that

- **Stages 0 to 2 are closed, and Milestone A with them:** the wire proven off
  DCS, the stand-in, the interop and round-trip controls. T17 closed it.
- **Stage 3 is closed:** eval across the carriers with `<file>:47` true in
  every state, and the `a_do_script` shift reproduced.
- **Stage 4's path is closed and Stage 5 has opened:** `tick-budget: 524`, `instr-budget: 2209`, `fence: 583`, `events: 114`, `dormant: 40`, `arming: 95`, `heartbeat: 300`. Stage 6 has opened on the client side: `paths: 15`, `readers: 24`, `wait: 32`, `status: 24`, `id: 7`, `pipeline: 17`, `file_refusals: 27`, `file_source: 40`, `watch: 13`, `game_reads: 55`, `game_state: 92` under `cargo test`, the crate at 448.
  Milestone B needs the rest of Stage 5 and the mutation sweep.
- **Stage 9** is the critical path and cannot be shortened by parallel effort.
  Everything provable off DCS is proved before it.

## Carries forward

Things that must not be lost between sessions. Delete an entry when it is
resolved, and say where. Mark an entry only the maintainer can settle. Ten
entries at most: an eleventh means something here is finished, or belongs in
`docs/decisions/` or `CLAUDE.md` instead.

- **Cutover — the incumbent goes before this executor installs.** `dcs-api-bridge`'s
  `DcsApiEval.lua` sits in the same `Scripts\Hooks\`; ADR 0002 is why the names
  differ; T52 proves two hooks on one root cannot happen.
- **Maintainer decision — when the MCP registration is swapped.** Claude Code still
  points at `dcs-api-bridge`; the swap strands a session mid-task, so it happens at
  Milestone C. ADR 0001.
- **What only Stage 9 sees.** The load sentinel covers two: the hook guard's
  swallow path, which has no seam until a raising stub sits on the frame path, and
  whether the export state survives between missions. The wrapper is proved under
  the suite's own carrier alone — what DCS does with one raising inside
  `net.dostring_in`, and whether every state has `setfenv` and `_G`, is unmeasured,
  the model's stubs evaluating nothing — and the carrier against the incumbent's
  measured shift and `...`, not T49's. Unmeasured too: DCS's `os.clock` and
  `os.time` against the harness's, a count hook raising inside either carrier or
  already held by a state (`none`, ADR 0005), what `dcs.log` renders around a
  crossing's markers, which the reader ignores (ADR 0007), and what
  `lfs.tempdir()` really gives, which every fixture supplies by hand.
- **Declared before served, and absent before counted.** The handshake
  publishes `ops`, `states`, `eval` and the five figures from the first load,
  the two instruction figures provisional. No `state` is `bad-request`: the maintainer's call, 2026-09-11.
- **A path with a byte past ASCII stops the load.** Header values are ASCII, so a user name past ASCII puts one in every path the handshake names and the load refuses, naming the header in `dcs.log` (`docs/audit.md`, Open: the spec says nothing).
  The client parses it as the executor does, the maintainer's call at T13; a
  spelling for such a path on the wire is the writer's side, unsettled.
- **The disarm owes a sweep, and no row asks for one.** Its heartbeat half is
  built (T29); the sweep on that same disarm is unclaimed.
- **The held sibling is a held file.** The sweep's "cannot be removed" path is proved
  with a file the suite keeps open. T56's directory handle refuses a removal here
  too; that no server holds one between tool calls is T41's.
- **The round trip's `superseded` half is unblocked and unclaimed.** T17's row
  parks it on T54, which landed the fence against the stand-in and does not ask
  for it; no row claims it against the shipped Lua.
- **A `Minter` has no owner.** The window mints from whatever it was handed; who
  holds one across tool calls, so two do not restart at seq 1, is Stage 7's.
  `Minter::seeded_at` resumes a counter.
