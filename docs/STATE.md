# Working state

**Last updated:** 2026-09-20

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

Nothing. T57 landed and Milestone B closed with it.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T57** — the mutation sweep: `sh tools/sweep.sh` performs 31 of the 71 mutations the Stage 3–6 plan cells name, 3m31s warm, all 31 red, tree identical. 40 are out of scope and the run says so, as it says what its figure counts. `docs/mutations.md` is the inventory, `sweep-cover.sh` holds it to the plan, `.github/workflows/sweep.yml` runs it weekly — **the maintainer** sees that first scheduled run.
- **T53** — the two whole echoes cut at eighty: `ping: 336`, `eval-hook: 672`, `standin` 34, harness 8545. Comparing the dialects first turned up a third: the stand-in echoed a malformed `state` whole where the Lua already cut it. All five mutations seen red. The pull request holds the rest.
- **T36** — the game-state derivation: `game_state` prints 92, the crate 448. Six axes and the `ui` record, every `unknown` naming its reason, no axis filled from another's evidence (ADR 0018). The axes the first live run is meant to fill in are left undecided on purpose. All eight mutations seen red — one only after the sweep was widened to spoil every arm rather than one in seven.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T37** — Milestone C opens: `rmcp` stdio wiring, the `serve` role, the
client built per call. Done when `cargo test -p dcs-mcp serve` shows only
protocol frames on stdout and diagnostics on stderr; the mutations are a
diagnostic written to stdout and a client resolved once at start-up.

Stages 7 and 8 were re-split before being built, ten rows into fourteen; the
stage prose in `docs/PLAN.md` says why. Stage 8 needs nothing from Stage 7, so
the two build as separate fronts.

**An agent verifies** every row of both stages: Milestone C is proved off DCS.

## After that

- **Stages 0 to 2 are closed, and Milestone A with them:** the wire proven off
  DCS, the stand-in, the interop and round-trip controls. T17 closed it.
- **Stage 3 is closed:** eval across the carriers with `<file>:47` true in
  every state, and the `a_do_script` shift reproduced.
- **Stage 4's path is closed and Stage 5 has opened:** `tick-budget: 524`, `instr-budget: 2209`, `fence: 583`, `events: 114`, `dormant: 40`, `arming: 95`, `heartbeat: 300`. Stage 6 has opened on the client side: `paths: 15`, `readers: 24`, `wait: 32`, `status: 24`, `id: 7`, `pipeline: 17`, `file_refusals: 27`, `file_source: 40`, `watch: 13`, `game_reads: 55`, `game_state: 92` under `cargo test`, the crate at 448.
  Milestone B is closed: every Stage 3 to 6 row is built and swept.
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
- **Three sweeps are owed, no row asks for any.** The disarm's own (its heartbeat half is T29's),
  Stages 0 to 2, which `sweep-cover.sh` does not reach, and the mutations proved past what a plan cell named — a dozen at T35 — which are in neither figure the run prints.
- **The held sibling is a held file.** The sweep's "cannot be removed" path is proved
  with a file the suite keeps open. T56's directory handle refuses a removal here
  too; that no server holds one between tool calls is T41's.
- **The round trip's `superseded` half is unblocked and unclaimed.** T17's row
  parks it on T54, which landed the fence against the stand-in and does not ask
  for it; no row claims it against the shipped Lua.
- **A `Minter` has no owner.** The window mints from whatever it was handed; who
  holds one across tool calls, so two do not restart at seq 1, is Stage 7's.
  `Minter::seeded_at` resumes a counter.
