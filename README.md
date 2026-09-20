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

> **Uninstall `dcs-api-bridge` first.** This replaces it, and the two use the
> same directory and the same transport root. Running both is not a degraded
> mode; it is two hooks answering the same request. ADR 0001.

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
folder and its `DCS*` variants for you is *not built*. The server offers no
tools yet — *not built*; the six below are what it will offer.

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

`dcs_game_state` sends a fixed list of reads and nothing else; three calls
suspected in a DCS crash are never sent at all. A second tier of four reads,
the ones with no precedent for the state they would run in, is built and off,
and there is no way to ask for it yet — *not built*.

`dcs_eval_file` reads only what lies under a directory you allow it, and
refuses the `Config\` of every DCS write directory it is told about, and the
DCS install, whatever else is allowed — the first holds your account
credentials, the second holds nothing a chunk needs. Finding the `DCS*`
siblings of the write directory you configure, so a second variant's
credentials are refused too, is *not built*; nor is how the roots are
configured on the command line.

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
