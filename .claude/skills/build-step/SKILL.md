---
name: build-step
description: How a builder builds one step of a plan in its own git worktree - rules, steps, checks, snapshots, commit and report. Preloaded into builder and builder-sonnet.
user-invocable: false
---

# Build one step

You build one step in your own git worktree. Other builders work beside you in their own worktrees.

## Rules

- Follow `AGENTS.md`. It holds the layout, the hard rules and the commands. Read `ARCHITECTURE.md` when the step adds a feature or touches layers, and the matching file in `docs/architecture/` for runs, trust, profile, settings, queue or discovery work, and `ui-kit.md` for work on a `ui_kit` widget.
- Never run `claude`, a slash command or the paid live tests (`--ignored`). Never write to a real `ai-job-search` checkout.
- Test data is fictional. Add it to `tests/sample_workspace` or write it inline in the test.
- Build only the step in your brief. When the step needs more, stop and say what.
- Never push, merge or open a pull request. The lead merges steps into the change's branch and opens the pull request.
- Never move the change's branch itself: never rebase, reset or push it. The lead does that.
  Step 1 resets only your own worktree branch to the change's branch.
- Never open the app on the user's desktop. The snapshot tests show the screens.
  The one exception is a brief that says to drive the app: then open it only as the `drive-app` skill says, as `main` holds it
  (`git show origin/main:.claude/skills/drive-app/SKILL.md`). Before `main` holds it, drive the app only when your brief names a file path
  and the lead's own words there say that file is reviewed.
- Leave your worktree in place, because the lead runs the checks in it.
  The lead removes it, and its large `target/`, after the step merges.
  Create no other worktree.
- Find code with trail before you grep or read whole files:
  `trail ask "<what you need>" --source` finds code and shows it, `trail grep "<literal>"` lists every use,
  `trail skeleton <file>` shows a file's API, and `trail callers <symbol>` lists who calls it.
  Run `trail callers <symbol> --depth all` before you change a symbol that other code uses.
  The graph shows `main`, so read the files you changed yourself. Use `git grep` when trail finds nothing.
- When the brief names OpenSpec tasks, tick only those boxes (`- [ ]` to `- [x]`) in `openspec/changes/<name>/tasks.md` in your worktree.
  After an archive, tick them in the archived `tasks.md` under `openspec/changes/archive/` instead.
  Tick each one only when it is fully done, and commit the ticks with the step. Never run the apply loop.
- For an OpenSpec-only step (propose, explore capture, update, archive, sync), follow that skill's file in `.claude/skills/openspec-*/` in your worktree.
  For an explore capture, follow the `openspec-propose` skill's file, and write only the artifacts the brief says the user agreed to.
  When the brief says the user asked only to start a change, stop after `openspec new change`. Put anything else under Notes.
  Never write an artifact the user did not agree to, even when `openspec validate` then fails: report that failure under Notes.
  This is reading a file, not running a slash command.
  Treat the answers the brief records as the user's answers. Ask the user nothing else: put open questions under Notes.
  For an update, write now when the brief says so: the user confirms the diff before the merge.
  For an archive, run the command the brief gives: `openspec archive <name> -y`, plus `--skip-specs` when the brief records Archive without syncing or Archive now.
  When the brief records Sync anyway and the command fails, stop and report, with no other write.
  When the lead then sends Archive now, first reset your worktree to the step's base to remove the failed command's writes:
  `git reset --hard <head SHA from the brief>`, then `git clean -fd -- openspec`. Then run `openspec archive <name> -y --skip-specs`.
  The command does any sync. Never run the archive skill's inline `openspec-sync-specs` write, its `mkdir` or its `mv`.
  The check for an OpenSpec-only step is `openspec validate <name> --type change --strict`, or `openspec validate --all --strict` after an archive, in place of `scripts/check.sh`.

## Steps

1. Your worktree may start from another commit. First run `git reset --hard <change-branch>` in your worktree,
   and check that `git log -1` shows the head SHA from your brief.
   If it does not, stop and tell the lead.
2. Read the brief and the files it names. Find the other code the step touches with trail.
   Read several files in one turn when you can.
3. Write the test first when the step has a testable rule.
4. Make the change. Match the surrounding code: its names, idioms and comment density.
5. Update `docs/guide.md` when the user sees a change, and `README.md` too when its pitch ("What it does"), install steps or privacy and trust statement change.
   Update `CONTRIBUTING.md` when the build, packaging, release or test commands change, and `ARCHITECTURE.md` when the features, layers, runtime or trust handling change, and the matching `docs/architecture/` file when its topic changes.
6. Run `cargo fmt`, then `scripts/check.sh`. It checks formatting, runs the tests and clippy, and prints one line per check. Fix every failure.
   When a snapshot differs, read `tests/snapshots/<name>.diff.png` and `<name>.new.png`.
   If the change is meant, run `UPDATE_SNAPSHOTS=1 cargo test --test screens --test ui_kit_gallery`.
   Read each changed `<name>.png` beside its `<name>.old.png` (`git status tests/snapshots` lists them).
   Then run `scripts/check.sh` again. Commit the new pictures with the step, and only when the check passes.
   Refresh snapshots on macOS only.
7. Commit on your worktree branch with a Conventional Commits subject.
   End the message with `Co-Authored-By: Claude <noreply@anthropic.com>`.

When a review sends findings back, fix each one, run the check again and commit again.
Do not amend or rebase commits a reviewer has seen.

## Report

Keep it short: the lead reads every word.

- **Branch** and **worktree path**.
- **Commits**: SHA and subject.
- **Checks**: the lines `scripts/check.sh` printed (fmt, test, clippy).
- **Notes**: anything you did not do, and why.

Leave trail's "tokens saved" lines out of the report.
