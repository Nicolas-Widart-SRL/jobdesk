# Tasks

The Tasks screen shows one run: where it is, whose turn it is and what comes next, then the story of what Claude did. It answers one complaint: people ended runs because they did not see that Claude was waiting for them. What the user sees is in [docs/guide.md](../../docs/guide.md#tasks). When Claude waits on the user, and how the rail and quick replies work, is in [ARCHITECTURE.md](../../ARCHITECTURE.md#runtime). How messages and questions render is in [ARCHITECTURE.md](../../ARCHITECTURE.md#trust).

Canvases, in `mockups/`: [Grid-tasks-turn.png](../Grid-tasks-turn.png), an `/apply` run stopped after Evaluate, and the `RunLive-*.png` set for a run while Claude works (see [While Claude works](#while-claude-works)).

## Contents

- [Principles](#principles)
- [The head](#the-head)
- [The band](#the-band)
- [The question bar](#the-question-bar)
- [The story](#the-story)
- [The side column](#the-side-column)
- [While Claude works](#while-claude-works): live Elapsed and Cost, the quiet state, options A and B, motion, new widgets
- [Outside the page](#outside-the-page)
- [Accessibility](#accessibility)
- [Doc changes](#doc-changes)
- [Open](#open)

## Principles

1. **One loud thing.** Stamp ink appears only when the run needs the user.
2. **A report, not a log.** Claude's messages are the main content.
3. **Plain words first.** Technical rows open only on request.
4. **Say each thing once.** The band says whose turn it is, so the head carries only the title and the actions.
5. **Show the run is alive, honestly.** A live run moves, a quiet one says how long it has been quiet, and nothing pretends to know how far along it is.

Stamp ink is magenta because it is the one hue the navy and slate palette lacks, so it stands out in both themes without reading as a job status.

## The head

The command line on the left, cut to fit, with the full line on hover. On the right, only small buttons (`SMALL_BUTTON_HEIGHT`) on the title's centre line. There is no status tag.

| State | Action |
| --- | --- |
| Working, or a prompt pending | Stop run |
| Stopping | Force stop |
| Waiting for a reply, or stopped or failed with its session still open | End session…, which asks first. Keep it open is the default, and Esc keeps it. |
| Window narrower than `DETAILS_BELOW_WIDTH` (`src/runs/ui/run_screen.rs`) | Run details too, a popup with This run and Recent runs |

## The band

Three cells, Where, Whose turn and What next, each a label, a value and a note, lined up across cells (`turn_band`). The Whose turn cell fills with `turn` only when it is the user's.

| State | Where | Whose turn | What next |
| --- | --- | --- | --- |
| Starting | the step, "Starting the CLI" | Claude's | Nothing for now |
| Working | "2 Draft", "2 of 7 steps done" | Claude's, "Claude is working" | Nothing for now, "You can leave. The Tasks tab marks it when it needs you." |
| Stopping | as Working | Claude's, "Stopping" | Nothing for now |
| Stopped at a stop point | "1 Evaluate, done", "Draft waits for your go" | Yours, "Claude stopped" | Answer Claude, "Its question is right below." |
| Prompt pending | the step, "Paused inside the step" | Yours, "Claude asked" | what is asked, "The choices are right below." |
| Done for now | the step, done or paused | Nobody, "Claude is done for now" | Reply to continue, "Or end the session." |
| Failed | "5 Compile, failed" in `closed` | Nobody, the reason as plain text | Reply to resume, "Or leave it. Files so far stay." |
| Finished `/apply` | "All 7 steps" | Nobody, "The session is closed" | Read the documents, "Open the dossier, then send it yourself." |

While Claude works, the Whose turn and What next notes change with the quiet state, as [While Claude works](#while-claude-works) says.

The Where label holds a tally with one mark per step: done in `text`, working in `active`, waiting in `turn`, the rest in `off`. Below a window height of `BAND_COMPACT_HEIGHT`, each cell keeps to one line, label then value, with no note.

## The question bar

It appears only on the user's turn, under the band, as `prompt_box_tone` in stamp ink, so the question has a turn edge.

- **At a stop**: "Claude asks:" and the closing question as plain text, cut to a few lines with the whole on hover. Then one row: the quick replies, the field and Send. Enter sends.
- **Quick replies** are fixed text from the command catalog, such as "Go ahead with the draft" and "Stop here" after `/apply`'s Evaluate. They send through the same path as a typed reply, show their words in a tooltip, and never start a run. They are disabled while another run works or the field holds a draft.
- **AskUserQuestion**: the choices in two columns, then Send answer and Decline. AskUserQuestion always reaches the user.

## The story

Under "The run so far" ("The run" once the session is closed), on paper, in reading order.

- **Step lines** (`step_line`): one per started step. A `knot` (done, working, or stamp ink when waiting), the step name, one plain sentence, and a "Show N actions" toggle, "so far" while live. The sentence comes from the phrase table in the command catalog, never from Claude's text or a posting's. Open, the toggle shows the rows with short paths (`path_label`) and Done, Running or Failed tags.
- **Claude's messages** read as documents (`document`): headings held to the section size, the body in `Type::TAKE`, prose no wider than `DOC_MEASURE`, so a line stays readable on a wide screen, while top-level tables span the full width. A footnote that points across a table keeps the whole message at `DOC_MEASURE`.
- **A You line** shows each reply in the user's words, between the step it ended and the step it started.
- **Scroll**: stopped or finished, the story opens at the top of the newest message. While Claude works, it follows the bottom. Once the user scrolls, their position wins.

## The side column

A fixed `RUN_SIDE` width, never a share of the split, so long rows cannot push it out of the window. It holds This run and Recent runs and scrolls on its own. Recent runs tags a run that waits with a solid Your turn tag, and a working one "Claude's turn" in `active`. Below `DETAILS_BELOW_WIDTH` it moves behind Run details.

This run's Cost and Elapsed are live while Claude works, as the next section says.

## While Claude works

The brief: a run showed no sign of life while Claude worked, and Cost and Elapsed read 0 until the end, so a live run and a hung one looked the same. Two options are drawn. Both share the live figures and the quiet thresholds, and differ in the one element that moves.

### Canvases

`RunLive.html`, chosen with `?option=a|b&state=working|quiet|sheet&theme=light|dark`. Add `&motion=on` to watch the motion in a browser. The PNGs are still frames.

| PNG (1280x820) | State |
| --- | --- |
| [RunLive-a-working.png](../RunLive-a-working.png), [dark](../RunLive-a-working-dark.png) | A: Draft under way, an update 3 s ago, the sweep partway across the Whose turn cell. |
| [RunLive-a-quiet.png](../RunLive-a-quiet.png) | A: no update for 3 min. The edge is a still dashed rule, and What next says long writes can be quiet. |
| [RunLive-a-sheet.png](../RunLive-a-sheet.png), [dark](../RunLive-a-sheet-dark.png) | A: every state of the signal, the sweep's key frames, and Cost and Elapsed beside each. |
| [RunLive-b-working.png](../RunLive-b-working.png), [dark](../RunLive-b-working-dark.png) | B: the same run, the live line at the story's foot with its update strip and the last action. |
| [RunLive-b-quiet.png](../RunLive-b-quiet.png) | B: no update for 3 min. The strip is empty, and the band's note says how long. |
| [RunLive-b-sheet.png](../RunLive-b-sheet.png), [dark](../RunLive-b-sheet-dark.png) | B: every state of the live line, two frames 5 s apart, and Cost and Elapsed beside each. |

The screen-level loading, failed and empty states are unchanged: Tasks with no run is `UiPass-tasks-empty.png`, and the run screen itself has no loading view.

### Live figures, in both options

- **Elapsed** ticks every second while it is Claude's turn, as `m:ss` under an hour and `h:mm:ss` from there, such as 4:12 and 1:02:05. It counts Claude's turns as Jobdesk saw them, from the send to the turn's result, so it holds still on the user's turn and never jumps back when the result arrives. The finished run's figure uses the same form, so "14m 20s" becomes 14:20.
- **Cost** counts up as the run goes, from each Claude message's token use and the model's price, and reads "About $1.24 of $10.00" while it is an estimate. Under the bar, one line in `Type::SMALL` and `muted`: "Estimated until Claude's turn ends." When the turn's result arrives, About and the line go and the exact figure shows. A process that ended with no result, such as after Force stop, keeps About. The bar fills in `text` as now, with no motion: it moves in steps as messages land.
- **Recent runs** shows the same live figure without About, since the row has no room for it.
- **egui has no tabular figures**, so the ticking figures stay right-aligned in the side column, and their width changes at the left edge only.

### Quiet

An update is any line the run's process prints on its stream: a message, a tool call, a tool result. Jobdesk knows the process is up, and when the last update came. It does not know whether Claude is thinking, writing a long file or stuck, so the words stay calm and never guess.

| Time since the last update | Whose turn note | What next note |
| --- | --- | --- |
| Under 30 s | A: "Last update 3s ago". B: "Claude is working" as now. | as now |
| 30 s to 2 min | "No update for 48s" in A. B keeps "Claude is working". | as now |
| 2 min and more | "No update for 3 min", counted in whole minutes | "Long writes can be quiet. Stop run keeps the files so far." |

No quiet state uses `closed` or `turn` ink, and none starts or stops anything. One long Write of a CV can print nothing for minutes, so 2 min is not a failure.

### Where the signal stops

| State | Signal | Elapsed | Cost |
| --- | --- | --- | --- |
| Starting, the process up with no update yet | A: the sweep runs. B: the strip's baseline only, "Starting Claude Code". | ticks | $0.00 |
| Your turn: a question, a permission prompt or a stop point | gone. The question bar is the one loud thing. | holds | About while the turn is open (a permission prompt), exact once it ended |
| Stopping | still: A's edge a solid `muted` rule, B's strip stops sliding with its marks in `muted` | ticks until the process confirms | About |
| Done for now, finished | gone | final | exact |
| Failed, stopped | gone | final | exact, or About when no result came |

### Option A, the sweep

A `MARK_RULE` segment, 30% of the cell's width, travels the Whose turn cell's bottom edge, from just outside its left to past its right. No track is drawn, so at rest the band looks as it does today.

- **Working**: `active`, 2.4 s across with ease in and out, then 0.6 s at rest, so one pass every 3 s.
- **Quiet, 30 s to 2 min**: `muted`, half speed, one pass every 6 s. It slows, it does not stop.
- **2 min and more**: the segment stops and the edge becomes a still dashed rule in `muted`, the whole cell wide. A still line reads "nothing is coming in" without reading "broken".
- **Reduced motion**: a still solid rule, `active` while working, `muted` when quiet, dashed after 2 min.
- **Repaint**: about 30 frames a second, only while the sweep shows on screen. Elsewhere, and on the user's turn, the screen repaints once a second for Elapsed, or not at all.

Why: the band is the first thing read, and the cell that says Claude's is where "is it still going" is asked. One small moving line in the cell's own ink says the run is live without a fake percent.

### Option B, the update strip

The live line sits at the foot of "The run so far", inside the paper and pinned under its scroll area, divided by a `line` hairline, only while it is Claude's turn.

- **The strip** (`update_strip`), on the left: one mark per update over the last 2 minutes, newest at the right edge. An action (a tool call) is a full `TALLY_THIN` mark, anything else half height, in `active`. A `line` hairline is its baseline. It slides left 2 px a second, at 240 px wide.
- **The last update**, in the middle: the tool's name, then its short path through `path_label`, as the story's action rows show it. Claude's text reads "Claude wrote a message". It is the same untrusted summary the action rows render, as plain text cut to one line, full on hover.
- **How long ago**, on the right in `muted`: "2s ago", "48s ago", then whole minutes from 2 min: "3 min ago".
- **Quiet**: nothing new enters, so the right end empties as the marks drift left. After 2 min the strip is empty, and the band's note says "No update for 3 min".
- **Reduced motion**: the marks hold for 10 s, then step 20 px left. The ago text still updates each second.
- **Repaint**: twice a second while the line shows, one pixel per step, and once a second under Reduce motion.

Why: it shows only what happened. A busy run is a dense strip, a long write is a gap, and a stuck run is an empty one, so the user can judge without the app guessing. It also names the last action, which answers "what is it doing" without opening the rows.

### Motion rules, both options

- One element moves on the screen, slowly. Nothing else animates while a run works: no knot, tally or tab pulses.
- It moves only while it is Claude's turn and the Tasks screen shows. Settings › Appearance › Reduce motion swaps it for its still form. The Elapsed and ago text keep updating, since text that changes is not motion.
- No fade, glow or blur: flat fills and 1px-class shapes, as the rest of the kit.

### New widgets

| Widget | Option | What it is | Tokens |
| --- | --- | --- | --- |
| `live_sweep` | A | The moving segment on a band cell's bottom edge, with its quiet, long-quiet, stopping and still forms | `MARK_RULE`; `active`, `muted`; new motion constants for the pass, the rest and the quiet pass, beside `MOTION_SECONDS` |
| `update_strip` | B | One mark per update over a fixed window, sliding left | `TALLY_THIN`; `active`, `muted`, `line`; a new width and window constant |
| `live_line` | B | The strip, the last update through `path_label`, and the ago text, at the paper's foot | the story paper's own inset, `ROW_ITEM_GAP` between its parts, a `HAIRLINE` in `line` above; `Type::BODY` and `Type::MONO` through `path_label`, `Type::NOTE` for the ago text |
| `estimate_note` | both | The one muted line under the Cost bar while it is an estimate | `Type::SMALL`, `muted` |

## Outside the page

- **The Tasks tab** carries a stamp-ink count chip of the runs that wait on the user. It reads "Tasks, 1 need you".
- **Today** deals a Your turn card for a run stopped on a question or a known stop point. Its tab and frame are in stamp ink. Its title is the stop's label, such as "/apply: proceed with drafting?", and its body is Claude's question as plain text. Its actions are Later (L), as on every card, and Open task. It has no Y and no Skip, so a card never resumes a run.

## Accessibility

- The band is the group "Where the run is", and its tally reads "1 of 7 steps done".
- The question bar's group reads "Your turn. Claude asks: …".
- A knot reads its number and state, such as "2, waiting for you".
- The moving element is decoration. A's Whose turn cell reads "Claude's turn. Last update 3 seconds ago". B's strip reads "27 updates in the last 2 minutes, the last 2 seconds ago". Screen readers get the change only when the quiet state changes, never each second.
- Cost reads "About 1 dollar 24 of 10 dollars, estimated until Claude's turn ends".

## Doc changes

For the planner's handoff step, once the user picks an option:

- `docs/guide.md`, Tasks: while Claude works, [the sweep under Claude's / the live line at the foot of the story] shows the run is live. Elapsed ticks every second, and Cost counts up as an estimate marked About until Claude's turn ends, then shows the exact cost. When nothing new arrives for a while, the band says for how long: a long write can be quiet for minutes, and Stop run keeps the files so far. Under Appearance, Reduce motion also stills this signal.
- `docs/architecture/runs.md`, a new "Live figures" section: the time since the last update counts every stream line; the quiet thresholds are 30 s and 2 min; Cost is estimated from each message's token use and the model's price and replaced by the turn result's cost, and stays an estimate when no result came; Elapsed is the wall-clock time of Claude's turns as Jobdesk saw them. For B, the run keeps the times of its updates over the last 2 minutes.
- `docs/architecture/ui-kit.md`: the new widgets, and a motion rule: a looping animation is allowed only for a live run's signal, one per screen, only while it shows, and Reduce motion swaps it for a still form. Repaint on a timer only while it shows.
- `ARCHITECTURE.md`: no change.

## Open

- **Drawn**: A, the sweep on the Whose turn cell, and B, the update strip at the story's foot.
- **Picked so far**: nothing.
- **Undecided**: which option; whether the 30 s and 2 min thresholds suit `/apply`'s longest writes; whether Elapsed should keep counting the user's turn too, which today's figure does not.
