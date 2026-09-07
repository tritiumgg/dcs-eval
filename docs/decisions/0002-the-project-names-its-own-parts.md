# ADR 0002: The project names its own parts, and stops inheriting `dcs-api`'s

## Status

Accepted

## Context

The specifications name the in-game half "the bridge" throughout, and give it
three on-disk names, all inherited from the prior implementation this project
replaces (ADR 0001):

- the installed hook, `docs/specs/bridge.md` §6.2:

  > **One source file, `DcsApi.lua`, installed once at
  > `Saved Games\DCS\Scripts\Hooks\DcsApi.lua`.**

- the transport root, §4.2: `<lfs.tempdir()>/dcs-api/<host>/`
- the output leaf, §6.2: `Logs/DcsApi/<host>/`

Every one of those is a problem, and the first is a safety problem rather than
a matter of taste.

**"Bridge" is not one thing here.** The prior server this project replaces is
`dcs-api-bridge`; the word is the generic term for anything joining DCS to
something outside it, and more than one such thing exists; and these documents
use the bare word for one Lua file. A word that needs its context supplied
before it resolves is not a name.

**`DcsApi.lua` sits four characters from `DcsApiEval.lua`, in the same
directory.** That directory is `Saved Games\DCS\Scripts\Hooks\`, a namespace
shared with every other mod the user has installed. ADR 0001 establishes that
the incumbent and this executor must never both be loaded; naming them almost
identically, in one directory, makes the failure ADR 0001 forbids easy to
cause and hard to see. A person listing that folder should be able to tell at
a glance which file belongs to which project.

**`dcs-api` is another project's name.** This repository is `dcs-eval`. A
transport root and a log directory carrying the wrong project's name mislead
anyone reading a file listing while debugging, which is exactly when they are
read.

## Decision

The in-game half is **the executor**. The word "bridge" appears nowhere
outside `docs/specs/`, where it cannot be changed, and the filename
`bridge.md`.

The three on-disk names take this repository's own:

| | specification | build |
|---|---|---|
| installed hook | `Scripts\Hooks\DcsApi.lua` | `Scripts\Hooks\DcsEvalExecutor.lua` |
| transport root | `<lfs.tempdir()>\dcs-api\<host>\` | `<lfs.tempdir()>\dcs-eval\<host>\` |
| output leaf | `Logs\DcsApi\<host>\` | `Logs\DcsEval\<host>\` |

The `dofile` line the installer appends to `Export.lua` changes with the hook
it names.

Nothing else moves. These are names, not protocol: the wire, the headers, the
statuses, the seven states and the six tools are untouched, and a reader who
knows the specifications can substitute one word and read the build.

Rejected: keeping "bridge" and qualifying it in prose ("the eval bridge", "our
bridge"), which pays the disambiguation cost on every sentence forever.
Rejected: `evaluator`, which is the more literal word but sits beside the
`dcs-eval` repository, the `dcs-eval` crate and the `dcs_eval` tool, trading
one collision for a quieter one. Rejected: `DcsEval.lua` for the hook, which
is again a near-neighbour of `DcsApiEval.lua` in the shared directory the
first argument above is about.

## Consequences

**The frozen specifications and the build now use two words for one thing.**
That gap is permanent and is the price of this decision. `CLAUDE.md` states
the mapping at the top so a reader crossing between them is never guessing,
`docs/audit.md` records it as a resolved disagreement, and `tools/spec.sh`
still answers to the code `BRIDGE` because that is the document's filename.

A verbatim quotation of a specification keeps the word "bridge", because a
quotation that has been edited is not one. ADR 0001 shows both: its
blockquote says "bridge", its own prose says "executor".

The plan was renamed with everything else, so `docs/PLAN.md`'s task rows, its
harness suite names (`executor/load`, `executor/containment`, …) and its DR-1
and DR-2 now read "executor". The specification sections they cite still say
"bridge".

*Revisit if* a second consumer ever needs the executor released on its own
cadence — the plan's DR-1 carries that revisit condition — since a separately
released artefact would want a name settled with its consumers rather than
chosen here.
