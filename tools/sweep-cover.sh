#!/bin/sh
# Hold docs/mutations.md to the rows docs/PLAN.md claims a mutation for.
#
# Without this, the sweep's coverage line is summed from the inventory alone,
# so a control the plan names and nobody wrote into the inventory moves neither
# the numerator nor the denominator — and the sweep would report full coverage
# of a set it had quietly shrunk. That is, one level up, exactly the defect the
# sweep exists to catch.
#
# What it checks is presence per plan row, not per mutation: every Stage 0 to 8
# row whose done-condition names a mutation has at least one entry in the
# inventory, and every entry names a row the plan carries. A row naming three
# mutations with one entry written for it passes here. That gap is held by the
# rule in the inventory's own header — a task that builds a control adds its
# entry in the same pull request — and by nothing else.
#
# The two directions read different windows of the plan, and the difference is
# deliberate. Presence is owed only by Stages 0 to 8, the stages that are
# built: a later stage's row has no code to mutate yet, and demanding an entry
# for it would fail every run until the last row landed. Stage 9's rows are
# live proofs, and the few that name a mutation have entries written as their
# code landed. A stray is checked against the rows of Stage 0 upwards, however
# far the plan grows, because a stage being unbuilt is no reason to call the
# first entry written for it a control filed under a row that names no
# mutation.
#
# Stages 0 to 2 were once refused an entry outright: they closed before the
# sweep existed and stood in its figure as one hand-counted group, so an entry
# under one of their rows would have been counted twice. Each of their
# mutations now has an entry of its own, and the refusal went with the count.
# Presence has to be owed there, not merely allowed, or a Stage 0 to 2 entry
# deleted later would shrink the in-scope figure with nothing noticing.
#
# A plan that is finished is retired beside the current one rather than
# deleted, and both are read. Every entry in the inventory names the row it
# came from, so the day a finished plan stopped being read, every entry filed
# under it would become a stray and the gate would fail on work that shipped.
# Each plan numbers its own stages from zero, and the window above is about
# built rows against live ones, not about how far any one document got.
#
# Stage 0 to 8 stood in for "already built" only because this gate was written
# at the end of the plan it was written for, where every such row was. A plan
# still being built breaks that: its rows are written before their code, and a
# gate demanding an entry for a row nobody has started would fail every run
# until the last one landed. So the current plan — docs/PLAN.md — owes an
# entry for a row it marks done, and a retired plan, every row of which is
# done by the fact of its retirement, owes one across the built window as
# before. Either way, an entry is allowed under any row either plan names a
# mutation for.
#
# **What the mark does not do.** It is typed by hand, by the same person who
# would have written the entry, so a row that lands unmarked owes nothing and
# nothing here notices. That hole is why every run prints how many rows are
# not marked yet: the count falls as the plan is built, and a run where it did
# not fall while a row landed is the thing to look at. A plan is a document,
# and no reading of it can tell that code exists.
#
# A task ID belongs to one row across every plan. Reusing one would file a new
# row's controls under an old row's entries and read as covered, so a duplicate
# is refused here rather than discovered by a control going missing.
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

