# Working state

**Last updated:** 2026-09-09

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

Nothing. T10 is on its branch, waiting on its pull request; T11 has not started.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T10** — the parser: `request: 261 checks`, both hosts, under a sandbox. The
  envelope read back, `admit` from a path to a request or a `bad-request` on disk; eight mutations seen red.
- **T09** — the framer: `framer: 104 checks`, both hosts, under a sandbox. The
  envelope's bytes, publish by rename read off an operation log, a 300 KiB request refused unread; six mutations seen red.
- **T08** — the session: `session: 133 checks`, both hosts, under a sandbox. The
  stamp, the directories made, siblings swept, a held one left and logged; no `os.getpid` stops the load.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T11** — the handshake writer, `executor.txt` under the output
directory: every field of the specification's files table (`spec.sh read
BRIDGE 7.2`), `protocol: 2`, rewritten in place at load through `publish`.
Done when `lua5.1 tools/harness.lua executor/handshake` asserts every
required key present and `protocol: 2`; mutation: dropping `transport` or
`stamp` reddens the key-presence check. Needs T08. A field whose counter is
not built yet is a choice: a constant now, or absent until its task.

**An agent verifies** it: the harness loads the executor over a sandbox and
reads the file back, the way `executor/session` does.

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
- **`tick` and `cpu_ms` are absent from replies until T12 and T23.** The
  specification lists both as always present; `reply` reads the session
  headers off the namespace, so each lands when its counter does. The interop
  control at T16 must not read their absence as a defect.
- **The held sibling is a held file.** The sweep's "cannot be removed" path is
  proved with a file the suite keeps open; a client's directory handle from
  `ReadDirectoryChangesW` is only seen at Stage 9, with the real client.
- **A wrong `for` and an unknown op are admitted.** `admit` refuses a missing
  stamp or op and passes a wrong stamp (the fence, T25) and an unknown op
  (the op table, T12) through; the suite pins both until each task lands.
- **A read-but-unremovable request stays on the disk.** `admit` answers it
  `error` once per call; the tick loop (T12) must not take that name again.
- **`docs/PLAN.md`'s DR-1 and DR-2 are still the record for what they cover** —
  one repository, and Windows as the target. Neither was copied into
  `docs/decisions/`; a record that restates the plan is a second place to keep
  in step.
