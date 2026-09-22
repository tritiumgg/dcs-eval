# ADR 0037: An `--out` path is written wherever it points

## Status

Accepted

## Context

`mcp.md` §4.6, on where the command line keeps a reply:

> With `--capture`, the reply is also saved verbatim to
> `<data-dir>\replies\<id>.res`, headers and body through `latin1`, and a
> `pending` writes nothing, because a zero-byte file where a result is
> expected reads as a measured empty answer
> (`prior:pipeline/src/mcp/tools.ts:188-190`, D38). The data directory is
> never inside either DCS tree, and `save`'s containment guard applies to
> `--out` as D38 wrote it.

The containment guard the sentence names is the write side's, the one §7's
control row states:

> A handshake path, an `--allow` root and a `--out` path are refused when
> they resolve inside the install, inside `Saved Games` outside `Logs\`,
> through an 8.3 short spelling or through a junction;
> `std::fs::canonicalize`'s `\\?\` prefix is stripped before comparison and
> case is folded

The build keeps that guard on every path the client writes through on its
own account: a handshake's transport is judged against the handshake's own
`install_guard` and against `Saved Games` (ADR 0034), and `--capture` writes
through the data directory, which is judged as the install paths are. There
is no `--allow` root to judge, because ADR 0026 took the read-side rule off
`eval --file`: a chunk in the `hook` state opens any file with `io.open` in
one line, and a rule the executor itself does not keep protects nothing.
`--out` never had the guard, and the README said so on one line as *not
built*, which held open the question of whether it was coming.

## Decision

It is not. An `--out` path is written wherever it points, and where it lies
is never judged, for the reason ADR 0026 gives reads: a chunk in the `hook`
state writes the same file with `io.open` in one line, so a refusal on the
command line would guard a file the same command line can overwrite through
`eval`. The README says so where it once said *not built*.

Alternatives: judging `--out` as the transport is judged, against the
install and `Saved Games` outside `Logs\`, as §7's row asks — rejected,
because the command line is the operator's own
shell, `> path` beside it writes anywhere already, and a refusal here names
a rule for a file the operator owns while `eval` writes it anyway; judging
`--out` through the data directory's resolver as `--capture` is — rejected,
because `--out` exists to put the bytes where the caller names and a path
that has to lie under the data directory is `--capture` with a second
spelling.

## Consequences

An `--out` pointed at a file inside the install overwrites it, and that is
the operator's, as it would be from the shell. `--capture` stays judged, so
a reply kept under the data directory can never land inside either DCS tree.
The README carries the rule under *Use it from a terminal*.

*Revisit if* the command line is ever driven by something other than the
operator's own shell with `eval` withheld from it, because then `--out`
would be the one write the caller has and a guard on it would guard
something.
