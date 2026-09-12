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

This plan carries **52 tasks across 10 stages**. The count is driven by the four right-sizing
tests, and the splits fall at interfaces rather than at steps: each Lua carrier (`hook` local,
`net.dostring_in`, the `a_do_script` door) is one task because changing how one crosses a state
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
   by T07 (executor containment) and T43–T46 (the installer never writes the install and never probes
   writability by writing).
3. **`Saved Games` is writable under park-and-restore; nothing there that is not this project's is
   ever lost — a displaced file is registered and moved, never deleted or overwritten in place.**
   Enforced by T43–T44 and the register.
4. **The idle budget is a gate, not a goal.** A dormant frame is zero allocations, zero kernel
   entries; one `lfs.attributes` every `PROBE_EVERY` frames. Enforced by T27 as a byte-count control
   and by T48 as a frame-time measurement; a dormant figure above the baseline reopens `bridge.md`
   §2 (§2.3).
5. **Every task is marked `developer-only` or `DCS + human` and sequenced on it.** The DCS tasks are
   Stage 9, last, because they are wall-clock-bound and cannot be parallelised by adding effort.

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
  tier-1/tier-2 constant list (T35).

**Spikes** (open questions with exit conditions, folded into the live tasks rather than left as
rows): editor-vs-menu detection (T50; exit: one of three candidates measured, else `menu-or-editor`
stands); tier-2 read safety from a hook (T50; exit: each read measured alone under the supervisor
before it is enabled). Neither blocks Milestones A–C.

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
  door — each red under its stated mutation. *Acceptance:* `lua5.1 tools/harness.lua executor` and
  `cargo test -p dcs-eval` both print their full check counts, all green, and a scripted mutation
  sweep shows every §10/§7 control reddening. Covers Stages 3–6.
- **Milestone C — Installable and serving.** Someone can install the executor from one binary under
  park-and-restore, verify it, and an MCP agent can list and call six tools. *Acceptance:*
  `dcs-mcp install --saved-games <fixture>` then `dcs-mcp verify` prints `verified`; an in-memory
  MCP session lists six tools and calls each; `git status` on the fixture tree after `verify` is
  clean (verify wrote nothing). Covers Stages 7–8.
- **Milestone D — Proven live in DCS.** Someone can do the whole thing for real: install, evaluate
  in every reachable state including through the mission door, read game state, and leave the executor
  installed through play and a DCS update. *Acceptance:* the Stage 9 script prints every measured
  row and the permanent-install acceptance (T51) passes; the dormant frame-time is at or below the
  baseline. Covers Stage 9.

The critical path runs through Stage 9: those six tasks need DCS running with a person to set the
scene, are wall-clock-bound, and cannot be shortened by parallel effort — so every off-DCS control
that *can* be built and harness-proven earlier is, and Stage 9 inherits only what genuinely needs a
real frame.

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
| T17 | The end-to-end round-trip control: Rust client → harnessed `DcsEvalExecutor.lua` → client reads the `ping` reply | `cargo test -p dcs-eval e2e` (spawning `lua5.1` on the shipped file) returns a `pong`/`ok` reply the client parses; mutation: a stamp mismatch injected mid-run surfaces as `superseded`, not a hang | T14,T15,T16 | developer-only |

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
| T21 | The `missionscripting` two-hop door: `a_do_script`, `%q` args, the return-shift correction, string-only conversion, `door-shut` status | `lua5.1 tools/harness.lua executor/door` shows a value crossing as a string only, the slot-2 read, and `door-shut` with no mission; mutation: returning a table across the door (not converted in-state) reddens the string-only backstop | T20 | developer-only |
| T22 | The `a_do_script` off-by-one fixture reproduced under reference `lua5.1` | `lua5.1 tools/harness.lua executor/door-shift` exercises the shift and asserts the correction; mutation: removing the `+1` correction reddens it (a lone value dropped) | T21 | developer-only |

**Stage command:** `lua5.1 tools/harness.lua executor/eval-hook executor/result executor/dostring
executor/door executor/door-shift`.

---

## Stage 4 — Budgets and crash safety   *(Milestone B)*

