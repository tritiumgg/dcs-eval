# dcs-eval

Evaluate Lua inside a running DCS World. The **executor** is one Lua file DCS
loads; `dcs-mcp` is one Rust binary that installs it, speaks its file-based
protocol, and exposes six tools. It replaces `dcs-api-bridge`, which is
uninstalled before this is installed (ADR 0001).

**The word is "executor", and the frozen specifications say "bridge".** They
mean the same thing; ADR 0002 says why the build uses its own word. Nothing
outside `docs/specs/` says "bridge" except the filename `bridge.md`.

## Layout

```
CLAUDE.md              this file: durable facts about the project
README.md              what a user downloads, installs, configures and runs
docs/
  STATE.md             the handoff between sessions. Read it first
  PLAN.md              build order, 52 tasks in 10 stages. Not frozen
  audit.md             what the documents disagree about
  specs/               frozen: bridge.md and mcp.md. Never edited
  decisions/           where the build goes somewhere the specs did not
  conventions/         how a decision record is written
crates/dcs-eval        the client library: the executor's protocol, from outside DCS
crates/dcs-mcp         the binary: the MCP server, the installer and the CLI
types/dcs.lua          the DCS-provided globals, declared for the language server
tools/                 mklua.sh, check-lua.sh, check-lua-test.sh, spec.sh,
                       statecheck.sh, nospecrefs.sh, hooktest.sh,
                       buildcheck.sh, harness.lua, harness-test.sh
tools/harness/         the harness suites, registered in suites.lua
.claude/hooks/         the read guard, the frozen-write guard, the shell guard,
                       the commit checks, the session start, the stop check
```

## `docs/STATE.md` is the handoff between sessions

Read it before anything else. It names what is half-done, what to pick up, and
what carries forward.

Update it **before a session ends**, not only when a task finishes. Sessions
stop mid-task and the next one should not have to re-derive where things stood.
Stamp the "Last updated" line each time; it carries a date and nothing else.

At the end of a task: move it to "Just finished", clear "In progress", pull the
next task up, delete any carry-forward it resolved and say where. At the end of
a session that stops mid-task: fill "In progress" with what is done, what is
not, where to resume, what is committed versus only in the working tree, and
what is knowingly broken.

**Keep it small.** It is loaded cold every session. `tools/statecheck.sh`
enforces a budget per section and CI fails when it is over. Over budget nothing
is deleted, it moves: a stale completion to git log, a choice with reasoning to
a decision record, a durable fact to this file. Never write a paragraph where a
line will do, and never copy a fact from here into it.

## The specifications are frozen. Nothing else is.

`docs/specs/` is the starting point, is not maintained, and the build will
drift from it. Do not edit it and do not offer to bring it up to date. A
specification kept current becomes a second copy of the build, and the copy is
always the one that is wrong — while quietly editing it to match what was
built destroys the record of what was intended, which is the only thing it is
still good for.

Where the build goes somewhere the specifications did not anticipate, write a
decision record: copy `docs/decisions/TEMPLATE.md`, number it next, and follow
`docs/conventions/decision-records.md`. A probe answer is a record too, so the
Stage 9 measurements land there. A change with one obvious answer needs no
record, and neither does one whose reasoning already sits in the code, this
file or the plan.

`docs/PLAN.md` is not frozen. It states build order; edit it when the order
changes. Everything under `docs/specs/` is frozen and nothing else is, which is
the whole rule the two guards enforce.

**A task ID never appears in the code.** Tasks are ephemeral; the plan retires
when the build ships and a comment naming one then points at nothing. Neither
does a specification citation: a section number sends the reader to a document
the build is drifting away from, to find a reason that would have fitted in the
comment. Say why the code is the way it is in its own words, and where the
argument is too long, cite the decision record holding it.
`tools/nospecrefs.sh` enforces both; `docs/`, `CLAUDE.md` and `README.md` are
exempt, because documents cite documents.

## Never read a specification whole

`bridge.md` is 1,451 lines and `mcp.md` is 747. Loading one costs most of a
context window and buys nothing `tools/spec.sh` cannot locate more precisely.
A `PreToolUse` hook refuses an unbounded `Read` of one, and another refuses an
edit.

```sh
sh tools/spec.sh list                  # the codes and the paths
sh tools/spec.sh sections BRIDGE       # the heading tree with line counts
sh tools/spec.sh find BRIDGE dormant   # every heading and line matching
sh tools/spec.sh read BRIDGE 3.7       # one whole section
```

Start from `find`, not from `sections`.

## Two rules that override anything else

**Quote the document, not your memory of it.** A decision record's `Context`
carries the specification's own prose, retrieved with `tools/spec.sh read`.

**Say who verifies.** Most done-conditions in the plan need a running DCS
install or a person watching. Before starting a task, decide whether an agent
can observe the result itself, whether it needs a maintainer reading a CI
result, or whether only somebody at a live install can see it. Write it in
`docs/STATE.md` under the task. An agent that skips this declares victory on
something it never observed.

## Toolchain

**Windows is the only supported host** — not merely the only target. Every
done-condition past Milestone B needs a running DCS World, which ships for
Windows only, so a contributor on another platform could not finish a task.
There is no CI matrix and no cross-compilation.

