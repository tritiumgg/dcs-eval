# ADR 0008: The quiet period is measured by `os.time`

## Status

Accepted

## Context

The armed path disarms after a period with no request in it. The
specification says which clock counts that period, in §3.7:

> **Disarming.** An armed bridge that has seen no `.req` for `QUIET_S` seconds
> (default 3, measured with the `wall()` the armed path already reads every
> frame) disarms, in this order, because the order is what makes the race
> below unwinnable:

The clock the armed path here actually reads every frame is `os.clock`, which
the tick budget is counted from, and which the specification itself describes
in §3.4:

> - The bridge measures each tick's work with `os.clock()` (present in both
>   hosts; process CPU time, adequate for an interval, D94) and stops taking new
>   requests once the tick has spent `TICK_BUDGET_MS` (default 8, of a 17.5 ms
>   frame).

So the two sentences do not name the same call. `wall()` is the incumbent's,
and §3.1's inventory of what was read out of it says what it was:

> | `wall()` → `pcall(DCS.getRealTime)` and the heartbeat interval test |
> `:1981-1982` | — | — |

That leaves a gap this build has to close itself. Process CPU time cannot carry
a quiet period: an idle armed executor spends roughly 16.8 ms of it over three
wall seconds, so a `QUIET_S` of 3 counted on `os.clock` would be minutes of
wall time, and a busy one would disarm sooner than an idle one. And
`DCS.getRealTime` is a hook-host global the export host does not have, while
this build drives both hosts through one frame, so reaching for it would mean
a second rule for the export host and a C call every frame besides.

## Decision

The quiet period is measured with `os.time()`.

It is the same call the session stamp is already built from, it is present in
both hosts, and it answers wall-clock seconds, which is what a quiet period is
counted in. It is read on an armed frame whose listing held no `.req`, and on
no other frame: a frame with work to do clears the window without touching a
clock, so the armed busy path gains no kernel entry and the idle one gains
exactly one.

Alternatives, and the line that rejected each:

- `DCS.getRealTime` through `pcall`, the incumbent's `wall()` — a hook-host
  global only, a C call per frame, and the export host would need a rule of
  its own.
- `os.clock`, the clock the armed path already reads — process CPU time, by
  the specification's own words, so three of its seconds are minutes of a
  quiet period.
- Counting frames instead of seconds — the frame rate is the thing the period
  is measured against, not a constant to divide by.

## Consequences

`os.time` has whole-second granularity, so an actual quiet period is between
`QUIET_S` - 1 and `QUIET_S` seconds: the constant is a floor on how long the
executor stays awake with nothing to do, not a promise about when it sleeps. A
system clock stepped backwards lengthens the one quiet period it lands in and
never shortens it below zero, because the window is a difference and the
comparison is `>=`. The figure the handshake publishes as `quiet_s` is
unchanged, and means what it always did.

What would reopen this: a measurement inside DCS showing `os.clock` is wall
time there rather than process CPU time — MSVC's `clock()` is — which would
make the specification's own sentence buildable as written and cost one kernel
entry fewer. That is a Stage 9 figure, and it reopens this record rather than
a comment in the code.
