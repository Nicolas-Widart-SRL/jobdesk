# Mockups handoff

One Runs brief and two In tray briefs. The newest comes first.

## 1. Runs: where am I, and is it my turn?

Final handoff. The design is **D, the Turn band**, with the **Stamp magenta** accent, the **Story** layout, and no status tag in the page head. A, B and C were explored and not chosen. They are kept at the end of this section for reference.

### Brief

The user wrote: "Innovate on the Runs page, focus on user experience. It's not clear where we are in the run progress. It happened a few times where I ended the run because I didn't know I had to reply something."

What goes wrong today, traced in `src/runs/ui/run_screen.rs`:

1. **The stop point is invisible.** At an `/apply` stop, `RunStatus::AwaitingReply` reads "Ready for reply", in the same tag style as a running run. The only cue is one muted line and a one-line field under a long Markdown evaluation. Claude's question is the last paragraph of that message, so "End session" in the page head looks like the next step.
2. **No step is lit at the stop.** `step_rail(..., live = is_busy() || needs_user())` is false at a stop point.
3. **The rail can jump ahead.** Substring inference can match the wrong rule. A builder fixes that separately, so this design assumes the rail knows the right step.
4. **The log does not say where we are.** It is flat, with long absolute paths. People who don't code cannot read it, while Claude's Markdown message, which they need, is buried in it.
5. **Nothing outside the page.** `RunDesk::waiting_count` and `paused_card` count only pending prompts, so a run at a stop point gets no tab count and no In tray card.

In the user's screenshot the side column was pushed half off-screen, and a Markdown `#` heading drew at full H1 size. Nothing from the screenshot is used: the canvas uses a fictional company (Marlowe Systems), role and URL.

### How the design was reached

- **Round 0**: four proposals. A was a reply dock, B step chapters, C conversation and work, and D the Turn band. The user chose D.
- **Round 1**: D was reworked on three points.
  - The amber "Your turn" became an accent of its own. Three candidates were compared, and Stamp magenta won.
  - Claude's message became the main content, with tool rows folded into one plain line per step. Two arrangements were drawn: Report and Story.
  - The page head was aligned on one centre line.
- **Final**: the user approved magenta and Story, and dropped the head's status tag, because the band's Whose turn cell already says it. The Report variant is removed from the canvas, and its PNGs are deleted.

### Token plan

- **Colour**: Ledger stays as it is, plus one new pair used only when the run needs you.

  | Token | Light | Dark | Role |
  | --- | --- | --- | --- |
  | `sheet` | #eff3f8 | #13243a | The page and the document |
  | `panel` | #dfe7f0 | #1a2f49 | The band and the side slips |
  | `text` / `primary` | #12263f | #e6edf5 | Ink and the Send button |
  | `active` | #1657a8 | #8cc2f7 | Claude's turn |
  | `turn` (new) | #b0246a | #f48cc0 | Your turn: the Whose turn cell, the question bar's frame, the tally mark, the waiting step's knot, the Runs tab mark, the Recent runs tag and the In tray card |
  | `on_turn` (new) | #ffffff (the `page` value) | #0d1a2b (the `bg` value) | Text on `turn` |

- **Type**: the Ledger faces, each with one job.
  - Zilla Slab: band values and document headings, capped at 21px for `#` and 17px for `##`.
  - Schibsted Grotesk: the UI, and the document body at 15px, at most 68 characters a line.
  - Courier Prime: only the command line and the technical rows behind "Show N actions".
- **Layout**: left-aligned with a ragged right edge, one centre line per row.

  ```
  /apply jobs.marlowe.example/postings/…                         [End session…]   one centre line
  ┌ Where ▮▮▮▯▯▯▯ ─┬ Whose turn ───┬ What next ──────────────┐
  │ 1 Evaluate,done│ Yours  (turn) │ Answer Claude           │   label, value, note, aligned across cells
  └────────────────┴───────────────┴─────────────────────────┘
  ┌ Claude asks: Shall I go ahead and draft …?                       ┐
  │ [Go ahead with the draft] [Stop here] [reply………………] [Send ↵]    │   one 34px row
  └──────────────────────────────────────────────────────────────────┘
  ┌ The run so far ───────────────────────────────────────┐ ┌ This run ──┐
  │ (0) Fetch     Fetched the posting…   [Show 3 actions] │ │ Recent runs│
  │ (1) Evaluate  Read your profile…     [Show 3 actions] │ └────────────┘
  │ Job fit evaluation: …   the document, 68ch            │
  │ You  "Yes, proceed with drafting."                    │
  │ (2) Draft     Wrote the CV. Writing the letter now.   │
  └───────────────────────────────────────────────────────┘
  ```

- **Principles**:
  1. **One loud thing.** The turn colour appears only when you are needed.
  2. **A report, not a log.** Claude's message is the main content.
  3. **Plain words first.** Technical rows open only on request.
  4. **Say each thing once.** The band says whose turn it is, so the head carries only the title and the actions.

### Accent: Stamp magenta

`RunTurn-D-accents.png` compares three candidates in both themes. ΔE is CIEDE2000 against every Ledger tone in the same theme, including the Interview amber that stays in use for deadlines and guard flags.

| Candidate | Light fill / text | Contrast | Nearest tone | Dark fill / text | Contrast | Nearest tone |
| --- | --- | --- | --- | --- | --- | --- |
| **Stamp magenta (chosen)** | #b0246a / #ffffff | 6.4:1 | Closed, ΔE 24 | #f48cc0 / #0d1a2b | 7.8:1 | Closed and Offer, ΔE 20 |
| Highlighter lemon | #f7ea3c / #12263f | 12.2:1 | Interview, ΔE 50 | #e9ef5c / #0d1a2b | 14.1:1 | Interview, ΔE 18 |
| Signal cyan | #00788f / #ffffff | 5.1:1 | Drafted, ΔE 15 | #3fe0f2 / #0d1a2b | 11.0:1 | Drafted, ΔE 18 |

- **It fills a gap.** Magenta is the one hue the navy and slate palette lacks, so it stands out in both themes without reading as a job status.
- **The fill holds its own edge.** It has 5.7:1 against the light sheet and 7.0:1 against the dark one. The lemon fill is 1.1:1 on the light sheet and reads as a warning. Cyan sits among the cool tones.

### Canvas and PNGs

`RunTurn.html` uses `app.css` and `support.js`. Open D with `?proposal=D&scene=…&theme=light|dark`. The PNGs sit beside it, at 1280x820 unless noted.

