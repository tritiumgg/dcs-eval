#!/bin/sh
# Hold docs/STATE.md to its shape and its line budgets.
#
# STATE.md is loaded cold at every session start, so its size is a tax on
# every session. Over budget nothing is deleted — it moves: a stale completion
# to git log, a choice with reasoning behind it to a decision record, a
# durable fact to CLAUDE.md, a resolved carry-forward to nowhere.
#
# The Last updated line carries a date and nothing else. A status clause there
# says what "Just finished" says two lines below and goes stale on the next
# change, so this refuses one.

set -e

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
file="$root/docs/STATE.md"
fail=0

report() {
    printf 'docs/STATE.md: %s\n' "$1" >&2
    fail=1
}

[ -f "$file" ] || { report "missing"; exit 1; }

stamp=$(grep -n '^\*\*Last updated:\*\*' "$file" | head -1 | cut -d: -f2-)
if [ -z "$stamp" ]; then
    report "no '**Last updated:** YYYY-MM-DD' line"
elif ! printf '%s\n' "$stamp" | grep -Eq '^\*\*Last updated:\*\* [0-9]{4}-[0-9]{2}-[0-9]{2}$'; then
    report "the Last updated line carries more than a date:
    $stamp
  A status clause there duplicates 'Just finished' and goes stale on the
  next change. The date, and nothing else."
fi

# Section<TAB>budget. The budget counts every line under the heading,
# including its blank lines and its italic guidance.
budgets='In progress	10
Just finished	10
Next	12
After that	10
Carries forward	40'

printf '%s\n' "$budgets" | while IFS='	' read -r name budget; do
    n=$(awk -v want="## $name" '
        $0 == want { on = 1; next }
        on && /^## / { exit }
        on { count++ }
        END { print count + 0 }
    ' "$file")
    start=$(grep -c "^## $name\$" "$file" || true)
    if [ "$start" -eq 0 ]; then
        printf 'docs/STATE.md: no "## %s" section\n' "$name" >&2
        exit 1
    fi
    if [ "$n" -gt "$budget" ]; then
        printf 'docs/STATE.md: "%s" is %d lines, budget %d. Move something out, do not delete it.\n' \
            "$name" "$n" "$budget" >&2
        exit 1
    fi
done || fail=1

[ "$fail" -eq 0 ] || exit 1
echo "docs/STATE.md: within budget"
