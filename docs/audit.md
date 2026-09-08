# Audit: what the documents disagree about

The two specifications were written separately and frozen; `CLAUDE.md` says why they are never brought up to date. Where
they contradict each other, or contradict the plan, the build has to pick one
— and picking silently is how a contradiction becomes a bug nobody can trace
back to a document.

This file records every disagreement found, the resolution, and where the
resolution lives. A row without a resolution is an open question and belongs
in `docs/STATE.md`'s carry-forward as well, so a session cannot miss it.

**This is a partial pass.** The rows below are what setup found while reading
the documents for structure; a full reading of both against each other has not
been done. Add rows as the build reaches them — a disagreement discovered
mid-task is recorded here in the same commit that resolves it.

## Resolved

| Subject | The disagreement | Resolution |
|---|---|---|
| Where the executor script comes from | `mcp.md` §0 and §5.5 describe `dcs-mcp` as a standalone project embedding a *released* artefact of a separate executor project. Both halves are in one repository. | The plan's DR-1. One repository; the build embeds its own `DcsEvalExecutor.lua` with its SHA-256, and that hash is "the executor release the binary carries". The hash list §5.5 wants is kept. |
| Which platforms are supported | Both documents are Windows-specific throughout; `bridge.md` describes a Lua half that is OS-neutral, and the plan's DR-2 settles only the *target*. | `CLAUDE.md`, Toolchain. Windows is the host too. The Lua half stays OS-neutral and is simply not tested elsewhere. |
| The prior implementation's tool surface | Both documents cite `prior:` paths in `dcs-api-bridge` as the reference, but `bridge.md` §7.0 removes two of its four ops. | ADR 0001. `dcs-eval` supersedes it, the two never coexist, and the tool mapping is stated. No alias is shipped. |
| The reference interpreter | The plan's first standing rule pins Lua 5.1.5 PUC-Rio; `mise`, the project's dependency manager, cannot install it on the one supported host. | `tools/mklua.sh` and `mise.toml`, in their own comments. Built from the pinned tarball with MSVC, and `tools/check-lua.sh` proves what is on `PATH`. |
| What the in-game half is called | Both documents call it "the bridge", a word that also names the prior server (`dcs-api-bridge`) and the generic category of anything joining DCS to something outside it. | ADR 0002. The build calls it **the executor**; "bridge" survives only inside `docs/specs/` and in the filename `bridge.md`. A verbatim quotation keeps the original word. |
| What it is called on disk | The specifications name the hook `DcsApi.lua` (§6.2), the transport root `<temp>/dcs-api/<host>/` (§4.2) and the output leaf `Logs/DcsApi/<host>/` (§6.2) — all three inherited from the project this one replaces. | ADR 0002. `DcsEvalExecutor.lua`, `<temp>/dcs-eval/<host>/`, `Logs/DcsEval/<host>/`. The first is a safety fix: `DcsApi.lua` sits four characters from the incumbent's `DcsApiEval.lua` in a shared directory. |
| How many hook callbacks | `bridge.md` §6.2 registers "sixteen guarded callbacks, `try*` variants never" and cites the prior `DcsApiEval.lua:2150-2181` for them; those lines register eighteen. | `executor/DcsEvalExecutor.lua`, in its own comment. The eighteen the cited lines register, `try*` still absent, and `tools/harness/executor/load.lua` pins the count. |

## Open

| Subject | The question | Where it is settled |
|---|---|---|
| — | Nothing recorded yet. | — |

## What each document says it could not determine

Neither of these is a disagreement; both are blanks the authors left on
purpose, and the plan folds them into Stage 9 rather than guessing.

- `bridge.md` §11 — what could not be determined from the tree.
- `mcp.md` §8 — what could not be determined.

`tools/spec.sh read BRIDGE 11` and `tools/spec.sh read MCP 8` retrieve them.
A Stage 9 measurement that answers one is a decision record
(`docs/conventions/decision-records.md`: a probe answer is a record).
