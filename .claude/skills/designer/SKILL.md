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

1. **Brief.** Find the screen's feature in the Features table of `ARCHITECTURE.md`,
   its section in `docs/guide.md`, and the index `mockups/README.md` (or the `README.md` of the folder you will use), which names each canvas and the handoff file for each screen. Read only the lines that match.
2. **Draw.** Spawn `designer` with a brief that stands alone:
   - what to design, in the user's words;
   - the folder to write in, and the nearest canvases to start from;
   - the feature and the docs that describe it, by path;
   - what is out of scope;
   - at most 2 options.
   The designer renders each canvas to a PNG at 1280x820, the app's window size.
3. **Show.** Give the user each canvas path and the state it shows, the new widgets and the doc changes.
   Show the canvas PNGs in the thread as images.
4. **Iterate.** Spawn a fresh `designer` for each round of feedback.
   Never send feedback to an earlier designer with SendMessage, because its context grows with each round.
   Its brief: the user's feedback word for word, the folder, the index and the round's handoff files,
   the canvases to change, and what to keep. Stop when the user approves the canvases.

## After approval

The canvases and their handoff files in `mockups/handoff/` (or the `handoff/` folder of the brief's folder) are the handoff:
the brief, the states, the rules, the new widgets and the doc changes.
Spawn one last fresh `designer` whose brief says the user approved and names the chosen options.
Give it the folder, the paths of the handoff files and the canvases of each option.
It only finalizes the round: it removes the options not chosen and the Open section from the handoff files,
deletes the canvases and PNGs of the options not chosen, and drops them from the index line.
Commit the files only when the user asks.
The design session ends there. Tell the user to commit the canvases and handoff files and merge them to `origin/main`,
because `ship` builds from it. Then tell them to run `/clear` or start a new session
and run `ship` with the canvas paths and every changed handoff file there.
The planner reads those files and puts their doc changes in a docs step, which also deletes the Doc changes section from each file.

## Claude Design

The designer works on this machine only. It cannot reach claude.ai.
Send a canvas to a Claude Design project only when the user asks for it, in the main session, with the `DesignSync` tool.
That tool needs `/design-login` once. It reaches only the design-system projects the user can write to.
Send only canvases with fictional data. Show the user the paths in the plan before `finalize_plan`.
