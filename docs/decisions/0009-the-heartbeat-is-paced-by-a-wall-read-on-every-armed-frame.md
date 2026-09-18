# ADR 0009: The heartbeat is paced by one wall read on every armed frame

## Status

Accepted

## Context

The heartbeat is the file a client reads to decide whether a session is alive
and ticking. §4.7 says when it is written:

> **The heartbeat becomes event-driven while dormant and periodic while armed.**
> It is written:
>
> - every 2 s while armed, as today, and this is the only periodic write the
>   bridge makes;

And §3.7 says which clock the armed path is already holding, in the sentence
that sets the quiet window:

> **Disarming.** An armed bridge that has seen no `.req` for `QUIET_S` seconds
> (default 3, measured with the `wall()` the armed path already reads every
> frame) disarms, in this order, because the order is what makes the race
> below unwinnable:

The two collide here. ADR 0008 settled that `wall()` does not exist in this
build — it was the incumbent's `pcall(DCS.getRealTime)`, a hook-host global —
and that `os.time` is the clock the quiet window is counted on. But it also
promised, in its Consequences, that the reading would be taken only on an
armed frame whose listing held nothing:

> It is read on an armed frame whose listing held no `.req`, and on no other
> frame: a frame with work to do clears the window without touching a clock,
> so the armed busy path gains no kernel entry and the idle one gains exactly
> one.

A 2 s cadence cannot be held under that promise. An executor answering a
request every frame would never read a clock, so it would never beat, and a
client reading its heartbeat would call a working session stalled. The
cadence needs a reading on every armed frame, which is the promise this
decision narrows.

## Decision

An armed frame reads `os.time` once, at the top of the armed path, and that
one reading serves both obligations: the beat's 2 s interval and the quiet
window. The dormant path still reads no clock at all.

The writer therefore takes the clock as an argument rather than reading one of
its own. Every caller — the arm, the disarm, a phase change, the periodic beat
— already holds a reading, and a writer that read its own would put the second
read back on the frame this decision exists to keep at one.

Alternatives, and the line that rejected each:

- Pacing off `os.clock`, which the armed frame already reads for the tick
  budget — process CPU time by the specification's own word, and whether
  MSVC's elapsed-time `clock()` is what DCS hands a Lua state is an unmeasured
  figure, so a liveness signal must not rest on it. ADR 0008 refused it for
  the quiet window on the same ground.
- Beating only on an idle frame, which would keep ADR 0008's promise whole — a
  continuously busy executor would then never beat, and a client reading the
  status table would call it stalled while it was working, which is the one
  reading the heartbeat exists to prevent.
- A frame counter instead of a clock — the frame rate is the thing being
  measured, as ADR 0008 said of the quiet period.

## Consequences

An armed frame costs one `time()` where before a busy one cost none. It is one
kernel entry against a full `lfs.dir` of `req/` on the same frame, which is the
company it keeps; the dormant path, which is the one measured to cost nothing,
is untouched.

`os.time` has whole-second granularity, so the interval is a floor: a beat
lands between 2 s and 3 s after the last one. A client judging staleness
against a 10 s threshold has room for that; a check that asserted 2 s exactly
would be asserting something the clock cannot promise.

ADR 0008 is not superseded. Its decision — that the quiet period is measured
by `os.time` — stands unchanged, and the same reading now serves both
obligations rather than being taken twice. What moved is the cost sentence in
its Consequences, and this record holds the reason.

What would reopen this: a measurement inside DCS showing `os.clock` is elapsed
time there rather than process CPU time. That is the same Stage 9 figure ADR
0008 named, and it would let both the beat and the window run off a clock the
frame already holds.
