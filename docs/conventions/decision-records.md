# Decision records

`docs/specs/bridge.md` and `docs/specs/mcp.md` are frozen. An architecture
decision record is where the build goes somewhere they did not. The plan is not
frozen, so a change to build order is an edit to `docs/PLAN.md` rather than a
record.

A record states one choice and why. Records live in `docs/decisions/`, numbered
`NNNN-slug.md` and cited as `ADR NNNN`. Numbers are four digits, sequential,
and never reused or renumbered. Copy `docs/decisions/TEMPLATE.md` to start one.

A record is written once. The one edit it accepts afterwards is its `Status`.

## Format

Michael Nygard's four headings, and no others:

```
# ADR 0003: Ship the wider policy-gate union

## Status
## Context
## Decision
## Consequences
```

`Status` is exactly `Accepted` or `Superseded by [ADR MMMM](MMMM-slug.md)`.
There is no draft status: a record is written once the decision is made.

`Context` names the specification section the decision departs from and quotes
it, rather than paraphrasing. Retrieve the prose with `tools/spec.sh read`. It
says nothing when the documents said nothing on the subject.

## Superseding

Writing a record that replaces an earlier one means two edits in one commit.
The new record's `Context` names the old one and says what changed to justify
revisiting it. The old record's `Status` becomes `Superseded by [ADR
MMMM](MMMM-slug.md)`, and nothing else in it is rewritten — it stays as the
record of what was decided, and why, at the time.

Nothing enforces the pair. Check it with
`grep -H '^Superseded' docs/decisions/*.md`.

## When to write one

When the reasoning would otherwise be lost: a departure from what a
specification says, a risk knowingly accepted, an option rejected for a reason
someone will question later. A change with one obvious answer needs no record.

**A probe answer is a record.** Both specifications call several figures
provisional pending a measurement, and Stage 9 is where those measurements
happen. The result belongs here rather than in a document nobody is
maintaining. The dormant frame time, the round-trip percentiles, whether the
editor can be told from the menu — each is a record when it lands.

Say in `Consequences` what would reopen the decision, when something would.

## When not to write one

**"The reasoning would otherwise be lost" is the whole test.** Reasoning that
already sits somewhere a reader will meet it is not lost, and a record that
repeats it is a second copy to keep in step:

- a policy the project operates under belongs in `CLAUDE.md`, which is loaded
  every session; a record restating it is read by nobody;
- a choice the plan already carries, with its revisit condition, stays in
  `docs/PLAN.md` — DR-1 and DR-2 are there for that;
- why a script does something unobvious belongs in that script's header, where
  the person changing it is already looking.

Numbers are permanent and never reused, so a record written for the sake of
having one spends a number the project cannot get back. Six were written
before the first line of product code and four of them said what `CLAUDE.md`,
the plan and two comments already said; they were deleted and the series reset
before any of it mattered. That reset was a one-time correction at commit
three — from here the numbers stand.

## Reading them

Newest first. A later record that contradicts an earlier one wins, names it in
its `Context`, and says why.
