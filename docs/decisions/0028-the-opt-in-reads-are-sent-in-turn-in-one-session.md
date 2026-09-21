# ADR 0028: The opt-in reads are sent in turn in one session, and one that stops the run is retested alone

## Status

Accepted

## Context

`mcp.md` §3.3 says how the tier-2 reads are proved before they are enabled:

> They are enabled only after the first live run has sent each alone
> under the probe supervisor (`bridge.md` §4.6), one per session, and the result is a row in this
> table.

ADR 0023 carried that rule to all seven opt-in reads, and `dcs-mcp live read
<key>` enforces it: a second read in one DCS session is refused. Seven reads
therefore cost seven DCS launches.

"One per session" buys two things. A crash names its read, because the
session held nothing else. And nothing earlier in the session can have set the
crash up — a read that damaged state without crashing, whose damage surfaced
on the next call.

The first of those does not need a fresh session. Each read is published
alone, as the only request on the disk; the ledger writes it down as sent
before it is published; the executor's events log opens a marker for every
request and closes it only when the request finishes. A read in flight when
DCS dies is named three ways over, whatever came before it in the session.

Only the second — delayed damage — needs the restart, and it can be paid for
where it is needed rather than seven times over. The maintainer asked for the
cheaper sequence on 2026-09-21, to finish the live run in one sitting.

## Decision

`dcs-mcp live read all` sends every opt-in read that has no outcome in the
ledger yet, one after another in one session, each alone on the disk and each
written down before it is sent and after it answers. The order is the table's:
tier 2's four, then the three from the crashing batch.

It stops at the first read that comes to anything but an answer, a raise
inside its `pcall` or a reply in a shape nobody expected — the session gone,
superseded or still pending when the wait runs out, and also a reply refused
by the executor or one the client could not carry. A read sent after any of
those would be sent into a game nobody can vouch for, and stopping on the ones
that turn out harmless costs a restart, not a wrong row. A read that raises or
answers malformed answered: the session is up, and the next is sent.

The read that stopped the run is then retested alone with `live read <key>`
in a fresh session, which is §3.3's rule applied to the one read the sequence
cannot clear by itself. Its latest entry is its row. `live read all` in the
session after that skips every read with an outcome and carries on.

`live read <key>` is unchanged: one read, alone, in a session that has had
none. Both ways in are refused in a session that already had a read.

Rejected: dropping the stop and sending all seven regardless. A read sent
after the session stopped answering proves nothing and may be the thing that
takes the game down.

Rejected: keeping seven launches. The ledger and the events log already
attribute a crash to the read in flight; the launches were paying for delayed
damage on every read, including the six that will most likely answer.

## Consequences

The expected live run is one launch for all seven reads. A read that stops it
costs two more: the retest alone, and the session that finishes the rest.

A read that answered in the sequence but damaged something that surfaced on
a later read is not cleared by this: the later read takes the blame and is
retested alone, and if it then answers, the crash is left unexplained rather
than misattributed. That is the case the retest exists to expose. The report
shows only the latest entry per row, so it shows the retest's clean answer;
`live.jsonl` keeps both, and is where to look whenever a retest answers.

A read in a slow scene — a mission still loading — can come back pending
within the default wait and stop the run needlessly. That costs a restart,
not a wrong row.

*Revisit if* a retest alone answers where the same read stopped the sequence.
That is the delayed-damage case this record accepts, and the honest response is
to run the reads before it one per session until the culprit is found.
