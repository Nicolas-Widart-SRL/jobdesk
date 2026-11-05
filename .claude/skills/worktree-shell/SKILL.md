---
name: worktree-shell
description: Shell rules for an agent in an isolated git worktree, so the worktree guard lets each command run. Preloaded into builder, builder-sonnet, docs-writer and designer.
user-invocable: false
---

# Shell in a worktree

The worktree guard refuses any command it cannot verify stays inside your worktree.
A refused call is a wasted turn.

- Run one plain command per Bash call. No `;`, `&&`, `||`, `for`/`while` loops, heredocs, `$(...)` or shell variables.
- Change files with Edit or Write. Never edit with `python3 - <<EOF`, `sed -i` or `perl -i`.
- Read files outside the worktree, such as the cargo registry or a temp folder, with Read, Grep or Glob by literal absolute path. Never `cd` there.
- Run git only as `git <subcommand> <literal args>` from the worktree root.
- Never use `git -C`, `xargs git`, git inside python or another script, or `cd` to the main checkout.
- Give Edit and Write the worktree's absolute path, never the main checkout's path.
- After a refusal, split the command as the message says. Never retry the same command.
