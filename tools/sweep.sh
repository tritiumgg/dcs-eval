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

# --- the copies, the manifest and the breadcrumb ----------------------------
#
# Nothing here uses a git write path. `git checkout -- <file>` takes a working
# tree's uncommitted work with it, so a restore built on it would silently
# destroy an edit that had nothing to do with the sweep. Every mutated file is
# restored from a copy this run took first and confirmed with `cmp`.
#
# That per-file `cmp` is the load-bearing check. The `git status` snapshot
# around the run is the backstop, and it is a weaker one: it cannot see a
# gitignored path at all, so a mutation under `target/` would walk straight
# past it. What the snapshot catches that the copies cannot is a file created
# rather than modified. A reader who takes the snapshot for the guarantee will
# weaken the `cmp`; it is the other way round.

breadcrumb="$root/.sweep-inflight"

if [ -e "$breadcrumb" ]; then
    printf '%s: a previous run did not finish.\n\n' "$me" >&2
    cat "$breadcrumb" >&2
    printf '\nRestore each file from its copy by hand, check it with cmp, then\n' >&2
    printf 'remove %s. Nothing is swept until it is gone.\n' "$breadcrumb" >&2
    exit 2
fi

work=$(mktemp -d)
mkdir -p "$work/orig"
: > "$work/manifest"
keepwork=0
interrupted=0

note_breadcrumb() {
    {
        printf 'copies: %s\n' "$work/orig"
        printf 'pid: %s\n' "$$"
        printf 'control: %s\n' "${1:-<none yet>}"
        printf '\nrestore each with:\n'
        while IFS="$US" read -r path copy; do
            printf '  cp -p "%s" "%s"\n' "$copy" "$root/$path"
        done < "$work/manifest"
    } > "$breadcrumb"
}

note_breadcrumb

# Copy a file this control is about to touch, and record it. The manifest line
# is written straight after the `cp` and before the next one, so an interrupt
# between the two loses nothing: no file is edited until every copy is taken.
take_copy() {
    n=$(( $(wc -l < "$work/manifest") + 1 ))
    flat=$(printf '%s' "$1" | tr '/\\:' '___')
    copy=$(printf '%s/orig/%02d-%s' "$work" "$n" "$flat")
    cp -p "$root/$1" "$copy"
    printf '%s%s%s\n' "$1" "$US" "$copy" >> "$work/manifest"
}

# The one line that puts a file back, on its own so that it can be read, and
# substituted by the test that proves what the alternative costs. It is a copy
# and never `git checkout -- <file>`: checkout restores the committed content,
# so an uncommitted edit that had nothing to do with the sweep would be thrown
# away by a runner whose whole promise is to leave the tree as it found it.
#
# The copy is deliberately not `cp -p`. Preserving the original timestamp puts
# the restored file *older* than the artefacts cargo built from the mutated
# one, so cargo calls its own output fresh and the next control — and whoever
# runs the tests next — is testing a binary compiled from code that no longer
# exists on disk. The content is what `cmp` vouches for; the timestamp has to
# say the file just changed, because it just did.
put_back() {
    cp "$1" "$2" 2>/dev/null
}

# Put every copied file back and prove it went back. The manifest is the
# authority, not the loop that mutated: a control that failed halfway restores
# exactly what it had taken, in the same way as one that succeeded.
restore_all() {
    [ "$keepwork" -eq 0 ] || return 1
    [ -s "$work/manifest" ] || return 0
    bad=0
    while IFS="$US" read -r path copy; do
        tries=0
        # A test process that has not fully exited can still hold the file on
        # Windows, so a failing copy is retried briefly before it is believed.
        while ! put_back "$copy" "$root/$path"; do
            tries=$((tries + 1))
            [ "$tries" -ge 15 ] && break
            sleep 0.2
        done
        if ! cmp -s "$copy" "$root/$path"; then
            printf 'NOT-RESTORED  %s differs from its copy\n' "$path" >&2
            printf '              cp -p "%s" "%s"\n' "$copy" "$root/$path" >&2
            bad=1
        fi
    done < "$work/manifest"
    if [ "$bad" -ne 0 ]; then
        keepwork=1
        printf '              copies kept at %s\n' "$work/orig" >&2
        return 1
    fi
    : > "$work/manifest"
    return 0
}

