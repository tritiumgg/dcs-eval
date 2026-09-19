# ADR 0018: Every unknown names its reason, and no axis borrows another's evidence

## Status

Accepted

## Context

`mcp.md` §3.4 is a table with one row per outcome, introduced in its own words:

> The derivation is a table in code with one row per outcome and no default arm.
> This is that table.

§3.5 says what the missing rows come to:

> - A read that errored sets its axis to `unknown: <the error, verbatim>` and
>   nothing else; no axis is filled from another axis's evidence.
> - Facts that contradict — `phase` says `sim` and `mission_name` is empty; the
>   `gui` probe is `refused` at the menu, where S4 measured it answering — yield
>   `unknown` with every fact printed, and the headline says "the facts
>   disagree", because the disagreement is itself the finding: a build moved, or
>   a read means something else in this phase.
> - A tier-2 axis with tier 2 off reads `unknown (tier 2 off)`, so an agent
>   knows the answer is purchasable and how.

The table leaves three things open that an implementation cannot leave open.

**The first is what counts as "another axis's evidence".** §3.4 names three
places where one axis is consulted by another: `pause` is decided "while
`activity: mission`", and both `session: client` and `session: single-or-host`
require "`activity: mission`". Nothing says what those consultations may do.

**The second is that §3.4 and §3.5 disagree about an errored `mission_name`.**
§3.4's `activity` row says the axis is not decided when

> `phase` says mission and `mission_name` is empty **or errored**: `unknown`,
> both shown

while §3.5's first rule says an errored read sets its axis to

> `unknown: <the error, verbatim>` and nothing else

Under §3.4 an errored `mission_name` prints the phase beside the error; under
§3.5 it prints the error alone. Both sentences are frozen and only one can be
built.

**The third is that neither section says which heartbeat is read.** §3.1 gives
`activity` as "the phase, then one read". It does not say whose phase.
`status::BeatStatus` already carries a `belongs` flag, because "a printer with
no flag to read would print another install's phase, ticks and age as this
session's", and `wait::decide` takes the heartbeat through
`this_sessions_heartbeat`, which treats a foreign stamp as nothing to read.

Two smaller gaps sit beside those. §3.1 says of the export host that it

> answers `phase` alone (`loaded | sim | stopped`, `bridge.md` §6.2): `DCS` is
> nil there (D94), so no read is possible.

and `bridge.md` §6.2's table gives the hook host `menu, load, sim, paused` and
the export host `loaded, sim, stopped` — so `sim` is a word in both. And §3.4's
`bridge` row says only "`bridge.md` §4.4's table, verbatim", while §4.4 is a
table about a request that was sent, which on a load there is not.

## Decision

Every `unknown` this derivation produces carries a reason, and no axis is
filled from another axis's evidence.

The rule, precisely: **no axis's value may be computed from another axis's
value, or from a fact this table assigns to another axis. An axis may be
*gated* by another axis only where §3.4 names the gate, and a gate may only
select among that axis's own values — including `n/a` and `unknown` — never
supply one.** A gate whose own axis is unknown produces `unknown` naming the
gate, never a value.

The specifics:

- **One `Why` enum carries every axis's reason**, while the axis *values* stay
  separate enums per axis. `Why` records which evidence failed and how; it
  never carries a value, so it cannot become a route by which one axis hands
  another an answer. This is ADR 0017's argument for `Unanswered` one level up.
- **§3.5 wins over §3.4 on an errored read.** An errored `mission_name` is
  `unknown: <the error verbatim>` alone, and the empty-string case is the
  disagreement that prints both facts. The rule that applies to every axis
  beats the one written into a single row, and the plan's done-condition for
  this task asks for the error alone.
- **The reasons are kept apart rather than collapsed.** A read that errored, a
  read that was never sent because tier 2 is off, a read that was never sent
  because the session is loading, a heartbeat that is absent, one that would not
  parse, one carrying another session's stamp, one from another host, and a
  handshake that would not parse are eight findings and eight arms. Two of them
  render as §3.5's own lines, `unknown: <error>` and `unknown (tier 2 off)`.
