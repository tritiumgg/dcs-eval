# dcs-eval

Evaluate Lua inside a running DCS World, from an MCP agent or from a terminal.

Two halves of one product. **The executor** is a single Lua file DCS loads at
startup; it sleeps until something asks it for anything, and answers in the
state you name. **`dcs-mcp`** is a single Windows binary that installs the
executor, speaks its file-based protocol, and serves six MCP tools.

> **Built, and proved off DCS; not yet proved in it.** The executor, the
> client, the MCP server and its six tools, the command line and the installer
> are built and held by tests that need no game. What remains is the live
> proof at a running DCS install, and where a section below describes
> something not built it says so. `docs/STATE.md` says where the work stands.

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

## Install

> **Uninstall `dcs-api-bridge` first, by hand.** This replaces it, and the two
> use the same directory and the same transport root. Running both is not a
> degraded mode; it is two hooks answering the same request, at twice the idle
> cost. Delete `Scripts\Hooks\DcsApiEval.lua` and remove its `dofile` line from
> `Scripts\Export.lua` before you install.
>
> **Nothing here checks that you did.** `install` looks for one file name in
> `Scripts\Hooks\` — its own — and `verify` reports only what this project put
> there. What else you load is yours. ADR 0001, ADR 0022.

Download `dcs-mcp.exe` and run it from a terminal:

```
dcs-mcp install
```

`install` finds your `Saved Games` folder the way Windows does — through the
shell's known folder, so one you have moved is found where it really is — and
the `DCS*` folders in it, one per DCS variant. With one, that is where it
installs. With more than one it installs into none of them, names them all,
and asks you to say which:

```
dcs-mcp install --variant DCS
```

It never picks one for you and never stops at a prompt: the refusal is the
question, and the flag is the answer. `--saved-games <dir>` points it at a
folder of your choosing instead.

It places the executor in `Scripts\Hooks\` and appends one line to
`Scripts\Export.lua`, creating the file if there is none. It never writes to
the DCS install itself, and nothing already in `Saved Games` is deleted or
overwritten — a file in the way is registered and moved aside, and `uninstall`
puts it back. A file already at the executor's name that this project did not
ship is refused and named; `--replace` moves it aside and installs anyway.
Nothing else in `Scripts\Hooks\` is looked at. The register and the copies
live in `%LOCALAPPDATA%\dcs-mcp\` (or `--data-dir <dir>`), outside both DCS
folders so that removing either leaves them standing, and nothing under there
is ever deleted.

When it is done it says what it did, that DCS picks the executor up the next
time it starts, the `verify` line to run after that, and a snippet to register
the server with your MCP client, naming the folder and variant it installed
into.

```
dcs-mcp verify --variant DCS
dcs-mcp uninstall --variant DCS
```

`verify` reads the installation and the executor's session and writes
nothing. Its first line is `verified` or `not verified`, with the folder it
looked at, and under it is a row for each part with what to do where
something is wrong:

```
verified: C:\Users\you\Saved Games\DCS

  ok       hook          Scripts\Hooks\DcsEvalExecutor.lua, this release
  ok       Export.lua    loads the executor once
  ok       DCS           running (process 25924), not used yet since it started
  note     DCS version   2.9.29.27468
  note     autoexec.cfg  net.allow_unsafe_api is not set
  note     autoexec.cfg  net.allow_dostring_in is not set
