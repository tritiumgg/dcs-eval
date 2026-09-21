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
every Stage 0–8 plan row naming a mutation has an entry here, but it checks
presence per task, not per mutation, so a second control added to a row that
already has an entry can still go unwritten if nobody writes it.

**Which rows owe an entry, and which may hold one.** Every row from Stage 0 to
Stage 8 — every stage that is built — whose done-condition names a mutation
owes one, and an entry filed under any row from Stage 0 upward is accepted,
because a task past Stage 8 that builds a control writes its entry in the same
pull request like every other. Stages 0–2 were once the one place an entry was
refused: they closed before this runner existed and stood in the figure as one
hand-counted group, so an entry under one of their rows would have been
counted twice. Each of their mutations now has an entry of its own, and the
hand count went with the reason for the refusal. Stage 9's rows are live
proofs and owe nothing; the five that name a mutation of something that runs
off DCS — T47, T48, T50, T52 and T63 — have their entries above all the
same, written as their code landed, and counted in scope.

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

## Stage 0 — toolchain and harness floor

### interpreter/wrong-version-admitted

- task: T01
- command: `sh tools/check-lua-test.sh`
- reddens: `5.4 refused, by version`
- note: the plan's mutation is an input — pointing the guard at a 5.4 binary —
  and the test feeds that input, a fake banner, on every run. What the sweep
  breaks is the guard, so that the banner it is fed is admitted.

```sweep-edit tools/check-lua.sh
-     "$want "*"PUC-Rio"*)
+     "Lua "*)
```

### workspace/library-dropped-from-members

- task: T02
- command: `mise exec -- sh tools/buildcheck.sh`
- reddens: `crates/dcs-eval is not in Cargo.toml's members`
- note: cargo pulls a path dependency into the workspace by itself, so fmt,
  clippy, build and test all stay green and the roll call at the foot of the
  build gate is the red, which is why the roll call exists; it leads with
  `FAIL  ` so that this red is not read as a build that broke. The resolution
  does not change, so `Cargo.lock` does not move. Dropping `dcs-mcp` instead,
  which nothing depends on, would rewrite `Cargo.lock`, a file the runner
  takes no copy of, so that variant is not entered. The command is the whole
  build gate, run twice.

```sweep-edit Cargo.toml
- members = ["crates/dcs-eval", "crates/dcs-mcp"]
+ members = ["crates/dcs-mcp"]
```

### harness/empty-suite-passes

- task: T03
- command: `mise exec -- sh tools/harness-test.sh`
- reddens: `no checks is nothing ran`
- note: the plan's mutation is an input, a suite with no assertions, which the
  test feeds the runner on every run. What the sweep breaks is the runner's
  refusal of it.

```sweep-edit tools/harness.lua
-   if empty > 0 or total == 0 then
+   if false then
```

### stubs/dostring_in-evaluates

- task: T04
- command: `mise exec -- lua5.1 tools/harness.lua stubs`
- reddens: `a chunk that would return a value answers empty`

```sweep-edit tools/harness/states.lua
- local function dostring_in(state, _)
-   if DOSTRING_STATES[state] then
-     return ""
+ local function dostring_in(state, chunk)
+   if DOSTRING_STATES[state] then
+     return tostring((loadstring(chunk) or function() end)())
```

### interop/interpreter-absent-reddens

- task: T05
- command: `mise exec -- cargo test -p dcs-eval interop`
- reddens: `the_shipped_executors_handshake_parses`
- note: the plan's mutation removes CI's interpreter step, which only a CI run
  can see. What that removal does to the test is performed here instead: the
  spawn meets no interpreter, and the module panics rather than skipping, so a
  change that turned that into a skip would leave this green. Every test that
  spawns the interpreter goes red with it.

```sweep-edit crates/dcs-eval/src/interop.rs
-     let out = Command::new("lua5.1.exe")
+     let out = Command::new("lua5.1-absent.exe")
```

---

## Stage 1 — the load shell and the envelope

### load/top-level-pcall-dropped

- task: T06
- command: `mise exec -- lua5.1 tools/harness.lua executor/load`
- reddens: `raised: no host`
- note: the plan names a registration that raises, and the suite's case for
  it checks `mutation: the load must not raise`. With the guard gone the
  suite never reaches that case: the load raises first in the state with
  neither host, where the file refuses to start, and the suite reports the
  raise. Either way it is the load raising that goes red.

```sweep-edit executor/DcsEvalExecutor.lua
- local ok, err = pcall(main)
+ local ok, err = true, main()
```

### containment/relative-admitted

- task: T07
- command: `mise exec -- lua5.1 tools/harness.lua executor/containment`
- reddens: `relative: Temp`
- note: this and the two refusals below redden the same check, that the
  directory handed in falls back to the one the executor chooses; the
  message names the case.

```sweep-edit executor/DcsEvalExecutor.lua
-   if not absolute(dir) then
+   if false then
```

### containment/install-admitted

- task: T07
- command: `mise exec -- lua5.1 tools/harness.lua executor/containment`
- reddens: `install: C:`

```sweep-edit executor/DcsEvalExecutor.lua
-   if install and inside(dir, install) then
+   if false then
```

### containment/saved-games-outside-logs-admitted

- task: T07
- command: `mise exec -- lua5.1 tools/harness.lua executor/containment`
- reddens: `saved games: C:`

```sweep-edit executor/DcsEvalExecutor.lua
-   if inside(dir, wd) and not inside(dir, wd .. SEP .. "Logs") then
+   if false then
```

### session/getpid-test-dropped

- task: T08
- command: `mise exec -- lua5.1 tools/harness.lua executor/session`
- reddens: `the line names os.getpid`

```sweep-edit executor/DcsEvalExecutor.lua
-   if type(getpid) ~= "function" then
+   if false then
```

### session/sibling-removed-as-a-file

- task: T08
- command: `mise exec -- lua5.1 tools/harness.lua executor/session`
- reddens: `sweep: both siblings are counted`
- note: the cell's "a request in a foreign sibling is never listed" is held
  by removing the sibling whole; `os.remove` cannot remove a directory, so the
  foreign sibling, and the request in it, stays on the disk.

```sweep-edit executor/DcsEvalExecutor.lua
-   local ok, why = lfs.rmdir(path)
+   local ok, why = os.remove(path)
```

### framer/final-name-written-directly

- task: T09
- command: `mise exec -- lua5.1 tools/harness.lua executor/framer`
- reddens: `a remove of the final name and a rename onto it`

```sweep-edit executor/DcsEvalExecutor.lua
-   local tmp = path .. ".tmp"
+   local tmp = path

-     ok, why = os.rename(tmp, path)
+     ok = true
```

### framer/size-test-dropped

- task: T09
- command: `mise exec -- lua5.1 tools/harness.lua executor/framer`
- reddens: `limit: one byte over is not`

```sweep-edit executor/DcsEvalExecutor.lua
-   if size > MAX_REQUEST_BYTES then
+   if false then
```

### framer/silence-read-as-refusal

- task: T09
- command: `mise exec -- lua5.1 tools/harness.lua executor/framer`
- reddens: `dcs: a publish whose calls answer nothing on success lands`
- note: added after the first live load, where DCS's own `io` or `os`
  answered a success with nothing and the handshake was refused as
  `executor.txt: nil`. The harness's `io` is Lua 5.1's, which always
  answers, so only the suite's model of DCS's library can see this.

```sweep-edit executor/DcsEvalExecutor.lua
-   return not ok and why ~= nil
+   return not ok
```

### framer/false-taken-for-silence

- task: T09
- command: `mise exec -- lua5.1 tools/harness.lua executor/framer`
- reddens: `dcs false: with its message`
- note: a refusal read as `nil` and a message only lets `false` and a
  message through as a silence; the stat then refuses it for the wrong
  reason, and the host's own message is lost.

```sweep-edit executor/DcsEvalExecutor.lua
-   return not ok and why ~= nil
+   return ok == nil and why ~= nil
```

### framer/silent-rename-trusted

- task: T09
- command: `mise exec -- lua5.1 tools/harness.lua executor/framer`
- reddens: `dcs rename: a rename that answered nothing and did nothing is refused`

