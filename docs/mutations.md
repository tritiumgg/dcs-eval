# The mutation inventory

Every control in this build was proved the same way: the code it watches was
broken on purpose, the check went red, the code was put back. That proof
happened once, by hand, in the session that built the control — and nothing
re-ran it. A check that quietly stopped reddening as the code moved underneath
it would look exactly like a check that still works.

This file is the record those proofs are re-run from. `tools/sweep.sh` reads
it, applies each mutation to a file it has copied first, runs the one command
that must go red, restores from the copy and reports. A control that no longer
reddens is a finding; a control whose mutation no longer applies is a finding
too, never a silent skip.

**A task that builds a control adds its entry here, in the same pull request.**
That is the only way the file stays honest. `tools/sweep-cover.sh` checks that
every Stage 3–6 plan row naming a mutation has an entry here, but it checks
presence per task, not per mutation, so a second control added to a row that
already has an entry can still go unwritten if nobody writes it.

**`reddens:` is what was observed, not what was predicted.** Where the red a
mutation produced is not the red its plan cell named, the entry says so in a
`note:`; that difference is the interesting part, because it says what the
check actually watches.

**Why this file is under `docs/`.** It has to name the task a control came
from. `tools/nospecrefs.sh` refuses a plan task ID or a specification citation
anywhere outside `docs/`, `CLAUDE.md` and `README.md`, so an inventory living
beside the runner could neither say where a control came from nor cite the
control table it was mined out of. `tools/sweep.sh` therefore carries no
inventory data at all, and control IDs are lowercase slugs because the runner
prints them.

---

## The format

One `###` heading per control. The heading text is the control's ID: a
lowercase slug, `group/what-breaks`, the group naming the area so that
`--only group/` selects the lot.

Bullets, one per line, each `- name: value`:

- `task:` — the plan row the control came from. One ID.
- `command:` — the one command that must go red, in backticks. Run from the
  repository root, through `mise exec` where it needs the toolchain.
- `reddens:` — one line, and one line only: a substring of the failing check's
  own output, as it was observed. The runner looks for it in what the command
  printed, so it is the check's name or a fragment of its message, never a
  description of it.
- `note:` — optional prose. Where the red observed is not the red the plan cell
  predicted, this is where that is said.
- `folds:` — optional, and its value begins with a number. Present where one
  entry covers several mutations the plan names separately because they cannot
  be performed apart; the runner counts the entry as that many controls, so the
  in-scope figure still matches the plan's own count.

Then one or more fenced blocks whose info string is `sweep-edit <path>`:

    ```sweep-edit executor/DcsEvalExecutor.lua
    -   local chunk, why = loadstring(req.body, chunkname)
    +   local chunk, why = loadstring("\n" .. req.body, chunkname)
    ```

Inside a block, a *hunk* is a run of `- ` lines — the anchor, taken verbatim
after the two-character prefix — followed by zero or more `+ ` lines, the
replacement. A blank line separates hunks. A hunk with no `+` lines deletes.

**Matching is exact whole-line equality and never a line number.** The anchor
run must occur exactly once in the file: zero occurrences means the code moved,
two means the anchor is ambiguous, and either way the control is reported
UNPERFORMED rather than guessed at.

Three limits follow from that, and a control needing more than the format gives
is out of scope rather than silently wrong:

- an anchor cannot contain a blank line, because a blank line separates hunks;
- a replacement line cannot begin with `- `, because it would be read as the
  start of the next anchor;
- comparison is on LF lines. A CR anywhere in the target file means no anchor
  will ever match, so the runner detects one and says so in the UNPERFORMED
  reason rather than leaving a reader staring at a mysterious zero match.
  `executor/DcsEvalExecutor.lua` is LF today and `.gitattributes` keeps it so.

## What out of scope looks like

A group that is not swept is an entry in this same file carrying
`out-of-scope:` — the reason — and `controls: N`, the number of controls it
stands for. The runner sums them into its coverage line, so what the sweep does
not cover is printed by the sweep itself rather than left to be assumed.

---

## Stage 3 — eval and line-truth

### eval/prologue-before-compile

- task: T18
- command: `mise exec -- lua5.1 tools/harness.lua executor/eval-hook`
- reddens: `FAIL  executor/eval-hook`

```sweep-edit executor/DcsEvalExecutor.lua
-   local chunk, why = loadstring(req.body, chunkname)
+   local chunk, why = loadstring("\n" .. req.body, chunkname)
```

---

## Stage 6 — the client library

### paths/segment-boundary-dropped

- task: T30
- command: `mise exec -- cargo test -p dcs-eval paths`
- reddens: `containment_stops_at_a_segment_boundary`

```sweep-edit crates/dcs-eval/src/paths.rs
-         let mut boundary = root;
-         if boundary.last() != Some(&SEPARATOR) {
-             boundary.push(SEPARATOR);
-         }
+         let boundary = root;
```

---

## Out of scope

The counts below are read off `docs/PLAN.md`: one for each distinct code edit a
row's `mutations:` clause names, so a cell naming two edits counts two.

### out/stages-0-to-2

- out-of-scope: Milestone A, closed before this runner existed, and several of
  its mutations are not source edits at all — a second interpreter binary, a CI
  step removed, a workspace member removed. No plan row asks for a sweep over
  them, and one is owed.
- controls: 23

### out/stages-7-to-9

- out-of-scope: not built. Stages 7 to 9 are the MCP server, the installer and
  the live proofs; there is no code to mutate, and the last of them needs a
  running game rather than a runner.
- controls: 13

### out/the-runner-itself

- out-of-scope: the sweep cannot sweep itself — a mutation of the runner would
  be applied by the runner it broke. Its own two mutations are proved by
  `tools/sweep-test.sh`, which drives a copy of it over a sandbox inventory.
- controls: 2
