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
CASES=24

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

fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/applies

- command: `sh -c 'if grep -q moved alpha.txt; then echo "FAIL  fixture: held"; exit 1; fi; exit 0'`
- reddens: `FAIL  fixture: held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```
EOF
sleep 1
touch "$sandbox/before-the-run"
out=$(run_) && got=0 || got=$?
tree_intact || got=98
check 'a control that still reddens passes, and its file comes back identical' \
    0 "$got" "REDDENED      fixture/applies" "$out"

# A restore that put the original timestamp back would leave the file older
# than whatever was built from the mutated one, and cargo would call its own
# output fresh — so the next control, and whoever runs the tests afterwards,
# would be testing code that is no longer on disk.
if [ -n "$(find "$tree/alpha.txt" -newer "$sandbox/before-the-run")" ]; then
    got=0
else
    got=96
fi
check 'a restored file is stamped as having just changed, because it did' \
    0 "$got" "REDDENED      fixture/applies" "$out"

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

# --- what the command said --------------------------------------------------

# Every command below is a one-liner that reads the fixture file, so its
# baseline run — before any mutation — is green and its mutated run is not.
# A command that failed both ways would be a baseline error, which is its own
# case further down.

fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/unnoticed

- command: `sh -c 'exit 0'`
- reddens: `held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```
EOF
out=$(run_) && got=0 || got=$?
tree_intact || got=98
check 'a mutation nothing notices is a failure, not a pass' \
    1 "$got" "the command exited 0" "$out"

fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/will-not-build

- command: `sh -c 'if grep -q moved alpha.txt; then echo "error[E0425]: cannot find value"; exit 101; fi; exit 0'`
- reddens: `held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```
EOF
out=$(run_) && got=0 || got=$?
tree_intact || got=98
case "$out" in *REDDENED*) got=97 ;; esac
check 'a mutation that will not build is not evidence of a red control' \
    1 "$got" "BUILD-FAILED  fixture/will-not-build" "$out"

fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/moved-red

