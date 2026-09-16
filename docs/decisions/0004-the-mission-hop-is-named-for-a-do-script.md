# ADR 0004: The `missionscripting` carrier is named for `a_do_script`, and its refusal for the missing mission

## Status

Accepted

## Context

`net.dostring_in` reaches `missionscripting` under no name, so the executor
reaches it in two hops: into `mission`, then through `mission`'s global
`a_do_script`. The specifications call that second hop "the door", and carry
the metaphor onto the wire twice. `bridge.md` §5.4, on the status a caller
gets with no mission loaded:

> A door that is shut answers a status of its own, `door-shut`, with a body
> saying no mission is loaded. Today this is an `error` with `stage: remote`
> and a message a consumer has to read (`:1491-1495`); a status is what a
> consumer can branch on.

and §7.4, on the `stage` header:

> `compile`, `run`, `dostring_in`, `remote`, `door`, `oversize`, `budget`,
> `bridge` — opaque to a consumer, rendered and never branched on (D45).

`mcp.md` §3 names `door-shut` among the refusals the tools word.

The metaphor names nothing a reader can look up. The thing it stands for
already has a name: the reply headers the same section specifies read
`carrier: a_do_script` and `via: mission`, and `a_do_script` is the DCS global
a reader will search for. A status word built on the metaphor tells a caller
something is shut; the caller has to read the body to learn that what it
lacks is a loaded mission. ADR 0002 renamed names and left the statuses
alone; this is a status, so it needs its own record.

Nothing consumes either word yet. The client library and the MCP server parse
neither, and nothing is released.

## Decision

The build names the hop for what it calls: **`a_do_script`**. The status for a
request with no mission loaded is **`no-mission`**, and the stage for a
failure at the call is **`stage: a_do_script`**. "Door" is not a name in the
build: no status, stage, identifier, suite or task is called by it, though a
quotation of the specifications keeps the word, as ADR 0002's keep "bridge".

| | specification | build |
|---|---|---|
| status, no mission loaded | `door-shut` | `no-mission` |
| stage, the call failed or handed back no string | `door` | `a_do_script` |
| the carrier, in prose and code | the door, `DOOR`, `FAR` | `a_do_script`; `NEAR` runs in `mission`, `FAR` in `missionscripting` |

Everything else in those sections is built as written: the two hops, the `%q`
arguments, the slot-2 read, the string-only rule, and `carrier: a_do_script`,
`via: mission` on every reply.

Rejected: keeping the specification's wire names and renaming only the code,
which leaves a word on the wire that the code no longer explains. Rejected:
another metaphor (`relay`, `hop`, `trigger`), which repeats the problem with a
different word. Rejected: `stage: no-mission` alongside `status: no-mission`,
which would make a raise inside `a_do_script` read as a missing mission.

## Consequences

A caller written against the specifications branches on a status the build
never sends. No such caller exists: the client library reads `status` as a
header value and maps no status to anything yet, the tool wording (plan task
T39) is written against the build's names, and `docs/PLAN.md` uses them.

The specifications and the build use different words for one status and one
stage, a gap as permanent as ADR 0002's. `docs/audit.md` records the mapping.

