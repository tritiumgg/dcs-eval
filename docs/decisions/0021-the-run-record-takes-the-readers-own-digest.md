# ADR 0021: The run record takes the reader's own digest, and no other

## Status

Accepted

## Context

`docs/specs/mcp.md` §4.6, retrieved with `sh tools/spec.sh read MCP 4.6`, asks
for the record and shows one line of it:

> A record of what ran that cannot be reproduced is worth little, so every
> evaluation — file or inline — appends one line to `<data-dir>\runs.jsonl`
> (default `%LOCALAPPDATA%\dcs-mcp\`) before the reply is rendered:
>
> ```
> {"ts": "...", "id": "0000000042-k3Jd", "stamp": "1757160000-31244", "host": "hook",
>  "state": "gui", "source": "file", "path": "C:/Users/tritiumgg/projects/x/probe.lua",
>  "sha256": "…", "bytes": 4071, "chunkname": "@C:/Users/tritiumgg/projects/x/probe.lua",
>  "bom": "none", "shebang": "none", "status": "ok", "stage": null, "cpu_ms": 0.41,
>  "tick": 88123, "budget": "instructions=1000000"}
> ```
>
> The tool's text carries the same path and hash on its first line, so the
> agent's transcript holds the provenance beside the result.

It says the line carries `sha256` and that an evaluation appends one. It does
not say where the digest comes from, what an inline chunk — which has no path
and no reader — puts in those fields, or what is to happen when the record
cannot be written.

Decision record 0015 already settled that the client library's reader hashes
the bytes it read and keeps that digest beside them, and `source::Source` says
in its own words that rendering the line belongs to whoever holds the rest.

## Decision

The digest on a line is taken from the source reader's record and is never
computed by the module that writes the line.

- **The hash is taken, not recomputed.** `runs::line` maps over the
  `Source` the reader handed back and asks it for its digest. A digest
  computed where the line is written could only be over the bytes about to
  go on the wire, which is a different claim under the same name: it would
  agree with the file in every case except a reader that passed on something
  other than what it read, which is the one case a provenance record exists
  for. Rejected: hashing the request body at the writer — cheaper to write,
  and it makes the record unfalsifiable.
- **An inline chunk records `null` for `path`, `sha256`, `bom` and
  `shebang`.** There is no reader's record to take a digest from, and taking
  one here would put a recomputed digest in the very function whose point is
  that it computes none. Rejected: hashing the chunk's own bytes — it would
  make an inline line reproducible, at the cost of the rule above having an
  exception.
- **A data directory that will not resolve refuses the evaluation, before
  anything is published.** A misconfigured directory is a fact about the
  install, and running a chunk anyway would leave a result nobody can trace.
- **An append that fails after the chunk ran is a `tracing::warn!` and
  nothing more.** The chunk is not un-run by a record that could not be
  kept, and refusing at that point would hide a result the caller asked for
  behind a failure at something else.
- **The path is recorded as this build resolved it**, with backslashes, and
  not respelt to match the forward slashes in the sample above. A second
  spelling would name a path the reader never opened.
- **`budget` is the number the call asked for, not the wire's spelling of
  it.** The sample above shows `"instructions=1000000"`, which is how a
  ceiling is written into a header; on a line of the record it would be a
  count nobody can compare without first splitting a string on `=`. The field
  holds the number as it was asked for, and `null` where the call named none.
  Rejected: copying the header's text — it matches the sample, at the cost of
  making the one field on the line anybody would sort by the only one that
  has to be parsed twice.
- **The line is written from one seam.** Both eval verbs come through
  `tools::one`, and the record is appended there, so it can neither be
  written twice nor be left off one of them.

## Consequences

A file evaluation refused before a byte of it was read writes nothing, and
that falls out of the ordering — every refusal above `source::read` returns
first — rather than out of a guard that could rot apart from the code it
guards. A later reader of `eval_file` who moves the data directory's
resolution upward silently breaks it; the control in `docs/mutations.md` is
what notices.

A run whose record could not be appended is a run with no provenance, and
nothing tells the caller so beyond a warning on stderr. That is accepted
knowingly. It would be worth revisiting if the record ever becomes something
a consumer reads back rather than something a person reads afterwards.

The server now creates `%LOCALAPPDATA%\dcs-mcp` on the first evaluation on a
machine where nothing was ever installed. The containment rule still applies,
so it can never land inside a DCS tree.

Inline lines being unreproducible is the visible cost. If a consumer ever
needs to re-run an inline chunk from the record, the answer is to record the
chunk's bytes, not to record a digest of them — a hash on a line that names
no path would be a number nobody could check.
