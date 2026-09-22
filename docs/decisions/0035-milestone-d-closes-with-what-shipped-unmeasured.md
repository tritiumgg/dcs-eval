# ADR 0035: Milestone D closes, and this is what shipped unmeasured

## Status

Accepted

## Context

Both specifications carry a list of what their authors could not determine
from the tree: `bridge.md` §11, "What could not be determined from the tree",
and `mcp.md` §8, "What could not be determined". `docs/audit.md` folds both
into Stage 9 rather than guessing, and the convention says a probe answer is
a record. Stage 9 is over: T47 to T50 were measured on 2026-09-21 (ADR 0031),
the cutover T52 and the permanent-installation acceptance T51 were done by
hand at the maintainer's install on 2026-09-22 — a `Saved Games` holding
`DCS`, `DCS_F4E` and `DCS_OH58D`, so every installer verb took `--variant
DCS` — and the temporary `dcs-mcp live` that took the figures has left the
binary. Nothing further is scheduled
to measure anything, so every blank still open at this point is a blank the
project ships with, and `docs/STATE.md` had been carrying the list under
"What only Stage 9 sees" as if a stage were still coming to see it.

`mcp.md` §8, on two reads whose answers the same carry held:

> **What `DCS.getSimulatorMode()` returns**, and whether `DCS.getMissionName()`
> is empty at the menu. §3.5–3.6. Recorded raw until a table of observed values
> exists.

## Decision

Milestone D is closed, and the blanks below are shipped as blanks, each with
the record or the reason that stands in for a measurement.

**Answered, and where.** From `bridge.md` §11: what `lfs.tempdir()` returns
inside DCS is `%TEMP%\DCS` (ADR 0029), and `ping` answered through whatever
transport the first live handshake named; whether that was the temp
candidate or the `Logs` fallback, which is what ED's `lfs.dir` clamping a
path would turn on, no record wrote down, and the row stays open below; the
cost of one `lfs.attributes` in DCS is 13.9 µs present and 8.7 µs absent, and
the dormant frame is 0.0011 ms against the 0.098 ms baseline, so a dormant
executor is a number and not "not noticeable" (ADR 0031); the operator policy
gate is read by `verify` and reported, never edited (ADR 0032). From `mcp.md`
§8: the editor is told from the menu by `MapWindow.getVisible()` in `gui`;
the tier-2 reads were sent in turn in one session under the supervisor, a
read that stopped the run retested alone (ADR 0028), and six are on by
default with `getMissionLoaded` gone; a particular read is the hazard and
not the batching, because `getMissionLoaded` crashed DCS in the sequence and
again sent alone; `getSimulatorMode()` is 1 at the menu and in the editor
and 4 in a mission, and `getMissionName()` is empty at the menu and
`tempMission` in single player; of the callback names the executor
registers, `onMissionLoadBegin`, `onMissionLoadEnd`, `onSimulationStart`
and `onSimulationResume` fired in a mission, so the dispatcher looks those
four up, and none fired at the menu or in the editor, `onShowMainInterface`
included (all ADR 0031). `canonicalize` against an 8.3 name
and a junction is `paths/resolution-left-textual` in `docs/mutations.md`,
the one resolution ADR 0026 keeps ahead of the size check. `rmcp`'s stdio
server under a real MCP client is the registration that has been serving
Claude Desktop and Claude Code since 2026-09-22.

**The handshake's figures.** The handshake publishes `ops`, `states`, `eval`
and the five figures from the executor's first load, the two instruction
figures provisional, and nothing is served before it is declared. No `state`
is `bad-request`: a state the executor does not reach is `invalid-state`, and
`missionscripting` without a mission is `no-mission`, the maintainer's call
of 2026-09-11.

**Shipped unmeasured, and why nothing measured it.**

- *The hook guard's swallow path.* It has no seam: nothing short of a raising
  stub on the frame path exercises it, and putting one in a live DCS is a
  crash run for a path the load sentinel already covers.
