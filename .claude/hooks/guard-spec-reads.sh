#!/bin/sh
# PreToolUse guard: refuse an unbounded Read of a frozen specification.
#
# bridge.md is 1,451 lines and mcp.md is 747. Loading one whole costs most of
# a context window and buys nothing tools/spec.sh cannot locate more
# precisely. This hook makes that constraint hold rather than hoping the
# instruction in CLAUDE.md is followed.
#
# A bounded Read still passes: pass offset and limit, with limit at or under
# the cap. Everything else routes through tools/spec.sh, which runs in sh and
# is not affected by this hook.
#
# Exit 2 blocks the tool call and hands stderr to the model as the reason.
# Exit 0 renders no decision and the normal permission flow continues.

. "$(dirname -- "$0")/payload.sh"

CAP=${SPEC_READ_MAX_LINES:-400}

tool=$(parse tool_name)
[ "$tool" = "Read" ] || exit 0

path=$(parse file_path)
[ -n "$path" ] || exit 0

norm=$(normalize_path "$path")

# The frozen specifications only. docs/PLAN.md changes with the build and is
# read whole often enough that guarding it would cost more than it saves.
guarded=no
case "$norm" in
    */docs/specs/*.md|docs/specs/*.md) guarded=yes ;;
esac
[ "$guarded" = yes ] || exit 0

limit=$(parse limit)

if [ -n "$limit" ] && [ "$limit" -le "$CAP" ] 2>/dev/null; then
    exit 0
fi

name=${norm##*/}
code=$(printf '%s' "${name%.md}" | tr '[:lower:]' '[:upper:]')

cat >&2 <<EOF
Blocked: an unbounded Read of $norm.

This document is frozen and too large to load whole, and tools/spec.sh exists
so you do not have to. Locate first, then retrieve:

    sh tools/spec.sh list                    the codes and the paths
    sh tools/spec.sh find $code <text>       every heading and line matching
    sh tools/spec.sh sections $code          the heading tree with line counts
    sh tools/spec.sh read $code "3.7"        one whole section

Start from find, not from sections.

If you truly need the Read tool here, bound it: pass offset and limit with
limit at or under $CAP lines.
EOF
exit 2
