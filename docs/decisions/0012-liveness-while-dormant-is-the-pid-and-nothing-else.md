# ADR 0012: Liveness while dormant is the PID, and four readings the table does not hold

## Status

Accepted

## Context

`bridge.md` §4.4 gives eight rows, retrieved with `sh tools/spec.sh read BRIDGE
4.4`. The `dead` row reads:

> | heartbeat age > 10 s, `pid` from the handshake **not running** | `dead` | DCS is gone and no new session has started. The request did not run |

and the section's own preamble says when the process is looked at at all:

> `wait` never returns an error on time alone. On every wake it reads two small
> files and, when they are stale, checks one process. The heartbeat's `armed`
> field (§4.7) decides whether its age means anything: a dormant bridge stops
> writing it by design, so age is evidence only while `armed: yes`.

Those two cannot both be read literally. Under `armed: no` there is no age that
means anything, so a dormant session three seconds into a silence, with its
process already gone, matches no row: the `dead` row asks for an age it may not
consult, and the `armed: no` rows all say `pid` running. §4.7 settles which way
that gap closes, retrieved with `sh tools/spec.sh read BRIDGE 4.7`:

> **Liveness while dormant is the PID and nothing else.** A heartbeat reading
> `armed: no` says the bridge chose silence; its age says when, not whether the
> process lives. `wait` and `status()` read `armed` before they read the age,
> and a stale heartbeat from a dormant bridge is the expected state, never a
> flag.

The other three readings are states the table does not name at all. §4.7 says
what a dormant session leaves on the disk — "`bridge.txt`, unchanged since
load; `heartbeat.txt`, last written at the last transition or phase change" —
but a session that has never armed has had no transition, and ADR 0010's
carry-forward records that the file is absent until the first arm. The process
probe is specified as an existence check and nothing more, "`process.kill(pid,
0)` or an equivalent existence probe", which says nothing about a probe that is
refused rather than answered; ADR 0011 chose the equivalent, and an
`OpenProcess` that is refused for access is a third answer neither document
anticipated. And the sent-at the `waking` row turns on is "the client's own
clock at the moment `send` returned", which a client that did not make the send
does not have.

## Decision

The client's outcome table is an ordered list, and four readings the frozen
table does not reach are decided here:

1. **A dormant session whose process is gone is `dead`,** at any heartbeat age.
   The `armed: no` branch consults the heartbeat's age for nothing, so the
   probe is what decides, and it decides first.
2. **A probe that could not decide never produces `dead`.** `Unknown` falls in
   with `Running`. Only a probe that positively says the process is gone may
   produce a terminal answer, because `dead` says the request did not and will
   not run.
3. **An absent heartbeat, and a heartbeat carrying another session's stamp,
   both read as the dormant branch** rather than as a refusal. A heartbeat that
   is there and will not parse is a refusal naming the file.
4. **A `Sent` this process did not mint is never `waking`.** Its age is no age
   at all, and every rule turning on "sent less than ten seconds ago" reads it
   as older than that.

Rejected:

- **Reading a dormant heartbeat's age as staleness**, which would make the
  dormant branch reachable only under ten seconds. It contradicts §4.7
  directly, and turns the expected state of a session that has been idle since
  the last mission change into a `stalled` flag.
- **`Unknown` as `dead`.** An access-denied handle is evidence about this
  process's rights, not about the other process's life; the id is somebody's,
  which is the opposite of gone.
- **An absent heartbeat as a refusal.** It is the state immediately after a
  load, where the session is dormant and has never armed (ADR 0010).
- **A heartbeat from another stamp as `superseded`.** `superseded` means the
  handshake's stamp changed. Two installs writing into one output directory is
  a different problem, one §8 lists among the things `status()` reports, and
  quietly answering `superseded` would hide it.
- **`waking` for an id a caller was handed.** It would be the client claiming
  it ensured an arm file it never touched.

## Consequences

The probe fires on every wake while a session is dormant, some hundreds of
times over a long wait, where the frozen reading would have fired it only once
the heartbeat went stale. That is the cost of reading `armed` first, and it is
accepted: §4.7 leaves the process id as the only liveness evidence a dormant
session offers, and an open, a zero-millisecond wait and a close cost the game
nothing at all — they never touch it. What they do cost is this client's own
wake, which is why the poll interval and not the probe is what bounds it.

The first two are a pair and only safe as one: `dead` is terminal, so reading 1
ends a wait that nothing will reopen, and reading 2 is the guard that keeps it
from ending on a probe that did not know. A probe generous with `Exited` plus a
branch that treats `Exited` as terminal would abandon live requests. Anything
that widens what `sys::liveness` calls `Exited` reopens this record.

Readings 3 and 4 make the dormant branch the catch-all, which means a client
defect that loses the heartbeat path entirely looks like a dormant session
rather than an error. `status()` is what distinguishes those, and it is not
built yet; until it is, a session reported `pending`/`stalled` with the phase
`unknown` is the shape that ambiguity takes.
