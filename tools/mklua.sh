#!/bin/sh
# Build the reference interpreter, Lua 5.1.5 PUC-Rio, into .lua/bin/lua5.1.exe.
#
# mise's lua plugin runs the tarball's makefile and Windows has no make, so
# this script compiles the same sources with the MSVC toolchain instead. Same
# tarball, same version, same checksum — the interpreter is not a third
# party's build of it.
#
# The tarball's checksum is pinned. A mismatch stops the build rather than
# compiling whatever was served, because every Lua figure this project
# records is a figure about this interpreter.
#
# Run it as `mise run lua-build`.

set -e

VERSION=5.1.5
SHA256=2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333
URL="https://www.lua.org/ftp/lua-$VERSION.tar.gz"

# pwd -P rather than pwd: the shell's PWD can arrive holding a Windows path
# (mise sets it that way), and the builtin would echo it back.
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
out="$root/.lua"
exe="$out/bin/lua5.1.exe"
tarname="lua-$VERSION.tar.gz"

if [ "$1" != "--force" ] && [ -x "$exe" ]; then
    echo "already built: $exe"
    exit 0
fi

mkdir -p "$out/bin"

tarball="$out/$tarname"
if [ ! -f "$tarball" ]; then
    echo "fetching $URL"
    curl -fsSL -o "$tarball" "$URL"
fi

got=$(sha256sum "$tarball" | cut -d' ' -f1)
if [ "$got" != "$SHA256" ]; then
    echo "checksum mismatch for lua-$VERSION.tar.gz" >&2
    echo "  expected $SHA256" >&2
    echo "  got      $got" >&2
    rm -f "$tarball"
    exit 1
fi

rm -rf "$out/src"
mkdir -p "$out/src"

# Extracted from inside the directory, naming the tarball relatively. GNU tar
# reads an argument whose first colon precedes the first slash as host:path, so
# an absolute Windows path makes it try to reach a machine called `D`:
#
#     tar (child): Cannot connect to D: resolve failed
#
# A relative path cannot carry a drive letter, which settles it whatever shape
# the caller's paths arrive in.
( cd "$out/src" && tar -xzf "../$tarname" --strip-components=1 )

# lua.c holds the interpreter's main. luac.c holds the compiler's, and
# print.c is only ever linked into luac, so neither belongs in this link.
rm -f "$out/src/src/luac.c" "$out/src/src/print.c"

# vswhere reports every Visual Studio and Build Tools install. The latest one
# carrying the C++ toolset is the one that can compile this.
vswhere="/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
if [ ! -x "$vswhere" ]; then
    echo "vswhere.exe not found. Install the Visual Studio Build Tools with" >&2
    echo "the 'Desktop development with C++' workload." >&2
    exit 1
fi

vsdir=$("$vswhere" -latest -products '*' \
    -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 \
    -property installationPath | tr -d '\r')
[ -n "$vsdir" ] || { echo "no Visual Studio install carries the C++ toolset" >&2; exit 1; }

# The compile runs from a batch file rather than an inline cmd.exe string:
# vcvars64.bat lives under a path with spaces, and the quoting that survives
# both this shell and cmd.exe is not worth writing twice.
bat="$out/build.bat"
{
    echo "@echo off"
    echo "call \"$vsdir\VC\Auxiliary\Build\vcvars64.bat\" >nul || exit /b 1"
    echo "cd /d \"$(cygpath -w "$out/src/src")\" || exit /b 1"
    echo "cl /nologo /MD /O2 /W3 /D_CRT_SECURE_NO_DEPRECATE /c *.c || exit /b 1"
    echo "link /nologo /out:\"$(cygpath -w "$exe")\" *.obj || exit /b 1"
} > "$bat"

echo "compiling Lua $VERSION with MSVC"
cmd.exe //c "$(cygpath -w "$bat")" || { echo "the MSVC build failed" >&2; exit 1; }

[ -x "$exe" ] || { echo "the build produced no $exe" >&2; exit 1; }
"$exe" -v
echo "built: $exe"
