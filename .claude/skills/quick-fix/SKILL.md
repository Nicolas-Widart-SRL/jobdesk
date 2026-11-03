---
name: quick-fix
description: The lead's fast path for a small, low-risk change, with one builder and no planner. Use instead of `ship` when the user asks for a small fix and the change qualifies below.
argument-hint: "<what to fix>"
---

# Ship a quick fix

The main session is the lead, defined in `.claude/agents/lead.md`.
It writes no code or docs itself: one builder writes the fix, or `docs-writer` writes a docs-only fix.
The user's words in the task win over this workflow, for example "do it yourself" or "use ship".

Task: $ARGUMENTS

## Does it qualify

A task qualifies only when all of these hold:

- it touches at most 3 files;
- it stays in one layer;
- it adds no feature or screen, adds or changes no public type, and adds or changes no canvas or design;
- it falls under none of the risky lenses in step 6 of `.claude/agents/planner.md`,
  except a one-value or one-sentence change in `.claude/`.

Typical quick fixes: copy, a constant, a typo, one bug with its test, a docs line, a script or CI tweak,
or a one-value or one-sentence change in `.claude/`.

Anything else does not qualify: run `ship`. When unsure, ask nothing: pick `ship`.
Findings on a pull request, from CodeRabbit or a review, never come here: Answer CodeRabbit in `ship` handles them.

Many quick fixes can run in one session, one branch each.
The session does not end after a pull request opens.

## Also reviewed

Step 6 of `planner.md` stays the risky test. A step that touches any of these paths also gets one `reviewer` pass:

- `.claude/`
- `scripts/`
- `.github/`
- `AGENTS.md`
- `ARCHITECTURE.md`

A docs line elsewhere, copy, a constant, a typo, or one bug with its test needs no review.

## Steps

1. **Start the branch.** Run `scripts/sync-main.sh`, then `git fetch --quiet origin`.
   The code to fix must be on `origin/main`. Otherwise use `ship`.
   Always create a new branch: name it `fix/<slug>`, `chore/<slug>` or `docs/<slug>`,
   and create it with `git branch <branch> origin/main`.
   A fix to a merged change gets a new branch too.
   Never check out, reset or pull in the main checkout.
   Never check out the branch in any worktree, because `merge-step.sh` moves it with `update-ref`.
2. **Brief one builder.** Run no planner and no plan waves. Write the one step yourself: its files, the edit and its check.
   Use `docs-writer` when the fix changes only `README.md`, `AGENTS.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md` or files in `docs/`.
   Use `builder-sonnet` when you name the files, the edit and the test. Use `builder` otherwise.
   Brief it as the Briefs section of the `ship` skill says.
   Once it reports, check the step as step 6 says, before any review.
3. **Review.** A step on the Also reviewed list gets one `reviewer` pass, one round.
   Brief it as the Briefs section of `ship` says, with the range `<branch>..<step-branch>` and the step's worktree as the head checkout.
   Send the builder, or the docs-writer, every High and Medium finding with SendMessage.
   Once it has applied them, run the step 6 check again (`git diff --stat`, files, layers, areas) before the merge.
   This is not another reviewer round.
   A High also escalates after the builder's fix (step 6), so `ship` re-reviews it.
   Every other step gets only the merge check, and CodeRabbit on the pull request.
   When snapshots change, read each changed PNG in the step's worktree with `Read` before the merge.
   If a changed snapshot PNG is not an intended, clean change, use `ship`.
4. **Merge.** Run `scripts/merge-step.sh <step-branch> <branch>`.
   Use `NO_CHECK=1` for a step that touches only docs, or only `.claude/` and Markdown.
   Then run `scripts/cleanup.sh <branch>`.
5. **Open the pull request.** First write a minimal plan file, named as `lead.md` says, with `# <branch>` on its first line.
   Then follow step 7 of `ship`, in its order: it records the pull request, calls `watch_pull_request`,
   runs `squash-msg` and step 8 once, then gives the user one message.
   From then on, step 8 and the Answer CodeRabbit section of `ship` run on this plan file.
6. **Escalate.** Read `git diff --stat <branch>..<step-branch>` yourself for the file count and the layers.
   Stop when the step touches more than 3 files, crosses a layer, adds a feature or screen, adds or changes a public type, or adds or changes a canvas or a design.
   Stop too when it touches a risky area from `planner.md` step 6
   (the one-value or one-sentence `.claude/` change in the criteria is not one),
   when two checks failed, when the review found a High, or when a changed snapshot is not intended and clean.
   Say so to the user in one line.
   When the step is already built, keep its branch, and do not plan or build it again.
   Go on with `ship` at step 5, merging the step into `<branch>` with `scripts/merge-step.sh`, then step 6, the change review.
   When nothing is built yet, go on with `ship` from its step 2, on the same branch.
