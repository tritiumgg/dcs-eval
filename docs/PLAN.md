# Implementation plan: the eval executor and the MCP server that fronts it

This plan builds two halves of one product in one repository: the in-game Lua executor
(`bridge.md`) and the Rust client library and MCP server that sit on it (`mcp.md`). The two
documents are the settled design; this plan constructs them and does not redesign either. Every
done-condition below is mined from `bridge.md` §10 and `mcp.md` §6–§7, which already list what the
repository must prove; where a check is stated, the mutation that must redden it is stated with it.

Because nothing here can be measured before the code runs, there is no "before" column: a task is
defined by the one artefact it adds and the one command whose output shows it done.

**Vocabulary.** This plan says *executor* where the specifications say *bridge*; they are the same
thing, and ADR 0002 says why the build uses its own word. The on-disk names differ with it:
`Scripts\Hooks\DcsEvalExecutor.lua`, `<temp>\dcs-eval\<host>\`, `Logs\DcsEval\<host>\`. Section
citations below point into documents that still say "bridge".

---

## Granularity

This plan carries **64 tasks across 10 stages**. The count is driven by the four right-sizing
tests, and the splits fall at interfaces rather than at steps: each Lua carrier (`hook` local,
`net.dostring_in`, `a_do_script`) is one task because changing how one crosses a state
boundary must not rewrite the others; the client and the server are separate crates and separate
tasks for the same reason; and every behaviour that must survive the game is split into a
*built-and-harness-proven* task (developer-only, off DCS) and a later *proven-live* task (DCS
running), because how a thing is built and whether it survives a real frame are two done-conditions
with two commands. The result is roughly five tasks per subsystem — an order of magnitude below the
~200-row plan the consuming project abandoned — and no task's done-condition names more than one
command. Where two behaviours share one command and one file and cannot drift apart independently
(the reply framer with its oversize refusal; result conversion with the reply ceiling), they are
one task, as the framer example in the brief requires.

---

## Definition of done, at four levels

Done is never a judgement; it is output a person reads. "Green" never means "exited 0": every gate
names a count, a diff, or a test that reddens under a stated mutation.

- **A task is done** when its one command prints the stated number or an empty diff. Where the task
  builds a check, that check must go red under the mutation named in its "done when" cell — a check
  that cannot be shown to fail is not evidence.
- **A stage is done** when every task in it is done and the stage's named command re-runs them
  together: `lua5.1 tools/harness.lua <stage-glob>` for Lua stages (asserting the check count, never
  the exit code, since the harness exits 2 for "nothing ran"), `cargo test -p <crate> <stage-mod>`
  for Rust stages, and for the live stage the stage script that prints each measured row.
- **A milestone is done** when its acceptance criteria, stated as commands below, pass.
- **The project is done** (T51) when, on a real machine, a person installs the binary, an agent
  evaluates Lua in every reachable state and reads the game's state, and the executor is left
  installed through ordinary play and a DCS update with `dcs-mcp verify` still green afterward —
  each a command, none an opinion.

---

## Standing rules, decisions, and what is out of plan

**Standing rules** (stated once here, each enforced by the task named; not repeated as rows):

1. **Lua is 5.1.5 PUC-Rio, never LuaJIT, never 5.4.** A green run under a dev machine's 5.4 says
   nothing. Enforced by T01 (the pinned interpreter and its guard) and by every Lua stage command
   running under `lua5.1`.
2. **The DCS install is read-only, always, including a probe of whether it is writable.** Enforced
   by T07 (executor containment) and by every Stage 8 row (the installer never writes the install
   and never probes writability by writing).
3. **`Saved Games` is writable under park-and-restore; nothing there that is not this project's is
   ever lost — a displaced file is registered and moved, never deleted or overwritten in place.**
   Enforced by T60, which builds the register and the park store, and by T44, T61 and T45, the only
   rows that move a file under `Saved Games`.
4. **The idle budget is a gate, not a goal.** A dormant frame is zero allocations, zero kernel
   entries; one `lfs.attributes` every `PROBE_EVERY` frames. Enforced by T27 as a byte-count control
   and by T48 as an in-process per-frame measurement inside DCS (ADR 0025); a dormant figure above the baseline reopens `bridge.md`
   §2 (§2.3).
5. **Every task is marked `developer-only` or `DCS + human` and sequenced on it.** The DCS tasks are
   Stage 9, last, because they are wall-clock-bound and cannot be parallelised by adding effort —
   with two developer-only rows, T62 and T63, filed there because they are T50's switch and
   T52's verbs.

**Decision records** (this plan makes two; each carries a revisit condition):

- **DR-1 — One repository, both halves; the binary embeds the repo's own built `DcsEvalExecutor.lua`.**
  `mcp.md` §0 and §5.5 frame `dcs-mcp` as a standalone project embedding a *released* executor script
  from a separate executor project. The brief places both halves in one repository. Resolution: one
  repository; the build embeds the repository's own built `DcsEvalExecutor.lua` via `include_bytes!` with its
  SHA-256 (T42), and "the executor release the binary carries" is that in-repo artefact's hash.
  *Revisit if* a second consumer needs the executor versioned and released independently of the
  server's cadence — then split the crate out and the wire, unchanged, becomes the seam.
- **DR-2 — Windows is the only supported target for the binary and the path-canonicalisation CI.**
  Both documents are Windows-specific (`\\?\` prefix, `FOLDERID_SavedGames`, NTFS timestamp
  behaviour, 8.3 short names). The executor Lua is OS-neutral and its harness runs anywhere; the Rust
  containment controls (T30) require Windows. *Revisit if* a non-Windows client is required.

**Out of plan** (unbounded, demand-driven; the machinery is built and a named party decides what
gets done — no task with no honest terminal state):

- **Which DCS calls are dangerous — the hazards catalogue.** `bridge.md` §11 states plainly this is
  not the executor's question. The executor carries only the marker discipline that makes a crash name
  its killer (T26). Cataloguing dangerous calls belongs to the consuming census project, on its own
  records.
- **Consumer walkers, record grammars, drivers, cursors.** `bridge.md` §7.0 moved `census` and
  `reflect` out of the protocol. This project ships the two ops and the wrapper; a consumer ships
  the chunk it evaluates and decides what to walk. `mcp.md` keeps no catalogue of reads beyond the
  tier-1 list and its seven opt-in reads (T35, T62).

**Spikes** (open questions with exit conditions, folded into the live tasks rather than left as
rows): editor-vs-menu detection (T50; exit: one of three candidates measured, else `menu-or-editor`
stands); whether the seven opt-in reads are safe from a hook (T50; exit: each sent alone under the
supervisor, one per session, and the result recorded). Neither blocks Milestones A–C.

---

## Milestones

- **Milestone A — The wire proven.** Someone can evaluate a chunk through the *actual shipped Lua
  executor* from the *actual Rust client*, off DCS, in CI — proving the two implementations agree on
  the bytes before anything else is built on them. This is the net for the whole port (`mcp.md`
  §1.4: the interop control first, the stand-in second). *Acceptance:* `cargo test -p dcs-eval
  interop` is green with the interpreter present, and the round-trip control (T17) drives a
  harnessed `DcsEvalExecutor.lua` to answer a `ping` the client reads back; a byte of the executor's
  `frame` that changes the wire reddens interop. Covers Stages 0–2.
- **Milestone B — The full protocol and its controls.** Someone can trust every §7 behaviour
  without a sortie: line-truth in every carrier, the oversize refusal, the tick and instruction
  budgets, the stamp fence, the dormant budget as a number, the arming race, and the seven-state
  `a_do_script` — each red under its stated mutation. *Acceptance:* `lua5.1 tools/harness.lua executor` and
  `cargo test -p dcs-eval` both print their full check counts, all green, and a scripted mutation
  sweep shows every §10/§7 control reddening. Covers Stages 3–6.
- **Milestone C — Installable and serving.** Someone can install the executor from one binary under
  park-and-restore, verify it, and an MCP agent can list and call six tools. *Acceptance:*
  `dcs-mcp install --saved-games <fixture>` then `dcs-mcp verify` over the same fixture prints
  `verified` once a session is published there, which could be run only from T63 on, because the
  binary had no installer verb when this milestone closed; an in-memory MCP session lists six
  tools and calls each; `git status` on the fixture tree after `verify` is clean (verify wrote
  nothing). Covers Stages 7–8.
- **Milestone D — Proven live in DCS.** Someone can do the whole thing for real: install, evaluate
  in every reachable state including `missionscripting` through `a_do_script`, read game state, and leave the executor
  installed through play and a DCS update. *Acceptance:* `dcs-mcp live report` prints every
  row, measured or `unmeasured` with its reason, and the permanent-install acceptance (T51) passes;
  the dormant per-frame cost (T48) is at or below the baseline. Covers Stage 9.

The critical path runs through Stage 9: its six DCS + human tasks need DCS running with a person to
set the scene, are wall-clock-bound, and cannot be shortened by parallel effort — so every off-DCS
control that *can* be built and harness-proven earlier is, and Stage 9 inherits only what genuinely
needs a real frame, and T62 and T63, the switch T50 turns and the verbs T52 runs.

---

## Stage 0 — Toolchain and harness floor

The floor everything else stands on. Nothing loads into DCS untested; the harness models the seven
states strictly (an unmodelled name raises where it is read) and runs under the 5.1 interpreter.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T01 | The pinned reference Lua 5.1.5 PUC-Rio interpreter and a guard refusing any other | `tools/check-lua` prints `lua5.1 5.1.5`; mutation: pointing it at a 5.4 binary makes the guard print the version and exit non-zero | — | developer-only |
| T02 | The Cargo workspace: `dcs-eval` (lib) and `dcs-mcp` (bin) skeletons | `cargo build` prints both crate names and produces `dcs-mcp.exe`; mutation: removing a crate from the workspace members reddens the build | — | developer-only |
| T03 | The Lua harness runner: loads `DcsEvalExecutor.lua`, counts checks, exits 2 for "nothing ran", strict-by-default | `lua5.1 tools/harness.lua selftest` prints `selftest: 1 check` and exits 0; mutation: a test with no assertions makes it exit 2 | T01 | developer-only |
| T04 | The DCS state stubs: seven global tables per §1's surface, `net.dostring_in` returns `''`, `lfs`/`io`/`os`/`debug` models | `lua5.1 tools/harness.lua stubs` prints the modelled-name count and asserts `net.dostring_in` returns empty; mutation: a stub that evaluates instead of returning `''` reddens the empty-answer check | T03 | developer-only |
| T05 | The CI workflow running the harness under 5.1 and `cargo test` | a CI run prints both suites' check counts; mutation: removing the interpreter step makes the interop job (T16) red, not skipped | T01,T02,T03 | developer-only |

**Stage command:** `tools/check-lua && lua5.1 tools/harness.lua selftest stubs && cargo build`.

---

## Stage 1 — The load shell and the envelope   *(Milestone A)*

The file DCS loads, the directory it writes into, and the request/reply envelope — everything the
wire needs before an op runs.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T06 | Host detection and the two registration tails, both driven from one file, under one top-level `pcall` | `lua5.1 tools/harness.lua executor/load` drives both `hook` and `export` from one file and prints `load: N checks` (count asserted, not exit code); mutation: a harness whose `DCS.setUserCallbacks` raises must still write one `dcs.log` line and register nothing — removing the top-level `pcall` reddens this | T04 | developer-only |
| T07 | Executor containment and transport-root selection with the `Logs\` fallback | `lua5.1 tools/harness.lua executor/containment` prints its check count; mutations: a path inside the install, a relative path, and `Saved Games` outside `Logs\` (including `Logs\..\Config`) must each be refused — allowing any one reddens the suite | T06 | developer-only |
| T08 | The session directory, the `<os.time>-<os.getpid>` stamp, and the sibling sweep at load | `lua5.1 tools/harness.lua executor/session` shows a sibling directory removed at load and the own stamp kept; mutations: absent `os.getpid` must refuse to run (visible failure, not a weaker fence); a request in a foreign sibling is never listed | T07 | developer-only |
| T09 | The reply framer: header/blank/body, publish-by-rename with `.tmp` in the destination, `os.remove` before `os.rename`, request over 262,144 bytes refused unread | `lua5.1 tools/harness.lua executor/framer` prints its count; mutations: writing the final `.res` name directly (no rename) reddens the half-written-not-collected pair; a 300 KiB request parsed as code reddens the size refusal | T08 | developer-only |
| T10 | The request parser: `name: value` headers, `for` required, empty body `bad-request`, id name-agnostic | `lua5.1 tools/harness.lua executor/request` prints its count; mutations: a request missing `for` that runs anyway reddens the fence precursor; an empty body answered `ok` reddens `bad-request` | T09 | developer-only |
| T11 | The handshake writer (`executor.txt`): every §7.2 field, `protocol: 2` | `lua5.1 tools/harness.lua executor/handshake` asserts all required keys present and `protocol: 2`; mutation: dropping `transport` or `stamp` reddens the key-presence check | T08 | developer-only |

**Stage command:** `lua5.1 tools/harness.lua executor/load executor/containment executor/session
executor/framer executor/request executor/handshake`.

---

## Stage 2 — ping, and the wire proven   *(Milestone A)*

The first op, the client half of the wire, the stand-in, and the interop and round-trip controls
that prove the two implementations agree on bytes.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T12 | The `ping` op | `lua5.1 tools/harness.lua executor/ping` shows a `ping` answered with `phase`, `tick`; mutation: a `ping` reply missing `status` reddens the reply-shape check | T10,T11 | developer-only |
| T13 | Client protocol framing (encode/decode, latin1 body, CRLF-normalised headers, CR/LF-in-value refused) | `cargo test -p dcs-eval protocol` prints its count; mutations: a header value carrying `\n` that is written rather than refused reddens the injection guard; a cp1251 body decoded on the way in reddens the byte-for-byte check | T02 | developer-only |
| T14 | Client publish-by-rename and arm-file-ensure on send | `cargo test -p dcs-eval publish` shows a request landing only under its final name and the arm file created when absent; mutation: a client that removes the arm file reddens the never-removes check | T13 | developer-only |
| T15 | The Rust stand-in executor with a deliberately unshared encoder | `cargo test -p dcs-eval standin` drives a full round trip against the stand-in; mutation: replacing the stand-in encoder with the client's serialiser reddens the "encoder is not the client's" assertion | T13 | developer-only |
| T16 | The interop control: the shipped `DcsEvalExecutor.lua`'s own bytes parse under the Rust client, in CI with the interpreter | `cargo test -p dcs-eval interop` green with `lua5.1` present; mutation: a byte of the executor's `frame` that changes the wire reddens it, a comment byte does not; an empty `net.dostring_in` answer must arrive as an empty answer | T04,T12,T13 | developer-only |
| T17 | The end-to-end round-trip control: Rust client → harnessed `DcsEvalExecutor.lua` → client reads the `ping` reply | `cargo test -p dcs-eval e2e` (spawning `lua5.1` on the shipped file) returns a `pong`/`ok` reply the client parses; mutation: a byte of the executor's `frame` that changes the wire reddens it, and a suite that stops ticking reddens it under the deadline, not a hang; the `superseded` half — a request whose session restarts before a frame takes it, read as `superseded` by the client's `wait` — is a second test under the same `cargo test -p dcs-eval e2e` and is swept under T54, since Stages 0–2 hold no inventory entry | T14,T15,T16 | developer-only |

**Stage command:** `lua5.1 tools/harness.lua executor/ping && cargo test -p dcs-eval
protocol publish standin interop e2e`.  **Milestone A acceptance:** this command green; the T16
`frame` mutation reddens interop.

---

## Stage 3 — eval and line-truth   *(Milestone B)*

The only op that runs anything, across all four carriers, with the promise that `<file>:47` names
line 47 of the caller's file in every state.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T18 | The `hook` eval carrier (`loadstring` + `setfenv` into the host `_G`) with a true `chunkname` | `lua5.1 tools/harness.lua executor/eval-hook` shows a raise on line 47 reported as `<name>:47`; mutation: prepending one line to the body before compiling reddens the line-truth check | T12 | developer-only |
| T19 | Result conversion (`describe_local`: scalars printed, `%.14g`/`%.17g`, `inf`/`-inf`/`nan` by name, tables/functions/userdata by type only) and the reply ceiling — refused, never cut | `lua5.1 tools/harness.lua executor/result` prints its count; mutations: a table stringified with `tostring` reddens the type-only rule; a result over `max_result_bytes` cut instead of refused (`stage: oversize`, `result_bytes`) reddens the ceiling | T18 | developer-only |
| T20 | The `net.dostring_in` carrier and its `%q` wrapper (body compiled in-state), with the three answers kept apart | `lua5.1 tools/harness.lua executor/dostring` shows `ok`/`refused`/`invalid-state` distinct and line 47 true through the wrapper; mutation: reading a `nil` refusal as an empty `ok` reddens the three-answers check | T19 | developer-only |
| T21 | The `missionscripting` carrier, two hops through `a_do_script`: `%q` args, the return-shift correction, string-only conversion, `no-mission` status (ADR 0004) | `lua5.1 tools/harness.lua executor/a_do_script` shows a value crossing as a string only, the slot-2 read, and `no-mission` with no mission; mutation: returning a table through `a_do_script` (not converted in-state) reddens the string-only backstop | T20 | developer-only |
| T22 | The `a_do_script` off-by-one fixture reproduced under reference `lua5.1` | `lua5.1 tools/harness.lua executor/a_do_script-shift` exercises the shift and asserts the correction; mutation: removing the sacrificial `0` the far chunk returns after its payload reddens it (a lone value dropped) | T21 | developer-only |

**Stage command:** `lua5.1 tools/harness.lua executor/eval-hook executor/result executor/dostring
executor/a_do_script executor/a_do_script-shift`.

---

## Stage 4 — Budgets and crash safety   *(Milestone B)*

The three protections generalised from `census` to every eval, the instruction guard that bounds a
single chunk, and the stamp fence and markers that close the 2026-09-02 kill.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T23 | The per-tick CPU budget between requests, and `cpu_ms`/`tick` on every reply | `lua5.1 tools/harness.lua executor/tick-budget` shows the executor stopping new requests past `TICK_BUDGET_MS` and W-deep replies answered in id order sharing a tick; mutation: a reply missing `cpu_ms` reddens the presence check | T19 | developer-only |
| T24 | The instruction count-hook budget inside the chunk, refused at load for `0`/non-integer, `budget: none` in `mission` | `lua5.1 tools/harness.lua executor/instr-budget` shows a looping chunk stopped `stage: budget` where `debug` exists and `budget: none` in `mission`; mutation: an `INSTRUCTION_BUDGET` of `0` silently defaulted (Lua installs no hook, `debug.gethook` still returns it) reddens the load-time refusal | T18 | developer-only |
| T25 | The stamp fence, the executor's half of §4.3: `for` ≠ stamp → `stale-session`, chunk never runs (the reply whose `stamp` is not the session addressed is discarded by the client, and moved to T54 with the `collect` it needs) | `lua5.1 tools/harness.lua executor/fence` shows a foreign `for` answered `stale-session` without running, at load and on a tick; mutation: running the chunk anyway reddens the kill-reproduction control | T10 | developer-only |
| T26 | The events log `B|`/`O|` markers and rotation to `events.prev.log` at load, and `a_do_script`'s own markers through `log.write` inside `mission` (T21 built the carrier without them) | `lua5.1 tools/harness.lua executor/events` shows the last `B|` with no `O|` naming the killer and one generation kept, and a crossing marked in `dcs.log` before and after `a_do_script`; mutation: a synthetic unbalanced file whose killer is misread reddens the reader | T12,T21 | developer-only |
| T53 | The two refusals that echo a request value whole, held to the excerpt their siblings keep: `dispatch` echoes `req.headers.op` and `OPS.eval` echoes a `state` no host serves, where the shape refusals for `state` and `max_instructions` cut at 80 bytes — an op of 262,126 bytes buys a 262,238-byte reply and a state of 262,000 letters a 262,132-byte one (both measured at T25, ADR 0006's Context). It sits last in the stage because it is docked work found while measuring T25, not the path through it | `lua5.1 tools/harness.lua executor/ping executor/eval-hook` shows an op of a hundred bytes and an unserved state of a hundred bytes each named in its message at eighty and three dots, and `cargo test -p dcs-eval standin` the same on the stand-in; mutation: dropping either echo's `excerpt`, in either dialect, reddens that echo's check alone | T12,T15 | developer-only |

**Stage command:** `lua5.1 tools/harness.lua executor/tick-budget executor/instr-budget executor/fence
executor/events executor/ping executor/eval-hook && cargo test -p dcs-eval standin`.  The last three
are T53's: the refusals it bounds are pinned where they were built, in suites Stage 2 owns.

---

## Stage 5 — Dormancy and arming   *(Milestone B)*

The one restructuring the maintainer's idle requirement is about: a executor that costs almost nothing
while the game is played and wakes on a file.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T27 | The dormant path: zero allocations, zero kernel entries per frame, one `lfs.attributes` every `PROBE_EVERY` frames, as a byte-count control | `lua5.1 tools/harness.lua executor/dormant` shows 100,000 dormant ticks growing `collectgarbage('count')` by zero and 12,500 ticks calling the stubbed `lfs.attributes` exactly 12,500 times, `lfs.dir` never; mutation: reintroducing the per-call closure in the callback wrapper reddens the byte count | T04,T12 | developer-only |
| T28 | Arming and disarming: the arm-file wake, the ordered `QUIET_S` disarm, the race proof, and state-in-target surviving a disarm | `lua5.1 tools/harness.lua executor/arming` shows a request published while dormant answered within `PROBE_EVERY`+1 ticks, one published on the disarm tick still answered, an abandoned arm file costing one quiet period, and every arm and disarm appended to `events.log` as a line whose first field is neither `B` nor `O`, which is what ADR 0007 narrowed that file against; mutation: reversing the `os.remove`/list order in disarm reddens the strand-a-request proof | T27 | developer-only |
| T29 | The heartbeat writer: `armed`/`since`/`phase`/`ticks`/`last_callback`/`callbacks`, event-driven while dormant and 2 s while armed | `lua5.1 tools/harness.lua executor/heartbeat` shows it written at every arm, disarm and phase change and never per dormant frame, with every phase change also appended to `events.log` under a first field that is neither `B` nor `O` (ADR 0007); mutation: a dormant executor that keeps writing every 2 s reddens the dormant-write-count check | T28 | developer-only |
| T64 | The uncollected-reply sweep: a reply kept 300 s from the frame it was published on and removed by the first armed frame to find it that old, the disarming frame included, off a ledger of what the session published rather than a listing of `res/` — the client removes nothing it reads — with the removals spent from the tick budget and nothing done while dormant (ADR 0027) | `lua5.1 tools/harness.lua executor/uncollected` shows a reply kept at 299 s and removed at 300 s while armed, one removed by the frame that disarms, one past 300 s still on the disk after a hundred dormant frames and gone once an armed frame runs, a reply already removed costing nothing, a backlog of three cleared across two frames under the 8 ms budget, one removal made by a frame whose requests spent that budget, and the 300 s removal on the export host too; mutations: never calling the sweep reddens the 300 s removal; a limit of zero reddens the reply on the disk the frame it is answered; dropping the budget test reddens the backlog left for the next frame; testing the budget before the first removal reddens the removal made with the budget gone; sweeping on the dormant path reddens the dormant keep; `cargo test -p dcs-mcp tools_listed_wording` shows `dcs_collect` on an id with nothing under it saying the reply may have been removed, and putting back the wording that names only a reply not yet landed reddens it | T29 | developer-only |

**Stage command:** `lua5.1 tools/harness.lua executor/dormant executor/arming executor/heartbeat executor/uncollected`.

---

## Stage 6 — The client library completed   *(Milestone B)*

The `dcs-eval` crate's remaining surface: containment, the readers, the death-vs-silence outcome
table, status, the pipeline, the reply watch, the file source, and the game-state reads and their
derivation.

Two rows here carried two jobs each when the stage was first written. T31 carried the handshake and
heartbeat readers, `collect`, `wait` and the §4.4 table together, which is a file-format interface
and a decision table in one task and about 1,500 lines with its tests; T34 carried what `evalFile`
refuses before it reads a byte and what it does with the bytes afterwards, which `mcp.md` §7 lists
as three separate controls. Both are split at the interface, as this plan's granularity rule asks.
T56 is not a split but a new row: `bridge.md` §3.2's watch is a task's worth of declared Win32 that
`wait` does not need in order to be correct, so `wait` ships on the poll that section keeps as its
fallback and the watch lands on top of it. ADR 0011 is why none of this brings in a crate.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T30 | Client path containment and resolution (`canonicalize`, strip `\\?\`, fold case, segment boundary, 8.3 short names and junctions) | `cargo test -p dcs-eval paths` (on Windows) prints its count, with the sandbox's short-name and junction helpers failing loudly on a volume that cannot make one rather than skipping; mutations: a path admitted through an 8.3 short spelling, a junction, or a byte-prefix match reddens the boundary check | T02 | developer-only |
| T31 | The handshake and heartbeat readers, and the stand-in publishing both — `standin.rs` left both files for "the readers for them" | `cargo test -p dcs-eval readers` prints its count; a handshake missing `stamp`, one whose `protocol` is not `2`, one carrying a byte past ASCII, and a heartbeat whose `armed` is neither `yes` nor `no` are each refused naming the field, and a heartbeat's age is taken from the file's mtime while `since` stays display-only, being local wall clock with no zone; mutation: an absent `armed` defaulted to `no` reddens the tri-state check | T13,T29 | developer-only |
| T54 | Client `collect`/`wait`: a reply on the stamp addressed or discarded as `foreign` (T25 built the executor's half of §4.3), and the §4.4 outcome table, over the 25 ms poll `bridge.md` §3.2 keeps as its fallback | `cargo test -p dcs-eval wait` shows `superseded` on a stamp change, `dead` on a gone PID, `pending`+`waking` for a dormant executor under 10 s, `stalled` past it, `pending` at any age while the phase is `load`, and a reply carrying another session's `stamp` discarded rather than returned; mutations: treating a dormant heartbeat's age as staleness reddens the `armed: no` rule; a `decide` that ignores the re-read stamp, or compares pids in its place, reddens `cargo test -p dcs-eval e2e`, where the shipped executor relaunched over one box, under a new pid or the old one back, must be read `superseded` (the round trip's half T17 defers) | T31,T14,T17 | developer-only |
| T32 | Client `status()` (files + PID probe, problems found, the running `app_version` against the build the embedded executor was last measured on — a difference, never a refusal, and `unmeasured` until Stage 9 records one) | `cargo test -p dcs-eval status` shows a foreign-stamp and a foreign-transport heartbeat each reported as a problem and zero round trips; mutation: a round trip issued by `status` reddens the "costs the executor nothing" check | T31 | developer-only |
| T33 | Client `pipeline(specs, W)` yielding replies in id order, and the id minted — `publish::is_id` validates the shape and nothing mints one yet | `cargo test -p dcs-eval pipeline` shows W-deep sends and in-order replies against a stand-in that answers out of order; mutation: out-of-order yielding reddens the ordering check | T54 | developer-only |
| T34 | What `evalFile` refuses before a byte is read: the ceiling taken off the stat against the handshake's `max_request_bytes`, never assumed — and nothing for where the path lies, since the roots, `Config\` and install rules were removed (ADR 0026, superseding ADR 0014) | `cargo test -p dcs-eval file_refusals` prints its count, showing a file under a write directory's `Config\`, an install-shaped tree and anywhere else admitted, a file one byte over the ceiling refused naming the limit and the size and no content, and one at the ceiling admitted; mutations: opening the file before the ceiling check reddens the guard, whose fixture is a held file the process could not read anyway so that a late refusal cannot pass for an early one; the ceiling check dropped, so an oversize file is admitted, reddens the limit | T30,T31 | developer-only |
| T55 | The file-source reader (`evalFile`): SHA-256 (ADR 0011), BOM strip, shebang blank, CRLF passthrough, `chunkname: @<resolved path>`, and the provenance record | `cargo test -p dcs-eval file_source` shows a BOM+`#`+CRLF file raising on its line 47 and the run record carrying `bom: stripped`, `shebang: blanked`, the byte count and the SHA-256, with the raise asserted against the abbreviated tail Lua prints for a chunkname over 60 bytes, and a file exactly at the ceiling sent whole; mutations: a shebang line removed rather than blanked reddens the line-47 check; a `\r\n` converted on the way in reddens the byte-for-byte check | T34 | developer-only |
| T56 | The event-driven reply watch (`ReadDirectoryChangesW`, declared under ADR 0011), with the poll kept as the fallback `bridge.md` §3.2 requires for a watch that reports nothing | `cargo test -p dcs-eval watch` shows a reply woken on rather than polled for, a watch reporting nothing still answered by the poll, and no handle held once `wait` returns; mutation: a watch left open across a `wait` reddens the handle check | T54 | developer-only |
| T35 | Game-state reads: the tier-1 chunks (one `pcall` each), the constant read-list with its opt-in reads built and left off (T62 adds the switch), never-send-unlisted | `cargo test -p dcs-eval game_reads` shows each read as its own request under one `pcall` and every opt-in read absent from a default gather; mutation: sending a `DCS.*` name not in the constant list reddens the never-send check | T33 | developer-only |
| T36 | Game-state derivation: the axes, `unknown` as a value, no default arm | `cargo test -p dcs-eval game_state` shows `loading` with no round trip, `paused (read)` noting a disagreeing callback phase, `session: client` on a `refused` `gui` probe, `unknown: <error>` on an errored axis alone, `unknown (tier 2 off)` when off; mutation: any axis filled from another's evidence reddens a no-default-arm check | T35,T31 | developer-only |
| T57 | The mutation sweep, scripted: one runner that applies each control's named mutation to a copy, runs the one command that must redden, restores, and reports | `sh tools/sweep.sh` prints one row per control — the mutation, the command, the tests that failed — and exits non-zero if any control stayed green or any file did not come back identical; mutations: a control whose mutation no longer applies must be reported as unperformed rather than skipped silently, and a runner that restores with `git checkout` rather than from its own copy reddens the tree-clean check | T36,T53 | developer-only |

