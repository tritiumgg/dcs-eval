# The MCP server, specified as its own project

**Citation convention.** A path written `prior:<path>` is a file in the superseded `dcs-api`
repository this document was written from, cited as evidence and readable there on demand. Nothing in the new project reads one. An unprefixed path is a file this document expects the new project to create.
`bridge.md §N` is the eval bridge specification this one sits on, in its revised form (its
§7.0); `D<n>` and `S<n>` are decision and spike records in `prior:docs/decisions/`.

**What this was written from.** `prior:pipeline/src/mcp/` (server.ts 185 lines, tools.ts 237,
server.test.ts 292), `prior:pipeline/src/bridge/`, the phase machinery of
`prior:tools/hooks/DcsApiEval.lua:1960-2183`, the committed model's records for the `hook` state
(`prior:model/hook/DCS.yaml`, `prior:model/hook/net.yaml`) and the `gui` state's callback globals,
D36–D38, D94, D128, S4, S7 and S17. The network was reachable in this session: every SDK
fact in §1 was fetched by a command in §9 on 2026-09-06. `prior:docs/memory/**`, every `README.md`
and every `CLAUDE.md` were unreadable under the session's deny rules, as `bridge.md` records.

**Where a number is asserted**, the command that produced it is in §9.

---

## 0. What the server is, and what it is not

**One sentence.** A program a coding agent talks to over MCP that evaluates Lua in any of DCS's
seven states through the eval bridge, tells the agent what the game is doing, and can be installed
by a person who wants to fly rather than build software.

**Scope, deliberately narrow.** Evaluating Lua, and knowing the game's state. Not census, not
collection, not modelling: a project that wants those ships its own Lua and evaluates it through
this server, which is what `bridge.md` §7.0 made of `census` and `reflect`. It is not the
bridge either — the bridge is the Lua file DCS loads, specified in `bridge.md`; this server is
its client, and the installer that puts it in place.

**A standalone project, `dcs-mcp`, holding both halves.** Not part of the DCS API project. One
repository builds the in-game Lua and the outside binary together: the bridge client is a library
crate, so several unrelated projects use one client without depending on any one of them, and the
binary embeds the repository's own built `DcsApi.lua` with its hash, so installing the server
installs the bridge and the two can never be a version apart. The dependency runs one way:
consuming projects depend on this
project's crate or on the wire (§1.5).

Six decisions, each argued in its own section:

| § | Question | Decision |
|---|---|---|
| 1 | Language | **Rust.** One static binary, no runtime, the bridge script embedded. Decided by the installer requirement and confirmed against the Rust SDK's state on 2026-09-06; the port is priced as a number and a defect net (§1.4), and the condition under which Rust loses is stated (§1.6) |
| 2 | Shape | One binary, `dcs-mcp.exe`, three roles: `serve` (MCP over stdio), `install`/`verify`/`uninstall`, and a CLI for a person. Six tools. The client is a library crate, `dcs-eval`, that the binary and any Rust consumer link; the wire is the contract for everyone else |
| 3 | Game state | Two layers: facts read from the bridge's files, from `ping`, and from one-read chunks evaluated in `hook`; a derived state with its basis stated, and `unknown` whenever the facts do not decide it. Pause is read from `DCS.getPause()`, never inferred from a callback |
| 4 | File as source | The server reads the file outside DCS, hashes it, and ships it with `chunkname: @<path>`; it reads only under roots the operator allowed, refuses an oversize file naming the limit, and records path and SHA-256 for every run |
| 5 | Installer | `install` places the embedded `DcsApi.lua` and one `dofile` line, registers before it moves, never writes the install, and never destroys anything under `Saved Games`; `verify` is what `dcs_status` runs |
| 6 | Idle cost | Inherited from `bridge.md` §3.6–3.8, not re-decided: on every publish the client ensures the arm file; it never holds the bridge armed, never polls it, and never keeps a handle on a session directory it is not waiting in |

---

## 1. Language

### 1.1 What the work actually is

Before the argument, the inventory, because a language is chosen for the work and not for its
reputation. The server does these things and nothing else:

- **Byte-exact framing.** A reply is header lines, a blank line, and a body that is bytes in any
  encoding — ED's strings mix UTF-8 and cp1251, so a reply is read as one codepoint per byte and
  never decoded on the way in (`prior:pipeline/src/bridge/fs.ts:18-24`, D38). A request is written
  as bytes and published by rename (`prior:pipeline/src/bridge/fs.ts:67-101`). Ids sort.
- **Real path resolution.** A handshake path and an operator's path are resolved to real paths —
  8.3 short names expanded, junctions followed — before any containment test, because a textual
  test let both through once (`prior:pipeline/src/bridge/paths.ts:154-211`, D20, D38). The
  `Saved Games` root comes from the shell's known folder, not from a string.
- **Waiting.** An event-driven wait on a reply directory — `ReadDirectoryChangesW` — with a poll
  fallback (`bridge.md` §3.2), at an allocation cost small enough to sit beside a game for
  hours and at a handle discipline that never blocks the bridge's sweep (§6).
- **A long-lived process.** Launched by an MCP client at session start and left running; it must
  not leak, spin, or hold a handle it is not using.
- **Small utilities.** Hashing a file, reading `autoexec.cfg`, editing `Export.lua` by one exact
  line, probing a PID.
- **MCP.** Stdio transport, JSON-RPC framing, tool schemas — the one part best bought rather than
  built, which D38 decided once for Node (`@modelcontextprotocol/sdk`, not a hand-rolled layer)
  and which this document decides the same way for whichever language wins.

None of it needs a garbage collector, an event loop or a package ecosystem. All of it needs a
filesystem API that is honest about Windows.

### 1.2 The Rust SDK, checked rather than asserted

The premise to test was that a Rust MCP SDK might lag the protocol or lack a transport. Fetched on
2026-09-06 by the commands in §9:

| Fact | Value | Source |
|---|---|---|
| Crate, status | `rmcp`, "the official Rust SDK for the Model Context Protocol", `github.com/modelcontextprotocol/rust-sdk` | crates.io, the repository README |
| Latest release | **3.2.0, 2026-08-31**; five releases in August 2026 (3.1.1 through 3.2.0) | crates.io API |
| Downloads | 24,601,989 | crates.io API |
| Protocol revision | implements **2026-07-28**, which the protocol's own versioning page names as *current*; remains compatible with 2025-11-25 and earlier | repository README, `modelcontextprotocol.io/specification/versioning` |
| Transports | stdio server (`transport-io`), child-process client, streamable HTTP client and server; the legacy HTTP+SSE of 2024-11-05 deliberately absent | docs.rs feature list |
| Runtime | tokio `^1`; `#[tool]` and `#[prompt]` macros in the default `macros` feature | docs.rs |
| Caveat | a 3.x migration guide exists for breaking changes, so the version is pinned in `Cargo.lock` and bumped deliberately | repository README |

