# ADR 0003: The wrapper answers three fields in one string, and the conversion travels as source

## Status

Accepted

## Context

An `eval` in any state but the host's own crosses `net.dostring_in`, which
answers one string. `bridge.md` §7.3 says what the chunk handed to it is:

> For every other state the chunk handed to the carrier is a one-line
> wrapper that carries the body as a `%q` literal and compiles it *inside the
> target state* — `loadstring(<body>, <chunkname>)` — then sets the count hook
> (§3.4), runs it under `pcall`, converts the result as §5.3 requires and
> returns the string.

and §5.3 says what the conversion is:

> a string passes as is; a number, boolean or nil becomes its text with
> `result_type` stated; a table, function, userdata or thread yields an empty
> body and its type name.

Two things the documents do not say. How `result_type`, and the difference
between a value returned and a message raised, travel back across a carrier
that answers one string: a reply carries `result_type` on `ok` and `stage`
on `error`, and both have to be read off the one string the state answered.
And where the conversion's code comes from: the executor already converts in
its own state with three functions, and the specification's rule that a
value is converted where it lives means the same rule has to run inside a
state the executor's functions cannot reach, and can only reach as text.

§5.1 keeps three answers of the carrier's own apart:

> a Lua string is the chunk's result; `nil` is `refused` […]; the literal
> `'Invalid state name'` is `invalid-state`

so whatever the wrapper answers must also not be mistaken for the literal,
and a chunk that happens to return the literal must not be mistaken for the
carrier's refusal.

## Decision

The wrapper answers one string of three fields: a status, a detail and the
body, the first two on a line each and the body the rest of the string.

- The status is one of six words: `ok`, `run` and `oversize` from the
  conversion; `compile` where `loadstring` in the state refused the body;
  `unsupported` where the state has no `loadstring` or no `setfenv`; and
  `bridge` where the wrapper itself raised. The executor reads no other
  word as the shape.
- The detail is the type on `ok`, what was too big on `oversize`, and empty
  otherwise.
- The body is the rest, so a result of any bytes and any length, blank lines
  included, is carried whole; the two newlines that end the first two fields
  are the only structure.

A string not in that shape, the harness stub's empty string among them, is
`error` under `stage: dostring_in` with the string as the body: the state
answered, and not with the wrapper's reply. A chunk returning the literal
`Invalid state name` arrives inside the shape and is a result; the bare
literal is the carrier's, and is `invalid-state`.

The conversion is one Lua source string in the executor, compiled once for
the host's own state the first time its carrier answers, never at load,
because a load parses nothing as code, and embedded as it is in every
wrapper, so the two carriers convert under one copy of one rule. The ceiling is applied
inside the state by that same source, so what crosses back for an oversize
result is the refusal and its length, and the result never leaves the state.

Alternatives:

- Two copies of the conversion, functions here and text in the wrapper:
  rejected, because the copy in the wrapper is the one nobody runs while
  editing the other, and they drift.
- A serialised table, or a fixed-width header, for the wrapper's answer:
  rejected, because a body is arbitrary bytes and a length field is one more
  number to get wrong; two newlines and a status word are read with two
  `find`s.
- Apply the ceiling after the crossing: rejected, because a result of any
  size would then cross `net.dostring_in` before being refused, and the
  point of the ceiling is that the reply is bounded before it is built.

## Consequences

The language server checks the conversion as a string and not as code: a
slip in it is found by the harness, which runs it under the reference
interpreter, and not by the lint. The wrapper carries the conversion's
source with every request, some hundreds of bytes, which is a cost on every
`eval` into another state and none on the host's own.

What the harness proves is the wrapper under the reference interpreter in a
modelled state; what DCS does with the wrapper, whether a raise inside
`net.dostring_in` is returned or propagated, and whether every state carries
`setfenv` and `_G`, is measured only at a live install, which is where this
is revisited. The instruction count hook the specification names in the
wrapper is not set yet; the task that builds it changes this wrapper and
the host's own carrier together, and does not change the three fields.
