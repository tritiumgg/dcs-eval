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

Nothing. T33 landed; T34 has not started.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T33** — the client's `pipeline(specs, W)` and the id nothing minted before: `cargo test -p dcs-eval pipeline` prints 16, `id` 7, and the crate 205. An id is `<seq>-<tag>`: the counter is zero-padded to ten digits, which is what makes "lowest id" and "published first" the same question the executor's sorted listing answers, and the tag is eight base36 characters fixed for one minter's life, seeded per ADR 0011 from the process id, the clock, a stack address and a process-local counter. The counter is the honest part: it is what makes two minters in one process distinct by mechanism, which is exactly and only what the 1,000-minter test claims — nothing here says anything about two *processes* started in the same millisecond, and the test says so in its own comment. The encoder reduces modulo 36^8 and left-pads, because a naive base36 of a small draw is under `is_id`'s four characters and of a huge one over its twelve; both ends are pinned on the encoder directly rather than on one lucky draw. The window is W *published* requests, refilled before the yield and never after, and the head of the line is waited on by name however long ago anything behind it landed. ADR 0013 decides the five answers the frozen text leaves open: a timed-out head yields `pending` and frees its slot, a spec the framer refuses waits in the queue and is yielded in its own place without costing the window a request, a terminal head stops publication and each id still in flight is collected once before it is reported *in the head's own terminal word* rather than always `superseded`, a `wait` that cannot read the session ends the drain once and for ever and hands its window back by name through `in_flight()` rather than draining it, since a published request runs whether or not the client can read the answer and the same unreadable session would fail every one of them in turn, and a counter with no ten-digit `seq` left is yielded *behind* the window it had already published rather than in front of it, once, ending the drain — in front of it the requests on the disk would never be waited on and their ids would never reach the caller, and without the ending `for outcome in pipeline` would never return. The ordering control is the one that matters and it forces disorder on the disk rather than by sleeping: a new `Standin::tick_with` lets the fixture wait for all four requests to be published, answer 4, 3 and 2, then wait for three replies plus 100 ms — four of the client's polls — before answering 1, and the test asserts both that the yields ascend and that the fixture really did answer backwards, so it can never pass quietly. The window is proved from the session's side, never from the client's bookkeeping: nine specs three deep give the census `[3,3,3,3,3,3,3,2,1]`, which a batch of three cannot produce (it gives `[3,2,1,3,2,1,3,2,1]`), and a second reading on the other side of W catches a flood. Seventeen of the twenty mutations were seen red — a true arrival-order head reddens the backwards control (`left: [02,03,04,01] right: ascending`) and leaves the in-order baseline and both window checks green, which is why the baseline cannot be the control; batching reddens the census with `[3,2,1,3,2,1,3,2,1]` and flooding with `saw 9 requests in req/, wanted at most 3`; a tag drawn per id reddens the one-tag check with 50 tags; an unpadded seq reddens the sort across the power-of-ten boundary; the mixer's last xor-shift dropped reddens the golden tag; the seq guard dropped reddens the eleven-digit refusal; an encoder neither reduced nor padded reddens `draw 0 encoded as 0` at one character and the golden tag at thirteen; the pick ignored reddens both stand-in checks *and* the backwards control's second assertion, which is how that assertion earns its place; publication past a terminal outcome reddens six requests where three were wanted; a neighbour reported without its one `collect` reddens the reply that landed before the kill; a held slot behind a `pending` reddens with `[01,01,01,01]` against the four ids; a refused send ending the drain reddens with two items where five were wanted; a `WaitError` kept in flight reddens "the drain ends there"; a neighbour whose `collect` failed dropped from the window reddens "both neighbours are still nameable"; an exhausted counter reported in front of the window reddens with the refusal alone where the id it had published was wanted first; and the same refusal yielded without ending the drain reddens with four copies of it against two items. Two were argued and not run, and the argument is in the comment and in ADR 0013: refilling after the yield rather than before reddens only intermittently, and a neighbour given `superseded` regardless of the head's kind has no fixture, because the crate cannot produce a `dead` head with a window in flight without inventing a whole test for a one-line branch. One was run and did **not** redden: dropping the process-local counter from the seed left the 1,000-minter check green on this box three times over, the clock separating adjacent calls by itself, so the counter's contribution is unproven here rather than proved. An agent verified every claim itself on this machine under `mise exec -- cargo test`, restoring each mutation from a saved copy and `cmp`ing it back. It did not see DCS: every reply came from the stand-in, which answers every request it lists by construction, so "answers until the tick budget is spent and the rest waits for the next tick" — the thing the window exists to exploit — is still unobserved, and `Minter` has no owner yet. `README.md` needed no change; it carries no library-API prose.
- **T32** — the client's `status()`: `cargo test -p dcs-eval status` prints 24, and the crate 180. It returns a value and never a `Result`, which is the answer to "is a problem a refusal": no handshake, one that will not parse, a heartbeat left by somebody else and a process that is gone are each a `Problem` in a list beside whatever of the session could still be read, and a heartbeat absent because nothing has armed is no problem at all. The read budget is the handshake, the heartbeat, one pid probe and one stat of the arm path, and the control proves it rather than asserting it: four directory listings identical across the call, those same four directories' modification times unchanged, both files' bytes *and* modification times unchanged — so reading the heartbeat did not refresh the stamp its age comes from — no arm file made or removed, and the output directory renaming afterwards, which Windows refuses while a handle under it is open. The stamps are what sees a file that appeared and vanished inside the call, which the listings alone cannot and which wakes the executor just the same; the clock is proved past every recorded stamp first, so a create during the call stamps later whichever way it ends. What is still unseen is the event rather than its trace: a create in a directory none of the four is, a filesystem that does not stamp its directories, and the order things happened in. Watching a create as it happens wants T56's directory watch, which this does not have. The arm stat has three answers and not two, for the reason the pid probe has three: only a `NotFound` is the file's absence, and every other failure is `Undecided` beside a problem naming the path, never folded into "nothing has armed this session". It is reached through a pure `arm_of` for the reason `process_of` is pure — the refusal would have to be arranged with an access control list, and an elevated host traverses what an unelevated one does not. The heartbeat's path comes from `wait`'s own `Session`, the file is read before its stamp is looked at, and `armed` is read before any age, so the two cannot drift: a dormant age is `Age::Dormant` and nothing in the module reads one as evidence. Stamp, host and transport are all three compared against the handshake, the transport by mutual containment rather than `==`, since a `Real` compares bytes and only `contains` folds case — a case difference reported as two installs would be a false finding in the first line a user reads. `app_version` is compared with `MEASURED_ON`, which is `None`: nothing has been measured, so every report says `unmeasured` rather than inventing a build, and a difference is never a problem. All eleven mutations were seen red — a `publish::send` at the end of `status_at` reddens the cost control's directory leg (`0000000001-aaaa.req` and `arm req res` against empty and `req res`), and no longer the arm test with it, because the stat now runs before the send rather than in the returned struct; a request published and removed again inside `status_at` leaves that leg green and reddens the stamps beside it; an unconditional `Ticking` reddens the dormant age (`saw Ticking(600s), wanted Dormant(600s)`); `Unknown` folded into `Exited` reddens the constructed probe (`left: Exited, right: Undecided`) and pid 4 with it on this unelevated host; the transport push dropped, the stamp push dropped and the host push dropped each redden their own check with `[]`; `==` for the transport reddens the case fixture with a `SUB`/`sub` pair; an unmeasured build read as `Same` reddens both version checks; the `NotInstalled` push dropped reddens both handshake checks; and a `File` leaked on the heartbeat reddens the rename with `Access is denied`. An agent verified every claim itself on this machine under `mise exec -- cargo test`, restoring each mutation from a saved copy and `cmp`ing it back. It did not see DCS: every fixture is the stand-in's bytes or a hand-framed envelope, so no real handshake or heartbeat has been through this; a real session's `lfs.tempdir()` is the host's and the fixtures say so by hand, which is the agreement Stage 9 has to check; and the probe ran against this process, a reaped child and pid 4, never against a game that crashed.
- **T54** — the client's `collect`, `wait` and the outcome table: `cargo test -p dcs-eval wait` prints 32 (29 in `wait`, 3 in `sys`), and the crate 156. `collect` checks the id before it becomes a path, reads only the exact `<id>.res` and never the `.tmp` beside it, and discards a reply on another stamp as `foreign` naming both spellings, leaving the file where it is; a reply with no stamp at all is a refusal. `decide` is the §4.4 table as an ordered list over two file reads and one probe, with no sleeping in it: the stamp beats any heartbeat, `armed` is read before the age, and all eight rows are proved — `superseded` over an armed hour-old beat and a gone pid, `pending` fresh and armed (also with a gone pid, which proves the probe is not consulted while fresh), `waking`, `stalled`, `load` at any age both sides of `armed`, and `dead` both ways. Liveness is `OpenProcess`/`WaitForSingleObject(0)`/`CloseHandle` in `sys.rs`, the crate's only `unsafe`, declared per ADR 0011 and driven against real handles: this process, a reaped `cmd /C exit 259` held across the probe, and pid 4. ADR 0012 argues the four readings past the frozen table — `dead` while dormant, `Unknown` never terminal, an absent or foreign heartbeat as the dormant branch, and a `Sent` this process did not mint never `waking`. The heartbeat is read before its stamp is looked at, so one that is foreign *and* will not read is a refusal naming the file rather than the dormant branch, and `sys.rs` says what the probe cannot tell at all: Windows reissues a process id, so a stale one reads `Running` about whatever holds it now. `wait` is the 25 ms poll §3.2 keeps as the fallback, all its sleeping behind one function taking the reply directory it does not yet use, and a deadline past the end of the clock saturates to the furthest instant it can name rather than panicking; the watch is T56's and is not here. All four mutations were seen red — a dormant beat's age read as staleness reddens the `armed: no` rule (`saw Some(Stalled), wanted Some(Waking)`); `WAIT_OBJECT_0` read as running reddens the 259 child and both `dead` rows; the stamp comparison dropped reddens the foreign discard, the wait that goes on past one, and the no-stamp refusal; an unknown send time read as "just now" reddens `never_waking` and its `publish` twin. An agent verified every claim itself on this machine under `mise exec -- cargo test`, restoring each mutation from a saved copy and `cmp`ing it back. It did not see DCS: the pid probe is proved against processes this test started, not against a game that crashed, and no reply here came from the executor — the round trip's `superseded` half is still unclaimed.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T34** — what `evalFile` refuses before a byte is read: containment
against the roots, `Config\` and the install, and the ceiling taken off the
stat against the handshake's `max_request_bytes`, never assumed. Done when
`cargo test -p dcs-eval file_refusals` prints its count. Needs T30 and T31,
both landed.

**An agent verifies** it: `mise exec -- cargo test` on this machine. No DCS
is needed — every refusal is a path decision this side makes before the disk
is touched.

## After that

- **Stages 0 to 2 are closed, and Milestone A with them:** the wire proven off
  DCS, the stand-in, the interop and round-trip controls. T17 closed it.
- **Stage 3 is closed:** eval across the carriers with `<file>:47` true in
  every state, and the `a_do_script` shift reproduced.
- **Stage 4's path is closed and Stage 5 has opened:** `tick-budget: 524`, `instr-budget: 2209`, `fence: 583`, `events: 114`, `dormant: 40`, `arming: 95`, `heartbeat: 300`. Stage 6 has opened on the client side: `paths: 15`, `readers: 24`, `wait: 32`, `status: 24`, `id: 7`, `pipeline: 16` under `cargo test`, the crate at 205.
  Milestone B needs T53, the rest of Stages 5 and 6, the mutation sweep.
- **Stage 9** is the critical path and cannot be shortened by parallel effort.
  Everything provable off DCS is proved before it.

## Carries forward

Things that must not be lost between sessions. Delete an entry when it is
resolved, and say where. Mark an entry only the maintainer can settle. Ten
entries at most: an eleventh means something here is finished, or belongs in
`docs/decisions/` or `CLAUDE.md` instead.

- **Cutover — the incumbent goes before this executor installs.** `dcs-api-bridge`'s
  `DcsApiEval.lua` sits in the same `Scripts\Hooks\`; ADR 0002 is why the names
  differ; T52 proves two hooks on one root cannot happen.
- **Maintainer decision — when the MCP registration is swapped.** Claude Code still
  points at `dcs-api-bridge`; the swap strands a session mid-task, so it happens at
  Milestone C. ADR 0001.
- **What only Stage 9 sees.** The load sentinel covers two: the hook guard's
  swallow path, which has no seam until a raising stub sits on the frame path, and
  whether the export state survives between missions. The wrapper is proved under
  the suite's own carrier alone — what DCS does with one raising inside
  `net.dostring_in`, and whether every state has `setfenv` and `_G`, is unmeasured,
  the model's stubs evaluating nothing — and the carrier against the incumbent's
  measured shift and `...`, not T49's. Unmeasured too: DCS's `os.clock` and
  `os.time` against the harness's, a count hook raising inside either carrier or
  already held by a state (`none`, ADR 0005), what `dcs.log` renders around a
  crossing's markers, which the reader ignores (ADR 0007), and what
  `lfs.tempdir()` really gives, which every fixture supplies by hand.
- **Declared before served, and absent before counted.** The handshake
  publishes `ops`, `states`, `eval` and the five figures from the first load,
  the two instruction figures provisional. No `state` is `bad-request`: the maintainer's call, 2026-09-11.
- **A path with a byte past ASCII stops the load.** Header values are ASCII, so a user name past ASCII puts one in every path the handshake names and the load refuses, naming the header in `dcs.log` (`docs/audit.md`, Open: the spec says nothing).
  The client parses it as the executor does, the maintainer's call at T13; a
  spelling for such a path on the wire is the writer's side, unsettled.
- **The disarm owes a sweep, and no row asks for one.** Its heartbeat half is
  built (T29); the sweep on that same disarm is unclaimed.
- **The held sibling is a held file.** The sweep's "cannot be removed" path is proved
  with a file the suite keeps open; the client's own handle is T56's, unheld at T41.
- **The round trip's `superseded` half is unblocked and unclaimed.** T17's row
  parks it on T54, which landed the fence against the stand-in and does not ask
  for it; no row claims it against the shipped Lua.
- **A `Minter` has no owner.** The window mints from whatever it was handed; who
  holds one across tool calls, so two do not restart at seq 1, is Stage 7's.
  `Minter::seeded_at` resumes a counter.
