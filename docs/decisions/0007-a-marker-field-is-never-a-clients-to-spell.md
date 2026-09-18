# ADR 0007: A marker field is never a client's to spell, and the crossing is marked without a clock

## Status

Accepted

## Context

A request that kills DCS leaves no reply, so the only record of which one it
was is what the executor wrote before it ran. `bridge.md` §4.5 gives that
record its grammar:

> The bridge appends to `<output>/events.log` before and after every request
> it handles:
>
> ```
> B|<id>|<op>|<state>|<stamp>
> O|<id>|<status>|<cpu_ms>
> ```
>
> This is the probe progress file's grammar
> (`prior:pipeline/src/probe/capture.ts:1-6`), so the supervisor's reader
> (`Get-CrasherLabel`) names a *bridge* request that killed DCS the same way
> it names a probe call — the last `B|` with no `O|`. Today a request that
> kills the process leaves no record of which it was, because the request
> file is deleted before the chunk runs (correctly) and the events log
> records only raises. The `missionscripting` door adds its own markers
> through `log.write` inside `mission`, which has no `io` (§5.3).

Three of the four fields of a `B|` line are bytes a client wrote. `id` is a
filename, `op` and `state` are request headers, and §7.7 lets a request be
262,144 bytes: an `op` alone may be a quarter of a megabyte, and neither
header is bounded by anything the envelope checks. The reader splits a record
on `|` and on the line, so a client that spells either byte inside a header
writes a record of its own choosing into the file that will name its killer,
and a client that spells a long one writes a file §6.3 bounds only by launch:

> rotated at load: `events.log` becomes `events.prev.log`, one generation
> kept, so the ceiling is two sessions of activity. A dormant session writes
> its load banner and its phase changes — some tens of lines per launch, none
> per frame

The markers of the crossing are named and not spelled. The same table places
them outside the file — "`dcs.log` door markers | DCS's own log | only when a
`missionscripting` request runs (§5.2); a dormant bridge writes none" — and
the state they are written from is sanitised: §5.3's carrier runs inside
`mission`, which has no `io`, and no `os` either, so there is no clock in it
to charge a `cpu_ms` from.

§7.2 lists the events log's contents wider than its markers: "load, refusals,
raises, phase changes, arm and disarm, and the `B|`/`O|` markers of §4.5".

## Decision

Every field of a marker that a client could spell is held to the length and
the alphabet the grammar can carry, and the crossing into `missionscripting`
is marked in the same grammar with an empty `cpu_ms`.

- The tick writes `B|<id>|<op>|<state>|<stamp>` before it dispatches a
  request and `O|<id>|<status>|<cpu_ms>` after. `state` is empty for an op
  that names none, which is `ping`.
- `id`, `op` and `state` each go through the 80-byte excerpt every refusal
  message already keeps, and every byte in them that is not printable ASCII,
  the separator included, is written `?`. `stamp`, `status` and `cpu_ms` are
  the executor's own words and are written as they are.
- `status` and `cpu_ms` in a closing marker are the figures the reply
  carried, taken from the same charge rather than from a second reading of
  the clock, so a record and the reply beside it never disagree.
- The pair brackets the dispatch alone. A request refused by `admit` — a bad
  envelope, a foreign stamp, no op, an empty body — never reaches an op and
  cannot be a killer, and its refusal is already on the wire as a reply.
- The `missionscripting` carrier writes `B|` before it looks for
  `a_do_script` and `O|<id>|<status>|` after the crossing answers, through
  `log.write` at `INFO` under the executor's name, with the same four opening
  fields and an empty `cpu_ms`.
- Every other line of the events log opens with a field that is neither `B`
  nor `O` — the load banner is `load|<stamp>|<host>|<protocol>` — so a reader
  keyed on the two markers passes over the rest of the file.

The alternatives:

- **Echo the fields whole, as a `stale-session` reply echoes `for` (ADR
  0006).** That echo is a reply to the client that wrote the bytes, read by
  that client; this is a record read by a supervisor after the process died,
  and a 262,000-byte `op` would be a line no reader can use in a file whose
  ceiling is two launches.
- **Write the id alone and drop the two headers.** The id is a filename the
  client chose; the supervisor's question is which call killed DCS, and the
  op and the state are the answer to it.
- **Charge the closing marker from a fresh clock read.** Two figures for one
  request, differing by the publish of the reply between them.
- **Read a clock inside `mission` for the crossing's own `cpu_ms`.**
  `mission` is sanitised and has no `os`; the tick's own pair already charges
  the crossing.

## Consequences

The grammar now has a byte no field may carry, and every field added to a
marker later goes through the same sanitisation or the reader is back where
this record found it. A killer whose op is longer than 80 bytes is named by
its first 80 and three dots, and so is its id: a supervisor matching a
marker against the request it sent matches on an id cut the same way, so
two requests whose names share their first 80 bytes share one marker id
and the record cannot tell them apart. Names kept distinct inside 80 bytes
are the supervisor's own to choose, as the id itself is.

A request refused before dispatch leaves no pair. That holds only while
nothing refused ever runs, compiles or crosses into a state; a refusal that
gains any of those needs its own marker, and this record is the place that
says so.

A kill inside `missionscripting` leaves two unmatched `B|` records, one in
`events.log` and one in `dcs.log`, naming the same id. A reader of either
names the same request, and a reader of both learns whether the crossing had
begun.

The file is narrower than §7.2's row for it. It carries the load banner and
these markers; the refusals, raises, phase changes and arm and disarm that
the row also lists are not in it. A refusal is already a reply the client
reads and a raise is already counted on the namespace, and neither is the
question a supervisor brings to a dead session, which is what was running.
Arming, disarming and the phase changes they move belong to Stage 5, which
builds them and the heartbeat that reports them; each goes in beside the
markers when it exists, as a line whose first field is neither `B` nor `O`.

A crossing's closing status is the only field in either marker that is
neither cut nor sanitised: it is the first line of what the near chunk
answered, which is one of the words this executor spells there. A DCS build
whose `a_do_script` hands back a string in some other shape — the case the
near chunk already answers `stage: a_do_script` for — would put that line
into `dcs.log` whole, and is what would put this field through `field` with
the rest.

What `log.write` renders into `dcs.log` around the message is DCS's, and no
harness can show it: the executor's half of the crossing's markers is proved
under a model that records the message alone. Stage 9 is where a real
`dcs.log` line is read, and a rendering a marker cannot be separated out of
reopens how that half is spelt — not the grammar, which is the events log's.
