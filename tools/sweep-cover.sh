#!/bin/sh
# Hold docs/mutations.md to the rows docs/PLAN.md claims a mutation for.
#
# Without this, the sweep's coverage line is summed from the inventory alone,
# so a control the plan names and nobody wrote into the inventory moves neither
# the numerator nor the denominator — and the sweep would report full coverage
# of a set it had quietly shrunk. That is, one level up, exactly the defect the
# sweep exists to catch.
#
# What it checks is presence per plan row, not per mutation: every Stage 3 to 6
# row whose done-condition names a mutation has at least one entry in the
# inventory, and every entry names a row the plan carries. A row naming three
# mutations with one entry written for it passes here. That gap is held by the
# rule in the inventory's own header — a task that builds a control adds its
# entry in the same pull request — and by nothing else.
#
# No toolchain, so it runs inside `mise run check` and in CI's preflight job.

set -e

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
# --root points the two reads at a sandbox, which is how the cases below
# prove this refuses: over the repository's own pair it is expected to pass,
# and a check nobody has watched fail is not evidence.
if [ "$1" = --root ] && [ -n "$2" ]; then
    root=$(CDPATH= cd -- "$2" && pwd -P)
fi
plan="$root/docs/PLAN.md"
inventory="$root/docs/mutations.md"

for f in "$plan" "$inventory"; do
    [ -f "$f" ] || { printf 'sweep-cover: missing %s\n' "$f" >&2; exit 2; }
done

# The plan's own IDs are matched by a pattern built out of character classes.
# Spelling one literally here would redden tools/nospecrefs.sh, which refuses
# a plan task ID anywhere outside docs/, CLAUDE.md and README.md.
ID='T[0-9][0-9]'

# Every Stage 3 to 6 row whose done-condition names a mutation. The stage is
# read from the heading above the table. The row is matched whole rather than
# by column: a cell can carry a literal pipe inside backticks, which shifts
# every column after it and would drop the row silently.
claimed=$(awk -v id="$ID" '
    /^## Stage / { stage = $3 + 0 }
    stage < 3 || stage > 6 { next }
    $0 !~ ("^\\|[ \t]*" id "[ \t]*\\|") { next }
    /mutations?:/ {
        row = $0
        sub(/^\|[ \t]*/, "", row)
        sub(/[ \t]*\|.*$/, "", row)
        print row
    }
' "$plan" | sort -u)

# Every task the inventory names. The runner itself is excluded: its own row is
# the one row whose control cannot be swept by the runner it describes, and the
# inventory says so as an out-of-scope entry rather than as a task bullet.
written=$(awk -v id="$ID" '
    $0 ~ ("^- task:[ \t]+" id "[ \t]*$") { t = $3; print t }
' "$inventory" | sort -u)

fail=0

missing=$(printf '%s\n' "$claimed" | grep -vxF "$(printf '%s\n' "$written")" 2>/dev/null || true)
for m in $missing; do
    [ -n "$m" ] || continue
    printf 'sweep-cover: the plan names a mutation for %s and the inventory has no entry for it\n' "$m" >&2
    fail=1
done

stray=$(printf '%s\n' "$written" | grep -vxF "$(printf '%s\n' "$claimed")" 2>/dev/null || true)
for s in $stray; do
    [ -n "$s" ] || continue
    printf 'sweep-cover: the inventory files a control under %s, which names no mutation in the plan\n' "$s" >&2
    fail=1
done

if [ "$fail" -ne 0 ]; then
    printf '\nAdd the entry to docs/mutations.md, or say in docs/PLAN.md why the row\n' >&2
    printf 'carries no mutation. An inventory that judges its own coverage judges\n' >&2
    printf 'nothing.\n' >&2
    exit 1
fi

n=$(printf '%s\n' "$claimed" | grep -c . || true)
printf 'sweep-cover: %s plan rows name a mutation, all present in the inventory\n' "$n"
