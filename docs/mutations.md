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

## Stage 7 — the MCP server and CLI

### serve/diagnostic-to-stdout

- task: T37
- command: `mise exec -- cargo test -p dcs-mcp serve`
- reddens: `serve_writes_only_protocol_frames_to_stdout`
- note: the one line that decides where a diagnostic goes, which is why that
  choice sits in a module of its own. `std::io::stdout` satisfies the same
  bound as `std::io::stderr`, so the mutation compiles and the red is the
  assertion rather than the type checker. What was observed is the start-up
  line arriving on the transport: `stdout carried a line that is not a
  protocol frame`, with the log line printed whole after it.

```sweep-edit crates/dcs-mcp/src/diag.rs
-         .with_writer(std::io::stderr)
+         .with_writer(std::io::stdout)
```

### serve/client-resolved-once

- task: T37
- command: `mise exec -- cargo test -p dcs-mcp serve`
- reddens: `a_root_that_appears_between_two_calls_is_resolved_by_the_second`
- note: turns "built per call" into "built once and kept". The assertion that
  goes is `the second call resolves the root that appeared between them`. Both `Client` and
  `NoSession` derive `Clone` and hold only `Sync` fields, which is what lets a
  `static` hold one — and the reason `NoSession` renders its reason to a
  `String` rather than carrying the `io::Error` that raised it. Observed red is
  three tests, not one: `a_second_serve_over_the_same_options_resolves_independently`
  and `a_session_replaced_by_one_with_another_stamp_is_picked_up` fail beside
  it.

  The edit below caches an `Err` as readily as an `Ok`, which is the loudest
  version of the fault and not the only one. A cache that kept only successes,
  per `Serve` instance, would still be wrong the moment DCS reloads and the
  handshake names a new stamp, and it slips past both of the other two tests —
  neither of them resolves twice on one instance after a success. That is the
  third test's whole job, and it was watched failing on its own under a
  per-instance `OnceLock<Client>` before this row was written.

```sweep-edit crates/dcs-mcp/src/serve.rs
-     pub fn client(&self) -> Result<Client, NoSession> {
-         Client::resolve(&self.opts)
+     pub fn client(&self) -> Result<Client, NoSession> {
+         static ONCE: std::sync::OnceLock<Result<Client, NoSession>> = std::sync::OnceLock::new();
+         ONCE.get_or_init(|| Client::resolve(&self.opts)).clone()
```

### tools/registered-not-listed

- task: T38
- command: `mise exec -- cargo test -p dcs-mcp tools_listed`
- reddens: `tools_listed_are_exactly_the_six`
- note: the fault the cell names is a tool that registers and is not listed,
  and `ToolRouter::with_disabled` is the SDK's own way of producing exactly
  that: the route stays in the router's map and the listing filters it out.
  The mutated server holds six tools and announces five. `with_disabled` needs
  no import the file does not already carry, so the mutation compiles and the
  red is the assertion rather than the type checker; the router is filled on a
  line of its own so that line is an anchor occurring once.

  Observed red is two tests, not one. The router refuses a call on a disabled
  name as well as hiding it from the listing, so
  `tools_listed_each_answer_a_call` fails beside it with `dcs_collect is routed
  and answers: Mcp error: -32602: tool not found`. That is worth writing down:
  the two tests watch different things — one the listing, one the dispatch —
  and the SDK offers no seam that hides a route from the listing while still
  calling it, so this is as close to the cell's wording as an edit can get.
  The assertion that goes is the set comparison, which prints both lists:
  five names came back and `dcs_collect` was the one missing.

```sweep-edit crates/dcs-mcp/src/serve.rs
-             tools: Self::tool_router(),
+             tools: Self::tool_router().with_disabled("dcs_collect"),
```

### wording/oversize-worded-as-an-answer