```sweep-edit executor/DcsEvalExecutor.lua
-   if asked and (lfs.attributes(path, "size") ~= #bytes or lfs.attributes(tmp, "mode") ~= nil) then
+   if false then
```

### framer/silent-rename-unasked

- task: T09
- command: `mise exec -- lua5.1 tools/harness.lua executor/framer`
- reddens: `dcs rename: a rename that answered nothing and did nothing is refused`
- note: the rename's own silence, apart from the write's and the close's;
  the case has every other call answer the way Lua 5.1 does.

```sweep-edit executor/DcsEvalExecutor.lua
-   local asked = silent or not ok
+   local asked = silent
```

### framer/silent-write-forgotten

- task: T09
- command: `mise exec -- lua5.1 tools/harness.lua executor/framer`
- reddens: `dcs write: a write that answered nothing and wrote nothing is refused`
- note: a write's silence is carried past a close and a rename that answer
  true, because an empty file renames as well as a full one.

```sweep-edit executor/DcsEvalExecutor.lua
-     silent = not ok
+     silent = false
```

### framer/silent-close-forgotten

- task: T09
- command: `mise exec -- lua5.1 tools/harness.lua executor/framer`
- reddens: `dcs close: a close that answered nothing and flushed nothing is refused`

```sweep-edit executor/DcsEvalExecutor.lua
-     silent = silent or not ok
+     silent = silent
```

### framer/stale-final-taken

- task: T09
- command: `mise exec -- lua5.1 tools/harness.lua executor/framer`
- reddens: `dcs stale: a rename that did nothing over a file of the same size is refused`
- note: the size alone agrees with a file the rename never replaced; the
  `.tmp` still standing is what gives it away.

```sweep-edit executor/DcsEvalExecutor.lua
-   if asked and (lfs.attributes(path, "size") ~= #bytes or lfs.attributes(tmp, "mode") ~= nil) then
+   if asked and lfs.attributes(path, "size") ~= #bytes then
```

### framer/silent-remove-trusted

- task: T09
- command: `mise exec -- lua5.1 tools/harness.lua executor/framer`
- reddens: `dcs take lie: a remove that answered nothing and removed nothing withholds the bytes`

```sweep-edit executor/DcsEvalExecutor.lua
-   if refused(ok, why) or (not ok and lfs.attributes(path, "mode") ~= nil) then
+   if refused(ok, why) then
```

### session/silent-remove-stops-the-sweep

- task: T08
- command: `mise exec -- lua5.1 tools/harness.lua executor/framer`
- reddens: `dcs relaunch: the first session is swept`
- note: proved in `executor/framer`, where the model of DCS's library
  lives: a remove that answers nothing on success, read as Lua 5.1's
  answer, stops the sweep at the first file and leaves the session behind.

```sweep-edit executor/DcsEvalExecutor.lua
-       ok = not refused(ok, why)
```

### request/for-check-dropped

- task: T10
- command: `mise exec -- lua5.1 tools/harness.lua executor/request`
- reddens: `no for: nothing is handed back`
- note: a request missing `for` that runs anyway, as the cell names it. Taking
  out the missing-`for` arm alone does not do that: the request falls to the
  stale-session arm, whose excerpt of a nil `for` raises. So the stale arm is
  told to pass a missing `for` through too, and the request is handed back to
  be run.

```sweep-edit executor/DcsEvalExecutor.lua
-     elseif headers["for"] == nil or headers["for"] == "" then
-       why = "no for: the request does not name the session stamp it is for"
-     elseif headers["for"] ~= E.stamp then
+     elseif false then
+       why = "no for: the request does not name the session stamp it is for"
+     elseif headers["for"] ~= nil and headers["for"] ~= "" and headers["for"] ~= E.stamp then
```

### request/empty-body-admitted

- task: T10
- command: `mise exec -- lua5.1 tools/harness.lua executor/request`
- reddens: `empty body: nothing is handed back`

```sweep-edit executor/DcsEvalExecutor.lua
-     elseif BODY_REQUIRED[headers.op] and body == "" then
+     elseif false then
```

### handshake/transport-dropped

- task: T11
- command: `mise exec -- lua5.1 tools/harness.lua executor/handshake`
- reddens: `every field of the table is present, and no other`
- note: the `started` line is in the anchor because the `transport` line alone
  matches twice, in the handshake and in the heartbeat; the pair is unique and
  sits in the handshake.

```sweep-edit executor/DcsEvalExecutor.lua
-     { "started", os.date("%Y-%m-%d %H:%M:%S", E.started) },
-     { "transport", E.session },
+     { "started", os.date("%Y-%m-%d %H:%M:%S", E.started) },
```

---

## Stage 2 — ping, and the wire proven

### ping/status-dropped

- task: T12
- command: `mise exec -- lua5.1 tools/harness.lua executor/ping`
- reddens: `ping: every header is present, and no other`
- note: the line is in `reply`, so every reply loses its `status`.

```sweep-edit executor/DcsEvalExecutor.lua
-     { "status", status },
```

### protocol/line-break-written

- task: T13
- command: `mise exec -- cargo test -p dcs-eval protocol`
- reddens: `frame_refuses_an_lf_in_a_value_the_injection_guard`
- note: the build warns that the line-break error is never constructed; the
  warning is not the red.

```sweep-edit crates/dcs-eval/src/protocol.rs
-         if value.bytes().any(|b| b == b'\r' || b == b'\n') {
+         if false {
```

### protocol/body-decoded-lossily

- task: T13
- command: `mise exec -- cargo test -p dcs-eval protocol`
- reddens: `parse_the_body_byte_for_byte_a_cp1251_body`

```sweep-edit crates/dcs-eval/src/protocol.rs
-                 body: bytes[nl + 1..].to_vec(),
+                 body: String::from_utf8_lossy(&bytes[nl + 1..]).into_owned().into_bytes(),
```

### publish/arm-file-removed-on-send

- task: T14
- command: `mise exec -- cargo test -p dcs-eval publish`
- reddens: `send_never_removes_the_arm_file`

```sweep-edit crates/dcs-eval/src/publish.rs
-     arm(arm_path).map_err(SendError::Arm)?;
+     arm(arm_path).map_err(SendError::Arm)?;
+     let _ = fs::remove_file(arm_path);
```

### standin/encoder-is-the-clients

- task: T15
- command: `mise exec -- cargo test -p dcs-eval standin`
- reddens: `encoder_is_not_the_clients`
- note: the build warns that the stand-in's own encoder is unreachable; the
  warning is not the red.

```sweep-edit crates/dcs-eval/src/standin.rs
- pub fn encode(headers: &[(&str, &str)], body: &[u8]) -> Result<Vec<u8>, String> {
+ pub fn encode(headers: &[(&str, &str)], body: &[u8]) -> Result<Vec<u8>, String> {
+     return crate::protocol::frame(headers, body).map_err(|e| e.to_string());
```

### interop/frame-blank-line-dropped

- task: T16
- command: `mise exec -- cargo test -p dcs-eval interop`
- reddens: `the_shipped_executors_handshake_parses`

```sweep-edit executor/DcsEvalExecutor.lua
-   return table.concat(lines) .. "\n" .. body
+   return table.concat(lines) .. body
```

### interop/blank-kept-before-an-empty-value

- task: T16
- command: `mise exec -- cargo test -p dcs-eval interop`
- reddens: `the_ping_reply_parses_and_an_empty_value_arrives_empty`

```sweep-edit crates/dcs-eval/src/protocol.rs
-             .unwrap_or(rest.len());
+             .unwrap_or(0);
```

### e2e/frame-terminator-crlf

- task: T17
- command: `mise exec -- cargo test -p dcs-eval e2e`
- reddens: `a_ping_the_client_sends_is_answered_by_the_shipped_executor`
- note: a byte of the executor's `frame` that changes the wire; the test's
  message says the Lua writes LF alone.

```sweep-edit executor/DcsEvalExecutor.lua
-     lines[#lines + 1] = name .. ": " .. value .. "\n"
+     lines[#lines + 1] = name .. ": " .. value .. "\r\n"
```