on_exit() {
    status=$?
    trap - EXIT
    restored=$(wc -l < "$work/manifest" 2>/dev/null || echo 0)
    restore_all || status=2
    if [ "$interrupted" -eq 1 ]; then
        printf 'interrupted: restored %s files from their copies\n' "$restored" >&2
        [ "$status" -eq 2 ] || status=130
    fi
    [ "$keepwork" -eq 1 ] || { rm -f "$breadcrumb"; rm -rf "$work"; }
    exit "$status"
}
trap on_exit EXIT
trap 'interrupted=1; exit 130' INT TERM HUP

# --- applying one control's edits -------------------------------------------

# gawk on Windows opens a file in text mode and drops CR as it reads, so a
# CRLF file would match an LF anchor and then be written back LF-only — a
# silent conversion of a file the sweep promised not to change. MSYS grep is
# blind to CR for the same reason, so the bytes are counted with tr instead.
has_cr() {
    [ -n "$(tr -dc '\r' < "$1" | head -c 1)" ]
}

# The applier. Reads the hunks of one file and rewrites that file, counting
# every occurrence of an anchor before it changes anything: exactly one, or it
# writes nothing and says which anchor and how many places it found.
APPLY='
function bail(msg) { printf "%s\n", msg > "/dev/stderr"; exit 3 }
BEGIN {
    while ((getline line < hunks) > 0) {
        if (line == "") { mode = ""; continue }
        head = substr(line, 1, 2)
        if (head == "- " || line == "-") {
            if (mode != "anchor") { nh++; na[nh] = 0; nr[nh] = 0; mode = "anchor" }
            na[nh]++; A[nh, na[nh]] = (line == "-" ? "" : substr(line, 3))
        } else if (head == "+ " || line == "+") {
            if (mode == "") bail("a replacement line with no anchor above it: " line)
            mode = "repl"
            nr[nh]++; R[nh, nr[nh]] = (line == "+" ? "" : substr(line, 3))
        } else {
            bail("a block line is neither anchor nor replacement: " line)
        }
    }
    close(hunks)
    if (nh == 0) bail("the block holds no hunks")
}
{ L[NR] = $0 }
END {
    n = NR
    for (h = 1; h <= nh; h++) {
        c = 0
        for (i = 1; i + na[h] - 1 <= n; i++) {
            ok = 1
            for (k = 1; k <= na[h]; k++) if (L[i + k - 1] != A[h, k]) { ok = 0; break }
            if (ok) { c++; at[h] = i }
        }
        if (c == 0) bail(sprintf("no hunk matching \"%s\" (0 of 1 located)", A[h, 1]))
        if (c > 1) bail(sprintf("the anchor \"%s\" matches %d places, wanted exactly 1", A[h, 1], c))
    }
    for (h = 1; h <= nh; h++)
        for (g = h + 1; g <= nh; g++)
            if (at[h] <= at[g] + na[g] - 1 && at[g] <= at[h] + na[h] - 1)
                bail("two hunks of this block overlap")
    for (i = 1; i <= n; i++) {
        hit = 0
        for (h = 1; h <= nh; h++) if (at[h] == i) hit = h
        if (hit) {
            for (k = 1; k <= nr[hit]; k++) print R[hit, k]
            i += na[hit] - 1
            continue
        }
        print L[i]
    }
}'

# The paths one control touches, in the order its blocks first name them. A
# control may carry two blocks for one file — a line moved from here to there
# is two hunks far apart — and that file is copied once and rewritten once.
paths_of() {
    awk '/^F\t/ { p = $0; sub(/^F\t/, "", p); if (!(p in seen)) { seen[p] = 1; print p } }' \
        "$work/edits"
}

