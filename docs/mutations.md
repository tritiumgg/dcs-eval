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

**Which rows may hold an entry is wider than which rows owe one.** Stages 3–6
are what the coverage figure below is summed over, and so what is owed an
entry. An entry filed under any row in Stage 3 or later is accepted, because a
task past Stage 6 that builds a control writes its entry in the same pull
request like every other, and a gate calling that entry stray would make the
rule impossible to follow. Stages 0–2 are the one place an entry is refused:
the coverage figure declares them out of scope with a hand count, and an entry
filed under a row there would be counted once by hand and once by the sweep.

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
- two blocks may name the same file, where a control moves a line from one
  place to another; their hunks are applied together in one rewrite;
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
- reddens: `line 47 of the body is line 47 of the message`

```sweep-edit executor/DcsEvalExecutor.lua
-   local chunk, why = loadstring(req.body, chunkname)
+   local chunk, why = loadstring("\n" .. req.body, chunkname)
```

### result/table-stringified

- task: T19
- command: `mise exec -- lua5.1 tools/harness.lua executor/result`
- reddens: `nil: an empty body`
- note: the plan cell names the type-only rule. What goes red is the check
  that a table comes back with an empty body, which is where that rule is
  observable from outside.

```sweep-edit executor/DcsEvalExecutor.lua
-   return kind, ""
+   return kind, tostring(value)
```

### result/ceiling-cut-not-refused

- task: T19
- command: `mise exec -- lua5.1 tools/harness.lua executor/result`
- reddens: `over: every header is present`

```sweep-edit executor/DcsEvalExecutor.lua
-   if #body > ceiling then
-     return "oversize", ok and "result" or "error message", string.format("%d", #body)
-   end
+   if #body > ceiling then
+     body = body:sub(1, ceiling)
+   end
```

### dostring/nil-refusal-as-empty-ok

- task: T20
- command: `mise exec -- lua5.1 tools/harness.lua executor/dostring`
- reddens: `refused: every header is present`

```sweep-edit executor/DcsEvalExecutor.lua
-   local answered = dostring_in(state, chunk)
+   local answered = dostring_in(state, chunk) or ""
```

### a_do_script/payload-not-string-admitted

- task: T21
- command: `mise exec -- lua5.1 tools/harness.lua executor/a_do_script`
- reddens: `unshifted: under stage a_do_script`

```sweep-edit executor/DcsEvalExecutor.lua
-   .. 'if type(payload) ~= "string" then return "a_do_script\\n\\n\\nslot 1 is " .. type(lead) .. " and slot 2 is " '
+   .. 'if false then return "a_do_script\\n\\n\\nslot 1 is " .. type(lead) .. " and slot 2 is " '
```

### a_do_script/sacrificial-zero-dropped

- task: T22
- command: `mise exec -- lua5.1 tools/harness.lua executor/a_do_script-shift`
- reddens: `string: a lone value through the shift is answered`

```sweep-edit executor/DcsEvalExecutor.lua
-   .. WRAP_HEAD .. "tonumber(count)" .. WRAP_MID .. "body, chunkname" .. WRAP_TAIL .. ", 0"
+   .. WRAP_HEAD .. "tonumber(count)" .. WRAP_MID .. "body, chunkname" .. WRAP_TAIL
```

---

## Stage 4 — budgets and crash safety

### tick/cpu-ms-dropped

- task: T23
- command: `mise exec -- lua5.1 tools/harness.lua executor/tick-budget`
- reddens: `ping: the reply carries cpu_ms`

```sweep-edit executor/DcsEvalExecutor.lua
-     { "cpu_ms", charged },
```

### instr/zero-budget-admitted

- task: T24
- command: `mise exec -- lua5.1 tools/harness.lua executor/instr-budget`
- reddens: `hook, INSTRUCTION_BUDGET = 0: no namespace is published`

