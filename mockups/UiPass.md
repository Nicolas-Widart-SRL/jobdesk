# UI pass: bugs and gaps

The user asked for a UI pass over the whole app: "links and clickable items dont show a hand icon. job list: job title and sub title show a cursor, line not clickable there. and similar."

**Sources**: the code in `src/ui_kit/` and `src/*/ui*`, the committed snapshots in `tests/snapshots/` (fictional data from `tests/sample_workspace`), and the egui 0.37 and egui_extras 0.37 sources the app builds against. No real job search was used.

**Canvas**: `UiPass.html`, rendered as `UiPass-*.png`. The UI pass section of `README.md` lists each scene.

Impact means how often a person meets the problem, times how much it stops them. Each finding gives what the user meets, where, the fix as a rule (one rule in `ui_kit` wherever one rule covers every call site), and which snapshots and tests change.

## Why the two seed bugs happen

Both come from egui defaults that the kit never overrides.

1. **No hand.** `install_theme` sets `visuals.interact_cursor = Some(PointingHand)` (`palette.rs:207`). In egui 0.37 only `egui::Button` reads that setting (`egui/src/widgets/button.rs:376`). Every Jobdesk control is painted by the kit, with `allocate_exact_size(.., Sense::click())`, so none of them ever sets a cursor. Only `egui::Link` sets the hand itself (`egui/src/widgets/hyperlink.rs:72`).
2. **I-beam and dead clicks on row text.** `selectable_labels` defaults to `true` (`egui/src/style.rs:1599`), and the kit never turns it off. A selectable label senses click-and-drag (`egui/src/widgets/label.rs:141-168`) and sets the text cursor on hover (`egui/src/text_selection/label_text_selection.rs:645`). Because it sits on top of the row, it becomes the click target. egui then marks nothing under an interactive widget as hovered (`egui/src/interaction.rs:282-297`), so the row neither lights up nor gets the click. Every kit text goes through `show_label` (`text.rs:30-32`), so this happens wherever text sits inside something clickable.

The Recent runs list proves the fix: `recent_row` turns selection off by hand (`run_screen.rs:843`, `847`), with the comment "Its labels take no clicks, so a click anywhere on the row picks the run." The rule below makes that the default everywhere.

## Bugs: broken behavior

### B1. No clickable control shows a pointing hand

**What the user meets**: buttons, tabs, segments, the theme switch, Jobs rows, application cards, Coming up rows, command rows, Recent runs and step toggles all keep the arrow. Only "Show the command", "Bring them back now" and links inside Claude's messages show a hand, so the app looks half-clickable.

**Where**: every kit widget that senses a click: `controls.rs:72` (all buttons), `146` (New task), `295` (tabs), `348` (segments, the theme switch), `400` and `415` (pick and command rows); `surfaces.rs:361` (`pick_row`), `545` (`index_card`); `tray.rs:212` (agenda rows). egui's own controls on a surface also lack it: the checkbox and model list in the launcher (`launcher.rs:108`, `125`), and the choices in a question (`run_screen.rs:736`, `738`).

**Fix, one rule**: replace `focus_ring` with one `interactive(ui, &response, radius)` helper in `controls.rs` that every clickable kit widget calls last. It does three things:
- sets `CursorIcon::PointingHand` while hovered and enabled;
- paints the pressed state (G2);
- paints the focus ring.

A disabled widget never reports hover (`egui/src/context.rs:1787`), so it keeps the arrow by itself. egui checkboxes, radios and combo boxes get `.on_hover_cursor(PointingHand)` where they are added. Keep `interact_cursor` for the egui buttons in the combo popup.

**Snapshots**: none, because the cursor is not in a screenshot.
**Tests**: add one to `ui_kit` that hovers each clickable widget in the gallery and checks `ctx.output(|o| o.cursor_icon) == PointingHand`, plus an arrow over a disabled button.

### B2. Text inside a clickable row or card swallows the click and shows an I-beam

