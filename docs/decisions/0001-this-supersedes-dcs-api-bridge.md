# ADR 0001: This project replaces `dcs-api-bridge`, and the two never coexist

## Status

Accepted

## Context

An eval bridge and MCP server already exist and are in daily use: the
`dcs-api-bridge` server, whose hook half is `DcsApiEval.lua` in
`Saved Games\DCS\Scripts\Hooks\` and whose transport root is
`Saved Games\DCS\Logs\DcsApiBridge\`. Both specifications here are written
against it — the figures in `bridge.md` §3 and §12 are measurements *of* it,
and the `prior:` citations throughout both documents point into its tree.

This project is not an evolution of it. `bridge.md` §7.0 removes two of the
four ops outright:

> `reflect` and `census` are removed from the protocol. […] `census` was a
> convenience verb around `eval` of a chunk that reads and writes a global,
> and the ~2,000 lines behind it […] belong to the project that consumes their
> output. `reflect` is the same shape with a 3,308-byte chunk and no session,
> and no reason was found for it to differ.

The executor is also restructured around an idle budget it was never built for
(`bridge.md` §2.4, §3.6–3.8), re-hosted from TypeScript to Rust
(`mcp.md` §1.6), and given a session-stamp fence that closes a kill the
incumbent suffered.

The two cannot run side by side. Both install a hook into
`Saved Games\DCS\Scripts\Hooks\` and both poll a transport root under the same
`Logs\` tree, so both are called from every callback of the same process. Two
hooks polling is not a degraded mode; it is one answering a request the other
is also answering, at twice the idle cost the whole restructuring exists to
remove.

The specifications make this worse than it needs to be by naming the file
`DcsApi.lua`, four characters from the incumbent's `DcsApiEval.lua` and in the
same directory. ADR 0002 renames it.

## Decision

`dcs-eval` supersedes `dcs-api-bridge`. The incumbent is uninstalled before
this executor is installed, and the MCP registration is swapped at Milestone C
rather than earlier, because the swap strands any agent session mid-task.

The tool surface maps as follows. It is a mapping, not a compatibility layer:
no alias is shipped and no old name is answered.

| `dcs-api-bridge` | `dcs-eval` |
|---|---|
| `dcs_bridge_status` | `dcs_status`, widened to carry `verify`'s findings |
| `dcs_ping` | `dcs_ping` |
| `dcs_eval_in` | `dcs_eval`, plus `dcs_eval_file` for a path source |
| `dcs_collect` | `dcs_collect` |
| `dcs_reflect` | removed; a consumer ships the chunk and walks it |
| — | `dcs_game_state`, new |

A plan task proves the two never run at once, because a check that says so is
worth more than an instruction that asks for it.

Rejected: installing alongside the incumbent under a distinct transport root,
which leaves two hooks polling every frame and doubles the idle cost the whole
restructuring exists to remove. Rejected: aliasing the old tool names, which
would keep `dcs_reflect` answerable and re-import the ~2,000 lines §7.0 sends
elsewhere.

## Consequences

There is a cutover, and it is manual: uninstall, install, swap the
registration, restart the agent. `docs/STATE.md` carries it until it is done.

Every measurement the incumbent produced stays valid as a *baseline* and not
as a target — notably the 0.098 ms dormant poll cost, which the restructured
executor must beat rather than match. A figure measured on one machine and one
DCS build is compared against, never inherited.

Consumers of `census` and `reflect` are not served by this project at all.
That is `bridge.md` §7.0's decision and this record only carries it forward;
the consuming census project ships its own chunk against `dcs_eval`.

*Revisit if* the cutover proves the incumbent still does something this does
not. The honest response would be a plan task, not a resurrection of the old
server.