So the premise is false as of the date: the official crate tracks the current revision, ships the
one transport this server needs, and has the download count of a thing people run. For comparison,
`@modelcontextprotocol/sdk` — what `prior:pipeline/package.json:22` pins at `^1.30.0` — is at
1.30.0, requires Node ≥18, and carries 17 runtime dependencies including `express`, `hono`, `jose`
and `cors`, for a server that will only ever speak stdio.

### 1.3 What Node has, and what it costs

**Has.** 2,589 lines of working source and 1,409 lines of controls (§9); a control that drives a
real MCP session over an in-memory pair, so the tool surface is proven listed and callable rather
than assumed (`prior:pipeline/src/mcp/server.test.ts:44-77`); and the maintainer's fluency in it.
The stand-in bridge (`prior:pipeline/src/bridge/standin.ts`, 468 lines) is a test double with a
deliberately unshared encoder, which is the property that makes the client tests mean something.

**Costs.** A runtime the user installs (Node ≥22, `prior:pipeline/package.json:7-9`) and keeps
current; a `node_modules` tree beside the script; `npx` start-up on every MCP session; the bridge's
Lua shipped as a file beside the script and found at runtime rather than embedded. The two names
`bridge.md` §8 wrote as though settled — `realpathSync.native` and `os.tmpdir()` — are Node's
spellings of Win32 calls that any language reaches; they are not an argument for Node, and that
document now says so.

### 1.4 The port, priced

A Rust server is a translation of the TypeScript, not a copy, and a translation has a defect
budget. The surface: 2,589 lines of source (client 580, protocol 684, paths 334, fs 101, stand-in
468, server 185, tools 237) and 1,409 lines of controls (client 448, interop 286, paths 222,
protocol 161, server 292). `bridge.md` cites 14 TypeScript files 41 times, 34 of them with line
ranges (§9); each citation is a case to re-derive in the new language, not a line to carry over.

The defect net, in the order it is built:

1. **The interop control first.** The shipped bridge script's own bytes, produced under
   `tools/harness.lua --fs`, through the new parser (`prior:pipeline/src/bridge/interop.test.ts`,
   D38). It is the only place the two implementations of the protocol meet, it is
   language-neutral, and it is required in CI with the interpreter installed. A Rust client that
   does not pass it has not been ported.
2. **The stand-in second**, ported with its unshared encoder, so that every client control has a
   bridge to drive that is not the client's own serialiser turned around.
3. **Every row of `bridge.md` §10 that names a client behaviour**, and every row of §7 below.

That is the honest cost: roughly 2,600 lines rewritten under 1,400 lines of tests that must be
rewritten first. It is a few days of a maintainer who knows both languages; it is not free, and
nothing below pretends it is.

### 1.5 The middle option: one language for the server, any for a heavy driver

Ruled: **accepted, and the mechanism is the wire, not a binding.** `bridge.md` §7 is a complete
specification of a directory protocol. A consuming project's throughput driver — a census walker
publishing W-deep — may be written in whatever that project uses, and the existing TypeScript
client is one such driver, on two conditions: it passes the same interop control against the same
shipped bytes, and it honours §6 (ensure the arm file on every publish, hold no handle on a
session it is not waiting in). A driver that wants the Rust client links the `dcs-eval` crate.

What is *not* offered: a subprocess "driver mode" of the binary, or an FFI. A process per request
wastes a frame per request; a JSON-lines mode over a pipe is MCP reinvented without its clients.
The MCP server itself is usable as a driver by any language with an MCP client library — tool
calls may be issued concurrently, and the bridge answers them in id order within the tick budget —
but an interactive tool surface is the wrong shape for 13,605 requests, and the wire is right
there.

### 1.6 The decision, and what would reverse it

**Rust.** Decided by four things, in the order they weigh:

1. **The installer.** `dcs-mcp.exe` with `DcsApi.lua` inside it (`include_bytes!`), one download,
   no runtime, no package manager, on a Windows machine whose owner wants to fly. §5 is written
   for that person and could not be written for a Node script without a paragraph about installing
   Node.
2. **A long-lived process beside a game.** No collector, no event loop, allocation under the
   author's control, and a blocking stdio reader that costs nothing while no tool call is in
   flight (§6).
3. **The premise checked true.** §1.2. Had `rmcp` lacked stdio or lagged the revision, this section
   would say so and Node would win on the SDK alone.
4. **The toolchain is already here.** `cargo 1.98.1` is on this machine (§9). A cross-compiled
   single-file release is a solved shape in Rust, so the "install a toolchain" cost the
   requirement worries about falls on the maintainer once, not on a user ever.

**Rust loses if** any of these holds, and the maintainer is told plainly rather than by omission:

- The interop control of §1.4 cannot be made to pass. Then the port has not happened, and the
  Node server is the server.
- The server is needed before the port is done. The Node server exists and works; nothing here
  forbids it serving until the Rust one passes the interop control, and the wire does not change
  between them.
- The maintainer's estimate of §1.4 is wrong by a large factor on first contact. The defect net is
  what makes that visible early; if the stand-in port alone takes a week, stop and say so.

Nothing found argues for Node except that it exists, and that argument is answered by letting it
keep existing until it is replaced.

## 2. Shape

### 2.1 One binary, three roles

```
dcs-mcp serve   [--host hook|export] [--saved-games <dir>] [--variant <name>]
                [--install <dir>] [--allow <dir>]... [--reads base|extra] [--data-dir <dir>]
dcs-mcp install | verify | uninstall   [--saved-games <dir>] [--variant <name>] [--replace] [--yes]
dcs-mcp status | ping | game-state | eval <state> (<code> | --file <path>) [--out <path>]
```

`serve` is MCP over stdio. **stdout is the transport**: nothing but protocol frames goes there, and
every diagnostic goes to stderr (`prior:pipeline/src/mcp/server.ts:11-12`). The client is built per
call rather than once at start-up, because the bridge's output directory appears the first time
DCS loads it and a server that resolved everything up front would need restarting after an install
(`:14-16`). The CLI verbs are the same functions with `--out` writing the reply verbatim, headers
and body, never a `pending` (D38, `prior:pipeline/src/mcp/tools.ts:181-202`).