**What the user meets**:
- On Jobs, the title and company show an I-beam, and clicking them does not pick the job. Only the blank parts of the row work.
- The same happens on the score, the location, the day rate, the date, "+1" and the site name.
- On Applications, clicking the company, role or status line of an index card does nothing. Only its padding works.
- While the pointer is over that text, the row or card loses its hover fill, so it looks dead.

**Where**:
- Jobs: `discovery/ui.rs:192-228`. `title_over` lays its two lines out as labels (`text.rs:94-105`), and `score`, the `Label`s at `198`, `203`, `207`, `208`, `225` and `muted` at `217` add more.
- Applications: `tracking/ui.rs:187-197`.
- Every other label in the app is selectable too. Outside clickable areas that is only a wrong cursor, such as the I-beam over the Today card, headings and slips.

**Fix, one rule**: `install_theme` sets `style.interaction.selectable_labels = false`. Labels then sense hover only, so clicks fall through to the row and the row keeps its hover.
- Copying stays where it matters, through a new `copyable` scope in `text.rs`. It turns selection back on for Claude's documents (`document`, `markdown`), `code_box` (the command line, the permission input), the archived posting (`dossier/ui.rs:413`), the research note (`394`), follow-up drafts (`553`) and a failed tool's reason.
- A `copyable` text must never sit inside a clickable container. The kit owns all of these, so a test can check it.
- Drop the two hand-written `.selectable(false)` calls in `run_screen.rs:843`, `847`.

**Snapshots**: none. Labels look the same at rest.
**Tests**: add `screens` tests that press and release the pointer on the second job's title and check that the detail shows that job. Do the same on an application card's role, and check that the card's company is shown in the detail slip.

### B3. Hover and selected look the same, and on a slip neither shows

**What the user meets**:
- On Jobs, the row under the pointer gets the same fill as the picked row, so for a moment two jobs look picked.
- In the launcher, a hovered command looks picked. In dark, the picked command has no fill at all, so nothing says which command the form belongs to (`launcher_dark.png`, `/scrape`).
- In Recent runs, neither hover nor the picked run shows. Only the ink changes (`run_turn_light.png`).
- On Applications, the picked card and a hovered card look alike.

**Where**:
- The Jobs table: egui_extras paints selected with `selection.bg_fill` and hover with `widgets.hovered.bg_fill` (`egui_extras/src/layout.rs:137-149`). `palette.rs:205` and `215` set both to `raised`.
- `command_row` and `select_row` use `panel` for both states (`controls.rs:401`, `416`), and `card == panel` in dark (`palette.rs:68`).
- `pick_row` fills `panel` (`surfaces.rs:362-364`), but it sits on a slip that is already `panel`.
- `index_card` uses `raised` for both states (`surfaces.rs:549`).

**Fix, one rule** for every pickable thing:
- **Hover** is the `raised` fill.
- **Selected** is `raised` plus an ink edge: a 3px `text` bar at the row's leading edge (new `PICK_EDGE = 3.0`, the same as `WAITS_EDGE`), or a `HAIRLINE` border in `text` on an index card. The card keeps its left edge for Today's `turn` mark.
- **Focus** on a row is the 2px `active` ring, drawn inset because rows touch.
- One `pick_look(p, on, hovered, pressed)` in `controls.rs` returns the fill and edge for `select_row`, `command_row`, `pick_row`, `index_card`, `agenda_row` and the Jobs table. `selection.bg_fill` stays `raised` for text selection.

**Canvas**: `UiPass-states.png`, `UiPass-states-dark.png` (Rows, Index cards) and `UiPass-jobs.png`.
**Snapshots**: `jobs_light`, `jobs_dark`, `launcher_light`, `launcher_dark`, every `run_*` snapshot (Recent runs), `ui_kit_gallery_light`, `ui_kit_gallery_dark`.

### B4. Disabled controls go see-through, and most never say why

