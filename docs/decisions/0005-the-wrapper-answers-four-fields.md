# ADR 0005: The wrapper answers four fields, the budget one of them

## Status

Accepted

## Context

ADR 0003 settled what the wrapper hands back across `net.dostring_in`: one
string of three fields, a status, a detail and the body. It closed by
saying the instruction count hook would change the wrapper and the host's
own carrier together "and does not change the three fields". Building the
hook showed that it does.

`bridge.md` §3.4 says where the hook is set and what a reply says about it:

> the wrapper §7.3 compiles the body under installs a count hook,
> `debug.sethook(fn, '', n)`, inside the target state before the chunk runs
> and clears it after. Exceeding it is `error` with `stage: budget`. […]
> and `mission` has no `debug` (§1, S5), so a reply from there carries
> `budget: none` and a caller reads it.

and §7.4 makes the budget a header of every `eval` reply:

> | `budget` | `eval` | `instructions=<n>` or `none` (§3.4) |

Whether a state has `debug` is known in that state and nowhere else. The
executor could guess it from the state's name, but the only state the
specification says lacks it is `mission`, and whether every other state
DCS runs has one is unmeasured. The budget therefore has to be read where
the chunk runs and cross back beside the result, and ADR 0003's three
fields have no place for it: the detail already means the type on `ok`
and what was too big on `oversize`, and the body is the rest of the string.

## Decision

The wrapper answers one string of four fields: a status, a detail, the
budget and the body, the first three on a line each and the body the rest.

- The statuses are ADR 0003's six and `budget`, for a chunk that spent its
  count.
- The budget field is `instructions=<n>` where a count hook was set, `none`
  where none was, and empty where the wrapper never reached the chunk:
  `unsupported`, `bridge`, and the statuses the `a_do_script` near chunk
  adds. It is settled before the body is compiled, so `compile` carries it.
  The executor adds the `budget` header where the field is not empty, after
  `chunkname`.
- The count travels in as a number the executor printed. Through
  `a_do_script` it crosses as a third string argument beside the body and
  `chunkname`, so the far chunk's source stays the same bytes for every
  request.
- What bounds a chunk is one Lua source string beside the conversion, for
  ADR 0003's reason: it runs in the host's own state and inside every
  other, and one copy is what keeps the two from drifting. The conversion
  still travels as source, and everything else ADR 0003 decided stands.

Three things the specification did not say, decided with it:

- A hook already set in the state is not displaced. The chunk runs
  unbounded and the reply says `none`, because the hook belongs to DCS or a
  debugger, and clearing it after the chunk would break that tool silently.
- A budget once spent stays spent: the hook raises on every instruction
  after the first raise, so a chunk that catches it with `pcall` cannot go
  on looping.
- A firing on the executor's own instructions between the chunk returning
  and the hook being cleared is ignored, so a chunk that finishes right at
  its count reads `ok` and not a raise from the executor.

Alternatives:

- Decide the budget in the executor from the state's name: rejected,
  because it states as fact a property of each state that only `mission`'s
  has been read.
- Carry the budget inside the detail field: rejected, because the detail
  already has a meaning per status and a reader would parse one field two
  ways.
- Run the chunk in a coroutine with the hook set on that thread alone, so
  no hook is ever set on the state's own thread: rejected, because it
  changes what a chunk sees (`coroutine.running`, a top-level `yield`), and
  what DCS does with its own functions called from a coroutine is
  unmeasured.

## Consequences

Every reader of the wrapper's answer reads four fields, and the harness
suites that pin the string moved with it. A string that looks like three
fields is now outside the shape and reads as `stage: dostring_in`.

The hook is a guard against accident and not a boundary, and its limits
are stated rather than hidden: one long C call is not interrupted, a chunk
can clear the hook itself, and a coroutine the chunk runs escapes it,
because Lua 5.1's `debug` keeps a hook's function per thread.

A state that already holds a hook runs every chunk unbounded. Whether any
DCS state does is unmeasured, and so is what DCS does with a hook raising
inside `net.dostring_in` or `a_do_script`; the first live run that reads
`budget: none` from a state other than `mission` reopens the first
decision above.
