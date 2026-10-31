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
   its section in `README.md`, and the nearest canvases in `mockups/`. Read only their names and titles.
2. **Draw.** Spawn `designer` with a brief that stands alone:
   - what to design, in the user's words;
   - the folder to write in, and the nearest canvases to start from;
   - the feature and the docs that describe it, by path;
   - what is out of scope.
   The designer renders each canvas to a PNG at 1280x820, the app's window size.
3. **Show.** Give the user each canvas path and the state it shows, the new widgets and the doc changes.
   Show the canvas PNGs in the thread as images.
4. **Iterate.** Send the user's feedback to the same `designer` with SendMessage. It keeps its context.
   Spawn a fresh one only for a different screen.
   Stop when the user approves the canvases.

## After approval

The canvases and their `README.md` are the handoff: the brief, the states, the new widgets and the doc changes.
Commit them only when the user asks.
To build the screen, the user runs `ship` with the canvas paths. The planner reads the README and puts the doc changes in a docs step.

## Claude Design

The designer works on this machine only. It cannot reach claude.ai.
Send a canvas to a Claude Design project only when the user asks for it, in the main session, with the `DesignSync` tool.
That tool needs `/design-login` once. It reaches only the design-system projects the user can write to.
Send only canvases with fictional data. Show the user the paths in the plan before `finalize_plan`.
