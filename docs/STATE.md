# Working state

**Last updated:** 2026-09-21

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

**Stage 9 live run, paused 2026-09-21 at step 8 of 10** — DCS 2.9.29.27468, variant `DCS`. Done: install and verify (after PR #80 and ADR 0029); `live dormant --label menu` 0.0011 ms/frame against 0.098, an upper bound (the empty-loop floor capped before the clock); `live rtt` at the menu and in a mission, hook and export: p50 13.8–14.0 ms against 30, ~300–410 replies/s; `missionscripting` answered through `a_do_script` (T49's first half).
`live read all --label mission`: multiplayer false, server true, track false, player_id 0; **`mission_loaded` crashed DCS** — ACCESS_VIOLATION in lua.dll `luaS_newlstr` ← `lua_next` ← edCore `ED_lua_copyindex`, the events log ending on its open marker. A finding, not a regression; nothing is uncommitted.
Resume: `live read mission_loaded --label mission` alone in a fresh mission (ADR 0028's retest); `live read all --label mission` for player_unit_type and mission_theatre; a fresh launch at the menu, `live read all --label menu`; `live report`. Results are in `%LOCALAPPDATA%\dcs-mcp\live.jsonl`. Then the figures into a decision record, and `live` leaves the binary.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **`verify` failed straight after every relaunch**: the executor writes no heartbeat at load, so the last session's file read as two problems; one written before the handshake is now a leftover and no problem, one since still one (ADR 0030). Client-only, no reinstall; four controls under T32 and T46.
- **`verify` failed a session `ping` answered**: DCS gives `lfs.tempdir()` as `%TEMP%\DCS`, under the client's, and `status` read that as a disagreement; now `Within` and no problem, one outside still one (ADR 0029). The round trip always read the handshake's transport; three controls under T32.
- **The first live load refused its handshake** as `executor.txt: nil`: DCS's own `io`/`os` answers a success with nothing, so only nil and a message is a refusal now, and a silence is settled by a stat of the final name (`publish`) or the request (`take`); modelled in `executor/framer`, eleven controls under T08, T09 and T26; `SHIPPED` gains the third hash.

*The last three at most, one line each. Git log holds the rest.*

## Next

**Stage 9**, the live proofs; **only the maintainer can verify any of it**, at a running
install. The instrument is `dcs-mcp live` (README, "Measuring a live install"), proved off DCS only: whether it measures DCS correctly is the first session's to see.
What is left of the run is In progress's resume line; `live report` prints all 47 rows.
Not built, rows printing `unmeasured: not built`: `live scene` (T50's `sim_mode` per scene, editor-vs-menu, `mission_name` at the menu, callbacks), a follow-up branch; and the s17 fixture (T49).
T52 starts from `dcs-mcp install --variant DCS`: the maintainer's `Saved Games` holds `DCS`, `DCS_F4E` and `DCS_OH58D`, and with no `--variant` every installer verb refuses and names all three.

The MCP registration still points at `dcs-api-bridge`; the swap strands a session
mid-task and is the maintainer's call (ADR 0001); `install` prints the snippet.

## After that

- **Milestone A is closed:** Stages 0 to 2 — the wire proven off DCS, the
  stand-in, the interop and round-trip controls.
- **Milestone B is closed:** Stages 3 to 6 — eval across the carriers with
  `<file>:47` true in every state, the budgets and crash safety, the dormant
  frame, and the client library. Every row built and swept; figures in git log.
- **Milestone C is closed:** Stages 7 and 8 — the MCP server and its six tools,
  one wording for every reply, the CLI, the run record, the server's idle and
  the installer under park-and-restore. Fourteen rows, 26 controls all seen red.

## Carries forward

Things that must not be lost between sessions. Delete an entry when it is
resolved, and say where. Mark an entry only the maintainer can settle. Ten
entries at most: an eleventh means something here is finished, or belongs in
`docs/decisions/` or `CLAUDE.md` instead.

- **Cutover — by hand, and nothing checks it.** `DcsApiEval.lua` and its `Export.lua`
  line go before this installs. ADR 0022 removed the refusal, so installing beside it
  now succeeds and both poll. Maintainer's; the README is the only warning.
- **What only Stage 9 sees.** The load sentinel covers two: the hook guard's
  swallow path, which has no seam until a raising stub sits on the frame path, and
  whether the export state survives between missions. The wrapper is proved under
  the suite's own carrier alone — what DCS does with one raising inside
  `net.dostring_in`, and whether every state has `setfenv` and `_G`, is unmeasured,
  the model's stubs evaluating nothing — and the carrier against the incumbent's
  measured shift and `...`, not T49's. Unmeasured too: DCS's `os.clock` and
  `os.time` against the harness's, a count hook raising inside either carrier or
  already held by a state (`none`, ADR 0005), what `dcs.log` renders around a
  crossing's markers, which the reader ignores (ADR 0007). `lfs.tempdir()` is
  measured: `%TEMP%\DCS` on one machine, 2.9.29 (ADR 0029). Which of
  `write`, `close`, `rename`, `remove` answers a success with nothing is unmeasured
  (the empty output dir fits a write, a close, or a rename that did nothing); under that,
  the waking frame's heartbeat pays a stat.
- **Declared before served, and absent before counted.** The handshake
  publishes `ops`, `states`, `eval` and the five figures from the first load,
  the two instruction figures provisional. No `state` is `bad-request`: the maintainer's call, 2026-09-11.
- **A path with a byte past ASCII stops the load.** Header values are ASCII, so a user name past ASCII puts one in every path the handshake names and the load refuses, naming the header in `dcs.log` (`docs/audit.md`, Open: the spec says nothing).
  The client parses it as the executor does, the maintainer's call at T13; a
  spelling for such a path on the wire is the writer's side, unsettled.
- **The held sibling is a held file.** The sweep's "cannot be removed" path is proved
  with a file the suite keeps open. T56's directory handle refuses a removal here
  too; that no server holds one between tool calls is T41's.
- **A `Minter` has no owner.** The window mints from whatever it was handed; who
  holds one across tool calls, so two do not restart at seq 1, is Stage 7's.
  `Minter::seeded_at` resumes a counter.
- **`dcs-mcp live` is temporary.** Once Stage 9's figures are in a decision record,
  it leaves the binary with its tests and `live/` entries: the maintainer's call, 2026-09-21.
- **`install` over a copy of ours, then `uninstall`, leaves one in place.** Same
  release or older: `install` parks ours, `uninstall` restores it (T45's); after an
  upgrade it takes two runs. Maintainer's: restore a park we shipped? A row of its own.
