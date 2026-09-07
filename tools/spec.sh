#!/bin/sh
# Retrieve a numbered section of a document instead of reading it whole.
#
# The specifications are frozen and large. Loading one costs most of a context
# window and buys nothing this cannot locate more precisely, so retrieval
# happens here rather than in a Read. A PreToolUse hook refuses an unbounded
# Read of a frozen document and points at these subcommands.
#
#   tools/spec.sh list                     the codes and the paths
#   tools/spec.sh sections BRIDGE          the heading tree with line counts
#   tools/spec.sh find BRIDGE dormant      every heading and line matching
#   tools/spec.sh read BRIDGE 3.7          one whole section
#
# A heading inside a fenced code block is not a heading; both specifications
# have `#` comment lines inside their figure blocks, so the fences are tracked.

set -e

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
specs="$root/docs/specs"

usage() {
    sed -n '3,14p' "$0" | sed 's/^# \{0,1\}//'
    exit "${1:-1}"
}

# A document's code is its filename upper-cased: docs/specs/bridge.md is
# BRIDGE. There is no index to keep in step with the directory, and adding a
# specification is adding a file.
codes() {
    for f in "$specs"/*.md; do
        [ -f "$f" ] || continue
        n=${f##*/}
        printf '%s\t%s\n' \
            "$(printf '%s' "${n%.md}" | tr '[:lower:]' '[:upper:]')" \
            "docs/specs/$n"
    done
}

path_for() {
    p=$(codes | awk -F '\t' -v c="$1" '$1 == c { print $2 }')
    [ -n "$p" ] || {
        echo "no document with code '$1'. Known codes:" >&2
        codes | awk -F '\t' '{ print "  " $1 }' >&2
        exit 1
    }
    printf '%s/%s' "$root" "$p"
}

# Every heading line, as: line<TAB>level<TAB>text. Fenced blocks are skipped.
headings() {
    awk '
        /^```/ { fence = !fence; next }
        fence { next }
        /^#+[ \t]/ {
            n = index($0, " ")
            level = length(substr($0, 1, n - 1))
            printf "%d\t%d\t%s\n", NR, level, substr($0, n + 1)
        }
    ' "$1"
}

cmd=${1:-}
[ -n "$cmd" ] || usage

case "$cmd" in
list)
    { printf 'code\tpath\n'; codes; } \
        | { column -t -s "$(printf '\t')" 2>/dev/null || cat; }
    ;;

sections)
    [ -n "${2:-}" ] || usage
    file=$(path_for "$2")
    total=$(wc -l < "$file")
    headings "$file" | awk -F '\t' -v total="$total" '
        { line[NR] = $1; lvl[NR] = $2; txt[NR] = $3 }
        END {
            for (i = 1; i <= NR; i++) {
                end = (i < NR) ? line[i + 1] - 1 : total
                indent = ""
                for (j = 2; j <= lvl[i]; j++) indent = indent "  "
                printf "%5d  %4d  %s%s\n", line[i], end - line[i] + 1, indent, txt[i]
            }
        }'
    ;;

find)
    [ -n "${3:-}" ] || usage
    file=$(path_for "$2")
    shift 2
    echo "== headings"
    headings "$file" | grep -i -- "$*" | awk -F '\t' '{ printf "%5d  %s\n", $1, $3 }' || true
    echo "== lines"
    grep -in -- "$*" "$file" | head -40 || true
    ;;

read)
    [ -n "${3:-}" ] || usage
    file=$(path_for "$2")
    # "3.7" matches the heading that opens with that number, and the section
    # runs to the next heading at the same level or shallower.
    headings "$file" | awk -F '\t' -v want="$3" '
        BEGIN { start = 0 }
        {
            if (!start && index($3, want) == 1 && substr($3, length(want) + 1, 1) ~ /[ .]/) {
                start = $1; level = $2; next
            }
            if (start && $2 <= level) { print start, $1 - 1; done = 1; exit }
        }
        # exit still runs END, so the fall-through to end-of-file is flagged.
        END { if (start && !done) print start, 0 }
    ' | while read -r a b; do
        [ "$b" = 0 ] && b=$(wc -l < "$file")
        sed -n "${a},${b}p" "$file"
    done
    ;;

*)
    usage ;;
esac