**Stage command:** `cargo test -p dcs-eval paths readers wait status pipeline file_refusals
file_source watch game_reads game_state`.  A filter here is a substring of a Rust test path, so it
carries underscores; the hyphenated filters in later stages do not match anything and are corrected
when their stage is reached.  **Milestone B acceptance:** the Stage 3–6 commands all green,
plus a mutation sweep showing each §10/§7 control reddening under the mutation in its cell.

---

## Stage 7 — The MCP server and CLI   *(Milestone C)*

The `dcs-mcp` binary: `rmcp` wiring, six tools, one wording for one reply, and the CLI that speaks
the same functions.

Two rows here carried more than one check when the stage was first written, and are split at the
interface as this plan's granularity rule asks. T40 carried what `--out` writes, what the provenance
line records and whether the CLI and the tool word a reply alike — a capture rule and a file format
in one row, of which only the capture rule had a mutation; the file format is now T58. T41 carried
the handle rules and the server's own idle together, and `mcp.md` §7 lists them as two controls: a
handle left open is one mechanism and a timer that wakes is another, and the sixty-second control
its prose named was in no done-condition at all. That control is now T59. T37 is not split: the
client built per call is the same file and the same command as the transport, and it simply gains
the mutation it was missing.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T37 | `rmcp` stdio wiring, the `serve` role, the client built per call | `cargo test -p dcs-mcp serve` shows only protocol frames on stdout, diagnostics on stderr, and a transport root that appears between two calls resolved by the second; mutations: a diagnostic written to stdout reddens the transport-cleanliness check; a client resolved once at start-up reddens the appears-between-calls check | T02 | developer-only |
| T38 | The six tools registered, listed and callable over a real MCP session on an in-memory pair | `cargo test -p dcs-mcp tools_listed` lists exactly `dcs_status`, `dcs_ping`, `dcs_game_state`, `dcs_eval`, `dcs_eval_file`, `dcs_collect` and calls each; mutation: a tool that registers but is not listed reddens the count | T37 | developer-only |
| T39 | The reply wording (one place): refusals read as refusals — `no-mission`, `stale-session`, `oversize`, `budget` — and `pending` names its id and phase | `cargo test -p dcs-mcp wording` shows each status worded as a non-empty refusal and `pending` not marked `isError`; mutation: an `oversize` reply worded like an empty result reddens it | T38 | developer-only |
| T40 | The read-and-eval CLI verbs (`status`, `ping`, `game-state`, `eval`, `eval --file`) with `--out`/`--capture` — reply verbatim, nothing for `pending` — and the one wording proved to be one | `cargo test -p dcs-mcp cli` shows `--out` writing headers+body verbatim, a `pending` writing no file at all, and the CLI's text for one reply byte-identical to the tool's as an empty diff inside the same test; mutations: a zero-byte file written for a `pending` reddens the capture check; the CLI rendering a reply through its own formatter rather than the tool's reddens the byte-identity check | T39,T55 | developer-only |
| T58 | The `runs.jsonl` provenance record (`mcp.md` §4.6): one line per eval, the resolved path and the SHA-256 the source reader already computed, and nothing written for a refusal that never read a byte | `cargo test -p dcs-mcp runs` shows one line per eval carrying path and SHA-256 and no line for a `dcs_eval_file` refused before it read; mutation: a line whose SHA-256 is recomputed from the reply rather than taken from the source record reddens the hash-provenance check | T40 | developer-only |
| T41 | The reply watch's handles (`mcp.md` §6): open only while a wait is in flight, never opened on a session `wait` has reported `superseded`, and no handle the next executor session's sibling sweep would trip over | `cargo test -p dcs-mcp watching` shows a stand-in's sibling-directory sweep succeeding while the server is idle, no watch held between two tool calls, and a `superseded` session waited on without a watch being opened; mutations: a watch left open across a tool call reddens the sweep check; a watch opened on a `superseded` session reddens the superseded check | T54,T56,T37 | developer-only |
| T59 | The server's own idle (`mcp.md` §6): no thread of the server wakes in 60 s of silence, no keepalive or periodic `ping`, and the executor never held armed between calls | `cargo test -p dcs-mcp idle` shows 60 s of silence with no wake recorded against the server's own clock and no request published in it; mutations: a keepalive `ping` on a timer reddens the never-held-armed check; a runtime builder reaching for `enable_all` in place of the timer reddens the shipped-runtime check; `enable_all` added beside the timer reddens that check's refusal of a blanket reach | T37 | developer-only |