### 2.2 The crates

| Crate | Kind | Holds |
|---|---|---|
| `dcs-eval` | library | the client: handshake and heartbeat readers, containment, publish by rename, the arm file, wait and collect, `status`, `pipeline`, `save`, the file-source reader of §4, the game-state reads of §3 |
| `dcs-mcp` | binary | `rmcp` wiring, the tool wording, the installer, the CLI |

Wording lives apart from wiring, as `prior:pipeline/src/mcp/tools.ts` does from `server.ts`, for
the reason its header gives: a control drives what a reply *says* without a protocol session in the
way, and the CLI and the tool surface cannot word one reply differently.

### 2.3 The six tools

| Tool | Round trips | Wakes the bridge | Arguments | Says |
|---|---|---|---|---|
| `dcs_status` | none | no | `host` | what is readable without asking the bridge anything: installed, session stamp and PID alive, phase, `armed` and `since`, heartbeat age (qualified as meaningless while `armed: no`, `bridge.md` §4.7), transport, `app_version` against the bridge's build, every problem found, and `verify`'s findings (§5.3) |
| `dcs_ping` | one | yes | `host`, `wait_seconds` | liveness proved by a reply; `phase`, `tick`, `last_callback`, `callbacks` |
| `dcs_game_state` | one window | once | `host`, `wait_seconds` | §3.4's derived state, every fact it rests on, and the basis of each value |
| `dcs_eval` | one | yes | `host`, `state`, `code`, `chunkname?`, `max_instructions?`, `wait_seconds?` | the reply as §2.4 renders it |
| `dcs_eval_file` | one | yes | `host`, `state`, `path`, `max_instructions?`, `wait_seconds?` | the same, plus the provenance line of §4.6 |
| `dcs_collect` | none | no | `host`, `id` | a `pending` reply picked up later by id; one pass, no waiting, and the request is not resent (`prior:pipeline/src/mcp/tools.ts:227-237`) |

`host` replaces the prior `endpoint` to match `bridge.md` §6; it is `hook` by default and
`export` for the collector resident in `Export.lua`, which serves the `export` state alone and is
the only host that answers on a client joined to a real server (S7, D94). The default wait is 15 s
and waiting longer is never a failure (`prior:pipeline/src/mcp/tools.ts:17-18`); the wording of
`pending` during a load is updated with the figures now on record — 4.7 to 34.7 s across five
instrumented loads and 20 to 60 s in the same machine's `dcs.log`
— so an agent reads "15 s" as a wait and never as a
limit.

### 2.4 What a reply says

