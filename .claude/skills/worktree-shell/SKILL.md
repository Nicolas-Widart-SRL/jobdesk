---
name: worktree-shell
description: Shell rules for an agent in an isolated git worktree, so the worktree guard lets each command run. Preloaded into builder, builder-sonnet, docs-writer and designer.
user-invocable: false
---

# Shell in a worktree

The worktree guard refuses any command it cannot verify stays inside your worktree.
A refused call is a wasted turn.

- Run one plain command per Bash call. No `;`, `&&`, `||`, `for`/`while` loops, heredocs, `$(...)`, backticks, `<(...)` or shell variables.
- Start each command with a bare program name or a repo script, such as `cargo` or `scripts/check.sh`. A quoted or backslash-escaped program path is refused even alone. Render canvases with `scripts/render-canvas.sh`.
- Shell variables means `$NAME` expansion.
- An env prefix is fine, such as `UPDATE_SNAPSHOTS=1 cargo test --test screens --test ui_kit_gallery`.
- One pipe into a filter is fine, such as `| tail -n 40` or `| grep ...`.
- A `>` or `2>&1` redirect is fine.
- Write a multi-line commit message with one `-m` per paragraph: `git commit -m "<subject>" -m "<body>"`.
- Change file contents with Edit or Write. Never edit with `python3 - <<EOF`, `sed -i` or `perl -i`.
- Delete or rename files with `git rm` or `git mv` and literal paths.
- Read files outside the worktree, such as the cargo registry or a temp folder, with Read, Grep or Glob by literal absolute path. Never `cd` there.
- Run git only as `git <subcommand> <literal args>` from the worktree root.
- Never use `git -C`, `xargs git`, git inside python or another script, or `cd` to the main checkout.
- Give Edit and Write the worktree's absolute path, never the main checkout's path.
- After a refusal, split the command as the message says. Never retry the same command.
- When the refusal names the main checkout, redo the call in the worktree as the rules above say.
