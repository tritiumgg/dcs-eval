#!/bin/sh
# Prove the interpreter on PATH is Lua 5.1.5 PUC-Rio, and refuse anything else.
#
# A green harness run under a machine's Lua 5.4 says nothing about this
# project: 5.4 has no setfenv, integer division changes what %.14g prints, and
# LuaJIT's debug hooks count differently. Every Lua figure this project records
# is a figure about 5.1.5, so the version is checked rather than assumed.
#
# Prints `lua5.1 5.1.5` and exits 0. Anything else prints what it found and
# exits non-zero.

set -e

want="Lua 5.1.5"

if ! command -v lua5.1 >/dev/null 2>&1; then
    cat >&2 <<EOF
No lua5.1 on PATH.

Build the reference interpreter, then let mise put it on PATH:

    mise run lua-build
    mise install
EOF
    exit 1
fi

banner=$(lua5.1 -v 2>&1 | head -1)

case "$banner" in
    "$want "*"PUC-Rio"*)
        echo "lua5.1 5.1.5"
        exit 0 ;;
esac

cat >&2 <<EOF
The lua5.1 on PATH is not the reference interpreter.

    wanted: $want ... PUC-Rio
    found:  $banner
    at:     $(command -v lua5.1)

This project is proven under 5.1.5 PUC-Rio and under nothing else. A run
under 5.4 or LuaJIT passes for reasons that do not carry into DCS.
EOF
exit 1
