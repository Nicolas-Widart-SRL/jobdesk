---
name: worktree-shell
description: Shell rules for an agent in an isolated git worktree, so the worktree guard lets each command run. Preloaded into builder, builder-sonnet, docs-writer and designer.
user-invocable: false
---

# Shell in a worktree

The worktree guard refuses any command it cannot verify stays inside your worktree.
A refused call is a wasted turn.

- Run one plain command per Bash call. No `;`, `&&`, `||`, `for`/`while` loops, heredocs, `$(...)`, backticks, `<(...)` or `$NAME` shell variables.
- Start each command with a bare program name or a repo script, such as `cargo` or `scripts/check.sh`. A quoted or backslash-escaped program path is refused even alone. Render canvases with `scripts/render-canvas.sh`.
- An env prefix is fine, such as `UPDATE_SNAPSHOTS=1 cargo test --test screens --test ui_kit_gallery`.
- One pipe into a filter is fine, such as `| tail -n 40` or `| grep ...`.
- `2>&1` is fine. Send `>` only to `/dev/null` or to a literal path inside the worktree that git does not track, such as `target/out.txt`.
- Write a multi-line commit message with one `-m` per paragraph, in single quotes: `git commit -m '<subject>' -m '<body>'`.
  Inside double quotes a backtick runs a command and `$` expands, and a body often holds code spans.
  A body that holds a single quote needs another `-m` split or other wording.
- Change file contents with Edit or Write. Never edit with `python3 - <<EOF`, `sed -i` or `perl -i`.
- Delete or rename files with `git rm` or `git mv` and literal paths.
- Read files outside the worktree, such as the cargo registry or a temp folder, with Read, Grep or Glob by literal absolute path. Never `cd` there.
- Run git only as `git <subcommand> <literal args>` from the worktree root.
- Never use `git -C`, `xargs git`, git inside python or another script, or `cd` to the main checkout.
- Give Edit and Write the worktree's absolute path, never the main checkout's path.
- After a refusal, split the command as the message says. Never retry the same command.
- When the refusal names the main checkout, redo the call in the worktree as the rules above say.
