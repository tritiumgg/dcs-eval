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

Nothing. T06 landed on `main`; T07 has not started.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T06** — the load shell: `load: 79 checks` over both hosts, four non-host
  states and a raising `setUserCallbacks`; no `pcall`, or `type(DCS)` detection, reddens it.
- **T05** — the CI workflow, built at setup, closed on observation: main run
  34277046532 printed both counts; run 34278109722, the interpreter steps removed, went red at the guard.
- **T04** — the state stubs: `stubs: 149 checks`, one per surface cell, per name
  `types/dcs.lua` declares, and the behaviour; an evaluating stub reddens it.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T07** — executor containment and transport-root selection with the
`Logs\` fallback. Done when `lua5.1 tools/harness.lua executor/containment`
prints its check count; the mutations are a path inside the install, a
relative path, and `Saved Games` outside `Logs\` (`Logs\..\Config` included),
each of which must be refused, and allowing any one reddens the suite. Needs
T06. It extends `executor/DcsEvalExecutor.lua` past registration and its suite
goes beside `executor/load.lua`.

**An agent verifies** it: the harness runs off DCS under the T04 stubs, with
`host.writedir` and `host.cwd` pointed where each mutation needs.

## After that

- **Stage 0 is closed.** Stage 1 (T06–T11) is the load shell and the envelope,
  every task harness-proven off DCS.
- **Milestone A** is Stages 0–2: the wire proven off DCS, the interop control
  first and the stand-in second. The CI interop job arrives with T15.
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
- **Two gaps T06 left.** The hook guard's swallow path has no seam until a
  raising stub sits on the frame path. Whether the export state survives
  between missions is unmeasured; the load sentinel covers both answers, and
  Stage 9 can settle it.
- **`docs/PLAN.md`'s DR-1 and DR-2 are still the record for what they cover** —
  one repository, and Windows as the target. Neither was copied into
  `docs/decisions/`; a record that restates the plan is a second place to keep
  in step.
