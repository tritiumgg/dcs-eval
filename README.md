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
puts it back.

`verify` re-checks the installation and writes nothing.

> **Uninstall `dcs-api-bridge` first.** This replaces it, and the two use the
> same directory and the same transport root. Running both is not a degraded
> mode; it is two hooks answering the same request. ADR 0001.

## The six tools — *not built*

| Tool | What it says |
|---|---|
| `dcs_status` | what is readable without asking the executor anything: installed, session alive, phase, armed, every problem found |
| `dcs_ping` | liveness proved by a reply, with the phase and tick |
| `dcs_game_state` | what the game is doing, every fact it rests on, and the basis of each value |
| `dcs_eval` | evaluate a chunk in the state you name |
| `dcs_eval_file` | the same from a file, with its path and content hash recorded |
| `dcs_collect` | pick up a reply that was still pending, by id |

`unknown` is a value, not a guess: an axis nothing measured says so and says
why.

## Use it from a terminal — *not built*

The CLI speaks the same functions as the tools and prints the same words:

```
dcs-mcp eval hook "return #coalition.getGroups(2)"
dcs-mcp eval-file missionscripting ./probe.lua --out reply.txt
dcs-mcp game-state
```

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

## Where things are written down

- `docs/STATE.md` — what was just done and what is next. Read it first.
- `docs/PLAN.md` — build order, 52 tasks in 10 stages.
- `docs/specs/` — the two frozen specifications the build starts from. They are
  not maintained and the build drifts from them by design.
- `docs/decisions/` — where the build went somewhere the specifications did
  not, and the Stage 9 measurements.
- `docs/audit.md` — what the two documents disagree about.

## Licence

MIT. See `LICENSE`.
