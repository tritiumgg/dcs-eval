# ADR 0026: A file evaluation reads any path, and only its size is judged

## Status

Accepted

## Context

`mcp.md`'s requirement table sets the rule in one cell:

> | 4 | File as source | The server reads the file outside DCS, hashes it, and ships it with `chunkname: @<path>`; it reads only under roots the operator allowed, refuses an oversize file naming the limit, and records path and SHA-256 for every run |

§4.2 makes it a decision:

> - **Allowed roots are configured**, `--allow <dir>` repeatable on `serve`, resolved to real paths
>   at start-up. When none is given, the one root is the server's working directory at launch,
>   which is where an MCP client launches it — the project the agent is working in. A path is
>   admitted only if its *resolved* real path — 8.3 short names expanded, junctions followed, `..`
>   collapsed, case folded — lies under a root at a segment boundary
>   (`prior:pipeline/src/bridge/paths.ts:117-203`, D20, D38). The textual path is never tested.
> - **Always refused, inside a root or not:** anything under `<Saved Games>\DCS*\Config\`, because
>   `network.vault` there holds the user's account credentials and no chunk this server serves has
>   business reading it; and anything under the DCS install when the install is known (`--install`
>   or `DCS_INSTALL`), because nothing this server serves needs ED's file as a chunk — an agent that
>   wants one runs `dofile` in a one-line chunk and DCS reads its own file.
> - **A refusal names the rule and never the content**: not the first line, not the byte count
>   beyond the limit sentence, not whether the path exists outside the roots.

§4.3 says what the rule is for:

> A Lua compile error echoes source: `x.lua:3: unexpected symbol near 'secret'` carries the token,
> and for an unterminated string the token is the rest of the file. A run error echoes whatever the
> chunk formats. The wire returns both verbatim (`bridge.md` §7.3), so "read any path" plus
> "return errors verbatim" is a way to read the first token of any file on the machine through a
> compile error. The composition rule: **errors are verbatim because the read was already
> permitted**, and a read that is not permitted returns nothing of the file. §4.2 is therefore the
> whole of the protection, and it is enforced before a byte is read, not after a compile fails.

and §7 lists the control:

> | `dcs_eval_file` refuses a path outside every root, any path under `Config\`, and the install, before reading a byte; the refusal names the rule and no content | §4.2–4.3 | — |

ADR 0014 built this in the client library with the roots supplied by the
caller. The binary supplied none, because no `--allow` was ever built, and an
empty list admits nothing, so every path given to `dcs_eval_file` or
`eval --file` was refused. That was the shipped behaviour until this record.

What changed is the maintainer's reading of what the rule protects, on
2026-09-21. The `hook` state the executor evaluates in has Lua's full `io`
library, so `dcs_eval hook "return io.open([[path]]):read('*a')"` already
reads, whole, any file the game can read, including every `network.vault`. The
agent calling this server has its own file tools. And §4.2 itself names the way
around its install rule: "an agent that wants one runs `dofile` in a one-line
chunk and DCS reads its own file". A rule a one-line chunk walks around
protects nothing, and what it did in practice was refuse every path. The
maintainer's standard: nothing is blocked unless it would crash something.

## Decision

A file evaluation reads any path this process can open. Where the path lies is
not judged.

- **There are no allowed roots, no `Config\` refusal and no install refusal.**
  `Roots` is gone from `dcs-eval`, `file::check` takes the handshake, the
  headers and the resolved path, and `dcs-mcp` threads nothing into it.
- **The ceiling stays, and stays ahead of the read.** It is a protocol limit
  and not a policy: the executor refuses a request over `max_request_bytes`
  whole. A file is refused when its size plus the framed header block exceeds
  that figure, `size + header_block > max_request_bytes` as a saturating sum
  and never as a subtraction, on one stat taken before the file is opened. The
  figure comes off the handshake and is never assumed. The refusal names the
  limit, the size and the header block, and the two ways out; it carries no
  byte of the file.
- **The path is still resolved first, and `check` still takes a
  `paths::Real`.** It is no longer for containment. The `chunkname` is
  `@<resolved path>`, the run record names the resolved path, and the stat has
  to measure that same object. A type only `paths::resolve` can make keeps the
  three from coming apart.
- **What ADR 0014 settled beyond the roots stands:** the header block is
  measured by framing the exact set the caller sends with an empty body, and
  the reader owns the gap between the stat and the read. The reader's refusal
  to follow a reparse point put in place since the path was resolved
  (ADR 0015) stays too, for provenance now rather than containment: what is
  hashed and sent is the object that was measured.

Alternatives rejected:

- *`--allow` roots with the launch directory as the default, as §4.2 has it.*
  The `hook` state's `io` reads what the roots refuse, so the roots only make
  the tool fail on the files an agent most often points it at.
- *Keep the `Config\` refusal alone, as a credentials guard.* One `io.open`
  line in `hook` reads the vault whole; a guard the same caller walks around in
  one line is not a guard.
- *Keep the install refusal.* §4.2's own remedy for it is `dofile` inside DCS,
  which reads the same file.

## Consequences

"Read any path" plus "return errors verbatim" is now exactly the composition
§4.3 warns about: a compile error of any file quotes its tokens to the caller.
That is accepted, because the caller could read the file whole through `io` in
`hook`. A stat refusal now says whether a path exists, which §4.2's third
bullet forbade, and is accepted for the same reason. §4.2's `DCS_INSTALL` has
no use left in the file evaluation, so the spelling question in
`docs/audit.md` turns on nothing here.

`Roots`, `Refusal::Credentials`, `Refusal::Install` and
`Refusal::OutsideEveryRoot` leave the library's public surface. No consumer
outside this workspace exists yet.

What would reopen this: a state that has no `io` becoming the only one the
executor can reach, which Stage 9 measures; or this server being offered to a
caller who has no file access of their own, for whom `dcs_eval_file` would
then be a new reach rather than a convenience.
