# dcs-eval

Evaluate Lua inside a running DCS World, from an MCP agent or from a terminal.

Two halves of one product. **The executor** is a single Lua file DCS loads at
startup; it sleeps until something asks it for anything, and answers in the
state you name. **`dcs-mcp`** is a single Windows binary that installs the
executor, speaks its file-based protocol, and serves six MCP tools.

> **Nothing here is built yet.** This repository holds two frozen
> specifications, a plan, and the toolchain they stand on. Every section below
> describes what the finished thing does and is marked `not built` until the
> task that delivers it lands. `docs/STATE.md` says where the work actually
> stands.

## What it is for

An agent that can read DCS's Lua state can answer questions no external API
exposes: what units exist, what a mission's triggers are doing, what a table
in `_G` actually holds. Getting there means running code *inside* the
simulator, in the right Lua state, without destabilising a game somebody is
flying.

The executor does that, and it does it while costing almost nothing when nobody
is asking. A dormant frame is a handful of VM instructions and one file stat
every few frames; the executor wakes on a file appearing and goes back to sleep
after a quiet period.

The dormant frame is built and held to zero allocations by the harness, and so
are the two transitions around it: the executor loads asleep, wakes within a
few frames of the arm file a client writes beside its first request, and after
a few seconds with nothing to answer removes that file itself and goes back to
sleep. While it is awake it writes a heartbeat file every couple of seconds,
and at every arm, disarm and phase change; while it is asleep it writes none,
which is the point — a tool asking how things are reads that file and the
process id and costs the game nothing.

## Install — *not built*

Download `dcs-mcp.exe` and run:

```
dcs-mcp install
dcs-mcp verify
```