| PNG | State |
| --- | --- |
| `RunTurn-D-story-working.png` | (a) Claude works in Draft. The band reads "2 Draft / Claude's / Nothing for now", and the head holds only Stop run. The story ends on the evaluation, your reply and the live Draft line with "Show 3 actions so far". |
| `RunTurn-D-story-details.png` | (a) with Draft's actions open: "Hide actions", then today's rows with short paths and Done tags. |
| `RunTurn-D-story-turn.png` | (b) Stopped after Evaluate, your turn. The Whose turn cell is magenta, the question bar has quick replies, the field and Send, and the view opens at the top of the evaluation, with its gates table. The head holds only End session…. |
| `RunTurn-D-story-turn-1040.png` | (b) at 1040x650, the user's window size. Run details replaces the side column and sits beside End session… on the title's centre line. The band is one line, and the question, quick replies and Send are all in view. |
| `RunTurn-D-story-turn-dark.png` | (b) in dark. |
| `RunTurn-D-story-ask.png` | (c) AskUserQuestion in Draft: the two choices side by side in the question bar, and the Draft line in magenta with "Show 4 actions so far". The head holds only Stop run. |
| `RunTurn-D-story-finished.png` | (d) Finished: seven one-line steps, then Claude's closing summary. The head holds only Open dossier. |
| `RunTurn-D-accents.png` | The three accents, light and dark: tab mark, tags beside the existing tones, band and question bar. |
| `RunTurn-D-tray.png` | Supporting: the In tray "Your turn" card, its tab and the Runs tab mark in magenta. It has no run head, so the final round leaves it unchanged. |

States not drawn for D, and why:

- **Starting, failed and stopping**: the band carries them (see Rules). B's `RunTurn-B-starting.png` and `RunTurn-B-failed.png` show the same data.
- **Empty and history unreadable**: unchanged from round 0 (`RunTurn-empty.png`, `RunTurn-B-nolog.png`).

### Rules for the builder

- **Head**:
  - The command line on the left, cut to fit with the full line on hover. On the right, only the actions, all 28px small buttons on the title's centre line:

    | State | Action |
    | --- | --- |
    | Working, or a prompt pending | Stop run |
    | Stopping | Force stop |
    | `AwaitingReply` | End session…, which asks first, as `RunTurn-B-confirm.png` draws |
    | Finished | Open dossier, when the run made one |
    | Window under 1100px wide | Run details, a popup with This run and Recent runs |

  - There is no status tag in the head.
- **Band**: three cells, each a label, a value and a note, lined up across cells.

  | State | Where | Whose turn | What next |
  | --- | --- | --- | --- |
  | Starting | "0 Fetch", note "Starting the CLI" | "Claude's" in `active` | "Nothing for now" |
  | Working | "2 Draft", note "2 of 7 steps done" | "Claude's" in `active`, note "Working for 1m 50s" | "Nothing for now", note "You can leave. The Runs tab marks it when it needs you." |
  | `AwaitingReply` at a known stop | "1 Evaluate, done", note "Draft waits for your go" | "Yours" on a `turn` fill, note "Claude stopped 2 min ago" | "Answer Claude", note "Its question is right below." |
  | Prompt pending | "2 Draft", note "Paused inside the step" | "Yours" on `turn` | "Pick a CV template" (from the question), note "The choices are right below." |
  | Stopping | as Working | "Claude's", note "Stopping" | "Nothing for now" |
  | Failed | "5 Compile, failed" in `closed` | "Nobody", note the reason as plain text | "Reply to resume", note "Or leave it. Files so far stay." |
  | Finished | "All 7 steps", note "Finished in 14m 20s" | "Nobody", note "The session is closed" | "Read the documents", note "Open the dossier, then send it yourself." |

  - The Where label holds a seven-mark tally: done marks in `text`, the working mark in `active`, the waiting mark in `turn`, the others in `off`.
  - Below 700px of window height, each cell becomes one line, label then value, with no note.
- **Question bar**:
  - It shows only at your turn, as `prompt_box` in `turn`.
  - It reads "Claude asks:" and the closing question as plain text (`Run::closing_question()`), then one 34px row: quick replies from the catalog, the field and Send. Enter sends.
  - For AskUserQuestion, the choices sit in two columns, then Send answer and Decline. AskUserQuestion always reaches the user.
  - Quick replies send their fixed words through `desk.reply`, exactly like Send. Their tooltips show the words. They are disabled while another run is busy, and they never start a run.
- **The story**, in reading order, under "The run so far" ("The run" once finished):
  - **Step lines**: one per started step. A knot (done, working, or `turn` when waiting), the step name, one plain sentence, and a "Show N actions" toggle ("so far" while live).
    - The sentence comes from a fixed phrase table in the command catalog, keyed by tool and path pattern, then "and N other actions". It never comes from Claude's text or the posting.
    - Open, the toggle shows today's rows: short paths, with Done, Running and Failed tags.
    - Steps not started yet are left out. The band's tally shows them.
  - **Claude's messages**: each top-level message is a document where it was written.
    - It renders as Markdown on the sheet: `#` as 21px slab, `##` as 17px slab, and the body at 15px, at most 68 characters a line.
    - Tables, lists and emphasis render. Images never load, and links are text that opens only on click for http and https.
    - The paragraph `closing_question()` picked gets a 3px `turn` rule on its left.
  - **Your reply**: a "You" line with your words in quotes, between the step it ended and the step it started.
  - **Scroll**: at a stop and when finished, the view opens at the top of the newest message. While Claude works, it sticks to the bottom. Once the user scrolls, their position wins.
- **Side column**: a fixed 280px with This run and Recent runs, scrolling on its own. Recent runs keeps each run's status tag: "Your turn" as a solid `turn` tag, "Claude's turn" in `active`.
- **Outside the page**:
  - The Runs tab draws an 8px `turn` square before its count. `waiting_count` also counts runs in `AwaitingReply`.
  - `intray` deals a `CardKind::Turn` card: tab "Your turn" in `turn` ink, the card frame in `turn`, title from the catalog's stop label ("/apply: proceed with drafting?"), and the closing question as plain text.
  - Its actions are Open run and Later. There is no Y and no Skip.
- **Accessibility**:
  - The band is labelled "Where the run is". Its tally reads "1 of 7 steps done".
  - The question bar's group label is "Your turn. Claude asks: …".
  - Each step toggle sets `aria-expanded`.
  - The Runs tab reads "Runs, 1 run waits for you".