```sweep-edit executor/DcsEvalExecutor.lua
-     if type(n) ~= "number" or not (n >= 1 and n <= MAX_HOOK_COUNT) or n % 1 ~= 0 then
+     if type(n) ~= "number" or not (n >= 0 and n <= MAX_HOOK_COUNT) or n % 1 ~= 0 then
```

### fence/foreign-for-runs-anyway

- task: T25
- command: `mise exec -- lua5.1 tools/harness.lua executor/fence`
- reddens: `on a tick: no chunk was compiled`
- note: the mutation takes the whole arm out, which is what "running the chunk
  anyway" costs here: a foreign `for` is no longer answered `stale-session`
  either. So a green run vouches for the arm being watched at all, not for
  each half being watched separately.

```sweep-edit executor/DcsEvalExecutor.lua
-     elseif headers["for"] ~= E.stamp then
+     elseif false then
```

### events/killer-read-from-the-wrong-end

- task: T26
- command: `mise exec -- lua5.1 tools/harness.lua executor/events`
- reddens: `reader: the last record left open, not the first`
- note: the mutation is in the oracle, not the executor. The reader is kept
  apart from the code it judges on purpose, and it is the thing a synthetic
  unbalanced file is fed to, so it is the thing this control watches.

```sweep-edit tools/harness/crasher.lua
-   return open[#open]
+   return open[1]
```

### echo/op-uncut-in-the-executor

- task: T53
- command: `mise exec -- lua5.1 tools/harness.lua executor/ping`
- reddens: `long op: the op it names is cut at eighty bytes and three dots`

```sweep-edit executor/DcsEvalExecutor.lua
-   return reply(req.id, "bad-request", nil, "unknown op: " .. excerpt(req.headers.op))
+   return reply(req.id, "bad-request", nil, "unknown op: " .. tostring(req.headers.op))
```

### echo/state-uncut-in-the-executor

- task: T53
- command: `mise exec -- lua5.1 tools/harness.lua executor/eval-hook`
- reddens: `long unknown: the state it names is cut at eighty bytes and three dots`

```sweep-edit executor/DcsEvalExecutor.lua
-     return reply(req.id, "unsupported", nil, excerpt(state) .. " is not a state this host serves")
+     return reply(req.id, "unsupported", nil, tostring(state) .. " is not a state this host serves")
```

### echo/op-uncut-in-the-standin

- task: T53
- command: `mise exec -- cargo test -p dcs-eval standin`
- reddens: `an_unknown_op_and_a_miscased_one_are_bad_requests`

```sweep-edit crates/dcs-eval/src/standin.rs
-             other => Answer::refusal("bad-request", format!("unknown op: {}", excerpt(other))),
+             other => Answer::refusal("bad-request", format!("unknown op: {other}")),
```

### echo/state-uncut-in-the-standin

- task: T53
- command: `mise exec -- cargo test -p dcs-eval standin`
- reddens: `an_eval_is_answered_as_a_chunk_that_returned_nil_and_refused_as_the_executor_refuses`

```sweep-edit crates/dcs-eval/src/standin.rs
-                 format!("{} is not a state this host serves", excerpt(state)),
+                 format!("{state} is not a state this host serves"),
```

---

## Stage 5 — dormancy and arming

### dormant/per-call-closure

- task: T27
- command: `mise exec -- lua5.1 tools/harness.lua executor/dormant`
- reddens: `the hundred thousand dormant frames allocate nothing`

```sweep-edit executor/DcsEvalExecutor.lua
-   return function(...)
-     local ok, err = pcall(body, ...)
+   return function(...)
+     local ok, err = pcall(function() return body() end)
```

### arming/disarm-lists-after-removing

- task: T28
- command: `mise exec -- lua5.1 tools/harness.lua executor/arming`
- reddens: `the client found the arm file gone and put it back`

```sweep-edit executor/DcsEvalExecutor.lua
-   os.remove(E.arm)
```

```sweep-edit executor/DcsEvalExecutor.lua
-   record("disarm|" .. E.stamp .. "|" .. E.tick)
+   os.remove(E.arm)
+   record("disarm|" .. E.stamp .. "|" .. E.tick)
```

