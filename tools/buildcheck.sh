#!/bin/sh
# The build gate: the Rust workspace and the Lua harness.
#
# `mise run check` is what CI gates a pull request on and what CLAUDE.md tells
# a contributor to run. Until the workspace and the harness existed, each
# block here skipped, loudly, when its subject was absent; both exist now, so
# both run every time and the script is just the build gate.

set -e

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

# --- Rust -------------------------------------------------------------------

echo "cargo: fmt, clippy, build, test"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace

# The roll call. Cargo pulls a path dependency into the workspace by itself,
# so a crate dropped from the member list still builds, and still shows in
# cargo's own view of the workspace; the four commands above stay green. So
# the manifest is read as written. Both crates are named on purpose: the
# library so that a Rust consumer other than the binary can link it, the
# binary because it is what a user downloads.
members=$(grep -E '^members[[:space:]]*=' Cargo.toml)
for crate in dcs-eval dcs-mcp; do
    case "$members" in
        *"\"crates/$crate\""*) ;;
        *)
            echo "workspace: crates/$crate is not in Cargo.toml's members" >&2
            exit 1
            ;;
    esac
done
if [ ! -f target/debug/dcs-mcp.exe ]; then
    echo "build: target/debug/dcs-mcp.exe was not produced" >&2
    exit 1
fi

# --- The Lua harness --------------------------------------------------------

# Every registered suite, not a directory of them: a name that matches no
# suite is "nothing ran" and exits 2, so asking for the executor's suites
# before the first one exists would redden the gate rather than tighten it.
# The runner gets stricter as tools/harness/suites.lua grows.
echo "harness: every suite"
lua5.1 tools/harness.lua
