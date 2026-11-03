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

## The status bar and updates

The right of the status bar is Jobdesk's own: the update group, a `status_divider`, then Settings. The group shows only while there is news of an update, and its words are cut first, so its action and Settings keep their width.

- `status_mark_button` leads the group: a square and words that open the release notes, `on` while they are open. `status_mark` draws the same with nothing to open, and takes no click and no focus.
- `status_action` is the group's one step, such as Download. It is the one bordered control in the bar, and it is never cut.
- `status_progress` replaces the square and words while a download runs: the words, a bar, then the share and size in figures of equal width, so the strip holds still as they count.
- The square is `text` for news, `hired` when the download is ready and `closed` when something failed. An update does not wait on the user, so it never takes the stamp ink.
- `release_notes_popover` opens above the group, right edges level. Its primary action takes the focus on open, unless it waits: then the notes take it, and the slip says why, as the hover does. Esc, a click outside and the primary action close it.
- `release_markdown` draws the notes, as [Trust](trust.md#rendering-untrusted-text) says.
- `small_primary_button` and `setting_row_lines` serve Settings › About: the one step a row offers, and help of several lines, each in its own ink.

## Gallery snapshots

`tests/ui_kit_gallery.rs` snapshots every component in both themes.
