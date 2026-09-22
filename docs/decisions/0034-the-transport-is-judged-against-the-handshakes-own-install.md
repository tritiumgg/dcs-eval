# ADR 0034: The transport is judged against the handshake's own install

## Status

Accepted

## Context

`bridge.md` §8 says which transports the client refuses to write into:

> It
> is refused when relative, when inside the install (supplied by `--install` or `DCS_API_DCS_INSTALL`,
> since the consumer cannot derive it from a user profile), and when inside `Saved Games` but not
> under a `Logs` segment, written as *inside writedir and not inside writedir/Logs* so
> `Logs\..\Config` is refused (`prior:pipeline/src/bridge/paths.ts:93-115`, D36). Containment folds
> case, resolves `..`, and requires a segment boundary, because a byte-prefix test let all three
> through once (D20).

The build has no `--install` and no `DCS_INSTALL`: nothing it does needs the
install, `install` and `uninstall` work in the Saved Games tree, and ADR 0026
retired the one rule that would have used one. The relative and Saved Games
halves stand on what the client already has, `paths::resolve` and
`--saved-games`/`--variant`. The install half has no install to stand on.

ADR 0014, since superseded by ADR 0026 for other reasons, rejected the
alternative taken here for the file-evaluation roots: "The install is taken
from the handshake's `install_guard`. A refusal that depends on whether a
handshake has been read is not a rule."

## Decision

The install half judges the transport against the handshake's own
`install_guard`, the executor's `lfs.currentdir()`, where it resolved, and no
flag is invented for it.

- A handshake whose transport lies inside the install its own executor guarded
  against contradicts itself, and is refused before anything is written.
- Where `install_guard` did not resolve, the install half is not judged. The
  Saved Games half does not read the handshake's roots and is judged whatever
  `install_guard` says.
- ADR 0014's objection does not carry over. There, a rule on a path the
  caller chose would have fired or not depending on whether a handshake had
  been read. Here the path judged is the handshake's own, and no write happens
  without reading it: every call that publishes resolves the handshake first,
  so the rule has its root at every point it is asked.

Rejected:

- **A `--install` flag, or `DCS_INSTALL`.** Configuration for one refusal,
  which a user would have to keep true across every install they move, and
  which is absent in every setup that does not set it — the same case as an
  unresolved `install_guard`, reached more often.
- **Deriving the install from the registry or the process's image path.** A
  guess about which install is running, where the executor has already said.
- **No install half.** The client would then write wherever a handshake with
  a sane `install_guard` pointed inside it, which is the one inconsistency the
  client can see for itself.

## Consequences

A handshake corrupted in both `transport` and `install_guard`, so that the two
agree, gets past the install half. The executor refuses the same places for
its own writes, so the client's check is a second line, not the only one.

Reopened by a flag or setting that supplies the install for another reason:
the rule would then judge against that too.