- *Whether the export state survives between missions.* The load sentinel
  covers the case that matters, a second load; a survived state answers the
  same way.
- *A raise inside `net.dostring_in`, and whether every state has `setfenv`
  and `_G`.* The wrapper is proved under the suite's own carrier; the model's
  stubs evaluate nothing, and every reachable state answered `return 1` live
  (ADR 0031), which is as far as a measurement without a deliberate crash
  goes.
- *The carrier against the incumbent's measured shift and `...`.* T49
  measured its own carrier's round trip and not the incumbent's; the
  incumbent is uninstalled.
- *DCS's `os.time` against the harness's.* Its `os.clock` steps in whole
  milliseconds (ADR 0031), so `cpu_ms` is a resolution and not a cost, and
  `os.time` was not compared.
- *A count hook raising inside either carrier, or already held by a state.*
  ADR 0005 chose `none` and named a live `budget: none` from a state other
  than `mission` as what reopens it; no record holds the `budget` header any
  live state answered, so whether one held a hook is unmeasured too.
- *What `dcs.log` renders around a crossing's markers.* T49 crossed into
  `missionscripting` live (ADR 0031), so a real line was written, and no
  session read it; whether the rendering can be separated from the marker,
  which ADR 0007 names as what reopens the marker's spelling, is untested.
- *Which callback names DCS's hook dispatcher looks up, beyond the four that
  fired.* Nothing enumerates them, and no session probed a name the executor
  does not register.
- *Which of `write`, `close`, `rename` and `remove` answers a success with
  nothing.* The first live load found that DCS's `io` and `os` do (ADR
  0031), without saying which call, so the executor reads a bare nil from
  any of the four as a success and asks the disk where it matters; the empty
  output directory fits a write, a close, or a rename that did nothing, and
  the waking frame's heartbeat pays a stat for it.
- *`bridge.md` §11's remaining rows.* `require('socket')` in `hook` or
  `export` changes nothing §2 decided; the per-page cost of a walk belongs to
  the consuming project, which ships the walker; whether `lfs.dir` lists the
  temp transport turns on the live handshake's `transport_source`, which no
  session recorded; how promptly NTFS moves a directory's mtime through
  `lfs.attributes` was not timed, so the arm file stays; `net.dostring_in`
  from `missionscripting` is barred by the wire and was not asked; a
  multiplayer server's script-integrity check against the `dofile` line and
  the `mp-server` role were not exercised because no session was a client on
  a server or a dedicated server; the duplication figure changed nothing.
  `mcp.md` §8's five tier-1 reads on a joined client are the same gap: every
  figure is single player, as ADR 0031 says.

Alternatives: a Stage 10 to measure the list — rejected, because the items
that need a scene (a joined client, a dedicated server, a track) need one the
maintainer does not have, the items that need a deliberate crash run need one
nobody schedules, and the rest — `os.time`, the `budget` header, the
`dcs.log` line, which of the four calls answers nothing, the NTFS mtime — are
single-machine readings any session could have taken and none did, so a stage
with no one to run it is the carry this record replaces. Leaving the list in
`docs/STATE.md` —
rejected, because that file is loaded cold every session and this is a
durable fact about what shipped, not a handoff.

## Consequences

The project's done-condition is met: T51 passed at the maintainer's install
on 2026-09-22, and the dormant per-frame cost is below the baseline.
`docs/STATE.md` carries none of the above.

Every figure is one machine, one DCS build, single player. The first session
on a client joined to a server, on a dedicated server, or in a track replay
is a measurement this project never took, and a read that crashes there is
attributed by its chunkname (ADR 0031) rather than by anything measured
beforehand.

*Revisit if* a DCS update moves any figure ADR 0031 holds, or a scene above
is played with the executor installed and a read misbehaves there: the
answer is a new record, not an edit to this one.