# Every hunk of one file, from however many blocks name it, with a blank line
# between blocks so the last hunk of one does not run into the first of the
# next.
hunks_of() {
    awk -v want="$1" '
        /^F\t/ {
            p = $0; sub(/^F\t/, "", p)
            on = (p == want)
            if (on && any) print ""
            next
        }
        on && /^L\t/ { t = $0; sub(/^L\t/, "", t); any = 1; print t }
    ' "$work/edits"
}

# Apply every edit of one control. Sets `unperformed` and returns 1 when the
# mutation no longer applies; whatever had already been mutated is left for
# restore_all, which is called by the caller either way.
apply_control() {
    unperformed=
    edits_of "$1" > "$work/edits"
    if [ ! -s "$work/edits" ]; then
        unperformed="the inventory carries no sweep-edit block"
        return 1
    fi
    paths=$(paths_of)
    # Every file is checked before any file is copied, and every copy is taken
    # before any file is written.
    for p in $paths; do
        if [ ! -f "$root/$p" ]; then
            unperformed="no such file: $p"
            return 1
        fi
        if has_cr "$root/$p"; then
            unperformed="$p carries CR line endings; anchors compare on LF lines, so no anchor can match"
            return 1
        fi
    done
    for p in $paths; do take_copy "$p"; done
    for p in $paths; do
        hunks_of "$p" > "$work/hunks"
        if why=$(awk -v hunks="$work/hunks" "$APPLY" "$root/$p" 2>&1 >"$work/out"); then
            mv "$work/out" "$root/$p"
        else
            unperformed="$p: $why"
            return 1
        fi
    done
    return 0
}

# --- running the command, and what counts as red ----------------------------

command -v timeout >/dev/null 2>&1 ||
    die "no timeout on PATH; a control that hangs would hang the sweep with a mutated file in the tree"

# Run one control's command from the repository root, capturing everything it
# said. Sets `rc`; 124 is the timeout's own.
run_command() {
    if timeout -k 10 "$timeout_s" sh -c "cd \"$root\" && $1" > "$work/out.log" 2>&1
    then rc=0
    else rc=$?
    fi
}

# The failing checks in what a command printed, in the two dialects this build
# speaks: the Rust harness names one line per failed test, and the Lua harness
# names the suite and the check it stopped at.
#
# An empty result is the point of this function. A Rust mutation that does not
# compile exits non-zero with no test having run, and a Lua one that is a
# syntax error prints no failing check either. Reading a non-zero exit as
# evidence would be the sweep manufacturing the very thing it exists to
# observe, so a non-zero exit with nothing here is BUILD-FAILED, never red.
failing_checks() {
    awk '/^test .* \.\.\. FAILED$/ || /^FAIL  / { print }' "$work/out.log"
}

# --- the run ----------------------------------------------------------------

printf '%s\n' "$rows" | grep -v "${US}out${US}" > "$work/todo" || true

# One baseline per distinct command, before anything is mutated, so a command
# that was already failing is reported as such rather than counted as a red
# this sweep produced.
: > "$work/baselines"
while IFS="$US" read -r id kind cmd reddens controls reason; do
    selected "$id" || continue
    grep -qF "$cmd$US" "$work/baselines" && continue
    run_command "$cmd"
    if [ "$rc" -eq 0 ]; then
        printf '%s%s\n' "$cmd" "$US" >> "$work/baselines"
    else
        printf '%s%sERROR  the command is red before any mutation (exit %s): %s\n' \
            "$cmd" "$US" "$rc" "$(failing_checks | head -1)" >> "$work/baselines"
    fi
done < "$work/todo"

baseline_of() {
    awk -v us="$US" -v want="$1" '
        { i = index($0, us); if (substr($0, 1, i - 1) == want) { print substr($0, i + 1); exit } }
    ' "$work/baselines"
}

