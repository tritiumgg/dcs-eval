# ADR 0016: The poll is the event's timeout, and the watch belongs to one wait

## Status

Accepted

## Context

The client's `wait` shipped over a fixed 25 ms poll so that the outcome table
could be built and proved before anything harder sat under it. The event-driven
watch now lands on top of it. Three things about how the two fit together are
not obvious, and each would be re-derived wrongly by somebody changing this
code later.

`bridge.md` §3.2, whole:

> The client's `wait` replaces its fixed sleep with a watch on the reply directory
> (`fs.watch`, which on Windows is `ReadDirectoryChangesW`), waking on any event and then listing the
> directory once. A poll at 25 ms remains as the fallback for a watch that reports nothing, because
> `fs.watch` is documented as unreliable on some filesystems and the failure would be silent. Expected
> effect: the 12.5 ms poll term disappears, and the round trip's p50 approaches one tick. Measured,
> not assumed: §3.5 requires the new client to report its own p50 against S4's 14 ms.

`mcp.md` §6's fourth bullet, whole:

> - **Hold no handle the bridge would trip over.** The reply watch (`ReadDirectoryChangesW` on
>   `res/`) is opened when a request is sent and closed when the reply arrives or the deadline
>   passes; it is never left open across tool calls, and never opened on a session `wait` has
>   reported `superseded`, because a directory with an open handle cannot be removed and the next
>   bridge session removes every sibling at load (`bridge.md` §4.2).

And the consequence §4.2 draws, quoted to the end of the sentence because the
continuation is half of what it says:

> - **At load, before anything else is written**, the bridge removes every sibling directory under
>   `<root>` whose name is not its own stamp — requests and replies together. Each is a session that
>   has ended, and nothing in it is addressed to this one. This replaces "clear responses, keep
>   requests" (D36, D37 §Rejected) and replaces the supervisor's `-ClearRequests` step, which is kept
>   as a no-op-safe command and is no longer load-bearing. A sibling that cannot be removed — on
>   Windows, a directory on which a client still holds a handle, which a client watching `res/`
>   through `ReadDirectoryChangesW` does — is logged and left, and the next load tries again. A
>   client therefore watches a session directory only while it has a request in flight there, and
>   never one it has learned is superseded (`mcp.md` §6).

ADR 0011 already settles that the six Win32 symbols are declared in this crate
rather than taken from a binding crate, and names `CancelIoEx` among them
because "a `wait` that times out must cancel the pending overlapped read before
its buffer goes out of scope". It does not say what the cancellation's answer is
for, which is the third thing below.

## Decision

**The poll is the event wait's timeout, not a branch taken when the watch
fails.** One sleep is computed per pass, `min(left, poll)`, and it is handed to
whichever mechanism is available: `WaitForSingleObject` on the completion event
where a watch is open and armed, `thread::sleep` where it is not. The loop
lists the reply directory afterwards whichever of the three things — an event, a
spent nap, a deadline — ended the sleep. There is no runtime detection of a
broken watch anywhere, deliberately: §3.2 keeps the poll on the grounds that the
failure is *silent*, so code that waited to notice would never notice. The only
seam is in the tests, which can ask for a wait that opens no watch at all and so
exercise the fallback alone.

**The watch is one wait's, and lives in that wait's own local.** There is no
public constructor for it, no parameter that hands one in, and no field on
`Session` holding one. "No handle is held once `wait` returns" is therefore a
property of where the value sits rather than a rule to be remembered, and it
falls out identically on every way out: a reply, a spent deadline, a terminal
outcome, a `?` on a file that would not read, and a panic. The handle is opened
lazily at the first sleep, so a wait whose first look answers `superseded` or
`dead` never opens one at all — which is exactly what §6 asks for and what §4.2
says the next session's sweep depends on.

**The drain's answer is recorded in a sink the watch owns, rather than
returned.** The cancellation runs in `Drop`, because `Drop` is the only place
that covers all of those exits; `Drop` has no caller to return a value to and
cannot reach the counters the wait is keeping. So the watch holds a shared cell,
writes what the drain saw into it, and a caller that wants the answer clones the
cell before the value is dropped. That is not bookkeeping for its own sake: the
corruption the cancellation prevents — the kernel writing into a buffer Rust has
reclaimed — is invisible to any test that could be written here, and a drain that
is recorded to have run and completed is the only evidence this design can
produce. Without the sink, deleting the cancellation reddens nothing at all.

Rejected:

- **The poll as an error path, entered when the watch reports a failure.** It
  reads better and it answers the wrong question: the failure §3.2 names is one
  that reports nothing rather than one that reports an error.
- **Cancelling at the return sites instead of in `Drop`.** There are four
  returns and an unbounded number of panic sites, and only the first set can be
  enumerated.
- **Returning the drain's answer from a method the wait calls before dropping
  the watch.** It covers the ordinary exits and misses the `?` and the panic,
  which are the exits the design exists for.
- **A `#[cfg(test)]` seam in place of the sink.** The cancellation is real
  behaviour on every path, and evidence that exists only under `cfg(test)` is
  evidence about a different program.

## Consequences

The watch reports a wake by ending a sleep, and nothing reads the notification
records it woke on. That is what keeps it small — no `FILE_NOTIFY_INFORMATION`
walk, no record-chain arithmetic, no UTF-16 decoding — and it means a wake says
only "look again", which the loop was going to do anyway. One reply is several
notifications, because it is published as a temporary file, written, and renamed
over; nothing here counts them and no check may assert an exact number.

Read and write sharing are granted on the directory handle and delete sharing is
not. That is deliberate and load-bearing rather than caution: §4.2's sweep reads
a directory that refuses its own removal as "a client is still watching this
session", and every check that no handle outlives a wait rests on the same
refusal. Granting delete sharing is the one flag that could let a removal
through and make all of it vacuous.

What the checks prove is narrower than it looks and must not be written up as
more. Deleting the cancellation and the drain while keeping the handle close
leaves every check about a held handle green, because closing the handle does
release the directory. The cancellation is **observed to have run and
completed**, by way of the sink; it is not proved that no use-after-free is
possible.

What would reopen this: a measurement. §3.2 expects the poll term to disappear
from the round trip and §3.5 requires the new client to report its own p50
against the incumbent's 14 ms. If the watch does not move the figure on a real
install — or if it reports nothing on the filesystem a user's `Saved Games` sits
on, which nothing off DCS can tell — the poll interval, and whether the watch
earns its code at all, are open again. That measurement is Stage 9's and lands
as a record of its own.
