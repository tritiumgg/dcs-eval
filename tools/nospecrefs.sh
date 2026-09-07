#!/bin/sh
# Keep specification citations and plan task IDs out of the code.
#
# A comment reading `bridge.md §5.2` sends the reader to a frozen document the
# build is drifting away from, to find a reason that would have fitted in the
# comment. A comment reading `task T21` points at nothing once the plan
# retires, which it does when the build ships. Both are ephemeral references
# standing in for an argument the code should make in its own words; where the
# argument is too long for a comment, it cites the decision record holding it.
#
# Everything under docs/, plus CLAUDE.md and README.md, is exempt: documents
# cite documents, and a dated record may name a task because it says what was
# true on its date.

set -e

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

CITE='(bridge|mcp)\.md[[:space:]]*(§|#)|SPEC[[:space:]]*§|§[0-9]+\.[0-9]'
TASK='\b[Tt]ask[[:space:]]+T[0-9]{2}\b|\bT[0-9]{2}\b|done-when'

files=$(git ls-files 2>/dev/null || find . -type f -not -path './.git/*')

fail=0
for f in $files; do
    case "$f" in
        docs/*|CLAUDE.md|README.md|tools/nospecrefs.sh|.claude/hooks/*) continue ;;
    esac
    [ -f "$f" ] || continue
    hits=$(grep -nE "$CITE" "$f" 2>/dev/null || true)
    if [ -n "$hits" ]; then
        printf '%s cites a specification:\n%s\n' "$f" "$hits" >&2
        fail=1
    fi
    hits=$(grep -nE "$TASK" "$f" 2>/dev/null || true)
    if [ -n "$hits" ]; then
        printf '%s names a plan task:\n%s\n' "$f" "$hits" >&2
        fail=1
    fi
done

if [ "$fail" -ne 0 ]; then
    cat >&2 <<'EOF'

Say why the code is the way it is, in the comment's own words. Where the
argument is too long for one, cite the decision record that holds it.
Task IDs belong in docs/STATE.md, docs/PLAN.md and docs/audit.md.
EOF
    exit 1
fi
echo "no specification citations or task IDs in the code"
