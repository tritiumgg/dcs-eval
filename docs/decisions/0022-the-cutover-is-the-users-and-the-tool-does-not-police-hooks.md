# ADR 0022: The cutover is the user's, and the tool looks for no hook but its own

## Status

Accepted

## Context

`mcp.md` §5.1 puts the prior project's files inside the installer's placement
step:

> If a file is already there: its hash is one this project ever shipped —
> an upgrade, replaced by rename with the old bytes parked under
> `<data-dir>\parked\<utc>\Scripts\Hooks\`; any other hash — refused and named,
> unless `--replace`, which parks it the same way. Nothing is deleted, ever.
> The prior project's `DcsApiEval.lua` and `DcsApiExport.lua`, if present, are
> named as a second bridge that would register callbacks twice
> (`bridge.md` §6.1) and parked under the same rule.

§5.3 asks the same of the report:

> Reads, and writes nothing: the hook file's hash against the embedded
> release's; the `Export.lua` line present exactly once; no other
> `DcsApi*.lua` in `Hooks\`

ADR 0001 carried both forward and went one further, ending its `Decision`
before the two rejections with "a plan task
proves the two never run at once, because a check that says so is worth more
than an instruction that asks for it." That task is T52, and the build
answered §5.1 with `InstallError::Incumbent` over a two-name list and §5.3
with a `dcsapi` entry in `verify`'s stray prefixes.

What changed is the maintainer's reading of whose directory `Scripts\Hooks\`
is. It is not this project's. A real one holds ten files from nine projects —
SRS, Tacview, OpenKneeboard, MarkPresets and the rest — and this tool was
given one file in it. A refusal that reads the other nine's names to decide
whether an install may proceed is this tool auditing a directory it was lent
a corner of, and it makes `install` fail on a machine where nothing is wrong
except that the user has not yet finished a tidy-up that was always theirs to
do.

The two-executors hazard §5.1 names is real and ADR 0001 states it correctly.
The disagreement is only about who acts on it.

## Decision

The cutover from `dcs-api-bridge` is manual, unassisted and unverified. The
user deletes the old executor; nothing in this binary looks for it.

`INCUMBENT`, `InstallError::Incumbent` and the directory scan behind them are
removed from `install`. `install` reads `Scripts\Hooks\` for exactly one leaf
name — its own release's, case folded — and every other file there is
invisible to it: not read, not hashed, not named, not moved. `--replace`
still answers for a file at *our* name whose hash this project never shipped,
which is the §5.1 rule this record leaves standing.

`verify`'s `STRAY_PREFIXES` loses `dcsapi` and keeps `dcseval`. The surviving
half is not a narrower version of §5.3's check; it is a different one. DCS
loads every `.lua` in the directory, so a `DcsEvalExecutor.old.lua` — a
backup, or a file a DCS update renamed — is a second copy of *this* executor
registering *our* callbacks against *our* transport root. That is this
project's own footprint, and reporting it is the tool answering for what it
put there.

Rejected: keeping the checks and softening them to warnings. A warning on
every install, on a machine where the user has already decided what lives in
their Hooks directory, is noise that teaches people to stop reading the
output — and the refusal was the only part with any force in it.

Rejected: checking for the prior project's transport root under `Logs\`
instead. A directory of stale log files polls nothing. The hook is the thing
that runs.

## Consequences

Installing this executor beside `dcs-api-bridge` now succeeds, and both poll
every frame at twice the idle cost the restructuring exists to remove. That
is a real hazard, knowingly accepted, and nothing in the build will mention
it. `README.md` keeps its "uninstall `dcs-api-bridge` first" note, which is
now the only place a user is told.

**The hazard is bounded to frame time, and that is ADR 0002's doing.** The
two projects share no writable path: ADR 0002 moved every on-disk name off
`dcs-api`, so the output tree is `Logs\DcsEval\<host>` against the
incumbent's `Logs\DcsApiBridge\`, and containment is judged at segment
boundaries rather than by prefix. The load-time sibling sweep — the one
destructive thing the executor does — lists only its own transport root, so
a foreign session directory is never a candidate for removal. Had the build
kept the specification's `DcsApi.lua` and `Logs\DcsApiBridge\`, removing
this check would have put two sweeps on one root and the accepted cost
would have been another project's live session, not milliseconds.

This is a dependency between two records rather than a property of either.
A future change that moves the names back has to revisit this one.

T52's code half becomes a removal plus one control that proves the removal
stuck: `verify/stray-prefix-aimed-at-the-wrong-project` reddens if the one
surviving prefix is ever aimed back at the other project. The rest of T52 is
live and manual: the old
executor gone by hand, this one installed, `verify` green — and the plan row
says so.

`verify` can now be green on a real machine. Under §5.3's wording it could
not: `no other DcsApi*.lua` is the narrow reading, but the build's prefix list
was the first step toward auditing the directory, and every step further would
have reddened a report against files the user wants loaded.

The mutation `install/foreign-hash-replaced-without-replace` changes
character. Its guard is now `replace`'s only reader, so the mutant leaves the
parameter unused and warns; the assertion is still what reddens it, and
`docs/mutations.md` says so rather than leaving a future reader to find the
warning and wonder.

*Revisit if* a user reports two executors polling and could not tell from the
symptoms. The honest response is a line in `verify`'s report naming what it
found at its own name and nothing else — not a return to reading the
directory.
