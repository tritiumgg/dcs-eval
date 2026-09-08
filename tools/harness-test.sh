#!/bin/sh
# Drive tools/harness.lua against a sandbox of suites and assert what it says.
#
# The runner's promise is its exit codes: 0 when every check held, 1 when one
# did not or a suite broke, 2 when nothing ran. A runner that stops exiting 2
# for an empty suite turns a test file with its assertions commented out into
# a pass, and nothing else in the repository would notice. So each case below
# is a suite written into a sandbox next to a copy of the runner, and what the
# runner prints and exits with is asserted.
#
# Needs the reference interpreter on PATH. `mise run check` runs this after
# tools/check-lua.sh has proven what that interpreter is, and so does CI.

set -e

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

# Every case below must be reached; the count is asserted rather than
# reported. Raise this when a case is added.
CASES=10

sandbox=$(mktemp -d)
trap 'rm -rf "$sandbox"' EXIT INT TERM

mkdir -p "$sandbox/tools/harness/dir"
cp tools/harness.lua "$sandbox/tools/harness.lua"
runner="$sandbox/tools/harness.lua"

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

# Run the sandboxed runner over $4.. and assert exit $2 and output $3.
case_() {
    name=$1; want=$2; want_says=$3; shift 3
    out=$(lua5.1 "$runner" "$@" 2>&1) && got=0 || got=$?
    check "$name" "$want" "$got" "$want_says" "$out"
}

# --- the suites the cases drive ---------------------------------------------

cat > "$sandbox/tools/harness/suites.lua" <<'EOF'
return { "one", "two", "empty", "fails", "raises", "global", "noexec", "dir/a", "dir/b" }
EOF

cat > "$sandbox/tools/harness/one.lua" <<'EOF'
local t = ...
t.check(true, "holds")
EOF

cat > "$sandbox/tools/harness/two.lua" <<'EOF'
local t = ...
t.check(1 < 2, "one is below two")
t.eq(1 + 1, 2, "one and one")
EOF

# The mutation the plan names: a test with no assertions.
cat > "$sandbox/tools/harness/empty.lua" <<'EOF'
local t = ...
local _ = t
EOF

cat > "$sandbox/tools/harness/fails.lua" <<'EOF'
local t = ...
t.check(true, "holds")
t.eq(1, 2, "one is two")
t.check(true, "never reached")
EOF

cat > "$sandbox/tools/harness/raises.lua" <<'EOF'
local t = ...
t.check(true, "holds")
error("boom")
EOF

# The sealed globals: a suite that writes a name it never declared.
cat > "$sandbox/tools/harness/global.lua" <<'EOF'
local t = ...
t.check(true, "holds")
undeclared = 1
EOF

# The sandbox has no executor file, so loading it must raise rather than
# hand the suite nil to go on counting checks about.
cat > "$sandbox/tools/harness/noexec.lua" <<'EOF'
local t = ...
t.raises(function() t.load_executor({}) end, "harness: cannot load the executor",
  "a missing executor is a raise, not nil")
EOF

for s in a b; do
    cat > "$sandbox/tools/harness/dir/$s.lua" <<'EOF'
local t = ...
t.check(true, "holds")
EOF
done

# --- what the runner must say -----------------------------------------------

case_ 'one check counts as one'        0 'one: 1 check'   one
case_ 'two checks count as two'        0 'two: 2 checks'  two
case_ 'a directory runs every suite in it' 0 'harness: 2 suites, 2 checks' dir
case_ 'a missing executor raises'      0 'noexec: 1 check' noexec

# --- and what it must refuse ------------------------------------------------

case_ 'no checks is nothing ran'       2 'FAIL  empty: no checks ran' empty
case_ 'a name matching nothing'        2 'harness: no suite named nosuch' nosuch
case_ 'a failed check names its line and what held before it' \
                                       1 'after 1 check passed' fails
case_ 'a suite that breaks is a failure, with the message' \
                                       1 'boom' raises
case_ 'an undeclared global is a raise' 1 "assignment to undeclared global 'undeclared'" global

# --- the repository's own gate ----------------------------------------------

out=$(lua5.1 tools/harness.lua selftest 2>&1) && got=0 || got=$?
if [ "$got" = 0 ] && [ "$out" = 'selftest: 1 check' ]; then
    pass=$((pass + 1))
else
    fail=$((fail + 1))
    printf 'FAIL  the selftest prints exactly its count\n  wanted exit 0 and "selftest: 1 check"\n  got exit %s and: %s\n' "$got" "$out" >&2
fi

ran=$((pass + fail))
if [ "$ran" -ne "$CASES" ]; then
    printf '\nharness-test: %d cases ran, %d expected. A case was lost.\n' \
        "$ran" "$CASES" >&2
    exit 1
fi
if [ "$fail" -ne 0 ]; then
    printf '\nharness-test: %d passed, %d failed\n' "$pass" "$fail" >&2
    exit 1
fi
printf 'harness-test: %d checks, all passed\n' "$pass"