# Every row of stages $1 to $2 whose done-condition names a mutation, where an
# empty $2 means every stage from $1 upwards. The stage is read from the
# heading above the table, so a row above the first heading belongs to no stage
# and is read by neither direction: a numeric ceiling would have to be raised
# the day a stage passed it, and a floor of zero would silently swallow a row
# the plan had not filed under a stage at all. That is why the stage starts
# below zero rather than at it: both directions now ask from Stage 0, and a
# row above every heading must still be read by neither.
#
# The row is matched whole rather than by column: a cell can carry a literal
# pipe inside backticks, which shifts every column after it and would drop the
# row silently.
#
# $3 is `done` to keep only a row the plan marks done, which is how the
# current plan says a row's code exists. $4 is `current` for docs/PLAN.md and
# `retired` for the plans beside it; the stage resets per file, because each
# plan numbers its stages from its own zero.
rows() {
    lo=$1
    hi=$2
    only=$3
    if [ "$4" = current ]; then
        set -- "$plan"
    else
        set --
        for f in "$root"/docs/PLAN-*.md; do
            [ -f "$f" ] && set -- "$@" "$f"
        done
        [ "$#" -gt 0 ] || return 0
    fi
    awk -v id="$ID" -v lo="$lo" -v hi="$hi" -v only="$only" '
        FNR == 1 { stage = -1 }
        /^## Stage / { stage = $3 + 0 }
        stage < 0 || stage < lo { next }
        hi != "" && stage > hi { next }
        $0 !~ ("^\\|[ \t]*" id "[ \t]*\\|") { next }
        # The mark is read from the task cell alone. Read from the whole row it
        # would be set by the words "**Done when**" in a done-condition, or by
        # anything else a later cell happened to say.
        only == "done" {
            n = split($0, cell, "|")
            if (n < 3 || cell[3] !~ /\*\*Done/) next
        }
        /mutations?:/ {
            row = $0
            sub(/^\|[ \t]*/, "", row)
            sub(/[ \t]*\|.*$/, "", row)
            print row
        }
    ' "$@" | sort -u
}

# What an entry is owed for, and what an entry is allowed to name.
claimed=$({ rows 0 8 any retired; rows 0 '' done current; } | sort -u)
named=$({ rows 0 '' any retired; rows 0 '' any current; } | sort -u)

# Every task the inventory names, in-scope entry and out-of-scope entry alike:
# a `task:` bullet is read the same way wherever it sits. The runner's own row
# is the one whose control cannot be swept by the runner it describes, so its
# entry is out-of-scope and carries the bullet anyway — which is what keeps it
# out of the missing list, since the plan row names a mutation like any other.
written=$(awk -v id="$ID" '
    $0 ~ ("^- task:[ \t]+" id "[ \t]*$") { t = $3; print t }
' "$inventory" | sort -u)

fail=0

# An ID two plans both carry would file one row's controls under the other's
# entries, and read as covered. Every row is read here, whether it names a
# mutation or not: a reused ID is wrong before anyone asks what it covers.
dup=$(
    set -- "$plan"
    for f in "$root"/docs/PLAN-*.md; do
        [ -f "$f" ] && set -- "$@" "$f"
    done
    awk -v id="$ID" '
        $0 !~ ("^\\|[ \t]*" id "[ \t]*\\|") { next }
        {
            row = $0
            sub(/^\|[ \t]*/, "", row)
            sub(/[ \t]*\|.*$/, "", row)
            print row
        }
    ' "$@" | sort | uniq -d
)
for d in $dup; do
    [ -n "$d" ] || continue
    printf 'sweep-cover: %s is carried by more than one row; an ID belongs to one\n' "$d" >&2
    fail=1
done

missing=$(printf '%s\n' "$claimed" | grep -vxF "$(printf '%s\n' "$written")" 2>/dev/null || true)
for m in $missing; do
    [ -n "$m" ] || continue
    printf 'sweep-cover: the plan names a mutation for %s and the inventory has no entry for it\n' "$m" >&2
    fail=1
done

stray=$(printf '%s\n' "$written" | grep -vxF "$(printf '%s\n' "$named")" 2>/dev/null || true)
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

# The mark is typed by hand, so this count is the only thing that would look
# odd if a row landed without one: it falls as the plan is built, and a run
# where it did not fall while a row landed is worth a look.
waiting=$(rows 0 '' any current | grep -c . || true)
marked=$(rows 0 '' done current | grep -c . || true)
if [ "$waiting" -gt "$marked" ]; then
    printf 'sweep-cover: %s more rows in the current plan name a mutation and are not marked done, owing nothing yet\n' \
        "$((waiting - marked))"
fi
