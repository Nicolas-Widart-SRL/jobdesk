---
name: designer
description: The lead's workflow to have the designer draw or redraw canvases for a screen, and iterate on them with the user. Use when the user asks for a design, a mockup, a canvas or a redesign, before any code.
argument-hint: "<screen and what to design>"
---

# Design a screen with the designer

The main session is the lead. It writes the designer's brief, shows the user the canvases and relays feedback.
It draws nothing itself, unless the user says to.

Design: $ARGUMENTS

## Steps

1. **Start the branch.** Run `scripts/sync-main.sh`.
   When it prints `main: could not fetch origin`, stop and tell the user, because `origin/main` may be stale.
   For any other line, tell the user in one line and go on, because the branch starts from `origin/main`, not from the main checkout.
   Then create the change's branch as step 1 of the `ship` skill says:
   name it, then `git branch <change> origin/main`.
   Every designer round starts from it: round 1 from its head, each later round from the round before.
   Each designer works in its own worktree, never in the main checkout.
2. **Brief.** Find the screen's feature in the Features table of `ARCHITECTURE.md`,
   its section in `docs/guide.md`, and the index `mockups/README.md` (or the `README.md` of the folder you will use), which names each canvas and the handoff file for each screen. Read only the lines that match.
3. **Draw.** Spawn `designer` with a brief that stands alone:
   - the change's branch name and its head SHA, so it resets its worktree to that commit;
   - what to design, in the user's words;
   - the folder to write in, and the nearest canvases to start from;
   - the feature and the docs that describe it, by path;
   - what is out of scope;
   - at most 2 options.
   The designer renders each canvas to a PNG at 1280x820, the app's window size.
4. **Show.** Give the user each canvas path and the state it shows, the new widgets and the doc changes.
   Show the canvas PNGs in the thread as images, from the designer's worktree.
   Record the designer's branch and head SHA in the change's plan file.
5. **Iterate.** Spawn a fresh `designer` for each round of feedback.
   Never send feedback to an earlier designer with SendMessage, because its context grows with each round.
   Its brief: the previous round's designer branch and head SHA, so it resets its worktree to that commit first,
   the user's feedback word for word, the folder, the index and the round's handoff files,
   the canvases to change, and what to keep. Stop when the user approves the canvases.

## After approval

The canvases and their handoff files in `mockups/handoff/` (or the `handoff/` folder of the brief's folder) are the handoff:
the brief, the states, the rules, the new widgets and the doc changes.
Spawn one last fresh `designer` to finalize the round. Its brief gives the previous round's designer branch and head SHA,
says the user approved, and gives the folder, the handoff file paths, the canvases and the options the user chose.
It rewrites the handoff files, their index lines and the canvases to show only the approved design:
it removes the options not chosen, names the kept canvas for its screen, re-renders its PNGs,
drops the Open section, and leaves Doc changes listing only what the approved design still needs.

The final designer's branch is the finished design.
Merge it into the change's branch with `NO_CHECK=1 scripts/merge-step.sh <design-branch> <change>`,
then run `scripts/cleanup.sh <change>`.
Tell the user in one line that the design is on `<change>`, and ask whether to plan and build it now, in this session.
The default is yes. On yes, go on with `ship` from its step 2, on the same branch,
with the canvas paths and every changed handoff file (see Other agents on call in `ship`).
The planner reads those files and puts their doc changes in a docs step, which also deletes the Doc changes section from each file after applying it.

## Claude Design

The designer works on this machine only. It cannot reach claude.ai.
Send a canvas to a Claude Design project only when the user asks for it, in the main session, with the `DesignSync` tool.
That tool needs `/design-login` once. It reaches only the design-system projects the user can write to.
Send only canvases with fictional data. Show the user the paths in the plan before `finalize_plan`.
