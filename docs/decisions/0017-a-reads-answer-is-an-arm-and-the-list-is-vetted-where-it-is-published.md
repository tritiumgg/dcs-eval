# ADR 0017: A read's answer is an arm, and the list is vetted where it is published

## Status

Accepted

## Context

`mcp.md` §3.3 settles what the game-state reads are and the shape each one is
sent in. It gives the wrapping chunk verbatim:

```lua
local ok, v = pcall(DCS.getPause)
if not ok then return 'error\t' .. tostring(v) end
local t = type(v)
if t == 'table' or t == 'function' or t == 'userdata' or t == 'thread' then return t .. '\t' end
return t .. '\t' .. tostring(v)
```

and beside it: "`tostring` is applied to a scalar only; a table is reported by
type and never stringified, which is the bridge's own rule (D37) kept one level
up."

On why one chunk per read: "One chunk per read is chosen because it is **the
shape under which a crash names its killer** (`bridge.md` §4.5: the last `B|`
with no `O|`); a batched chunk that kills DCS says only that one of five reads
did it." And, on what is refused outright:

> **Never:** `DCS.getMissionLoaded()`, a named suspect in a hook-state crash
> , and `getPlayerUnitType` and `getMissionTheatre`, which
> were in the crashing batch. The list of reads is a constant in `dcs-eval`,
> and a chunk that is not in it is not a game-state read; an agent that wants
> one evaluates it as its own `dcs_eval`, under its own name.

(The gap after "crash" is a stripped citation in the frozen file; it is quoted
here as it stands.)

The document also says batching is not known to be unsafe — "ED's own
`webGUI.lua` batches its five and is not known to crash" — and `mcp.md` §8
leaves "Whether the batching of `DCS.*` reads is the hazard, or particular
reads are" undetermined. Tier 2 is "off by default, `--reads extra`", enabled
"only after the first live run has sent each alone under the probe supervisor".

What the document does not settle is what a *client* does with the answer: the
grammar's shape is given, but not how an errored read is told from an absent
one, nor where the constant list is enforced, nor what else may ride the
window the reads share.

## Decision

A read's answer is one of five arms with no default, and the constant list is
checked again on the bytes at the seam where they are published.

**The arms.** `Value { lua_type, value }` — `value` is `None` for the four
types the chunk reports without stringifying. `Raised { message }` — the read
threw inside its own `pcall`, message verbatim, and nothing else produces this
arm. `Malformed { body }` — an `ok` reply that is not the grammar.
`Unanswered { why }` — anything that is not an `ok` reply. `NotSent { why }` —
never published, because tier 2 is off or the session is loading.

**`why` is data and not a sentence.** `Unanswered` carries its own arms —
`NotOk { status, stage, detail }`, `Pending`, `Superseded`, `Dead`, `Window`
and `Unyielded` — and `Display` renders the prose from them. The derivation
built on this has to tell a refusal from a pending, and the two refusals the
document gives the probe, `refused` and `invalid-state`, from each other; doing
that by prefix-matching a formatted string is a default arm wearing a string
match, and a later change to the wording would silently change what it decides.
The status and the stage are what the far end said, so they are kept as the far
end's words rather than folded into this side's.

`Malformed` is not folded into `Raised` because a far end that has stopped
speaking the grammar is a finding about this build, not a fact about the game;
folding them would let a broken executor report itself as a game that threw.
`Unanswered` absorbs the window's error half — a spec that never reached the
disk, a spent counter, a session that would not read — because all of it means
"no answer came back" to a reader, and dropping it would turn a window that
could not publish into a read that was never listed. The four arms
`Value{boolean,false}`, `Raised`, `Unanswered` and `NotSent` are distinct, so
an errored read, an absent read and a false read cannot be confused; the
derivation built on top of this depends on that and on nothing else.

**The body is split at the first tab only**, because a raised message or a
returned string may carry tabs and newlines and the tail is passed through
verbatim. `error` is not a Lua type name, so a read that returns the string
"error" is tagged `string`.

