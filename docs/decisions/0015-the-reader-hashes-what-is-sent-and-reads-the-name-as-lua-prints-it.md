# ADR 0015: The reader hashes what is sent, and reads the name as Lua prints it

## Status

Accepted

## Context

T34 settled what `evalFile` refuses before a byte is read and produced an
`Admitted`. This is the other half: the reader that opens one, takes the
bytes, applies the two rules Lua's own loader applies, and produces the
provenance the run record carries.

`mcp.md` §4.5 is the whole of what the bytes get done to them:

> - The file is read as bytes and shipped as bytes: Lua source may be UTF-8 or
>   cp1251 and the body is opaque to the protocol (`bridge.md` §7.1).
> - A leading UTF-8 byte-order mark is stripped and `bom: stripped` is
>   recorded, because Lua 5.1.5's `loadstring` does not skip one and a chunk
>   beginning with it fails to compile on line 1. No line number moves: a BOM
>   holds no newline.
> - A first line beginning `#` is replaced by an empty line and
>   `shebang: blanked` is recorded. That is what `luaL_loadfile` does in
>   5.1.5 — it discards the line and supplies a `\n` in its place so the count
>   is kept — and `loadstring` does not.
> - CRLF passes through: Lua's lexer counts `\r\n` as one line, so a Windows
>   file's numbers are true without conversion.
> - `chunkname` is `@` followed by the resolved real path, so an error reads
>   `<path>:<line>:`. Lua abbreviates a name over 60 bytes to `...` and its
>   tail in messages (`bridge.md` §7.3); the reply's `chunkname` header
>   carries the whole path, and the tool prints both.

§4.6 says what the record holds, and its example line is:

> ```
> {"ts": "...", "id": "0000000042-k3Jd", "stamp": "1757160000-31244", "host": "hook",
>  "state": "gui", "source": "file", "path": "C:/Users/tritiumgg/projects/x/probe.lua",
>  "sha256": "…", "bytes": 4071, "chunkname": "@C:/Users/tritiumgg/projects/x/probe.lua",
>  "bom": "none", "shebang": "none", "status": "ok", "stage": null, "cpu_ms": 0.41,
>  "tick": 88123, "budget": "instructions=1000000"}
> ```

§4.1 says why the hash is taken here at all:

> a run of a file must record the hash of what ran, and only a
> process that read the bytes can hash them at the moment they were sent

`bridge.md` §7.3 is the far end's reading of the name and the body:

> | `chunkname` | `eval`, optional | the name the chunk is compiled under,
> passed to `loadstring` verbatim. Lua's own rule applies: `@<path>` reports as
> `<path>:<line>:`, `=<name>` as `<name>:<line>:`, anything else as
> `[string "…"]:<line>:`. ASCII, no CR or LF (§7.1), at most 200 bytes.
> Absent, the bridge uses `=dcs-api-eval` |
> | body | `eval` | the chunk, byte for byte; empty is `bad-request` |

and, in the same section:

> Lua abbreviates a source name over `LUA_IDSIZE` (60) bytes in messages to
> `...` and its tail, which keeps the line true and shortens the path; the
> reply's `chunkname` header carries the full name.

## Decision

The reader hashes the bytes it is about to send, refuses locally what the wire
would refuse remotely, and renders a chunkname the way the interpreter renders
it rather than the way the documents describe it. Six readings:

1. **The hash is over what is sent, not over the file as it sits on disk.**
   §4.1 decides it: the record answers "what ran", and what ran is the body
   after the byte-order mark is gone and the first line is blanked. So a
   BOM'd or shebanged file hashes differently from `sha256sum` on that same
   file, and `bom` and `shebang` in the record are exactly what lets a reader
   reproduce the digest from the file on disk. A file with neither must agree
   with `sha256sum` byte for byte, and one test pins both readings against
   each other so neither can drift alone.

2. **The shebang's content is blanked; its own terminator is kept as it was.**
   Supplying a `\n` over a line that ended `\r\n` would be the conversion the
   next bullet of §4.5 forbids in the same breath, so `#!/usr/bin/env lua\r\n`
   becomes `\r\n`: the count is kept and no byte is rewritten, only dropped.
   The byte-order mark is stripped **first**, so a file that is a mark
   followed by a `#` line still has its shebang blanked.

3. **An empty body is refused here rather than sent.** §7.3 answers an empty
   body `bad-request`; a request that could only come back refused is not
   worth a round trip, and a local refusal can say *why* it is empty — zero
   bytes, or a byte-order mark and nothing else. A remote `bad-request` says
   neither. The mark is the only thing the refusal carries: a `#` line with
   no terminator behind it is told the same as a zero-byte file, because the
   distinction the reader could draw there is not worth a second field on the
   variant.

