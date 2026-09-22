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
CASES=47

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
    0 "$got" "5 mutations, nothing here is built yet, so there is nothing to break and nothing to put back." "$out"

out=$(run_ --list --only fixture/) && got=0 || got=$?
case "$out" in *other/elsewhere*) got=99 ;; esac
check 'a trailing slash selects a group and nothing else' \
    0 "$got" "fixture/second" "$out"

out=$(sh "$runner" --inventory "$sandbox/nosuch.md" --list 2>&1) && got=0 || got=$?
check 'a missing inventory is refused, not assumed empty' \
    2 "$got" "no inventory at" "$out"

# An entry with no `reddens:` would pass on any failure at all, because the
# REDDENED verdict greps for that string and an empty one matches everything.
# The command below fails with a line naming nothing this entry watches.
fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/no-reddens

- command: `sh -c 'echo "FAIL  fixture: something else entirely"; exit 1'`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```
EOF
out=$(run_) && got=0 || got=$?
tree_intact || got=98
case "$out" in *REDDENED*) got=97 ;; esac
check 'an entry with no reddens line is refused, not read as a red control' \
    2 "$got" "carries no reddens: line" "$out"

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

# A control owns what sits under its own `###` heading and nothing past the
# next heading. The inventory's prose sections illustrate the block format, so
# a block written under a `##` must be documentation rather than a hunk handed
# to whichever control was last read — which is invisible when it happens,
# because it fails as an anchor that does not match.
fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/applies

- command: `sh -c 'if grep -q moved alpha.txt; then echo "FAIL  fixture: held"; exit 1; fi; exit 0'`
- reddens: `FAIL  fixture: held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```

## Out of scope

What a block looks like, written out here for a reader:

```sweep-edit alpha.txt
- a line this file does not carry
+ replaced
```

- command: `sh -c 'exit 1'`

### out/not-reached

- out-of-scope: nothing here is built yet.
- controls: 5
EOF
out=$(run_) && got=0 || got=$?
tree_intact || got=98
check 'a block under a prose heading is not a hunk of the control above it' \
    0 "$got" "REDDENED      fixture/applies" "$out"

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

# The Lua harness catches its own load error and prints it in the shape of a
# failing check, so a mutation that is a syntax error looks from the outside
# like a red that moved. It is a build failure and has to be reported as one.
fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/will-not-load

