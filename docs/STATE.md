# Working state

**Last updated:** 2026-09-19

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

Nothing. T54 landed; T32 has not started.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T54** — the client's `collect`, `wait` and the outcome table: `cargo test -p dcs-eval wait` prints 32 (29 in `wait`, 3 in `sys`), and the crate 156. `collect` checks the id before it becomes a path, reads only the exact `<id>.res` and never the `.tmp` beside it, and discards a reply on another stamp as `foreign` naming both spellings, leaving the file where it is; a reply with no stamp at all is a refusal. `decide` is the §4.4 table as an ordered list over two file reads and one probe, with no sleeping in it: the stamp beats any heartbeat, `armed` is read before the age, and all eight rows are proved — `superseded` over an armed hour-old beat and a gone pid, `pending` fresh and armed (also with a gone pid, which proves the probe is not consulted while fresh), `waking`, `stalled`, `load` at any age both sides of `armed`, and `dead` both ways. Liveness is `OpenProcess`/`WaitForSingleObject(0)`/`CloseHandle` in `sys.rs`, the crate's only `unsafe`, declared per ADR 0011 and driven against real handles: this process, a reaped `cmd /C exit 259` held across the probe, and pid 4. ADR 0012 argues the four readings past the frozen table — `dead` while dormant, `Unknown` never terminal, an absent or foreign heartbeat as the dormant branch, and a `Sent` this process did not mint never `waking`. The heartbeat is read before its stamp is looked at, so one that is foreign *and* will not read is a refusal naming the file rather than the dormant branch, and `sys.rs` says what the probe cannot tell at all: Windows reissues a process id, so a stale one reads `Running` about whatever holds it now. `wait` is the 25 ms poll §3.2 keeps as the fallback, all its sleeping behind one function taking the reply directory it does not yet use, and a deadline past the end of the clock saturates to the furthest instant it can name rather than panicking; the watch is T56's and is not here. All four mutations were seen red — a dormant beat's age read as staleness reddens the `armed: no` rule (`saw Some(Stalled), wanted Some(Waking)`); `WAIT_OBJECT_0` read as running reddens the 259 child and both `dead` rows; the stamp comparison dropped reddens the foreign discard, the wait that goes on past one, and the no-stamp refusal; an unknown send time read as "just now" reddens `never_waking` and its `publish` twin. An agent verified every claim itself on this machine under `mise exec -- cargo test`, restoring each mutation from a saved copy and `cmp`ing it back. It did not see DCS: the pid probe is proved against processes this test started, not against a game that crashed, and no reply here came from the executor — the round trip's `superseded` half is still unclaimed.
- **T31** — the handshake and heartbeat readers: `readers: 24 checks`. Typed views over `protocol`, not a second parser: the handshake's 27 headers in the writer's order (`app_version` between `quiet_s` and the byte limits), the heartbeat's ten from ADR 0010. Every header is required, `ABSENT` is a value the header must still carry, and a header this version does not know is stepped over. The paths divide, and the module says why: `transport`, `req`, `res`, `arm` and `output` are resolved through `paths::resolve` and one that will not resolve refuses the file, because the client publishes into them; `lfs_tempdir` and `install_guard` are reported and never used, so one that will not resolve is kept as what the file spelt and why, which is the finding `status` exists to carry rather than a fault in the file. `armed` and `eval` are the two spellings the executor writes, case unfolded, or a refusal naming the field; there is no default. A heartbeat's age comes from the mtime taken off the same handle the bytes came from, and `since` and `started` are display only, never parsed. The stand-in now publishes both files: the session moved under `<output>\rpc\<stamp>`, which is the executor's own fallback, and `beat(at)` sets the mtime a test gives it. The interop control reads its own capture of the shipped executor's handshake through the typed reader, so a value the Lua respells reddens where the envelope check stays green — seen, with `eval: allowed` spelt `yes`. All three mutations were seen red — an absent `armed` defaulted to `no` reddens the absent-header loop at `armed` and the armed-spelling test, the mtime taken from `SystemTime::now()` reddens the age (`wanted at least 30s, saw 0ns`), and a relative path anchored under `C:\` rather than refused reddens the path refusal. An agent verified every claim itself on this machine under `mise exec -- cargo test`, restoring each mutation from a saved copy and `cmp`ing it back. No real heartbeat is read anywhere, and none is claimed: the interop run publishes replies on one frame and never writes `heartbeat.txt`, so that reader's fixtures are still the stand-in's bytes and hand-framed envelopes.
- **T30** — client path containment: `paths: 15 checks`. `resolve` refuses anything not absolute — `C:x` and `\x` included, because the drive they land on is an accident of where the client was started — then collapses `.` and `..` without letting `..` climb past the drive, canonicalizes the nearest existing ancestor, drops the `\\?\`, and reattaches the tail that does not exist yet. Only a `Real` has `contains`, which folds ASCII case over the encoded bytes and matches at a segment boundary. Proved: a short spelling and a junction each resolving to what they name, a laundering junction inside a permitted root not laundering anything, a short-spelled root still holding what sneaks in under it, `LogsX` not under `Logs`, a drive root holding everything on it, and a path no part of which resolves refused. Both mutations were seen red — resolution left textual reddens six, the boundary dropped reddens the sibling. An agent verified every claim itself on this machine under `mise exec -- cargo test`; the sandbox's short-name and junction helpers panic rather than skip where the volume cannot make one. It did not see, and does not claim, that a junction DCS would put in the way behaves as the one the test makes.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T32** — client `status()`: the two files, the PID probe, the problems
found, and the running `app_version` against the build the embedded executor
was last measured on — a difference, never a refusal, and `unmeasured` until
Stage 9 records one. Done when `cargo test -p dcs-eval status` shows a
foreign-stamp and a foreign-transport heartbeat each reported as a problem and
zero round trips; mutation: a round trip issued by `status` reddens the "costs
the executor nothing" check. Needs T31, landed. `wait.rs` already carries the
session, the probe and the readers it wants.

**An agent verifies** it: `mise exec -- cargo test` on this machine.

## After that

- **Stages 0 to 2 are closed, and Milestone A with them:** the wire proven off
  DCS, the stand-in, the interop and round-trip controls. T17 closed it.
- **Stage 3 is closed:** eval across the carriers with `<file>:47` true in
  every state, and the `a_do_script` shift reproduced.
- **Stage 4's path is closed and Stage 5 has opened:** `tick-budget: 524`, `instr-budget: 2209`, `fence: 583`, `events: 114`, `dormant: 40`, `arming: 95`, `heartbeat: 300`. Stage 6 has opened on the client side: `paths: 15`, `readers: 24`, `wait: 32` under `cargo test`, the crate at 156.
  Milestone B needs T53, the rest of Stages 5 and 6, the mutation sweep.
- **Stage 9** is the critical path and cannot be shortened by parallel effort.
  Everything provable off DCS is proved before it.

## Carries forward

Things that must not be lost between sessions. Delete an entry when it is
resolved, and say where. Mark an entry only the maintainer can settle. Ten
entries at most: an eleventh means something here is finished, or belongs in
`docs/decisions/` or `CLAUDE.md` instead.

- **Cutover — the incumbent goes before this executor installs.**
  `dcs-api-bridge`'s `DcsApiEval.lua` sits in the same `Scripts\Hooks\`; ADR
  0002 is why the names differ; T52 proves two hooks on one root cannot happen.
- **Maintainer decision — when the MCP registration is swapped.** Claude Code
  still points at `dcs-api-bridge`; the swap strands any session mid-task, so
  it happens at Milestone C and not before. ADR 0001.
- **What only Stage 9 sees.** The load sentinel covers two: the hook guard's
  swallow path, which has no seam until a raising stub sits on the frame path,
  and whether the export state survives between missions. The wrapper is proved
  under the suite's own carrier alone — what DCS does with one raising inside
  `net.dostring_in`, and whether every state has `setfenv` and `_G`, is
  unmeasured, the model's stubs evaluating nothing — and the carrier against the
  incumbent's measured shift and `...`, not T49's. Unmeasured too: DCS's
  `os.clock` and `os.time` against the harness's, a count hook raising inside
  either carrier or already held by a state (`none`, ADR 0005), and what
  `dcs.log` renders around a crossing's markers, which the reader ignores (ADR
  0007).
- **Declared before served, and absent before counted.** The handshake
  publishes `ops`, `states`, `eval` and the five figures from the first load,
  the two instruction figures provisional. No `state` is `bad-request`: the maintainer's call, 2026-09-11.
- **A path with a byte past ASCII stops the load.** Header values are ASCII, so
  a user name past ASCII puts one in every path the handshake names and the load
  refuses, naming the header in `dcs.log` (`docs/audit.md`, Open: the
  specification says nothing). The client's parser refuses such a value as the
  executor does, the maintainer's call at T13; a spelling for such a path on the
  wire is the writer's side, unsettled.
- **The disarm owes a sweep, and no row asks for one.** Its heartbeat half is
  built (T29); the sweep on that same disarm is unclaimed.
- **The held sibling is a held file.** The sweep's "cannot be removed" path is proved
  with a file the suite keeps open; the client's own handle is T56's, unheld at T41.
- **The round trip's `superseded` half is unblocked and unclaimed.** T17's row
  parks it on T54, which landed the client's fence against the stand-in but
  does not ask for it; no row claims it against the shipped Lua.
