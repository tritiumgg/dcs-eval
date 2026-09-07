#!/bin/sh
# PreToolUse guard for Bash: refuse the commands the project forbids, and hand
# the ones the maintainer decides to the maintainer.
#
# Two outcomes. A refusal exits 2 with the reason on stderr, and the model
# reads it and picks the replacement. An ask prints a permission decision on
# stdout and exits 0, so the harness prompts the person at the keyboard
# whatever the permission mode. That split follows CLAUDE.md: some things are
# wrong here and are not done, and some are the maintainer's call.
#
# Refused:
#   sed -i, grep -P, readlink -f     CLAUDE.md, Portability
#   cargo, lua, rustc bare           run through mise exec -- or mise run
#   rustup, mise use rust            the toolchain lives in rust-toolchain.toml
#   git merge without --ff-only      history is linear
#   git push --force                 --force-with-lease, on a topic branch
#   a redirect, tee, cp, mv or rm    the specifications are frozen
#     aimed at docs/specs/ or .gitattributes
#   gh pr create without a           every pull request follows the template
#     Summary and a README heading
#
# Asked:
#   git push to main, git tag, gh release, gh pr merge, and a rewrite of
#   main's history. A tag is a release here.

. "$(dirname -- "$0")/payload.sh"

tool=$(parse tool_name)
[ "$tool" = "Bash" ] || exit 0

cmd=$(parse command)
[ -n "$cmd" ] || exit 0

# Every line of the command that is not a comment.
lines=$(printf '%s\n' "$cmd" | grep -v '^[[:space:]]*#' || true)

has() {
    printf '%s\n' "$lines" | grep -Eq -e "$1"
}

refuse() {
    printf 'Blocked: %s\n\n%s\n' "$1" "$2" >&2
    exit 2
}

ask() {
    reason=$(printf '%s' "$1" | tr '\n' ' ' | sed 's/["\\]/\\&/g')
    printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"ask","permissionDecisionReason":"%s"}}\n' "$reason"
    exit 0
}

# A word in command position: the start of a line or of a pipeline segment.
START='(^|[;&|(]|then|do|else)[[:space:]]*'
OPTS='(-[A-Za-z]*[[:space:]]+)*'

# --- refusals --------------------------------------------------------------

if has "${START}sed[[:space:]]+${OPTS}-[A-Za-z]*i"; then
    refuse "sed -i." \
"Use the Edit tool. It verifies the match before writing, and an in-place sed
that matches nothing reports success."
fi

if has "${START}grep[[:space:]]+${OPTS}-[A-Za-z]*P|--perl-regexp"; then
    refuse "grep -P." \
"Write the pattern for grep -E, or use rg. CLAUDE.md, Portability."
fi

if has "${START}readlink[[:space:]]+-[A-Za-z]*f"; then
    refuse "readlink -f." \
"Use CDPATH= cd -- \"\$(dirname -- \"\$f\")\" && pwd. CLAUDE.md, Portability."
fi

if has "${START}(cargo|lua5\\.1|lua|luac|rustc|rustfmt|clippy-driver)([[:space:]]|$)"; then
    refuse "a toolchain command outside mise." \
"A non-interactive shell does not pick up mise's PATH activation, so the tool
is the wrong version or missing. That matters most for Lua: this project is
proven under 5.1.5 PUC-Rio and a machine's other interpreter proves nothing.
Run it as mise exec -- <command>, or as one of the tasks in mise.toml:
mise run check, mise run test, mise run harness, mise run lua-check."
fi

if has "${START}rustup([[:space:]]|$)|${START}mise[[:space:]]+use[[:space:]]+rust"; then
    refuse "a direct change to the Rust toolchain." \
"The Rust version, its components and the Windows target live in
rust-toolchain.toml, and mise reads that file. mise use rust@ writes a second
version into mise.toml and mise stops reading the file. Edit channel in
rust-toolchain.toml, then run mise install."
fi

if has "git[[:space:]]+merge([[:space:]]|$)" && ! has "git[[:space:]]+merge.*--ff-only"; then
    refuse "git merge without --ff-only." \
"History is linear. Bring a branch up to date with git rebase origin/main;
main is fast-forwarded to it. If the fast-forward is refused, fix the branch."
fi

