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

Nothing. T35 landed; T36 has not started.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **T35** — the game-state reads: `cargo test -p dcs-eval game_reads` prints 53, `standin` 34, the crate 354. Nine reads in a constant table, one `pcall` each, tier 2 built and off; the ping, the five reads and the `gui` probe share one window and one tick. An answer is five arms with no default so an errored, absent, false and malformed read cannot be confused, its `Unanswered` carries the status and stage as fields rather than as prose, and the ping and the probe answer on types of their own — a probe that says `ok` is `Reachable` and not a body this side cannot read (ADR 0017), and `vet` refuses a never-sent or unlisted name on the bytes at the publication seam, with the stand-in's raw-byte ledger sweeping what actually reached the disk. **All twelve mutations reddened**, an agent watching each run and restoring from a saved copy; three did not say what was predicted — the named `DCS.getMissionLoaded` added to the table reddens the sweep by refusing the whole gather rather than by a request in the ledger, and the batched chunk and the depth of 1 redden through the ticker's own wait ("waited for 7 .req files and saw 3") rather than on a count. Tier 2 sent while off left the tier-two skip check green, because two entries then sat under one key and the lookup found the first; it now counts entries too, in a commit of its own. **Every answer here is the stand-in's, told what to say**: what a real `DCS.getPause` returns, whether the five answer on a joined client and whether any crashes the hook state are Stage 9's. The never-send claim is about what leaves this side, which is the claim it can support. A review after that sweep moved the probe and the ping onto answers of their own and `Unanswered` onto fields; those are covered by tests and by no mutation.
- **T56** — the event-driven reply watch: `cargo test -p dcs-eval watch` prints 13 and the crate 296. `ReadDirectoryChangesW` on `res/` over an overlapped handle and an auto-reset event, with the 25 ms poll kept as **that event wait's timeout** rather than as a branch taken on error — one `min(left, poll)` per pass, handed to whichever mechanism is there, and the directory listed afterwards whichever of the three things ended the sleep. There is no runtime detection of a broken watch anywhere, deliberately: the failure the poll exists for reports nothing, so code waiting to notice would never notice, and the only seam is a pace a test can ask for that opens no watch at all (ADR 0016, which also holds why the drain's answer is recorded in a sink the watch owns rather than returned). **A wake is told from a poll by a tally and never by a stopwatch** — the poll set an hour away, the reply renamed in 150 ms, and `polls == 0` the load-bearing half. The buffer and the `OVERLAPPED` sit in one heap allocation held **by raw pointer and not by `Box`** — a `Box` asserts nothing else refers to what it points at, which is false for every moment a read is outstanding — and `Drop` cancels, drains and only then reclaims it, which is what covers the `?` and the panic as well as the four returns; the watch lives in `wait`'s own local and has no public constructor, so "no handle once `wait` returns" is where the value sits rather than a rule. **The cancel is observed to have run and completed, not proved:** deleting it and keeping the handle close leaves every handle check green, because closing the handle does release the directory — what reddens is the drain sink, `None` where `995` was wanted, and the handle check staying green under that mutation is the finding. The handle check's positive control was run first and alone: a directory a `Changes` holds refuses `remove_dir` with os error 32 here. Eight of eleven mutations reddened. A watch left open across a `wait` reddens the handle check with "could not be removed once the wait returned" and leaves the superseded test green; the cancel and drain dropped reddens the two drain checks alone; the re-arm dropped reddens the foreign-reply check *and* the wake check, both on `polls`; an eager open reddens seven including the three `opens: 1, wanted 0`; `FILE_SHARE_DELETE` added lets the removal straight through, so excluding it is load-bearing and the comment beside the share mode now says what was seen. **Three did not redden.** The poll removed left the fallback check green at first — the loop lists the directory before it reads the deadline, so a wait that slept through the reply still found it on the way out — and the check was strengthened to count the looks (`polls: 1, wanted at least 4`) in a commit of its own; that gap is worth remembering for any check phrased as "it looked again". A manual-reset event stayed green and did not spin, because `ReadDirectoryChangesW` clears the event as it starts each new read, so the re-arm does the reset the auto-reset would — the auto-reset choice is unproved here rather than proved. `GetOverlappedResult` asked to wait in `woke` stayed green, as expected: it returns at once when the read is complete, and the reason to pass `FALSE` is the case a manual-reset event manufactures, not the ordinary one. `align(8)` dropped is green too and guards a later field reordering. An agent verified every claim on this machine under `mise exec -- cargo test`, restoring each mutation from a saved copy and `cmp`ing it back, and ran the watch suite 31 times over with no flake. **What it did not do:** see DCS. Whether the watch reports at all on the filesystem a user's `Saved Games` sits on is unknown and is exactly what §3.2 says is silent when it does not — every event here is this suite's own rename into a sandbox. Nothing measured the round trip, so §3.2's "the 12.5 ms poll term disappears" is unobserved and Stage 9's. And the use-after-free the cancel prevents was not hunted for beyond the tests: the pinned stable toolchain refuses `-Zsanitizer` ("failed to run `rustc` to learn about target-specific information"), and `appverif.exe` is present but needs elevation this session did not have, so no sanitizer and no Application Verifier run happened. **A review then found the drain unbounded**, and reproduced it: with the cancel made to fail, `GetOverlappedResult(bWait=TRUE)` waited on an auto-reset event nothing would set again and the test binary had to be killed after ten minutes, with nothing written to the sink either. `quiesce` now reads `CancelIoEx`'s return, treats `ERROR_NOT_FOUND` as the one failure it may carry on from, bounds the wait at five seconds on the completion event and collects with `bWait=FALSE`; where any of that gives up it records why and **strands** the allocation — leaked, not freed, because a read still the kernel's makes freeing the corruption and waiting the hang — and stranding is sticky and refuses a re-arm. **None of the give-up paths has a fixture**: reaching one needs a handle or a kernel misbehaving in a way nothing here can stage, and the seam that would stage it would sit in the crate's only unsafe, so they are reasoned and not observed. The fallback check's reply now lands at 500 ms rather than 150 against the 25 ms poll, so the `polls >= 4` floor is five times under the expectation instead of under half of it and scheduler noise cannot redden it. The named mutation was re-run on the restructured code: a watch left open across a `wait` (`mem::forget` on the deadline path) reddens both the handle check — os error 32, "could not be removed once the wait returned" — and the drain check with `drained: None`. A second review found the wake-count check a **ceiling alone**: a watch made silent left it green at `events: 0`, so it now carries a floor too, and that mutation reddens it. The same review found the foreign-reply check opening a second stand-in, whose `res` is cached off a stamp minted at open, so two opens straddling a second boundary put both replies in a directory nothing was watching; it publishes both from the one stand-in now.
- **T55** — the file-source reader, the other half of `evalFile`: `cargo test -p dcs-eval file_source` prints 40 and the crate 275. `source::read` takes T34's `Admitted` and **takes no headers, and derives no name**: it puts the body behind the very block `check` framed and reads the `chunkname` off that same block — `check` lifts the value out of the slice it framed and keeps it beside the bytes — so "the reader cannot disagree with what is being sent" is structural and there is no second set to disagree, which is why no `HeaderDrift` mutation exists to run. A request naming no chunk records `=dcs-api-eval`, the far end's own default, rather than a path nothing carries. The order is name, open, read, size, mark, shebang, empty, hash: the 200-byte cap falls on the header value going out because the framer caps no value at any length, the size is compared against the raw bytes before any rule shortens them, the mark comes off *before* the `#` line is looked for, and the digest is over the body that is sent and not the file on disk (ADR 0015, which also holds the two thresholds and the backslash spelling the record's example line does not use). SHA-256 is written in the crate per ADR 0011 and proved twice: the three published vectors, and `sha256sum` on a fixture at every length 0..=130 and over 300 kB — `sha256sum` marks a line whose name holds a backslash by putting one in front of the digest, which every Windows path here earns, so the helper strips it. **What Lua prints is 52 bytes, not 60**: measured on the pinned 5.1.5 at name lengths 50 to 90, a raise shows the whole path at 52 and `...` plus the last 52 from 53 up, a compile error the whole path at 72 and `...` plus the last 72 from 73 up — 60 is `LUA_IDSIZE`, the buffer, and `luaO_chunkid` spends eight of it before it copies. Both frozen documents print 60 and both are describing the buffer; an assertion written against it would have been wrong in both directions. So `chunkid` is pinned against the interpreter at seven lengths and the line-47 control computes its expectation through it, with two guards in front — the path is past the threshold, and the message begins `...` — so a fixture that stopped exercising the case fails rather than passes. Twelve mutations were seen red. The shebang dropped rather than blanked reddens four, the line-47 test among them with `line47.lua:46:` against `:47:`, which is the whole reason the fixture counts to 47; a `\r\n` converted on the way in reddens the byte-for-byte control with a `13` missing in front of every `10`, and the line-47 test stays green on 47 because Lua counts both alike — which is why the bytes are the control and the line number is not; the hash taken over the bytes as read reddens the two-readings test alone; `Bom::None` although one was stripped reddens four; the mark stripped *after* the `#` test reddens three, the line-47 file then failing to compile at all on `unexpected symbol near '#'`; the `Empty` refusal dropped reddens three, each handing back a `Source` of zero bytes; the 200-byte cap dropped reddens the header-value test; the name derived from the path rather than read off the block reddens all three name tests; the eight-byte reserve cut to zero reddens four including the line-47 test; the cap taken off the read — the whole file buffered before the ceiling is applied — reddens three, the four-megabyte fixture among them, and the `Grew` check dropped reddens one, while the same check moved past the two rules reddens the zero-headroom fixture that grows by a mark alone — the raw-bytes comparison is ADR 0015 reading 7. **One mutation did not redden**: `FILE_FLAG_OPEN_REPARSE_POINT` taken off the open leaves the swapped-leaf control green, because a junction followed leads to a directory and opening a directory as a file is refused anyway — so the flag's own contribution is unproven on this machine and the test says so in its comment. The `Relinked` arm is unproven with it: a file symlink here needs elevation (`mklink`, observed unelevated: "You do not have sufficient privilege to perform this operation."), and the swapped-leaf test asserts the refusal it really saw, `Open` with "Access is denied." An agent verified every claim on this machine under `mise exec -- cargo test`, restoring each mutation from a saved copy and `cmp`ing it back. **It did not see DCS.** The raise on line 47 is the reference interpreter's under a driver reading the body in `"rb"`, not a game's; the two thresholds were measured on Lua 5.1.5 PUC-Rio on this box and not inside a game process; the ceiling comes off a stand-in's handshake; and nothing here has been *sent* — MCP §7's "one at the ceiling is sent whole" is proved as far as *framed to exactly `max_request_bytes`*, no further.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Task T36** — the game-state derivation: the axes, `unknown` as a value, no
default arm. Done when `cargo test -p dcs-eval game_state` shows `loading` with
no round trip, `paused (read)` noting a disagreeing callback phase, `session:
client` on a `refused` `gui` probe, `unknown: <error>` on an errored axis alone
and `unknown (tier 2 off)` when off; the mutation is any axis filled from
another's evidence, reddening a no-default-arm check.

**An agent verifies** it here, and nothing in it sees DCS. T35 leaves it
`Answer`'s five arms, `Unanswered`'s status and stage as fields, `Probe`'s
three answers, a `ping` that says why it left no envelope, and `script`.

## After that

- **Stages 0 to 2 are closed, and Milestone A with them:** the wire proven off
  DCS, the stand-in, the interop and round-trip controls. T17 closed it.
- **Stage 3 is closed:** eval across the carriers with `<file>:47` true in
  every state, and the `a_do_script` shift reproduced.
- **Stage 4's path is closed and Stage 5 has opened:** `tick-budget: 524`, `instr-budget: 2209`, `fence: 583`, `events: 114`, `dormant: 40`, `arming: 95`, `heartbeat: 300`. Stage 6 has opened on the client side: `paths: 15`, `readers: 24`, `wait: 32`, `status: 24`, `id: 7`, `pipeline: 17`, `file_refusals: 27`, `file_source: 40`, `watch: 13`, `game_reads: 53` under `cargo test`, the crate at 354.
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
  with a file the suite keeps open. T56's directory handle refuses a removal here
  too; that no server holds one between tool calls is T41's.
- **The round trip's `superseded` half is unblocked and unclaimed.** T17's row
  parks it on T54, which landed the fence against the stand-in and does not ask
  for it; no row claims it against the shipped Lua.
- **A `Minter` has no owner.** The window mints from whatever it was handed; who
  holds one across tool calls, so two do not restart at seq 1, is Stage 7's.
  `Minter::seeded_at` resumes a counter.