- **This derivation reads only a heartbeat whose stamp is the handshake's and
  whose host is the one the output directory is for.** The verdict is taken
  once, before any axis, and every axis reads that verdict rather than the raw
  file. A foreign or wrong-host heartbeat yields its own `Why` and no phase.
  `status::BeatStatus::belongs` and `wait::this_sessions_heartbeat` already take
  this reading; a derivation that skipped it would make `activity` definite out
  of another install's file and would disagree with both.
- **The phase word is parsed against the named host**, because `sim` is in both
  vocabularies. A word the named host's vocabulary does not hold is
  unrecognised and carries its own text. On the export host every phase word,
  `sim` included, gives `activity: unknown` naming that no read is possible
  there — not a mission derived from a read that cannot exist.
- **`isMultiplayer` true with `isServer` false is unknown naming both reads.**
  §3.4 maps false → `single` and true-with-server-true → `host`, and maps this
  combination to nothing. Agreeing with the probe here would be a guess wearing
  a corroboration.
- **The probe is read before the tier-2 reads.** A `refused` probe in a mission
  is `session: client` whatever tier 2 says; the tier-2 mapping is reached only
  where the probe was reachable. Without that ordering written down, a `refused`
  probe with `isMultiplayer` true and `isServer` false has two §3.4 rows
  claiming it at once. Both are `session`'s own evidence, so neither borrows.
- **`refused` at the menu lands on `session` and leaves `activity` definite.**
  It is §3.5's second example of contradicting facts; the probe is `session`'s
  evidence, so unknowning `activity` with it would be the borrowing this record
  forbids.
- **`menu-or-editor` stands**, per §3.6's "Until one of them is measured,
  `menu-or-editor` stands". The value's name holds the indeterminacy rather than
  picking between two; it is not `unknown`, because the axis knows the game is
  at one of the two. `sim_mode` is recorded verbatim and maps to nothing, since
  no table of observed values exists.
- **The `bridge` axis is not §4.4 verbatim, and this says so plainly.** §4.4
  decides about a request that was sent. The ping is that request and its
  outcome carries §4.4's verdict, run once by `wait::decide` and not
  re-implemented here. On a load no window opens and there is no request, so the
  axis falls back to the heartbeat's own `armed` word alone. Claiming the table
  was implemented would be false.
- **A pending with no flag is not proof of an armed session.** `wait::decide`
  returns a flagless pending on three branches: armed and fresh, and *either*
  branch while loading. So the `bridge` axis reads a flagless pending as unknown
  naming it, and the flag is carried through the read's `Unanswered::Pending`
  rather than re-derived, because re-deriving §4.4's table would be two
  implementations of one frozen table.

Alternatives rejected:

- *One `unknown` with a sentence.* Prose cannot be told apart by a reader
  without prefix-matching a sentence, which is the defect `Unanswered` was moved
  off in T35.
- *Following §3.4's "both shown" for an errored read.* It contradicts the rule
  that governs every axis, and makes the phase part of an answer it did not
  decide.
- *Reading the raw heartbeat.* Cheaper by one type, and wrong on every
  two-install output directory, silently.

## Consequences

Every axis that can be unknown has more arms than values, and adding an axis
means adding its reasons too. That is the cost, and it is the point: a reader
of an unknown can tell why, and an agent can tell whether the answer is
purchasable.

The gate rule is what makes "no default arm" performable as a mutation: there
is a specific line in each axis's function that a mutation can change to fill
that axis from another's evidence, and a check that reddens when it does.

What this record does **not** decide, and who does. Everything §3.6 leaves to
the first live run stays open: whether the editor can be told from the menu,
what `DCS.getMissionName()` gives at the menu, what the callback vocabulary
really fires, and whether the five reads answer on a joined client. Every value
this derivation produces today is a stand-in's, told what to say. `session:
client` here is what a `refused` probe *means*, not what a live client does, and
§3.4's reason for letting the read beat the callback phase — that this
executor's phase reports `sim` for a mission that began paused — is itself
unmeasured on this build. Stage 9 fills all of it, and a measurement that
contradicts a mapping here reopens that mapping's row and nothing else.
