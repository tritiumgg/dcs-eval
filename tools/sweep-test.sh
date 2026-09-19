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
CASES=4

sandbox=$(mktemp -d)
trap 'rm -rf "$sandbox"' EXIT INT TERM

cp tools/sweep.sh "$sandbox/sweep.sh"
runner="$sandbox/sweep.sh"

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

# --- the sandbox ------------------------------------------------------------

mkdir -p "$sandbox/tree"
cat > "$sandbox/tree/alpha.txt" <<'EOF'
first line
the line that moves
last line
EOF

# Fixture IDs are deliberately unlike the real ones: tools/nospecrefs.sh reads
# every tracked file outside docs/ and refuses a plan task ID, so a fixture
# that spelled one would redden a check this file has nothing to do with.
cat > "$sandbox/inventory.md" <<'EOF'
### fixture/applies

- command: `sh -c 'exit 0'`
- reddens: `held`

```sweep-edit tree/alpha.txt
- the line that moves
+ the line that moved
```

### fixture/second

- command: `sh -c 'exit 0'`
- reddens: `held`

```sweep-edit tree/alpha.txt
- last line
+ final line
```

### other/elsewhere

- command: `sh -c 'exit 0'`
- reddens: `held`

```sweep-edit tree/alpha.txt
- first line
+ initial line
```

### out/not-reached

- out-of-scope: nothing here is built yet, so there is nothing to
  break and nothing to put back.
- controls: 5
EOF

# Run the sandboxed runner and assert exit $2 and output $3.
case_() {
    name=$1; want=$2; want_says=$3; shift 3
    out=$(sh "$runner" --inventory "$sandbox/inventory.md" --root "$sandbox/tree" "$@" 2>&1) \
        && got=0 || got=$?
    check "$name" "$want" "$got" "$want_says" "$out"
}

# --- the cases --------------------------------------------------------------

case_ 'the listing names a control and its command' \
    0 "control       fixture/applies" --list

case_ 'the listing names an out-of-scope group and its count' \
    0 "5 controls, nothing here is built yet, so there is nothing to break and nothing to put back." --list

out=$(sh "$runner" --inventory "$sandbox/inventory.md" --root "$sandbox/tree" \
    --list --only fixture/ 2>&1) && got=0 || got=$?
case "$out" in
    *other/elsewhere*) got=99 ;;
esac
check 'a trailing slash selects a group and nothing else' \
    0 "$got" "fixture/second" "$out"

out=$(sh "$runner" --inventory "$sandbox/nosuch.md" --list 2>&1) && got=0 || got=$?
check 'a missing inventory is refused, not assumed empty' \
    2 "$got" "no inventory at" "$out"

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