**The chunk indexes the callee outside the `pcall`**, as the document writes
it. On a host where the table it sits under is nil the chunk raises before
`pcall` is entered, and the executor answers `error` with a stage — which is
`Unanswered`, never `Raised`. A read that threw and a host without the table
are different findings and nothing folds them together.

**The vet sits on the bytes at the publication seam.** `Read`'s fields are
private and the table is the only value of that type, so production cannot
assemble an unlisted read — but that is an argument and not a check, the specs
are what reach the disk, and a guard nothing can drive proves nothing. The
check therefore runs inside the one function that publishes, where a test can
hand it a spec the table could not have produced. Its cost is a substring scan
of a few short strings once per window.

**It has two refusals and not one.** `NeverSent` is checked against the three
bare names and `Unlisted` against the table, and neither subsumes the other: a
name promoted *into* the table would pass an allowlist, and the never gate is
what still refuses it. The never scan is a substring match rather than a parse,
which is sound only while no listed callee contains one of the three; a test
keeps that true. The allowlist falls on any body shaped like a read — a chunk
that protects a call — so the probe, which calls nothing, is held to the never
rule alone.

**The chunkname is `=dcs-eval read <callee>`**, so a crash, a raise and a log
line all say which read it was, which is the whole reason for one chunk each.
All nine reads go to `state: hook`; the reachability probe is an `eval` of
`return 'ok'` in `gui`.

**The window carries the ping first, the reads, and the probe last.** A window
is one wake and one quiet period whatever is in it, so taking them separately
would cost two. What the ping and the probe *mean* is not decided here: the
ping comes back as the envelope it was and the probe as its own answer.

**Neither the ping nor the probe speaks the read grammar, so neither is given a
read's answer.** The probe has `Reachable`, `Unanswered { why }` and
`Malformed { body }`: its success is a body of `ok`, which carries no tab, so a
probe read as a read would report the one outcome meaning "this state can be
reached" on the arm reserved for a far end that has stopped speaking — a
finding about this build standing in for a fact about the session. The three
answers the document gives it land on three of ours: `ok` is `Reachable`, and
`refused` and `invalid-state` are the status on `NotOk`. The ping comes back as
`Result<Envelope, Unanswered>`, so a ping that was refused is told from a
window that was never opened, which the load branch's `None` means; collapsing
both to `None` would lose exactly the distinction the read arms exist to keep.

Alternatives rejected: a default arm folding absence into a value — it is
exactly the confusion the next stage must not be able to make; a blocklist
instead of a constant list — the document says the list is the rule and it is
not known whether batching or particular reads are the hazard; a vet on the
table rather than the bytes — nothing could drive it; a `#[cfg(test)]`
constructor for `Read` to drive it — a guard reachable only from tests is a
guard about tests.

## Consequences

Tier 2 is built, off, and has no way to ask for it outside a test:
`Tiers::with_tier_two()` is the only route and no flag reaches it. Turning it
on is a live run's business, one read at a time under the probe supervisor, and
the `--reads extra` flag the document names is not built.

Nothing here proves the five tier-1 reads are safe. Every claim this makes is
about what leaves this side: the never-send check is a claim about the bytes
published, which is exactly the claim it can support. What a real `DCS.getPause`
returns, whether the five answer on a joined client, and whether any of them
crashes the hook state are unmeasured — every answer in the suite is a
stand-in's, told what to say.

The vet is code whose job is to make an impossibility checkable, and it will
look redundant to the next reader; that is why the argument is here and beside
it in the code. Deleting it leaves the ledger sweep green, because production
still cannot assemble an unlisted read from the table — what reddens is the
three tests that drive the seam directly, which is why they exist.

What would reopen this: a live run that shows the batching rather than the
particular reads was the hazard would make the one-chunk shape a cost rather
than a safeguard, though the attribution argument would stand. A measurement of
a tier-2 read alone promotes that row and nothing else.