- command: `sh -c 'if grep -q moved alpha.txt; then echo "test other::tests::somewhere_else ... FAILED"; exit 1; fi; exit 0'`
- reddens: `tests::the_one_recorded`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```
EOF
out=$(run_) && got=0 || got=$?
tree_intact || got=98
check 'a red that has moved names both the record and what actually failed' \
    1 "$got" "went red instead: test other::tests::somewhere_else ... FAILED" "$out"

fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/already-red

- command: `sh -c 'echo "FAIL  fixture: broken before anyone touched it"; exit 1'`
- reddens: `held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```
EOF
out=$(run_) && got=0 || got=$?
tree_intact || got=98
check 'a command already failing is reported, and nothing is mutated under it' \
    1 "$got" "the command is red before any mutation" "$out"

fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/slow

- command: `sh -c 'if grep -q moved alpha.txt; then sleep 9; fi; exit 0'`
- reddens: `held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```
EOF
out=$(run_ --timeout 1) && got=0 || got=$?
tree_intact || got=98
check 'a command that will not finish is killed, and the file still comes back' \
    1 "$got" "TIMEOUT       fixture/slow" "$out"

# --- a restore that does not restore ----------------------------------------

fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/wrecks-its-target

- command: `sh -c 'if grep -q moved alpha.txt; then rm -f alpha.txt; mkdir alpha.txt; fi; exit 0'`
- reddens: `held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```

### fixture/never-reached

- command: `sh -c 'exit 0'`
- reddens: `held`

```sweep-edit beta.txt
- beta two
+ beta three
```
EOF
out=$(run_) && got=0 || got=$?
case "$out" in *never-reached*) got=97 ;; esac
check 'a file that will not go back stops the run and keeps the copies' \
    2 "$got" "copies kept at" "$out"

# --- the tree the run was handed --------------------------------------------
#
# These want a real repository, because what they assert is what
# `git status --porcelain` says before and against after. Signing is off for
# the sandbox's own commits: they are fixtures, not history.

repo="$sandbox/repo"
mkdir -p "$repo"
git -C "$repo" init -q
git -C "$repo" config user.email fixture@example.invalid
git -C "$repo" config user.name fixture
git -C "$repo" config commit.gpgsign false
git -C "$repo" config core.autocrlf false

fresh_repo() {
    cat > "$repo/alpha.txt" <<'EOF'
first line
the line that moves
last line
EOF
    printf 'kept\n' > "$repo/unrelated.txt"
    git -C "$repo" add alpha.txt unrelated.txt
    git -C "$repo" commit -q -m fixture >/dev/null 2>&1 || true
    # The uncommitted edit a git-checkout restore would throw away.
    printf 'an edit nobody committed\n' >> "$repo/alpha.txt"
}

cat > "$sandbox/inventory.md" <<'EOF'
### fixture/in-a-repo

- command: `sh -c 'if grep -q moved alpha.txt; then echo "FAIL  fixture: held"; exit 1; fi; exit 0'`
- reddens: `FAIL  fixture: held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```
EOF

fresh_repo
out=$(sh "$runner" --inventory "$sandbox/inventory.md" --root "$repo" 2>&1) && got=0 || got=$?
grep -q 'an edit nobody committed' "$repo/alpha.txt" || got=98
check 'a copy-based restore keeps an uncommitted edit the sweep never asked about' \
    0 "$got" "REDDENED      fixture/in-a-repo" "$out"

# The same sandbox against a runner whose one restore line is `git checkout`.
sed 's|^    cp "\$1" "\$2" 2>/dev/null$|    git -C "$root" checkout -- "$2"|' \
    tools/sweep.sh > "$sandbox/checkout.sh"
if cmp -s tools/sweep.sh "$sandbox/checkout.sh"; then
    fail=$((fail + 1))
    printf 'FAIL  the restore line the git-checkout case substitutes has moved\n' >&2
else
    fresh_repo
    out=$(sh "$sandbox/checkout.sh" --inventory "$sandbox/inventory.md" --root "$repo" 2>&1) \
        && got=0 || got=$?
    grep -q 'an edit nobody committed' "$repo/alpha.txt" && got=98
    check 'a git-checkout restore loses the uncommitted edit and is caught' \
        2 "$got" "alpha.txt differs from its copy" "$out"
fi
# That run kept its copies and its breadcrumb on purpose. The next case would
# otherwise be refused by it, which is the breadcrumb working as intended.
rm -f "$repo/.sweep-inflight"

fresh_repo
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/collateral

- command: `sh -c 'if grep -q moved alpha.txt; then rm -f unrelated.txt; echo "FAIL  fixture: held"; exit 1; fi; exit 0'`
- reddens: `FAIL  fixture: held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```
EOF
out=$(sh "$runner" --inventory "$sandbox/inventory.md" --root "$repo" 2>&1) && got=0 || got=$?
check 'damage the copies cannot see is caught by the snapshot around the run' \
    2 "$got" "the working tree is not as it was found" "$out"

fresh_repo
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/leaves-a-file

- command: `sh -c 'touch made-on-every-run.txt; if grep -q moved alpha.txt; then echo "FAIL  fixture: held"; exit 1; fi; exit 0'`
- reddens: `FAIL  fixture: held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```
EOF
rm -f "$repo/made-on-every-run.txt"
out=$(sh "$runner" --inventory "$sandbox/inventory.md" --root "$repo" 2>&1) && got=0 || got=$?
check 'a file the first run leaves behind is not read as damage' \
    0 "$got" "REDDENED      fixture/leaves-a-file" "$out"
rm -f "$repo/made-on-every-run.txt"

# --- an interrupted run -----------------------------------------------------

fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/interruptible

- command: `sh -c 'if grep -q moved alpha.txt; then sleep 4; fi; exit 0'`
- reddens: `held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```
EOF
# The runner is backgrounded directly rather than through the helper, so that
# the signal reaches the shell holding the trap and not a wrapper around it.
# A trap fires once the command in front of it returns, so the run ends when
# the sleep does, not when the signal lands.
sh "$runner" --inventory "$sandbox/inventory.md" --root "$tree" \
    > "$sandbox/interrupt.log" 2>&1 &
bg=$!
sleep 2
kill -TERM "$bg" 2>/dev/null || true
wait "$bg" && got=0 || got=$?
out=$(cat "$sandbox/interrupt.log")
tree_intact || got=98
[ -e "$tree/.sweep-inflight" ] && got=97
check 'an interrupted run puts the file back and says how many' \
    130 "$got" "interrupted: restored" "$out"

# --- what the run says it covered -------------------------------------------

fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/applies

- command: `sh -c 'if grep -q moved alpha.txt; then echo "FAIL  fixture: held"; exit 1; fi; exit 0'`
- reddens: `FAIL  fixture: held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```

### fixture/second

- command: `sh -c 'if grep -q three beta.txt; then echo "FAIL  fixture: held"; exit 1; fi; exit 0'`
- reddens: `FAIL  fixture: held`

```sweep-edit beta.txt
- beta two
+ beta three
```

### out/not-reached

- out-of-scope: nothing here is built yet.
- controls: 5
EOF
out=$(run_) && got=0 || got=$?
tree_intact || got=98
check 'the run says how much of the whole it covered' \
    0 "$got" "coverage: 2 of 7 controls swept, 5 out of scope" "$out"

out=$(run_ --only fixture/applies) && got=0 || got=$?
tree_intact || got=98
check 'a filtered run cannot be read as a whole one' \
    0 "$got" "1 of 2 in-scope controls performed" "$out"

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
