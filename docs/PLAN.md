# Implementation plan: the screenshot capability

This plan builds what `docs/specs/screenshot.md` specifies: a seventh tool, `dcs_screenshot`, and a
fifth command-line verb, `screenshot`. It supersedes the plan that built the executor, the client
and the server, which finished on 2026-09-22 and is retired at `docs/PLAN-SHIPPED.md`. That
document is still read — by anyone asking what was built, and by `tools/sweep-cover.sh`, which
files every control in `docs/mutations.md` under the plan row it came from.

Task IDs carry on from it rather than starting again. `T64` was the last it used, so this one opens
at `T65`: an ID is never reused, for the same reason a decision record's number never is, and
`tools/sweep-cover.sh` refuses an ID two plans both carry — a reused one would file this plan's
controls under that plan's entries and read as covered.

**Stage numbers start at zero here.** Each plan numbers its own stages, and the window
`tools/sweep-cover.sh` asks presence in is about built rows against live ones, not about how far
any one document got.

---

## Granularity

Seven tasks in two stages. The splits fall where a done-condition changes hands: naming and
completeness are decided off DCS against fixture files, the capture is the two-phase wait around
one published request, and the tool and the verb are two surfaces over one function. The live
stage is last because it is wall-clock-bound and cannot be parallelised by adding effort — the
rule the retired plan set and this one keeps.

No task's done-condition names more than one command. Where the specification names a control, the
row that builds it names the mutation that must redden it, and writes its entry into
`docs/mutations.md` in the same pull request.

---

## Definition of done

- **A task is done** when its one command prints what the row says. A row that builds a control is
  done when that control goes red under the mutation named beside it: a check nobody has watched
  fail is not evidence.
- **A stage is done** when every task in it is done and the stage command re-runs them together.
- **A milestone is done** when its acceptance, stated as a command or as a record, passes.
- **A live row is done** when its answer is in a decision record. There is no other way to finish
  one: only somebody at a running DCS can see it.

**A row that lands is marked `**Done**` in its task cell**, in the pull request that lands it.
That mark is what `tools/sweep-cover.sh` reads to decide the row owes its entry in
`docs/mutations.md`: until it is there the row is a plan, and after it the row is code somebody
can break.

---

## Standing rules

1. **The specification is frozen.** `docs/specs/screenshot.md` is what this builds. Where the build
   goes somewhere it did not anticipate, that is a decision record, not an edit.
2. **Nothing on the wire changes.** The executor keeps its two ops, the protocol its headers, the
   installer its four dispositions. A change to any of them means the specification was misread.
3. **The only file this project writes is the copy `--out` asks for.** DCS writes the capture;
   nothing here deletes, renames or tidies anything under `Saved Games`.
4. **Every row is marked `developer-only` or `DCS + human`** and sequenced on it.
5. **The README moves with the change.** The task that adds a surface a user runs updates the
   README in the same pull request, and takes out any "planned" note it settles.

---

## Milestones

- **Milestone E — The capability built.** Someone can ask for a capture against a fixture
  directory and get back a path, a format, a size and the picture's dimensions, with every refusal
  in the specification's words and each of its controls seen red. *Acceptance:* Stage 0's command
  green; an in-memory MCP session lists seven tools and calls `dcs_screenshot`; `mise run check`
  green. Covers Stage 0.
- **Milestone F — Proven live in DCS.** Someone at a running install asks for a capture and looks
  at the picture, in a mission, at the menu and in the editor, and the questions
  `docs/specs/screenshot.md` §4 leaves open are answered or written down as still open.
  *Acceptance:* both Stage 1 rows hold a decision record. Covers Stage 1.

---

## Stage 0 — Built off DCS   *(Milestone E)*

