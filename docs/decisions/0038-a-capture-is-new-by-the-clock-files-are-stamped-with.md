# ADR 0038: A capture is new by the clock files are stamped with

## Status

Accepted

## Context

`screenshot.md` §2.4, the second of the three things that must hold before a
capture is answered `ok`:

> **It is newer than the request.** Its modified time is at or after the clock
> reading taken *before* the request was published, so a file written while
> the request was still being published still counts as this capture's.

The specification does not say which clock. Rust's `SystemTime::now()` reads
`GetSystemTimePreciseAsFileTime`, which moves continuously. NTFS stamps a
write from the system time as `GetSystemTimeAsFileTime` gives it, which moves
only on the system timer's tick. The two are not the same reading of the same
instant: the precise one can be ahead of the coarse one by up to a tick, so a
file written just after a precise reading can be stamped *before* it.

A probe measured it on the maintainer's machine: a precise reading, then a
one-byte file written, then its modified time compared. It was run three
times, 1,000 writes a time, with 0.7 ms between writes. The file was dated
before the precise reading 1,000, 999 and 997 times, by at most 134 to 287
microseconds. Against a `GetSystemTimeAsFileTime` reading taken the same way,
it was never dated before it: 0 in each run.

## Decision

The reading the newness test compares against is taken from
`GetSystemTimeAsFileTime`, through `sys::file_clock_now`, the clock the
filesystem stamps writes from. It is taken before the request is published.

Alternatives:

- the precise clock less one timer tick: rejected, because the tick is a
  setting and not a constant, and the margin would be a guess;
- the `.req` file's own modified time: rejected, because it is stamped while
  the request is being published rather than before it, and the executor
  deletes the file once it has read it;
- the precise clock as it is: rejected by the probe above. A file DCS wrote in
  the same tick as the reading would be judged last week's.

## Consequences

The newness test and the timestamp it compares against come from the same
clock, so a write in the same tick as the reading counts, as §2.4 asks.

`sys` carries one more Win32 declaration. The unit test beside it writes 200
files and requires none to be dated before a reading taken just ahead of it.
Swapping the precise clock in reddens that test on every run. It does not
redden `a_file_written_while_the_request_is_published_counts` in `screenshot`:
publishing takes long enough that a file written after it lands a tick or
more past either reading. That test holds the order, reading before
publishing. The `sys` test holds the clock.

*Revisit if* a capture on a live install is ever answered `not-written` while
a file under its name is dated within a tick of the request. That would mean
DCS's writes are stamped some other way than the probe's were.