### New and changed widgets

| Widget | Where | Purpose | Tokens |
| --- | --- | --- | --- |
| `Palette.turn`, `Palette.on_turn`, `Tone::Turn` (new) | `palette.rs` | The your-turn accent. The palette comment "Colour is for job status only" gains this one exception, and so does `app.css`'s header. | light #b0246a and #ffffff, dark #f48cc0 and #0d1a2b |
| `solid_tag(ui, text, tone)` (new) | `marks.rs` | "Your turn" in Recent runs, filled with its tone | `TAG_HEIGHT`, `TAG_PADDING`, `Type::TAG`, `RADIUS_CARD`, fill `turn`, ink `on_turn` |
| `page_head` (changed) | `surfaces.rs` | For a one-line `Title::Typed` head, the actions centre on the title line: `Align::Center` instead of `Align::BOTTOM` at line 106, and the title is not dropped by lines 89 to 93. The action row is no longer bottom-aligned, so mixed control heights would not show, but D uses one height. | `HEAD_GAP`, `SMALL_BUTTON_HEIGHT` |
| `small_danger_button(ui, text)` (new) | `controls.rs` | Stop run and Force stop at 28px, matching End session…, Open dossier and Run details (`small_button`): `press(ui, text, Look::Danger, SMALL, None)` | `SMALL_BUTTON_HEIGHT`, `SMALL_BUTTON_PADDING`, `Type::SMALL_BUTTON` |
| `folder_tab(…, attention)` (changed) | `controls.rs` | The Runs tab mark | `STATUS_DOT` (8) in `turn` |
| `turn_band(ui, cells: [BandCell; 3])` (new) | `surfaces.rs` | The three cells: label, value, note, one filled `turn` when yours. One line per cell under 700px of height. | `panel`, `HAIRLINE` dividers, `RADIUS_PANEL`, `NODE_PADDING`, `Type::NOTE` label, `Type::SCORE` value, `Type::BODY` note in `Ink::Muted`, `TALLY_THIN` |
| `prompt_box_tone(ui, tone, add)` (changed `prompt_box`) | `surfaces.rs` | The question bar in `turn` | `HAIRLINE`, `OFFSET_SHADOW`, `PROMPT_PADDING` |
| `document(ui, cache, text, marked_paragraph)` (new, wraps `markdown`) | `text.rs` | A message as a document: capped headings, a 68ch measure, an optional `turn` rule on one paragraph | `Type::SCORE` (21) for `#`, `Type::SECTION` (17) for `##`, `Type::STRONG` below, `Type::TAKE` body, new `DOC_MEASURE = 620.0`, new `QUOTE_RULE = 3.0` |
| `step_line(ui, knot, name, sentence, toggle) -> Response` (new) | `surfaces.rs` | One plain line per step, with its toggle | new `STEP_LINE_HEIGHT = 48.0`, new `STEP_NAME_WIDTH = 74.0`, `Type::STRONG` name, `Type::BODY`, `small_button` |
| `knot(ui, text, state)` (new) | `marks.rs` | The numbered square: done (`text` hairline), working (`primary` fill), waiting (`turn` fill), failed (`closed` hairline) | new `KNOT_SIZE = 22.0` (18 in step lines), `Type::TAG`, `RADIUS_CARD` |
| `path_label(ui, dir, file)` (new) | `text.rs` | An open action row's path: the folder in `Ink::Faint`, the file in `Ink::Text` | `Type::MONO` |

Dropped from earlier rounds:

- **`head_status`**: nothing uses it now that the head has no tag.
- **`step_column`, `chapter_head` and `quote_box`**: they belonged to B.

`step_rail` leaves the run screen, because the band's tally replaces it.

### Data the screen needs

- **`Activity.step: Option<usize>`**: the step the rail was on when the row arrived. Rows before the first match belong to step 0.
- **Phrase table** in `catalog.rs`: per command, `(tool, path pattern) -> phrase`.
- **Stop points** in `catalog.rs`: `StopPoint { after_step, label, replies: &[(button, text)] }`. `/apply` stops after step 1, labelled "proceed with drafting", with "Go ahead with the draft" sending "Yes, proceed with drafting." and "Stop here" sending "No, stop here. Do not draft."
- **`Run::closing_question() -> Option<String>`** in `application`: the last paragraph of the turn's last top-level Claude message, with Markdown markup and control, bidi and zero-width characters removed.
- **`RunDesk::waiting_count`** also counts runs in `AwaitingReply` that are not busy.
- **`intray`** deals the new `CardKind::Turn` card.
- **Per-step time** (optional): for the band's notes, from timestamps the infrastructure puts on events. `domain` stays clock-free.

### Proposed doc changes (for the lead)

- **`README.md` line 12**: replace "When Claude finishes a step, reply to continue or press End session." with:
  > A run reads like a report: Claude's messages come first, and each step is one plain sentence, with its actions a click away. A band at the top says where the run is, whose turn it is and what to do next. When it is your turn, the Runs tab shows a mark, the In tray deals a card, and Claude's question sits above the reply box, with one-click replies at known stop points such as /apply's proceed with drafting. End session asks first while Claude waits for you.

  The `## Run` section of `README.md` is about `cargo run` and needs no change.
- **`ARCHITECTURE.md` Features table, `runs` row**: Language becomes "run, activity, prompt, decision, step rail, stop point, quick reply, step phrase, command catalog".
- **`ARCHITECTURE.md` Runtime, after line 55**, add:
  > A turn that ends with no pending prompt leaves the run waiting for a reply. Like a pending prompt, that counts as the user's turn: the Runs tab marks it and `intray` deals a card for it. Each activity row keeps the step the rail was on when it arrived, so the Runs screen folds rows under their step. Each step's plain sentence comes from a fixed phrase table in the command catalog, never from Claude's text. The rail is display only and never steers the run. Quick replies are fixed text from the command catalog, sent through the same path as a typed reply, so they never start a run.
- **`ARCHITECTURE.md` line 61**: "Permission, question and guard cards have no Y action." becomes "Permission, question, guard and Your turn cards have no Y action."
- **`ARCHITECTURE.md` Trust, after "Claude's replies and prep packs render as Markdown with no image loading."**, add:
  > On the Runs screen they read as documents, with headings capped at the section size. Claude's closing question, pinned above the reply box and on the In tray card, renders as plain text with Markdown markup and control, bidi and zero-width characters removed.
