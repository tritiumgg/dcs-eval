# ADR 0010: The heartbeat carries what the session already keeps

## Status

Accepted

## Context

§7.2's table names the heartbeat's contents:

> | heartbeat | `<output>/heartbeat.txt`, every 2 s **while armed**, and at every
> arm, disarm and phase change (§4.7) | bridge | `stamp`, `phase`, `armed`
> (`yes`/`no`), `since` (local wall clock of the last arm or disarm), `ticks`,
> `answered`, `queued` (requests seen and not yet answered), `last_tick_at`
> (local wall clock), `busy` (the id being handled, when one is),
> `last_callback` (`<name>@<tick>` of the last callback other than
> `onSimulationFrame` to fire, §7.4), `dormant_cpu_ms_per_1000_ticks` (§3.5,
> from the last dormant period) |

And §8 says the client reads three of those back out:

> - `status()` — reads the handshake and heartbeat, checks the PID, reports
>   phase, `armed` and `since`, heartbeat age (qualified as meaningless while
>   `armed: no`, §4.7), ticks, answered, queued, `busy`, the transport, whether
>   the arm file exists, the running `app_version` against the model's, whether
>   `lfs.tempdir()` agreed with `os.tmpdir()`, and every problem it found

Four of the table's fields name figures this build's session does not keep.

## Decision

The heartbeat carries `protocol`, `host`, `stamp`, `transport`, `phase`,
`armed`, `since`, `ticks`, `last_callback` and `callbacks`, and nothing else.

Why each of the four is absent:

- `answered` — the session keeps no count of requests answered. It counts the
  replies it could *not* publish and nothing else, and a figure invented at
  write time is worse than an absent one.
- `queued` — a depth is a second listing of `req/`, made on the beat rather
  than on the frame, and the frame already listed.
- `busy` — the id being handled is a field that would have to be kept across a
  dispatch, which is the path a killing chunk runs through. The events log's
  `B|` with no `O|` already names that request, and it names it from the disk,
  where a crash cannot take it back.
- `dormant_cpu_ms_per_1000_ticks` — a measurement that has not been taken
  inside DCS. It belongs to the row that takes it.

`last_tick_at` is absent too, and for a different reason: it is `ticks`
alongside the file's own modification time, which a client reading the file
already has.

Three fields the table does not name are present. `protocol` is §7.1's rule
for this file — "`protocol: 2` in the handshake, the heartbeat and every
reply". `host` and `transport` are what let a client see two installs writing
into one output, which §8 lists among the problems `status()` reports.
`callbacks` is already the session's own record, published on the namespace
and answered by `ping`, and costs one `table.concat` on a path that runs a few
times a minute at most.

## Consequences

`status()` reports what it has and says nothing it cannot see. A client written
against §7.2's full table would find four fields missing; the client half of
this build is written against this record instead.

A later row that starts counting answers, or that takes the dormant figure, can
add its field without moving anything here: the file is an envelope of headers
and a reader takes the ones it knows.

What would reopen this: a row that needs a queue depth more than it needs a
frame that does not list twice.
