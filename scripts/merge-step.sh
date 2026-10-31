#!/usr/bin/env bash
# Merges one built step into the change's branch, fast-forward only.
# It rebases the step onto the change's branch when needed, runs scripts/check.sh
# in the step's worktree, then moves the change's branch to the step's tip.
# It never touches main: the user merges the pull request.
#
#   scripts/merge-step.sh STEP CHANGE              for example worktree-agent-a1b2 feat/standing-orders
#   NO_CHECK=1 scripts/merge-step.sh STEP CHANGE   skip the checks, for a docs-only step
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

step=${1:?usage: scripts/merge-step.sh STEP CHANGE}
change=${2:?usage: scripts/merge-step.sh STEP CHANGE}
if [ "$change" = main ]; then
  echo "merge-step: the change's branch is never main. Open a pull request instead." >&2
  exit 1
fi
git rev-parse -q --verify "refs/heads/$step" >/dev/null || { echo "merge-step: no branch $step" >&2; exit 1; }
git rev-parse -q --verify "refs/heads/$change" >/dev/null || git branch "$change" main

# The worktree that has a branch checked out, if any.
worktree_of() {
  git worktree list --porcelain | awk -v ref="refs/heads/$1" '
    /^worktree / { path = substr($0, 10) }
    $0 == "branch " ref { print path }'
}
step_tree=$(worktree_of "$step")
change_tree=$(worktree_of "$change")

dirty() { [ -n "$step_tree" ] && [ -n "$(git -C "$step_tree" status --porcelain)" ]; }
if dirty; then
  echo "merge-step: $step has uncommitted changes in $step_tree. Ask its builder to commit them." >&2
  exit 1
fi

if ! git merge-base --is-ancestor "$change" "$step"; then
  [ -n "$step_tree" ] || { echo "merge-step: $step needs a rebase and has no worktree" >&2; exit 1; }
  if ! git -C "$step_tree" rebase --quiet "$change"; then
    git -C "$step_tree" rebase --abort
    echo "merge-step: $step conflicts with $change. Ask its builder to rebase." >&2
    exit 1
  fi
  echo "rebased $step onto $change"
fi

# The exact commit that gets checked is the one that merges.
commit=$(git rev-parse "refs/heads/$step")
if [ -z "${NO_CHECK:-}" ]; then
  [ -n "$step_tree" ] || { echo "merge-step: $step has no worktree to check. Set NO_CHECK=1 to skip." >&2; exit 1; }
  scripts/check.sh "$step_tree" || { echo "merge-step: checks failed, $change is unchanged" >&2; exit 1; }
  if [ "$(git rev-parse "refs/heads/$step")" != "$commit" ] || dirty; then
    echo "merge-step: $step changed while it was checked, $change is unchanged" >&2
    exit 1
  fi
fi

if [ -n "$change_tree" ]; then
  git -C "$change_tree" merge --quiet --ff-only "$commit"
else
  git merge-base --is-ancestor "$change" "$commit" || { echo "merge-step: $commit does not fast-forward $change" >&2; exit 1; }
  git update-ref "refs/heads/$change" "$commit" "$(git rev-parse "refs/heads/$change")"
fi
echo "merged $step into $change: $(git log -1 --format='%h %s' "$change")"
