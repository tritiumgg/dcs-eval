# ADR 0025: The dormant cost is measured by its own operations, inside DCS

## Status

Accepted

## Context

`bridge.md` §3.5 asks the first live run for the dormant cost, two ways:

> It measures a fourth term, the one the maintainer's requirement is actually about: **the
> dormant cost inside DCS**. `cpu_ms` cannot carry it — a dormant frame is below `os.clock`'s
> resolution — so it is measured two ways. In the bridge, `ping` reports
> `dormant_cpu_ms_per_1000_ticks`: the bridge reads `os.clock()` once at each disarm and once at
> the next arm, and divides by the ticks between, which costs two clock reads per transition and
> nothing per frame. Outside the bridge, DCS's own frame-time counter over one fixed scene, three
> ways — hook file absent, hook installed and dormant, hook installed and armed with an idle
> client — because "no noticeable performance impact" is a statement about frame time and only
> frame time answers it. A dormant figure above today's 0.098 ms per frame reopens §2 (§2.3).

The baseline is §3.6's:

> At 57 Hz that is 285 heap allocations and roughly 260 kernel entries per second, in perpetuity,
> in the menu and in the cockpit alike, while nothing is or will be asked. S4 measured the listing
> at 0.098 ms per tick, 0.59% of a frame. The maintainer's observation that this is not noticeable
> is consistent with that figure; it is also the baseline this section must hold.

§2's comparison table gives the same figure as the idle cost per look:

> today: a directory listing — open, read, read, close, five heap allocations, a sort (§3.6;
> 0.098 ms measured in DCS, S4).

§2.3's reopen condition names it:

> **Reopen condition.** A measurement showing the per-round-trip filesystem cost above 5 ms at p50
> on the target machine, or DCS exposing a Lua execution context off the simulation thread, or
> (§2.4) a dormant frame measured above today's 0.098 ms. None is an argument; all three are
> numbers.

And §11 lists the one operation the restructured frame adds as unmeasured:

> - **The cost of one `lfs.attributes` inside DCS.** S4 measured the listing (0.098 ms) and
>   nothing else; the stat that replaces it is unmeasured in DCS.

ADR 0010 left `dormant_cpu_ms_per_1000_ticks` off the heartbeat, "to the row
that takes it". The executor's own note on `cpu_ms` says what that figure
would be made of: `os.clock` under the Microsoft C runtime is elapsed time,
stepping in whole milliseconds, not CPU time.

## Decision

The dormant cost is measured in process, inside DCS, as the cost of the
dormant frame's own operations. `dcs-mcp live dormant` sends one chunk to the
host's local state, five times, each request alone. The chunk times five
operations, each repeated until `os.clock` has moved 50 ms or a million
iterations have run:

- `lfs.attributes(path, "mode")` on a path beside the arm file that does not
  exist — the executor's own call with its own second argument, and what a
  dormant frame stats (the executor is armed while the chunk runs, so the arm
  file itself is present, and its absent sibling is the like-for-like path);
- the same on the arm file, which is present;
- a full `lfs.dir` of the empty request directory, which is the armed-idle
  frame's listing and exactly the operation S4 measured at 0.098 ms;
- `pcall` of a function that increments a table field, a model of the floor
  every frame pays for the callback wrapper and the tick counter;
- an empty loop, whose per-iteration cost is subtracted from the other four.

`live dormant` takes the median of the five requests and derives:

- installed-dormant per frame = wrapper floor + absent stat ÷ `probe_every`,
  the latter read off the handshake;
- armed-idle per frame = the listing alone (the armed frame's `os.time`,
  `os.clock` and two table constructions are not timed, and the row says so);
- the incumbent's listing on this machine = the same listing, set against
  0.098 ms, which makes the comparison like-for-like on this hardware;
- hook absent = 0 by construction, printed as not a measurement.

The whole-frame three-way is not taken by this instrument. It stays in the
report as a by-hand row, so the axis §3.5 asks for is visibly untaken rather
than dropped.

`dormant_cpu_ms_per_1000_ticks` is not built. ADR 0010's open end closes here.

Rejected:

- DCS's whole-frame counter, three ways, as this instrument's figure: the
  expected difference (about 0.01 ms a frame, an estimate) is two orders of
  magnitude below frame-time jitter, "hook absent" needs another DCS launch,
  and a vsync or frame cap flattens all three readings.
- `dormant_cpu_ms_per_1000_ticks`: `os.clock` here is the wall clock in whole
  milliseconds, so the figure would be the frame interval and not the
  executor's share of it.
- A second, observing hook: another file in `Scripts\Hooks\`, with the same
  noise problem as the whole-frame counter.
- Calling the executor's `tick` in a loop: `tick` is not on the namespace, and
  exposing it would change the file under measurement.

## Consequences

The figure is a warm-cache lower bound. A stat repeated tens of thousands of
times is answered hotter than one made every eighth frame.

The wrapper figure is a model, `pcall` of a function doing the counter's work,
and not the executor's own registered callback.

DCS's own cost of entering a registered Lua callback every frame is invisible
in process. It is the one term by which installed-dormant can differ from hook
absent that this method cannot see. The by-hand whole-frame row and the
permanent-installation acceptance's "no noticeable frame impact" stand for it.

Each probe request holds one frame for about a quarter of a second, so the
phase is run at the menu or paused, and the README says so.

The chunk is sent with `max_instructions: 0`, unbounded, because a count
hook would sit on the very path being timed, which a dormant frame never has:
every figure would carry the hook's cost. The loops are bounded by their own
cap instead. With every loop run to its cap the chunk executes about 50
million instructions, at the executor's 50,000,000 ceiling rather than
clearly over it, so a bounded budget would have had to sit about there
anyway. That was counted on the reference interpreter with a count hook every
1,000 instructions and a clock that never moves: 48.5 million with
`lfs.attributes` stubbed as a C function, which is what DCS's `lfs` is, and
50.5 million with it stubbed in Lua. The figure moves with the stub, not with
the chunk. The cost is that a chunk this side got wrong could hold the game
for as long as that worst case takes. A request the executor refuses is
recorded as refused, not as a figure.

The measured figures land later as a probe-answer record of their own.

*Revisit if* the dormant figure lands within a factor of two of the
same-machine listing, or the maintainer reports a noticeable frame impact
during the permanent-installation acceptance. Either would justify an external
whole-frame timer.