### e2e/suite-stops-ticking

- task: T17
- command: `mise exec -- cargo test -p dcs-eval e2e`
- reddens: `a_ping_the_client_sends_is_answered_by_the_shipped_executor`
- note: the red comes at the Lua suite's own deadline, naming the tick with
  the request still waiting, and not as a hang.

```sweep-edit tools/harness/executor/e2e.lua
-   frame()
+   -- frame()
```

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

### events/silent-line-refused

- task: T26
- command: `mise exec -- lua5.1 tools/harness.lua executor/framer`
- reddens: `dcs load: the events line was written`
- note: proved in `executor/framer`, where the model of DCS's library
  lives: a close that answers nothing on success, read as Lua 5.1's answer,
  counts every line written as one lost.

```sweep-edit executor/DcsEvalExecutor.lua
-     if not refused(ok, why) then
+     if ok then
```

### events/silent-rotation-refused

- task: T26
- command: `mise exec -- lua5.1 tools/harness.lua executor/framer`
- reddens: `dcs relaunch: the events log was rotated`
- note: proved in `executor/framer`, as the line above is: a rename that
  answers nothing on success is recorded as a rotation that failed.

```sweep-edit executor/DcsEvalExecutor.lua
-   if refused(ok, why) then
-     E.events_left = E.events .. ": " .. tostring(why)
+   if not ok then
+     E.events_left = E.events .. ": " .. tostring(why)
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

### echo/state-shape-uncut-in-the-standin

- task: T53
- command: `mise exec -- cargo test -p dcs-eval standin`
- reddens: `an_eval_is_answered_as_a_chunk_that_returned_nil_and_refused_as_the_executor_refuses`
- note: the third echo T53 found by comparing the dialects, a malformed
  `state` echoed whole by the stand-in where the Lua already cut it; it was
  fixed before the two cuts above were built on it. It reddens the same test
  as `echo/state-uncut-in-the-standin`, whose cases hold both refusals.

```sweep-edit crates/dcs-eval/src/standin.rs
-                 format!("state: {} is not [A-Za-z][A-Za-z0-9_]*", excerpt(state)),
+                 format!("state: {state} is not [A-Za-z][A-Za-z0-9_]*"),
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

### uncollected/never-expired

- task: T64
- command: `mise exec -- lua5.1 tools/harness.lua executor/uncollected`
- reddens: `armed: the reply 300 s old is removed`
- note: the call deleted, so nothing ever removes a reply. Every case before
  this check passes, because each only reads what was answered.

```sweep-edit executor/DcsEvalExecutor.lua
-   expire(now, clock, start)
```

### uncollected/kept-for-no-time

- task: T64
- command: `mise exec -- lua5.1 tools/harness.lua executor/uncollected`
- reddens: `armed: the reply is on the disk the frame it is answered`
- note: with the limit at zero, a reply is removed by the frame that published
  it, since the sweep runs after the requests. This is the "everything goes at
  once" reading of the specification's "once at disarm", in its sharpest form.

```sweep-edit executor/DcsEvalExecutor.lua
- local UNCOLLECTED_S = 300
+ local UNCOLLECTED_S = 0
```

### uncollected/backlog-cleared-in-one-frame

- task: T64
- command: `mise exec -- lua5.1 tools/harness.lua executor/uncollected`
- reddens: `backlog: one reply is left for the next frame`

```sweep-edit executor/DcsEvalExecutor.lua
-     if not first and (clock() - start) * 1000 >= TICK_BUDGET_MS then
+     if false then
```

### uncollected/none-removed-once-spent

- task: T64
- command: `mise exec -- lua5.1 tools/harness.lua executor/uncollected`
- reddens: `spent: one expired reply is removed even with the budget gone`
- note: the budget read before the first removal as well as the ones after
  it. The backlog case cannot see this, because its frames answer nothing and
  the first reading is 0 ms either way; only a frame whose requests spent the
  budget can.

```sweep-edit executor/DcsEvalExecutor.lua
-     if not first and (clock() - start) * 1000 >= TICK_BUDGET_MS then
+     if (clock() - start) * 1000 >= TICK_BUDGET_MS then
```

### uncollected/collect-names-one-reason

- task: T64
- command: `mise exec -- cargo test -p dcs-mcp tools_listed_wording`
- reddens: `tools_listed_wording_of_an_uncollected_id_names_a_phase`
- note: `dcs_collect`'s answer for an id with nothing under it, put back to the
  wording from before the sweep, which names only a reply not yet landed. It
  fails at `it says the reply may have been removed`.

```sweep-edit crates/dcs-mcp/src/tools.rs
-                     "nothing has landed under that id yet, or it landed and went uncollected long enough to be removed",
+                     "nothing has landed under that id yet",
```

### uncollected/expired-while-dormant

- task: T64
- command: `mise exec -- lua5.1 tools/harness.lua executor/uncollected`
- reddens: `asleep: a reply past 300 s stays until the executor wakes`
- note: the sweep put on the sleeping path, with a clock read to feed it, which
  is exactly what the dormant budget forbids. `executor/dormant`'s byte count
  is not what reddens: `os.time` and `rawget` allocate nothing, and an empty
  ledger removes nothing. It is this suite that sees a reply go while nothing
  is awake. The anchor is the one `heartbeat/dormant-keeps-beating` uses; two
  controls may share an anchor, because each is applied alone.

```sweep-edit executor/DcsEvalExecutor.lua
-     return
-   end
-   began = nil
+     expire(rawget(rawget(_G, "os"), "time")(), rawget(rawget(_G, "os"), "clock"), 0)
+     return
+   end
+   began = nil
```

### uncollected/disarming-frame-skips-the-sweep

- task: T64
- command: `mise exec -- lua5.1 tools/harness.lua executor/uncollected`
- reddens: `disarm: the frame that went back to sleep removed the reply 300 s old`
- note: the sweep moved below the quiet window and run only by a frame still
  armed after it, so every armed frame sweeps but the one that disarms. This
  is the half of the transition the specification sweeps "once at disarm"
  on; the executor has it because the frame sweeps before it looks at the
  window, so moving the call is enough to lose it.

```sweep-edit executor/DcsEvalExecutor.lua
-   expire(now, clock, start)
```

