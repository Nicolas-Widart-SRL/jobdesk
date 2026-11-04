# ui_kit

This page covers the rules every `ui_kit` widget follows. `ui_kit` is the single owner of the Ledger design system, and its place among the modules is in [ARCHITECTURE.md](../../ARCHITECTURE.md#features).

## Tokens and colour

- `ui_kit` owns the colour, spacing, radius, stroke and type tokens, plus every widget that paints them.
- Colour is for job status, plus one stamp ink for what waits on the user.
- The window is a gridded desk in both themes, and anything read in rows sits on paper.

## Interaction states

- Every clickable kit widget shows the pointing hand, a hover, a pressed and a focus state.
- It paints its own disabled look, with the reason on hover.
- A picked row or card adds an ink edge to the hover fill.

## What can be selected

Labels are not selectable, so a click on a row's text picks the row. Only these can be selected and copied:

- documents, code boxes, alerts and load errors;
- your replies;
- Claude's questions shown whole, and their choices;
- the research note, the archived posting and drafts;
- the folder path in Settings › General.

None of them sits inside something clickable.

## Motion

- A looping animation is allowed only for a live run's signal, one per screen, and only while it shows. Everything else moves once, for `motion_time`, and stops.
- Reduce motion swaps the loop for a still form: `motion_time` is zero, so `sweep_moves` is false.
- A widget never schedules its own frames. It returns whether it moves, and the screen asks for a repaint on a timer only while it shows and the answer is yes.

## The live sweep

`live_sweep` paints the live-run signal along the bottom edge of a band cell, and `turn_band_live` puts it on the Whose turn cell. The caller passes the seconds into the sweep and a `SweepForm`:

- `Working`: an `active` segment crosses the edge in 2.4 s, then rests 0.6 s.
- `Quiet`: a `muted` segment at half the speed.
- `LongQuiet`: a still dashed rule in `muted`.
- `Stopping`: a still solid rule in `muted`.

With motion reduced, `Working` and `Quiet` become a solid rule in their ink. The sweep is decoration, so it adds nothing for screen readers. `band_note_spoken` gives a band note other words for them than it shows, for a note that counts seconds.

`estimate_note` is the one muted line under a figure's bar while the figure is an estimate, such as a live run's Cost.

## Gallery snapshots

`tests/ui_kit_gallery.rs` snapshots every component in both themes.
The live sheet, `ui_kit_gallery_live_{light,dark}`, shows each form of the live sweep at a fixed time, its reduced-motion forms, and the estimate note under Cost.