4. **A chunkname over 200 bytes is refused here too, for the same reason,
   and the name refused is the one the request carries.** §7.3 caps the header
   at 200 bytes, and this crate's framer enforces no length on any value at
   all, so nothing would otherwise stop a deeply nested resolved path
   producing a header the far end rejects. The reader therefore reads the
   name off the block it is about to send rather than deriving it from the
   path a second time: the cap then falls on the value that is really going
   out, and the record names the chunk the far end will really compile. A
   request carrying no `chunkname` at all is not refused — §7.3 makes the
   header optional — and the record says `=dcs-api-eval`, which is what the
   far end names it and what a raise from it will print. The name is what
   this half owns; the containment and the ceiling are ADR 0014's.

5. **What Lua prints of a long name is its last 52 bytes, not its last 60, and
   a compile error abbreviates later than a raise.** Measured on the pinned
   5.1.5 interpreter at name lengths 50 through 90: a raise shows the whole
   name at 52 and `...` plus the last 52 from 53 up; a compile error shows the
   whole name at 72 and `...` plus the last 72 from 73 up. 60 is `LUA_IDSIZE`,
   the buffer `luaL_where` hands `luaO_chunkid`; that function spends
   `sizeof(" '...' ")` — eight bytes — before it copies, and the lexer renders
   a compile error into an 80-byte buffer instead. Both frozen documents print
   60, and both are describing the buffer rather than the cut. An assertion
   written against 60 would be wrong in both directions: it would expect no
   abbreviation for a 55-byte path, and the wrong tail for everything longer.
   The crate therefore ships `RUNTIME_IDSIZE = 60` and `COMPILE_IDSIZE = 80`
   with the eight-byte reserve inside the renderer, so the constants are the
   ones Lua's own source names and the arithmetic is in one place.

6. **The name is spelt as this crate resolves it.** A resolved path prints
   with backslashes and no `\\?\` prefix, while §4.6's example record line
   shows forward slashes. Lua treats the name as opaque, the reply echoes it
   whole, and a user recognises their own spelling, so the backslash form is
   what goes in `chunkname` and in the record. The drift from the example is
   recorded here rather than discovered later.

Rejected:

- **Reframing the headers inside the reader.** The reader would then need the
  caller's headers a second time and could be handed a different set from the
  one the ceiling was measured against. It uses the block `check` already
  framed instead, so there is no second set to get wrong. The `chunkname` is
  the one field both halves would otherwise have an opinion about, and it is
  not excepted: `check` lifts the value out of the very slice it framed and
  keeps it beside the block, and the reader reads that. Deriving it from the
  path in the reader was tried and is what this bullet rejects — it left the
  record free to print `@C:\...\probe.lua` for a request whose header said
  `=something-else`, or said nothing.
- **One shared `Mark { None, Applied }` for both rules.** The two words that
  are not `none` are different words — `stripped` and `blanked` — and a shared
  enum would have to carry them anyway.
- **Hashing the file as it sits and recording that.** Cheaper to cross-check,
  and wrong: it is not the hash of what ran.

## Consequences

Two thresholds are now this crate's to be right about, and they were measured
on this machine rather than read out of a document. What would reopen them is
a DCS build carrying a Lua whose `LUA_IDSIZE` or lexer buffer differs; the
constants are named for Lua's own and the renderer is pinned against the live
interpreter at seven name lengths, so a difference shows as a failing test
rather than as a wrong assertion.

A user reproducing a digest from a file on disk needs `bom` and `shebang` out
of the record to know what to strip first. That is the price of hashing what
ran, and the record carries both fields for exactly this.

The handle judgement closes less than `Admitted`'s doc promises. The reader
opens without following a reparse point and refuses what the handle says is
not a regular file, which catches a leaf swapped for a junction or a link
between the resolve and the open. It does not prove the file is the same file:
that wants a volume-serial-plus-file-index comparison, whose standard-library
accessors are unstable and whose Win32 call is not in ADR 0011's declared set,
so it is an amendment to that record rather than a line in this one. **The
symlink arm is unproven on this machine**: creating a file symlink here needs
elevation (observed unelevated, from `mklink`: "You do not have sufficient
privilege to perform this operation."), so the test drives a directory
junction, which needs none, and proves only that a swapped leaf does not come
back as bytes.