`install` finds your `Saved Games\DCS*` folder, places the executor in
`Scripts\Hooks\`, and appends one line to `Export.lua`. It never writes to the
DCS install itself, and nothing already in `Saved Games` is deleted or
overwritten — a file in the way is registered and moved aside, and `uninstall`
puts it back. The register and the copies live in `%LOCALAPPDATA%\dcs-mcp\`,
outside both DCS folders so that removing either leaves them standing, and
nothing under there is ever deleted.

`verify` re-checks the installation and writes nothing.

> **Uninstall `dcs-api-bridge` first, by hand.** This replaces it, and the two
> use the same directory and the same transport root. Running both is not a
> degraded mode; it is two hooks answering the same request, at twice the idle
> cost. Delete `Scripts\Hooks\DcsApiEval.lua` and remove its `dofile` line from
> `Scripts\Export.lua` before you install.
>
> **Nothing here checks that you did.** `install` looks for one file name in
> `Scripts\Hooks\` — its own — and `verify` reports only what this project put
> there. What else you load is yours. ADR 0001, ADR 0022.

## Run it as an MCP server

Point your MCP client at the binary and the `serve` verb:

```
dcs-mcp serve --saved-games "C:\Users\you\Saved Games" --variant DCS.openbeta
```

Give the folder its real path. `Saved Games` can be relocated, so a path built
out of `%USERPROFILE%` is not reliably the folder DCS writes into; if you are
not sure where yours is, `install` reports the one it found — *not built*, so
until it is, read the path off the DCS folder in your own `Saved Games`.

Add `--host export` to talk to the Export.lua half instead of the hook.

It speaks MCP over stdin and stdout, so stdout carries nothing but protocol
frames and every diagnostic goes to stderr — which is where your client's log
will show them.

The executor is looked for afresh on every call rather than once at start-up:
its directory appears the first time DCS loads it, so you can point a client at
the server before you install and before the game is running, and nothing has
to be restarted afterwards.

`--saved-games` and `--variant` must both be given; finding your `Saved Games`
folder and its `DCS*` variants for you is *not built*. The server registers,
lists and answers all six tools below; each takes an optional `host`, which is
`hook` or `export` and falls back to `--host`.

## The six tools

| Tool | What it says |
|---|---|
| `dcs_status` | what is readable without asking the executor anything, and it writes nothing: the hook's hash against the release this build carries, the `Export.lua` line present exactly once, any second hook beside ours, the two `autoexec.cfg` policy keys as they are written — then the session, alive, phase, armed, every problem found. A DCS build that differs from the one this was measured on is reported as a difference and never as a fault |
| `dcs_ping` | liveness proved by a reply, with the phase and tick |
| `dcs_game_state` | what the game is doing, every fact it rests on, and the basis of each value |
| `dcs_eval` | evaluate a chunk in the state you name |
| `dcs_eval_file` | the same from a file, with its path and content hash recorded |
| `dcs_collect` | pick up a reply that was still pending, by id |

An answer comes back as one word on the first line and a line each after it. A
reply the executor answered is headed `reply` and carries its headers and then
its body. A reply that refused — `no-mission`, `stale-session`, `oversize` and
`budget` among them — is headed by the word that refused it, says in one line
why, and is marked an error, so a refusal never reads as a call that succeeded
and came back empty. A `pending` names the id to collect under and the phase
the session was in, and is *not* an error: nothing failed, and the reply is
picked up afterwards.

A call that names no `wait_seconds` waits 15 seconds. That is a wait and never
a limit: when it runs out the request is still published, the answer names an
id, and `dcs_collect` picks the reply up afterwards. A mission load on its own
routinely takes longer than the wait.

`unknown` is a value, not a guess: an axis nothing measured says so and says
why.

`dcs_game_state` sends five reads every time — the five ED's own hook script
makes — and seven more only when you ask. Four, `extra`, have no precedent in
the state they would run in; three, `suspect`, were in a batch of reads that
crashed DCS, one of them named as the likely cause. None of the seven is
measured safe yet. Ask with `reads` on the tool or `--reads` on the
`game-state` verb: a group, or one read by its name — `multiplayer`, `server`,
`track`, `player_id`, `mission_loaded`, `player_unit_type`, `mission_theatre`
— several separated by commas on the command line. A read you did not ask for
says so, `unknown (tier 2 off)` or `unknown (suspect reads off)`, and every
read no summary is made of is printed on a line of its own.

`dcs_eval_file` reads only what lies under a directory you allow it, and
refuses the `Config\` of every DCS write directory it is told about, and the
DCS install, whatever else is allowed — the first holds your account
credentials, the second holds nothing a chunk needs. Finding the `DCS*`
siblings of the write directory you configure, so a second variant's
credentials are refused too, is *not built*; nor is how the roots are
configured on the command line. Until a root can be allowed, the allowed list
is empty — and an empty list admits nothing rather than everything, so
`dcs_eval_file` refuses every path you give it by that same rule. Use
`dcs_eval` meanwhile. `eval --file` on the command line is refused for the
same reason, and will be until a root can be allowed.

## Use it from a terminal

Four verbs, and each one calls the same function its tool calls, so the words
you read here are the words the tool gives — the head word, the reply's
headers and body, a refusal that says why, a `pending` that names its id.

```
dcs-mcp status --saved-games "%USERPROFILE%\Saved Games" --variant DCS.openbeta
dcs-mcp ping --saved-games ... --variant DCS.openbeta
dcs-mcp game-state --saved-games ... --variant DCS.openbeta
dcs-mcp game-state --reads mission_loaded --saved-games ... --variant DCS.openbeta
dcs-mcp eval hook "return #coalition.getGroups(2)" --saved-games ... --variant DCS.openbeta
dcs-mcp eval missionscripting --file .\probe.lua --saved-games ... --variant DCS.openbeta
```

`--saved-games <dir>` and `--variant <name>` are required, the same as for
`serve`, and `--host hook|export` picks which of the executor's two hosts to
talk to. `--wait-seconds`, `--max-instructions` and `--chunkname` are the
call's own, and a verb waits 15 seconds by default — a wait and never a
limit, exactly as for the tools. `--reads` belongs to `game-state` alone.

`--out <path>` writes the reply to a file, and `--capture` keeps a copy under
this build's own data directory. Both write the bytes the executor published,
byte for byte, rather than a re-rendering of them. Where no reply came back —
a `pending`, or a request that was refused before it was sent — **neither
writes anything at all**, not even an empty file, because an empty file reads
back as a reply that returned nothing. `status` and `game-state` answer without one reply off the wire, so
they refuse both flags rather than accept them and write nothing.

`--data-dir <dir>` says where that data directory is, and it is not tied to
`--capture`: the directory holds the run record too, and every evaluation
writes one whether or not a reply is being kept. It works for `serve` as
well, with the same spelling.

A verb exits 0 for an answer or a `pending`, 1 where the answer is a refusal
or a file you asked for could not be written, and 2 for a command line that
would not parse.

`install`, `verify` and `uninstall` are *not built*. An `--out` path is not
yet judged against the containment rule the install paths are judged by —
*not built*; `--capture` is, because it goes through the same data directory.

## What ran, written down

Every evaluation — from a tool call or from the terminal — appends one line of
JSON to `runs.jsonl` under the data directory (`%LOCALAPPDATA%\dcs-mcp` unless
`--data-dir` says otherwise), before the answer is rendered. A line carries
when it ran, the id the reply came back under, the executor session, the host
and the Lua state, whether the chunk came from a file or off the line, and what
the reply said: its status, the stage that failed if one did, the CPU
milliseconds and the tick.

For a file, it also carries the resolved path, the chunk name and the SHA-256
of the bytes that were read — the same hash the answer's own last line shows,
because both come from the reader that opened the file. No root can be allowed
yet, so nothing you can run today produces such a line — *not built*, the same
sentence as for `eval --file` above. A chunk given on the line has no path and
no hash: nothing read it, so there is nothing to attest.

A file evaluation refused **before** the file is read — a path outside the
allowed roots, a name too long, anything judged before it is opened — writes
no line at all. An absent line means nothing ran.

Nothing reads this file back. It is a record for you, and for whoever asks
afterwards what a result came from.

## Building it

Windows only. Both halves exist to talk to a running DCS, and DCS is a Windows
product. You need the Visual Studio Build Tools with the C++
workload and [mise](https://mise.jdx.dev).

```sh
mise install        # the Rust toolchain and the two language servers
mise run lua-build  # builds the reference Lua 5.1.5 from lua.org's tarball, once
mise run check      # everything CI gates a pull request on
```

The second step is not the usual shape and there is a reason: this project is
proven under Lua 5.1.5 PUC-Rio and nothing else, and mise's Lua plugin builds
that version with `make`, which Windows does not have. `tools/mklua.sh`
compiles the same pinned tarball with MSVC instead, and `tools/check-lua.sh`
refuses to run anything under a different interpreter.

## The mutation sweep

Every check in this build was proved by breaking the code it watches and
watching it go red. `tools/sweep.sh` re-runs those proofs: it applies each
recorded mutation to a file it has copied first, runs the one command that must
fail, restores from the copy and reports one row per control.

```sh
sh tools/sweep.sh                  # every control
sh tools/sweep.sh --list           # what the inventory holds; edits nothing
sh tools/sweep.sh --only paths/    # one control, or one group by its slash
```

It is deliberately not part of `mise run check`: it is slow and it edits files
in the working tree. A run over the Stage 3–6 controls takes about three and a
half minutes on a warm `target/`; a cold checkout pays a full workspace build
first. It restores from its own copies and never from git, so an uncommitted
edit of your own survives a run — and a run that fails leaves the tree exactly
as it found it. It exits 0 when every control reddened as recorded, 1 when one
did not, and 2 when the tree cannot be vouched for, which is the code to stop
and look at.

It runs weekly against `main` — `.github/workflows/sweep.yml`, which also takes
a manual run from the Actions tab — and it is green before a milestone closes.
Run it yourself after changing any code a control watches, rather than waiting
for the Monday run to tell you.

The controls themselves are `docs/mutations.md`, which also says what the sweep
does *not* cover. A task that builds a control adds its entry there.

## Where things are written down

- `docs/STATE.md` — what was just done and what is next. Read it first.
- `docs/mutations.md` — every control, the mutation that must redden it, and
  what was observed when it did.
- `docs/PLAN.md` — build order, 61 tasks in 10 stages.
- `docs/specs/` — the two frozen specifications the build starts from. They are
  not maintained and the build drifts from them by design.
- `docs/decisions/` — where the build went somewhere the specifications did
  not, and the Stage 9 measurements.
- `docs/audit.md` — what the two documents disagree about.

## Licence

MIT. See `LICENSE`.