### heartbeat/dormant-keeps-beating

- task: T29
- command: `mise exec -- lua5.1 tools/harness.lua executor/heartbeat`
- reddens: `beat intervals wrote nothing`

```sweep-edit executor/DcsEvalExecutor.lua
-     return
-   end
-   began = nil
+     local asleep = rawget(rawget(_G, "os"), "time")()
+     if asleep - E.beat_at >= HEARTBEAT_S then
+       heartbeat(asleep)
+     end
+     return
+   end
+   began = nil
```

---

## Stage 6 — the client library

### paths/resolution-left-textual

- task: T30
- command: `mise exec -- cargo test -p dcs-eval paths`
- reddens: `containment_sees_through_an_8_3_short_spelling`
- note: four checks go red together, the short spelling and the junction among
  them, and one about a path no part of which resolves. One `canonicalize`
  answers all four.
- folds: 2 — the 8.3 short spelling and the junction. The plan names them
  separately and they are one edit here: both are undone by the same
  `canonicalize`, so nothing distinguishes a runner that defeats one from a
  runner that defeats the other.

```sweep-edit crates/dcs-eval/src/paths.rs
-         match fs::canonicalize(&head) {
+         match Ok::<PathBuf, io::Error>(head.clone()) {
```

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

### readers/absent-armed-defaulted

- task: T31
- command: `mise exec -- cargo test -p dcs-eval readers`
- reddens: `heartbeat_refuses_an_absent_header_naming_it`
- note: the absent-header check reddens beside the tri-state one, because
  `one_of` is what refuses both and the mutation takes its refusal away.

```sweep-edit crates/dcs-eval/src/readers.rs
-             armed: one_of(h, "armed", "yes", "no")?,
+             armed: one_of(h, "armed", "yes", "no").unwrap_or(false),
```

### wait/dormant-age-as-staleness

- task: T54
- command: `mise exec -- cargo test -p dcs-eval wait`
- reddens: `a_dormant_heartbeat_an_hour_old_is_pending_waking_while_the_send_is_young`

```sweep-edit crates/dcs-eval/src/wait.rs
-         Some(b) if b.armed => {
+         Some(b) if b.armed || !b.armed => {
```

### status/round-trip-issued

- task: T32
- command: `mise exec -- cargo test -p dcs-eval status`
- reddens: `status_costs_the_executor_nothing`
- note: status is handed a path and not a session, so it has no way to make a
  round trip at all. What is mutated is the only write it could make: the arm
  file it stats. The arm-file check reddens beside the costs-nothing one.

```sweep-edit crates/dcs-eval/src/status.rs
-     let (arm_file, undecided) = arm_of(arm, std::fs::metadata(arm).map(|_| ()));
+     let (arm_file, undecided) = arm_of(arm, std::fs::write(arm, b"").and_then(|()| std::fs::metadata(arm)).map(|_| ()));
```

### pipeline/yielded-in-arrival-order

- task: T33
- command: `mise exec -- cargo test -p dcs-eval pipeline`
- reddens: `pipeline_yields_every_reply_in_id_order_when_the_standin_answers_in_order`
- note: the plan cell names the backwards stand-in. What was observed is that
  the in-order baseline reddens too — the mutation drains the window from
  the wrong end rather than merely yielding what arrived first.

```sweep-edit crates/dcs-eval/src/pipeline.rs
-         let head = match self.flight.front() {
+         let head = match self.flight.back() {
```

### file_refusals/file-opened-before-containment

- task: T34
- command: `mise exec -- cargo test -p dcs-eval file_refusals`
- reddens: `a_held_file_outside_every_root_is_refused_without_being_opened`
- note: the plan cell says the fixture is a path the process could not read
  anyway, and that is the whole of it: the file is held open elsewhere, so a
  read placed before the judgement turns three containment refusals into open
  errors.

