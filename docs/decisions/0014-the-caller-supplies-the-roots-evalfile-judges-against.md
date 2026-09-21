# ADR 0014: The caller supplies the roots `evalFile` judges against

## Status

Superseded by [ADR 0026](0026-a-file-evaluation-reads-any-path-and-only-its-size-is-judged.md)

## Context

`mcp.md` §4.2 decides which paths a file evaluation may read:

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

§4.3 says what that protection is worth and when it has to run:

> A Lua compile error echoes source: `x.lua:3: unexpected symbol near 'secret'` carries the token,
> and for an unterminated string the token is the rest of the file. A run error echoes whatever the
> chunk formats. The wire returns both verbatim (`bridge.md` §7.3), so "read any path" plus
> "return errors verbatim" is a way to read the first token of any file on the machine through a
> compile error. The composition rule: **errors are verbatim because the read was already
> permitted**, and a read that is not permitted returns nothing of the file. §4.2 is therefore the
> whole of the protection, and it is enforced before a byte is read, not after a compile fails.

§4.4 is the size ceiling:

> The bridge refuses a request over `max_request_bytes` — 262,144 bytes, read from the handshake and
> never assumed — and never parses it as code (`bridge.md` §7.3). The server stats the file before
> reading it and refuses one whose size plus the header block exceeds the limit, with a message that
> names the limit, the file's size, and the two ways out: split the file, or `dofile` it from a
> one-line chunk in a state that has `io`. Nothing is truncated, ever: a file cut at a byte boundary
> is a different program that might compile.

§7's control line for the ceiling:

> | A file one byte over the ceiling is refused naming the limit and the size; one at the ceiling is sent whole | §4.4 | — |

`bridge.md` §8 is quoted for one clause only. Its containment sentence is
about the *transport directory* and about where the client **writes**, not
about what `evalFile` reads, so it is not this rule; what it carries here is
the parenthesis saying the install is configuration:

> is refused when relative, when inside the install (supplied by `--install` or `DCS_API_DCS_INSTALL`,
> since the consumer cannot derive it from a user profile)

And the executor's own comparison, which is the behaviour the client must not
trip, read from the build rather than from prose —
`executor/DcsEvalExecutor.lua` stats the whole `.req` and compares

```lua
if size > MAX_REQUEST_BYTES then
```

Four things the frozen text leaves the client library unable to do as
written. It has no `argv`, no start-up and no launch: it is a library whose
`check` is called on tool call fifty. It cannot know an install that "is
known" without being told one. And `check` cannot both resolve a path itself
and measure the header block the caller will send, because one of those
headers is `chunkname: @<resolved path>` — the number it would be measuring
is not knowable until the resolution it has not done yet has happened.

## Decision

The caller supplies the roots, and `evalFile`'s judgement is made against a
path the type system already proves was resolved.

- **`Roots::new(allowed, writedirs, install)` takes what it judges against.**
  All three are `paths::Real`, resolved by the caller. The library reads no
  argument vector, no environment variable, and never the process's current
  directory. `Roots::new` resolves a `Config` per write directory once, so a
  junction at one is followed too.
