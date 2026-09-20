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

Nothing. **Milestone C is closed:** all fourteen Stage 7 and Stage 8 rows are on
`main`, and the sweep the cadence rule asks for before a milestone closes was
run against `906282a`: **57 reddened, 0 stayed green, 0 unperformed, 0
failures**, 58 of 58 in scope, tree identical afterward.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T40, T45, T46** — the read-and-eval CLI verbs with `--out`/`--capture` worded by the one renderer; `uninstall` removing its own `dofile` line by whole-line equality and restoring what it parked; `verify` reading everything and writing nothing. **Stage 8 is closed.**
- **T58, T59** — the run record takes the reader's own digest, never a recomputed one, and a file eval refused before it read a byte writes no line (ADR 0021); sixty seconds of silence wake nothing, measured on the runtime's own paused clock rather than by sleeping. **Stage 7 is closed.**
- **T42, T43, T60, T44, T61** — the installer's half of Milestone C: the embedded executor and its shipped-hash list, the `Saved Games` locator, the register and park store, the hook placed by rename, the `Export.lua` line. ADR 0019, ADR 0020.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Stage 9**, the live proofs. It is the critical path, it cannot be shortened by
parallel effort, and **only the maintainer can verify any of it**, at a running
install. Everything provable off DCS is now proved.

The MCP registration still points at `dcs-api-bridge`; the swap was parked on
Milestone C, which has now closed. That is the maintainer's call to make.

## After that

- **Milestone A is closed:** Stages 0 to 2 — the wire proven off DCS, the
  stand-in, the interop and round-trip controls.
- **Milestone B is closed:** Stages 3 to 6 — eval across the carriers with
  `<file>:47` true in every state, the budgets and crash safety, the dormant
  frame, and the client library. Every row built and swept; figures in git log.
- **Milestone C is closed:** Stages 7 and 8 — the MCP server and its six tools,
  one wording for every reply, the CLI, the run record, the server's idle and
  the installer under park-and-restore. Fourteen rows, 26 controls all seen red.

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