The three protections generalised from `census` to every eval, the instruction guard that bounds a
single chunk, and the stamp fence and markers that close the 2026-09-02 kill.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T23 | The per-tick CPU budget between requests, and `cpu_ms`/`tick` on every reply | `lua5.1 tools/harness.lua executor/tick-budget` shows the executor stopping new requests past `TICK_BUDGET_MS` and W-deep replies answered in id order sharing a tick; mutation: a reply missing `cpu_ms` reddens the presence check | T19 | developer-only |
| T24 | The instruction count-hook budget inside the chunk, refused at load for `0`/non-integer, `budget: none` in `mission` | `lua5.1 tools/harness.lua executor/instr-budget` shows a looping chunk stopped `stage: budget` where `debug` exists and `budget: none` in `mission`; mutation: an `INSTRUCTION_BUDGET` of `0` silently defaulted (Lua installs no hook, `debug.gethook` still returns it) reddens the load-time refusal | T18 | developer-only |
| T25 | The stamp fence: `for` ≠ stamp → `stale-session`, reply stamp mismatch → discarded `foreign`, chunk never runs | `lua5.1 tools/harness.lua executor/fence` shows a foreign `for` answered `stale-session` without running, at load and on a tick; mutation: running the chunk anyway reddens the kill-reproduction control | T10 | developer-only |
| T26 | The events log `B|`/`O|` markers and rotation to `events.prev.log` at load | `lua5.1 tools/harness.lua executor/events` shows the last `B|` with no `O|` naming the killer and one generation kept; mutation: a synthetic unbalanced file whose killer is misread reddens the reader | T12 | developer-only |

**Stage command:** `lua5.1 tools/harness.lua executor/tick-budget executor/instr-budget executor/fence
executor/events`.

---

## Stage 5 — Dormancy and arming   *(Milestone B)*

The one restructuring the maintainer's idle requirement is about: a executor that costs almost nothing
while the game is played and wakes on a file.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T27 | The dormant path: zero allocations, zero kernel entries per frame, one `lfs.attributes` every `PROBE_EVERY` frames, as a byte-count control | `lua5.1 tools/harness.lua executor/dormant` shows 100,000 dormant ticks growing `collectgarbage('count')` by zero and 12,500 ticks calling the stubbed `lfs.attributes` exactly 12,500 times, `lfs.dir` never; mutation: reintroducing the per-call closure in the callback wrapper reddens the byte count | T04,T12 | developer-only |
| T28 | Arming and disarming: the arm-file wake, the ordered `QUIET_S` disarm, the race proof, and state-in-target surviving a disarm | `lua5.1 tools/harness.lua executor/arming` shows a request published while dormant answered within `PROBE_EVERY`+1 ticks, one published on the disarm tick still answered, an abandoned arm file costing one quiet period; mutation: reversing the `os.remove`/list order in disarm reddens the strand-a-request proof | T27 | developer-only |
| T29 | The heartbeat writer: `armed`/`since`/`phase`/`ticks`/`last_callback`/`callbacks`, event-driven while dormant and 2 s while armed | `lua5.1 tools/harness.lua executor/heartbeat` shows it written at every arm, disarm and phase change and never per dormant frame; mutation: a dormant executor that keeps writing every 2 s reddens the dormant-write-count check | T28 | developer-only |

**Stage command:** `lua5.1 tools/harness.lua executor/dormant executor/arming executor/heartbeat`.

---

## Stage 6 — The client library completed   *(Milestone B)*

