# ADR 0036: A path with a byte past ASCII stops the load, and that is the shipped limit

## Status

Accepted

## Context

`bridge.md` §7.1 on header values:

> Header values are ASCII and **may not contain CR or LF**; a writer refuses one
> rather than escaping it, because a value carrying a newline would inject a
> header (`prior:pipeline/src/bridge/protocol.ts:8-12`, `:125-129`). `\r\n` in
> the header block is normalised; the body is never touched.

The handshake carries seven paths. `install_guard` is the DCS install and
carries no user name; `output` lies under `Saved Games`, `lfs_tempdir` under
`%TEMP%`, and the transport with its `req`, `res` and `arm` under one or the
other (ADR 0029), so a Windows user name with a character past ASCII puts
such a byte in `output` at the least, whichever transport the load took, and
one header is enough to stop it. Neither
specification says what the wire does with one; `docs/audit.md` holds it as
not settled. The executor frames every handshake value through the same
check, so it refuses its own handshake and stops the load with the header
named in `dcs.log` — a visible failure rather than a file the client's parser
would refuse. The client parses a header as the executor does, the
maintainer's call at T13. What stayed open was the writer's side: a spelling
for such a path on the wire.

## Decision

There is no spelling. A user name with a character past ASCII is a limit this
release ships with: the executor refuses the load and names the header, the
README says so under Install, and nothing escapes, encodes or transliterates
a path.

Alternatives: percent-encoding header values past ASCII — rejected, because
it changes the wire's behaviour in the executor, the client's parser and the
harness at once, needs a decoding rule on every reader and a control on each
side, and serves an install nobody has, which is not a day's work at the end
of a project whose wire is proven; a UTF-8 exemption for the six path headers
under the profile alone — rejected, because a rule that holds for six headers
and not the others is the kind of exception the framer exists to refuse.

## Consequences

An install under such a user name has no session and `verify` says
`waiting`; the one line that says why is in `dcs.log`, and the README is the
only place a user is told beforehand. `docs/audit.md`'s row points here.

*Revisit if* a user reports one. The encoding, if it is ever built, is a new
record superseding this one, and it lands on both sides of the wire in one
pull request with its interop control.
