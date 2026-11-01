---
name: build-step
description: How a builder builds one step of a plan in its own git worktree - rules, steps, checks, snapshots, commit and report. Preloaded into builder and builder-sonnet.
user-invocable: false
---

# Build one step

You build one step in your own git worktree. Other builders work beside you in their own worktrees.

## Rules

- Follow `AGENTS.md`. It holds the layout, the hard rules and the commands. Read `ARCHITECTURE.md` when the step adds a feature or touches layers, runs or trust handling.
- Never run `claude`, a slash command or the paid live tests (`--ignored`). Never write to a real `ai-job-search` checkout.
- Test data is fictional. Add it to `tests/sample_workspace` or write it inline in the test.
- Build only the step in your brief. When the step needs more, stop and say what.
- Never push, merge or open a pull request. The lead merges steps into the change's branch and opens the pull request.
- Never rebase, reset or push the change's branch. The lead does that.
- Never open the app on the user's desktop. The snapshot tests show the screens.
- Leave your worktree in place, because the lead runs the checks in it.
  The lead removes it, and its large `target/`, after the step merges.
  Create no other worktree.
- Find code with trail before you grep or read whole files:
  `trail ask "<what you need>" --source` finds code and shows it, `trail grep "<literal>"` lists every use,
  `trail skeleton <file>` shows a file's API, and `trail callers <symbol>` lists who calls it.
  Run `trail callers <symbol> --depth all` before you change a symbol that other code uses.
  The graph shows `main`, so read the files you changed yourself. Use `git grep` when trail finds nothing.

## Steps

1. Read the brief and the files it names. Find the other code the step touches with trail.
   Read several files in one turn when you can.
2. Write the test first when the step has a testable rule.
3. Make the change. Match the surrounding code: its names, idioms, comment density and hand formatting.
   Never run `cargo fmt`.
4. Update `README.md` when the user sees a change, and `ARCHITECTURE.md` when the features, layers, runtime or trust handling change.
5. Run `scripts/check.sh`. It runs `cargo test` and prints one line. Fix every failure.
   When a snapshot differs, read `tests/snapshots/<name>.diff.png` and `<name>.new.png`.
   If the change is meant, run `UPDATE_SNAPSHOTS=1 cargo test --test screens --test ui_kit_gallery`.
   Read each changed `<name>.png` beside its `<name>.old.png` (`git status tests/snapshots` lists them).
   Then run `scripts/check.sh` again. Commit the new pictures with the step, and only when the check passes.
   Refresh snapshots on macOS only.
6. Commit on your worktree branch with a Conventional Commits subject.
   End the message with `Co-Authored-By: Claude <noreply@anthropic.com>`.

When a review sends findings back, fix each one, run the check again and commit again.
Do not amend or rebase commits a reviewer has seen.

## Report

Keep it short: the lead reads every word.

- **Branch** and **worktree path**.
- **Commits**: SHA and subject.
- **Checks**: the line `scripts/check.sh` printed.
- **Notes**: anything you did not do, and why.

Leave trail's "tokens saved" lines out of the report.
