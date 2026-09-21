# ADR 0029: The executor's temp directory may lie under the client's

## Status

Accepted

## Context

`bridge.md` §8 has `status()` report the executor's temp directory against the
client's:

> the running `app_version` against the
>   model's, whether `lfs.tempdir()` agreed with `os.tmpdir()`, and every problem it found

and says, a few lines on, why the client never builds the transport path from
its own:

> The transport path is read out of the handshake and never derived (D37): the client runtime's temp
> directory — Node's `os.tmpdir()` today — and the bridge's `lfs.tempdir()` have never been measured
> to agree, and the failure would be silent.

§11 lists the value as unmeasured and records the one hint there was:

> **What `lfs.tempdir()` returns inside DCS** is recorded in the handshake and reported by `status`
>   against `os.tmpdir()`, and the tree holds no committed value for it; the harness models it as a
>   directory *inside* the write directory precisely because nobody has measured it
>   (`prior:tools/harness_stubs.lua:88-94`). The only hint is a withheld `machineLocal` value shaped
>   `C:\Users\<user>\AppData\Local\Temp\DCS\…` (`prior:generated/reports/CONFLICTS.md:10`).

The build read "agreed" as "the same directory", and made anything else a
problem, so `verify` failed on it. The first live load, on DCS 2.9.29.27468 on
2026-09-21, measured the value: `lfs.tempdir()` gave
`C:\Users\<user>\AppData\Local\Temp\DCS`, a folder DCS keeps for itself inside
the temp directory this client's `GetTempPath` answers. `verify` printed the
two as a disagreement and said `not verified`, while `dcs-mcp ping` against the
same session answered `pong` — the round trip reads the transport out of the
handshake, as §8 says, and never looked at either temp directory.

## Decision

An executor temp directory that lies under the client's, at a segment boundary,
is its own answer and not a problem: `status` reports it as within the client's,
and `verify` passes. The same directory is still agreement, and a temp
directory anywhere else — including a sibling sharing the client's bytes, such
as `...\TempDCS` — is still a problem that fails `verify`. The comparison is the
resolved `contains` every other containment in the client uses, so it folds
case and resolves short names and junctions first.

The transport is still read out of the handshake and never derived, and nothing
about the round trip changes.

Rejected: gating the transport itself on lying under the client's temp
directory. The executor falls back to `<output>\rpc`, under `Logs\` in the
write directory, when its temp candidate is refused, and that transport is
legitimate and outside every temp directory.

Rejected: dropping the comparison. A temp directory outside the client's is
still worth a line: it says the two sides are not looking at one user's temp
tree, which is the silent failure §8 names.

## Consequences

The one measured value is one machine's, one build's. Another build or another
launcher handing DCS a temp directory outside the user's — a portable install,
a `TMP` override for the game alone — fails `verify` again, and so does an MCP
host that sets `TEMP` for `dcs-mcp` alone. That is the report working: the line
names both directories. Where that turns out to be a
healthy install, this decision reopens.

The harness and the stand-in still model `lfs.tempdir()` by hand; the fixtures
that want a healthy session name the client's own temp directory, and the two
within tests name a folder under it, as DCS does.