```sweep-edit crates/dcs-eval/src/file.rs
-     roots.judge(real)?;
+     let _peek = fs::read(real.as_path()).map_err(|source| FileRefusal {
+         path: real.clone(),
+         kind: Refusal::Stat(source),
+     })?;
+     roots.judge(real)?;
```


### file_refusals/ceiling-dropped

- task: T34
- command: `mise exec -- cargo test -p dcs-eval file_refusals`
- reddens: `refuses_a_file_one_byte_over_the_file_ceiling_naming_the_limit_and_the_size`

```sweep-edit crates/dcs-eval/src/file.rs
-     if total > h.max_request_bytes {
+     if total > u64::MAX {
```

### file_source/shebang-removed-not-blanked

- task: T55
- command: `mise exec -- cargo test -p dcs-eval file_source`
- reddens: `a_bom_shebang_crlf_file_raises_on_line_forty_seven`

```sweep-edit crates/dcs-eval/src/source.rs
-                 body.drain(..keep_from);
+                 body.drain(..nl + 1);
```

### file_source/crlf-converted

- task: T55
- command: `mise exec -- cargo test -p dcs-eval file_source`
- reddens: `a_plain_file_is_shipped_byte_for_byte`

```sweep-edit crates/dcs-eval/src/source.rs
-     if body.is_empty() {
-         return Err(refusal(Refusal::Empty { bom }));
+     body.retain(|&b| b != b'\r');
+     if body.is_empty() {
+         return Err(refusal(Refusal::Empty { bom }));
```

### watch/handle-left-open

- task: T56
- command: `mise exec -- cargo test -p dcs-eval watch`
- reddens: `a_wait_that_returned_holds_no_handle_on_the_reply_directory`

```sweep-edit crates/dcs-eval/src/watch.rs
-                 *changes = Some(observer);
+                 *changes = Some(observer);
+                 std::mem::forget(Changes::open(res).ok());
```

### reads/unlisted-name-sent

- task: T35
- command: `mise exec -- cargo test -p dcs-eval game_reads`
- reddens: `a_name_that_is_merely_unlisted_is_refused_too_because_the_rule_is_a_list_and_not_a_blocklist`

```sweep-edit crates/dcs-eval/src/reads.rs
-         if !READS.iter().any(|r| r.callee() == callee) {
+         if !READS.iter().any(|r| r.callee() == callee) && callee.is_empty() {
```

### game/unanswered-axis-defaulted

- task: T36
- command: `mise exec -- cargo test -p dcs-eval game_state`
- reddens: `every_axis_is_decided_by_its_own_evidence`

```sweep-edit crates/dcs-eval/src/game.rs
-         Some(Answer::Unanswered { why }) => Err(Why::Unanswered { why: why.clone() }),
+         Some(Answer::Unanswered { .. }) => Ok(false),
```

---

## Stage 8 — the installer and embedding

### embed/stale-embedded-copy

- task: T42
- command: `mise exec -- cargo test -p dcs-mcp embed`
- reddens: `the_embedded_bytes_are_the_repository_s_executor`
- note: the bytes the binary carries are no longer the repository's executor,
  which is the shape the plan cell names, and the check says so — "the embedded
  copy is not the repository's executor". Only that one check goes red: the
  hash on record and the file on disk are both untouched, and the decoy is LF
  like everything else here, so the carriage-return check stays green too.

```sweep-edit crates/dcs-mcp/src/embed.rs
- pub const EXECUTOR: &[u8] = include_bytes!("../../../executor/DcsEvalExecutor.lua");
+ pub const EXECUTOR: &[u8] = include_bytes!("../../../tools/harness/executor/interop.lua");
```

### embed/current-hash-absent-from-shipped-list

- task: T42
- command: `mise exec -- cargo test -p dcs-mcp embed`
- reddens: `the_embedded_release_s_hash_is_in_the_shipped_list`
- note: a deletion. The list no longer holds the bytes the binary is actually
  carrying, which is what would later let the installer mistake this project's
  own file for a stranger's. It is why the list holds the digest as a literal
  of its own rather than as a reference to the constant: written that way there
  would be no list line to delete and the check could never be watched failing.
  The anchor is the indented, comma-terminated list line; the same digest on
  the constant's own line is a different line and is left alone.

