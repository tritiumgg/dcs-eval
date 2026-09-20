# ADR 0020: A park is paired with its row by stamp, relative path and an absent destination

## Status

Accepted

## Context

`mcp.md` §5.2 says what `uninstall` owes:

> Removes exactly what `install` put there and nothing beside it: the hook file
> only when its hash is one this project shipped; the `Export.lua` line only by
> exact match including its marker, and never the lines around it; then
> restores any file `install` parked, and writes the register row
> `uninstalled`. A hook file with an unknown hash is left and named.

The store it restores from is described in §5.1, step 4:

> If a file is already there: its hash is one this project ever shipped — an
> upgrade, replaced by rename with the old bytes parked under
> `<data-dir>\parked\<utc>\Scripts\Hooks\`; any other hash — refused and named,
> unless `--replace`, which parks it the same way. Nothing is deleted, ever.

and `bridge.md` §6.3, which §5.2's neighbouring line points at, says why a
register row is an install rather than a park:

> It is registered under D128's rules — a row written *before* the copy, in a
> state named **installed** rather than **parked**, because a park is a
> run-scoped condition that must be restored before the session ends and an
> install is a standing one that must not be.

"Restores any file `install` parked" is one clause, and three things it needs
are missing. The register's five columns — stamp, action, path, digest, status
— name the file that moved but not the directory it moved into. The park store
names a path relative to *a* variant without recording which variant. And
nothing anywhere distinguishes a file that was **moved** out of the way from
one that was only **copied aside**: the build does both, and through the same
store. `register::park` moves a displaced hook out; `register::copy_aside`
takes a copy of `Export.lua` before that file is appended to, leaving the
original exactly where DCS looks for it. Both mint a directory under
`parked\<stamp>\`, both hold the file at its variant-relative path, and both
sit inside an `install` row whose `path` is the file's own.

So a restore driven by the stamp and the relative path alone puts the
pre-install `Export.lua` back on top of the file the uninstall has just edited
— undoing the line removal, and with it every line SRS or Tacview appended
since the install. That is a silent loss of somebody else's work, caused by the
very apparatus built to prevent one.

## Decision

A park is paired with its row by three things: the row's **stamp**, which names
the directories the park may be in; the row's **path relative to the variant**,
which names the file inside one of them; and the **destination not existing on
disk** at the moment that row is reached.

The stamp gives the candidate directories because minting allocates `<stamp>`,
`<stamp>-2`, `<stamp>-3` contiguously and nothing is ever deleted from the
store, so probing those names and stopping at the first that is not there
enumerates exactly that second's parks, oldest first. The relative path picks
the file within one. The absence is what tells a move from a copy: a
`copy_aside` file is still at its path, so its row is skipped; a `park`ed file
is gone from its path, so it comes back.

The absence is read per row, at the moment that row is reached, not once at the
start. Two rows naming the same path then resolve themselves — the park row is
reached first, the file comes back, the placement row that follows finds the
path occupied and leaves it alone.

Rejected:

- **A sixth register column** saying which directory a row parked into. It
  breaks every register already on disk and the parser that reads them, for a
  fact the store's own shape already carries.
- **Walking the park store alone**, restoring every file in it. Two variants
  park the same relative path and the store cannot tell them apart, and it
  would restore a park belonging to an install that is still in place.
- **Stamp and relative path alone**, without the absence. This is the defect
  above: it restores the pre-install `Export.lua` over the file the uninstall
  just wrote.
- **Deleting an `Export.lua` the removal leaves empty.** `Outcome::Created`
  writes no row, so nothing on disk proves this build created that file;
  deleting it is a guess, and the loss it risks is the one this whole apparatus
  exists against. It is left in place, empty, and the report names it.

## Consequences

The restore is idempotent for free. A second uninstall finds every destination
occupied and moves nothing, which is what `bridge.md` §6.3's "two idempotent
commands" asks for, without a flag or a second record to carry it.

A park whose original path has since been re-occupied — by the user, or by
another installer — stays in the store rather than clobbering the occupant.
That is the conservative direction, and it is visible: the store holds the file
and the register holds the row, so somebody can put it back by hand.

The absence cannot tell a file this build moved from one the user deleted
themselves. Where somebody removes their own `Export.lua` after an install, the
row's destination is absent, the `copy_aside` copy is in the store, and the
uninstall puts that copy back — resurrecting a file its owner had thrown away.
Nothing on disk distinguishes the two absences, and of the two mistakes
available this is the one that loses nothing: the file returns where it was,
under the name it had, and deleting it again is one keystroke, whereas the
other direction would leave a displaced file stranded in the store.

One ambiguity is left unresolved rather than decided: two parks holding the
same relative path for the same variant within the same second. The oldest
candidate directory holding that path wins and the rest stay in the store.
Reaching it takes a scripted double-install inside one second, and nothing in
either specification decides it.

The absence rule is load-bearing and invisible in the finished state — a
restore that ignored it would leave a tree that looks right in every test whose
parked copy happens to be the expected bytes. The check that watches it
appends a line to `Export.lua` *after* the install, so the parked copy and the
expected bytes differ; losing that line would quietly stop the check watching
anything.

What would reopen this: a prune of the park store. The contiguous probe stops
at the first missing name, so a gap in `<stamp>-N` would silently hide every
park past it. The store is documented as never deleting anything, and a change
to that has to come back here.
