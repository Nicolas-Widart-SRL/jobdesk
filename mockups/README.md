# Mockups handoff: In tray horizon indicators

## Brief

The user wrote: "can we make this clear on the dates fields, there is no number or indicator."
Their tray had 7 decisions, all "No deadline", so all 14 cells of the horizon strip were blank.
They had also pressed Later on one card and could not find it again.
Today the strip draws only `Mark`s for dated cards in the window (`queue()` in `src/intray/application/in_tray.rs`, `horizon()` and `horizon_mark()` in `src/intray/ui.rs`).
A card set aside comes back only through "Bring them back" on the end panel, or through its own dated mark.

The user approved these indicators:

1. **Tomorrow's cell** shows the cards set aside, for example "1 back". It names them on hover, and a click brings them all back now (`bring_back_all`).
2. **Today's cell** shows the pending count, "7 to decide", with a split by kind under it.
3. **Today's cell** adds "N overdue" for pending cards whose due date is before today. Those cards rank first but fall off the strip. (Round 2)
4. **The empty days** show one quiet line when no dated card falls in the window (`HORIZON_DAYS = 14`, the offsets `0..14` in `Card::is_on_horizon`).

Out of scope: the card face, stack, undo bar, end panel, ranking and the other screens.

## Canvas

`TrayHorizon.html` is a copy of `Tray.html` with only the strip redrawn. It uses `app.css` and `support.js`.
Choose a scene with `?scene=A|B|C|D|E` (or the `scene` prop) and a theme with `?theme=light|dark`. Today is Saturday 31 October. All data is fictional.

| PNG (1280x820) | Scene | What it shows |
| --- | --- | --- |
| `TrayHorizon-A.png` | A, light | The user's case: 7 pending, all undated, 1 set aside. Today: "7 to decide / 5 triage / 2 send". Sun 1: a "1 back" chip. Days 3 to 14: "No deadlines in the next 14 days". |
| `TrayHorizon-A-dark.png` | A, dark | The same case in dark. |
| `TrayHorizon-B.png` | B, light | Mixed: 8 pending, 2 set aside, with dated marks on Sun 1, Wed 4 (two marks), Thu 5, Mon 9 (decided, struck through) and Thu 12. Today: "8 to decide / 4 triage / 4 other". Sun 1 holds the "Harbor deadline" mark and a "2 back" chip with no crowding. One of the two cards set aside (Meridian) is dated, so its Thu 5 mark stays active, as it does today. There is no empty-days note, because marks exist. |
| `TrayHorizon-C.png` | C, light | Scene B with the pointer on "2 back". The chip's border turns `text`, and a tooltip shows "Set aside with Later", one line per name (Federal Data Agency, Meridian Rail Systems), then "Back in the tray tomorrow. Click to bring them back now." |
| `TrayHorizon-D.png` | D, light | The tray is clear but 1 card is set aside. Today shows a muted "0 to decide", and the "1 back" chip and empty-days note still show. The end panel is unchanged. |
| `TrayHorizon-E.png` | E, light | Scene A plus one overdue card: Juniper Retail, whose deadline passed 2 days ago, is on top. Today: "7 to decide / 1 overdue / 5 triage / 2 send". The note reads "No other deadlines in the next 14 days". |
| `TrayHorizon-E-dark.png` | E, dark | The same, as a check of the warning ink on the dark `raised` cell. |

States not drawn, and why:

- **Loading**: none. The strip derives from the `Snapshot` on every frame.
- **Error**: a decisions file that cannot be read already shows `alert` above the strip, and the strip itself does not change.
- **Feature failed to load**: none of its own. With no queue, the strip is scene D without the chip.
- **Theme**: the screen does not differ between themes beyond tokens, so A and E are drawn in dark only as checks.

## Rules for the builder

- **Order within a cell**: day label, then dated marks, then the count block (today) or the return chip (tomorrow).
- **Today's count block**, top to bottom:
  1. "N to decide" in `Type::TAG` with `Ink::Text`. With 0 pending it switches to `Ink::Muted`.
  2. "N overdue", only when N > 0, in `Type::TAG` with `Tone::Interview`. That is Ledger's warning ink, the token `install_theme` already gives `warn_fg_color`. It counts pending cards whose `due.on` is before `queue.today`. It is not clickable, because those cards already rank first.
  3. Kind lines in `Type::SMALL` with `Ink::Muted`. Each line is "count kind", using the `kind_tag` label in lowercase ("triage", "send", "follow-up", "guard flag").
- **Kind lines**:
  - They count every pending card, so overdue cards are counted in their kind too.
  - Kinds sort by count, largest first, with ties in `CardKind` order.
  - There are at most 2 kind lines. With more than 2 kinds, the second line folds the rest into "N other" (scene B).
  - "5 triage · 2 send" on one line does not fit: a cell is about 87px wide at 1280, with 73px inside. That is why the split is stacked.
  - The count is not clickable, because the stack below already holds those cards.
- **Fit with the overdue line (decision)**:
  - I kept 2 kind lines rather than dropping to 1.
  - Every line fits the 73px width. The widest, "7 to decide", is about 66px, and "12 overdue" would be about 62px.
  - The overdue line adds one line of height (about 18px) to the strip, only while something is overdue. Scene E's strip is 106px tall. That is no taller than any day with two wrapping marks (scene B, Wed 4, is 112px).
  - With 1 kind line, a tray of several kinds would need a fold like "7 cards", which says nothing new. A single line like "5 triage, 2 send" does not fit.
