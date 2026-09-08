# Working state

**Last updated:** 2026-09-08

The handoff between sessions. Read it first; update it before a session ends,
not only when a task finishes. Stamp the date above each time; it carries a
date and nothing else, because what changed is what the sections below are for.

**This file is loaded cold every session, so its size is a tax on all of them.**
Each section has a line budget and `tools/statecheck.sh` enforces it. Over
budget, nothing is deleted — it moves. A completion older than the last few
goes to git log. A choice with reasoning behind it becomes a decision record.
A durable fact about the project belongs in `CLAUDE.md`. A resolved
carry-forward is just deleted. One or two lines per entry, never paragraphs.

---

## In progress

Nothing. T04 landed on `main`; T05 has not started.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T04** — the state stubs: `stubs: 148 checks`, one per surface cell, per name
  `types/dcs.lua` declares, and the behaviour; an evaluating stub reddens it.
- **T03** — the harness: `selftest: 1 check`, and `tools/harness-test.sh`
  showing a suite with no checks exit 2. Suites register in `tools/harness/suites.lua`.
- **T02** — the workspace: `crates/dcs-eval` (lib) and `crates/dcs-mcp` (bin);
  `tools/buildcheck.sh` reads the member list and reddens when either is dropped.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T05** — the CI workflow: the harness under 5.1 and `cargo test`. Done
when a CI run prints both suites' check counts; the mutation is removing the
interpreter step, which must make the interop job red rather than skipped.
Needs T01–T03. `mise run check` is already what CI should run, so the
workflow is that command on a Windows runner after `mise install` and
`mise run lua-build`.

**An agent verifies** the counts by reading the run with `gh run view`. The
interop job the mutation names does not exist until T15, so only the harness
half of the mutation can be shown now; say so in the PR's "Not covered".

## After that

- **T05**'s CI closes Stage 0, all developer-only.
- **Milestone A** is Stages 0–2: the wire proven off DCS, the interop control
  first and the stand-in second.
- **Stage 9** is the critical path and cannot be shortened by parallel effort.
  Everything provable off DCS is proved before it.

## Carries forward

Things that must not be lost between sessions. Delete an entry when it is
resolved, and say where. Mark an entry only the maintainer can settle. Ten
entries at most: an eleventh means something here is finished, or belongs in
`docs/decisions/` or `CLAUDE.md` instead.

- **Cutover — the incumbent goes before this executor installs.**
  `dcs-api-bridge` puts `DcsApiEval.lua` in `Saved Games\DCS\Scripts\Hooks\`
  and polls `Saved Games\DCS\Logs\DcsApiBridge\`. This executor lands in the
  same directory; ADR 0002 is why it no longer does so under a near-identical
  name. Two hooks on one transport root is what T52 proves cannot happen.
- **Maintainer decision — when the MCP registration is swapped.** Claude Code
  still points at `dcs-api-bridge`. The swap is a config edit the maintainer
  makes, and it strands any session mid-task, so it happens at Milestone C and
  not before. ADR 0001.
- **The 0.098 ms dormant baseline is the incumbent's, on one machine.** T48
  compares against it. It was measured on DCS 2.9.28.26385, one session, and
  a figure that disagrees on other hardware is a new measurement, not a
  regression.
- **`docs/PLAN.md`'s DR-1 and DR-2 are still the record for what they cover** —
  one repository, and Windows as the target. Neither was copied into
  `docs/decisions/`; a record that restates the plan is a second place to keep
  in step.
