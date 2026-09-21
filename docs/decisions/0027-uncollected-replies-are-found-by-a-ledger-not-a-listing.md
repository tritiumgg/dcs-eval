# ADR 0027: Uncollected replies are found by a ledger, not a listing

## Status

Accepted

## Context

The specification bounds a session's reply directory with a sweep. §6.3's
table of what the executor writes unasked:

> | uncollected replies | `<session>/res/` | swept at 300 s while armed and once at disarm (§3.7); at most W of them between; gone with the session directory at the next load |

and §7.7's limits:

> | uncollected reply | swept after 300 s while armed, and once at disarm | a backstop for a requester that died, not a bin for one that is alive |

§3.7's disarm puts the sweep in its third step:

> Otherwise: run the sweep once, write the heartbeat with `armed: no` and `since` (§4.7), read `os.clock()`, set the next probe tick, and return to dormancy.

and §3.6 costs the prior implementation's sweep as a listing, and forbids it
on the dormant frame:

> | Every 2 s: `sweep_responses` — a second listing, and `lfs.attributes` on every entry | `:1925-1936` | as `list_dir` | 4–5 + one per entry |

> - **Zero** concatenations, **no** sort, **no** heartbeat, **no** sweep.

"At most W of them between" rests on a client that removes what it reads.
§2.2 counts one:

> ~10 filesystem operations (client write+rename; bridge list, open, read, remove, write, rename; client list, read, remove).

§8's library does not:

> `collect(id)` → reply or nothing; `wait(id)` → `reply | pending | superseded | dead` (§4.4)

and the build's does not either: the client library removes nothing it did
not write, and `dcs_collect` may read one id as often as it is asked. So
`res/` holds every reply a session published until the next load, and a
listing of it grows with the session.

## Decision

The executor keeps a ledger of the replies it published — each one's id and
the armed frame's wall-clock second — and every armed frame removes, oldest
first, those 300 s old or older, before it looks at the quiet window.

- Every armed frame sweeps, so the frame that disarms has swept like the rest;
  "once at disarm" is that frame's sweep and not a second one. It removes what
  is 300 s old, never everything: a reply `dcs_collect` has not yet picked up
  after a long mission load is the one the sweep must leave alone.
- A dormant frame never sweeps. A reply that ages past the limit while the
  executor sleeps goes on the next armed frame, or with the session directory
  at the next load.
- The removals are spent from the tick budget as requests are: read before
  each after the first, one always made, so a backlog shrinks across frames.
- A reply is stamped with the frame's own `os.time` reading, so the armed
  frame still reads the wall clock once (ADR 0009).
- A removal that fails is not retried: the next load removes the directory.
- The limit is a constant in the executor and not in the handshake; no client
  decides anything by it.

Alternatives:

- Listing `res/` with `lfs.attributes` every 2 s, as the prior did — its cost
  grows with every reply the session has published, because nothing else
  removes them: a W=8 run at a reply a frame holds some 17,000 at 300 s.
- Emptying `res/` at disarm — three seconds after the last reply, which
  destroys the replies `dcs_collect` exists to pick up.
- Having the client remove a reply once read — it breaks a second collect of
  one id, and does nothing for the client that died, which is the one case the
  sweep is for.

## Consequences

- `res/` holds up to 300 s of replies while armed, not "at most W"; the ledger
  holds the same number of entries in memory, two slots each.
- `dcs_collect` more than five minutes after a reply landed finds nothing,
  and cannot tell that from a reply that has not landed; its wording names
  both.
- The executor's bytes changed, so the embedded hash moved and the shipped
  list gained a line: an installed copy of the earlier executor is an upgrade.
- A wall clock stepped backwards holds the sweep at the head of the ledger
  until the clock passes it again; one stepped forwards removes replies early.
  Both follow from `os.time`, which ADR 0008 chose for the quiet window.
- A reply is stamped with the time its frame began, the one reading ADR 0009
  allows an armed frame, so one answered after a long dispatch is kept for
  300 s less that dispatch's duration.
- Two replies under one id — only a hand-dropped request can reuse one — share
  a name on the disk, and the first entry's removal takes the second's file
  early.
- Reopen this if a client ever removes what it reads: the bound becomes W
  again, and the ledger is then a cost the directory no longer needs.
