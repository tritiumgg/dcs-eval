# ADR 0031: The reads that answered live go on by default, `getMissionLoaded` goes, and the editor is read from its map

## Status

Accepted

## Context

Stage 9's live run finished on 2026-09-21 on the maintainer's machine: DCS
2.9.29.27468, variant `DCS`, one install. A probe answer is a record, so the
figures are here, and so are the four decisions the maintainer took on them.

`mcp.md` §3.3 held seven reads back until a live run had sent them:

> **Tier 2, off by default, `--reads extra`:** `DCS.isMultiplayer()`, `DCS.isServer()`,
> `DCS.isTrackPlaying()`, `net.get_my_player_id()`. […] They are enabled only after the first
> live run has sent each alone under the probe supervisor (`bridge.md` §4.6), one per session, and
> the result is a row in this table.

> **Never:** `DCS.getMissionLoaded()`, a named suspect in a hook-state crash
> , and `getPlayerUnitType` and `getMissionTheatre`, which
> were in the crashing batch.

ADR 0023 made all seven opt-in, by group or by key, through `game-state
--reads` and `dcs_game_state`'s `reads`, and said what would reopen it: "a live
crash is attributed to one of the seven by its chunkname". ADR 0028 said how
the run would send them: in turn in one session, each alone on the disk, and a
read that stopped the run retested alone in a fresh one. That method stands;
this records what it found.

§3.6 left the editor open:

> **The editor.** Three candidates, none measured. […] (c) A read in the `gui` state —
> `me_editorManager` and its neighbours are `gui` globals by the census — which is a chunk under
> D37's rules and safe to try. Until one of them is measured, `menu-or-editor` stands.

and §7 reported a lead about the mission's name:

> A mission flown from
> the editor is reported by an unverified lead as carrying the *name* `tempMission` — a value that
> says "flown from the editor", not "in the editor", and is recorded as such.

### What was measured

**The dormant frame** (`live dormant --label menu`, median of 5). Installed
and dormant, 0.0011 ms a frame against the incumbent's 0.098 ms; the empty-loop
floor the figure is corrected by, 0.058 µs, was capped before DCS's clock
moved, so 0.0011 is an upper bound. Armed and idle, 0.0290 ms a frame. One
directory listing cost 0.0290 ms on this machine where the incumbent's
measurement gave 0.098; a stat of a present file 13.936 µs, of an absent one
8.666 µs.

**Round trips**, 200 sequential `return 1` per state. At the menu, hook host,
p50/p95 in ms: `hook` 13.8/14.8, `gui` 13.9/14.7, `scripting` 13.9/14.7,
`mission` 13.9/14.7, `config` 13.9/14.8; 385–412 replies a second, 5.4–5.7
replies a tick at W=8. `export` answered `invalid-state` and
`missionscripting` `no-mission` at the menu, as they should. In a mission,
hook host: `hook` 13.8/16.4, `gui` 14.0/17.0, `scripting` 13.9/17.0,
`mission` 13.8/16.8, `config` 13.9/16.6, `export` 13.8/15.9,
`missionscripting` 13.9/15.8; 300–367 replies a second. The export host's own
state, 14.0/15.3 at 318.5 replies a second. The baselines were the Node
client's 30 ms p50 and 37 ms p95, and 14 ms median measured in Lua. `cpu_ms`
per reply was 0 or 1 at p50 and 1 at p95: DCS's `os.clock` steps in whole
milliseconds, so that is the clock's resolution and not a cost. A seven-state
generation projected from `return 1` in the hook state alone comes to 33–42 s
against 465 s; the two are not comparable, and a consumer's real generation
is unmeasured.

**`missionscripting`** answered through `a_do_script` in a mission, p50
13.9 ms.

**The seven reads**, each sent alone on the disk in turn (ADR 0028), in a
single-player mission and then at the menu:

| read | mission | menu |
|---|---|---|
| `multiplayer` | false | false |
| `server` | true | true |
| `track` | false | false |
| `player_id` | 0 | 0 |
| `player_unit_type` | `F-4E-45MC` | nil |
| `mission_theatre` | `Afghanistan` | nil |
| `mission_loaded` | crashed DCS, twice | nil |

DCS counts single player as hosting: `server` is true in both scenes.
`mission_loaded` crashed DCS in a mission in the sequence and again sent alone
in a fresh session, ADR 0028's retest: an access violation, C0000005, in
`lua.dll` (`luaS_newlstr`, then `lua_pushnil`) under edCore's
`ED_lua_copyindex`, with the executor's events log ending on the read's open
marker. `getMissionLoaded` is not in the install's `API/Sim_ControlAPI.md`,
which documents `getCurrentMission()` as returning "table with the currently
loaded mission", and nothing in ED's own Lua in the install calls it. The crash
is in ED's copy of a table from one state into another, which fits it
returning the mission table.