**Stage command:** `cargo test -p dcs-mcp serve tools-listed wording cli runs watching idle`.

---

## Stage 8 — The installer and embedding   *(Milestone C)*

The program that puts the executor in place for a person who downloaded one file, under park-and-
restore, writing nothing to the install and destroying nothing under `Saved Games`.

T44 carried three interfaces when the stage was first written, and `mcp.md` §5.1 lists them as
three numbered steps with three different failure modes: a store outside both DCS trees (the
register's row-before-move and the park tree that makes "nothing is deleted, ever" true), a
placement policy over four hashes (absent, a hash this project shipped, a foreign hash, the prior
project's two files), and an editor for a text file that belongs to SRS and Tacview as much as to
this project. They are split as T60, T44 and T61 — the same split the stage's other half already
has, since T45 removes the line T61 appends and restores what T60 parked. T45 and T46 are not
split: each is one verb reading one tree, and they gain the mutations their cells were missing
rather than rows.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T42 | Embed `DcsEvalExecutor.lua` via `include_bytes!` with its SHA-256, and the list of every hash this project has shipped (`mcp.md` §5.5), which is what lets T44 call a file an upgrade and T45 call one its own; the binary names the executor build | `cargo test -p dcs-mcp embed` shows the embedded hash matching the repo's `DcsEvalExecutor.lua` and present in the shipped-hash list; mutations: a stale embedded copy reddens the hash match; the current hash missing from the list reddens the list check | T16 | developer-only |
| T43 | Find `Saved Games` via `FOLDERID_SavedGames`, variant selection with ambiguity asked-not-picked, refuse the wrong tree | `cargo test -p dcs-mcp locate` (fixture folders) shows two variants reported as an ambiguity and a target under the install refused; mutation: picking one of two variants reddens the ambiguity check | T30 | developer-only |
| T60 | The server's own data directory: `install-register.tsv` — `<utc> install <path> <sha256> pending` appended before anything moves and marked after — and the park store, `parked\<utc>\` holding a displaced file under its path relative to the variant. Nothing here is ever deleted, and nothing here is under either DCS tree | `cargo test -p dcs-mcp register` shows a move whose register row was written before it and marked after, a parked file recoverable from its own relative path, and two parks in one second landing in distinct directories; mutations: a delete in place of a park reddens the park check; a row written after the move rather than before reddens the ordering check | T30 | developer-only |
| T44 | `install` places the hook: `Scripts\Hooks\DcsEvalExecutor.lua` written as `.tmp` in that directory and renamed in, over four dispositions — absent, a hash this project shipped (an upgrade, the old bytes parked), and a foreign hash (refused and named unless `--replace`, which parks it). Every other file in that directory is invisible to the placement, the prior project's included (ADR 0022) | `cargo test -p dcs-mcp install` shows `install` twice leaving one hook file, a foreign file refused without `--replace` and parked with it, a shipped-hash file replaced and parked, and a directory holding two hooks that are not ours installed through with neither read nor moved; mutations: writing the final name directly rather than by rename reddens the half-written check; a foreign hash replaced without `--replace` reddens the refusal | T42,T43,T60 | developer-only |
| T61 | The `Export.lua` line: one `dofile(lfs.writedir() .. 'Scripts/Hooks/DcsEvalExecutor.lua') -- dcs-mcp` appended by its exact marker, the file created if absent, a newline added first where the file lacks a trailing one, a copy parked before the write, and no other byte changed — the marker is what makes T45's removal an exact-line match | `cargo test -p dcs-mcp export_line` shows `install` twice leaving one `dofile` line, a file with no trailing newline gaining one rather than a joined line, an absent file created, and every other byte of a fixture `Export.lua` identical afterward; mutations: a second `dofile` line reddens the once-only check; the marker dropped from the appended line reddens T45's exact-match removal | T43,T60 | developer-only |
| T45 | `uninstall`: remove exactly what `install` put there, hash-gated, restore parked files, and write the register row `uninstalled` | `cargo test -p dcs-mcp uninstall` shows the `Export.lua` line removed by exact match with its neighbours byte-identical, a hook whose hash is not in the shipped list left in place and named, a parked file restored to its original path, and the register carrying the `uninstalled` row; mutations: removing a neighbouring line reddens the exact-match check; removing a hook of unknown hash reddens the hash gate; a restore that copies without removing the park reddens the restore check | T44,T61,T60 | developer-only |
| T46 | `verify` (which `dcs_status` also runs): the hook's hash against the embedded release's, the `dofile` line present exactly once, no other `DcsEval*` hook beside it and no opinion at all about anyone else's (ADR 0022), `executor.txt` present with `protocol: 2` and its `app_version` reported as a difference and never a refusal, the heartbeat, the `autoexec.cfg` key report, and nothing written | `cargo test -p dcs-mcp verify` reports both policy-gate keys from a fixture, names a second copy of our own executor and a duplicated `dofile` line as problems while leaving another project's hook unnamed, reports a differing `app_version` without refusing, and leaves a `git status`-clean fixture tree; mutations: any write during `verify` reddens the clean-tree assertion; a differing `app_version` treated as a failure reddens the difference-not-refusal check | T42,T44,T61 | developer-only |

**Stage command:** `cargo test -p dcs-mcp embed locate register install export_line uninstall
verify`.  **Milestone C acceptance:** Stages 7–8 commands green; `dcs-mcp install --saved-games
<fixture> && dcs-mcp verify --saved-games <fixture>` prints `verified` against a published session
(the verbs are T63's, in Stage 9); the fixture tree is `git`-clean after verify.

---

## Stage 9 — Proven live in DCS   *(Milestone D)*

The wall-clock-bound tasks that need DCS running and a person to set the scene. Each fills a blank
the documents left on purpose or confirms a number the harness cannot produce. This is the critical
path; nothing here is parallelisable by adding developer effort. T62 and T63 are the exceptions,
developer-only both: T62 is the switch T50 turns, and T63 the installer verbs T52 starts from. They
are here rather than in Stages 7 and 8 so that Milestone C stays as it closed.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T63 | The installer's three verbs on the binary: `dcs-mcp install`, `verify` and `uninstall` over what T43–T46, T60 and T61 built — `Saved Games` from the known folder unless `--saved-games`, two variants or more refused with every name and `--variant` in the refusal, never picked and never prompted for (ADR 0024), `--replace` the one yes, and `install` printing what happens next, the `verify` line and the MCP registration snippet | `cargo test -p dcs-mcp installer` shows three variants refused with each named and nothing written, a named one installed with its siblings byte-identical, a foreign hook refused without `--replace`, `verify` exiting 1 on a problem and 0 on `verified`, and the real binary installing into a fixture, printing `verified` against a stand-in session and uninstalling back to the bytes it started from; mutations: an unnamed variant defaulting to `DCS` reddens the three-variant check; `--replace` assumed rather than read reddens the foreign-hook check; `verify` exiting 0 whatever it found reddens the exit check; the verbs left out of the binary's dispatch reddens the real-binary check | T43,T44,T45,T46,T60,T61 | developer-only |
| T52 | The cutover from `dcs-api-bridge`, by hand and unassisted: its hook deleted and its `Export.lua` line removed before this executor installs (ADR 0022) | on a real machine: `DcsApiEval.lua` gone from `Scripts\Hooks\` and its `dofile` line gone from `Export.lua`, then `dcs-mcp install` and `dcs-mcp verify` green, each given `--variant` where `Saved Games` holds more than one; no code checks any of it, and the refusal that once did is removed; mutation: pointing `verify`'s one stray prefix at `dcsapi` instead of `dcseval` must redden — a report that stops naming a second copy of our own executor and starts naming the project this one replaces has undone the narrowing in both directions at once | T63, Milestone C | DCS + human |
| T47 | The first live run: round-trip p50/p95 with the event-driven wait, `cpu_ms`/reply, replies/tick at W=8, seven-state generation wall time | `dcs-mcp live report` prints all four rows per state against the recorded baselines (30 ms p50 from the Node client and S4's 14 ms in-Lua median; 465 s/generation), the generation a floor projected from the W=8 `return 1` throughput because this project ships no walker; a run that prints none has not measured the change; mutation: a report that drops a row it holds no figure for, rather than printing it `unmeasured`, reddens the every-row check | Milestone C | DCS + human |
| T48 | The dormant cost in DCS three ways (hook absent / installed-dormant / armed-idle), measured in process on the executor's own paths (ADR 0025) | `dcs-mcp live report` prints the three per-frame figures and the incumbent's per-frame listing re-measured on the same machine, and a by-hand row for the whole-frame three-way this instrument does not take; the dormant figure is at or below the 0.098 ms baseline, which is the incumbent measured on one machine, DCS 2.9.28.26385, one session, so a figure that disagrees on other hardware is a new measurement rather than a regression; a higher figure here reopens `bridge.md` §2 and is reported as such; mutation: the dormant stat taken on the arm file a live request has just written, rather than on a path that does not exist, reddens the probe suite | Milestone C | DCS + human |
| T49 | `missionscripting` through `a_do_script` live with a mission loaded, and the s17 flag-agreement fixture | `dcs-mcp eval missionscripting …` returns through `a_do_script` with a mission loaded, and the ported s17 fixture shows a 16-bit flag crossing agreeing with a `DO SCRIPT` action; a disagreement is reported, not read as an empty walk; `dcs-mcp live rtt` in a mission prints the `a_do_script` reply as a row of `live report`, and until the s17 fixture is ported its row prints `unmeasured: not built` | Milestone C | DCS + human |
| T62 | The opt-in reads' switch: the four tier-2 reads and the three from the crashing batch (ADR 0023), off by default and asked for by group (`extra`, `suspect`) or one read at a time by key, through `game-state --reads` and `dcs_game_state`'s `reads`, and every read no axis is made of printed a line each | `cargo test --workspace opt_in` shows a default gather publishing none of the seven, each key alone publishing its read and no other opt-in one, an unasked read answering `unknown (tier 2 off)` or `unknown (suspect reads off)`, and the flag and the argument each reaching the gather; mutations: a suspect read sent by default reddens the default-gather check; a key that turns on its whole group reddens the one-read check; the flag dropped between the command line and the gather reddens the flag check; the argument dropped between the tool and the gather reddens the argument check | T35,T36,T38,T40 | developer-only |
| T50 | The seven opt-in reads — the four tier-2 and the three from the crashing batch (ADR 0023) — each sent alone with T62's switch under the supervisor, one per session; editor-vs-menu detection, `mission_name` at the menu, the callback vocabulary | `dcs-mcp live report` prints a row per opt-in read (sent alone, with its result or the crash that named it), records `getSimulatorMode` raw per state, and notes which offered callbacks were seen; an unmeasured axis stays `unknown`/`menu-or-editor` and says so; `dcs-mcp live read <key>` sends each alone and refuses a second in one DCS session, and the scene half (`sim_mode` per scene, `mission_name` at the menu, the callbacks) is a follow-up `live scene` whose rows print `unmeasured: not built` until then; mutations: a second read admitted in one session reddens the one-per-session check, a read written to the ledger only once it is answered reddens the ledger-first check, and the read published beside a ping reddens the alone check | T62, Milestone C | DCS + human |
| T51 | Permanent-installation acceptance: install, fly with the executor dormant, survive a DCS update, `verify` still green | on a real machine: `dcs-mcp install`, given `--variant` where `Saved Games` holds more than one; a play session with the executor dormant and no noticeable frame impact; a DCS update leaving `Saved Games` untouched; `dcs-mcp eval` in every reachable state; `dcs-mcp game-state` reporting the state; `dcs-mcp verify` green afterward — the project's done-condition | T52,T47,T48,T49,T50 | DCS + human |

**Stage command:** `dcs-mcp live report`, printing every row above — measured, or `unmeasured` with
its reason; T51 and T52 are by hand and print as such.  **Milestone D acceptance:** T51 passes and
the dormant per-frame cost (T48) is at or below baseline.
