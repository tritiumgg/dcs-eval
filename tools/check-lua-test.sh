#!/bin/sh
# Drive tools/check-lua.sh against a fake interpreter and assert what it says.
#
# The guard is the only thing standing between this project and a green run
# under somebody's 5.4, and a guard nobody has watched refuse is not evidence
# that it would. So each case below puts a `lua5.1` on PATH that prints one
# banner, and asserts the exit code and the sentence the guard prints back.
#
# The banners are fakes rather than real installs on purpose: the cases must
# run wherever this repository is checked out, including a CI job with no
# toolchain at all. That the *real* interpreter satisfies the guard is a
# separate claim, and `mise run lua-check` is the one that makes it.
#
# `mise run check` runs this, and so does CI.

set -e

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

# Mixed-form Windows paths do not survive being put on PATH for a child sh —
# `command -v` will not find anything under one — so the sandbox is named in
# the POSIX form throughout. mktemp -d already returns that form.
sandbox=$(mktemp -d)
trap 'rm -rf "$sandbox"' EXIT INT TERM

pass=0
fail=0

# Run the guard with $1 as the whole of the fake interpreter's -v output, then
# assert its exit code is $2 and that its combined output matches $3.
case_() {
    banner=$1; want=$2; want_says=$3; name=$4

    rm -rf "$sandbox/bin"
    mkdir -p "$sandbox/bin"
    # Real 5.1 prints -v to stderr and real 5.4 prints it to stdout; the guard
    # folds both, so the fake picks one and the difference stays untested here.
    printf '#!/bin/sh\necho %s\n' "'$banner'" > "$sandbox/bin/lua5.1"
    chmod +x "$sandbox/bin/lua5.1"

    out=$(PATH="$sandbox/bin:$PATH" sh tools/check-lua.sh 2>&1) && got=0 || got=$?
    check "$name" "$want" "$got" "$want_says" "$out"
}

check() {
    name=$1; want=$2; got=$3; want_says=$4; out=$5

    if [ "$got" != "$want" ]; then
        fail=$((fail + 1))
        printf 'FAIL  %s\n  wanted exit %s, got %s\n' "$name" "$want" "$got" >&2
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

# --- what the guard must admit ---------------------------------------------
case_ 'Lua 5.1.5  Copyright (C) 1994-2012 Lua.org, PUC-Rio' \
      0 'lua5.1 5.1.5' 'the reference interpreter'

# --- what it must refuse, and the version it must name doing it ------------
#
# 5.4 is the mutation the plan names: it is what a developer machine has, and
# it is the one whose green run would say the least. It has no setfenv, and
# its integer division changes what %.14g prints.
case_ 'Lua 5.4.6  Copyright (C) 1994-2023 Lua.org, PUC-Rio' \
      1 'found:  Lua 5.4.6' '5.4 refused, by version'
case_ 'LuaJIT 2.1.0-beta3 -- Copyright (C) 2005-2017 Mike Pall.' \
      1 'found:  LuaJIT 2.1.0-beta3' 'LuaJIT refused, by version'
# The right series is not the right interpreter. 5.1.4 would run most of this
# project and is still not what any figure here was measured under.
case_ 'Lua 5.1.4  Copyright (C) 1994-2008 Lua.org, PUC-Rio' \
      1 'found:  Lua 5.1.4' '5.1.4 refused, by version'
# A 5.1.5 banner that is not PUC-Rio's is somebody else's fork of it.
case_ 'Lua 5.1.5  Copyright (C) 1994-2012 Lua.org' \
      1 'found:  Lua 5.1.5' 'a 5.1.5 that is not PUC-Rio refused'

# --- and the case that is not a wrong version but no version ---------------
#
# PATH is cut back to wherever the coreutils this script already ran live, so
# the guard finds no interpreter while `command`, `head` and `cat` still work.
utils=$(dirname "$(command -v head)")
if PATH="$utils" command -v lua5.1 >/dev/null 2>&1; then
    printf 'skipped: the no-interpreter case — %s has its own lua5.1\n' "$utils"
else
    out=$(PATH="$utils" sh tools/check-lua.sh 2>&1) && got=0 || got=$?
    check 'no interpreter names the way out' 1 "$got" 'mise run lua-build' "$out"
fi

if [ "$fail" -ne 0 ]; then
    printf '\ncheck-lua: %d passed, %d failed\n' "$pass" "$fail" >&2
    exit 1
fi
printf 'check-lua: %d checks, all passed\n' "$pass"