- task: T39
- command: `mise exec -- cargo test -p dcs-mcp wording`
- reddens: `an_oversize_reply_is_a_refusal_and_not_an_empty_answer`
- note: the fault the cell names is a refusal worded like an empty result, and
  `oversize` is the sharpest case of it: the executor ran the chunk, refused
  the result whole rather than cutting it, and said so under `stage:
  oversize`. Judged in one arm of `verdict`, so the mutation is one arm, and
  the sentence sits inline in it rather than in a named constant — a constant
  would be left unused by the edit and the red would be the compiler rather
  than the assertion. The mutated build renders the reply through the
  answering path: headed `reply`, not marked an error, and nothing in it
  saying anything failed. A caller then sees a successful call whose content
  is a pile of headers, and has to infer from `stage: oversize` buried among
  them that nothing came back.

  Observed red is two tests, not one, and that is expected rather than an
  over-broad edit. The table in `every_refusing_status_reads_as_a_non_empty_refusal`
  is the plan cell's "each status", and `oversize` is one of them, so it fails
  on that row beside the named test: `assertion left == right failed: oversize
  is marked an error / left: Some(false) / right: Some(true)`. The two watch
  the same word from different angles — one the whole vocabulary, one the
  single case with its own headers and sentence — and the named one is the
  test that exists only for this control.

```sweep-edit crates/dcs-mcp/src/wording.rs
-         "oversize" => Verdict::Refused("the result was refused whole, not cut"),
+         "oversize" => Verdict::Answered,
```

### watching/handle-left-open-across-a-call

- task: T41
- command: `mise exec -- cargo test -p dcs-mcp watching`
- reddens: `a_sibling_sweep_succeeds_while_the_server_is_idle_between_two_calls`
- note: **this edit is byte for byte the one under `watch/handle-left-open`
  above, and that is deliberate.** Same injury, different observer: there the
  red is a `remove_dir` inside the `watch` module's own test, with the private
  tally to hand; here it is the next executor session's sibling sweep, from
  another crate, reached only through the path a tool call takes. A reader
  finding the two blocks identical is looking at one mechanism watched from
  both sides, not at a copy-paste slip.

  A second handle on the same directory, leaked, so that it outlives the call
  the way one cached across calls would. The leak is the instrument rather
  than the fault being modelled: an open-then-closed handle is invisible from
  outside the crate, and what the rule is about is a handle still standing
  when the next session sweeps. It fires only where `settle` ran — that is,
  only where the wait actually slept — so the observed red is two tests and
  not three: `a_sweep_is_refused_while_a_wait_is_in_flight_and_succeeds_once_it_returns`
  fails beside it on its second sweep, and
  `a_superseded_session_is_waited_on_and_swept_straight_after` stays green.
  That separation is what says the third test watches the terminal path and
  not the sleeping one. Observed in both:
  `The process cannot access the file because it is being used by another process`.

```sweep-edit crates/dcs-eval/src/watch.rs
-                 *changes = Some(observer);
+                 *changes = Some(observer);
+                 std::mem::forget(Changes::open(res).ok());
```

### watching/watch-opened-on-a-superseded-session

- task: T41
- command: `mise exec -- cargo test -p dcs-mcp watching`
- reddens: `a_superseded_session_is_waited_on_and_swept_straight_after`
- note: the watch hoisted out of the sleep and opened before the first look,
  which is the natural way to write this wrong — so it is opened even on a
  wait that answers `superseded` on its first pass and never sleeps at all.
  Leaked for the same reason as the entry above: the harm is the next
  session's sweep finding a handle, and that sweep runs after the terminal
  call has returned. `Changes` is already in scope in `wait.rs` and `s.res()`
  is the session being waited on, so the edit compiles and the red is an
  assertion.

  **The red is not exclusive, and the entry above is what tells the two
  apart.** This leaks on every wait, so all three tests were observed
  failing: the other two are
  `a_sweep_is_refused_while_a_wait_is_in_flight_and_succeeds_once_it_returns`
  and `a_sibling_sweep_succeeds_while_the_server_is_idle_between_two_calls`.
  The superseded test is the one only this mutation reddens.

