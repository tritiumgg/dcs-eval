# ADR 0039: A pending capture takes its directory from the handshake

## Status

Accepted

## Context

`screenshot.md` §2.5 gives a `pending` capture a directory:

> | `pending` | the executor did not answer within the wait | the id to collect
> under and the phase, as `mcp.md` §2.4 has it, plus the directory and the name
> the capture would take. Not an error |

§2.2 says where the directory comes from:

> The writedir comes back so the directory is the executor's own answer rather
> than the client's arithmetic over `--saved-games` and `--variant`.

A `pending` is the case where nothing came back. The reply that carries the
write directory is the reply that did not arrive, so the specification asks
for a directory in the one answer that has no source for it.

## Decision

A `pending` takes its directory from the executor's own layout, read off the
handshake it already has. The executor puts its output at
`<lfs.writedir()>\Logs\DcsEval\<host>`, as its source says where it decides
that. So the client takes the handshake's `output`, checks that its last
three components are `Logs`, `DcsEval` and the handshake's own host (case
folded), removes them, and appends `ScreenShots`. An output of any other
shape gives no directory, and the `pending` says so with `None`.

Alternatives:

- leaving the directory out of a `pending`: rejected, because §2.5 asks for
  it, and a caller who collects needs somewhere to look;
- deriving it from `--saved-games` and `--variant`: rejected, because §2.2
  rules that arithmetic out, and neither flag reaches the client library;
- publishing a second request for `lfs.writedir()` alone: rejected, because
  it would wait on the same executor that did not answer the first.

## Consequences

The directory in a `pending` is still the executor's answer, given at load
rather than in the reply. But it is the handshake's `output` as the client
resolved it: short names expanded, junctions followed, the on-disk case. An
`ok` reply's directory is `lfs.writedir()` exactly as DCS returned it. On an
install whose Saved Games is reached through an 8.3 name, a junction or a
redirected folder, the two answers can spell one directory two ways. A
caller comparing them as strings will see two directories.

The tool built over this has to put a `None` directory into words: an output
that is not where the executor puts it. That happens only when the
handshake was not written by this executor.

*Revisit if* the executor ever moves its output out of
`Logs\DcsEval\<host>`: this derivation would then answer `None` for every
`pending`. No test here would notice, because the stand-in's output is
wherever a test opens it.
