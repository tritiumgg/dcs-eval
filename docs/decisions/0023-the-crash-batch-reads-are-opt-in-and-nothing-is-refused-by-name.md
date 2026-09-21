# ADR 0023: The crash-batch reads are opt-in, and nothing is refused by name

## Status

Superseded by [ADR 0031](0031-the-live-reads-go-on-by-default-and-getmissionloaded-goes.md)

## Context

`mcp.md` §3.3 sorts the game-state reads into three: five sent by default,
four more behind a switch, and three never sent at all.

> **Tier 2, off by default, `--reads extra`:** `DCS.isMultiplayer()`, `DCS.isServer()`,
> `DCS.isTrackPlaying()`, `net.get_my_player_id()`. Each is present in `hook` by the census
> (`prior:model/hook/DCS.yaml`, `prior:model/hook/net.yaml`) and called by ED only from the `gui`
> state — `MissionEditor/GameGUI.lua` and the multiplayer dialogs — so there is no hook-state
> precedent for any of them. They are enabled only after the first live run has sent each alone
> under the probe supervisor (`bridge.md` §4.6), one per session, and the result is a row in this
> table.

> **Never:** `DCS.getMissionLoaded()`, a named suspect in a hook-state crash
> , and `getPlayerUnitType` and `getMissionTheatre`, which
> were in the crashing batch. The list of reads is a constant in `dcs-eval`, and a chunk that is not
> in it is not a game-state read; an agent that wants one evaluates it as its own `dcs_eval`, under
> its own name.

(The gap after "crash" is a stripped citation in the frozen file, quoted as it
stands.) The same section says what the list is not:

> A crash this server causes is reported to the caller with the chunk that caused it; **this server
> keeps no catalogue of dangerous calls**, which is a modelling concern for whoever is measuring
> DCS.

§2.1 puts the switch on the server's command line:

> ```
> dcs-mcp serve   [--host hook|export] [--saved-games <dir>] [--variant <name>]
>                 [--install <dir>] [--allow <dir>]... [--reads base|extra] [--data-dir <dir>]
> ```

and §2.3 gives `dcs_game_state` two arguments:

> | `dcs_game_state` | one window | once | `host`, `wait_seconds` | §3.4's derived state, every fact it rests on, and the basis of each value |

ADR 0017 built both gates. Its paragraph on them begins "**It has two refusals
and not one.**": `NeverSent` checked every published byte against the three
bare names, so that a name promoted into the table would still be refused,
and `Unlisted` checked a read's callee against the table. Its `Consequences`
said "Tier 2 is built, off, and has no way to ask for it outside a test".

What changed is the maintainer's ruling, 2026-09-21: "I don't really think
anything should be blocked unless it's truly going to cause a crash." The
evidence behind the never gate is one crash, with `getMissionLoaded` the named
suspect and the other two only in the same batch. The next live run is meant
to send each of the seven — tier 2's four and the three — alone, one per
session, and nothing built could send any of them.

The packaged DCS model (v0.2.0, DCS 2.9.29.27278) places all three under `DCS.`
in the `hook` state, with arity `?`, `0` and `0`.

## Decision

The three become a third tier, `suspect`, opt-in exactly as tier 2 is: off by
default, and sent only when a caller names them. The never gate and
`Refused::NeverSent` are removed; the allowlist stays, so a callee the table
does not hold is still never sent. This overrides ADR 0017's two-refusal
paragraph and its sentence about tier 2 having no switch. The rest of 0017 —
the answer arms, the chunk grammar, the vet at the publication seam — stands,
so it is not marked superseded.

The selection is per call and one grammar serves both routes: a comma-separated
`--reads` on the `game-state` verb, and an array `reads` on `dcs_game_state`.
A word is `base` (nothing extra), `extra` (tier 2's four), `suspect` (the
three), or one read's key — `mission_loaded`, `player_unit_type` and
`mission_theatre` for the three. A tier-1 key is accepted and changes nothing,
because tier 1 is always sent. Any other word is refused naming it, and matching
is case-sensitive. There is no `serve --reads`.

An unasked read says which group it sits in: `unknown (tier 2 off)` or
`unknown (suspect reads off)`. And the game-state answer prints every read no
axis is made of, a line each after the headline, so an answer that was asked
for is seen.

Rejected:

- keeping the never gate: the maintainer's ruling above;
- one group for all seven: the live run needs each alone, and `--reads extra`
  would send a crash suspect without the caller saying so;
- `serve --reads`, as §2.1 has it: a server-wide selection changes only by
  restarting the registration, and the live run changes it every session;
- a way to switch tier 1 off, for a truly solitary read: not asked for, and
  `dcs_eval hook "return DCS.getMissionLoaded()"` already is one.

## Consequences

Nothing on this side now stands between a caller and a read that may crash DCS
except its being off by default and the caller's own choice. That is the risk
the maintainer accepted. The README says none of the seven is measured safe.

"Alone" means the only opt-in read in its window. The five tier-1 reads ride
the same window, each its own chunk under its own `pcall`. That shape and the
`=dcs-eval read <callee>` chunkname are what let a crash name the read that
caused it, and they are now load-bearing rather than tidy.

The default answer of `dcs_game_state` grows from one line to eight: the
headline, then seven reads, four of them `unknown (… off)`. The headline is
still first.

*Revisit if* a live crash is attributed to one of the seven by its chunkname.
That would justify refusing that one read again, recorded as a probe answer —
which is the one case the maintainer's rule says to block.