- **Tomorrow's chip**:
  - It shows only when `queue.set_aside` is not empty. The text is "{set_aside.len()} back".
  - A click sends `Pressed::BringAll`, so it uses the same path as the end panel's button.
  - It starts no run. It changes only the queue, so the hard rule on card actions holds.
- **Tooltip**: use `on_hover_ui`. The names come from `set_aside_names()`, one per line, so two cards from one company show one name. End with the foot line: "it" for 1 card, "them" for more. Names are card names drawn from postings, so render them as plain `Label`s only (see [Trust](../ARCHITECTURE.md#trust)).
- **Empty-days note**:
  - It shows when `queue.marks` is empty. A struck-through mark still counts, so the note never sits on top of one.
  - It reads "No deadlines in the next 14 days". When anything is overdue it reads "No other deadlines in the next 14 days", so it never contradicts the overdue line. "Upcoming" was rejected: "upcoming in the next 14 days" says the same thing twice and still ignores the overdue card.
  - Paint it after the columns, centred on columns 2 to 13 (the third to the last day) and vertically centred in the strip.
  - Fill a `panel` rect of the text size plus `HORIZON_NOTE_PADDING` (14 by 7) behind it. Each cell hairline then stops 7px above and below the text, so the line reads as a label on the strip, not text struck by rules.
- **Accessibility**: the chip's `WidgetInfo` label is "2 set aside, back tomorrow: Federal Data Agency, Meridian Rail Systems. Bring them back now."

## New widgets (`src/ui_kit/tray.rs`)

| Widget | Purpose | Tokens |
| --- | --- | --- |
| `horizon_count(ui, count, overdue, kinds: &[(usize, &str)])` | Today's pending count, the overdue line and up to two kind lines | `Type::TAG` with `Ink::Text`, or `Ink::Muted` at 0; `Type::TAG` with `Tone::Interview` for overdue; `Type::SMALL` with `Ink::Muted`; the cell's existing 3px gap |
| `horizon_return(ui, text) -> Response` | The clickable chip on tomorrow's cell for cards set aside | Fill `sheet`, `HAIRLINE` stroke in `line` (`text` on hover), `RADIUS_CARD`, `Type::TAG` with `Ink::Text`, new `HORIZON_CHIP_HEIGHT = 20.0` and `HORIZON_CHIP_PADDING = 6.0` |
| Note on the `horizon` strip | One muted line across the empty days | A `panel`-filled rect painted above the cell hairlines, `Type::SMALL` with `Ink::Muted`, new `HORIZON_NOTE_PADDING: (f32, f32) = (14.0, 7.0)`. `horizon()` needs an `Option<&str>` note argument, because only it knows the column rects. |

The tooltip reuses egui's popup, which `install_theme` already styles: `sheet` fill, a `text` hairline, `RADIUS_PANEL` and the `WINDOW_SHADOW` offset in `line`. A tooltip width of about 250 matches the canvas.
The overdue count needs no new application state: `Card.due` is public, so the UI can count from `queue.cards`. A `Queue::overdue()` helper beside `headline()` would keep the rule in one place.

## Proposed doc changes (for the lead)

- **`ARCHITECTURE.md` line 59** (the Later paragraph), replace "Its mark pulls it back to the top." with:
  > Its mark pulls it back to the top. Tomorrow's cell on the horizon counts the cards set aside, names them on hover, and brings them all back when clicked.
- **`ARCHITECTURE.md` line 57** (the queue paragraph), add after "then the rest.":
  > The horizon strip shows the 14 days from today. Today's cell counts the pending cards by kind and the ones past their deadline, which rank first but fall off the strip. When no dated card falls in the window, the strip says so, so no cell is silently blank.
- **`ARCHITECTURE.md` Features table, `intray` row**: no change. The counts derive from `Queue.cards` and `Queue.set_aside`, which already exist.
- **`README.md` line 10**, replace "with paused runs first, then deadlines, then the rest." with:
  > with paused runs first, then deadlines, then the rest. The strip above it counts what is left to decide today, and what is overdue.
- **`README.md` line 10**, replace "L sets the card aside until tomorrow," with:
  > L sets the card aside until tomorrow (the horizon's next day shows how many, and a click brings them back),

## Changes in round 2

- **Overdue**: a new "N overdue" line in today's cell, the new scene E (light and dark), and the conditional note wording. The answered open question is removed.
- **Note mask**: `.hz-none` is now a padded block (7px by 14px, `background-color: var(--panel)`, `z-index: 2`), so the hairlines stop well clear of the text. Before, the mask was only as tall as the text, so the line read as crossed out at normal zoom. I re-rendered A, A-dark and D and checked each at full size.
- **`Tray.html` line 380**: the Lumen follow-up draft now signs off "Robin Vasquez". No other canvas has the real name.

## Changes in round 1

- **`app.css`**: added the `.hz-count`, `.hz-late`, `.hz-kind`, `.hz-back`, `.hz-tip`, `.hz-none` and `.tbox.lift` classes. I also fixed a selector-strength bug: `.app button` outranked `.mark` and `.c-*`, so the horizon marks in `Tray.html` drew in the plain text colour and regular weight. The new `.hz .mark` rules give them the `TAG` face and kind tone that `horizon_mark` uses in the app, so `Tray.html` now matches the app too.
