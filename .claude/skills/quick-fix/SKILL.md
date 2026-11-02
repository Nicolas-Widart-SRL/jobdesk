---
name: quick-fix
description: The lead's fast path for a small, low-risk change, with one builder and no planner. Use instead of `ship` when the user asks for a small fix and the change qualifies below.
argument-hint: "<what to fix>"
---

# Ship a quick fix

The main session is the lead, defined in `.claude/agents/lead.md`.
It writes no code itself: one builder writes the fix.
The user's words in the task win over this workflow, for example "do it yourself" or "use ship".

Task: $ARGUMENTS

## Does it qualify

A task qualifies only when all of these hold:

- it touches at most 3 files;
- it stays in one layer;
- it adds no feature, screen, public type or design, and no canvas;
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
   Always create a new branch: name it `fix/<slug>`, `chore/<slug>` or `docs/<slug>`,
   and create it with `git branch <branch> origin/main`.
   A fix to a merged change gets a new branch too.
   A fix to an open change that `ship` still builds belongs to `ship`, not here.
   Never check out, reset or pull in the main checkout.
   Never check out the branch in any worktree, because `merge-step.sh` moves it with `update-ref`.
2. **Brief one builder.** Run no planner and no plan waves. Write the one step yourself: its files, the edit and its check.
   Use `builder-sonnet` when you name the files, the edit and the test. Use `builder` otherwise.
   Brief it as the Briefs section of the `ship` skill says.
3. **Review.** A step on the Also reviewed list gets one `reviewer` pass, one round.
   Brief it as the Briefs section of `ship` says, with the range `<branch>..<step-branch>` and the step's worktree as the head checkout.
   Send its findings to the builder with SendMessage, as Review rounds in `ship` says.
   Re-check only when a High or a finding in the risky lens was fixed, and then only the fix range `<old head>..<new head>`.
   Every other step gets only the merge check, and CodeRabbit on the pull request.
   When snapshots change, read each changed PNG in the step's worktree with `Read` before the merge.
   When a change is not clean and intended, escalate as step 6 says.
4. **Merge.** Run `scripts/merge-step.sh <step-branch> <branch>`.
   Use `NO_CHECK=1` for a step that touches only docs, or only `.claude/` and Markdown.
   Then run `scripts/cleanup.sh <branch>`.
5. **Open the pull request.** First write a minimal plan file, named as `lead.md` says, with `# <branch>` on its first line.
   Then follow step 7 of `ship`, in its order: it records the pull request, calls `watch_pull_request`,
   runs `squash-msg` and step 8 once, then gives the user one message.
   From then on, step 8 and the Answer CodeRabbit section of `ship` run on this plan file.
6. **Escalate.** Stop when the builder reports more than 3 files, a risky lens, or two failed checks.
   Say so to the user in one line, then go on with `ship` from its step 2, on the same branch.