The `dcs-eval` crate's remaining surface: containment, the death-vs-silence outcome table, status,
the pipeline, the file source, and the game-state reads and their derivation.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T30 | Client path containment and resolution (`canonicalize`, strip `\\?\`, fold case, segment boundary, 8.3 short names and junctions) | `cargo test -p dcs-eval paths` (on Windows) prints its count; mutations: a path admitted through an 8.3 short spelling, a junction, or a byte-prefix match reddens the boundary check | T02 | developer-only |
| T31 | Client `wait`/`collect` and the §4.4 outcome table reading the heartbeat `armed` field | `cargo test -p dcs-eval wait` shows `superseded` on a stamp change, `dead` on a gone PID, `pending`+`waking` for a dormant executor under 10 s, `stalled` past it; mutation: treating a dormant heartbeat's age as staleness reddens the `armed: no` rule | T13,T29 | developer-only |
| T32 | Client `status()` (files + PID probe, problems found, running `app_version` vs the model's) | `cargo test -p dcs-eval status` shows a foreign-stamp and a foreign-transport heartbeat each reported as a problem and zero round trips; mutation: a round trip issued by `status` reddens the "costs the executor nothing" check | T31 | developer-only |
| T33 | Client `pipeline(specs, W)` yielding replies in id order | `cargo test -p dcs-eval pipeline` shows W-deep sends and in-order replies; mutation: out-of-order yielding reddens the ordering check | T14 | developer-only |
| T34 | The file-source reader (`evalFile`): hash, BOM strip, shebang blank, CRLF passthrough, `chunkname: @<path>`, size ceiling, containment before a byte is read | `cargo test -p dcs-eval file-source` shows a BOM+`#`+CRLF file raising on its line 47 and the run record carrying `bom: stripped`, `shebang: blanked`, the SHA-256; mutations: reading a path outside every root, or a `Config\` path, before refusing reddens the guard; a file one byte over the ceiling sent rather than refused reddens the limit | T30 | developer-only |
| T35 | Game-state reads: the tier-1 chunks (one `pcall` each), the constant read-list, never-send-unlisted | `cargo test -p dcs-eval reads` shows each read as its own request under one `pcall`; mutation: sending a `DCS.*` name not in the constant list reddens the never-send check | T30 | developer-only |
| T36 | Game-state derivation: the axes, `unknown` as a value, no default arm | `cargo test -p dcs-eval game-state` shows `loading` with no round trip, `paused (read)` noting a disagreeing callback phase, `session: client` on a `refused` `gui` probe, `unknown: <error>` on an errored axis alone, `unknown (tier 2 off)` when off; mutation: any axis filled from another's evidence reddens a no-default-arm check | T35 | developer-only |

**Stage command:** `cargo test -p dcs-eval paths wait status pipeline file-source reads
game-state`.  **Milestone B acceptance:** the Stage 3–6 commands all green, plus a mutation sweep
showing each §10/§7 control reddening under the mutation in its cell.

---

## Stage 7 — The MCP server and CLI   *(Milestone C)*

The `dcs-mcp` binary: `rmcp` wiring, six tools, one wording for one reply, and the CLI that speaks
the same functions.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T37 | `rmcp` stdio wiring, the `serve` role, the client built per call | `cargo test -p dcs-mcp serve` shows only protocol frames on stdout and diagnostics on stderr; mutation: a diagnostic written to stdout reddens the transport-cleanliness check | T02 | developer-only |
| T38 | The six tools registered, listed and callable over a real MCP session on an in-memory pair | `cargo test -p dcs-mcp tools-listed` lists exactly `dcs_status`, `dcs_ping`, `dcs_game_state`, `dcs_eval`, `dcs_eval_file`, `dcs_collect` and calls each; mutation: a tool that registers but is not listed reddens the count | T37 | developer-only |
| T39 | The reply wording (one place): refusals read as refusals — `door-shut`, `stale-session`, `oversize`, `budget` — and `pending` names its id and phase | `cargo test -p dcs-mcp wording` shows each status worded as a non-empty refusal and `pending` not marked `isError`; mutation: an `oversize` reply worded like an empty result reddens it | T38 | developer-only |
| T40 | The CLI verbs with `--out`/`--capture` (reply verbatim, nothing for `pending`) and the `runs.jsonl` provenance line | `cargo test -p dcs-mcp cli` shows `--out` writing headers+body verbatim and a `pending` writing nothing, plus one `runs.jsonl` line per eval with path+SHA-256; the CLI's text for one reply is byte-identical to the tool's (diff empty); mutation: a zero-byte file written for a `pending` reddens the capture check | T39 | developer-only |
| T41 | The server-idle obligations (`mcp.md` §6): no thread wakes in 60 s of silence, the reply watch open only while waiting and never on a superseded session, the executor never held armed | `cargo test -p dcs-mcp idle` shows a sibling-directory sweep succeeding while the server is idle and no watch held between calls; mutation: a keepalive `ping` or a watch left open reddens the "never hold armed"/"hold no handle" checks | T31,T37 | developer-only |

**Stage command:** `cargo test -p dcs-mcp serve tools-listed wording cli idle`.

---

## Stage 8 — The installer and embedding   *(Milestone C)*

The program that puts the executor in place for a person who downloaded one file, under park-and-
restore, writing nothing to the install and destroying nothing under `Saved Games`.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T42 | Embed `DcsEvalExecutor.lua` via `include_bytes!` with its SHA-256; the binary names the executor build; `verify` prints both | `cargo test -p dcs-mcp embed` shows the embedded hash matching the repo's `DcsEvalExecutor.lua`; mutation: a stale embedded copy reddens the hash match | T16 | developer-only |
| T43 | Find `Saved Games` via `FOLDERID_SavedGames`, variant selection with ambiguity asked-not-picked, refuse the wrong tree | `cargo test -p dcs-mcp locate` (fixture folders) shows two variants reported as an ambiguity and a target under the install refused; mutation: picking one of two variants reddens the ambiguity check | T30 | developer-only |
| T44 | `install`: register-before-move, place the hook by rename, park an existing/foreign file, append the `Export.lua` line by its exact marker | `cargo test -p dcs-mcp install` shows `install` twice leaving one hook file and one `dofile` line, a foreign `DcsEvalExecutor.lua` refused without `--replace` and parked with it, and a register row written before each move; mutation: a delete instead of a park, or a second `dofile` line, reddens it | T42,T43 | developer-only |
| T45 | `uninstall`: remove exactly what `install` put there, hash-gated, restore parked files | `cargo test -p dcs-mcp uninstall` shows the `Export.lua` line removed by exact match and its neighbours untouched, an unknown-hash hook left and named, a parked file restored; mutation: removing a neighbouring line reddens the exact-match check | T44 | developer-only |
| T46 | `verify` (which `dcs_status` also runs): hashes, single `dofile` line, `autoexec.cfg` key report, writes nothing | `cargo test -p dcs-mcp verify` reports both policy-gate keys from a fixture and a `git status`-clean fixture tree afterward; mutation: any write during `verify` reddens the clean-tree assertion | T44 | developer-only |

**Stage command:** `cargo test -p dcs-mcp embed locate install uninstall verify`.  **Milestone C
acceptance:** Stages 7–8 commands green; `dcs-mcp install --saved-games <fixture> && dcs-mcp verify`
prints `verified`; the fixture tree is `git`-clean after verify.

---

## Stage 9 — Proven live in DCS   *(Milestone D)*

The wall-clock-bound tasks that need DCS running and a person to set the scene. Each fills a blank
the documents left on purpose or confirms a number the harness cannot produce. This is the critical
path; nothing here is parallelisable by adding developer effort.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T52 | The cutover from `dcs-api-bridge`: its hook and transport root gone before this executor installs, and a check proving the two never poll at once | `dcs-mcp verify` reports the incumbent's `DcsApiEval.lua` absent from `Scripts\Hooks\` and no `DcsApiBridge\` under `Logs\`, and names either as a problem while it is there; mutation: leaving the incumbent's hook in place must be reported, not tolerated — a `verify` that passes with both installed reddens the check | Milestone C | DCS + human |
| T47 | The first live run: round-trip p50/p95 with the event-driven wait, `cpu_ms`/reply, replies/tick at W=8, seven-state generation wall time | the run script prints all four rows per state against the recorded baselines (30 ms p50, 465 s/generation); a run that prints none has not measured the change | Milestone C | DCS + human |
| T48 | The dormant frame-time three-way comparison in DCS (hook absent / installed-dormant / armed-idle) | the script prints the three frame-time figures; the dormant figure is at or below the 0.098 ms baseline — a higher figure reopens `bridge.md` §2 and is reported as such | Milestone C | DCS + human |
| T49 | The `missionscripting` door live with a mission loaded, and the s17 flag-agreement fixture | `dcs-mcp eval missionscripting …` returns through the door with a mission loaded, and the ported s17 fixture shows a 16-bit flag crossing agreeing with a `DO SCRIPT` action; a disagreement is reported, not read as an empty walk | Milestone C | DCS + human |
| T50 | Tier-2 reads (each sent alone under the supervisor, one per session), editor-vs-menu detection, `mission_name` at the menu, the callback vocabulary | the script prints a row per tier-2 read (enabled or not, with its result), records `getSimulatorMode` raw per state, and notes which offered callbacks were seen; an unmeasured axis stays `unknown`/`menu-or-editor` and says so | Milestone C | DCS + human |
| T51 | Permanent-installation acceptance: install, fly with the executor dormant, survive a DCS update, `verify` still green | on a real machine: `dcs-mcp install`; a play session with the executor dormant and no noticeable frame impact; a DCS update leaving `Saved Games` untouched; `dcs-mcp eval` in every reachable state; `dcs-mcp game-state` reporting the state; `dcs-mcp verify` green afterward — the project's done-condition | T52,T47,T48,T49,T50 | DCS + human |

**Stage command:** the live-run script printing every row above.  **Milestone D acceptance:** T51
passes and the dormant frame-time (T48) is at or below baseline.
