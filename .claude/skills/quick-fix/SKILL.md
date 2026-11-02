---
name: quick-fix
description: The lead's fast path for a small, low-risk change, with one builder and no planner. Use instead of `ship` when the user asks for a small fix, or for a review finding on a merged or open pull request, and the change qualifies below.
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
a snapshot refresh, or a one-value or one-sentence change in `.claude/`.
A `.claude/` change gets one `reviewer` pass (step 3). No other quick fix gets one.

Anything else does not qualify: run `ship`. When unsure, ask nothing: pick `ship`.

Many quick fixes can run in one session, one branch each.
The session does not end after a pull request opens.

## Steps

1. **Start the branch.** Run `scripts/sync-main.sh`, then `git fetch --quiet origin`.
   Name the branch `fix/<slug>`, `chore/<slug>` or `docs/<slug>`, and create it with `git branch <branch> origin/main`.
   When the fix belongs to a change that already has a branch, use that branch.
   Never check out, reset or pull in the main checkout.
   Never check out the branch in any worktree, because `merge-step.sh` moves it with `update-ref`.
2. **Brief one builder.** Run no planner and no plan waves. Write the one step yourself: its files, the edit and its check.
   Use `builder-sonnet` when you name the files, the edit and the test. Use `builder` otherwise.
   Brief it as the Briefs section of the `ship` skill says.
3. **Review.** Only a step that touches `.claude/` gets a review: one `reviewer` pass.
   Brief it as the Briefs section of `ship` says, with the range `<branch>..<step-branch>` and the step's worktree as the head checkout.
   Send its findings to the builder with SendMessage, as Review rounds in `ship` says.
   Re-check only when a High or a finding in the risky lens was fixed, and then only the fix range `<old head>..<new head>`.
   Every other step gets only the merge check, and CodeRabbit on the pull request.
4. **Merge.** Run `scripts/merge-step.sh <step-branch> <branch>`.
   Use `NO_CHECK=1` for a step that touches only docs, or only `.claude/` and Markdown.
   Then run `scripts/cleanup.sh <branch>`.
5. **Open the pull request.** Push the branch and open a pull request against `main`. Link it to the thread when the tool exists.
   Write a minimal plan file, named as `lead.md` says: `# <branch>` on the first line,
   the pull request's number and URL, and a round count of 0.
   Call `watch_pull_request` on it, and record in the plan file that this session watches it.
   Run the `squash-msg` skill with the pull request's number, and record its message in the plan file.
   In one message, give the user the pull request's URL, the squash message's code block,
   and one line saying you watch the pull request.
   From then on, step 8 and the Answer CodeRabbit section of `ship` run on this plan file.
   Without `watch_pull_request`, follow step 7 of `ship` for that case.
   When the branch already has an open pull request, open no second one and keep its plan file:
   run `gh pr view <pr> --json state -q .state` right before the push, and push only when it is `OPEN`.
6. **Escalate.** Stop when the builder reports more than 3 files, a risky lens, or two failed checks.
   Say so to the user in one line, then go on with `ship` from its step 2, on the same branch.