Every row here is developer-only and runs against fixture files: a directory standing in for
`ScreenShots`, images that are whole and images that are cut short. The capture itself is a
published request like any other, so the client's existing test transport drives it with no DCS in
sight.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T65 | **Done.** The name: a caller's validated (`A-Z a-z 0-9 _ -`, one to sixty-four characters), and the tool's own `dcs-eval-YYYYMMDD-HHMMSS-mmm` where none is given. A refusal names the character that broke the rule, and no name is ever rewritten | `mise exec -- cargo test -p dcs-eval shot_name` shows a dot, a backslash, a forward slash, an empty name and a sixty-fifth character each refused with the character named, and two names taken in immediate succession differing; mutations: a dot accepted reddens the name refusal; a supplied name cut to the second reddens the distinctness check | — | developer-only |
| T66 | **Done.** The finished file: the format found among `.png`, `.jpg`, `.bmp` and the bare name an abandoned capture leaves, wholeness by the format's own end (PNG `IEND`, JPEG `FF D9`, BMP's header size against the file), a zero-byte file answered `empty`, and width and height from the header — for JPEG by walking the markers, so a thumbnail in the metadata is not read as the picture | `mise exec -- cargo test -p dcs-eval shot_file` reads a whole fixture of each format with its true dimensions, refuses each cut short of its end, and answers a zero-byte file `empty`; mutations: wholeness judged by a size that stopped growing reddens the truncated-PNG check; a zero-byte file waited out reddens the `empty` check; the first frame header taken for the picture reddens the thumbnail check | — | developer-only |
| T67 | The capture: the chunk `DCS.makeScreenShot(<name>) return lfs.writedir()` published for the `hook` state, the export host refused as `unsupported`, the newness test against a clock reading taken before publication, and one `wait_seconds` spanning both phases — `pending` when no reply comes, `not-written` when the reply came and the file did not | `mise exec -- cargo test -p dcs-eval screenshot` shows a capture answered `ok` from a file written late in the wait, a complete file older than the request answered `not-written` rather than returned, a reply that never comes answered `pending` with an id, and the export host refused; mutations: the newness test dropped reddens the stale-file check; the wait ending at the reply reddens the late-file check; `pending` and `not-written` swapped reddens the two-phase check; the export host served reddens the host refusal | T65,T66 | developer-only |
| T68 | `dcs_screenshot` registered, listed and answered over a real MCP session, in the wording every other tool answers in: `ok` with `path`, `format`, `bytes`, `width` and `height`, and each refusal headed by the word that refused it while `pending` and `not-written` are marked no such thing. The README says seven tools | `mise exec -- cargo test -p dcs-mcp tools_listed` lists exactly the seven and calls each, and a `not-written` comes back unmarked; mutations: a tool that registers but is not listed reddens the count; `not-written` marked an error reddens the not-an-error check | T67 | developer-only |
| T69 | The CLI verb: `dcs-mcp screenshot [--name N] [--wait-seconds S] [--out PATH]`, `--out` copying the finished file byte for byte and writing nothing for any answer that is not `ok`, `--capture` not offered, and the binary's exit codes — 0 for an answer including `pending` and `not-written`, 1 for a refusal or a failed copy, 2 for a line that will not parse. The README documents the verb | `mise exec -- cargo test -p dcs-mcp screenshot_cli` shows the three flags parsed and each refused twice over, `--capture` refused by name, an `--out` copy identical to the file it came from, nothing written at `PATH` for a `not-written`, and the three exit codes; mutations: `--out` written from the reply rather than the file reddens the copy check; a copy written for a `not-written` reddens the no-write check; `not-written` exiting 1 reddens the exit-code check | T68 | developer-only |

**Stage command:** `mise exec -- cargo test -p dcs-eval -- shot_name shot_file screenshot` then
`mise exec -- cargo test -p dcs-mcp -- tools_listed screenshot_cli`. Cargo takes one test-name
filter and the rest after `--`, and a filter matching nothing exits 0 — so each row's own
done-condition, not the stage command's exit, is what says it ran.

---

## Stage 1 — Proven live in DCS   *(Milestone F)*

Both rows need DCS running and a person to set the scene and look at the picture. Neither builds a
control, so neither names a mutation: what they produce is a decision record, and the controls
they exercise were all built in Stage 0.

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| T70 | The first live capture: in a mission, at the main menu and in the mission editor, each looked at by a person. The format and resolution the answer reports against the file on disk, and the spread between asking and the file being whole over ten captures | a decision record carrying the three pictures' filenames, the reported format and dimensions, and the timing spread, with the wait that spread argues for; `dcs-mcp screenshot --name <n>` answers `ok` in each of the three, and the picture is of what was on screen | Milestone E | DCS + human |
| T71 | The questions `docs/specs/screenshot.md` §4 leaves open, each answered or written down as still open: a name already on disk, a separator in a name, a capture while paused, a capture on a dedicated server, and what a VR session captures | a decision record with a line per question — the answer and how it was seen, or that it was not reached and why. The README says what a user should expect of a name already taken | T70 | DCS + human |

**Stage command:** none. Both rows are records.

---

## Out of plan

- **Cropping, and returning the picture inside the answer.** Argued out in
  `docs/specs/screenshot.md` §3. Either becomes a plan row the day a caller cannot do its work
  with a path, and not before.
- **Aiming the camera, pausing, hiding the interface.** All reachable through `eval`, and all the
  caller's to do in their own call.
- **A gallery.** No retention, no cleanup, no listing of what is in `ScreenShots`.
