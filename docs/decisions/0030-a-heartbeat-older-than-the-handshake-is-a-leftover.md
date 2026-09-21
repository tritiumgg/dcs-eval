# ADR 0030: A heartbeat older than the handshake is a leftover, not a problem

## Status

Accepted

## Context

`bridge.md` §8 lists what `status()` reports, retrieved with
`sh tools/spec.sh read BRIDGE 8`:

> every problem it found (a heartbeat from another stamp; a heartbeat naming
> another transport — two copies installed, or a stale file;
> `prior:pipeline/src/bridge/client.ts:325-367`)

So the specification counts a stale file as a problem, alongside two copies
installed. §4.7 settles when the heartbeat is written, retrieved with
`sh tools/spec.sh read BRIDGE 4.7`:

> - every 2 s while armed, as today, and this is the only periodic write the
>   bridge makes;
> - at every arm and every disarm, carrying `armed: yes|no` and `since`, the
>   wall-clock time of that transition;
> - on every phase change, dormant or not, as today (…)

A load is none of those, and the executor writes nothing at load. The file
lives at `<output>\heartbeat.txt`, one name per host that every session
rewrites in place, so after a relaunch it is the last session's until the new
one first arms. ADR 0012 already decided that a wait reads another stamp's
heartbeat as the dormant branch and not as evidence. `status` did not: it
reported the stamp and the transport as two problems.

The live run on 2026-09-21, DCS 2.9.29.27468, met exactly this. After a
relaunch and before any request, `dcs-mcp verify` printed:

```
session: 1790027086-25924 pid 25924, running
heartbeat: phase menu, 856s since it went quiet, which is when it stopped writing and not whether it lives
problem: the heartbeat is stamped 1790020166-15404, and this session is 1790027086-25924
problem: the heartbeat names the transport ...\hook\1790020166-15404, and this session's is ...\hook\1790027086-25924
not verified: 2 found
```

That is `not verified` straight after every DCS start, on any machine that
has run the executor before. The first request fixes it.

## Decision

A heartbeat whose stamp is not the handshake's is a **leftover** when its file
was last written strictly before the handshake's. It is reported as the
stamp that left it and as no problem, and none of its fields are reported as
this session's heartbeat. The client reads the handshake's time off the same
handle as its bytes, just as it reads the heartbeat's.

A foreign heartbeat written at or after the handshake's time stays what it was:
`ForeignStamp`, with `ForeignHost` and `ForeignTransport` where they differ,
flagged `belongs: false`. So does one whose handshake time would not read. A
time that cannot be placed against the handshake gets no benefit of the doubt.

The line falls where the evidence changes. A file untouched since before this
session loaded says nothing about any writer during this session. A file
written since does: some other executor is writing into this output.

Rejected:

- **The executor removes the old heartbeat at load.** It changes the embedded
  hash and forces a reinstall in the middle of the live run. It adds a
  load-time `os.remove` when nobody has yet measured which of DCS's `os` calls
  report success with no result (PR #80 met the first of them). And it hides
  nothing the client rule does not: a second executor that stays dormant is
  just as invisible either way. Every executor already shipped would still
  leave the file too, so the client needs the rule regardless.
- **Ordering by the stamp's own seconds.** It parses a spelling the client
  otherwise treats as opaque, and it misses an export state rebuilt inside
  one process, where the old stamp and the new one can share a pid.
- **A heartbeat whose stamp's pid has gone is a leftover.** It misses the
  same export case, where the pid is alive and the old session is gone, and
  it spends a second probe.
- **Reading every foreign heartbeat as a leftover.** That hides the second
  executor that §8's problem exists to report.

## Consequences

`verify` passes straight after a relaunch and names the leftover on its
heartbeat line, where it used to report two problems. `dcs_status` carries the
new `leftover` field in its report.

A second executor that loaded before this one and has not written since goes
unreported until it next writes. Only its heartbeat could show it, and it has
not written one. The next arm, disarm or phase change it makes is reported as
before. A probe of the pid in the leftover's stamp could narrow this: a live
pid that is not the handshake's would be a second process. That is not done.
It would read the pid out of a stamp the client otherwise treats as opaque,
and a reused pid would report a live stranger as a second executor.

The rule depends on one filesystem's clock stamping both files. The
executor's two writes and the client's reads all happen on one volume, so a
clock set backwards between loads is the one way it misleads. In that case a
leftover can read as written after the handshake and is reported as a
problem, which is the old behaviour.

`status` and `game` read the file separately. The game-state derivation still
reads a leftover's phase as unknown and names the foreign stamp as the reason.
That is true, and it is not a problem count, so it is left alone.

What would reopen this: an executor that writes a heartbeat at load, or that
removes the old one. Then a leftover would be a fault and not a normal state.
