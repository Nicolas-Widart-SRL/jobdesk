---
name: ship
description: The lead's workflow for a change of more than one step, from plan to pull request, with the subagent team. Use when the user asks to implement a feature, a plan or any change that spans several steps or layers. For a one-step fix, do it directly.
argument-hint: "<feature or task>"
---

# Ship a change with the subagent team

The main session is the lead. It runs on Opus at medium effort.
It reads the notes, writes each subagent's brief and merges steps into the change's branch. It writes no code itself.
Each subagent's model and effort live in its file in `.claude/agents/`, so do not override them in the call.
The user's words in the task win over this workflow, for example "do it yourself" or "skip the planner".

Task: $ARGUMENTS

## One session per change

Run one change per session, so the lead's context holds only that change.
Keep its state in `.claude/plans/<change-branch>.md`, which git ignores:
the plan, the user's decisions, and each step's branch, worktree, head SHA and status.
Update the file after each step merges and after each review. A new session reads it and goes on from there.
When the pull request is open, tell the user to run `/clear` or start a new session before the next change.

Keep command output out of your context. Run checks only through `scripts/check.sh` and `scripts/merge-step.sh`,
which print one line per check. Read a subagent's report once, then keep only its SHAs and findings in the plan file.

## Steps

1. **Plan.** `planner` turns the task into steps, each with a check.
   Name the change's branch, such as `feat/standing-orders`, when the user did not.
   Write the plan to `.claude/plans/<change-branch>.md`.
   Show the user the plan and stop, unless the user said to go on without it.
2. **Build.** A builder builds one step in its own git worktree and commits it.
   Use `builder-sonnet` only when the planner marked the step written down. Use `builder` for every other step.
   Steps that do not depend on each other build in parallel.
3. **Review a risky step.** Run the `code-review` skill on a step the planner marked risky,
   against the change's branch. Send its findings to the step's builder with SendMessage.
   Every other step gets only the check in `scripts/merge-step.sh`, and the change review below.
4. **Merge the step.** Once the review passes, or at once when the step is not risky,
   run `scripts/merge-step.sh <step-branch> <change-branch>`.
   It rebases the step when needed, runs the check and fast-forwards the change's branch.
   Then run `scripts/cleanup.sh <change-branch>` (see Clean up).
5. **Review the change.** When every step is in, run the `code-review` skill once on the whole change:
   base is where the branch left `main`, head is the branch tip.
   For a visible change, read the changed snapshot PNGs, and compare them with the canvases when there are some.
   Nothing goes into a pull request without this pass.
6. **Open the pull request.** Push the change's branch and open a pull request against `main`.
   Link it to the thread when the tool exists.
   Never merge into `main`. The user merges the pull request.

A change with one step skips the step review: the change review is its only review.
A docs-only step is never risky: merge it with `NO_CHECK=1 scripts/merge-step.sh`.

The planner has no edit tools, so it cannot change what it plans.
The lead never fixes a finding itself: the builder that built the step fixes it.

## Briefs

Give each subagent a brief that stands alone. It has none of your context.
Give a builder:

- the step from the plan, word for word, with its files and its check;
- the change's branch, so it branches from the right commit;
- what is out of scope.

When the change builds a design, add the canvas paths.

## Review rounds

Review in two rounds at most: the first review, then one more after the builder fixes the findings.
This holds for each risky step and for the change.
In round 2, review only the fix commits.
When the second round still fails, stop and report its open findings to the user.
In your final message, say what the reviews covered and what they found.

## Clean up

Run `scripts/cleanup.sh <change-branch>` after each step merges, and `scripts/cleanup.sh` after the user merges the pull request.
It removes merged worktrees in `.claude/worktrees/` and the merged branches.
Keep the change's branch until its pull request merges.

## Other agents on call

- `designer` draws HTML canvases for a new screen in `mockups/`, before the planner. The `designer` skill runs it with the user.
  When the task names canvases, give their `README.md` to the planner.

Use cheap models only for work that is fully written down.