- **`AGENTS.md`**: no change. The Your turn card only opens the run, so "Card actions never start a run" holds. AskUserQuestion still always reaches the user.

### Explored and not chosen: A, B and C (round 0)

The user did not choose these proposals. They stay below, and in the canvas with `?proposal=A|B|C`, for reference only. Where they differ from the final design above, the final design wins. In particular, the round-0 tables still list D's first PNGs, which are deleted, and B's "recommendation", which was not taken.

#### Canvas

`RunTurn.html` uses `app.css` and `support.js`, and the fit layout of `TrayUndo.html`: head and side column stay put, and only the activity scrolls.
Choose with `?proposal=A|B|C|D`, `?scene=working|turn|ask|finished|starting|failed|confirm|nolog|details|empty|tray|accents`, `?activity` (removed in the final round: D draws only the story) and `?theme=light|dark`.
Every proposal shows the same `/apply` run in the four required states. B also has the remaining states, a dark render and a 1040x650 render.

| PNG | Proposal | State |
| --- | --- | --- |
| `RunTurn-A-working.png` | A | (a) Claude works in Draft: the rail lights 2 Draft, a quiet "Claude's turn" line sits under the log, and the head offers Stop run. |
| `RunTurn-A-turn.png` | A | (b) Stopped after Evaluate: a "Your turn" cell sits between 1 Evaluate and 2 Draft, the dock pins Claude's question with two quick replies and a field, and End session is demoted. |
| `RunTurn-A-ask.png` | A | (c) AskUserQuestion in Draft: 2 Draft is filled in the warning ink and the dock holds the choices. |
| `RunTurn-A-finished.png` | A | (d) Finished: all steps done, a quiet line with Open dossier and Open in Tracker, and the resume field. |
| `RunTurn-B-working.png` | B (recommended) | (a) The left rail lights 2 Draft with "Working, 1m 50s". Fetch and Evaluate are one-line chapters, and Draft is open, starting with your reply. |
| `RunTurn-B-turn.png` | B | (b) The rail inserts a "Your turn" stop under 1 Evaluate. The Evaluate chapter is open with a shortened message, and the dock pins the question with quick replies. |
| `RunTurn-B-turn-1040.png` | B | (b) at 1040x650, the user's window size. The side column folds into a facts line under the title and a Recent runs button. Rail, question and reply all stay in view. |
| `RunTurn-B-turn-dark.png` | B | (b) in dark. |
| `RunTurn-B-ask.png` | B | (c) The Draft knot and chapter are in the warning ink, and the AskUserQuestion row reads "Waiting for you" above the choices. |
| `RunTurn-B-finished.png` | B | (d) Every chapter is closed except Finish, which ends with Claude's summary. |
| `RunTurn-B-starting.png` | B | Loading: Fetch is lit with "Starting", the log reads "Starting the claude CLI…", and Model and Session are not known yet. |
| `RunTurn-B-failed.png` | B | Error: the Compile knot is outlined in the closed ink, the open tool reads Failed with the budget-cap notice, and a red-ruled dock offers to resume. |
| `RunTurn-B-confirm.png` | B | (b) after a click on End session…: a popover asks first, with Keep it open as the default. |
| `RunTurn-B-nolog.png` | B | Feature failed to load: older run logs cannot be read. Recent runs shows the alert, and the live run is unaffected. |
| `RunTurn-C-working.png`, `-turn`, `-ask`, `-finished` | C | (a) to (d): the conversation on the left ends in whoever holds the turn, and the work is grouped by step on the right. |
| (deleted in round 1) | D | The round-0 D renders are replaced by the round-1 PNGs above. |
| `RunTurn-empty.png` | any | Empty: no runs yet. The note adds that the tab marks a run that needs you. |
| `RunTurn-tray.png`, `RunTurn-tray-dark.png` | supporting | The In tray card for a run at a stop point. The tab reads "Runs ■ 1" and today's horizon cell counts "1 your turn". |

All the run PNGs are 1280x820 except the 1040 render. The Runs tab mark appears in every (b) and (c) scene.

#### The four proposals

| | A. Reply dock | B. Step chapters | C. Conversation and work | D. Turn band |
| --- | --- | --- | --- | --- |
| Idea | Keep today's page, and make the reply box the hero when the run waits. | The rail stands on the left, the log folds into one chapter per step, and the turn docks under the open chapter. | Split what was said from what was done, so the conversation always ends with whoever holds the turn. | A band at the top answers the three questions before anything else. |
| Which step | Horizontal rail plus a "Your turn" cell | Vertical rail, with the open chapter lit and per-step times | Knots in the Work column and hand-off lines in the conversation | "Where" cell with a seven-mark tally |
| Whose turn | Tag and dock | Tag, rail stop and dock | Tag and the end of the conversation | The filled "Whose turn" cell |
| What next | The dock | The dock | The dock | The "What next" cell and the question bar |
| Fit at 1040x650 | The log gets about 150px | Good: the side column folds away | Tight: two columns of about 440px | Poor: band and bar take about 270px before the log |
| Builder cost | Small | Medium | Large: moves facts and Recent runs | Small to medium |

Common to all four:

- **The tag reads "Your turn"** in a new solid tag (warning-ink fill, sheet ink), so it never looks like running. Claude's work reads "Claude's turn" in the active ink.
- **Claude's closing question is pinned** above the reply as plain text.
- **Known stop points offer quick replies.** Each sends its fixed words as an ordinary reply.
- **End session is demoted** to a small plain button and asks first.
- **The Runs tab shows a mark** and the In tray deals a card.

#### Recommendation in round 0: B, Step chapters

Superseded: the user chose D. The reasoning is kept for reference.


- **It answers all three questions in one place.** The left rail says where. The rail's "Your turn" stop, the tag and the dock say whose turn. The dock says what to type or press.
- **The rail and the log are the same thing.** The chapter you read is lit in the rail, finished steps collapse to one line with their time, and a step that went wrong is visibly that step.
- **It fixes the cause, not only the symptom.** The 31-line evaluation no longer pushes the question away: the message shows its first blocks, and the question is pinned outside the scroll area.
- **It fits the user's window.** At 1040x650 the side column folds into a facts line and a Recent runs button. The rail, the question, the quick replies and Send all stay in view.
- **It reuses most of A.** The dock is `prompt_box`, already used for permission prompts, so A's work is not lost if B is staged: ship A's dock, tag, badge and tray card first, then the chapters.

