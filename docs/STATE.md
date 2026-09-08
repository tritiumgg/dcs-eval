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

Nothing. T01 landed on `main`; T02 has not started.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T01** — the interpreter and its guard, both halves shown: `lua5.1 5.1.5`,
  and `tools/check-lua-test.sh` reddening on 5.4, LuaJIT and 5.1.4.
- **Naming** — the in-game half is the *executor*; its three on-disk names are
  this project's, not `dcs-api`'s. ADR 0002.
- **Language servers** — both pinned; `types/dcs.lua` declares the DCS globals
  so a misspelt call fails `mise run lua-lint`.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T02** — the Cargo workspace: `dcs-eval` (lib) and `dcs-mcp` (bin)
skeletons. Done when `cargo build` prints both crate names and produces
`dcs-mcp.exe`; the mutation is removing a crate from the workspace members and
watching the build redden. It needs nothing from T01. `tools/buildcheck.sh`
stops skipping the Rust gates the moment `Cargo.toml` exists, so `mise run
check` gets stricter on its own and wants no edit here.

**An agent verifies** this one end to end; it needs no DCS and no person.

## After that

- **T03** the harness runner and **T04** the state stubs, then **T05**'s CI —
  the rest of Stage 0, all developer-only.
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
- **Attribution is off, and the history carries none.** `attribution.commit`,
  `.pr` and `.sessionUrl` are `false` in `.claude/settings.json`, and the six
  commits that carried a trailer were rewritten to drop it. A session whose
  harness tells it to add one should not: the record here is deliberate.
- **`docs/PLAN.md`'s DR-1 and DR-2 are still the record for what they cover** —
  one repository, and Windows as the target. Neither was copied into
  `docs/decisions/`; a record that restates the plan is a second place to keep
  in step.
