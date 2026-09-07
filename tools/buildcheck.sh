#!/bin/sh
# The gates that can only run once the code they check exists.
#
# `mise run check` is what CI gates a pull request on and what CLAUDE.md tells
# a contributor to run, so it has to be green on a fresh checkout of a
# repository that is still mostly plan. It also has to get stricter on its own
# as the build lands, because a gate somebody has to remember to switch on is
# a gate that stays off.
#
# So each block below runs when its subject is present and says what it
# skipped when it is not. The Cargo workspace and the Lua harness are the
# plan's first two build tasks; neither needs an edit here to take effect.
# When the last block stops skipping, this script is just the build gate.
#
# A skip is not a pass. It prints why, and the plan says who owes it.

set -e

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

skipped=0

skip() {
    printf 'skipped: %s\n' "$1"
    skipped=$((skipped + 1))
}

# --- Rust -------------------------------------------------------------------

if [ -f Cargo.toml ]; then
    echo "cargo: fmt, clippy, build, test"
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo build --workspace --all-targets
    cargo test --workspace
else
    skip "the Rust gates — no Cargo.toml, so the workspace is not built yet"
fi

# --- The Lua harness --------------------------------------------------------

if [ -f tools/harness.lua ]; then
    echo "harness: every executor suite"
    lua5.1 tools/harness.lua executor
else
    skip "the Lua harness — tools/harness.lua is not built yet"
fi

if [ "$skipped" -gt 0 ]; then
    printf '\n%d gate(s) skipped because what they check does not exist yet.\n' "$skipped"
    printf 'They start running by themselves when it does.\n'
fi
