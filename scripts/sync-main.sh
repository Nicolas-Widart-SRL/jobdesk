#!/usr/bin/env bash
# Brings the main checkout up to date with origin/main, fast-forward only.
# Run it from any checkout, an agent worktree included: it always works on the main checkout.
# It informs and never fails the session, so it prints exactly one line and exits 0:
#
#   main: not a git checkout                     run outside a repository
#   main: not on main (on <branch>)
#   main: has uncommitted changes                untracked files do not count
#   main: has untracked files under openspec/
#   main: could not fetch origin
#   main: <n> local commits not on origin/main
#   main: up to date
#   main: fast-forwarded to <short sha>
#   main: could not fast-forward
#
# The local checks (branch, uncommitted changes, untracked openspec/ files) run before the fetch, so they show even offline.
# It fast-forwards only when main is checked out, clean, free of untracked openspec/ files, behind origin/main and not ahead.
# It never resets, checks out or stashes.
set -euo pipefail

say() { echo "main: $1"; exit 0; }

top=$(git rev-parse --show-toplevel 2>/dev/null) || say "not a git checkout"
common=$(cd "$top" 2>/dev/null && git rev-parse --git-common-dir 2>/dev/null) || say "not a git checkout"
root=$(cd "$top" 2>/dev/null && cd "$common/.." 2>/dev/null && pwd) || say "not a git checkout"
cd "$root" 2>/dev/null || say "not a git checkout"

branch=$(git branch --show-current 2>/dev/null || true)
[ "$branch" = main ] || say "not on main (on ${branch:-detached HEAD})"

# A failing git status counts as dirty.
if ! status=$(git status --porcelain --untracked-files=no 2>/dev/null) || [ -n "$status" ]; then
  say "has uncommitted changes"
fi

# Untracked files under openspec/ are a change's work that belongs on its branch, not on main.
if ! untracked=$(git status --porcelain --untracked-files=all -- openspec 2>/dev/null) || [ -n "$untracked" ]; then
  say "has untracked files under openspec/"
fi

git fetch --quiet origin 2>/dev/null || say "could not fetch origin"

ahead=$(git rev-list --count origin/main..main 2>/dev/null) || say "could not fast-forward"
behind=$(git rev-list --count main..origin/main 2>/dev/null) || say "could not fast-forward"
[ "$ahead" = 0 ] || say "$ahead local commits not on origin/main"
[ "$behind" != 0 ] || say "up to date"

git merge --quiet --ff-only origin/main >/dev/null 2>&1 || say "could not fast-forward"
say "fast-forwarded to $(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