# The tree is photographed after the baselines and before the first mutation.
# A command's first run can leave fixtures, caches or stray output behind, and
# those are not damage — they are already there in both photographs this way.
in_repo=0
if git -C "$root" rev-parse --git-dir >/dev/null 2>&1; then
    in_repo=1
    git -C "$root" status --porcelain > "$work/snap.before"
fi

reddened=0
green=0
unperformed_n=0
performed=0
failures=0

report() {
    printf '%-13s %s\n' "$1" "$2"
    shift 2
    for line in "$@"; do printf '              %s\n' "$line"; done
}

while IFS="$US" read -r id kind cmd reddens controls reason; do
    selected "$id" || continue
    performed=$((performed + controls))
    note_breadcrumb "$id"

    base=$(baseline_of "$cmd")
    if [ -n "$base" ]; then
        report UNPERFORMED "$id" "$base"
        unperformed_n=$((unperformed_n + 1))
        failures=$((failures + 1))
        continue
    fi

    if ! apply_control "$id"; then
        report UNPERFORMED "$id" "$unperformed"
        unperformed_n=$((unperformed_n + 1))
        failures=$((failures + 1))
        restore_all || exit 2
        continue
    fi

    run_command "$cmd"
    failed=$(failing_checks)
    restore_all || exit 2

    if [ "$rc" -eq 0 ]; then
        report STAYED-GREEN "$id" "$cmd" "the command exited 0; the mutation applied and nothing noticed"
        green=$((green + 1))
        failures=$((failures + 1))
    elif [ "$rc" -eq 124 ]; then
        report TIMEOUT "$id" "$cmd" "killed after ${timeout_s}s"
        failures=$((failures + 1))
    elif [ -z "$failed" ]; then
        report BUILD-FAILED "$id" "$cmd" \
            "exit $rc with no failing check named; a mutation that will not build proves nothing" \
            "$(head -3 "$work/out.log" | tr '\n' ' ')"
        failures=$((failures + 1))
    elif printf '%s\n' "$failed" | grep -qF "$reddens"; then
        report REDDENED "$id" "$cmd" "$(printf '%s\n' "$failed" | head -4 | tr '\n' '|')"
        reddened=$((reddened + 1))
    else
        report REDDENED-ELSEWHERE "$id" "$cmd" \
            "recorded: $reddens" \
            "went red instead: $(printf '%s\n' "$failed" | head -4 | tr '\n' '|')"
        failures=$((failures + 1))
    fi
done < "$work/todo"

note_breadcrumb

# --- what was covered, and what was not -------------------------------------

in_total=0
out_total=0
while IFS="$US" read -r id kind cmd reddens controls reason; do
    if [ "$kind" = out ]; then
        out_total=$((out_total + controls))
    else
        in_total=$((in_total + controls))
    fi
done <<EOF
$rows
EOF

printf '\n'
printf 'coverage: %s of %s controls swept, %s out of scope\n' \
    "$performed" "$((in_total + out_total))" "$out_total"
printf '          %s of %s in-scope controls performed\n' "$performed" "$in_total"
printf '%s\n' "$rows" | while IFS="$US" read -r id kind cmd reddens controls reason; do
    [ "$kind" = out ] || continue
    printf '          %s not swept: %s controls, %s\n' "$id" "$controls" "$reason"
done
printf 'summary:  %s reddened, %s stayed green, %s unperformed, %s failures\n' \
    "$reddened" "$green" "$unperformed_n" "$failures"

if [ "$in_repo" -eq 1 ]; then
    git -C "$root" status --porcelain > "$work/snap.after"
    if ! cmp -s "$work/snap.before" "$work/snap.after"; then
        printf '\nthe working tree is not as it was found:\n' >&2
        diff "$work/snap.before" "$work/snap.after" >&2 || true
        exit 2
    fi
fi

[ "$failures" -eq 0 ] || exit 1
exit 0