**What the user meets**:
- On the first run, the grid runs through the faded New task button and the faded tabs.
- The reply box's Send and the question's Send answer turn grey with no word on why.
- Setup's Check again greys out while the check runs, and its label still says "Check again".

**Where**:
- `ui.disable()` multiplies the painter's opacity (`egui/src/ui.rs:523-527`). So a kit button's opaque `card` fill becomes 50% on the desk (`controls.rs:44`, `147`), against Grid's rule that controls on the grid are opaque. You can see it in `first_run_light.png`.
- Disabled with no reason: New task and the tabs on the first run (`app.rs:344`, `349`), Send (`run_screen.rs:700`), Send answer (`778`) and Check again (`workspace/ui.rs:253`).
- The quick replies (`run_screen.rs:674`) and Run (`launcher.rs:128-134`) already say why. They are the model.

**Fix, one rule**: a kit control paints its disabled look itself, at full opacity:
- the opaque rest fill (`card` on the desk, clear on a surface);
- ink and border at the new `DISABLED_OPACITY = 0.5`;
- a primary's fill at 50% over the surface;
- the arrow cursor.

Every disabled control also gets `on_disabled_hover_text`:
- New task and tabs: "Choose your job search folder first."
- Send: "Type a reply first."
- Send answer: "Pick an answer for each question."
- Check again shows "Checking…" while it runs.

**Canvas**: `UiPass-states.png` (Disabled column, "Disabled says why, done says so").
**Snapshots**: `first_run_light`, `run_turn_light`, `run_turn_dark`, `run_turn_1040_light`, `run_question_light`, `ui_kit_gallery_*`.

### B5. A screen whose file failed shows its empty state under the alert

**What the user meets**: when the tracker cannot be read, Applications shows the alert and then "No applications yet. The tracker appears when /apply records your first application" with an /apply button. The alert says the file is unreadable, and the body says there is nothing. Jobs says "No jobs wait here. Search your job sites for new ones." Insights shows zeros and "No applications sent yet."

On these screens the alert also sits above the screen's title, while Today puts it under its head.

**Where**:
- A failed source loads as the empty default (`shell/snapshot.rs:49-56`).
- Each screen then draws its empty state: `tracking/ui.rs:118-126`, `discovery/ui.rs:251-254`, `insights/ui.rs:149-151`, `164-166`.
- The alert goes above the head at `app.rs:439-441`, and under it on Today at `intray/ui.rs:124`.

**Fix, one rule**:
- The shell tells each screen which of its sources failed.
- A failed screen draws its head, then the alert, then a "hidden, not gone" panel in place of the empty state, with no command to start. The panel says "Your applications are hidden, not gone" and "Jobdesk changed nothing in your folder. The board comes back by itself once the tracker file is fixed, because Jobdesk watches it."
- The subtitle says "Not shown while the tracker cannot be read."
- The alert moves under the head on every screen, as on Today.

**Canvas**: `UiPass-applications-failed.png`.
**Snapshots**: new `tracker_failed_light`, `jobs_failed_dark` and `insights_failed_light`. No existing snapshot changes.

### B6. Every screen but Today shows a false empty state while the folder is first read

**What the user meets**: while Today says "Reading your job search…", any other tab answers. Jobs says no jobs wait, Applications says "No applications yet", Insights shows zeros and Setup shows an empty profile, until the first read lands.

**Where**: only Today checks `loaded` (`app.rs:444`). Jobs, Applications, Insights and Setup draw from the default snapshot (`app.rs:463-499`).

**Fix, one rule**: the shell passes `loaded` to every screen. Before it, each screen draws its head with the subtitle "Reading your …" and the shape of its body in `skeleton` and `card_skeleton` blocks. That means the Jobs table rows, the Applications lanes with blank cards, the Insights band and columns, and the Setup slips. Lane names and table heads are fixed text, so they show right away. Actions stay live, because they only open the launcher.