```sweep-edit crates/dcs-eval/src/wait.rs
-     let mut changes: Option<Changes> = None;
+     let mut changes: Option<Changes> = None;
+     std::mem::forget(Changes::open(s.res()).ok());
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

### park/deleted-in-place-of-parked

- task: T60
- command: `mise exec -- cargo test -p dcs-mcp register`
- reddens: `a_parked_file_is_recoverable_from_its_path_relative_to_the_variant`
- note: the displaced file is removed rather than moved aside, which is the
  defect the park store exists against — the install still "succeeds" and the
  file is gone. The move is one named call, so the anchor occurs exactly once.
  The replacement names a variant that really exists, so the mutant compiles
  clean and the red that comes back is the assertion's own words rather than a
  compiler's; a mutation that failed to build would go red without printing
  anything the runner matches on. `two_parks_in_one_second_land_in_distinct_directories`
  goes red alongside it, because it too asserts each park holds its own file;
  the entry names the check the plan cell means and this says why the red is
  two lines rather than one.

```sweep-edit crates/dcs-mcp/src/register.rs
-             move_file(source.as_path(), &destination)?;
+             std::fs::remove_file(source.as_path()).map_err(|why| RegisterError::Disk {
+                 path: source.as_path().to_owned(),
+                 why,
+             })?;
```

### register/row-written-after-the-move

- task: T60
- command: `mise exec -- cargo test -p dcs-mcp register`
- reddens: `the_row_goes_in_before_the_move_and_is_marked_after`
- note: two independent locals swapped, so the mutant compiles and warns about
  nothing, and the finished state is byte-identical — the only difference is
  what a run killed between the two would leave behind. That is why the check
  makes its assertions from *inside* the closure, at the moment of the move:
  read afterwards, both orderings look the same and this mutation would redden
  nothing at all. The red is `one row, already there / left: 0 / right: 1` —
  the register file does not exist yet where the move expected one `pending`
  row. `a_move_that_fails_leaves_its_row_pending` goes red with it, for the
  same reason from the other side: with the append moved after the closure, a
  closure that refuses leaves no row to be pending.

```sweep-edit crates/dcs-mcp/src/register.rs
-         let at = self.append(now, file, sha256)?;
-         let done = moving()?;
+         let done = moving()?;
+         let at = self.append(now, file, sha256)?;
```

### export_line/second-dofile-line-appended

- task: T61
- command: `mise exec -- cargo test -p dcs-mcp export_line`
- reddens: `install_twice_leaves_one_dofile_line`
- note: the once-only guard's bound is raised rather than the guard removed, so
  the early return stays reachable and the mutant carries no unreachable-code
  warning — the red is the assertion and nothing else. A second `install` then
  no longer recognises the line the first one wrote and appends another beside
  it, which is the defect the plan cell names. The line that goes red is the
  outcome comparison — `the line it wrote is the line it finds`, `left:
  Appended { .. } / right: AlreadyThere` — and not the count of dofile lines
  below it, which the check never reaches. The count is the assertion that says
  what the wrong outcome costs the file; the outcome is what is observed going
  red.
  `a_crlf_terminated_copy_of_the_line_is_still_the_line` goes red alongside it,
  for the same reason from the other side: it too asks the guard to recognise a
  line that is already there. `every_other_byte_of_the_file_is_the_byte_it_was`
  stays green, because it calls `ensure` once — that separation is what says
  this control watches the guard and not the append.

```sweep-edit crates/dcs-mcp/src/export_line.rs
-     if occurrences(&found) > 0 {
+     if occurrences(&found) > 1 {
```

### export_line/marker-dropped-from-the-line

- task: T61
- command: `mise exec -- cargo test -p dcs-mcp export_line`
- reddens: `the_line_carries_the_marker_that_makes_its_removal_an_exact_match`
- note: the plan cell names the red as the uninstaller's exact-match removal,
  and the uninstaller is not built. What is observable today is one step
  earlier and is the same defect: the line this build appends no longer ends in
  the marker that the removal will match a whole line against, so it can no
  longer be told from a `dofile` of the same hook somebody wrote by hand. Every
  other check stays green, because they all compare against `LINE` itself and
  would follow it wherever it went — which is why the marker needs an assertion
  that does not, and why without one this mutation would redden nothing at all.
  `MARKER` stays `pub` and is still read by that check, so the mutant compiles
  clean under `-D warnings`.

```sweep-edit crates/dcs-mcp/src/export_line.rs
- pub const LINE: &str = "dofile(lfs.writedir() .. 'Scripts/Hooks/DcsEvalExecutor.lua') -- dcs-mcp";
+ pub const LINE: &str = "dofile(lfs.writedir() .. 'Scripts/Hooks/DcsEvalExecutor.lua')";
```

### install/final-name-written-directly

- task: T44
- command: `mise exec -- cargo test -p dcs-mcp install::`
- reddens: `install::tests::the_hook_never_appears_half_written`
- note: the bytes go straight to the name DCS loads, so the rename that
  follows renames the file onto itself. Windows accepts that, and the mutant
  therefore compiles clean, warns about nothing and still "succeeds" — which is
  the defect exactly: the final name holds a partial file for as long as the
  write takes, and a game started in that window loads a truncated chunk. Every
  other check in the module stays green, because read afterwards the two
  orderings are byte-identical; that is why the red one makes its assertions
  from *inside* the closure, between the write and the rename. The observed red
  is `the name DCS loads holds nothing until the rename: …\Scripts\Hooks\DcsEvalExecutor.lua`.
  A second check goes red with it,
  `the_placement_puts_the_bytes_down_under_the_staging_name_first`, and it is
  the one that says the whole placement still goes through the staging and not
  merely that the staging works when it is called: it occupies the staging name
  with a directory, which a placement writing straight to the final name would
  sail past. The sibling assertion in the same closure — that
  the staging file's parent is the destination directory — is what holds the
  other half of this row, a `.tmp` written on another volume, where the rename
  becomes a copy and the window reopens; no mutation is named for it here
  because writing the staging file elsewhere is the same defect from the other
  side and reddens the same check.

```sweep-edit crates/dcs-mcp/src/install.rs
-     let staging = dir.join(format!("{name}.tmp"));
+     let staging = dir.join(name);
```

### install/foreign-hash-replaced-without-replace

- task: T44
- command: `mise exec -- cargo test -p dcs-mcp install::`
- reddens: `install::tests::a_foreign_hook_is_refused_and_named_without_replace`
- note: the guard is deleted outright, so a hook file this project never
  shipped is parked and replaced with nobody having said it may be. The run
  occurs exactly once — the incumbent's refusal beside it is spelled with a
  different condition, so no shared line makes the anchor ambiguous — and the
  mutant compiles without a warning, because the refusal clones its two fields
  rather than moving them and both are read again below by the park and the
  report, and because `replace` is still read by the incumbent guard left
  standing. The red is the `expect_err` coming back with a `Placed` naming the
  park it made. `a_foreign_hook_is_parked_when_replace_answers_for_it` stays
  green, and that is the point of the pair: the control watches the refusal and
  not the parking. It holds only because nothing below the guards consults
  `replace` again — what is parked is decided by what was found.

```sweep-edit crates/dcs-mcp/src/install.rs
-         if let (Some(hook), Disposition::Foreign { sha256: hash }) = (&ours, &disposition) {
-             return Err(InstallError::Foreign {
-                 file: hook.clone(),
-                 hash: hash.clone(),
-             });
-         }
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
  runner. Stages 7 and 8 have both begun to be built, so what keeps the rows
  below here is the figure's scope and not an absence of code to mutate.
- controls: 10
- breakdown: T58, T59 one each, 2; T40, T46 two each, 4; T45
  three, 3; T52 one, 1. Stage 9's remaining rows name no mutation
  and are owed none. This figure falls as Stages 7 and 8 are built and their
  rows move into the inventory proper.

### out/the-runner-itself

- task: T57
- out-of-scope: the sweep cannot sweep itself — a mutation of the runner would
  be applied by the runner it broke. Its own two mutations are proved by
  `tools/sweep-test.sh`, which drives a copy of it over a sandbox inventory.
- controls: 2
