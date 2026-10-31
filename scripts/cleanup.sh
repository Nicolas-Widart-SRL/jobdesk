#!/usr/bin/env bash
# Cleans up after merged work: agent worktrees and stale branches.
# A branch is merged when the base already holds all its changes, even after a rebase
# or a squash merge. The default base is origin/main. Pass a change's branch as well
# to clean up the steps already merged into it.
#
#   scripts/cleanup.sh                        after the user merges a pull request
#   scripts/cleanup.sh feat/standing-orders   after steps merge into feat/standing-orders
#   DRY_RUN=1 scripts/cleanup.sh              print what would go, remove nothing
#
# It keeps a worktree with uncommitted changes, a locked worktree, the checkout
# it runs from, and every branch not yet merged.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
here=$(pwd)
root=$(cd "$(git rev-parse --git-common-dir)/.." && pwd)

git fetch --quiet --prune origin
bases=("$@" origin/main)
run() { if [ -n "${DRY_RUN:-}" ]; then echo "  would run: $*"; else "$@"; fi; }

# True when one of the bases already holds every change of the commit.
merged() {
  local base tree
  for base in "${bases[@]}"; do
    git rev-parse -q --verify "$base" >/dev/null || continue
    if ! git cherry "$base" "$1" | grep -q '^+'; then return 0; fi
    tree=$(git merge-tree --write-tree "$base" "$1" 2>/dev/null | head -1) || continue
    [ "$tree" = "$(git rev-parse "$base^{tree}")" ] && return 0
  done
  return 1
}

kept=0
while IFS=$'\t' read -r path head branch locked; do
  case "$path" in "$root/.claude/worktrees/"*) ;; *) continue ;; esac
  [ "$path" = "$here" ] && continue
  name=${branch:-detached $head}
  if [ -n "$locked" ]; then echo "keep $path: locked"; kept=$((kept + 1)); continue; fi
  if ! merged "$head"; then echo "keep $path: $name is not merged"; kept=$((kept + 1)); continue; fi
  if [ -n "$(git -C "$path" status --porcelain 2>/dev/null)" ]; then
    echo "keep $path: uncommitted changes"; kept=$((kept + 1)); continue
  fi
  echo "remove worktree $path ($name)"
  run git worktree remove "$path"
done < <(git worktree list --porcelain | awk '
  /^worktree / { path = substr($0, 10); head = ""; branch = ""; locked = "" }
  /^HEAD / { head = substr($0, 6, 7) }
  /^branch / { branch = substr($0, 19) }
  /^locked/ { locked = "yes" }
  /^$/ { print path "\t" head "\t" branch "\t" locked; path = "" }
  END { if (path != "") print path "\t" head "\t" branch "\t" locked }')

git worktree prune
in_use=$(git worktree list --porcelain | awk '/^branch / { print substr($0, 19) }')
for branch in $(git for-each-ref --format='%(refname:short)' refs/heads); do
  case "$branch" in main) continue ;; esac
  for base in "$@"; do [ "$branch" = "$base" ] && continue 2; done
  grep -qxF "$branch" <<<"$in_use" && continue
  if merged "$branch"; then
    echo "delete branch $branch"
    run git branch --quiet -D "$branch"
  fi
done

if [ "$(git -C "$root" symbolic-ref --short -q HEAD)" = main ] && [ -z "$(git -C "$root" status --porcelain)" ]; then
  run git -C "$root" merge --quiet --ff-only origin/main && echo "main is at $(git -C "$root" log -1 --format='%h %s')"
fi
echo "done, $kept worktrees kept"
