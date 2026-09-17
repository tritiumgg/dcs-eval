# ADR 0006: A `stale-session` reply echoes `for` whole, and the reply limit does not govern it

## Status

Accepted

## Context

A request written for another session is answered `stale-session`, and the
reply carries the stamp the request named back to the client. `bridge.md` §7.4
specifies that header as the last row of the reply table:

> | `for` | `stale-session` | the stamp the request named |

It says the stamp the request named and says nothing about a length. The
limits table, §7.7, gives two numbers that bear on the question and one that
shows how a bound is written when the specification wants one:

> | request | 262,144 bytes | refused `bad-request`, unread as code |
> | reply | 65,536 bytes, `max_result_bytes` in the handshake | **refused, never truncated**, for every evaluation: `error`/`oversize` with `result_bytes`, because a cut lands inside whatever the body is and the loss is silent (`prior:tools/hooks/DcsApiEval.lua:1693-1700`, the census-only form, generalised under §7.0). A consumer with a page budget clamps it below this number itself |
> | `chunkname` | 200 bytes, ASCII | §7.3; Lua abbreviates a name over 60 bytes in messages and the header carries it whole |

The reply limit is worded for an evaluation: it is what `max_result_bytes`
advertises, its refusal is `error` with `stage: oversize` and a `result_bytes`
header, and what it bounds is a result the executor produced. A refusal's echo
of bytes the client itself sent is not that. But a `for` header is only
bounded by the request limit above it, so a request at the 262,144-byte limit
can name a stamp of 262,137 bytes and the reply that refuses it is larger than
the reply limit — four times over. Nothing in the specifications says which
way that goes.

The executor's refusals mostly hold request bytes to a length: the refusals
that read a `state` or a `max_instructions` of the wrong shape cut the value
they name at 80 bytes, a `chunkname` over 200 bytes is refused outright, and a
result past `max_result_bytes` is refused rather than cut. They are not
unanimous, and were not before this record. Two refusals echo a request value
whole. `dispatch` refuses an unknown op with `"unknown op: " ..
req.headers.op`, and `OPS.eval` refuses a state this host does not serve with
`state .. " is not a state this host serves"` (both
`executor/DcsEvalExecutor.lua`); the second is the well-shaped sibling of the
80-byte cut above it, and nothing bounds a `state`'s length. A request naming
this session's own stamp and an op of 262,126 bytes is answered `bad-request`
with a reply of 262,238 bytes, and one naming a state of 262,000 letters is
answered `unsupported` with a reply of 262,132 bytes, both measured over the
harness's model of the hook state. So a reply bounded by the request limit is
not new here. What is new is one written deliberately, for a header whose
whole purpose is to be compared.

## Decision

The executor echoes `for` **whole and uncapped**, and the 65,536-byte reply
limit is deliberately not applied to it. The limit governs an evaluation
result — the bytes the executor produced, which a client cannot predict and
cannot check — and a refusal handing back the bytes the client itself sent is
a different thing. Echoing whole is what the header is for: a client compares
it against the stamp it wrote, byte for byte, and a capped echo would answer a
comparison with a maybe. The message in the body still excerpts the stamp at
80 bytes, because that half is for a person reading it.

A `for` of that size is a client defect. The fence exists to make a client
defect visible instead of fatal, and a client that writes a couple of hundred
thousand bytes where a stamp belongs is exactly the case it should not quietly
round off.

The cost is measured, not argued. A request over 262,144 bytes is refused
unread, so the most a `for` can carry is 262,137: `for: ` takes five bytes and
the blank line that ends the envelope takes two. Driven over the harness's
model of the hook state, with its `1000-7` stamp and a three-byte request id,
such a request is answered `stale-session` with a 262,430-byte reply — 262,143
bytes of `for` line, 101 bytes of the eight headers before it, the blank line,
and a 185-byte body. That is four times the reply limit. Four of those eight
are fixed only in the fixture: `stamp` is `<os.time()>-<pid>`, sixteen bytes on a
live session rather than the fixture's six; `id` is whatever the client named
its file; `tick` is one byte on the first frame and grows with the session,
and `cpu_ms` is five bytes at its shortest, `0.000`, because `reply` formats
it with `%.3f`, and grows past that with the figure. The 101 bytes above are
counted with those two as the first frame writes them. The body carries this
session's stamp a second time — the message names both — so a sixteen-byte
stamp costs ten bytes twice. The same
request against a sixteen-byte stamp and a fifteen-byte id measures 262,462
bytes: ten more in the header, ten more in the body, twelve more in the id.
The request limit bounds the echo; it does not bound the reply.

Rejected: capping the echo, with the 200-byte `chunkname` cap as the
precedent. It would hold every reply under the advertised limit, and it is the
alternative to revisit; it is not taken now because the cap would have to be
pinned in the executor and in the Rust stand-in as a second rule that can
drift, and it would break the one comparison the header exists to answer.
Rejected: refusing an oversize `for` as `bad-request`, which answers a client
defect with a status that hides which session the request was written for.

## Consequences

A reply can exceed the figure the handshake advertises as `max_result_bytes`,
and a client that sizes a buffer from that figure rather than from the file it
is reading will come up short on this one path. The build's own client reads
the reply file whole, so it does not.

The executor has three reply paths whose size is bounded by the request limit
and not the reply limit: this one, chosen, and the unknown-op and
unserved-state refusals above, which reach the same place by leaving an
excerpt out. Anything later that assumes every reply fits under
`max_result_bytes` has to except all three, and `docs/PLAN.md` carries a task
to put the excerpt back in the other two.

This reopens if a client chokes on such a reply, or if a second header is ever
designed to echo request bytes whole: a second *chosen* echo is a rule rather
than an exception, and the cap above is what it would be written as. The two
refusals are not that second header — nothing was decided at either, which is
why they have a task and this has a record. Nothing in the frozen documents
would have to move for the cap to be taken instead; they settle neither
question, which is why this record exists rather than a row in
`docs/audit.md`, where the disagreements between them live.
