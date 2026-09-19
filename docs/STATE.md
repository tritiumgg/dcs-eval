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

- **T53** — the two whole echoes cut at eighty: `ping: 336`, `eval-hook: 672`, `standin` 34 tests, the harness 23 suites and 8545 checks. Comparing the dialects first found the stand-in echoing a malformed `state` whole where the shipped Lua already cut it, fixed before the two new cuts were built on it; both now call their own dialect's existing excerpt, and two interop plants compare the resulting bytes rather than the same literal typed twice. All five mutations were seen red from a saved copy and `cmp`ed back, each reddening its own echo's check and the interop body beside it and nothing else.
- **T36** — the game-state derivation: `cargo test -p dcs-eval game_state` prints 92 and the crate 448. Six axes and the `ui` record, every `unknown` carrying its reason, one heartbeat verdict taken before any axis and no axis filled from another's evidence (ADR 0018); all eight mutations were seen red, and the clippy control found `wildcard_enum_match_arm` silent on a single-variant `_`, so a second deny sits beside it. A review then found the sweep spoiled one arm in seven per axis, and the named mutation — `pause` filled from the callback phase in the arm it never spoiled — ran green across the whole crate; the sweep now loops every arm of the match over an answer — the right type carrying a word that type has no value for included, which a second review found still unswept and reproduced in `pause` a second time — beside the probe outcomes and the ping refusals that are unknowns rather than values, and that mutation reddens it. The same review found `session` calling a definite `loading` or `menu` activity unknown, so the three unknowns outside a mission now carry three reasons, each quoting the gate rather than claiming a probe it never looked at was withheld. **Nothing here saw DCS**: every value is the stand-in told what to say, and what `getPause` really returns, whether a joined client really refuses the `gui` probe and whether the phase really says `sim` for a mission that began paused are Stage 9's.
- **T35** — the game-state reads: `cargo test -p dcs-eval game_reads` prints 53, `standin` 34, the crate 354. Nine reads in a constant table, one `pcall` each, tier 2 built and off; the ping, the five reads and the `gui` probe share one window and one tick. An answer is five arms with no default so an errored, absent, false and malformed read cannot be confused, its `Unanswered` carries the status and stage as fields rather than as prose, and the ping and the probe answer on types of their own — a probe that says `ok` is `Reachable` and not a body this side cannot read (ADR 0017), and `vet` refuses a never-sent or unlisted name on the bytes at the publication seam, with the stand-in's raw-byte ledger sweeping what actually reached the disk. **All twelve mutations reddened**, an agent watching each run and restoring from a saved copy; three did not say what was predicted — the named `DCS.getMissionLoaded` added to the table reddens the sweep by refusing the whole gather rather than by a request in the ledger, and the batched chunk and the depth of 1 redden through the ticker's own wait ("waited for 7 .req files and saw 3") rather than on a count. Tier 2 sent while off left the tier-two skip check green, because two entries then sat under one key and the lookup found the first; it now counts entries too, in a commit of its own. **Every answer here is the stand-in's, told what to say**: what a real `DCS.getPause` returns, whether the five answer on a joined client and whether any crashes the hook state are Stage 9's. The never-send claim is about what leaves this side, which is the claim it can support. A review after that sweep moved the probe and the ping onto answers of their own and `Unanswered` onto fields; those are covered by tests and by no mutation.

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
