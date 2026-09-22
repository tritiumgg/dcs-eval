#!/bin/sh
# PreToolUse guard: refuse an edit to a frozen document or to .gitattributes.
#
# The specifications are frozen. They are the starting point, they are not
# maintained, and the build drifts from them by design. Where the build goes
# somewhere they did not anticipate, the record of that is a decision record,
# not an edit. Everything under docs/specs/ is frozen, and nothing else is.
#
# Frozen is never edited, which is not the same as never written, and a
# document is frozen the moment it lands on main. A capability the shipped
# documents never considered is specified before it is built, the way those two
# were: written on a branch, reviewed and corrected there like any other work,
# and refused every write from the merge onwards. So the question this asks of
# a path under docs/specs/ is not whether the file exists — it is whether main
# already carries it. Where that cannot be answered, the answer is no.
#
# A path is resolved before it is judged. A spelling that walks out of the
# directory and back in, a UNC or extended-length prefix, or anything else this
# cannot resolve to a real place is refused rather than puzzled over: a guard
# that lets an unfamiliar spelling through is not a guard.
#
# .gitattributes disables line-ending conversion. Without it the embedded
# DcsEvalExecutor.lua's hash and the installer's hash gate both break against
# a file nobody edited.
#
# Exit 2 blocks the tool call and hands stderr to the model as the reason.

. "$(dirname -- "$0")/payload.sh"

# Write and Edit carry file_path; NotebookEdit carries notebook_path, and the
# settings matcher names it, so both are read here or the matcher is a claim
# this does not keep.
path=$(parse file_path)
[ -n "$path" ] || path=$(parse notebook_path)
[ -n "$path" ] || exit 0

norm=$(normalize_path "$path")
root=$(normalize_path "$(project_root)")

# Resolve to a real directory plus a name. cd -P follows what the filesystem
# says rather than what the string claims, which is what turns
# docs/specs/anything/../mcp.md back into the frozen file it points at.
# Unresolvable is a refusal, not a pass.
resolved=""
case "$norm" in
    /*|[A-Za-z]:/*) abs=$norm ;;
    *) abs="$root/$norm" ;;
esac
dir=$(dirname -- "$abs")
base=$(basename -- "$abs")
# Windows drops a trailing dot or space from a name, so `mcp.md.` opens
# `mcp.md`. The name is compared with those gone.
base=$(printf '%s' "$base" | sed 's/[. ]*$//')
if real=$(CDPATH= cd -- "$dir" 2>/dev/null && pwd -P); then
    resolved=$(normalize_path "$real/$base")
fi

# Either spelling puts this under docs/specs/: the one the tool sent, or the
# place it resolves to. A path that walks out and back in is caught by the
# first; one that arrives through a link, by the second. Case is folded for
# both, because NTFS opens DOCS/SPECS/MCP.MD as the file this is guarding
# while every string test here would call it a different path.
fold() { printf '%s' "$1" | tr 'ABCDEFGHIJKLMNOPQRSTUVWXYZ' 'abcdefghijklmnopqrstuvwxyz'; }
under_specs=no
case "$(fold "$norm")" in */docs/specs/*|docs/specs/*) under_specs=yes ;; esac
case "$(fold "$resolved")" in */docs/specs/*) under_specs=yes ;; esac

why=""
case "$under_specs" in
    yes)
        if [ -z "$resolved" ]; then
            why="a path under docs/specs/ that does not resolve to any real place"
        else
            # Frozen means landed. A document main does not carry yet is being
            # written; one it carries is never edited again. Where there is no
            # main to ask — a checkout without it — everything under docs/specs/
            # is treated as frozen.
            # The name main would carry, cut at the directory itself rather
            # than by stripping the checkout's path: the same place can be
            # spelled long or 8.3-short, and a prefix that does not match
            # would leave an absolute path here that no tree entry answers to.
            rel=$(printf '%s' "$resolved" | awk '
                { i = index(tolower($0), "/docs/specs/")
                  print i ? substr($0, i + 1) : $0 }
            ')
            # A local main where there is one, the remote's where a checkout
            # has fetched no branch of its own — which is how CI arrives.
            trunk=""
            for ref in main origin/main; do
                if git -C "$root" rev-parse --verify --quiet "$ref" >/dev/null 2>&1; then
                    trunk=$ref
                    break
                fi
            done
            if [ -n "$trunk" ]; then
                # Asked of the tree rather than by path, and matched without
                # case: git is case-sensitive and the filesystem under it is
                # not, so `main:DOCS/SPECS/MCP.MD` is a miss on a file that
                # opens perfectly well.
                git -C "$root" ls-tree -r --name-only "$trunk" -- docs/specs |
                    awk -v want="$(fold "$rel")" '
                        tolower($0) == want { found = 1 }
                        END { exit found ? 0 : 1 }
                    ' &&
                    why="a frozen specification: $trunk carries it"
            else
                why="a frozen specification (no main here to ask, so every one of them is)"
            fi
        fi ;;
esac

case "$norm" in
    */.gitattributes|.gitattributes)
        why=".gitattributes, which keeps every hash this project ships honest" ;;
esac

[ -n "$why" ] || exit 0

cat >&2 <<EOF
Blocked: a write to $norm, which is $why.

The specifications are frozen and are not brought up to date. Where the build
needs to go somewhere they did not anticipate, write a decision record: copy
docs/decisions/TEMPLATE.md and number it next.
docs/conventions/decision-records.md says how.

A capability none of them considered gets a specification of its own, written
before it is built. It is written on a branch and corrected there; from the
moment main carries it, it is refused every write like these.

Leave .gitattributes alone for the same reason: this project hashes the files
it ships, and line-ending conversion rewrites those bytes on checkout.
EOF
exit 2
