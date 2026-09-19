# ADR 0013: A window that loses a reply slides past it

## Status

Accepted

## Context

`bridge.md` §3.3 describes pipelining as a window and says what the client
keeps and what the bridge answers:

> - The client library keeps up to **W** requests published (default 8, per
>   call configurable). A request id is `<seq>-<tag>` where `<seq>` is a
>   zero-padded 10-digit per-client counter and `<tag>` is a short random
>   string, so ids sort in publication order and never collide across two
>   clients sharing a session.
> - The bridge lists `req/`, sorts, and answers in that order until the tick
>   budget (§3.4) is spent; what it did not reach stays for the next tick.
>   Replies therefore arrive in request order.

and says what a session's death does to a window:

> - Pipelined requests share a tick, so **a request that kills DCS kills its
>   neighbours' replies too**. The events log marker (§4.5) names which
>   request was running; the neighbours are simply unanswered and the client
>   reports them `superseded` (§4.4) like any request in a dead session.

§4.4 is the table those words point at, and it names two terminal rows rather
than one:

> | `bridge.txt` stamp ≠ the session the request was sent to | `superseded` |
>   DCS restarted. The request lies in a directory the new session will never
>   list; it did not and will not run. A census session in the state is gone
>   with the process |
> | heartbeat age > 10 s, `pid` from the handshake **not running** | `dead` |
>   DCS is gone and no new session has started. The request did not run |
>
> `superseded` and `dead` are terminal: the request's id is returned so a
> caller can log it, and nothing will ever collect it. `pending` carries the id
> and the phase, as today, and is collectable later.

What the specification never says is what a *window* does when the head of it
gives an answer that is neither a reply nor a death. A single-request caller
that gets `pending` waits again or gives up, and that is the end of it; a
window has W-1 other requests in flight behind the one that went quiet, and an
unstated rule here is the difference between a driver that drains and one that
stops dead behind a request the executor never reached. Five such answers exist
— a head that ran out of time, a spec that could not be published at all, a
head that came back terminal, a read of the session that failed outright, and
a counter with no ten-digit `seq` left to name the next request with —
and none of them has a rule in the frozen text.

## Decision

A window is a window of *publication*, not of attention: nothing but a terminal
answer or an unreadable session stops the client publishing the next spec, and
the head of the line is yielded once whatever it has to say, then its slot is
refilled.

- **A head that ran out of time yields `pending` in its place and frees its
  slot.** Its id is gone from the window and the next spec is published. This
  is the reading §4.4 already licenses — a `pending` "is collectable later" and
  carries its id, so the caller has everything it needs to collect it itself —
  and the alternative, holding the slot until the reply lands, makes one
  request queued behind a spent tick budget stall every request behind it,
  which is the head-of-line blocking the window exists to avoid.
- **A spec that could not be published is yielded as an error in the refused
  id's place, and the slot refills.** The window is W *published* requests, and
  a spec the disk refused was never one of them; shrinking the window each time
  a publish fails would make a run of transient failures quietly serialise the
  whole drain.
- **A terminal head stops publication, and each id still in flight is
  collected once before it is reported.** §3.3's neighbours "are simply
  unanswered", but that is a claim about the tick that killed the process, not
  about what is already on the disk: a reply that landed in an earlier tick is
  a real answer to a real request and throwing it away to report a uniform
  death would lose an answer the executor gave. So each neighbour is collected
  once; what answers is yielded as a reply, and what does not is reported dead.
- **A neighbour carries the head's own terminal kind, not always
  `superseded`.** §3.3 says `superseded` because it is describing a restart,
  and a restart is what the §4.4 row it cites is about. The other terminal row
  is `dead`, and a window whose head came back `dead` has neighbours in exactly
  the same position — the process is gone — so reporting them `superseded`
  would tell the caller a new session had started when the client had seen no
  such thing. The rule is one line: whatever terminal variant the head
  returned, each neighbour gets it with its own id.
- **A session that will not read ends the drain.** A `wait` that fails is a
  failure to read the handshake or a reply, not a verdict about the request,
  and there is nothing to retry against: the same unreadable file will fail
  the same way for ever. The error is yielded once and the iterator returns
  nothing afterwards for good, so a caller looping over it terminates instead
  of receiving an unbounded stream of identical errors.
- **A counter with no id left is reported behind the window it already
  filled, once, and ends the drain.** A `<seq>` is ten digits and a client
  that has spent all ten thousand million of them can mint no name the
  executor would take; the specs still waiting have nowhere to go. But the
  requests already on the disk were asked for first and are owed their
  answers, so the window drains in front of the refusal, which is also the
  caller's own order — the spec that could not be minted sits behind them.
  The refusal is then yielded once and the iterator ends, for the reason an
  unreadable session ends it: nothing about a spent counter changes on a
  retry, and an error yielded without ending is a `for` loop that never
  returns.
- **What was never published comes back.** `unsent()` is how many specs the
  drain never reached and `into_unsent()` hands them back, because a caller
  that gave the window its specs by value cannot retry what it cannot get
  hold of, and retrying against a new session is the whole answer to a
  terminal outcome.

Alternatives rejected:

- *Hold the slot until the head's reply lands.* Head-of-line blocking, which is
  what pipelining is for.
- *Report every neighbour `superseded` per §3.3's wording.* Mints an outcome
  the session never justified when the head was `dead`.
- *Report neighbours without collecting them.* Discards replies already on the
  disk.
- *Report an exhausted counter the moment the mint fails.* The refusal jumps
  the window: requests already on the disk are never waited on, never yielded
  and their ids never reach the caller, which cannot collect what it cannot
  name.
- *Keep re-waiting a head whose `wait` errored.* Unbounded; each re-wait is
  another `upto` and nothing bounds the loop, so the caller gets a hang rather
  than an error.

## Consequences

A `pending` yielded in place means a caller can see the same request twice: the
client yields `pending` for id N and the caller may later collect N's reply
itself. That is deliberate — the id is in the outcome for exactly that — but it
means "one yield per spec" is the contract and "one *answer* per spec" is not.

The terminal path costs one `collect` per in-flight id at the moment the window
dies. That is W reads of a directory whose session is already gone, which is
cheap and bounded, but it does mean a terminal outcome is not reported
instantly.

The head-kind rule is argued here and proved only on the `superseded` side: the
crate has no fixture that produces a `dead` head with a window in flight, and
building one — a handshake naming an exited process, held across a drain —
would be a whole test for a one-line branch. The branch is on the terminal
variant rather than on the word, so the two paths are the same code; what is
unproved is that no other code path spells the word by hand.

What would reopen this: a measurement showing a live executor's tick budget
routinely leaves requests unreached for longer than a caller's `upto`. Then
`pending` in place stops being an edge case and becomes the ordinary path, and
the client may owe a caller a way to put an id back in the window rather than
handing it back to be collected by hand.