Why not the others:

- **A** fixes the bug, but not "where are we": the log stays flat.
- **C** is the clearest read of the dialogue. It moves the facts and Recent runs into the head, the work column is cramped at 1040, and most runs have only two or three messages, so the left column is mostly empty.
- **D** is the loudest "your turn", but the band takes a fixed 120px on every run, including the many that never stop. At 1040 the log is left with almost nothing.

#### Rules for the builder (proposal B)

- **Status and tone**:

  | State | Tag | Tag style |
  | --- | --- | --- |
  | Starting | Starting | `Tone::Active` |
  | Working | Claude's turn | `Tone::Active` |
  | `AwaitingReply`, or a prompt pending | Your turn | the new solid tag |
  | Closed | Finished | `Tone::Plain` |
  | Failed | Failed | `Tone::Closed` |
  | Stopped | Stopped | `Tone::Plain` |

  Recent runs uses the same tags.
- **Head actions**:
  - Working and prompt pending: Stop run, as today.
  - `AwaitingReply`: a small plain "End session…" button. It opens a popover: "End this session?", why it matters, then Keep it open (primary, focused, Esc) and End session (danger). Only End session calls `desk.close`.
  - Finished: Open dossier, when the run made one.
- **Left rail** (`step_column`):
  - One row per step, 168px wide, outside every scroll area.
  - A knot shows the number. A done knot has a `text` hairline, the working one is filled `primary`, the waiting one is filled `interview`, the failed one has a `closed` hairline, and a next one has a `panel` fill.
  - Under each label, `Type::SMALL` in `Ink::Faint`: the step's time when done, "Working, 1m 50s", "Answer below" or "Budget cap".
  - At a known stop point, a "Your turn" row with a filled knot and no number is inserted after the stopped step, reading "Reply below to go on".
  - The row of the open chapter has a `panel` fill. A click on a started step opens its chapter and scrolls to it.
- **At a stop point**:
  - At a known stop point the stopped step is done and the stop row follows it.
  - At an unknown turn end (no catalog entry), the current step takes the waiting look instead, since it may not be done.
  - Do not show the stop row when a prompt is pending: the step itself waits (scene c).
- **Chapters**:
  - Each step that has started is a chapter: a knot, the step name in `Type::SECTION`, a muted summary, the time and a small "Show N" or "Hide" button.
  - Only the current chapter is open by default. The user's toggles stick for that run.
  - The summary is mechanical: agent names, written file names and tool counts. It is never taken from Claude's text.
  - Rows inside a chapter indent by 32px. Nested agent rows indent another `EVENT_NESTED`.
  - Your reply that resumed the run opens the next chapter, as a "You" row in full ink.
- **Tool rows**:
  - Paths show relative to the workspace, with the folder in `Ink::Faint` and the file in `Ink::Text`. Absolute paths outside the workspace stay whole.
  - A finished row has no tag. Only Running, Failed and "Waiting for you" are tagged.
  - The full summary stays on hover.
- **Claude's messages** stay Markdown, per [Trust](../ARCHITECTURE.md#trust), with two changes:
  - Headings are capped: `#` and `##` render as `Type::SECTION`, deeper ones as `Type::STRONG`.
  - A message longer than 8 lines shows its first 3 blocks, then "Show the whole message, N lines". The split happens on blank lines outside code fences, so no line is cut in half.
- **The dock** sits under the activity's scroll area, as the prompt does today. Its height is measured with the existing `prompt_height` logic.
  - **Your turn** (`AwaitingReply`), inside `prompt_box`:
    1. The solid "Your turn" tag, a one-line head ("Evaluate is done. Claude waits for your go before it drafts."), and "Waiting 2 min" on the right.
    2. The quote: "Claude asks, at the end of its message", then the closing question as plain text.
    3. One row: the quick-reply buttons, the field and Send.
    4. A faint line: "Each button sends its words as your reply, as if you typed them."
  - **Prompt pending**: the existing question or permission content, with the Your turn tag and the question as its head.
  - **Working**: a quiet line under a hairline: "Claude's turn" tag and "Nothing to do yet. The Runs tab and the In tray show when Claude needs you."
  - **Finished**: a quiet line with Open dossier and Open in Tracker, then "Reply to resume this session".
  - **Failed**: `prompt_box` in the closed ink, the reason as plain text, then the resume field.
- **Quick replies**:
  - They come only from the command catalog, keyed by command and stop step. `/apply` after Evaluate has "Go ahead with the draft", which sends "Yes, proceed with drafting.", and "Stop here", which sends "No, stop here. Do not draft."
  - A click calls `desk.reply` with that text, exactly like Send. They are disabled while another run is busy (one run at a time) and never start a new run.
  - The tooltip shows the exact words.
- **The closing question**:
  - It is the last paragraph of the turn's last top-level Claude message, with Markdown markup removed and control, bidi and zero-width characters stripped, as failure reasons are.
  - It renders as a plain `Label` of at most 3 lines, with the full text on hover.
  - When the turn ended with no Claude text, the quote box is left out and the head reads "Claude finished this step. Read its message, then reply."
- **Focus**: the field takes focus when the user opens a run that waits on them. It never takes focus while the user types somewhere else. Enter sends.
- **Narrow windows**: below 1100px of window width, the side column is not drawn. "This run" becomes one muted line under the title ("$1.12 of $3.00 · 5m 03s · model · session"). Recent runs moves behind a head button that opens the same list in a popup.
- **Side column**: a fixed 280px wide (`TRAY_RAIL` or a new `RUN_SIDE`), never a share of the split, so long rows cannot push it out of the window. It scrolls on its own.
- **Accessibility**:
  - The dock's group label is "Your turn. Claude asks: …".
  - The rail rows are buttons labelled "Step 1, Evaluate, done, 2 minutes 5 seconds" and "Your turn, reply below".
  - The Runs tab reads "Runs, 1 run waits for you".

#### Outside the page

- **Runs tab**:
  - `RunDesk::waiting_count` also counts runs in `AwaitingReply` that are not busy.
  - With a count above 0, the tab draws an 8px `interview` square before the count, and the count in `Type::TAG` with the warning ink.
