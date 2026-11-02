#!/usr/bin/env bash
# Brings the main checkout up to date with origin/main, fast-forward only.
# Run it from any checkout, an agent worktree included: it always works on the main checkout.
# It informs and never fails the session, so it prints exactly one line and exits 0:
#
#   main: not on main (on <branch>)
#   main: has uncommitted changes                untracked files do not count
#   main: <n> local commits not on origin/main
#   main: up to date
#   main: fast-forwarded to <short sha>
#   main: could not fast-forward
#   main: could not fetch origin
#
# It fast-forwards only when main is checked out, clean, behind origin/main and not ahead.
# It never resets, checks out or stashes.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
root=$(cd "$(git rev-parse --git-common-dir)/.." && pwd)
cd "$root"

if ! git fetch --quiet origin 2>/dev/null; then echo "main: could not fetch origin"; exit 0; fi

branch=$(git branch --show-current 2>/dev/null || true)
if [ "$branch" != main ]; then echo "main: not on main (on ${branch:-detached HEAD})"; exit 0; fi

# A failing git status counts as dirty.
if ! status=$(git status --porcelain --untracked-files=no 2>/dev/null) || [ -n "$status" ]; then
  echo "main: has uncommitted changes"; exit 0
fi

ahead=$(git rev-list --count origin/main..main 2>/dev/null || echo unknown)
behind=$(git rev-list --count main..origin/main 2>/dev/null || echo unknown)
if [ "$ahead" = unknown ] || [ "$behind" = unknown ]; then echo "main: could not fast-forward"; exit 0; fi
if [ "$ahead" != 0 ]; then echo "main: $ahead local commits not on origin/main"; exit 0; fi
if [ "$behind" = 0 ]; then echo "main: up to date"; exit 0; fi

if git merge --quiet --ff-only origin/main >/dev/null 2>&1; then
  echo "main: fast-forwarded to $(git rev-parse --short HEAD)"
else
  echo "main: could not fast-forward"
fi