- command: `sh -c 'if grep -q moved alpha.txt; then echo "FAIL  fixture/whatever: raised: harness: cannot load the executor: unexpected symbol"; exit 1; fi; exit 0'`
- reddens: `FAIL  fixture/whatever`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```
EOF
out=$(run_) && got=0 || got=$?
tree_intact || got=98
case "$out" in *REDDENED*) got=97 ;; esac
check 'a Lua mutation that will not load is a build failure, not a red' \
    1 "$got" "BUILD-FAILED  fixture/will-not-load" "$out"

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

# A kill -9 fires no trap, so the breadcrumb is all a human is left with. It
# has to name the mutated file and the copy it goes back from — and the
# command it prints has to be one that actually puts the file back.
fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/killed-outright

- command: `sh -c 'if grep -q moved alpha.txt; then sleep 9; fi; exit 0'`
- reddens: `held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```
EOF
sh "$runner" --inventory "$sandbox/inventory.md" --root "$tree" \
    > "$sandbox/killed.log" 2>&1 &
bg=$!
# Killing before the mutation lands would prove nothing about what the
# breadcrumb says once it has.
waited=0
while ! grep -q moved "$tree/alpha.txt" 2>/dev/null; do
    waited=$((waited + 1))
    [ "$waited" -ge 100 ] && break
    sleep 0.1
done
kill -KILL "$bg" 2>/dev/null || true
wait "$bg" >/dev/null 2>&1 || true
out=$(cat "$tree/.sweep-inflight" 2>&1)
got=0
grep -q moved "$tree/alpha.txt" || got=95
# Run the recovery command the breadcrumb printed, exactly as it printed it.
sh -c "$(grep '^  cp ' "$tree/.sweep-inflight")" || got=94
tree_intact || got=93
# Nothing cleaned up after the killed run, so this does: the copies it took
# and the breadcrumb that pointed at them.
rm -rf "$(dirname "$(sed -n 's/^copies: //p' "$tree/.sweep-inflight")")"
rm -f "$tree/.sweep-inflight"
# The destination is compared on its tail: the runner resolves --root with
# `pwd -P`, which on Windows spells the same directory a different way.
check 'a run killed outright leaves a breadcrumb that names the file and puts it back' \
    0 "$got" '/tree/alpha.txt"' "$out"

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
    0 "$got" "coverage: 2 of 7 mutations named by a plan done-condition swept, 5 out of scope" "$out"

out=$(run_ --only fixture/applies) && got=0 || got=$?
tree_intact || got=98
check 'a filtered run cannot be read as a whole one' \
    0 "$got" "1 of 2 in scope performed, 0 unperformed" "$out"

# A control whose mutation no longer applies has to come off the coverage
# figure as well as out of the summary, or the line the runner exists to keep
# honest overstates by exactly the controls it could not perform.
fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/applies

- command: `sh -c 'if grep -q moved alpha.txt; then echo "FAIL  fixture: held"; exit 1; fi; exit 0'`
- reddens: `FAIL  fixture: held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```

### fixture/anchor-gone

- command: `sh -c 'exit 0'`
- reddens: `held`

```sweep-edit beta.txt
- a line beta does not carry
+ replaced
```
EOF
out=$(run_) && got=0 || got=$?
tree_intact || got=98
check 'a control that could not be performed comes off the coverage figure' \
    1 "$got" "1 of 2 in scope performed, 1 unperformed" "$out"

# So does a control whose mutation never built. It reached no verdict about
# the check either, and the argument that keeps UNPERFORMED off the figure is
# the same one word for word.
fresh_tree
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/applies

- command: `sh -c 'if grep -q moved alpha.txt; then echo "FAIL  fixture: held"; exit 1; fi; exit 0'`
- reddens: `FAIL  fixture: held`

```sweep-edit alpha.txt
- the line that moves
+ the line that moved
```

### fixture/will-not-build

- command: `sh -c 'if grep -q three beta.txt; then echo "error[E0425]: cannot find value"; exit 101; fi; exit 0'`
- reddens: `FAIL  fixture: held`

```sweep-edit beta.txt
- beta two
+ beta three
```
EOF
out=$(run_) && got=0 || got=$?
tree_intact || got=98
check 'a mutation that never built comes off the coverage figure too' \
    1 "$got" "1 of 2 in scope performed, 0 unperformed, 1 inconclusive" "$out"

# --- the gate that stops the inventory judging its own coverage -------------
#
# A control the plan names and nobody wrote into the inventory moves neither
# side of the sweep's coverage line, so the sweep would report full coverage of
# a set it had quietly shrunk. These prove the gate that catches that.
#
# The fixture IDs are assembled rather than written: tools/nospecrefs.sh
# refuses a plan task ID anywhere outside docs/, and a fixture plan has to
# carry IDs shaped exactly like the real ones or the gate would not read them.
one=$(printf 'T%s' 91)
two=$(printf 'T%s' 92)
three=$(printf 'T%s' 93)
four=$(printf 'T%s' 94)
floor=$(printf 'T%s' 95)
ceiling=$(printf 'T%s' 96)

cover="$sandbox/cover"
mkdir -p "$cover/docs" "$cover/tools"
cp tools/sweep-cover.sh "$cover/tools/sweep-cover.sh"

# A six-row fixture plan, retired: every row of a retired plan is done by the
# fact of its retirement, so the built window is what decides which of them owe
# an entry. The stage heading is what the gate reads the stage off. The second
# row names no mutation, so it is owed no entry; the third sits outside the
# window presence is owed in, so it is owed no entry either — but an entry
# written for it early is not a stray; the fourth sits before Stage 3, which is
# owed an entry like any other built stage. The last two sit on the window's two
# edges, Stage 0 and Stage 8, so narrowing it at either end drops a row that is
# owed an entry.
#
# The current plan is a heading and nothing else here: the gate requires the
# file, and the cases for what a current plan owes are further down.
printf '# a fixture plan with no rows yet\n' > "$cover/docs/PLAN.md"
cat > "$cover/docs/PLAN-SHIPPED.md" <<EOF
## Stage 0 — the fixture's floor

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| $floor | a floor fixture | it holds; mutation: break it and it does not | — | developer-only |

## Stage 1 — an earlier fixture stage

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| $four | an early fixture | it holds; mutation: break it and it does not | — | developer-only |

## Stage 4 — a fixture stage

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| $one | a fixture | it holds; mutation: break it and it does not | — | developer-only |
| $two | another | it holds | — | developer-only |

## Stage 8 — the fixture's last built stage

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| $ceiling | a ceiling fixture | it holds; mutation: break it and it does not | — | developer-only |

## Stage 9 — a fixture stage built later

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| $three | a later fixture | it holds; mutation: break it | — | developer-only |
EOF

# An inventory of one entry per ID named, so a row outside the presence window
# can be seen being accepted rather than called a stray.
covered() {
    : > "$cover/docs/mutations.md"
    n=0
    for id in "$@"; do
        n=$((n + 1))
        printf '### fixture/entry-%s\n\n- task: %s\n- reddens: held\n\n' "$n" "$id" \
            >> "$cover/docs/mutations.md"
    done
}

covered "$one" "$four" "$floor" "$ceiling"
out=$(sh "$cover/tools/sweep-cover.sh" --root "$cover" 2>&1) && got=0 || got=$?
check 'a plan row and an entry for it is the case that passes' \
    0 "$got" "4 plan rows name a mutation" "$out"

covered "$two"
out=$(sh "$cover/tools/sweep-cover.sh" --root "$cover" 2>&1) && got=0 || got=$?
check 'a plan row nobody wrote an entry for is named' \
    1 "$got" "the plan names a mutation for $one and the inventory has no entry" "$out"

check 'an entry filed under a row that names no mutation is named too' \
    1 "$got" "files a control under $two, which names no mutation" "$out"

# A stage past the presence window is built row by row, and the first entry
# written for one of its rows arrives before the rest of the stage exists. The
# stray check reads every stage so that entry is accepted; the tally still
# counts only the rows presence is owed for.
covered "$one" "$three" "$four" "$floor" "$ceiling"
out=$(sh "$cover/tools/sweep-cover.sh" --root "$cover" 2>&1) && got=0 || got=$?
check 'an entry for a row outside the presence window is not a stray' \
    0 "$got" "4 plan rows name a mutation" "$out"

# Stages 0 to 2 were once refused an entry, because the inventory counted them
# by hand. Nothing counts them by hand now, so a row there is owed an entry
# like any other built stage, and one missing is named.
covered "$one" "$floor" "$ceiling"
out=$(sh "$cover/tools/sweep-cover.sh" --root "$cover" 2>&1) && got=0 || got=$?
check 'a row before Stage 3 is owed an entry like any other' \
    1 "$got" "the plan names a mutation for $four and the inventory has no entry" "$out"

# The window is Stage 0 to Stage 8 at both ends. Raise the floor to Stage 1 or
# drop the ceiling below Stage 8 and one of these rows stops being owed an
# entry, so its missing entry passes unnoticed.
covered "$one" "$four" "$ceiling"
out=$(sh "$cover/tools/sweep-cover.sh" --root "$cover" 2>&1) && got=0 || got=$?
check 'a Stage 0 row is owed an entry' \
    1 "$got" "the plan names a mutation for $floor and the inventory has no entry" "$out"

covered "$one" "$four" "$floor"
out=$(sh "$cover/tools/sweep-cover.sh" --root "$cover" 2>&1) && got=0 || got=$?
check 'a Stage 8 row is owed an entry' \
    1 "$got" "the plan names a mutation for $ceiling and the inventory has no entry" "$out"

# The stray check reads every stage from Stage 0 up, and "every" has to mean
# it however far the plan grows: a numeric ceiling would quietly start calling
# a real entry a stray the day a stage passed it. A row above the first stage
# heading belongs to no stage at all, and is read by neither direction rather
# than falling into the lowest one.
cat > "$cover/docs/PLAN-SHIPPED.md" <<EOF
| $four | a row filed under no stage | it holds; mutation: break it | — | developer-only |

## Stage 100 — a fixture stage far ahead of any ceiling

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| $three | a far later fixture | it holds; mutation: break it | — | developer-only |
EOF

covered "$three"
out=$(sh "$cover/tools/sweep-cover.sh" --root "$cover" 2>&1) && got=0 || got=$?
check 'an entry for a row in a stage past any ceiling is not a stray' \
    0 "$got" "0 plan rows name a mutation" "$out"

covered "$four"
out=$(sh "$cover/tools/sweep-cover.sh" --root "$cover" 2>&1) && got=0 || got=$?
check 'an entry for a row filed under no stage is a stray' \
    1 "$got" "files a control under $four" "$out"

# --- what the plan being built owes, and when -------------------------------
#
# The built window stood in for "already built" only because this gate was
# written at the end of the plan it was written for. A plan still being built
# writes its rows before their code, so the current plan owes an entry for a
# row it marks done and for no other — and an entry written under a row that
# is not marked done yet is still not a stray, because a control lands in the
# same pull request as the row's code and the mark goes on in it too.
rm -f "$cover/docs/PLAN-SHIPPED.md"
cat > "$cover/docs/PLAN.md" <<EOF
## Stage 0 — the plan being built

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| $one | a row that landed. **Done** | it holds; mutation: break it and it does not | — | developer-only |
| $two | a row nobody has started | it holds; mutation: break it and it does not | — | developer-only |
EOF

covered "$one"
out=$(sh "$cover/tools/sweep-cover.sh" --root "$cover" 2>&1) && got=0 || got=$?
check 'a row not marked done is owed no entry yet' \
    0 "$got" "1 plan rows name a mutation" "$out"

covered "$two"
out=$(sh "$cover/tools/sweep-cover.sh" --root "$cover" 2>&1) && got=0 || got=$?
check 'a row marked done is owed its entry' \
    1 "$got" "the plan names a mutation for $one and the inventory has no entry" "$out"

covered "$one" "$two"
out=$(sh "$cover/tools/sweep-cover.sh" --root "$cover" 2>&1) && got=0 || got=$?
check 'an entry under a row not yet marked done is not a stray' \
    0 "$got" "1 plan rows name a mutation" "$out"

# The mark is typed by hand, so the count of rows still waiting for one is the
# only thing that would look odd if a row landed unmarked. A run that stopped
# printing it would take that with it.
check 'the run says how many rows are waiting to be marked done' \
    0 "$got" "1 more rows in the current plan name a mutation and are not marked done" "$out"

# The mark is read from the task cell, not from anywhere in the row. Read from
# the whole row, a done-condition opening "**Done when**" would mark itself.
cat > "$cover/docs/PLAN.md" <<EOF
## Stage 0 — the plan being built

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| $one | a row nobody has started | **Done when** it holds; mutation: break it | — | developer-only |
EOF

covered
out=$(sh "$cover/tools/sweep-cover.sh" --root "$cover" 2>&1) && got=0 || got=$?
check 'a done-condition saying Done does not mark the row' \
    0 "$got" "0 plan rows name a mutation" "$out"

# Two retired plans, each read from its own first line. The second opens with a
# row above any heading, which belongs to no stage and is read by neither
# direction — so an entry for it is a stray. Read as one long document instead,
# that row would inherit the first plan's Stage 100 and the stray would pass.
cat > "$cover/docs/PLAN-FIRST.md" <<EOF
## Stage 100 — a retired plan that ran long

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| $three | a far later fixture | it holds; mutation: break it | — | developer-only |
EOF
cat > "$cover/docs/PLAN-SECOND.md" <<EOF
| $four | a row above this plan's first heading | it holds; mutation: break it | — | developer-only |

## Stage 0 — another retired plan, numbering from its own zero

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| $floor | an early fixture | it holds; mutation: break it | — | developer-only |
EOF

covered "$floor" "$four"
out=$(sh "$cover/tools/sweep-cover.sh" --root "$cover" 2>&1) && got=0 || got=$?
check 'each plan is read from its own first line, not as one document' \
    1 "$got" "files a control under $four" "$out"

covered "$three"
out=$(sh "$cover/tools/sweep-cover.sh" --root "$cover" 2>&1) && got=0 || got=$?
check 'a second retired plan owes its Stage 0 entry like any other' \
    1 "$got" "the plan names a mutation for $floor and the inventory has no entry" "$out"

# An ID two plans both carry would file one row's controls under the other's
# entries and read as covered.
cat > "$cover/docs/PLAN-SECOND.md" <<EOF
## Stage 0 — another retired plan, reusing an ID

| id | task | done when | needs | runs on |
|---|---|---|---|---|
| $three | a row reusing an ID | it holds; mutation: break it | — | developer-only |
EOF

covered "$three"
out=$(sh "$cover/tools/sweep-cover.sh" --root "$cover" 2>&1) && got=0 || got=$?
check 'an ID two plans both carry is refused' \
    1 "$got" "$three is carried by more than one row" "$out"

rm -f "$cover/docs/PLAN-FIRST.md" "$cover/docs/PLAN-SECOND.md"


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
