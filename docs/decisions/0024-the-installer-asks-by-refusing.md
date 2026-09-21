# ADR 0024: The installer asks by refusing, and takes no --yes

## Status

Accepted

## Context

`mcp.md` §5.1 has the installer ask two questions. Step 1:

> 1. **Find `Saved Games`** through the shell's known folder (`FOLDERID_SavedGames`), never a string
>    built from `%USERPROFILE%` — the folder can be relocated — and `--saved-games` overrides it.
>    List the `DCS*` variants there. One is the target; two is an ambiguity, asked (or `--variant`),
>    never picked (`prior:pipeline/src/bridge/paths.ts:307-334`).

And after the table of what it touches:

> **What it asks.** Which variant, when two exist. Whether to park a file it does not recognise
> (`--replace` answers yes, `--yes` answers every question yes). Nothing else: no wizard, no options
> page, no account.

Step 6 says what it prints when it is done:

> 6. **Say what happens next.** DCS loads `Scripts\Hooks\` at launch and `Export.lua` at mission
>    start, so the bridge appears after the next DCS start; print the MCP client registration
>    snippet for `dcs-mcp serve`; and print `dcs-mcp verify` as the step that confirms it after DCS
>    has run once.

§2.1 gives the three verbs one line of flags:

> ```
> dcs-mcp install | verify | uninstall   [--saved-games <dir>] [--variant <name>] [--replace] [--yes]
> ```

What stood before the verbs were built: `locate::SavedGames::target`
already refuses an ambiguity with `LocateError::Ambiguous`, and its module
says it "asks, prompts, picks or writes" nothing, leaving the asking to the
verb. `install::place_hook` already refuses a file at the hook's name whose
hash this project never shipped, unless it is handed `replace`. Both left
"asked" to a verb that did not exist yet. The maintainer's own machine holds
three variants, `DCS`, `DCS_F4E` and `DCS_OH58D`, so the first question is
the ordinary case there and not an edge.

## Decision

The installer never prompts. Every question is asked by refusing: exit 1,
with the refusal naming what it found and the flag that answers it —
`--variant <name>` for which variant, `--replace` for whether to move a file
this project did not ship. Nothing is written before a refusal, the data
directory included. This holds whether or not stdin is a console.

`--yes` is not taken. It is refused by name, as a line that will not parse,
and the refusal points at `--replace`. `--replace` is taken by `install`
alone; `verify` and `uninstall` refuse it by name.

The flags depart from §2.1's line in the other direction too, and this record
is where that is said. `install` and `uninstall` take `--data-dir`, the
spelling `serve` and the read-and-eval verbs already use, because the
register lives there and a fixture needs one of its own. `verify` takes
`--host`, as `dcs_status` takes `host`, to say which of the executor's two
sessions is reported. `verify` takes no `--data-dir`, because it reads no
register. There is no `--install`: the DCS install is not known to the
installer, so step 2's refusal "when the install is known" never fires, and
the `Saved Games` containment rule is the one that does.

Rejected:

- *A prompt when stdin is a console, and a refusal otherwise.* Two
  behaviours chosen by how the binary was started; the prompting half could
  be seen only by a person at a console, never by an agent or a CI run; and a
  prompt reading a stdin a client left open hangs.
- *`--yes` as a synonym of `--replace`.* One answer with two spellings, and
  "every question" would answer any question added later without anyone
  having decided that it should.
- *Picking the plain `DCS` folder, or the only variant with anything in it.*
  §5.1 says never picked, and on a machine with three variants it is a guess
  into the one directory the installer then writes to.

## Consequences

- A user with several variants runs the command twice. The refusal is the
  documentation: it names every variant and the flag.
- Scripted and agent use is deterministic. The same line always does the
  same thing, however it was launched.
- A double-clicked `dcs-mcp.exe` opens a console that closes as soon as the
  refusal is printed. The README says to run it from a terminal.
- A question added later gets a flag of its own. There is still no `--yes`.
- A caller that does know the DCS install gets `LocateError::InsideInstall`
  from the library; the verbs pass none.
- *Reopen if* a graphical or double-click install is wanted. That needs a
  front end which asks, built over these same refusals rather than beside
  them.
