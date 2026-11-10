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

## Switch or checkbox

- `switch` is for a state that takes effect at once on a thing in a list, such as a portal on or off. Its word, On or Off, follows the track, and a screen reader hears it as a checkbox named by its row, with its state.
- `setting_check` is for a preference on a settings page. A checkbox reads as part of a form, so it stays on settings pages.

## A tag in a column that gives way

`fitted_row_tag` is a `row_tag` where it fits in the room left, and otherwise its words cut with an ellipsis in the tag's ink, whole on hover. Use it in a column that gives up its width before the actions beside it, such as the community list's Maintained column on Portals.

## The status bar and updates

The right of the status bar is Jobdesk's own: the update group, a `status_divider`, then Settings. The group shows only while there is news of an update, and its words are cut first, so its action and Settings keep their width.

- `status_mark_button` leads the group: a square and words that open the release notes, `on` while they are open. `status_mark` draws the same with nothing to open, and takes no click and no focus.
- `status_action` is the group's one step, such as Download. It is the one bordered control in the bar, and it is never cut.
- `status_progress` replaces the square and words while a download runs: the words, a bar, then the share and size in figures of equal width, so the strip holds still as they count.
- The square is `text` for news, `hired` when the download is ready and `closed` when something failed. An update does not wait on the user, so it never takes the stamp ink.
- `release_notes_popover` opens above the group, right edges level. Its primary action takes the focus on open, unless it waits: then the notes take it, and the slip says why, as the hover does. Esc, a click outside and the primary action close it.
- `release_markdown` draws the notes, as [Trust](trust.md#rendering-untrusted-text) says.
- `small_primary_button` and `setting_row_lines` serve Settings › About: the one step a row offers, and help of several lines, each in its own ink.

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
The switch sheet, `ui_kit_gallery_switch_{light,dark}`, shows a switch on, off, disabled both ways, and focused, then `name_line` with its note, alone, and cut at the right, then `fitted_row_tag` whole and with its words cut.