```sweep-edit crates/dcs-mcp/src/embed.rs
-     "2a66399b06e4c14141179e3769d6180e9bf2e85ae27047a901c27d5f083ad871",
```

### locate/ambiguity-picked

- task: T43
- command: `mise exec -- cargo test -p dcs-mcp locate`
- reddens: `two_variants_are_an_ambiguity_carrying_both`
- note: the bound is raised rather than the guard removed, so the `return`
  stays reachable and the mutated build carries no unreachable-code warning —
  the red that comes back is the assertion and nothing else. It is also the
  closer reading of the plan cell: with the bound at two, three variants are
  still refused and two are picked from, which is the defect.
  `a_named_variant_settles_the_ambiguity` stays green, because a named variant
  is filtered to one before the guard is reached; that separation is what says
  this control watches the ambiguity and not the filter.

```sweep-edit crates/dcs-mcp/src/locate.rs
-         if found.len() > 1 {
+         if found.len() > 2 {
```

---

## Out of scope

The counts below are read off `docs/PLAN.md` by hand, and nothing re-derives
them: `tools/sweep-cover.sh` checks that a Stage 3–6 row has an entry, not what
any row's count is. They are part of the figure every run prints, so each one
carries its per-row working below and can be re-counted against the plan.

The rule: one for each distinct code edit a row's `mutations:` clause names, so
a cell naming two edits counts two. An edit named to show that a control does
*not* fire — the comment byte in T16, which must leave the interop control
green — is not counted, because the sweep has no verdict for an edit that is
meant to change nothing.

**So the figure a run prints counts mutations the plan names, and not every
mutation this build has seen go red.** Several sessions broke more than their
cell asked for: `docs/STATE.md` records twelve mutations red for T35 and eight
for T36, where each cell names one, and five for T53, where the cell names
four. Those extra proofs are in neither figure — not swept, not out of scope,
not counted at all — and nothing would notice one of them going quiet, which
is the same gap one level down that this whole file exists to close. A row
that wants one of them re-run names it in `docs/PLAN.md` first; it then gets
an entry here like any other, and both figures move.

### out/stages-0-to-2

- out-of-scope: Milestone A, closed before this runner existed, and several of
  its mutations are not source edits at all — a second interpreter binary, a CI
  step removed, a workspace member removed. No plan row asks for a sweep over
  them, and one is owed.
- controls: 25
- breakdown: T01–T06 one each, 6; T07 three refusals, 3; T08, T09, T10 two
  each, 6; T11 one, 1; T12 one, 1; T13 two, 2; T14, T15 one each, 2; T16 two
  (the comment byte uncounted), 2; T17 two, 2 — the `superseded` half its cell
  defers is not counted, because no row has claimed it yet.

### out/stages-7-to-9

- out-of-scope: not swept. Stages 7 to 9 are the MCP server, the installer and
  the live proofs; the coverage figure is summed over Stages 3 to 6 and does
  not reach them, and the last of them needs a running game rather than a
  runner. Stage 8 has begun to be built, so what keeps the rows below here is
  the figure's scope and not an absence of code to mutate.
- controls: 22
- breakdown: T38, T39, T58, T59 one each, 4; T37, T40, T41, T60, T44,
  T61, T46 two each, 14; T45 three, 3; T52 one, 1. Stage 9's remaining rows name
  no mutation and are owed none. This figure falls as Stages 7 and 8 are built
  and their rows move into the inventory proper.

### out/the-runner-itself

- task: T57
- out-of-scope: the sweep cannot sweep itself — a mutation of the runner would
  be applied by the runner it broke. Its own two mutations are proved by
  `tools/sweep-test.sh`, which drives a copy of it over a sandbox inventory.
- controls: 2
