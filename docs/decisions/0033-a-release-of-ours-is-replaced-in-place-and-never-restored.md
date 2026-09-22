# ADR 0033: A release of ours is replaced in place, and a park of one is never restored

## Status

Accepted

## Context

`mcp.md` §5.1, step 4, says what `install` does with a file already at the
hook's name:

> If a file is already there: its hash is one this project ever shipped —
> an upgrade, replaced by rename with the old bytes parked under
> `<data-dir>\parked\<utc>\Scripts\Hooks\`; any other hash — refused and named,
> unless `--replace`, which parks it the same way. Nothing is deleted, ever.

and §5.2 what `uninstall` does with what was parked:

> Removes exactly what `install` put there and nothing beside it: the hook file
> only when its hash is one this project shipped; the `Export.lua` line only by
> exact match including its marker, and never the lines around it; then
> restores any file `install` parked, and writes the register row
> `uninstalled`.

Read together, the two make `uninstall` undo only the *last* `install`. After
installing twice — the same release again, or an upgrade — the second install
parked our first copy, and the uninstall puts that copy back: the executor is
still at its name, DCS still loads it, and it takes as many uninstalls as there
were installs to be rid of it. The maintainer met this on their own machine,
which has installed two releases in turn, so its park store and register
already hold a parked copy of ours.

## Decision

`install` parks only a file this project never shipped; `uninstall` never
restores a park whose bytes hash to a release this project shipped.

- **Install.** A hook whose hash is in the shipped list is replaced in place,
  by the same staged rename that places every release, and the register row for
  the write still records it. Nothing is parked. A foreign file at our name is
  still refused without `--replace` and parked with it. PLAN's standing rule 3
  — nothing there that is not this project's is ever lost — holds, because the
  file replaced is ours.
- **Uninstall.** When a row pairs with a park (ADR 0020) and the parked bytes
  hash to a shipped release, the park is left in the store and the row is
  treated as answered: the search for that row stops there, as it does after a
  restore. This is what makes one uninstall enough on a machine an older binary
  already parked a copy of ours on.

Rejected:

- **Marking the row handled in the register**, so a later run skips it. It
  needs a status word the register does not have, and a row can pair with the
  same park again on every run anyway: the hash of the parked bytes answers the
  question afresh each time without a record to keep true.
- **Deleting a shipped park.** Nothing under the park store is ever deleted
  (ADR 0020 names a prune as what would reopen it), and the copy costs a few
  kilobytes.
- **Restoring only the oldest park and discarding the rest.** The oldest can
  itself be a copy of ours, from an install over an install; "oldest" is not
  "not ours".

## Consequences

One `uninstall` removes the executor however many times it was installed, and
it restores every foreign file and every `Export.lua` it would have before.

A copy of ours parked by an older binary stays in the store for good, with its
register row. It is recoverable by hand like any park, and nothing restores it.

`install` reports an upgrade as replaced, with no park directory to name, and
the register holds one row for it where it held two.

What would reopen this: a release whose hash is shipped but which a user must
be able to get back after uninstalling a later one. Nothing asks for that; a
downgrade is an install of the older binary.
