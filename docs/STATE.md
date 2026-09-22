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

Nothing. The Stage 9 live run finished on 2026-09-21; its figures and the maintainer's calls on them are ADR 0031.

*One task at most. Say what is done, what is not, and where to resume. Say what
is committed and what is only in the working tree. Say what is knowingly
broken. Empty this when the task closes.*

## Just finished

- **The client refuses a transport it must not write into**: a handshake naming one inside the install its own `install_guard` names (ADR 0034), or in the variant's Saved Games outside `Logs`, refuses every publishing call and is a `status`/`verify` problem; relative was already refused. Six controls under T31, T32, T37, T46; the live `%TEMP%` and `Logs` shapes pass on fixtures.
- **One `uninstall` removes the executor (ADR 0033)**: `install` replaces a release of ours in place and parks only a stranger's file; `uninstall` never restores a park of ours, an older binary's included, and leaves it in the store. Closes the install-twice carry-forward; two controls under T44 and T45, proved on fixtures; one run clearing the maintainer's own twice-installed `DCS` is theirs to see.
- **`dcs-mcp live` left the binary**, the maintainer's call once ADR 0031 held its figures: its module, the dormant probe suite, `reads::alone` and its eight controls, closing that carry-forward. PLAN's T47 to T50 now point at ADR 0031.

*The last three at most, one line each. Git log holds the rest.*

## Next

T52 and T51, **the maintainer's at a running install**. T52 starts from `dcs-mcp install --variant DCS`: the maintainer's `Saved Games` holds `DCS`, `DCS_F4E` and `DCS_OH58D`, and with no `--variant` every installer verb refuses and names all three.

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
  measured shift and `...`, not T49's. Unmeasured too: DCS's `os.time` against
  the harness's (its `os.clock` steps in whole ms, ADR 0031), a count hook raising
  inside either carrier or already held by a state (`none`, ADR 0005), what
  `dcs.log` renders around a crossing's markers, which the reader ignores (ADR
  0007). Which of
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