Carried from `prior:pipeline/src/mcp/tools.ts` because each line encodes a failure somebody paid
for: a refusal never reads like an empty result (`:72-90`); `pending` is not a failure and names
the id to collect and the phase (`:104-118`); a reply from `missionscripting` states in one
sentence that only a string crosses the door (`bridge.md` §5.4). Added for protocol 2: the
statuses `door-shut` and `stale-session`; the stages `oversize` (with `result_bytes` and the
sentence "the result was refused whole, not cut — lower what the chunk returns") and `budget`
("the chunk exceeded its instruction budget; `budget: none` in `mission` means it was never
bounded"); the `chunkname` line on every `eval`, so an agent reading `foo.lua:47` knows which file
that is; the `budget` line; and the `truncated` wording, which is deleted because nothing
truncates any more (`bridge.md` §7.7).

---

## 3. Game state

### 3.1 What "what the game is doing" resolves to

The requirement lists the main menu, the mission editor, a mission, mission paused, simulation
paused, loading, multiplayer, and anything else distinguishable. Against what can be measured
those resolve to five axes and one record, and the two pauses in the list resolve to one measured
fact plus one hint:

| Axis | Values | Decided by |
|---|---|---|
| `process` | `running`, `gone`, `never-ran` | the handshake and a PID probe |
| `bridge` | `dormant`, `armed`, `waking`, `stalled`, `superseded` | `bridge.md` §4.4, unchanged |
| `activity` | `loading`, `mission`, `menu-or-editor`, `unknown` | the phase, then one read |
| `pause` | `paused`, `running`, `n/a` | `DCS.getPause()`, read |
| `session` | `client`, `single-or-host`, `single`, `host`, `unknown` | reachability, then tier-2 reads where enabled |
| `ui` | the last non-frame callback and the set seen | the bridge's record, evidence only |

"Mission paused" and "simulation paused" are one fact — the simulation clock is stopped — and the
game menu that stops it in single player is a hint (`onShowGameMenu` in `ui`), not a state, because
a menu's disappearance fires no callback. The export host answers `phase` alone
(`loaded | sim | stopped`, `bridge.md` §6.2): `DCS` is nil there (D94), so no read is possible.

### 3.2 Sources, cheapest first

| Source | Costs the bridge | Facts | Freshness |
|---|---|---|---|
| `bridge.txt`, `heartbeat.txt`, a PID probe | nothing | stamp, PID, `app_version`, `protocol`; `phase`, `armed`, `since`, `ticks`, `last_tick_at`, `last_callback` | `phase` is written on every change even while dormant (`bridge.md` §4.7); `last_callback` is written at the next heartbeat write, so it may lag while dormant |
| `ping` | one wake | fresh `phase`, `tick`, `last_callback`, `callbacks` | this tick |
| one-read chunks in `hook`, §3.3 | the same wake, shared | `pause`, `mission_name`, `mission_file`, `model_time`, `sim_mode` | this tick; pipelined W-deep so they share it (`bridge.md` §3.3) |
| a reachability probe: `eval` in `gui` of `return 'ok'` | shared | `ok`, `refused`, `invalid-state` | this tick |

`dcs_status` uses the first row only and may be called as often as an agent likes.
`dcs_game_state` uses all four in one window — one wake, one quiet period — and skips the last
three when the heartbeat says `load`, because nothing answers during a load (S4, `bridge.md`
§2.1) and a request published into one only waits.

### 3.3 The reads, and the crash that sizes them

The bridge calls no `DCS.*` reader on anyone's behalf (`bridge.md` §7.4): it reads and never
calls (D37), and the two calls it does make — `DCS.getRealTime` for its clock and
`DCS.setUserCallbacks` once — are the whole of its surface. So the server does the reading, as
chunks it evaluates, and the set is chosen on precedent rather than on plausibility.

**Tier 1, on by default:** `DCS.getPause()`, `DCS.getMissionName()`, `DCS.getMissionFilename()`,
`DCS.getModelTime()`, `DCS.getSimulatorMode()`. These are exactly the five that ED's own
`Scripts/Hooks/webGUI.lua` calls from the hook state within twenty lines of one another — the
model records the call sites at that file's lines 257, 262, 273, 274 and 276
(`prior:model/hook/DCS.yaml`, the `cited_uses` of each; the command in §9 lists every `DCS.*`
name that file calls). ED's own hook makes these calls on a hook in every phase; that is the
strongest evidence this tree offers that a hook may.

**One read per chunk, each under its own `pcall`.** This server authors these chunks, so their
safety is its own concern and not the bridge's — the bridge evaluates what it is sent. ED's own
`webGUI.lua` batches its five and is not known to crash, so batching is not known to be unsafe.
One chunk per read is chosen because it is **the shape under which a crash names its killer**
(`bridge.md` §4.5: the last `B|` with no `O|`); a batched chunk that kills DCS says only that
one of five reads did it. It costs nothing — the five are pipelined and share a tick. A crash this
server causes is reported to the caller with the chunk that caused it; **this server keeps no
catalogue of dangerous calls**, which is a modelling concern for whoever is measuring DCS. The
chunk:

```lua
local ok, v = pcall(DCS.getPause)
if not ok then return 'error\t' .. tostring(v) end
local t = type(v)
if t == 'table' or t == 'function' or t == 'userdata' or t == 'thread' then return t .. '\t' end
return t .. '\t' .. tostring(v)
```

`tostring` is applied to a scalar only; a table is reported by type and never stringified, which
is the bridge's own rule (D37) kept one level up.

**Tier 2, off by default, `--reads extra`:** `DCS.isMultiplayer()`, `DCS.isServer()`,
`DCS.isTrackPlaying()`, `net.get_my_player_id()`. Each is present in `hook` by the census
(`prior:model/hook/DCS.yaml`, `prior:model/hook/net.yaml`) and called by ED only from the `gui`
state — `MissionEditor/GameGUI.lua` and the multiplayer dialogs — so there is no hook-state
precedent for any of them. They are enabled only after the first live run has sent each alone
under the probe supervisor (`bridge.md` §4.6), one per session, and the result is a row in this
table.

**Never:** `DCS.getMissionLoaded()`, a named suspect in a hook-state crash
, and `getPlayerUnitType` and `getMissionTheatre`, which
were in the crashing batch. The list of reads is a constant in `dcs-eval`, and a chunk that is not
in it is not a game-state read; an agent that wants one evaluates it as its own `dcs_eval`, under
its own name.

### 3.4 The vocabulary, and how each value is decided

The derivation is a table in code with one row per outcome and no default arm. This is that table.

| Axis, value | Decided when | Not decided (→ `unknown` with the reason) |
|---|---|---|
| `process: never-ran` | no `bridge.txt` under any `Saved Games\DCS*\Logs\DcsApi\<host>\` | two variants hold one: an ambiguity, reported with both, never a pick (`prior:pipeline/src/bridge/paths.ts:307-334`) |
| `process: gone` | `bridge.txt` present, its PID not running | — |
| `process: running` | the PID is running | — |
| `bridge: *` | `bridge.md` §4.4's table, verbatim | — |
| `activity: loading` | heartbeat `phase: load` — between `onMissionLoadBegin` and the next `onSimulationStart` (`prior:tools/hooks/DcsApiEval.lua:2152-2159`). No round trip is attempted | — |
| `activity: mission` | `phase` is `sim` or `paused`, and `mission_name` read non-empty | `phase` says mission and `mission_name` is empty or errored: `unknown`, both shown |
| `activity: menu-or-editor` | `phase` is `menu` — `onSimulationStop` fired, or no simulation has started since load | the editor is not distinguished from the menu on the measured build (§3.6); the value says so in its name rather than picking one |
| `pause: paused` / `running` | `DCS.getPause()` read `true` / `false` while `activity: mission`. The callback-derived `phase` is shown beside it as `phase_callback`, and a disagreement is a `note`, never resolved in the callback's favour: DCS can begin a mission already paused without firing either callback and can resume with no preceding pause (unverified lead), and this bridge's phase reports `sim` for a mission that began paused (`prior:tools/probes/crashermatrix_targets.lua:31-37`) | the read errored: `unknown: <error>`; outside a mission: `n/a` |
| `session: client` | `activity: mission` and the `gui` probe answered `refused`: on a client joined to a server, `net.dostring_in` returns nil for every state while `hook` still answers (S7) | — |
| `session: single-or-host` | `activity: mission`, the `gui` probe answered `ok`, tier 2 off. S7 measured that hosting from the client is indistinguishable from single player by reachability | — |
| `session: single` / `host` | tier 2 on: `isMultiplayer` false → `single`; true and `isServer` true → `host` | tier 2 on and either read errored |
| `track: replay` / `live` | tier 2 on: `isTrackPlaying` | tier 2 off, or errored |
| `ui` | `last_callback` and `callbacks` from `ping`; each `<name>@<tick>` | evidence, never a state |

The headline the agent reads first is composed from the axes and names its basis in the same
line, for example:

```
in a mission, paused (read), single player or host (tier 2 off), DCS 2.9.29.27278
loading — nothing answers until onMissionLoadEnd; 20–60 s is normal; collect the id later
at the main menu or in the mission editor (not distinguished on this build), dormant since 14:02:11
DCS is not running (bridge session 1757160000-31244 ended); install verified
```

### 3.5 Unknown is a value, not a guess

Three rules, each a control in §7:

- A read that errored sets its axis to `unknown: <the error, verbatim>` and nothing else; no axis
  is filled from another axis's evidence.
- Facts that contradict — `phase` says `sim` and `mission_name` is empty; the `gui` probe is
  `refused` at the menu, where S4 measured it answering — yield `unknown` with every fact printed,
  and the headline says "the facts disagree", because the disagreement is itself the finding: a
  build moved, or a read means something else in this phase.
- A tier-2 axis with tier 2 off reads `unknown (tier 2 off)`, so an agent knows the answer is
  purchasable and how.

Whether `DCS.getMissionName()` is empty at the menu, or still names the last mission flown, is
unmeasured (§3.6); until it is, `mission_name` is evidence for `activity: mission` only when the
phase already says so, and never evidence against `menu-or-editor`.

### 3.6 What the first live run must fill in

Each of these is a row this document leaves blank on purpose, with the measurement that fills it:

- **The editor.** Three candidates, none measured. (a) A callback: the bridge offers
  `onShowMissionEditor`, `onShowMainInterface` and `onShowMultiplayer`
  (`prior:tools/hooks/DcsApiEval.lua:2173-2175`), none seen to fire in S4's session — and
  `onShowMissionEditor` and `onShowMultiplayer` are **absent from the 63 `on*` globals ED's `gui`
  state defines**, while `onShowMainInterface`, `onShowPool` and `onShowIntermission` are present
  (§9). "Did not fire" is never "does not exist" (D36), but a name ED's own dispatcher does not
  define is a lead that nothing calls it. (b) `DCS.getSimulatorMode()`'s raw value per state,
  which the server records verbatim as `sim_mode` and maps to nothing until a table of observed
  values exists. (c) A read in the `gui` state — `me_editorManager` and its neighbours are `gui`
  globals by the census — which is a chunk under D37's rules and safe to try. Until one of them is
  measured, `menu-or-editor` stands.
- **The callback vocabulary.** The bridge offers 18 names; ED's `gui` state defines 63 `on*`
  globals (§9), of which these are worth offering next because each is a state edge and costs one
  table entry: `onMissionLoadFail`, `onNetDisconnect`, `onSimulationEsc`, `onPlayerStart`,
  `onPlayerStop`, `onShowPool`, `onShowIntermission`, `onQuit`, `onUserRequestMissionRestart`.
  Each is offered-unmeasured until `callbacks` reports it seen.
- **`mission_name` at the menu**, and `getMissionFilename` from the editor. A mission flown from
  the editor is reported by an unverified lead as carrying the *name* `tempMission` — a value that says "flown from the editor", not "in the
  editor", and is recorded as such.
- **Tier 2**, one read per session under the supervisor, and tier 1 on a joined client
  (`session: client`), where nothing has yet measured that the five reads answer.

---

## 4. A file as the source of an evaluation

### 4.1 Why the server reads it, and not DCS

An agent that has just written a 400-line Lua file should not read it back into its context to run
it. The chunk could reach DCS two ways: the server reads the file and ships the bytes, or the agent
sends a one-line chunk `dofile('C:/project/x.lua')` and DCS reads the file itself. The second is
free, gets `@path` line numbers for nothing, and is the right answer in the five states that have
`io` (`bridge.md` §1) for a file that is already on disk where DCS can see it. The server reads
the file anyway, for three reasons that the one-liner cannot meet: `mission` and `missionscripting`
have no `io` and no `dofile` route; a run of a file must record the hash of what ran, and only a
process that read the bytes can hash them at the moment they were sent; and a file an agent points
at must be refused or admitted by a rule the agent cannot rewrite from inside a chunk. The
one-liner stays available and `dcs_eval_file`'s refusal message names it where it applies.

The bridge is inside a game process and the install is read-only, so the read happens in the
server, outside DCS, and the bridge sees an `eval` with a body and a `chunkname` like any other.

### 4.2 Which paths may be read, as a decision

- **Allowed roots are configured**, `--allow <dir>` repeatable on `serve`, resolved to real paths
  at start-up. When none is given, the one root is the server's working directory at launch,
  which is where an MCP client launches it — the project the agent is working in. A path is
  admitted only if its *resolved* real path — 8.3 short names expanded, junctions followed, `..`
  collapsed, case folded — lies under a root at a segment boundary
  (`prior:pipeline/src/bridge/paths.ts:117-203`, D20, D38). The textual path is never tested.
- **Always refused, inside a root or not:** anything under `<Saved Games>\DCS*\Config\`, because
  `network.vault` there holds the user's account credentials and no chunk this server serves has
  business reading it; and anything under the DCS install when the install is known (`--install`
  or `DCS_INSTALL`), because nothing this server serves needs ED's file as a chunk — an agent that
  wants one runs `dofile` in a one-line chunk and DCS reads its own file.
- **A refusal names the rule and never the content**: not the first line, not the byte count
  beyond the limit sentence, not whether the path exists outside the roots.

### 4.3 The interaction the requirement names: reading any path, and errors verbatim

A Lua compile error echoes source: `x.lua:3: unexpected symbol near 'secret'` carries the token,
and for an unterminated string the token is the rest of the file. A run error echoes whatever the
chunk formats. The wire returns both verbatim (`bridge.md` §7.3), so "read any path" plus
"return errors verbatim" is a way to read the first token of any file on the machine through a
compile error. The composition rule: **errors are verbatim because the read was already
permitted**, and a read that is not permitted returns nothing of the file. §4.2 is therefore the
whole of the protection, and it is enforced before a byte is read, not after a compile fails.

### 4.4 The size ceiling, and a failure that names it

The bridge refuses a request over `max_request_bytes` — 262,144 bytes, read from the handshake and
never assumed — and never parses it as code (`bridge.md` §7.3). The server stats the file before
reading it and refuses one whose size plus the header block exceeds the limit, with a message that
names the limit, the file's size, and the two ways out: split the file, or `dofile` it from a
one-line chunk in a state that has `io`. Nothing is truncated, ever: a file cut at a byte boundary
is a different program that might compile.

### 4.5 Bytes, lines and the name

- The file is read as bytes and shipped as bytes: Lua source may be UTF-8 or cp1251 and the body is
  opaque to the protocol (`bridge.md` §7.1).
- A leading UTF-8 byte-order mark is stripped and `bom: stripped` is recorded, because Lua 5.1.5's
  `loadstring` does not skip one and a chunk beginning with it fails to compile on line 1. No line
  number moves: a BOM holds no newline.
- A first line beginning `#` is replaced by an empty line and `shebang: blanked` is recorded. That
  is what `luaL_loadfile` does in 5.1.5 — it discards the line and supplies a `\n` in its place so
  the count is kept — and `loadstring` does not.
- CRLF passes through: Lua's lexer counts `\r\n` as one line, so a Windows file's numbers are true
  without conversion.
- `chunkname` is `@` followed by the resolved real path, so an error reads `<path>:<line>:`. Lua
  abbreviates a name over 60 bytes to `...` and its tail in messages (`bridge.md` §7.3); the
  reply's `chunkname` header carries the whole path, and the tool prints both.
- The server prepends nothing and appends nothing. The wrapper that compiles the body inside a
  state is the bridge's (`bridge.md` §7.3), and it compiles the body as its own chunk, so line
  47 of the file is line 47 of every error from every state, including through the
  `missionscripting` door. §7 holds the control.

### 4.6 Provenance: path plus content hash

A record of what ran that cannot be reproduced is worth little, so every evaluation — file or
inline — appends one line to `<data-dir>\runs.jsonl` (default `%LOCALAPPDATA%\dcs-mcp\`) before
the reply is rendered:

```
{"ts": "...", "id": "0000000042-k3Jd", "stamp": "1757160000-31244", "host": "hook",
 "state": "gui", "source": "file", "path": "C:/Users/tritiumgg/projects/x/probe.lua",
 "sha256": "…", "bytes": 4071, "chunkname": "@C:/Users/tritiumgg/projects/x/probe.lua",
 "bom": "none", "shebang": "none", "status": "ok", "stage": null, "cpu_ms": 0.41,
 "tick": 88123, "budget": "instructions=1000000"}
```

The tool's text carries the same path and hash on its first line, so the agent's transcript holds
the provenance beside the result. With `--capture`, the reply is also saved verbatim to
`<data-dir>\replies\<id>.res`, headers and body through `latin1`, and a `pending` writes nothing,
because a zero-byte file where a result is expected reads as a measured empty answer
(`prior:pipeline/src/mcp/tools.ts:188-190`, D38). The data directory is never inside either DCS
tree, and `save`'s containment guard applies to `--out` as D38 wrote it.

---

## 5. The installer

`bridge.md` §6.3 decides what `install`, `uninstall` and `verify` mean, what the register is
for, and the hash-before-delete rule. This section is the program that does it, for a person who
has downloaded one file.

### 5.1 What `install` does, in order

1. **Find `Saved Games`** through the shell's known folder (`FOLDERID_SavedGames`), never a string
   built from `%USERPROFILE%` — the folder can be relocated — and `--saved-games` overrides it.
   List the `DCS*` variants there. One is the target; two is an ambiguity, asked (or `--variant`),
   never picked (`prior:pipeline/src/bridge/paths.ts:307-334`).
2. **Refuse the wrong tree.** Resolve the target's real path; refuse it if it lies under the DCS
   install when the install is known, and refuse anything that is not under `Saved Games`. The
   install is read-only, always, including a probe of whether it is writable (D18, D128).
3. **Register before moving.** Append a row to `<data-dir>\install-register.tsv` —
   `<utc> install <path> <sha256> pending` — before any file is touched, and mark it `installed`
   after (D128: a row goes in before anything moves).
4. **Place the hook.** `Scripts\Hooks\DcsApi.lua`, written as `.tmp` in the same directory and
   renamed into place. If a file is already there: its hash is one this project ever shipped —
   an upgrade, replaced by rename with the old bytes parked under
   `<data-dir>\parked\<utc>\Scripts\Hooks\`; any other hash — refused and named, unless `--replace`,
   which parks it the same way. Nothing is deleted, ever. The prior project's `DcsApiEval.lua` and
   `DcsApiExport.lua`, if present, are named as a second bridge that would register callbacks
   twice (`bridge.md` §6.1) and parked under the same rule.
5. **Append one line to `Scripts\Export.lua`:**
   `dofile(lfs.writedir() .. 'Scripts/Hooks/DcsApi.lua') -- dcs-mcp`, creating the file if it
   does not exist, adding a newline first if the file lacks a trailing one, and changing no other
   byte — the file is SRS's and Tacview's as much as ours (`bridge.md` §6.1). A copy of the file
   as found is parked first. The trailing marker is what makes removal an exact-line match.
6. **Say what happens next.** DCS loads `Scripts\Hooks\` at launch and `Export.lua` at mission
   start, so the bridge appears after the next DCS start; print the MCP client registration
   snippet for `dcs-mcp serve`; and print `dcs-mcp verify` as the step that confirms it after DCS
   has run once.

**What it touches**, in full:

| Path | Action | Bound |
|---|---|---|
| `<Saved Games>\<variant>\Scripts\Hooks\DcsApi.lua` | written by rename; an existing file parked | one file |
| `<Saved Games>\<variant>\Scripts\Export.lua` | one line appended; created if absent; a copy parked | one line |
| `<data-dir>\install-register.tsv`, `<data-dir>\parked\<utc>\…` | appended; moved-aside files | the server's own directory, outside both DCS trees |

**What it asks.** Which variant, when two exist. Whether to park a file it does not recognise
(`--replace` answers yes, `--yes` answers every question yes). Nothing else: no wizard, no options
page, no account.

### 5.2 `uninstall`

Removes exactly what `install` put there and nothing beside it: the hook file only when its hash
is one this project shipped; the `Export.lua` line only by exact match including its marker, and
never the lines around it; then restores any file `install` parked, and writes the register row
`uninstalled`. A hook file with an unknown hash is left and named.

### 5.3 `verify`, which `dcs_status` also runs

Reads, and writes nothing: the hook file's hash against the embedded release's; the `Export.lua`
line present exactly once; no other `DcsApi*.lua` in `Hooks\`; `bridge.txt` present with
`protocol: 2`, its `app_version` against the build the embedded script was last measured on
(reported as a difference, never a refusal, `bridge.md` §6.3), its PID; the heartbeat; and
`<Saved Games>\<variant>\Config\autoexec.cfg`, from which it reports the two policy-gate keys
`net.allow_unsafe_api` and `net.allow_dostring_in` if present. The gate is a live runtime
condition — enforced on 2.9.18, reverted by a hotfix, measured absent on 2.9.29.27278
— and `verify` says which states the file admits and
which the bridge would need, and **never writes the file**: every tool that uses this API wants
the same file and their lists differ, so an installer that writes it destroys another tool's
configuration silently (`:1856-1878`). A `refused` from the bridge at runtime points the agent at
this report.

### 5.4 What the installer never does

Writes into the DCS install. Writes `autoexec.cfg`. Deletes a file — it parks. Edits an MCP
client's configuration — it prints the snippet. Runs DCS. Probes whether anything is writable by
writing.

### 5.5 Where the bridge script comes from

`DcsApi.lua` is a release artefact of the bridge project, embedded at build time with its hash,
and the binary carries the hash of every release it has ever embedded, which is how `uninstall`
and an upgrade recognise a file as theirs. A `dcs-mcp` release therefore names the bridge release
it carries, and `verify` prints both.

---

## 6. What the server owes the dormant bridge

`bridge.md` §3.6–3.8 specifies a bridge that costs four VM instructions and one `pcall` per
frame while nobody is using it, and wakes on a file. The server inherits that design and re-decides
nothing; what it owes is a short list, each item a control in §7.

- **On every publish: ensure the arm file.** After the request is renamed into `req/`, stat
  `<session>/arm` and create it if absent; never remove it (`bridge.md` §3.7). The wake is then
  at most `PROBE_EVERY` + 1 frames — 158 ms at 57 Hz (`bridge.md` §12) — and it is paid once per
  dormant period, on the first request. The tool's wait absorbs it and the `pending` text says
  `waking` where §4.4 of that document says so.
- **Never hold the bridge armed.** No keepalive, no periodic `ping`, no timer that touches the
  transport while no tool call is in flight. `dcs_game_state` costs one wake and one quiet period
  — three seconds of armed listing, 16.8 ms of CPU spread over them — and then the bridge sleeps
  again. An agent that calls it every second keeps the bridge armed, and the tool's text says so
  after the third call in a minute.
- **Never wake for a question the files answer.** `dcs_status`, `dcs_collect` and `verify` read
  files and probe a PID and cost the bridge nothing, so they may be called freely; that is the
  property `bridge.md` §4.7 was designed to give them.
- **Hold no handle the bridge would trip over.** The reply watch (`ReadDirectoryChangesW` on
  `res/`) is opened when a request is sent and closed when the reply arrives or the deadline
  passes; it is never left open across tool calls, and never opened on a session `wait` has
  reported `superseded`, because a directory with an open handle cannot be removed and the next
  bridge session removes every sibling at load (`bridge.md` §4.2).
- **The server's own idle is zero.** `rmcp`'s stdio reader blocks on stdin; no tokio timer runs
  between tool calls. A control asserts no thread of the server wakes in 60 s of silence.
- **Both hosts, the same.** The export host has the same arm file and the same rules.

---

## 7. Controls the project must carry

| Control | What it defends | Prior |
|---|---|---|
| The shipped `DcsApi.lua`'s own bytes, produced under the Lua harness, parse under the Rust client; required in CI with the interpreter present | the two implementations of the protocol drifting; §1.4's first step | `prior:pipeline/src/bridge/interop.test.ts`, D38 |
| The stand-in bridge's encoder is not the client's serialiser | a client tested against its own reflection | `prior:pipeline/src/bridge/standin.ts` |
| Six tools listed and callable over a real MCP session on an in-memory pair | a server that offers nothing looks like a client problem | `prior:pipeline/src/mcp/server.test.ts:68-77` |
| A reply that has not arrived is `pending`, carries the id and the phase, and is not `isError` | D37's timeout rule, one layer out | `:93-108` |
| An uninstalled bridge answers the question — and names the file for the host asked, never the other host's | the wrong file named is worse than none | `:265-292` |
| Every refusal reads as a refusal, including `door-shut`, `stale-session`, `oversize` and `budget` | a refusal read as an empty result | `:150-160` |
| `--out` and `--capture` write nothing for a `pending` and the reply verbatim otherwise | a zero-byte file read as a measured empty answer | `:218-263` |
| A cp1251 body and a UTF-8 body survive file, request, reply and capture byte for byte | 599 of 986 descriptions are Russian | `prior:pipeline/src/bridge/client.test.ts:243-257` |
| A handshake path, an `--allow` root and a `--out` path are refused when they resolve inside the install, inside `Saved Games` outside `Logs\`, through an 8.3 short spelling or through a junction; `std::fs::canonicalize`'s `\\?\` prefix is stripped before comparison and case is folded | D18, D20, D38, measured on this machine's own short names | `prior:pipeline/src/bridge/paths.test.ts:99-205` |
| `dcs_eval_file` refuses a path outside every root, any path under `Config\`, and the install, before reading a byte; the refusal names the rule and no content | §4.2–4.3 | — |
| A file one byte over the ceiling is refused naming the limit and the size; one at the ceiling is sent whole | §4.4 | — |
| A file with a BOM, a `#` first line and CRLF endings raises on its line 47 and the reply says `…:47:` from `hook`, from a `dostring_in` state and through the door; the run record carries `bom: stripped`, `shebang: blanked` and the file's SHA-256 | §4.5–4.6, `bridge.md` §7.3 | — |
| `dcs_game_state` against a stand-in that answers `load` returns `loading` with no round trip; against `sim` with `getPause` true returns `paused (read)` and notes the callback phase disagreeing; against a `refused` `gui` probe returns `session: client`; against an errored read returns `unknown: <error>` on that axis alone; against tier 2 off returns `unknown (tier 2 off)` | §3.4–3.5 — no default arm | — |
| Every game-state read is its own request, one `pcall`, and a `DCS.*` name not in the constant list is never sent by the tool | §3.3 | — |
| `send` stats the arm file after the rename and creates it when absent; a stand-in that removes it between two sends sees it recreated; the client never removes it | §6, `bridge.md` §3.7 | — |
| No reply watch is open between tool calls, and none is opened on a superseded session; the stand-in's sweep of a sibling directory succeeds while the server is idle | §6, `bridge.md` §4.2 | — |
| Sixty seconds of silence wakes no thread of the server | §6 | — |
| `install` twice leaves one hook file and one `dofile` line; a foreign `DcsApi.lua` is refused without `--replace` and parked with it; `uninstall` removes the line and not its neighbours, refuses an unknown hash, restores a parked file, and the register holds a row written before each move | §5, D128 | `prior:tools/Park-DcsThirdPartyScripts.ps1:16-18` |
| `verify` reports both `autoexec.cfg` keys from a fixture file and a `git status`-clean fixture tree afterwards proves it wrote nothing | §5.3 | — |
| The CLI's text for one reply is byte-identical to the tool's | one wording for one reply, D38 | `prior:pipeline/src/mcp/tools.ts:1-11` |

---

## 8. What could not be determined

- **How the mission editor is told from the main menu.** §3.6. Three candidates, none measured;
  the value is `menu-or-editor` until one is.
- **Whether tier-2 reads are safe from a hook.** §3.3. No hook-state precedent; each is measured
  alone under the supervisor before it is enabled.
- **Whether the batching of `DCS.*` reads is the hazard, or particular reads are.** §3.3,
  `bridge.md` §11. One chunk per read is chosen so a crash names its killer either way.
- **What `DCS.getSimulatorMode()` returns**, and whether `DCS.getMissionName()` is empty at the
  menu. §3.5–3.6. Recorded raw until a table of observed values exists.
- **Which callback names DCS's hook dispatcher looks up.** Nothing enumerates them (D36). The 63
  `on*` globals of the `gui` state are the best lead, and two names the bridge offers are not
  among them. §3.6.
- **Whether `std::fs::canonicalize` expands an 8.3 short name and follows a junction on this
  machine the way `realpathSync.native` does.** Both call `GetFinalPathNameByHandleW`; the control
  in §7 is what turns that into a fact, and it needs the Rust toolchain that is present here but
  was not exercised in this session.
- **Whether `rmcp`'s stdio server runs cleanly under Windows console handles** when launched by an
  MCP client. Unverified here; the first `serve` under a real client is the measurement.
- **Whether the five tier-1 reads answer on a client joined to a server.** S7 measured `hook`
  answering there and nothing about these calls.
- **The policy gate's enforcement on the running build.** §5.3. A live condition, reported and
  never edited.
- **`prior:docs/memory/**`**, in particular `eval-bridge-consumer.md`'s mutation table and
  `bridge-operational-hazards.md`, which are cited here only through the decision records and
  code comments that cite them.

---

## 9. Figures, with the commands that produced them

```powershell
# The port surface (§1.3, §1.4): source and controls a Rust client replaces
Set-Location 'C:\Users\tritiumgg\projects\dcs-api'
$src = 'pipeline/src/bridge/client.ts','pipeline/src/bridge/fs.ts','pipeline/src/bridge/paths.ts',
  'pipeline/src/bridge/protocol.ts','pipeline/src/bridge/standin.ts',
  'pipeline/src/mcp/server.ts','pipeline/src/mcp/tools.ts'
$tst = 'pipeline/src/bridge/client.test.ts','pipeline/src/bridge/interop.test.ts',
  'pipeline/src/bridge/paths.test.ts','pipeline/src/bridge/protocol.test.ts',
  'pipeline/src/mcp/server.test.ts'
($src | % { (Get-Content $_).Count } | Measure-Object -Sum).Sum   # 2589
($tst | % { (Get-Content $_).Count } | Measure-Object -Sum).Sum   # 1409

# TypeScript citations in bridge.md, after its 2026-09-06 revision (§1.4)
$t = [IO.File]::ReadAllText('docs/reshape/bridge.md')
([regex]::Matches($t, 'prior:pipeline/src/[A-Za-z0-9_/.-]+\.ts')).Count       # 41
([regex]::Matches($t, 'prior:pipeline/src/[A-Za-z0-9_/.-]+\.ts:\d')).Count    # 34 with line refs
(([regex]::Matches($t, 'prior:pipeline/src/[A-Za-z0-9_/.-]+\.ts') | % Value |
  Sort-Object -Unique).Count)                                                  # 14 files
([regex]::Matches($t, 'prior:[A-Za-z0-9_/.-]+')).Count                         # 137 prior: in all
# The figure quoted to this document's author was 121; the method behind it is not known and the
# numbers above are the ones a reader can reproduce.

# The Rust SDK (§1.2), fetched 2026-09-06
$r = Invoke-RestMethod 'https://crates.io/api/v1/crates/rmcp' -Headers @{ 'User-Agent' = 'dcs-mcp-spec' }
"$($r.crate.max_version) $($r.crate.updated_at) $($r.crate.downloads)"
#   3.2.0 08/31/2026 23:16:49 24601989
$r.versions | Select-Object -First 5 | % { "$($_.num) $($_.created_at)" }
#   3.2.0 2026-08-31, 3.1.4 2026-08-20, 3.1.3 2026-08-17, 3.1.2 2026-08-07, 3.1.1 2026-08-05
# Protocol revision and transports: github.com/modelcontextprotocol/rust-sdk (README: implements
# stable MCP 2026-07-28, compatible with 2025-11-25 and earlier; stdio, child process, streamable
# HTTP); modelcontextprotocol.io/specification/versioning names 2026-07-28 as current; docs.rs
# lists the features `server`, `client`, `macros`, `transport-io`,
# `transport-streamable-http-client`, `transport-streamable-http-server`, tokio ^1.

# The TypeScript SDK, for comparison (§1.2)
$n = Invoke-RestMethod 'https://registry.npmjs.org/@modelcontextprotocol/sdk/latest'
"$($n.version) node $($n.engines.node) deps $(($n.dependencies | Get-Member -MemberType NoteProperty).Count)"
#   1.30.0 node >=18 deps 17

# The toolchain on this machine (§1.6)
cargo --version; rustc --version; node --version
#   cargo 1.98.1 (797e8a9bc 2026-08-05)   rustc 1.98.1 (48a229cea 2026-09-01)   v24.19.0

# ED's own hook-state reads (§3.3): every DCS.* name Scripts/Hooks/webGUI.lua is recorded calling
Set-Location 'C:\Users\tritiumgg\projects\dcs-api'
$p = ''; Get-Content model/hook/DCS.yaml | % {
  if ($_ -match '^    path: (DCS\.\w+)') { $p = $Matches[1] }
  elseif ($_ -match 'Scripts/Hooks/webGUI\.lua') { $p } } | Sort-Object -Unique
#   DCS.exportToMiz DCS.getLogHistory DCS.getMissionDescription DCS.getMissionFilename
#   DCS.getMissionName DCS.getMissionResult DCS.getModelTime DCS.getPause DCS.getSimulatorMode
#   DCS.setPause DCS.stopMission
# The five tier-1 reads are the getters of that list that answer a state question; their webGUI.lua
# call sites are lines 257, 262, 273, 274 and 276 (cited_uses in the same file).

# The callback vocabulary (§3.6): on* globals ED defines in the gui state, and what the bridge offers
$names = Get-ChildItem model/gui -Filter 'on*.yaml' | % {
  Select-String -Path $_.FullName -Pattern '^    path: (on[A-Za-z]+)$' | % { $_.Matches[0].Groups[1].Value } } |
  Sort-Object -Unique
$names.Count                                                     # 63
$names -contains 'onShowMissionEditor'; $names -contains 'onShowMultiplayer'   # False, False
$names -contains 'onShowMainInterface'; $names -contains 'onShowPool'          # True, True
(Select-String -Path tools/hooks/DcsApiEval.lua -Pattern '^callbacks\.(on[A-Za-z]+)').Count   # 18

# Wake bound and quiet-period cost, inherited (§6): bridge.md §12
"wake bound {0:N0} ms" -f ((8+1)*17.5)                          # 158
"disarm cost {0} listings, {1:N1} ms CPU" -f (3*57), (3*57*0.098)   # 171 listings, 16.8 ms over 3 s
```