**The scenes**, measured by hand with `dcs-mcp eval`.
`DCS.getSimulatorMode()` is 1 at the menu and in the editor and 4 in a
mission. `MapWindow.getVisible()` in the `gui` state — a plain Lua getter of the
editor's map window, `MissionEditor/modules/me_map_window.lua` line 4616 in the
install — is true in the mission editor and false at the menu and in a
mission. `getMissionName()` and `getMissionFilename()` are both the empty
string at the menu and in the editor, even with a mission open there. In a
single-player mission started normally, not from the editor,
`getMissionName()` gave `tempMission` and `getMissionFilename()` the real
path, `C:/Users/tritiumgg/Saved Games/DCS/Missions/TESTING/CLEAN//Caucuses_Empty_UH-1H.miz`,
DCS's own doubled slash included. `tempMission` is a single-player bug ED has
acknowledged since about May 2025
(<https://forum.dcs.world/topic/373021-dcsgetmissionname-is-now-borked/>);
the name is meant to come from the mission file's `sortie` field, the
editor's briefing name (<https://wiki.hoggitworld.com/view/Miz_mission_structure>),
which is often left blank. So the lead §7 quotes is wrong: `tempMission` does
not mean the mission was flown from the editor. No callback fired at the menu
or in the editor — `onShowMainInterface` did not, even on the way back from
the editor. In a mission, `onMissionLoadBegin`, `onMissionLoadEnd`,
`onSimulationStart` and `onSimulationResume` fired.

The run found two faults and they were fixed before it went on: DCS's `io`
and `os` answer a success with nothing, and DCS's `lfs.tempdir()` is
`%TEMP%\DCS` (ADR 0029). A third, a heartbeat the last session left read as a
problem after a relaunch, was fixed on a branch of its own (ADR 0030).

## Decision

The six reads that answered go on by default, `getMissionLoaded` leaves the
table, and the editor is told from the menu by `MapWindow.getVisible()` read in
the `gui` state. The maintainer's four calls, 2026-09-21:

- `mission_loaded` is removed from the reads. It is not refused by name and is
  on no list of what may not be sent: it is simply not a game-state read, so
  the allowlist refuses it as it refuses any other unlisted call, and an agent
  that wants it calls it deliberately with `dcs_eval`.
- `multiplayer`, `server`, `track`, `player_id`, `player_unit_type` and
  `mission_theatre` are sent by every game-state. With nothing left to opt
  into, the switch goes: no `--reads` on `game-state`, no `reads` on
  `dcs_game_state`, no groups, and no `unknown (tier 2 off)`.
- The editor is read from `MapWindow.getVisible()` in `gui`: true is
  `editor`, false is `menu`, while the phase is `menu`. `menu-or-editor` is
  kept for the one case where the read cannot be made — the `gui` state
  refusing it, as it does on a client joined to a server.
- The s17 flag-agreement fixture is dropped: `a_do_script` already answered
  live, and nothing depends on s17.

Three things follow from the figures without a choice between options:

- The session is read from `multiplayer` first. False is `single player`
  whatever `server` says, because DCS answers `server` true in single player;
  true with `server` true is `hosting`. `single-or-host` goes with the switch
  that made it necessary.
- `mission_name` that says `tempMission` is not presented as the mission's
  name. The mission is identified by `mission_file` where it answered, with
  DCS's doubled slashes collapsed for display, and the answer says the name
  was DCS's single-player placeholder.
- `player_unit_type` and `mission_theatre` join the mission's line where they
  answered a string, and stay on their own lines beside it.

What ADR 0023 decided about the switch is replaced whole, so it is superseded.
Two of its choices survive and are restated here: nothing is refused by name,
and every read no summary is made of is printed on a line of its own.

Rejected:

- keeping the switch with nothing behind it: machinery kept for its own sake;
- keeping `mission_loaded` opt-in: it took DCS down twice, and a game-state
  read is one an agent sends without thinking about it;
- `getSimulatorMode()` for the editor: 1 at the menu and in the editor alike;
- a callback for the editor: none fired at the menu or in the editor.

## Consequences

Every `dcs_game_state` publishes eleven reads in the hook state, one in `gui`,
a ping and the reachability probe: fourteen requests in one window, answered
in one tick as the five were.

The six were measured on one machine, one DCS build, single player, at the
menu and in one mission. Nothing measured them on a client joined to a server,
in a track replay, or in multiplayer as host. A crash there is attributed by
the chunkname each read still carries, `=dcs-eval read <callee>`.

`MapWindow` is the mission editor's own module, not an API. A DCS update that
renames it, or loads it later, turns the editor read into a raise, and the
answer at the menu becomes `unknown` naming it rather than a guess.

`live` is left sending six reads, because the next task removes it.

*Revisit if* a crash is attributed to one of the six by its chunkname, which
would take that read out of the table the way `mission_loaded` went; or if ED
fixes `getMissionName()` in single player, which would make `tempMission` an
ordinary name again; or if a build answers `server` false in single player.