```

Until DCS has started once with the executor in place there is no session
to read, so the DCS row says `waiting` and the report is `not verified`.
`--verbose` adds every problem in its exact words and every fact read: the
hashes, the paths, the session's stamp and heartbeat. `--host export`
reports the `Export.lua` host's session instead of the hook's.

`uninstall` takes the executor out — only a file whose hash this project
shipped — and our one line out of
`Export.lua`, and puts back every file `install` moved aside. A file at our
name that we did not ship is left where it is and named. If `install` created
`Export.lua`, `uninstall` leaves it empty rather than deleting a file nothing
recorded it creating. Give `uninstall` the same `--data-dir` you gave
`install`, if you gave one: the register it restores from lives there. After
`install` has run over a copy of ours, whether the same release or an older
one, `uninstall` puts that copy back; run `uninstall` again to take it out.

All three take `--saved-games` and `--variant`, and exit 0 when done (for
`verify`: when `verified`), 1 when refused or not verified, and 2 for a
command line that would not parse.

## Run it as an MCP server

Point your MCP client at the binary and the `serve` verb:

```
dcs-mcp serve --saved-games "C:\Users\you\Saved Games" --variant DCS.openbeta
```

Give the folder its real path. `Saved Games` can be relocated, so a path built
out of `%USERPROFILE%` is not reliably the folder DCS writes into; if you are
not sure where yours is, `install` prints a registration naming the one it
found.

Add `--host export` to talk to the Export.lua half instead of the hook.

It speaks MCP over stdin and stdout, so stdout carries nothing but protocol
frames and every diagnostic goes to stderr — which is where your client's log
will show them.

The executor is looked for afresh on every call rather than once at start-up:
its directory appears the first time DCS loads it, so you can point a client at
the server before you install and before the game is running, and nothing has
to be restarted afterwards.

`--saved-games` and `--variant` must both be given to `serve`; only the
installer finds them for you, and the line it prints names both. The server
registers, lists and answers all six tools below; each takes an optional
`host`, which is `hook` or `export` and falls back to `--host`.

## The six tools

| Tool | What it says |
|---|---|
| `dcs_status` | what is readable without asking the executor anything, and it writes nothing: the hook's hash against the release this build carries, the `Export.lua` line present exactly once, any second hook beside ours, the two `autoexec.cfg` policy keys as they are written — then the session, alive, phase, armed, every problem found. It is `verify --verbose` under the word `status`: the verdict and a row per part, every problem in its exact words, then every fact under `details`. A DCS build that differs from the one this was measured on is reported as a difference and never as a fault |
| `dcs_ping` | liveness proved by a reply, with the phase and tick |
| `dcs_game_state` | what the game is doing, every fact it rests on, and the basis of each value |
| `dcs_eval` | evaluate a chunk in the state you name |
| `dcs_eval_file` | the same from a file, with its path and content hash recorded |
| `dcs_collect` | pick up a reply that was still pending, by id |

An answer comes back as one word on the first line and a line each after it. A
reply the executor answered is headed `reply` and carries its headers, a blank
line, then its body, so the first blank line ends the headers as it does on the
wire. A reply that refused — `no-mission`, `stale-session`, `oversize` and
`budget` among them — is headed by the word that refused it, says in one line
why, and is marked an error, so a refusal never reads as a call that succeeded
and came back empty. A `pending` names the id to collect under and the phase
the session was in, and is *not* an error: nothing failed, and the reply is
picked up afterwards.

A call that names no `wait_seconds` waits 15 seconds. That is a wait and never
a limit: when it runs out the request is still published, the answer names an
id, and `dcs_collect` picks the reply up afterwards. A mission load on its own
routinely takes longer than the wait. A reply is kept for five minutes from the
frame that answered it, so one whose request ran long is kept that much less.
Past that, the executor removes it the next time it is awake — never
while it is asleep — and `dcs_collect` then finds nothing under the id, and
says it may have been removed.

`unknown` is a value, not a guess: an axis nothing measured says so and says
why.

`dcs_game_state` sends twelve reads every time: the five ED's own hook script
makes, six more — `multiplayer`, `server`, `track`, `player_id`,
`player_unit_type`, `mission_theatre` — that a live run sent alone from the
hook, at the menu and in a mission, and that answered, and one in the `gui`
state, the mission editor's map. `DCS.getMissionLoaded` crashed DCS in a
mission, twice, and is never sent; `dcs_eval` can still call it, at your own
risk. The session is `single player` where `multiplayer` says false — DCS
answers `server` true in single player too — and `hosting` where both say
true. Outside a mission and a load, the editor's map says where the game is,
`at the main menu (read)` or `in the mission editor (read)`; where the
`gui` state refuses the read, as on a client joined to a server, the answer
stays at the main menu or in the mission editor and says why. In a mission
the answer adds the theatre and your unit, and names the mission by its
file where DCS gives `tempMission` for the name, as it does for every
single-player mission. Every read no summary is made of is printed on a line
of its own.

`dcs_eval_file` reads any file this server can read, wherever it lies — much
as a chunk could open it with `io.open` in the `hook` state — and
`eval --file` on the command line is the same. Where the file lies is never
judged; the one limit is size: a file whose bytes and the request's own
headers together come to more than the ceiling the executor published is
refused before a byte of it is read, naming the limit and the file's size,
and nothing is ever cut to fit. A path that is not there, a directory, a
name too long and a path with a character past ASCII are refused too,
because none of them can be sent at all. A
compile error comes back as Lua wrote it, and that quotes the file — which is
the point of reading your own.

## Use it from a terminal

Four verbs, and each one calls the same function its tool calls, so the words
you read here are the words the tool gives — the head word, the reply's
headers and body, a refusal that says why, a `pending` that names its id.

```
dcs-mcp status --saved-games "%USERPROFILE%\Saved Games" --variant DCS.openbeta
dcs-mcp ping --saved-games ... --variant DCS.openbeta
dcs-mcp game-state --saved-games ... --variant DCS.openbeta
dcs-mcp eval hook "return #coalition.getGroups(2)" --saved-games ... --variant DCS.openbeta
dcs-mcp eval missionscripting --file .\probe.lua --saved-games ... --variant DCS.openbeta
```

`--saved-games <dir>` and `--variant <name>` are required, the same as for
`serve`, and `--host hook|export` picks which of the executor's two hosts to
talk to. `--wait-seconds`, `--max-instructions` and `--chunkname` are the
call's own, and a verb waits 15 seconds by default — a wait and never a
limit, exactly as for the tools.

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

An `--out` path is not yet judged against the containment rule the install
paths are judged by — *not built*; `--capture` is, because it goes through the
same data directory.

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
because both come from the reader that opened the file. A chunk given on the
line has no path and no hash: nothing read it, so there is nothing to attest.

A file evaluation refused **before** the file is read — a file too big for one
request, a path that is not there or is a directory, a name too long, a
path with a character past ASCII, anything judged before it is opened — writes no line at all. An absent line
means nothing ran.

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
in the working tree. A full run takes about fourteen minutes on a warm
`target/`; a cold checkout pays a full workspace build first. It restores from
its own copies and never from git, so an uncommitted edit of your own survives
a run — and a run that fails leaves the tree exactly as it found it. It exits 0
when every control reddened as recorded, 1 when one did not, and 2 when the
tree cannot be vouched for, which is the code to stop and look at.

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
- `docs/PLAN.md` — build order, 64 tasks in 10 stages.
- `docs/specs/` — the two frozen specifications the build starts from. They are
  not maintained and the build drifts from them by design.
- `docs/decisions/` — where the build went somewhere the specifications did
  not, and the Stage 9 measurements.
- `docs/audit.md` — what the two documents disagree about.

## Licence

MIT. See `LICENSE`.
