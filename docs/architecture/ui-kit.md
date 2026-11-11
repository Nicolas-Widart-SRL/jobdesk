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

## The first-run wizard

The first run shows one step at a time on a card. Its parts live in `wizard.rs`.

- `wizard_card` draws `step_dots`, then a centred card under them that takes the height left. Its head holds the setup stamp beside the title and the lead. The body scrolls over a foot pinned under a hairline.
- The foot follows the deal footer. The one primary comes first, then a plain button or a `link` for a quiet action. One muted line sits at the right.
- `step_dots` draws one `STATUS_DOT` square per step, with its name under it, joined by a hairline. Done steps are in `text`. The open one is ringed, and the rest are unlit.
- `setup_stamp` is `day_stamp` with one tick per step and "Step 2 of 4" inside. The welcome and the recap show the date in `day_stamp` instead.
- `setup_line` is one line of a step on paper: a knot, a title over a line under it, and a tag or a note at the right. It can add a bar row under the words, and `live_sweep` along its bottom on the line that works now. A line still ahead reads in `muted`, and a failed one says why in `closed`.
- `choice_card` is a pickable card with a radio, a title, an optional tag and a sentence. The whole card is one radio button, named by its title.
- `alert_slip` is a failure on a slip, with a `closed` edge, and its words can be copied.
- In dark, `panel` and the card are one colour. So on a wizard card, a slip, an alert and a knot still ahead take `sheet` (`folder_fill`).

## The Profile chapter

From the Profile chapter on, the window is the app's size and the wizard card widens. These parts live in `setup.rs`, apart from the two cards. Each widget takes plain lists and returns what the user did. None parses Claude's messages or starts a run.

- `wide_card` is `wizard_card` at `WIZARD_WIDE` (1120). Its body holds still and takes the height left over the foot, so a part in it can share that height out.
- `split_card` is `wide_card` split in two: a main column that scrolls on its own, with its bar clear of the words, and a side column of slips, `SPLIT_SIDE_BASIS` wide and `SPLIT_GAP` from it, that holds still.
- `side_slip` is a slip in that side column, with an optional section-face title and a muted count at its right. `slip_text` is its paragraph: a strong lead in `text`, then the rest in `muted`.
- `drop_zone` is paper with a dashed `line` edge in `SWEEP_DASH` dashes, which turns `active` while files are dragged over the window. Its full form holds two stacked mini pages, a title, a sentence and Choose files…. Its slim form, under a file list, holds a sentence and a small Choose files…. It returns a click on Choose files… and the paths dropped on the window.
- `file_list` is one row per added file on paper: a mini page, the name in `mono`, a menu of the kinds the screen gives that says what the file is, its page count and a small Remove. The name shows as plain text with hidden characters removed, cut with the whole name on hover. The menu is an egui `ComboBox`, `KIND_MENU_WIDTH` wide, and a screen reader hears it as Kind of and the name, and Remove with the name. On a row already copied, the menu and Remove are disabled and say "Already copied into your folder." on hover. The kit holds no list of kinds: the screen passes their labels.
- `question_sheet` is Claude's message with a field under each numbered question. Prose renders through `claude_markdown`, and a group's bold line in `Type::TAKE_STRONG`. A question's number is in `muted`, and its lettered sub-items sit inside it, over its one field. A field starts one line high and grows. An empty one says it can be skipped, as "Optional. Leave it empty to skip." under a question marked optional. A screen reader hears each field named by its question.
- `question_fallback` is the sheet when there are no numbered questions to find. Claude's message sits at the left and scrolls on its own. One large field at the right takes the height left over its help. It goes in `wide_card`.
- `answers_slip` is the sheet's side slips. It shows how many questions have an answer, with a `TALLY_FAT` mark for each, lit where answered, then when the draft saved and Show all Claude wrote. Its short form, for follow-ups, keeps the count and one line.
- `change_check` is one proposed change: a checkbox, painted as `setting_check` paints its own, then Claude's number in `muted`, then the change's words. The box, the number and the words are one control. A click on any of them flips it, and Tab stops on it once. A screen reader hears one checkbox named by the number and the words, ticked or not. Unticked, the words go `muted`. It has no hairline or setting row padding, so a list of 16 stays compact.
- `review_changes` is Claude's proposal laid out as `question_sheet` lays out questions, with each change a `change_check`, all ticked to start.
- `review_slip` is "N of M changes" with a mark for each change, lit where ticked, and which are left out. `reply_slip` follows it, showing the reply an action sends in `mono` on `field`, as it will go.
- `cv_check` is the CV's pages as Profile's CV documents draw them. `cv_slip` is its side slip: the CV's name as plain text, a sentence and Open PDF.

Claude's words follow `release_markdown`'s rules, as [Trust](trust.md#rendering-untrusted-text) says: hidden characters go first, a link is its words and never opens, an image is its alt text, and headings are capped at the section size.

- `claude_markdown` renders a block at `Type::TAKE`, held to `DOC_MEASURE`, and its words can be copied.
- A question's or a change's own words are laid out as one plain text with bold kept. They can be copied too, except a change's, whose words are part of its checkbox.

## Gallery snapshots

`tests/ui_kit_gallery.rs` snapshots every component in both themes.
The live sheet, `ui_kit_gallery_live_{light,dark}`, shows each form of the live sweep at a fixed time, its reduced-motion forms, and the estimate note under Cost.
The wizard sheet, `ui_kit_gallery_wizard_{light,dark}`, shows three wizard cards with their lines, alert, slip and foot, then the step dots and setup stamps at each step, and the choice cards.
The setup sheet, `ui_kit_gallery_setup_{light,dark}`, shows the Profile chapter's cards at 1280x820: the question sheet with its answers slip, the review card with two changes unticked and one focused, the fallback sheet, the documents step before and after files are added, and the CV check, then the short answers slip.
The switch sheet, `ui_kit_gallery_switch_{light,dark}`, shows a switch on, off, disabled both ways, and focused, then `name_line` with its note, alone, and cut at the right, then `fitted_row_tag` whole and with its words cut.
