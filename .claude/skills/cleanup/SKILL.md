---
name: cleanup
description: Removes merged agent worktrees, merged branches and their plan files in .claude/plans/. Use when the user asks to clean up worktrees, branches or plan files. When the user says a pull request merged, or the lead sees it merged on a watch wake, follow the lead's Session start, which runs this script.
argument-hint: "[change-branch]"
---

# Clean up after merged work

Run `scripts/cleanup.sh $ARGUMENTS` from the main checkout.
When the user says a pull request merged, or the lead sees it merged on a watch wake, follow Session start in `.claude/agents/lead.md` instead: it checks each branch with `gh pr view` first.

- With no argument, it cleans up after a merged pull request, against `origin/main`.
- With a change's branch, it also cleans up the steps already merged into that branch.
- For a dry run or a preview, run `DRY_RUN=1 scripts/cleanup.sh [change-branch]`. It prints what would go and removes nothing.
  `DRY_RUN` is an environment variable: never pass it as an argument, because the script reads an argument as a base and does a real run.

It never checks out, resets or pulls in the main checkout.

## Report

Build the report only from the script's output. Run no other command to explain it.

- Tell the user in one line what it removed: its `remove worktree`, `delete branch` and `delete plan` lines.
  After a dry run, say these would go, and that nothing was removed.
- Name each `keep` line that needs a human, as below.
- Make no claim about why a plan file without a line stays.

These worktree keeps need a human:

- `locked`
- `uncommitted changes`
- `ignored local files`
- `git status failed`

Name each one and ask the user before you remove anything by hand.
Leave every plan file the script keeps alone: it may hold live `ship` state.
Never unlock a worktree, force a removal or `rm` a file on your own.