- **In tray card** (`RunTurn-tray.png`): `compose.rs` deals a new `CardKind::Turn` for a run in `AwaitingReply` with no pending prompt.
  - Tab "Your turn", when "Run is waiting", with the waiting frame.
  - Title: the catalog's stop label, "/apply: proceed with drafting?", or "/apply is waiting for your reply" when the stop is unknown.
  - Subtitle: "{line} stopped after {step}, $x of $y spent".
  - Body: `Body::Code` with the closing question as plain text.
  - Actions: Open run as the only primary, and Later. There is no Y and no Skip.
  - It ranks with the paused runs. The key includes the turn count, so a later stop deals a new card.
  - It has no quick replies: a reply resumes a run, and that decision belongs on the Runs page, not on a card.

#### New and changed widgets

| Widget | Where | Purpose | Tokens |
| --- | --- | --- | --- |
| `solid_tag(ui, text, tone)` | `marks.rs` | The "Your turn" status: a tag filled with its tone | `TAG_HEIGHT`, `TAG_PADDING`, `Type::TAG`, `RADIUS_CARD`, fill `Tone::Interview`, ink `sheet` |
| `folder_tab(ui, label, count, on, attention)` (changed) | `controls.rs` | Marks the Runs tab when a run waits | `STATUS_DOT` (8) square in `interview`, count in `Type::TAG` with `Tone::Interview` |
| `step_column(ui, steps: &[StepMark], open) -> Option<usize>` | `surfaces.rs` | B's vertical rail: knots, labels, meta, the stop row, and the lit open step | new `STEP_COLUMN_WIDTH = 168.0`, `STEP_ROW_MIN = 48.0`, `KNOT_SIZE = 22.0`; `HAIRLINE` connector in `line`; `Type::STEP`, `STEP_DONE`, `STEP_NOW`; `Type::SMALL` with `Ink::Faint`; `RADIUS_CARD` knots, `RADIUS_BUTTON` lit row in `panel` |
| `knot(ui, text, state)` | `marks.rs` | The numbered square used by the rail, chapters and hand-offs | `KNOT_SIZE`, `Type::TAG`, done `text` hairline, now `primary` and `on_primary`, hold `interview` and `sheet`, fail `closed` hairline, next `panel` and `muted` |
| `chapter_head(ui, knot, label, summary, meta, open) -> Response` | `surfaces.rs` | One step's header row, with the Show or Hide button | new `CHAPTER_HEAD_HEIGHT = 42.0`, `CHAPTER_INDENT = 32.0`; `Type::SECTION`, `Ink::Muted` summary, `Ink::Faint` meta, `small_button` |
| `quote_box(ui, label, text)` | `surfaces.rs` | Claude's closing question, as plain text | `panel` fill, new `QUOTE_RULE = 3.0` in `interview`, new `QUOTE_PADDING: (i8, i8) = (12, 8)`, label `Type::SMALL` with `Ink::Muted`, text `Type::TAKE` |
| `path_label(ui, dir, file)` | `text.rs` | A tool row's path: faint folder, ink file, truncated | `Type::MONO`, `Ink::Faint` and `Ink::Text` |
| `prompt_box_tone(ui, tone, add)` (changed `prompt_box`) | `surfaces.rs` | The same frame in `closed` for a failed run | `HAIRLINE`, `OFFSET_SHADOW`, `PROMPT_PADDING` |
| `markdown` (changed) | `text.rs` | Caps heading sizes, so a message cannot push the page | `Type::SECTION` for `#` and `##`, `Type::STRONG` below |

The End session popover reuses egui's popup, which `install_theme` already styles: `sheet` fill, `text` hairline, `RADIUS_PANEL` and the `WINDOW_SHADOW` offset in `line`.
`step_rail` stays for proposal A, or can leave the run screen once B ships.

#### Data the screen needs (for the planner)

- **`Activity.step: Option<usize>`**: the step the rail was on when the row arrived. This makes the chapters. Rows from before the first match belong to step 0.
- **Per-step time**: the time each step started, from a timestamp the infrastructure puts on each event. `domain` stays clock-free. This is optional: without it, the rail shows tool counts instead of times.
- **Stop points in `catalog.rs`**: `StopPoint { after_step, label, replies: &[(button, text)] }`. `/apply` has one after step 1, labelled "proceed with drafting".
- **`Run::closing_question() -> Option<String>`** in `application`, built as the rules above say.
- **`RunDesk::waiting_count`** also counts runs in `AwaitingReply`.
- **`intray`**: deals a new `CardKind::Turn` from runs in `AwaitingReply` (see Outside the page).

#### Proposed doc changes (for the lead)

- **`README.md` line 12**: replace "When Claude finishes a step, reply to continue or press End session." with:
  > When a run waits for you, its status reads Your turn, the Runs tab shows a mark, and the In tray deals a card. Claude's question is pinned above the reply box, and known stop points such as /apply's proceed with drafting offer one-click replies, sent as ordinary replies. End session asks first while Claude waits. The activity folds tool calls into one chapter per step, beside a rail of the steps.

  The `## Run` section of `README.md` is about `cargo run` and needs no change.
- **`ARCHITECTURE.md` Features table, `runs` row**: Language becomes "run, activity, prompt, decision, step rail, stop point, quick reply, command catalog".
- **`ARCHITECTURE.md` Runtime, after line 55**, add:
  > A turn that ends with no pending prompt leaves the run waiting for a reply. Like a pending prompt, that counts as the user's turn: the Runs tab marks it and `intray` deals a card for it. Each activity row keeps the step the rail was on when it arrived, so the Runs screen folds rows into one chapter per step. The rail is display only and never steers the run. Quick replies are fixed text from the command catalog, sent through the same path as a typed reply, so they never start a run.
- **`ARCHITECTURE.md` line 61**: "Permission, question and guard cards have no Y action." becomes "Permission, question, guard and Your turn cards have no Y action."
- **`ARCHITECTURE.md` Trust, after "Claude's replies and prep packs render as Markdown with no image loading."**, add:
  > Their headings are capped at the section size. Claude's closing question, pinned above the reply box and on the In tray card, renders as plain text with Markdown markup and control, bidi and zero-width characters removed.
- **`AGENTS.md`**: no change. The Your turn card only opens the run, so "Card actions never start a run" holds.

#### Noticed, out of scope

- **The rail inference bug.** A `Bash` row that names `job_search_tracker.csv` matches the Finish rule. The brief already sends it to a builder. B makes such a jump visible, because rows land in the wrong chapter.
- **Desktop notification.** A macOS or Windows notification, or a Dock badge, when a run reaches Your turn would cover a minimized window. It belongs to the shell, so it is not drawn here.
- **`app.css`.** A new "Runs: where am I" section adds only new class names. The fit-mode overrides are scoped to `.rt`, so `TrayUndo.html` and the other canvases are unchanged.