if has "git[[:space:]]+push.*([[:space:]]--force|[[:space:]]-f)([[:space:]]|$)" && ! has '--force-with-lease'; then
    refuse "git push --force." \
"Use --force-with-lease, on a topic branch that is yours. Never on main."
fi

FROZEN='(docs/specs/|\.gitattributes)'
if has ">>?[[:space:]]*[\"'\'']?[^[:space:]\"'\'']*${FROZEN}" \
    || has "[[:space:]]tee[[:space:]].*${FROZEN}" \
    || has "${START}(cp|mv|rm|truncate|install)[[:space:]].*${FROZEN}"; then
    refuse "a shell write to a frozen document or to .gitattributes." \
"The specifications are frozen and are not brought up to date. Where the
build needs to go somewhere they did not anticipate, write a decision record
from docs/decisions/TEMPLATE.md. .gitattributes keeps every hash this project
ships honest on a Windows checkout."
fi

if has "gh[[:space:]]+pr[[:space:]]+create"; then
    body=$cmd
    file=$(printf '%s\n' "$lines" | sed -E -n 's/.*(--body-file|-F)[= ]*([^ ]*).*/\2/p' | head -1)
    if [ -n "$file" ]; then
        # A path the shell would expand, such as "$S/pr.md", is not readable
        # here, and a body that cannot be read cannot be judged.
        file=$(printf '%s' "$file" | tr -d '"'"'")
        [ -r "$file" ] || exit 0
        body=$(cat "$file")
    fi
    missing=""
    # The two sections every body carries. The template's other sections are
    # deleted when empty, so their absence proves nothing.
    for h in Summary README; do
        # Anywhere, not at line start: the body may arrive on one line with
        # literal \n sequences, or through a heredoc with real newlines.
        printf '%s\n' "$body" | grep -q "## $h" || missing="$missing $h"
    done
    if [ -n "$missing" ]; then
        refuse "a pull request body without the template's headings:$missing." \
"Every pull request body follows .github/PULL_REQUEST_TEMPLATE.md. gh pr
create --body does not read the template, so write the body to its headings.
Summary and README are always present; delete any other section that is
empty."
    fi
fi

# --- asks ------------------------------------------------------------------

RULE="CLAUDE.md: work reaches main through a pull request and lands when the maintainer says so. Ask first before pushing main, before rewriting history that has been pushed, and before tagging. A tag is a release."

branch=""
current_branch() {
    [ -n "$branch" ] && return
    branch=$(cd "$(project_root)" 2>/dev/null && git symbolic-ref --short -q HEAD 2>/dev/null || true)
}

if has "git[[:space:]]+push([[:space:]]|$)"; then
    # The non-option words after push. One or none means the current branch.
    args=$(printf '%s\n' "$lines" | grep -E 'git[[:space:]]+push' | head -1 \
        | sed 's/.*git[[:space:]]*push//' | tr ' ' '\n' | grep -v '^-' | grep -v '^$' || true)
    n=$(printf '%s\n' "$args" | grep -c . || true)
    if printf '%s\n' "$args" | grep -Eq '^(\+?main|[^:]*:main)$'; then
        ask "This pushes main. $RULE"
    fi
    if [ "$n" -le 1 ]; then
        current_branch
        [ "$branch" = "main" ] && ask "This pushes the current branch, which is main. $RULE"
    fi
fi

if has "git[[:space:]]+tag[[:space:]]+[^[:space:]]" \
    && ! has "git[[:space:]]+tag[[:space:]]+(-l|-n|--list|--contains|--points-at|--sort|--merged|--no-merged)"; then
    ask "This creates or deletes a tag, and release.yml publishes on v*. $RULE"
fi

if has "gh[[:space:]]+release[[:space:]]+(create|delete|edit|upload)"; then
    ask "This changes a GitHub release. $RULE"
fi

if has "gh[[:space:]]+pr[[:space:]]+merge"; then
    ask "This merges a pull request. $RULE"
fi

if has "git[[:space:]]+(rebase([[:space:]]|$)|commit.*--amend|reset[[:space:]]+.*--hard|push.*--force-with-lease)" \
    && ! has "git[[:space:]]+rebase[[:space:]]+--(continue|abort|skip)"; then
    current_branch
    [ "$branch" = "main" ] && ask "This rewrites main. $RULE"
fi

exit 0
