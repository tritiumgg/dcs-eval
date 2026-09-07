# Working state

**Last updated:** 2026-09-07

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

Nothing. The repository is set up; T01 is next.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **Naming** — the in-game half is the *executor*; its three on-disk names are
  this project's, not `dcs-api`'s. ADR 0002. `docs/index.tsv` went with it.
- **Language servers** — both pinned; `types/dcs.lua` declares the DCS globals
  so a misspelt call fails `mise run lua-lint`.
- **Repository setup** — the tree, `CLAUDE.md`, the hooks, `mise.toml`, the
  two decision records. No plan task; the floor they stand on.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T01** — the pinned reference interpreter and its guard. Setup already
landed `tools/mklua.sh` and `tools/check-lua.sh`, and `mise run lua-check`
prints `lua5.1 5.1.5`. What is left is the mutation the plan names: point the
guard at a 5.4 binary and show it printing the version and exiting non-zero.

**An agent verifies** this one end to end; it needs no DCS and no person.

## After that

- **T02** the Cargo workspace and **T03** the harness runner, then **T04**'s
  state stubs and **T05**'s CI — Stage 0, all developer-only.
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
