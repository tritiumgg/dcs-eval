#!/bin/sh
# PreToolUse guard: refuse an edit to a frozen document or to .gitattributes.
#
# The specifications are frozen. They are the starting point, they are not
# maintained, and the build drifts from them by design. Where the build goes
# somewhere they did not anticipate, the record of that is a decision record,
# not an edit. Everything under docs/specs/ is frozen, and nothing else is.
#
# .gitattributes disables line-ending conversion. Without it the embedded
# DcsEvalExecutor.lua's hash and the installer's hash gate both break against
# a file nobody edited.
#
# Exit 2 blocks the tool call and hands stderr to the model as the reason.

. "$(dirname -- "$0")/payload.sh"

path=$(parse file_path)
[ -n "$path" ] || exit 0

norm=$(normalize_path "$path")

why=""
case "$norm" in
    */docs/specs/*|docs/specs/*)
        why="a frozen specification" ;;
    */.gitattributes|.gitattributes)
        why=".gitattributes, which keeps every hash this project ships honest" ;;
esac

[ -n "$why" ] || exit 0

cat >&2 <<EOF
Blocked: a write to $norm, which is $why.

The specifications are frozen and are not brought up to date. Where the build
needs to go somewhere they did not anticipate, write a decision record: copy
docs/decisions/TEMPLATE.md and number it next.
docs/conventions/decision-records.md says how.

Leave .gitattributes alone for the same reason: this project hashes the files
it ships, and line-ending conversion rewrites those bytes on checkout.
EOF
exit 2