## 2. In tray: undo in visible space

### Brief

The user wrote: "The undo button is currently not visible on my screen size. We could find a more subtle place to put it in visible space."
Their window is about 1000x650. The undo bar (`undo_bar()` in `src/intray/ui.rs`, built from `undo_strip` and `undo_button`) is drawn after the stack, inside the card's `ScrollArea`. So it sits below "6 behind this one", and they had to scroll to find it.
It is a full-width dark bar, for example "Set aside: X [Undo U]", and after N it also holds the skip reasons (`SkipReason::ALL`). The workaround today scrolls the bar into view once when a skip asks for a reason.

The new spot must stay visible at any window height without scrolling, and be quieter than the dark bar. The skip-reason picker must still fit, and stay reachable by mouse and keyboard.
Out of scope: the horizon strip, card face content, ranking, other screens and code.
None of the user's screenshot content is copied: the company in the screenshot is replaced with fictional names.

### Canvas

`TrayUndo.html` mirrors the app's layout. The head, strip and rail stay put, and only the card column scrolls (the `.app.fit` rules in `app.css`). The rail is 280px wide, as in `side_column("tray_side", 280.0)`.
Pick the parameters with `?scene=A|B|C|D`, `?place=rail|tab|pill` and `?theme=light|dark`. Today is Saturday 31 October, and 6 cards are left.

| PNG | Size | Placement | State |
| --- | --- | --- | --- |
| `TrayUndo-A.png`, `TrayUndo-A-1040.png` | 1280x820, 1040x650 | rail (chosen) | After L: a raised row heads Cleared today, "Orion Freight [Undo U] / Set aside until tomorrow". The strip shows "1 back". |
| `TrayUndo-B.png`, `TrayUndo-B-1040.png` | 1280x820, 1040x650 | rail | After N on a skippable card, with no reason yet: "Quarry Labs [Undo U] / Skipped. Why? Optional." and the toggles Rate, Scope, Domain and Location. |
| `TrayUndo-B-dark.png` | 1280x820 | rail | B in dark. |
| `TrayUndo-C.png`, `TrayUndo-C-1040.png` | 1280x820, 1040x650 | rail | A reason picked: "Skipped, scope", with Scope filled. |
| `TrayUndo-D.png`, `TrayUndo-D-1040.png` | 1280x820, 1040x650 | rail | No undo offer: Cleared today lists plain rows. There is no gap in the rail or under the card, because the card column never held the bar. |
| `TrayUndo-alt-tab-B-1040.png` | 1040x650 | tab (alternative 1) | B as a slim line beside the card tab, pinned above the scrolling card. |
| `TrayUndo-alt-pill-B-1040.png` | 1040x650 | pill (alternative 2) | B as a pill pinned to the bottom of the card column. |

### Chosen placement: the head of the Cleared today rail

- **Always in view.** The rail is the `side_column`, built outside the card's `ScrollArea`, so it never scrolls. At 1040x650 the undo row sits at about y 350 to 460, under the strip and level with the card's title.
- **Where the decision lands.** The last decision is the head of the Cleared today log, so its undo lives on that row. Nothing new appears on screen. The row turns into a plain log line once the next decision is made.
- **Quieter.** It is a `raised` row inside an existing slip, with a small Undo button, instead of an inverted full-width bar.
- **The reasons fit.** The four small toggles take about 251px with 4px gaps, inside the rail's 252px content width. They wrap to a second row if a label ever grows.
- **No gap.** With no offer the row is simply absent (scene D), and the card column is untouched in every state.
- **Covers nothing.** It never overlaps the card or its action buttons.

Rejected:

- **Alternative 1, a line beside the card tab.**
  - The tab row would have to move out of the `ScrollArea` to stay pinned.
  - At 1040 the line wraps to two rows, which pushes the card down by about 30px.
  - Next to the new card's tab, "Skipped: Quarry Labs" reads as if it describes the card below it.
- **Alternative 2, a pill above the footer.**
  - It floats over the card. In the PNG it covers the gates, and once scrolled it would cover the action buttons unless the scroll area reserves about 64px at the bottom.
  - It is a second layer to manage, and louder than a row in the rail.

### Rules for the builder

- **Row content**:
  - The first line is the card name in `Type::STRONG`, truncated, with a small "Undo U" key button on the right.
  - The second line is in `Ink::Muted`:
    - Later: "Set aside until tomorrow".
    - A skip with no reason: "Skipped. Why? Optional."
    - Anything else: `Decision::label()`, such as "Skipped, scope", "Command opened" or "Kept".
- **Reasons**:
  - They show only when `offer.asks_reason` is true, as `SkipReason::ALL` in order, with `on` set to `offer.reason == Some(reason)`.
  - Picking one sends `Pressed::Reason`. Picking another replaces it. Picking the selected one again keeps it, as `give_reason` does today.
- **Log**:
  - For a logged outcome (skip, follow, keep), the undo row replaces that decision's plain row (`queue.cleared[0]`), so it is never drawn twice.
  - Later is not logged, so its row sits above the log only while it can be undone. The header count stays `cleared.len()`: 2 in scene A, 3 in B to D.
  - "Nothing yet." shows only when there is no log and no offer.
- **Lifetime**: unchanged. The offer lasts until the next decision or U.
- **Keyboard**:
  - U still undoes from anywhere.
  - The reason toggles and the Undo button are ordinary focusable buttons with the existing focus ring. Because the side column is built before the central panel, Tab reaches them first.
  - No new shortcuts. Number keys stay free for the cards.
- **Accessibility**: the row's label is "Last decision: Skipped, Quarry Labs. Press U to undo."
- **Data the row needs**: `UndoOffer` carries `text` ("Skipped: Quarry Labs") today. The row needs the parts: the `name`, the `label` (`Decision::label()`), and whether the outcome is logged (or the `Outcome` itself). This is an `application` change in `in_tray.rs`.

### New widgets

