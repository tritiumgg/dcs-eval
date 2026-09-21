# ADR 0032: `verify` prints a summary, and `status` the full report, from one renderer

## Status

Accepted

## Context

`mcp.md` §5.3 says what `verify` reads, retrieved with
`sh tools/spec.sh read MCP 5.3`:

> Reads, and writes nothing: the hook file's hash against the embedded
> release's; the `Export.lua` line present exactly once; (…) its
> `app_version` against the build the embedded script was last measured on
> (reported as a difference, never a refusal, `bridge.md` §6.3), its PID; the
> heartbeat; and `<Saved Games>\<variant>\Config\autoexec.cfg`, from which it
> reports the two policy-gate keys `net.allow_unsafe_api` and
> `net.allow_dostring_in` if present.

and §2.3 what `dcs_status` answers, retrieved with `sh tools/spec.sh read MCP 2.3`:

> what is readable without asking the bridge anything: installed, session
> stamp and PID alive, phase, `armed` and `since`, heartbeat age (qualified as
> meaningless while `armed: no`, `bridge.md` §4.7), transport, `app_version`
> against the bridge's build, every problem found, and `verify`'s findings
> (§5.3)

Neither says how the answer is laid out. The build printed one report for
both: the release line, the variant, a `key: value` line per fact with full
paths and hashes, a `problem:` line per finding, and the verdict last.
`status` and `dcs_status` then added the session as a Rust `{:#?}` dump. The
maintainer's first live runs read it on 2026-09-21: the verdict was the last
of 11 to 14 lines, and the dump doubled every backslash and spelled
`Real(...)` and `Some(...)`.

A person runs `verify` to learn whether it works and what to do if not. An
agent reads `dcs_status` and needs every fact, in wording that stays put.

## Decision

One renderer, `verify::render`, at two levels of detail. Both levels open
with the same verdict line and the same summary rows, and the full level only
adds lines after them.

- **Verdict first.** Line 1 starts `verified` or `not verified` and names the
  variant folder. The exit codes do not change: `verify` exits 1 while DCS has
  not yet loaded the executor, as before.
- **Summary.** One row per part, in a fixed order: hook, Export.lua, DCS,
  DCS version, autoexec.cfg. Each row carries a plain status word, `ok`,
  `PROBLEM`, `waiting` or `note`, and a sentence, plus a `fix:` line naming
  the command where one is known. Paths are relative to the variant folder.
  The summary shows no hash, stamp, transport or Debug syntax. A session that
  has not loaded yet, or whose DCS has exited, is `waiting`, not `PROBLEM`.
  `autoexec.cfg` gets one `note` row per key, or one row saying the file is
  not there.
- **Full.** The summary, then `problems (exact)`, holding every problem
  sentence exactly as before, then `details`, a `key  value` line for every
  fact the Debug dump carried.
- `verify` prints the summary, and `verify --verbose` the full report.
  `status` and `dcs_status` print `status` and then the full report. The CLI
  verb and the tool stay byte-identical.

No colour, and no terminal-width wrapping. The release line keeps its `·`,
under `details`.

Rejected:

- **Two renderers, one for people and one for agents.** They would drift, and
  "one wording" is the rule `wording.rs` exists for.
- **One readable level only.** It either hides facts an agent needs or keeps
  the noise a person does not.
- **`dcs_status` prints the summary.** An agent loses the exact sentences and
  every fact the summary leaves out.
- **Colour.** It needs another declared Win32 call for legacy conhost
  (ADR 0019), and the status words carry the signal already.

## Consequences

An agent's text from `dcs_status` changes shape. The verdict moves from the
last line to the second, and the Debug dump becomes the `details` block. No
fact is lost: a test destructures every field of the session and of its
heartbeat and finds each in `details`, so a new field fails to compile until
it is placed.

The plain wording is a second vocabulary beside the exact sentences, kept in
one `match` in `crates/dcs-mcp/src/verify/render.rs`. A new problem variant
has to be worded there too, and the compiler insists. A `fix:` line names
`dcs-mcp install --variant <name>` without `--saved-games`, so it is wrong for
someone whose `Saved Games` is not the known folder. `install` prints the
full line.

Whether the text reads well in conhost and a PowerShell 5.1 pipe is for
someone at the live install to see. What would reopen this: an agent that
parsed the old `key: value` lines, or a request for colour.