```sweep-edit executor/DcsEvalExecutor.lua
-   -- The beat, last of the frame, so the `ticks`, the phase and the last
+   if E.armed then
+     expire(now, clock, start)
+   end
+   -- The beat, last of the frame, so the `ticks`, the phase and the last
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

### e2e/stamp-change-not-superseded

- task: T54
- command: `mise exec -- cargo test -p dcs-eval e2e`
- reddens: `a_request_to_a_session_that_restarted_is_read_as_superseded`
- note: the round trip's `superseded` half, which T17's row defers and which
  cannot be filed under a Stage 2 row; it is filed here because the verdict it
  breaks is this row's `wait`. The shipped executor is loaded twice over one
  box, as DCS relaunched, and the first session's process has exited. With the
  re-read stamp ignored, the table falls through to the pid: the observed
  panic was `superseded, not Dead { id: "0000000001-ping" }`, at once, while
  the ping test stayed green.

```sweep-edit crates/dcs-eval/src/wait.rs
-     if handshake.stamp != s.stamp {
+     if false && handshake.stamp != s.stamp {
```

### e2e/pid-change-not-superseded

- task: T54
- command: `mise exec -- cargo test -p dcs-eval e2e`
- reddens: `a_relaunch_handed_the_old_pid_back_is_read_as_superseded`
- note: the same verdict read off the wrong half of the stamp. Both loads run
  under this test's own pid, live throughout, so the stamps differ in their
  time alone, as a relaunch handed the old pid back would. Comparing pids sees
  nothing changed: the observed panic was `superseded, not Pending { id:
  "0000000001-ping", phase: "unknown", flag: Some(Waking) }` after the 5 s
  verdict wait, while the exited-pid restart and the ping test stayed green.

```sweep-edit crates/dcs-eval/src/wait.rs
-     if handshake.stamp != s.stamp {
+     if handshake.pid != s.pid {
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

### status/tempdir-under-the-clients-read-as-a-disagreement

- task: T32
- command: `mise exec -- cargo test -p dcs-eval status`
- reddens: `lfs_tempdir_under_this_clients_temp_directory_is_within_and_no_problem`
- note: the first live load's `%TEMP%\DCS` (ADR 0029). With the within branch
  gone it falls through to a disagreement, and `verify` says `not verified`
  on a session `ping` answers.

```sweep-edit crates/dcs-eval/src/status.rs
-     } else if client.contains(&executor) {
+     } else if false {
```

### status/tempdir-anywhere-read-as-within

- task: T32
- command: `mise exec -- cargo test -p dcs-eval status`
- reddens: `lfs_tempdir_disagreeing_with_this_clients_temp_directory_is_a_problem`
- note: every resolved temp directory admitted as within, so one outside the
  client's tree stops being reported. The shared-bytes sibling check reddens
  beside it.

```sweep-edit crates/dcs-eval/src/status.rs
-     } else if client.contains(&executor) {
+     } else if true {
```

### status/tempdir-within-by-byte-prefix

- task: T32
- command: `mise exec -- cargo test -p dcs-eval status`
- reddens: `lfs_tempdir_sharing_only_this_clients_bytes_is_a_problem`
- note: containment taken as a folded prefix of the spelling rather than at a
  segment boundary, which admits `...\TempDCS` as inside `...\Temp`.

```sweep-edit crates/dcs-eval/src/status.rs
-     } else if client.contains(&executor) {
+     } else if executor.to_string().to_lowercase().starts_with(&client.to_string().to_lowercase()) {
```

### status/leftover-heartbeat-read-as-foreign

- task: T32
- command: `mise exec -- cargo test -p dcs-eval status`
- reddens: `a_heartbeat_the_last_session_left_before_this_one_loaded_is_no_problem`
- note: the live run's relaunch (ADR 0030). With no heartbeat ever read as a
  leftover, the last session's file is two problems again, stamp and
  transport, and nothing else in the module reddens: every other foreign
  fixture is written after its handshake.

```sweep-edit crates/dcs-eval/src/status.rs
-     beat.stamp != h.stamp && published.is_some_and(|at| beat.modified < at)
+     false
```

### status/every-foreign-heartbeat-read-as-a-leftover

- task: T32
- command: `mise exec -- cargo test -p dcs-eval status`
- reddens: `a_heartbeat_another_session_wrote_after_this_one_loaded_is_still_a_problem`
- note: the time dropped, so any other stamp is a leftover and a second
  executor writing since this session loaded goes unreported. Every foreign
  fixture reddens beside it — the instant check, the flagged-foreign check,
  and `game`'s agreement that status flags a foreign heartbeat, which the
  filter reaches by name.

```sweep-edit crates/dcs-eval/src/status.rs
-     beat.stamp != h.stamp && published.is_some_and(|at| beat.modified < at)
+     beat.stamp != h.stamp
```

### status/heartbeat-at-the-handshakes-instant-read-as-a-leftover

- task: T32
- command: `mise exec -- cargo test -p dcs-eval status`
- reddens: `a_foreign_heartbeat_written_at_the_handshakes_instant_is_still_a_problem`
- note: before taken as at-or-before, so a file that cannot be placed on
  either side of the handshake is given the benefit of the doubt. Nothing
  else reddens.

```sweep-edit crates/dcs-eval/src/status.rs
-     beat.stamp != h.stamp && published.is_some_and(|at| beat.modified < at)
+     beat.stamp != h.stamp && published.is_some_and(|at| beat.modified <= at)
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

### file_refusals/file-opened-before-the-ceiling

- task: T34
- command: `mise exec -- cargo test -p dcs-eval file_refusals`
- reddens: `a_held_file_over_the_ceiling_is_refused_without_being_opened`
- note: where a path lies is no longer judged (ADR 0026), so the refusal a
  read must not come ahead of is the ceiling's. A file too big for one
  request is refused on the stat; a read placed first would buffer the file
  for a request that cannot be sent, and would turn the refusal into an open
  error. The fixture is held open elsewhere, so a late refusal cannot pass
  for an early one.

  Observed red is five tests, more than the sweep's four-line summary shows:
  the held oversize file, which comes back as a failed stat rather than
  `Oversize`; `no_refusal_carries_a_byte_of_the_file_or_an_open_error`, whose
  held vault the read cannot open either; the two framer refusals, whose
  absent file now fails on the read before the framer is asked; and
  `refuses_a_directory`, which a read cannot open.

```sweep-edit crates/dcs-eval/src/file.rs
-     let block = protocol::frame(headers, b"").map_err(|source| FileRefusal {
+     let _peek = fs::read(real.as_path()).map_err(|source| FileRefusal {
+         path: real.clone(),
+         kind: Refusal::Stat(source),
+     })?;
+     let block = protocol::frame(headers, b"").map_err(|source| FileRefusal {
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

### cli/pending-writes-a-zero-byte-file

- task: T40
- command: `mise exec -- cargo test -p dcs-mcp cli`
- reddens: `cli_a_pending_writes_no_file_at_all`
- note: the keeping flags written unguarded, which is the natural way to
  write this wrong — the reply is "whatever came back", and what came back
  for a `pending` is nothing. The guard is the one `Option`, so the mutation
  hands the writing an answer that is always there and empty where none
  arrived; the match stands, so the edit compiles and the red is an assertion
  rather than the type checker.

  What the mutated build does is write a zero-byte file at `--out` and a
  zero-byte `.res` under the capture directory, for a request that is still
  in flight. A caller reading either back cannot tell it from a reply whose
  body really was empty, which is the same confusion between "nothing yet"
  and "nothing measured" that the `pending` wording exists against one level
  up.

  Observed red is two tests, both in the one command: the `pending` test,
  which is what this row is for, and `cli_every_read_verb_answers`, whose
  two `eval --file` cases, a `pending` and a refusal, are each given an
  `--out` and so watch the same rule from a file's side. The two reply tests
  write the same bytes either way, because a reply really did come back for
  them.

```sweep-edit crates/dcs-mcp/src/cli.rs
-     let published = written_bytes(&answered, parsed.out.is_some() || parsed.capture);
+     let published = Some(answered.published().unwrap_or(Ok(Reply {
+         id: "unanswered".to_owned(),
+         bytes: Vec::new(),
+     })));
```

### cli/reply-rendered-by-a-second-formatter

- task: T40
- command: `mise exec -- cargo test -p dcs-mcp cli`
- reddens: `cli_one_reply_is_written_verbatim_and_worded_as_the_tool_words_it`
- note: the one print seam replaced by a formatter of the command line's own,
  for replies only. It is the natural wrong way to write it — the command
  line is holding the reply's bytes already, and headers and a body are
  "obviously" what a reader wants — and it is deliberately narrow, since a
  `pending` has no reply behind it and goes on through the renderer. So the
  capture entry above stays green under it.

  `cli_eval_file_reads_a_file_from_wherever_it_lies` reddens beside the
  byte-identity test: a second formatter prints the published bytes alone,
  and the `source:` and `sha256:` lines a file eval's answer ends with are
  the tool's wording, which that formatter never reaches.

  Observed red is the empty-diff assertion, which prints both texts: the
  published bytes carry the executor's blank separator line where
  `wording::text` renders the headers and the body joined by one newline, so
  the diff is several lines wide — `- first / + (blank) / - second / + first`
  and on. The `--out` half of that same test stays green under this edit,
  which is what says its two halves watch different things.

```sweep-edit crates/dcs-mcp/src/cli.rs
-     let shown = wording::text(&answered.answer);
+     let shown = match answered.published() {
+         Some(Ok(reply)) => format!("reply\n{}", String::from_utf8_lossy(&reply.bytes)),
+         _ => wording::text(&answered.answer),
+     };
```

### runs/hash-recomputed-from-the-reply

- task: T58
- command: `mise exec -- cargo test -p dcs-mcp runs`
- reddens: `a_file_eval_records_the_path_and_the_readers_own_hash`
- note: the digest computed where the line is written instead of taken from
  the reader's record. It is the natural wrong way round — the writer is
  holding the request already, and hashing it "obviously" gives the same
  answer — and it does give the same answer in every case but the one a
  provenance record exists for. The unmutated statement is one line and the
  digest appears nowhere else in that module, which is what makes a single
  whole-line anchor possible.

  The fixture pairs a file holding `return 1` and a newline with a reply
  whose body is `nil`, so the two digests can never coincide. Observed red
  is the one hash assertion, printing `5da3a4c7…` — the reply's body — where
  `0805bfdc…`, the file's, was wanted. That expectation is a literal
  computed outside the build with `sha256sum`, so it is independent of both
  the reader and the writer.

  The other two tests stay green, which is what says the red is exclusive:
  both drive inline evals, where there is no source and the `map` yields
  nothing under either statement.

```sweep-edit crates/dcs-mcp/src/runs.rs
-     let sha256 = source.map(|source| source.sha256_hex());
+     let sha256 = source.map(|_| {
+         dcs_eval::sha256::hex(&dcs_eval::sha256::digest(match item {
+             Some(Ok(dcs_eval::wait::Outcome::Reply(envelope))) => envelope.body.as_slice(),
+             _ => &[],
+         }))
+     });
```

### runs/a-refusal-records-a-line-anyway

- task: T58
- command: `mise exec -- cargo test -p dcs-mcp runs`
- reddens: `a_file_eval_refused_before_a_byte_is_read_records_nothing`
- note: a record written of what was *asked for* rather than of what was
  read. The edit goes on `eval_file`'s refusal closure, which is where
  somebody would naturally put it: every refusal for a file eval passes
  through that one line, so it reads as the place to say a file eval
  happened.

  Nothing guards this in the unmutated build. The data directory is resolved
  after the read succeeds and every earlier refusal has already returned, so
  "no line for a path that was never opened" falls out of the ordering. That
  is also why the control matters: a later change moving the resolution
  upward would still compile and still pass every other test.

  Observed red is the assertion that the data directory does not exist —
  constructing a `DataDir` creates nothing, so the mutated build's append is
  what brings the directory into being. The other two tests never take that
  closure and stay green.

```sweep-edit crates/dcs-mcp/src/tools.rs
-     let refused = |why: String| Answered::plain(refuse("refused", vec![why]));
+     let refused = |why: String| {
+         if let Ok(data) = data_dir(serve) {
+             let _ = crate::runs::append(&data, "{\"source\":\"file\",\"status\":\"refused\"}");
+         }
+         Answered::plain(refuse("refused", vec![why]))
+     };
```

### idle/keepalive-ping-on-a-timer

- task: T59
- command: `mise exec -- cargo test -p dcs-mcp idle`
- reddens: `idle::sixty_seconds_of_silence_wakes_nothing`
- note: a keepalive on a fifteen-second timer, publishing a `ping` into
  whatever session resolves. It is the natural wrong way to write it — a
  server that wants its executor awake when a call arrives — and it is the one
  thing the specification's "never hold the bridge armed" forbids by name.
  Guarded on `Handle::try_current` because `Serve::new` is also called from a
  plain synchronous `cli::run`, where a bare spawn would panic and turn the
  mutation into a crash in an unrelated command rather than a red assertion in
  this one. The id is a well-formed one, checked by hand against
  `publish::is_id`: a malformed one would be refused before anything touched
  the disk and the mutation would quietly redden nothing.

  Observed red is exactly one assertion in one test — the never-held-armed
  check in the first quiet beat, printing the arm path it found: `the executor
  was armed while no call was in flight: …\rpc\<stamp>\arm`. The name above is
  what the runner can see, since it reads the `... FAILED` lines and not a
  panic message; it is spelt with its module because the command's `idle`
  filter also selects T41's
  `watching::a_sibling_sweep_succeeds_while_the_server_is_idle_between_two_calls`,
  which carries the word and stays green here.

  The transport byte count is **not** moved by this edit, and that is worth
  saying: a keepalive that touches the executor is not one that touches the
  wire, and the two instruments watch the two halves of the rule apart.

  What the check does **not** watch is worth the same plainness. The plan row
  says no thread of the server wakes; what is read is three observables — the
  bytes written, the arm file, the request directory — so a task that woke on
  a timer and touched none of the three would pass every assertion in the
  test. The gap is covered indirectly by the two entries below, which hold the
  runtime the release actually builds to one that starts no driver for such a
  task to wake on, and not at all by this one.

  Nothing else in the crate was observed failing: the other 107 tests of
  `cargo test -p dcs-mcp` passed under the edit, T41's `watching` command
  among them. `serve`'s and `watching`'s tests are plain `#[test]`s with no
  runtime, so `try_current` fails there; `tools`'s tests do run under one, and
  their calls finished before the first tick published anything.

```sweep-edit crates/dcs-mcp/src/serve.rs
-     pub fn new(opts: Options) -> Self {
+     pub fn new(opts: Options) -> Self {
+         if let Ok(handle) = tokio::runtime::Handle::try_current() {
+             let keeping = opts.clone();
+             handle.spawn(async move {
+                 let mut every = tokio::time::interval(std::time::Duration::from_secs(15));
+                 loop {
+                     every.tick().await;
+                     if let Ok(client) = Client::resolve(&keeping) {
+                         let h = client.handshake();
+                         let _ = dcs_eval::publish::send(
+                             h.req.as_path(),
+                             h.arm.as_path(),
+                             "0000000009-keep",
+                             &[("op", "ping"), ("for", h.stamp.as_str())],
+                             b"",
+                         );
+                     }
+                 }
+             });
+         }
```

### idle/enable-all-in-place-of-the-timer

- task: T59
- command: `mise exec -- cargo test -p dcs-mcp idle`
- reddens: `idle::the_shipped_runtime_is_current_thread_with_a_timer_and_no_io_driver`
- note: the natural wrong turn at that builder, and the one the comment beside
  it argues against by name: a driver is missing, `enable_all` is the call that
  makes every complaint go away, and it brings the IO driver along with the
  timer that was wanted. What goes red is the positive arm — the observed
  message is `the server's runtime no longer calls .enable_time()` — because
  the two wanted calls are checked before the two refused ones and the timer's
  own spelling is gone.

  This control reads the source text rather than a built runtime, which is the
  thing to understand before trusting it. The test beside it runs on the
  harness's runtime and can never see the one `run` builds, and tokio's
  `Builder` hands back nothing that says which drivers it was asked for, so the
  shape is held by `include_str!` over `serve.rs`. A mutation here is therefore
  observed as a changed line of source and not as changed behaviour, and the
  control is worth exactly that much: it catches the edit, not its consequence.

  Nothing else in the crate notices. Under this edit `cargo test -p dcs-mcp`
  ran 108 tests with 107 passing and this one failing — the server still comes
  up and still answers, which is why a check that reads the builder's own words
  is the only thing standing here.

```sweep-edit crates/dcs-mcp/src/serve.rs
-         .enable_time()
+         .enable_all()
```

### idle/enable-all-beside-the-timer

- task: T59
- command: `mise exec -- cargo test -p dcs-mcp idle`
- reddens: `idle::the_shipped_runtime_is_current_thread_with_a_timer_and_no_io_driver`
- note: the same reach for `enable_all`, added beside the timer rather than
  over it, which is what the entry above cannot observe: with `.enable_time()`
  still spelt out, both positive assertions hold and the red comes from the
  refusal instead. The observed message is the negative arm's own, `the
  server's runtime now calls .enable_all(…), which starts a driver the stdio
  transport does not need`. Same tally as above: 107 passed, this one failed.

  The sibling refusal of `.enable_io(` is **not** proved by any mutation, and
  deliberately so rather than by oversight. `Builder::enable_io` does not exist
  unless tokio's `net` feature is on, and this build asks for `rt` and `time`
  only, so the edit that would redden that assertion does not compile — which
  the runner reports as BUILD-FAILED, a verdict that says nothing about the
  check. That assertion stands against the day somebody turns the feature on,
  and until then it is an unproved line, written down here as one.

```sweep-edit crates/dcs-mcp/src/serve.rs
-         .enable_time()
+         .enable_time()
+         .enable_all()
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
  the constant's own line is a different line and is left alone. The anchor
  is the newest list line, the one holding the digest this binary carries.

```sweep-edit crates/dcs-mcp/src/embed.rs
-     "e8919849bd88232a4fe2ac6d3b3cfee6cce4e50d7544ecc478f6be1db54935eb",
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
  occurs exactly once: it is the only refusal in the module, the second one
  having gone with ADR 0022. The red is the `expect_err` coming back with a
  `Placed` naming the park it made.
  `a_foreign_hook_is_parked_when_replace_answers_for_it` stays green, and that
  is the point of the pair: the control watches the refusal and not the
  parking. It holds only because nothing below the guard consults `replace`
  again — what is parked is decided by what was found.
  **The mutant warns, and the warning is not what reddens it.** This guard is
  now `replace`'s only reader, so deleting it leaves the parameter unused and
  `rustc` says so. `cargo test` compiles a warning, and the assertion is what
  fails; a sweep that came back green here on a warning alone would be
  reporting the compiler, not the control. Before ADR 0022 the incumbent's
  refusal read `replace` beside this one and the mutant was warning-free.

```sweep-edit crates/dcs-mcp/src/install.rs
-     if let (false, Some(hook), Disposition::Foreign { sha256: hash }) =
-         (replace, &ours, &disposition)
-     {
-         return Err(InstallError::Foreign {
-             file: hook.clone(),
-             hash: hash.clone(),
-         });
-     }
```

### uninstall/neighbouring-line-removed

- task: T45
- command: `mise exec -- cargo test -p dcs-mcp uninstall`
- reddens: `uninstall::tests::the_export_line_is_removed_by_exact_match_with_its_neighbours_byte_identical`
- note: whole-line equality becomes the substring match the row exists against.
  The mutant compiles clean — `LINE` is still read by `occurrences`, by `ensure`
  and by the marker check — and it takes three lines out of the fixture instead
  of one: ours, SRS's `dofile`, and the hand-written `dofile` of our own hook
  that carries no marker. That last line is the whole reason the fixture holds
  it. The red is the byte comparison, printed as two byte vectors that differ
  from `dofile(lfs.…` onwards. The binding inside `without` is named `content`
  rather than `line` so that this anchor cannot collide with `occurrences`'s
  `line == LINE.as_bytes()` a few lines above it. Every other check in the
  module stays green, because none of them holds a neighbour to eat — that
  separation is what says this control watches the match and not the write.
  `a_second_uninstall_changes_nothing` stays green too: both passes agree with
  each other, wrongly.

```sweep-edit crates/dcs-mcp/src/export_line.rs
-         if content == LINE.as_bytes() {
+         if content.starts_with(b"dofile(lfs.writedir()") {
```

### uninstall/unknown-hash-removed

- task: T45
- command: `mise exec -- cargo test -p dcs-mcp uninstall`
- reddens: `uninstall::tests::a_hook_whose_hash_is_not_one_we_shipped_is_left_in_place_and_named`
- note: the gate is made to say yes to everything, so a file at our name that
  this project never shipped is removed with nobody having established it is
  ours. It is spelled as a condition rather than as `let ours = true;` so that
  nothing goes dead and no boolean lint has anything to say: `release.shipped`
  stays read here as well as in the placement, `sha` stays read by the row and
  by the report, and a release always ships something, so the answer is yes for
  a stranger's hook as surely as a constant would be. The red is the read of a
  file that is no longer there — `the file is still there: Os { code: 2, kind:
  NotFound … }`. `a_hook_this_project_shipped_is_removed_and_its_row_says_uninstalled`
  stays green, and that pair is the point: the control watches the gate and not
  the removal. `a_second_uninstall_changes_nothing` goes red alongside it,
  because the restored stranger is taken away on the second pass.

```sweep-edit crates/dcs-mcp/src/uninstall.rs
-         let ours = release.shipped.contains(&sha.as_str());
+         let ours = !release.shipped.is_empty();
```

### uninstall/park-copied-not-moved

- task: T45
- command: `mise exec -- cargo test -p dcs-mcp uninstall`
- reddens: `uninstall::tests::a_parked_file_is_restored_to_its_original_path_and_the_park_is_emptied`
- note: the restore copies instead of moving, so the store goes on holding a
  file it has already given back. The finished tree is identical either way —
  the file that was there before us is back at its own name — and the only
  difference is what is left in the park, which is why the check asserts on the
  park rather than on the tree. The red is that assertion's own words, `the park
  still holds the file it gave back`. The anchor is the call inside `restore`;
  `park`'s own call is spelled `move_file(source.as_path(), &destination)?;`, a
  different line and itself another row's anchor, and this mutation leaves it
  alone. The `park/…` and `register/…` controls stay green, which says this one
  is the restore's and not the park's, and
  `a_second_uninstall_changes_nothing` stays green as well: the destination is
  occupied by then, so the duplicate in the store is never handed out.

```sweep-edit crates/dcs-mcp/src/register.rs
-             move_file(parked, destination.as_path())
+             fs::copy(parked, destination.as_path())
+                 .map(|_| ())
+                 .map_err(|why| RegisterError::Disk {
+                     path: destination.as_path().to_owned(),
+                     why,
+                 })
```

### verify/a-write-during-verify

- task: T46
- command: `mise exec -- cargo test -p dcs-mcp verify`
- reddens: `verify::tests::the_tree_verify_read_is_git_clean_afterwards`
- note: a log dropped beside what was inspected is the ordinary way this
  verb goes wrong, and the mutant compiles clean and warns about nothing —
  `fs` is already imported for the reads, and the `let _` answers the
  `must_use` — because every read is unaffected and the report is byte for
  byte the report it was. The only thing that changes is the tree. The
  check goes red at its `git status --porcelain` assertion, which printed
  `verify wrote into the install it was asked about:` and under it
  `?? DCS.openbeta/Scripts/Hooks/verify.log`. The same test carries a second
  assertion, a byte-for-byte snapshot of the fixture taken before the call,
  and it is not redundant: git does not track an empty directory, so a
  `create_dir_all` on the way to a read is something git would call clean
  and the snapshot would not. This mutation reddens the porcelain assertion
  first, so the snapshot's own message is not observed here. No other check
  goes red: nothing else in the module looks at what is on disk after the
  call, which is what says this control watches the writing and not the
  reading.

```sweep-edit crates/dcs-mcp/src/verify.rs
-     let hooks = variant.as_path().join("Scripts").join("Hooks");
+     let hooks = variant.as_path().join("Scripts").join("Hooks");
+     let _ = fs::write(hooks.join("verify.log"), b"");
```

### verify/app-version-treated-as-a-failure

- task: T46
- command: `mise exec -- cargo test -p dcs-mcp verify`
- reddens: `verify::tests::a_differing_app_version_is_a_difference_and_never_a_refusal`
- note: this is why the report has no problem variant a version could be
  filed under — there is nowhere for a difference to be put, so the only way
  to treat one as a fault is to let it decide the verdict, which is the
  defect exactly. It compiles because `VersionCheck` is imported by name for
  the field's own type. One check goes red and no other, because every other
  fixture is handed a measured build that agrees or none at all; the pair
  with `a_matching_app_version_says_so` is what says the control watches the
  difference rather than the field being read at all. The observed message
  is the verdict assertion's own,
  `a build that differs from the one measured is a difference, not a
  refusal`. The `Display` assertion in the same test — that the rendered
  report still ends `verified` — would go red with it, but the verdict is
  asserted first and is what was seen.

```sweep-edit crates/dcs-mcp/src/verify.rs
-         self.problems.is_empty() && self.session.problems.is_empty()
+         self.problems.is_empty()
+             && self.session.problems.is_empty()
+             && !matches!(self.app_version, VersionCheck::Differs { .. })
```

### verify/stray-prefix-aimed-at-the-wrong-project

- task: T52
- command: `mise exec -- cargo test -p dcs-mcp verify`
- reddens: `verify::tests::a_second_hook_file_beside_ours_is_named_a_problem`
- note: the one prefix is swapped rather than widened or emptied, and that is
  what makes one edit kill both halves of the control at once. `DcsEvalExecutor.old.lua`
  stops being named, so the report says nothing about a second copy of our own
  executor polling the one transport root; and `DcsApiEval.lua` starts being
  named, which is the report auditing a directory it was given one file in —
  the thing ADR 0022 took out. Four assertions follow the fixture and the first
  is what is seen, the rest never running: the stray list comes back holding
  `DcsApiEval.lua` where `vec![&ours_again]` names `DcsEvalExecutor.old.lua`, so
  the printed pair is not an empty list against a full one but the two files
  swapped — which is the mutation's whole shape, read straight off the failure.
  It compiles clean, the array's length being written
  `[&str; 1]` either way. `a_healthy_install_verifies` stays green, because a
  fixture with no second copy in it has nothing for either prefix to match —
  which is the pair saying the control watches what is named and not merely
  that something is.

```sweep-edit crates/dcs-mcp/src/verify.rs
- const STRAY_PREFIXES: [&str; 1] = ["dcseval"];
+ const STRAY_PREFIXES: [&str; 1] = ["dcsapi"];
```

### verify/relaunch-leftover-heartbeat-not-verified

- task: T46
- command: `mise exec -- cargo test -p dcs-mcp verify`
- reddens: `verify::tests::a_relaunch_the_last_sessions_heartbeat_outlived_verifies`
- note: the mutation is `status`'s, because `verify` prints the report
  `status` makes. It is here so that the verb that failed live on
  2026-09-21 has a control of its own: with no leftover recognised, the
  fixture ends `not verified: 2 found`, as the live run did (ADR 0030).

```sweep-edit crates/dcs-eval/src/status.rs
-     beat.stamp != h.stamp && published.is_some_and(|at| beat.modified < at)
+     false
```

---

## Stage 9 — proven live

T63 is the developer-only row here: the installer verbs T52 starts from. Its
controls are swept like any other. T62's switch went when the reads it
selected went on by default (ADR 0031), and its four controls with it.
T47, T48 and T50 need a running game for their figures, but the instrument
that takes them, `dcs-mcp live`, runs off DCS, so its controls are entries
here too, and so are the ones for what T50's figures decided.

### installer/ambiguity-picked

- task: T63
- command: `mise exec -- cargo test -p dcs-mcp installer`
- reddens: `installer::tests::three_variants_are_refused_every_one_named_and_nothing_written`
- note: an unnamed variant defaults to the plain `DCS` folder, the guess a
  machine with three variants and only `DCS` in use invites. The mutant
  installs into `DCS` and exits 0, so the check reddens at the exit code
  before its snapshot. `one_variant_needs_no_name` reddens beside it: a lone
  `DCS.openbeta` with the default `DCS` is refused as a variant not there,
  the same defect from the other side. Every other test names its variant
  and stays green, and so does `locate/ambiguity-picked`: `locate` is
  unchanged, and this watches the verb handing it an answer nobody gave.

```sweep-edit crates/dcs-mcp/src/installer.rs
-         .target(parsed.variant.as_deref(), None)
+         .target(parsed.variant.as_deref().or(Some("DCS")), None)
```

### installer/replace-assumed

- task: T63
- command: `mise exec -- cargo test -p dcs-mcp installer`
- reddens: `installer::tests::a_foreign_hook_is_refused_without_replace_and_nothing_written`
- note: the flag read and then dropped between the command line and the
  placement. `install/foreign-hash-replaced-without-replace` watches the
  library's guard; this watches the wire to it.
  `replace_parks_the_foreign_hook_and_installs` stays green. The mutant
  warns that `replace` is a field never read; the warning is not the red,
  the assertion is.

```sweep-edit crates/dcs-mcp/src/installer.rs
-     let replace = parsed.replace;
+     let replace = true;
```

### installer/verify-exit-ignores-problems

- task: T63
- command: `mise exec -- cargo test -p dcs-mcp installer`
- reddens: `installer::tests::verify_exits_one_when_anything_is_found`
- note: the report still ends `not verified: N found`, and only the exit
  code lies, which is the defect for a script or an agent that reads the
  code, as T52 and T51 are run.
  `verify_exits_nought_on_a_healthy_install_with_a_session` stays green. The
  mutant compiles clean.

```sweep-edit crates/dcs-mcp/src/installer.rs
-     Ok(i32::from(!report.verified()))
+     Ok(0)
```

### installer/verbs-not-dispatched

- task: T63
- command: `mise exec -- cargo test -p dcs-mcp --test installer`
- reddens: `installer_the_binary_installs_verifies_and_uninstalls`
- note: with `takes` answering nothing, `main` falls through to `dcs-mcp does
  not take install` and exit 2. Every unit test stays green, because they
  call `run` directly and never go through `takes`; only the real binary can
  see this. `installer_the_binary_refuses_an_ambiguity_with_exit_one` reddens
  beside it, expecting 1 and given 2. The mutant compiles clean: `word` is
  still read, and `verb_of` is still called by `parse`.

```sweep-edit crates/dcs-mcp/src/installer.rs
-     verb_of(word).is_some()
+     word.is_empty()
```

### installer/verify-host-ignored

- task: T63
- command: `mise exec -- cargo test -p dcs-mcp installer`
- reddens: `installer::tests::verify_reads_the_session_of_the_host_it_is_given`
- note: `--host` parsed and then dropped, so `verify --host export` reports
  the hook's session, which is not there, and exits 1. Every other test
  leaves `--host` out and stays green. The mutant compiles clean.

```sweep-edit crates/dcs-mcp/src/installer.rs
-         host: host.unwrap_or(Host::Hook),
+         host: { let _ = host; Host::Hook },
```

### installer/data-dir-always-named

- task: T63
- command: `mise exec -- cargo test -p dcs-mcp installer`
- reddens: `installer::tests::the_snippet_names_a_data_directory_only_when_the_line_did`
- note: the snippet spells the known data directory into the client's
  config though the line never named one. Every test that runs `install`
  passes `--data-dir`, so none of them can tell; the check calls the
  choice directly, since an `install` without `--data-dir` would reach the
  real `%LOCALAPPDATA%`. The mutant warns that `parsed` is unused; the
  warning is not the red, the assertion is.

```sweep-edit crates/dcs-mcp/src/installer.rs
-     parsed.data_dir.as_ref().map(|_| data.path())
+     Some(data.path())
```

### live/row-without-a-figure-dropped

- task: T47
- command: `mise exec -- cargo test -p dcs-mcp live::report`
- reddens: `every_row_prints_over_an_empty_ledger`

```sweep-edit crates/dcs-mcp/src/live/report.rs
-             None => writeln!(out, "{}", unmeasured(row))?,
+             None => continue,
```

### live/dormant-stat-on-the-armed-arm-file

- task: T48
- command: `mise exec -- lua5.1 tools/harness.lua live/dormant-probe`
- reddens: `the dormant stat is of a path that does not exist`

```sweep-edit crates/dcs-mcp/src/live/dormant.lua
- local absent = E.arm .. ".absent"
+ local absent = E.arm
```

### live/second-read-in-one-session

- task: T50
- command: `mise exec -- cargo test -p dcs-mcp live::read`
- reddens: `a_second_opt_in_read_in_one_session_is_refused`
- note: the hunk deletes the line. `stamp` is then left unused, which warns
  and does not stop `cargo test`; the assertion is the red. The function it
  called is still read by `run_all`, which checks the session its own way,
  so `all_is_refused_in_a_session_that_already_had_a_read` stays green:
  this control watches the single read's refusal alone.

```sweep-edit crates/dcs-mcp/src/live/read.rs
-     refuse_a_second(&rows, stamp)?;
```

### live/all-sends-past-a-read-that-did-not-answer

- task: T50
- command: `mise exec -- cargo test -p dcs-mcp live::read`
- reddens: `all_stops_at_the_first_read_that_does_not_answer`
- note: the hunk deletes the return, so the run prints that it stopped and
  then sends the next read anyway, into a session that has just failed to
  answer — the one thing ADR 0028's sequence is not allowed to do. It
  compiles clean. The red is the exit code, 0 where 1 is asserted, printed
  before the ledger check that would also fail.

```sweep-edit crates/dcs-mcp/src/live/read.rs
-             return Ok(1);
```

### live/all-skips-a-read-tested-in-another-scene

- task: T50
- command: `mise exec -- cargo test -p dcs-mcp live::read`
- reddens: `all_runs_again_in_a_scene_it_has_not_been_run_in`
- note: the hunk drops the scene from the skip, so a read with a result at
  the menu is never sent in a mission — the one test the maintainer runs the
  reads twice for. `scene` is then unused, which warns and does not stop
  `cargo test`; the red is the second pass's request count, 0 where 6 is
  asserted.

```sweep-edit crates/dcs-mcp/src/live/read.rs
-             && e.scene.as_deref() == scene
```

### live/read-row-hides-a-scene

- task: T50
- command: `mise exec -- cargo test -p dcs-mcp live::report`
- reddens: `an_opt_in_read_prints_its_latest_in_every_scene`
- note: the hunk turns the per-scene arm off, so an opt-in read's row prints
  its newest entry alone and a result at the menu vanishes behind one taken
  in a mission. It compiles clean.

```sweep-edit crates/dcs-mcp/src/live/report.rs
-             Some(_) if row.key.starts_with("read.") => {
+             Some(_) if false => {
```

### live/read-recorded-only-after-its-answer

- task: T50
- command: `mise exec -- cargo test -p dcs-mcp live::read`
- reddens: `the_read_is_on_the_ledger_before_it_is_on_the_disk`
- note: the hunk skips the entry that says the read was sent, which is the
  ledger as it was when the outcome was the only write. The other
  `live::read` tests that count entries go red beside it, the `all_` ones
  among them, since `live read all` sends through the same line.

```sweep-edit crates/dcs-mcp/src/live/read.rs
-     ledger::append(data, &Entry::new("read", &row, session, SENT)).map_err(|why| {
+     let _ = SENT; Ok::<(), std::io::Error>(()).map_err(|why| {
```

### live/read-sent-beside-a-ping

- task: T50
- command: `mise exec -- cargo test -p dcs-eval game_reads`
- reddens: `alone_publishes_the_one_read_and_nothing_beside_it`
- note: `alone_answers_the_reads_value` goes red beside it, because the first
  item the window yields is then the ping's reply, which is not the read's
  grammar.

```sweep-edit crates/dcs-eval/src/reads.rs
-     let specs = vec![spec];
+     let specs = vec![Spec::new(&[("op", "ping"), ("for", h.stamp.as_str())], b""), spec];
```

### reads/dropped-read-put-back

- task: T50
- command: `mise exec -- cargo test -p dcs-eval game_reads`
- reddens: `the_read_that_crashed_dcs_is_never_published`
- note: the read that crashed DCS in a mission put back in the table, in the
  slot of the one beside it, which is the smallest edit that sends it again.
  Three more go red beside it: the list test, since the table no longer
  holds the callees it names; the unlisted test, since the vet now admits
  the read; and the alone test, which sends `mission_theatre` by its key and
  finds another callee on the disk.

```sweep-edit crates/dcs-eval/src/reads.rs
-         callee: "DCS.getMissionTheatre",
+         callee: "DCS.getMissionLoaded",
```

### reads/editor-map-read-in-the-hook

- task: T50
- command: `mise exec -- cargo test -p dcs-eval game_reads`
- reddens: `the_editor_map_is_read_in_the_gui_state`
- note: every read then goes to `hook`, where `MapWindow` is not, so the
  state-count tests go red beside it.

```sweep-edit crates/dcs-eval/src/reads.rs
-             ("state", read.state()),
+             ("state", "hook"),
```

### reads/gui-table-unguarded

- task: T50
- command: `mise exec -- cargo test -p dcs-eval game_reads`
- reddens: `a_gui_read_whose_table_is_nil_answers_a_raise_naming_it`
- note: without the guard the index of a nil `MapWindow` raises outside the
  pcall, so the reference interpreter stops and the chunk does not run to
  the end.

```sweep-edit crates/dcs-eval/src/reads.rs
-     if read.state() != "hook" {
+     if read.state() == "never" {
```

### game/editor-map-read-backwards

- task: T50
- command: `mise exec -- cargo test -p dcs-eval game_state`
- reddens: `the_editor_map_tells_the_editor_from_the_menu`
- note: the headline test at the menu goes red beside it, since it reads the
  same answer through the whole derivation.

```sweep-edit crates/dcs-eval/src/game.rs
-         Ok(true) => Activity::Editor,
+         Ok(true) => Activity::Menu,
```

### game/refused-editor-read-picked

- task: T50
- command: `mise exec -- cargo test -p dcs-eval game_state`
- reddens: `a_refused_editor_read_leaves_menu_or_editor_standing`
- note: the `gui` state out of reach is then read as the menu, which is a
  pick the evidence does not make. The headline test at the menu goes red
  beside it for the same reason.

```sweep-edit crates/dcs-eval/src/game.rs
-         return Activity::MenuOrEditor;
+         return Activity::Menu;
```

### game/placeholder-shown-as-the-name

- task: T50
- command: `mise exec -- cargo test -p dcs-eval game_state`
- reddens: `the_placeholder_name_is_never_shown_as_the_missions`
- note: the placeholder spelt as something DCS never answers, so
  `tempMission` is shown as the mission's name, which is what the old
  answer did.

```sweep-edit crates/dcs-eval/src/game.rs
- const PLACEHOLDER: &str = "tempMission";
+ const PLACEHOLDER: &str = "neverMission";
```

### game/doubled-slash-shown

- task: T50
- command: `mise exec -- cargo test -p dcs-eval game_state`
- reddens: `a_doubled_slash_in_the_mission_file_is_shown_once`
- note: the placeholder test goes red beside it, since it holds the tidied
  file too.

```sweep-edit crates/dcs-eval/src/game.rs
-         if c == '/' && out.len() > 1 && out.ends_with('/') {
+         if false {
```

### game/theatre-dropped

- task: T50
- command: `mise exec -- cargo test -p dcs-eval game_state`
- reddens: `a_mission_names_its_theatre_and_the_players_unit`

```sweep-edit crates/dcs-eval/src/game.rs
-             theatre: string_of(theatre),
+             theatre: None,
```

### game/player-unit-dropped

- task: T50
- command: `mise exec -- cargo test -p dcs-eval game_state`
- reddens: `a_mission_names_its_theatre_and_the_players_unit`

```sweep-edit crates/dcs-eval/src/game.rs
-             unit: string_of(unit),
+             unit: None,
```

---

## Out of scope

One group is left here: the runner's own two, which are the two edits T57's
cell names and are proved by `tools/sweep-test.sh` instead. Nothing re-derives
an out-of-scope count — `tools/sweep-cover.sh` checks that a Stage 0–8 row has
an entry, not what any row's count is — yet each is part of the figure every
run prints, so a group added here later carries its per-row working beside it
and can be re-counted against the plan.

The rule: one for each distinct code edit a row's `mutations:` clause names, so
a cell naming two edits counts two. An edit named to show that a control does
*not* fire — the comment byte in T16, which must leave the interop control
green — is not counted, because the sweep has no verdict for an edit that is
meant to change nothing.

**So the figure a run prints counts mutations the plan names, and not every
mutation this build has seen go red.** Several sessions broke more than their
cell asked for — the handoffs recorded fifteen for T25, thirteen for T26,
twelve for T35 and eight for T36, where each cell names one or two — but they
recorded counts, not edits, so there is nothing here to re-run them from, and
a mutation rebuilt from a description would be a new control rather than the
old proof. They are in neither figure. A row that wants one of them watched
names it in `docs/PLAN.md` first and writes it anew; it then gets an entry
here like any other, and both figures move. T53's fifth was recorded exactly
and went that way.

### out/the-runner-itself

- task: T57
- out-of-scope: the sweep cannot sweep itself — a mutation of the runner would
  be applied by the runner it broke. Its own two mutations are proved by
  `tools/sweep-test.sh`, which drives a copy of it over a sandbox inventory.
- controls: 2