| Widget | Where | Purpose | Tokens |
| --- | --- | --- | --- |
| `undo_row(ui, name, note, add) ` | `src/ui_kit/tray.rs` | The raised head row of a slip's list. It shows the name, the note, the Undo button slot and the extra content (reasons). | Fill `raised`, inset `INSET` (14), new `UNDO_ROW_PADDING: (i8, i8) = (10, 12)` (top, bottom), `ITEM_GAP` (6) between lines, `Type::STRONG` and `Ink::Muted` |
| `small_key_button(ui, text, key)` | `src/ui_kit/controls.rs` | The Undo button: `press(ui, text, Look::Plain, SMALL, Some(key))` | `SMALL_BUTTON_HEIGHT`, `SMALL_BUTTON_PADDING`, `Type::SMALL_BUTTON`, `KEY_SIZE` |
| `small_toggle(ui, text, on)` | `src/ui_kit/controls.rs` | A reason button. Off, it looks like `small_button`. On, it is filled `text` with `sheet` ink, like `.seg button.on`, and its `WidgetInfo` is `selected`. | `SMALL_BUTTON_HEIGHT`, `SMALL_BUTTON_PADDING`, `RADIUS_BUTTON`, `HAIRLINE`, new `TOGGLE_GAP: f32 = 4.0` (6px makes the four reasons wrap in the rail) |

### What to remove

- **`src/intray/ui.rs`**: the `undo_bar()` call inside the `ScrollArea`, `undo_bar()` itself, `undo_seen_id()` and the `scroll_to_me` workaround. Nothing needs to scroll into view any more.
- **`UndoOffer.id`**: remove it if nothing else reads it. Its doc comment ties it to the scroll-once logic.
- **`src/ui_kit/tray.rs`**: `undo_bar` and `undo_button`, and the `UNDO_PADDING` and `UNDO_GAP` tokens once they are unused.
- **`tests/ui_kit_gallery.rs` lines 296 to 301**: replace the `undo_bar` example with `undo_row`, `small_key_button` and `small_toggle`, then refresh the gallery and screen snapshots on macOS.

### Proposed doc changes (for the lead)

- **`README.md` line 10**, replace "N skips, U undoes." with:
  > N skips, U undoes the last decision. Until your next one it waits at the top of Cleared today, where a skip also takes its optional reason.
- **`ARCHITECTURE.md` line 57** (the queue paragraph), after "Decisions and skip reasons live in `decisions.json` in the app's data folder." add:
  > The last decision can be undone until the next one. Its row heads the Cleared today rail, which sits outside the card's scroll area, so it stays in view at any window height.
- **`ARCHITECTURE.md` Features table, `intray` row**: no change.

### Noticed, out of scope

At 1040 wide, a horizon cell is about 56px inside, so today's "6 to decide" wraps to two lines (`TrayUndo-*-1040.png`). The app may already handle this, after `3d36e4d` fitted the counts to narrow columns.

## 3. In tray: horizon indicators

### Brief

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

### Canvas

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

### Rules for the builder

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

### New widgets (`src/ui_kit/tray.rs`)

| Widget | Purpose | Tokens |
| --- | --- | --- |
| `horizon_count(ui, count, overdue, kinds: &[(usize, &str)])` | Today's pending count, the overdue line and up to two kind lines | `Type::TAG` with `Ink::Text`, or `Ink::Muted` at 0; `Type::TAG` with `Tone::Interview` for overdue; `Type::SMALL` with `Ink::Muted`; the cell's existing 3px gap |
| `horizon_return(ui, text) -> Response` | The clickable chip on tomorrow's cell for cards set aside | Fill `sheet`, `HAIRLINE` stroke in `line` (`text` on hover), `RADIUS_CARD`, `Type::TAG` with `Ink::Text`, new `HORIZON_CHIP_HEIGHT = 20.0` and `HORIZON_CHIP_PADDING = 6.0` |
| Note on the `horizon` strip | One muted line across the empty days | A `panel`-filled rect painted above the cell hairlines, `Type::SMALL` with `Ink::Muted`, new `HORIZON_NOTE_PADDING: (f32, f32) = (14.0, 7.0)`. `horizon()` needs an `Option<&str>` note argument, because only it knows the column rects. |

The tooltip reuses egui's popup, which `install_theme` already styles: `sheet` fill, a `text` hairline, `RADIUS_PANEL` and the `WINDOW_SHADOW` offset in `line`. A tooltip width of about 250 matches the canvas.
The overdue count needs no new application state: `Card.due` is public, so the UI can count from `queue.cards`. A `Queue::overdue()` helper beside `headline()` would keep the rule in one place.

### Proposed doc changes (for the lead)

- **`ARCHITECTURE.md` line 59** (the Later paragraph), replace "Its mark pulls it back to the top." with:
  > Its mark pulls it back to the top. Tomorrow's cell on the horizon counts the cards set aside, names them on hover, and brings them all back when clicked.
- **`ARCHITECTURE.md` line 57** (the queue paragraph), add after "then the rest.":
  > The horizon strip shows the 14 days from today. Today's cell counts the pending cards by kind and the ones past their deadline, which rank first but fall off the strip. When no dated card falls in the window, the strip says so, so no cell is silently blank.
- **`ARCHITECTURE.md` Features table, `intray` row**: no change. The counts derive from `Queue.cards` and `Queue.set_aside`, which already exist.
- **`README.md` line 10**, replace "with paused runs first, then deadlines, then the rest." with:
  > with paused runs first, then deadlines, then the rest. The strip above it counts what is left to decide today, and what is overdue.
- **`README.md` line 10**, replace "L sets the card aside until tomorrow," with:
  > L sets the card aside until tomorrow (the horizon's next day shows how many, and a click brings them back),

### Changes in round 2

- **Overdue**: a new "N overdue" line in today's cell, the new scene E (light and dark), and the conditional note wording. The answered open question is removed.
- **Note mask**: `.hz-none` is now a padded block (7px by 14px, `background-color: var(--panel)`, `z-index: 2`), so the hairlines stop well clear of the text. Before, the mask was only as tall as the text, so the line read as crossed out at normal zoom. I re-rendered A, A-dark and D and checked each at full size.
- **`Tray.html` line 380**: the Lumen follow-up draft now signs off "Robin Vasquez". No other canvas has the real name.

### Changes in round 1

- **`app.css`**: added the `.hz-count`, `.hz-late`, `.hz-kind`, `.hz-back`, `.hz-tip`, `.hz-none` and `.tbox.lift` classes. I also fixed a selector-strength bug: `.app button` outranked `.mark` and `.c-*`, so the horizon marks in `Tray.html` drew in the plain text colour and regular weight. The new `.hz .mark` rules give them the `TAG` face and kind tone that `horizon_mark` uses in the app, so `Tray.html` now matches the app too.