**Canvas**: `UiPass-applications-loading.png`.
**Snapshots**: new `jobs_loading_light`, `tracker_loading_light` and `insights_loading_dark`.

### B7. Keyboard focus lands on invisible targets in the Jobs table and on cards

**What the user meets**: pressing Tab through Jobs moves through about 8 invisible stops per row, one per cell. Enter on any of them picks the job, but nothing shows where focus is. On Applications, Tab reaches each card with no ring. In Recent runs, focus has no ring either.

**Where**:
- egui_extras gives each cell its own clickable `Ui` (`egui_extras/src/layout.rs:218`, with the table's `.sense(Sense::click())` at `discovery/ui.rs:168`).
- `index_card` (`surfaces.rs:542-566`) and `pick_row` (`surfaces.rs:359-371`) never call `focus_ring`.

**Fix, one rule**:
- A pickable row is one interaction: a kit `pick_table` or a `table_row` helper senses click once on the row rect, leaves cells at hover only, draws the inset focus ring and names the row for screen readers. For a job that name is "Tech Lead Java, Real-Time Traffic, Meridian Rail Systems, 84".
- `index_card` and `pick_row` go through the `interactive` helper from B1.

**Snapshots**: none at rest.
**Tests**: Tab from the Shortlist segment reaches the first job in one press. The second job is one press further.

### B8. The Now row in Coming up looks clickable and does nothing

**What the user meets**: the row marked Now lights up under the pointer (and will show the hand after B1), but clicking it changes nothing.

**Where**:
- `agenda_line` senses a click unless the row is Done (`tray.rs:211`).
- A click calls `Pressed::Bring` (`intray/ui.rs:600-603`). For the card already on top, `pull_forward` only reads it again (`intray/application/in_tray.rs:398-403`), which is a no-op.

**Fix**: `AgendaEnd::Now` senses hover only, like `Done`. It gets no fill and keeps the arrow. It still names itself "…, now" for screen readers.

**Canvas**: `UiPass-states.png` (Coming up).
**Snapshots**: none.
**Tests**: extend `a_done_agenda_row_cannot_be_clicked` (`tray.rs:360`) with the Now row.

## Gaps: missing states and affordances

### G1. Jobs: the actions for the picked job are below the fold

**What the user meets**: at the default 1280x820, the job detail ends at "Why it fits" (`jobs_light.png`). Draft application…, Find skill gaps… and Open job ad sit under the fold of a scroll area with no visible bar. This is the problem Review.md #2 fixed on Today.

**Where**: the actions are the last block of the scrolled detail (`discovery/ui.rs:138-143`, `316-325`).

**Fix, one rule**: generalize Today's pinned footer into a kit `paper_with_footer(ui, body, footer)` in `surfaces.rs`. It is the `deal` layout without the tab and stack edges, using `KeptHeight` and `deal_footer` (`tray.rs:68-126`, `141-151`). The body scrolls, and the actions stay pinned under a hairline. The Applications detail slip uses the same widget.

**Canvas**: `UiPass-jobs.png`.
**Snapshots**: `jobs_light`, `jobs_dark`, `tracker_*` (only if a card is selected in the test).

### G2. No control has a pressed state

**What the user meets**: a press changes nothing until release, so a slow click feels unregistered.

**Where**: none of the kit's paint code reads `is_pointer_button_down_on()`: `controls.rs:73`, `149`, `349`, `401`, `416`, `tray.rs:213`, `surfaces.rs:362`, `549`.

**Fix**, inside B1's `interactive` helper:
- Pressed is one step deeper than hover: the `line` fill for plain and danger buttons, and `muted` for the primary.
- A pressed tab shows the `mark` swipe without its rule, a preview of the highlighter.
- A pressed segment takes `raised`.
- A pressed link turns `text`.

**Canvas**: `UiPass-states.png`, Pressed column.
**Snapshots**: `ui_kit_gallery_*`, if the gallery adds a pressed row.

### G3. Links come in two styles, with small targets and no focus ring

**What the user meets**:
- "Show the command" is blue with no underline (`controls.rs:212-213`).
- "Bring them back now" is body ink, always underlined (`intray/ui.rs:479`).
- Both are about 17px tall.
- In the clear panel the link sits in a muted sentence, where colour alone would not mark it.

**Fix, one widget**: a new kit `link(ui, text) -> Response`:
- ink `active`, underlined at rest with a `HAIRLINE`;
- a `MARK_RULE` (2px) underline on hover, `text` ink when pressed;
- a hit rect at least 24px tall (new `MIN_HIT = 24.0`);
- the focus ring and the hand.

`reveal_command` and the clear panel both call it.

**Canvas**: `UiPass-states.png`, Tabs, segments and links.
**Snapshots**: every `intray_*` snapshot that shows "Show the command", and `intray_set_aside_dark`.

### G4. Truncated text with no way to read it

**What the user meets**: in Coming up, 3 of 4 rows are cut at the default size ("Harbor Bank deadli…", "Tessel Group technical int…", "Meridian Rail Systems clo…" in `intray_light.png`), and hovering shows nothing. The same goes for a command's summary or output files in the launcher at 1040, for band values in a short window, and for segment labels.

**Where**: `tray.rs:238`, `controls.rs:422-425`, `surfaces.rs:693`, `controls.rs:345`. These widgets lay text out with `line()` and paint it, so egui's own elided-label tooltip never runs. Labels added through `add_cut_text` already get one.

**Fix, one rule**: whenever a kit widget paints a galley that `line()` elided, it adds `on_hover_text` with the full text. The check is one place, in `paint_text`'s callers via a `cut_hover(response, &galley, text)` helper.

**Canvas**: `UiPass-states.png` (Coming up, Hover).
**Snapshots**: none.

### G5. Keyboard reach

| Where | Gap | Fix |
| --- | --- | --- |
| Quit dialog, `app.rs:310-321` | Esc does nothing. No button has focus. | Esc means Keep working, and Keep working takes focus, as End session's popover does (`run_screen.rs:334-337`). |
| Launcher, `launcher.rs:135-139` | Enter in a field does nothing. Esc does not leave. | Cmd+Enter (Ctrl+Enter) runs when Run is enabled, and the Run button shows the keys. Esc goes back to the screen the launcher came from. |
| Jobs list | No arrow keys. | Up and Down move the pick while no field has focus. |
| Applications detail, `tracking/ui.rs:129-134` | Once open, it cannot be closed. Esc does nothing. | Esc or clicking the picked card again closes it. |
| Step lines, `surfaces.rs:754-779` | Only the small "Show N actions" button toggles. | See G6. |

### G6. A step's line does not open its actions

**What the user meets**: on Tasks, clicking a step's name or sentence does nothing. Only the small "Show 3 actions" button on the right opens it.

**Where**: `step_line` adds the toggle as its own button and the rest as labels (`surfaces.rs:754-779`, `run_screen.rs:486-504`).

**Fix**: when a step has a toggle, the whole line is its hit area: one click sense over the line, the hand, `raised` on hover. The button stays as the visible label of the state.

**Snapshots**: none at rest.

### G7. Nothing shows that a column scrolls, and at 1040 the Jobs table hides its last columns

**What the user meets**: the scroll bars only appear under the pointer, and the fade at the edge is off (`palette.rs:178`), so the Jobs detail, the launcher list, Setup and Insights look cut off rather than scrollable. At the 1040 minimum width the Jobs table needs about 784px of columns (`discovery/ui.rs:170-177`) in about 636px. The Found on column and the "Waiting in Today" square scroll off to the right with no visible bar.

**Fix**:
- Set `style.spacing.scroll = ScrollStyle::thin()`, so a thin bar is visible whenever content overflows. In egui 0.37 the default `floating()` style has `dormant_handle_opacity: 0.0`.
- Below 1100px, the Jobs table drops the Found on column. It is in the detail already.

**Snapshots**: every snapshot with an overflowing scroll area: `jobs_*`, `launcher_*`, `workspace_*`, `insights_*`, `run_*`.

### G8. Empty states are built four ways, and two offer a raw command

| Screen | Now | Proposed |
| --- | --- | --- |
| Tasks, `run_screen.rs:271-280` | "No runs yet. Press New task to start one." No button. | "No tasks yet", a sentence on what a task is, then Find new jobs… (primary) and New task… |
| Applications, `tracking/ui.rs:121-123` | `/apply` as a typed command button | Draft an application… |
| Insights, `insights/ui.rs:164-166` | `/upskill` as a typed command button | Find skill gaps… |
| Jobs, `discovery/ui.rs:247-257` | Already a verb button | Unchanged |

**Fix, one widget**: a kit `empty_state(ui, title, body, actions)` on paper: a `SLIP_TITLE` heading, a `BODY` muted sentence of at most 560px, and a row of command actions in plain words.

**Canvas**: `UiPass-tasks-empty.png`.
**Snapshots**: new `runs_empty_light`; `tracker_*` and `insights_*` only in their empty tests.

### G9. Copy gives no sign that it worked

**What the user meets**: the follow-up draft's Copy button (`dossier/ui.rs:545-547`) copies silently.

**Fix**: the button reads "Copied" for 2 seconds, then "Copy" again. It is the same width, so nothing moves. Keep it in the kit as `copy_button(ui, text)`, so any later Copy behaves the same.

**Canvas**: `UiPass-states.png`.

### G10. Marks with no words on hover

**What the user meets**: the pink "Waiting in Today" square in the Jobs table has no column head and no tooltip. The failed square on a tab does not say what failed. "+1" next to a Jobs signal does not say what the other one is.

**Where**: `marks.rs:261-266` (`stamp_square`), `controls.rs:314-317` (the tab's failed square) and `discovery/ui.rs:216-218` ("+N").

**Fix, one rule**: a mark that has a spoken label shows the same words on hover:
- "Waiting in Today";
- "Applications could not be read";
- "+1" lists the rest, for example "Also: Language unclear".

### G11. Applications: no detail until a click, and no hint

**What the user meets**: the board fills the screen, and nothing says a card opens details, where Jobs always shows the first job (`tracking/ui.rs:129-134`).

**Fix**: after B1 the hand already hints at it. Also add "Pick an application for its details and dossier." to the legend line under the board, which sits there whenever an application waits in Today. Do not pre-select a card, because the side slip would cost a lane's width.

## Consistency

| # | The same thing, two ways | Where | One way |
| --- | --- | --- | --- |
| C1 | Actions that open the launcher end in "…", except these | `insights/ui.rs:17-18` (Find skill gaps, Rebuild dashboard), `insights/ui.rs:80` (Tune scoring), `dossier/ui.rs:76-77`, `171` (Prepare interview, Record a result), `workspace/ui.rs:85`, `152-153`, `279`, `288`, `292` (Update, Check portal health, Add portal, Add template, Check now, Sync now) | `command_action` and its siblings add the "…" when the label lacks it, so every call site follows. A kit test checks it. |
| C2 | One action, two names | "Open job ad" (`discovery/ui.rs:323`, `tracking/ui.rs:278`, the Today card) vs "Open posting" (`dossier/ui.rs:75`). "Prepare for the interview…" (`tracking/ui.rs:277`) vs "Prepare interview" (`dossier/ui.rs:76`, `171`). | "Open job ad" and "Prepare for the interview…" everywhere. |
| C3 | The menu says New task and the screen it opens says All commands | `launcher.rs:54` | Title the launcher "New task", with the subtitle "Pick a command. Nothing runs until you press Run." |
| C4 | Setup repeats "Look for more in your documents…" on six rows, and each opens the same `/expand` with no argument | `workspace/ui.rs:84` | One action in the Profile header, or pass the row's file to `/expand`. |
| C5 | The load alert sits above the head on four screens and under it on Today | `app.rs:439-441` vs `intray/ui.rs:124` | Under the head everywhere (B5). |
| C6 | Hover fills differ by widget: `raised` on agenda rows and cards, `panel` on command, pick and select rows | B3 | One `pick_look` (B3). |

## Already right

- Text fields show the I-beam, and nothing else will after B2.
- Disabled widgets never report hover, so they never light up.
- The quick replies and Run say why they wait.
- End session's popover takes focus and closes on Esc.
- Markdown links open only on click and only for `http` and `https`. B2's `copyable` scope keeps that, because selection does not change link handling.
- No control is icon-only, so no icon button lacks a tooltip.

## Proposed widget and token changes

| Kind | Name | Purpose | Tokens |
| --- | --- | --- | --- |
| Theme | `install_theme` | `selectable_labels = false`; `spacing.scroll = ScrollStyle::thin()` | none new |
| New helper | `interactive(ui, &response, radius)` in `controls.rs`, replacing `focus_ring` | The hand when hovered and enabled, the pressed fill, the focus ring | `FOCUS_RING`, `FOCUS_OFFSET`, `line`, `muted`, `active` |
| New helper | `pick_look(p, on, hovered, pressed)` | One fill and edge for every pickable row and card | `raised`, `text`, new `PICK_EDGE` |
| Changed | `press_as`, `shortcut_button`, `folder_tab`, `segment` | Paint their own disabled look at full opacity | new `DISABLED_OPACITY` |
| Changed | `pick_row`, `select_row`, `command_row`, `index_card`, `agenda_row` | `pick_look`, `interactive`; Now takes no click; cut text shows on hover | `PICK_EDGE`, `HAIRLINE` |
| New widget | `link(ui, text)` | One link: underlined, 24px hit, focus ring | `active`, `text`, `HAIRLINE`, `MARK_RULE`, new `MIN_HIT` |
| New widget | `paper_with_footer(ui, body, footer)` | A scrolled body with pinned actions, from `deal` | `card`, `line`, `DEAL_FOOT_PADDING` |
| New widget | `empty_state(ui, title, body, actions)` | One empty state for every screen | `SLIP_TITLE`, `BODY`, `muted` |
| New widget | `copy_button(ui, text)` | Copy, then "Copied" for 2 seconds | `SMALL_BUTTON_*` |
| New scope | `copyable(ui, add)` in `text.rs` | Turns text selection back on for documents, code boxes, the archived posting and drafts | none |
| New helper | `pick_table` or `table_row` | One click and focus target per table row, cells hover only | `PICK_EDGE`, `raised` |
| New tokens | `DISABLED_OPACITY = 0.5`, `PICK_EDGE = 3.0`, `MIN_HIT = 24.0` | Measures only. No new colour, font or radius. | |

## Doc changes for the lead

**ARCHITECTURE.md**, in the `ui_kit` paragraph, after "screens only arrange its widgets":

> Every clickable kit widget shows the pointing hand, a hover, a pressed and a focus state, and paints its own disabled look with the reason on hover. A picked row or card adds an ink edge to the hover fill. Labels are not selectable, so a click on a row's text picks the row. Only documents, code boxes, the archived posting and drafts can be selected and copied, and none of them sits inside something clickable.

**ARCHITECTURE.md**, in Runtime, after "A feature that fails to load reports an issue and the others keep working.":

> Until the first snapshot arrives, every screen shows a loading view, not an empty one. A screen whose source failed shows the alert under its head and says its content is hidden, never that it is empty.

**README.md**, after the Today paragraph:

> Click anywhere on a job, an application or a run to open it. Text in Claude's messages, command lines, archived postings and follow-up drafts can be selected and copied. Copy says Copied.

And, if G5 is taken:

> In the launcher, Cmd+Enter (Ctrl+Enter on Windows) runs the task, and Esc goes back.