- **`writedirs` is a list, because `<Saved Games>\DCS*\Config\` is a glob
  over every variant.** Stable, open beta and a dedicated server sit side by
  side under `Saved Games`, each with its own `network.vault`, and one
  allowed root of `Saved Games` covers all of them — a guard on a single
  write directory would leave the siblings' credentials readable, which is
  the opposite of what the bullet asks for. So every write directory the
  caller supplies is guarded. What the library will not do is expand the
  glob: which variants exist is a question about the machine, of the same
  kind as where the install is, and this library answers none of those. The
  expansion is `dcs-mcp`'s argument parsing in Stage 7, and until it lands
  the guard covers exactly the variants a caller named — which the README
  says on its line and a test pins from both sides.
- **No root admits nothing, and there is no fallback to the current
  directory.** §4.2's "the server's working directory at launch" is a policy
  of `dcs-mcp serve`, where "at launch" is a real moment; a library call has
  no launch, and containment against wherever this process happens to be
  sitting is a check on an accident — the reasoning `paths::PathErrorKind::Relative`
  already carries for a relative path. The default lands in `dcs-mcp`'s
  argument parsing. The `Config` and install rules are not switched off under
  an empty allow list; they are merely moot there, and a test says so.
- **The install and the write directory are supplied, never derived from the
  handshake.** `install_guard` and `lfs_tempdir` are `Diagnostic`s the reader
  declares reported-never-used, and a rule that silently fired on a value a
  handshake happened to carry would make a refusal depend on whether a
  handshake had been read at all. `dcs-mcp` may *seed* the supplied install
  from `install_guard` visibly at start-up, which keeps it configuration.
  With no install supplied the install rule does not fire; with no write
  directory supplied the `Config` rule has no root to fire on, and with one
  variant supplied it fires for that variant alone. "Always
  refused" means *an allowed root does not excuse it*, not *refused without
  knowing where Saved Games is* — refusing every path with a `Config` segment
  would refuse `C:\project\Config\x.lua`, which §4.2 does not ask for.
- **"At the ceiling" is the file's ceiling, and the comparison is the sum.**
  `size + header_block > max_request_bytes` refuses, which is §4.4's own
  sentence and matches the executor's `>` on the whole `.req`. The file
  ceiling is therefore `max_request_bytes - header_block`, and a file of
  exactly that is admitted because its framed request is exactly
  `max_request_bytes`, which the executor accepts. Written as a subtraction —
  `size > max.saturating_sub(header)` — a header block at or past the limit
  saturates to zero and then admits a zero-byte file whose framed request is
  already over, which is why the sum is the spelling and a test pins it.
  The header block's bytes are measured with the framer on an empty body,
  never estimated and never a constant.
- **`check` takes a `&Real` and a `&Handshake`.** `Real`'s field is private
  and only `paths::resolve` makes one, so a `&Real` parameter *is* the proof
  that resolution preceded judgement — a cheaper guarantee than a rule in a
  comment. The caller resolves once and builds its headers, `chunkname`
  included, from that same `Real`, so nothing is ever measured against a
  spelling. `&Handshake` rather than a `u64` because a `u64` parameter is a
  place for a caller to pass a figure it made up, which is the thing "never
  assumed" forbids.

Alternatives rejected:

- *The library falls back to the current directory when no root is given.* A
  containment check against an accident of where the process was started.
- *The install is taken from the handshake's `install_guard`.* A refusal that
  depends on whether a handshake has been read is not a rule.
- *Every path with a `Config` segment is refused.* Refuses `C:\project\Config\x.lua`,
  which §4.2 does not ask for.
- *`check` resolves the path itself and measures the headers.* Circular: the
  `chunkname` header whose bytes are counted is not knowable until the
  resolution has happened.

## Consequences

A caller that supplies no write directory gets **no credential guard**, and a
caller that supplies one of several variants gets it for that one only. That
is the honest reading of a library that derives nothing, and it moves the
obligation rather than removing it: `dcs-mcp serve` refusing to serve without
a write directory, and enumerating the `DCS*` siblings beside the one it was
given, are both Stage 7's, and until that lands the guard exists only where a
caller configures one. The same is true of the install rule.

The order of judgement is load-bearing. `Config` and the install are reported
ahead of the roots rule so a user who added an allowed root covering `Config`
is told the real reason rather than a weaker one; a mutation that moves the
roots branch up is what proves it.

The refusals carry the rule and the path and nothing else. The oversize
refusal names the limit, the file's size and the header block — all three off
the stat and the handshake, none off a byte of the file — and the two ways
out §4.4 lists. A path outside every root draws the same words whether or not
it exists, which is §4.2's third bullet enforced directly rather than
asserted.

Nothing here reads an environment variable, so §4.2's `DCS_INSTALL` and
§8's `DCS_API_DCS_INSTALL` do not have to be reconciled yet; the
disagreement is recorded in `docs/audit.md` for the Stage 7 task that must
pick one.

What this decision does not settle is everything that needs a byte: the hash,
the BOM, the shebang and the send are the file reader's, and the gap between
the stat and the read — a file that grows after it was measured — is there
too. `check` hands back what the stat said and says so.

Two more obligations sit on the far side of that boundary, and the reader
owns both.

The first is the other gap in the same place. `paths::resolve` canonicalises
the nearest existing ancestor and puts the rest of the path back on, so a
`Real` for a leaf that does not exist yet carries a final segment nothing
followed. A caller can resolve `<project>\x.lua` while it is absent, `check`
can admit it, and before the file is opened anyone who can write in that
allowed root — the agent's own project directory — can create `x.lua` as a
junction into the install, which `fs::metadata` follows and a read would
follow too. Nothing in `check` can close that: the judgement is made when it
is called, and the path is opened later by someone else. What closes it is
the reader judging the handle it actually opened — the final path of the open
file, not the path it was handed — or refusing a resolved leaf that is not
there. Either answer belongs to the reader; what belongs here is saying that
one is owed.

The second is the header set. `check` counts the `headers` it is given, and
nothing binds them to the headers the caller goes on to send: measure with
`op` and `state`, then send a request carrying `chunkname` and `id` as well,
and the framed request is over `max_request_bytes` after this side has
already handed over the whole file — the case the ceiling exists to avoid
here rather than at the executor. `check` cannot check it without building
the headers itself, which is the circularity rejected above. So it is a
contract, stated in the doc comment and repeated here: the caller measures
with the exact set it will send, and the reader that assembles the request is
the one holding that obligation.
