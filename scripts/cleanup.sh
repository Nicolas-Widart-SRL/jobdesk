#!/usr/bin/env bash
# Cleans up after merged work: agent worktrees, stale branches and their plan files.
# A branch is merged when the base already holds all its changes, even after a rebase
# or a squash merge. The default base is origin/main. Pass a change's branch as well
# to clean up the steps already merged into it. A branch also counts as merged when
# GitHub shows a merged pull request from its current head commit. That covers a
# squash merge after main changed the same lines. Agent branches (worktree-*) skip
# that check. A change's branch that main already contains is new, not merged, so
# it stays until GitHub shows its pull request merged.
# A plan file in .claude/plans/ names its branch on its first line, as "# <branch>".
# The plan file goes when this run deletes its branch, or when the branch is gone
# and GitHub shows its pull request merged. Without gh, it stays.
# It never checks out, resets or pulls in the main checkout.
#
#   scripts/cleanup.sh                        after the user merges a pull request
#   scripts/cleanup.sh feat/standing-orders   after steps merge into feat/standing-orders
#   DRY_RUN=1 scripts/cleanup.sh              print what would go, remove nothing
#
# It keeps a worktree with uncommitted changes or ignored files other than build
# output, a locked worktree, the checkout it runs from, and every branch not yet merged.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
here=$(pwd)
root=$(cd "$(git rev-parse --git-common-dir)/.." && pwd)

# A dry run fetches without pruning, so it removes no remote-tracking ref.
if [ -n "${DRY_RUN:-}" ]; then git fetch --quiet origin; else git fetch --quiet --prune origin; fi
bases=("$@" origin/main)
run() { if [ -n "${DRY_RUN:-}" ]; then echo "  would run: $*"; else "$@"; fi; }

# True when one of the bases already holds every change of the commit:
# merging the commit into the base leaves the base's tree as it is.
merged() {
  local base tree
  for base in "${bases[@]}"; do
    git rev-parse -q --verify "$base" >/dev/null || continue
    tree=$(git merge-tree --write-tree "$base" "$1" 2>/dev/null | head -1) || continue
    [ "$tree" = "$(git rev-parse "$base^{tree}")" ] && return 0
  done
  return 1
}

# True when GitHub shows a merged pull request whose head is the branch's current commit.
# Matching the commit keeps a reused branch name safe.
pr_merged() {
  command -v gh >/dev/null || return 1
  gh pr list --head "$1" --state merged --json headRefOid --jq '.[].headRefOid' 2>/dev/null | grep -qxF "$(git rev-parse "$1")"
}

# True when a base already contains the branch's head: a new change's branch starts there.
started() {
  local base
  for base in "${bases[@]}"; do
    git rev-parse -q --verify "$base" >/dev/null || continue
    git merge-base --is-ancestor "$1" "$base" && return 0
  done
  return 1
}

# True when a branch can go. A step branch goes once its change's branch holds it.
# A change's branch needs commits of its own, or a merged pull request from its head.
done_with() {
  case $1 in
    worktree-*) merged "$1" ;;
    *) { ! started "$1" && merged "$1"; } || pr_merged "$1" ;;
  esac
}

kept=0
while IFS=$'\t' read -r path head branch locked; do
  case "$path" in "$root/.claude/worktrees/"*) ;; *) continue ;; esac
  [ "$path" = "$here" ] && continue
  name=${branch:-detached $head}
  if [ -n "$locked" ]; then echo "keep $path: locked"; kept=$((kept + 1)); continue; fi
  if ! merged "$head"; then echo "keep $path: $name is not merged"; kept=$((kept + 1)); continue; fi
  if ! status=$(git -C "$path" status --porcelain --ignored --untracked-files=normal 2>/dev/null); then
    echo "keep $path: git status failed"; kept=$((kept + 1)); continue
  fi
  # Read the whole list: an early-exiting grep could fail the pipeline and pass a dirty worktree.
  changes=$(grep -v '^!! ' <<<"$status" || true)
  if [ -n "$changes" ]; then echo "keep $path: uncommitted changes"; kept=$((kept + 1)); continue; fi
  # git worktree remove deletes ignored files. Only build output and test artefacts may go.
  local_files=$(grep '^!! ' <<<"$status" | grep -vE '^!! (target/|graft/|.*\.DS_Store$|.*\.(diff|new|old)\.png$)' || true)
  if [ -n "$local_files" ]; then echo "keep $path: ignored local files"; kept=$((kept + 1)); continue; fi
  echo "remove worktree $path ($name)"
  run git worktree remove "$path"
done < <(git worktree list --porcelain | awk '
  /^worktree / { path = substr($0, 10); head = ""; branch = ""; locked = "" }
  /^HEAD / { head = substr($0, 6, 7) }
  /^branch / { branch = substr($0, 19) }
  /^locked/ { locked = "yes" }
  /^$/ { print path "\t" head "\t" branch "\t" locked; path = "" }
  END { if (path != "") print path "\t" head "\t" branch "\t" locked }')

if [ -n "${DRY_RUN:-}" ]; then git worktree prune --dry-run --verbose; else git worktree prune; fi
in_use=$(git worktree list --porcelain | awk '/^branch / { print substr($0, 19) }')
deleted=""
for branch in $(git for-each-ref --format='%(refname:short)' refs/heads); do
  case "$branch" in main) continue ;; esac
  for base in "$@"; do [ "$branch" = "$base" ] && continue 2; done
  grep -qxF "$branch" <<<"$in_use" && continue
  if done_with "$branch"; then
    echo "delete branch $branch"
    run git branch --quiet -D "$branch"
    deleted+="$branch"$'\n'
  fi
done

# True when GitHub shows the branch's pull request merged. Without gh, nothing is.
pr_state_merged() {
  command -v gh >/dev/null || return 1
  [ "$(gh pr view "$1" --json state -q .state 2>/dev/null)" = MERGED ]
}

# A dry run deletes no branch, so trust the list of deleted branches over the refs.
for plan in "$root"/.claude/plans/*.md; do
  [ -f "$plan" ] || continue
  branch=$(head -1 "$plan")
  case "$branch" in "# "?*) branch=${branch#\# } ;; *) echo "keep $plan: no branch on its first line"; continue ;; esac
  if grep -qxF "$branch" <<<"$deleted"; then
    :
  elif git rev-parse -q --verify "refs/heads/$branch" >/dev/null || ! pr_state_merged "$branch"; then
    continue
  fi
  echo "delete plan $plan"
  run rm "$plan"
done

echo "done, $kept worktrees kept"
