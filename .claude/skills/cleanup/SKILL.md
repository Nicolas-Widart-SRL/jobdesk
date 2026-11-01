---
name: cleanup
description: Removes merged agent worktrees, merged branches and their plan files in .claude/plans/. Use when the user says a pull request merged, or asks to clean up worktrees, branches or plan files.
argument-hint: "[change-branch]"
---

# Clean up after merged work

Run `scripts/cleanup.sh $ARGUMENTS` from the main checkout.

- With no argument, it cleans up after a merged pull request, against `origin/main`.
- With a change's branch, it also cleans up the steps already merged into that branch.
- `DRY_RUN=1` prints what would go and removes nothing.

It never checks out, resets or pulls in the main checkout.

Report what it removed and what it kept, with the reason for each kept worktree or plan file.
The script prints a `keep` line for each kept worktree. For plan files it prints one only when the first line names no branch,
so list `.claude/plans/` after the run and give the reason for each file left:
its branch still exists, it changed after its branch's pull request merged, that branch has an open pull request, or `gh` is missing.

These keeps need a human:

- a locked worktree
- a worktree with uncommitted changes or ignored local files other than build output
- a plan file that changed after its branch's pull request merged, has an open pull request, or names no branch on its first line

Name each one and ask the user before you remove anything by hand.
Never unlock a worktree, force a removal or `rm` a file on your own.
