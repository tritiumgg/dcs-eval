#!/bin/sh
# Re-run every mutation this build's controls were proved with.
#
# Each control here was proved once, by hand, in the session that built it:
# break the code, watch the check go red, put the code back. Nothing re-ran
# those proofs, so a check that stopped reddening as the code moved underneath
# it would look exactly like a check that still works. This runner applies each
# recorded mutation again, runs the one command that must go red, restores the
# file from a copy it took first, and reports.
#
# The inventory is docs/mutations.md, which is also where the format is
# written down. Nothing about a control lives here: the runner only reads.
#
# Not part of `mise run check`. It is slow, it edits files in the working tree,
# and a run that fails must leave the tree exactly as it found it — none of
# which belongs in a gate that runs on every commit.
#
#   sh tools/sweep.sh                  every control
#   sh tools/sweep.sh --list           what the inventory holds, no edits
#   sh tools/sweep.sh --only paths/    one control, or one group by its slash
#
# Nothing runs in parallel. Two controls can name the same file, and a second
# mutation landing while the first is in place would prove nothing about
# either.

set -e

me=$(basename -- "$0")

usage() {
    cat <<EOF
usage: $me [--list] [--only <id>] [--inventory <file>] [--root <dir>] [--timeout <s>]

  --list             print what the inventory holds and exit; edits nothing
  --only <id>        one control, or every control in a group when the id
                     ends in a slash, as in --only paths/
  --inventory <file> read the inventory from here instead of docs/mutations.md
  --root <dir>       apply mutations under here instead of the repository root
  --timeout <s>      seconds a control's command may run before it is killed
EOF
}

die() {
    printf '%s: %s\n' "$me" "$1" >&2
    exit "${2:-2}"
}

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
inventory=
only=
list=0
timeout_s=900

while [ $# -gt 0 ]; do
    case $1 in
        --list) list=1 ;;
        --only) [ $# -ge 2 ] || die "--only wants an id"; only=$2; shift ;;
        --inventory) [ $# -ge 2 ] || die "--inventory wants a file"; inventory=$2; shift ;;
        --root) [ $# -ge 2 ] || die "--root wants a directory"; root=$2; shift ;;
        --timeout) [ $# -ge 2 ] || die "--timeout wants seconds"; timeout_s=$2; shift ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; die "unknown argument: $1" ;;
    esac
    shift
done

[ -d "$root" ] || die "no such directory: $root"
root=$(CDPATH= cd -- "$root" && pwd -P)
[ -n "$inventory" ] || inventory="$root/docs/mutations.md"
[ -f "$inventory" ] || die "no inventory at $inventory"

# --- the inventory ----------------------------------------------------------

# One row per heading: id, kind, command, reddens, controls, reason. `kind` is
# `in` for a control the runner performs and `out` for a group it only counts.
# Fenced blocks are skipped here; the edits are read separately, per control,
# by edits_of.
#
# Fields are separated by US rather than by a tab, because `read` treats a tab
# as IFS whitespace and would fold two empty fields into one — and an
# out-of-scope row has three empty fields in a row.
US=$(printf '\037')

index() {
    awk -v us="$US" '
        function value(line) {
            sub(/^- [a-z-]+:[ \t]*/, "", line)
            gsub(/`/, "", line)
            sub(/[ \t]+$/, "", line)
            return line
        }
        function emit() {
            if (id != "")
                printf "%s%s%s%s%s%s%s%s%s%s%s\n",
                    id, us, kind, us, cmd, us, reddens, us, controls, us, reason
            id = ""; kind = ""; cmd = ""; reddens = ""; controls = ""; reason = ""
        }
        /^```/ { fenced = !fenced; next }
        fenced { next }
        /^### / { emit(); id = $0; sub(/^### /, "", id); kind = "in"; controls = 1; next }
        id == "" { next }
        /^- command:[ \t]/ { cmd = value($0); next }
        /^- reddens:[ \t]/ { reddens = value($0); next }
        /^- out-of-scope:[ \t]/ { kind = "out"; reason = value($0); wrapping = 1; next }
        # A reason is prose and wraps; every other bullet is one line by rule.
        /^[ \t]+[^ \t]/ && wrapping { t = $0; sub(/^[ \t]+/, "", t); gsub(/`/, "", t); reason = reason " " t; next }
        { wrapping = 0 }
        /^- controls:[ \t]/ { controls = value($0) + 0; next }
        /^- folds:[ \t]/ { controls = value($0) + 0; next }
        END { emit() }
    ' "$inventory"
}

# The `sweep-edit` blocks of one control, as a stream: `F<TAB><path>` opening a
# file, then one `L<TAB><line>` per line of the block, verbatim.
edits_of() {
    awk -v want="$1" '
        /^### / { id = $0; sub(/^### /, "", id); on = (id == want); next }
        !on { next }
        /^```sweep-edit / { path = $0; sub(/^```sweep-edit[ \t]+/, "", path); fenced = 1; printf "F\t%s\n", path; next }
        fenced && /^```/ { fenced = 0; next }
        fenced { printf "L\t%s\n", $0 }
    ' "$inventory"
}

# Does this id fall under --only? An empty --only takes everything; one ending
# in a slash takes the group.
selected() {
    [ -n "$only" ] || return 0
    case $only in
        */) case $1 in "$only"*) return 0 ;; esac ;;
        *) [ "$1" = "$only" ] && return 0 ;;
    esac
    return 1
}

rows=$(index)
[ -n "$rows" ] || die "the inventory holds no controls: $inventory"

if [ "$list" -eq 1 ]; then
    printf '%s\n' "$rows" | while IFS="$US" read -r id kind cmd reddens controls reason; do
        selected "$id" || continue
        if [ "$kind" = out ]; then
            printf 'out-of-scope  %-38s %s controls, %s\n' "$id" "$controls" "$reason"
        else
            printf 'control       %-38s %s\n' "$id" "$cmd"
        fi
    done
    exit 0
fi

die "only --list is built so far" 2
