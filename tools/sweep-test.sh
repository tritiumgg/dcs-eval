#!/bin/sh
# Drive tools/sweep.sh against a sandbox and assert what it says.
#
# The sweep's whole value is that it fails honestly: a control that stopped
# reddening must come back as a failure, a mutation that no longer applies must
# come back as UNPERFORMED rather than as a silent skip, and a file must come
# back byte-identical whatever happened. None of that is observable by running
# the sweep over the real inventory, where everything is expected to pass. So
# each case below is a fixture inventory in a sandbox, over fixture files and
# fake commands — `sh -c` one-liners, no interpreter and no compiler — and what
# the runner prints and exits with is asserted.
#
# Needs no toolchain, which is why it runs inside `mise run check` and in CI's
# preflight job while the sweep itself runs neither.

set -e

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

# Every case below must be reached; the count is asserted rather than
# reported. Raise this when a case is added.
CASES=10

sandbox=$(mktemp -d)
trap 'rm -rf "$sandbox"' EXIT INT TERM

cp tools/sweep.sh "$sandbox/sweep.sh"
runner="$sandbox/sweep.sh"
tree="$sandbox/tree"

pass=0
fail=0

# Assert an exit code and a substring of the combined output.
check() {
    name=$1; want=$2; got=$3; want_says=$4; out=$5

    if [ "$got" != "$want" ]; then
        fail=$((fail + 1))
        printf 'FAIL  %s\n  wanted exit %s, got %s\n  output: %s\n' "$name" "$want" "$got" "$out" >&2
        return
    fi
    case "$out" in
        *"$want_says"*) pass=$((pass + 1)) ;;
        *)
            fail=$((fail + 1))
            printf 'FAIL  %s\n  wanted output containing: %s\n  got: %s\n' \
                "$name" "$want_says" "$out" >&2 ;;
    esac
}

# A fresh tree: two fixture files, and a pristine copy of each to compare the
# tree against once the runner says it has put everything back.
fresh_tree() {
    rm -rf "$tree" "$sandbox/pristine"
    mkdir -p "$tree" "$sandbox/pristine"
    cat > "$tree/alpha.txt" <<'EOF'
first line
the line that moves
last line
EOF
    cat > "$tree/beta.txt" <<'EOF'
beta one
beta two
EOF
    cp -p "$tree/alpha.txt" "$tree/beta.txt" "$sandbox/pristine/"
}

# Every fixture file is byte-identical to the copy taken before the run.
tree_intact() {
    cmp -s "$tree/alpha.txt" "$sandbox/pristine/alpha.txt" &&
        cmp -s "$tree/beta.txt" "$sandbox/pristine/beta.txt"
}

# Run the sandboxed runner over $sandbox/inventory.md.
run_() {
    sh "$runner" --inventory "$sandbox/inventory.md" --root "$tree" "$@" 2>&1
}

# Fixture IDs are deliberately unlike the real ones: tools/nospecrefs.sh reads
# every tracked file outside docs/ and refuses a plan task ID, so a fixture
# that spelled one would redden a check this file has nothing to do with.

# --- listing ----------------------------------------------------------------

fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/applies

- command: `sh -c 'exit 0'`
- reddens: `held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```

### fixture/second

- command: `sh -c 'exit 0'`
- reddens: `held`

```sweep-edit beta.txt
- beta two
+ beta three
```

### other/elsewhere

- command: `sh -c 'exit 0'`
- reddens: `held`

```sweep-edit alpha.txt
- first line
+ initial line
```

### out/not-reached

- out-of-scope: nothing here is built yet, so there is nothing to
  break and nothing to put back.
- controls: 5
EOF

out=$(run_ --list) && got=0 || got=$?
check 'the listing names a control and its command' \
    0 "$got" "control       fixture/applies" "$out"

out=$(run_ --list) && got=0 || got=$?
check 'the listing names an out-of-scope group and its count' \
    0 "$got" "5 controls, nothing here is built yet, so there is nothing to break and nothing to put back." "$out"

out=$(run_ --list --only fixture/) && got=0 || got=$?
case "$out" in *other/elsewhere*) got=99 ;; esac
check 'a trailing slash selects a group and nothing else' \
    0 "$got" "fixture/second" "$out"

out=$(sh "$runner" --inventory "$sandbox/nosuch.md" --list 2>&1) && got=0 || got=$?
check 'a missing inventory is refused, not assumed empty' \
    2 "$got" "no inventory at" "$out"

# --- applying and putting back ----------------------------------------------

out=$(run_ --only fixture/applies) && got=0 || got=$?
tree_intact || got=98
check 'a mutation that applies leaves the file byte-identical' \
    0 "$got" "APPLIED       fixture/applies" "$out"

fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/moved

- command: `sh -c 'exit 0'`
- reddens: `held`

```sweep-edit alpha.txt
- a line this file does not carry
+ replaced
```
EOF
out=$(run_) && got=0 || got=$?
tree_intact || got=98
case "$out" in *SKIP*) got=97 ;; esac
check 'an anchor that no longer matches is unperformed, never skipped' \
    1 "$got" 'no hunk matching "a line this file does not carry" (0 of 1 located)' "$out"

fresh_tree
printf 'twin\ntwin\n' >> "$tree/alpha.txt"
cp -p "$tree/alpha.txt" "$sandbox/pristine/alpha.txt"
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/ambiguous

- command: `sh -c 'exit 0'`
- reddens: `held`

```sweep-edit alpha.txt
- twin
+ not twin
```
EOF
out=$(run_) && got=0 || got=$?
tree_intact || got=98
check 'an anchor matching twice is unperformed, not guessed at' \
    1 "$got" 'matches 2 places, wanted exactly 1' "$out"

fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/half

- command: `sh -c 'exit 0'`
- reddens: `held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```

```sweep-edit beta.txt
- a line beta does not carry
+ replaced
```
EOF
out=$(run_) && got=0 || got=$?
tree_intact || got=98
check 'a control that fails on its second file rolls the first back from the copy' \
    1 "$got" "UNPERFORMED   fixture/half" "$out"

fresh_tree
printf 'first line\r\nthe line that moves\r\nlast line\r\n' > "$tree/alpha.txt"
cp -p "$tree/alpha.txt" "$sandbox/pristine/alpha.txt"
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/crlf

- command: `sh -c 'exit 0'`
- reddens: `held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```
EOF
out=$(run_) && got=0 || got=$?
tree_intact || got=98
check 'a CR in the target is named, not left as a mysterious zero match' \
    1 "$got" "carries CR line endings" "$out"

# --- a run that never finished ----------------------------------------------

fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/applies

- command: `sh -c 'exit 0'`
- reddens: `held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```
EOF
printf 'copies: %s/elsewhere\n' "$sandbox" > "$tree/.sweep-inflight"
out=$(run_) && got=0 || got=$?
rm -f "$tree/.sweep-inflight"
tree_intact || got=98
check 'a leftover breadcrumb refuses the run before anything is touched' \
    2 "$got" "a previous run did not finish" "$out"

# --- the tally --------------------------------------------------------------

ran=$((pass + fail))
if [ "$ran" -ne "$CASES" ]; then
    printf '\nsweep-test: %d cases ran, %d expected. A case was lost.\n' \
        "$ran" "$CASES" >&2
    exit 1
fi
if [ "$fail" -ne 0 ]; then
    printf '\nsweep-test: %d passed, %d failed\n' "$pass" "$fail" >&2
    exit 1
fi
printf 'sweep-test: %d checks, all passed\n' "$pass"