Tool versions come from `mise.toml`. Do not install toolchains globally and do
not use a language's own version manager directly.

Run every project command through mise, because a non-interactive shell does
not pick up mise's PATH activation:

```sh
mise exec -- cargo test
mise run check
```

On a fresh checkout: `mise install`, then `mise run lua-build` once. The
reference interpreter is built from the official Lua 5.1.5 tarball with MSVC,
because mise's lua plugin builds it with `make`, which Windows does not have.
If a command reports the config is untrusted, run `mise trust`.

**Rust is the exception, and `mise use rust@<version>` breaks it.** The version
lives in `rust-toolchain.toml` with its components and target, and mise reads
that file. `mise use` writes a second version into `[tools]` and mise stops
reading it. Edit `channel`, then `mise install`.

**Lua is 5.1.5 PUC-Rio, never LuaJIT, never 5.4.** 5.4 has no `setfenv`, its
integer division changes what `%.14g` prints, and LuaJIT counts debug hooks
differently. A green run under anything else says nothing.
`tools/check-lua.sh` proves what is on PATH and every Lua task depends on
it; `tools/check-lua-test.sh` drives it with fake banners and is what says
the refusal still works.

## Language servers

Both are pinned in `mise.toml` and enabled in `.claude/settings.json`. mise's
`ls-shims` directory is on `PATH`, which is how the editor and the agent find
them by bare name; a version left unpinned resolves to whatever is installed
globally, and two machines then analyse the same tree differently.

`types/dcs.lua` declares what DCS provides — `os.getpid`, `lfs.writedir`,
`net.dostring_in`, the `DCS.*` reads — as a `---@meta` definition file rather
than as names in a `diagnostics.globals` list. The difference matters: a
globals list silences `DCS.getPuase()` along with `DCS.getPause()`.
`mise run lua-lint` is that check, and `mise run check` includes it.

Add to `types/dcs.lua` only what the build actually uses. It is not a catalogue
of the DCS API; cataloguing that belongs to the consuming project.

## Portability

Shell scripts are POSIX `sh` run under Git Bash: no bash arrays, no `[[`, no
`local`, no `sed -i`, no `grep -P`, no `readlink -f`. They need not be portable
beyond Windows, so `sha256sum`, `cygpath` and `cmd.exe` are fair
game.

Leave `.gitattributes` alone. It disables line-ending conversion, without which
the embedded `DcsEvalExecutor.lua`'s hash, the installer's hash gate and the
interop control all break against a file nobody edited.

## Version control

Trunk-based: `main` is always releasable, branches live hours rather than days,
and history is linear.

**A branch per plan task**, named `task/<id>-<summary>`, such as
`task/T01-reference-interpreter`. Work belonging to no task takes the `type` it
would commit under: `fix/`, `docs/`, `build/`.

**One pull request per task, reviewed commit by commit.** A large task is a
long series of small commits on one branch. Commit each slice as soon as its
test passes, so there is always a working state to return to; each commit does
one thing and leaves the tree passing `mise run check`. About 100 changed lines
is easy to review, 300 is fine for one logical change, 1000 is split before
committing, with tests counted as code. Formatting and behaviour, a refactor
and a feature, and an unrelated fix found along the way each take a commit of
their own. Stage paths by name, never `git add -A` or `git commit -a`, and read
the staged diff before writing the message. Conventional Commits:
`type(scope): summary`, imperative, under 72 characters.

**Review locally, run `mise run check`, then push.** A fix found after the push
costs a CI run. Every pull request body follows
`.github/PULL_REQUEST_TEMPLATE.md`; `Summary` and `README` are always present
and the shell guard refuses a body without them.

**History is linear. Rebase, never merge-commit.** A branch lands with
`git merge --ff-only`; a refused fast-forward means the branch is fixed.

**Work reaches `main` through a pull request, and lands when the maintainer
says so.** Open it, report it as waiting, and stop there. When the maintainer
asks for a pull request to be merged, land it: fast-forward `main`, push once,
delete the branch. Its CI run is green first; a red run is reported, not
landed. The ask covers the pull request it names and no later one.

Do these without asking: branch, commit, rebase onto `main`, push a topic
branch, force-with-lease a topic branch that is yours, delete a merged branch,
open a pull request with `gh`. Do these when the maintainer asks: fast-forward
`main` to a branch and push it. **Ask first** before rewriting pushed history
and before tagging, because `release.yml` fires on `v*` and a tag is a release.

**Attribution is off, and the history carries none.** `attribution.commit`,
`.pr` and `.sessionUrl` are `false` in `.claude/settings.json`, and the six
commits that once carried a `Co-Authored-By` trailer were rewritten to drop
it. The setting stops the CLI adding a trailer by itself; it does not stop a
session being told to add one, which every session is, at start and again
mid-session. Do not: a commit or a pull request body with one is wrong here.

The hooks in `.claude/hooks/` hold the rest of this section.

## `README.md` is for users, and the change that moves it updates it

The README says what a user downloads, installs, configures and runs. A task
that changes any of that updates the README in the same pull request. Where the
build has not reached something the README describes, the sentence says so on
one line with "not built" or "planned"; the task that settles one takes the
note out.
