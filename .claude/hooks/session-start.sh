#!/bin/sh
# SessionStart: put the handoff into context before the first prompt.
#
# docs/STATE.md is the handoff between sessions and CLAUDE.md says to read it
# before anything else. Printing it here makes that automatic. The working
# tree and the toolchain come with it, because a session that starts on a
# dirty tree, on a topic branch, or without the reference interpreter built
# should know at once rather than after its first failing command.
#
# Whatever a SessionStart hook prints to stdout is added to the context.
# A compaction keeps its own summary, so nothing is printed then.

. "$(dirname -- "$0")/payload.sh"

source=$(parse source)
[ "$source" = "compact" ] && exit 0

cd "$(project_root)" || exit 0

if [ -f docs/STATE.md ]; then
    printf '## docs/STATE.md\n\n'
    cat docs/STATE.md
    printf '\n'
fi

printf '## Toolchain\n\n'
if [ -x .lua/bin/lua5.1.exe ]; then
    printf 'lua5.1: %s\n' "$(.lua/bin/lua5.1.exe -v 2>&1 | head -1)"
else
    printf 'lua5.1: NOT BUILT. Run `mise run lua-build` before any Lua task.\n'
fi
printf '\n'

if git rev-parse --git-dir >/dev/null 2>&1; then
    printf '## Working tree\n\n'
    printf 'branch: %s\n' "$(git symbolic-ref --short -q HEAD 2>/dev/null || git rev-parse --short HEAD)"
    status=$(git status --short 2>/dev/null | head -20)
    if [ -n "$status" ]; then
        printf '%s\n' "$status"
    else
        printf 'clean\n'
    fi
fi
exit 0
