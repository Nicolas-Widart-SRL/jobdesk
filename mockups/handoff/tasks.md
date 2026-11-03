# Tasks

The Tasks screen shows one run: where it is, whose turn it is and what comes next, then the story of what Claude did. It answers one complaint: people ended runs because they did not see that Claude was waiting for them. What the user sees is in [docs/guide.md](../../docs/guide.md#tasks). When Claude waits on the user, and how the rail and quick replies work, is in [ARCHITECTURE.md](../../ARCHITECTURE.md#runtime). How messages and questions render is in [ARCHITECTURE.md](../../ARCHITECTURE.md#trust).

Canvases, in `mockups/`: [Grid-tasks-turn.png](../Grid-tasks-turn.png), an `/apply` run stopped after Evaluate, and the `RunLive-*.png` set for an open run: Claude working, quiet, and asking (see [While the run is open](#while-the-run-is-open)).

## Contents

- [Principles](#principles)
- [The head](#the-head)
- [The band](#the-band)
- [The question bar](#the-question-bar)
- [The story](#the-story)
- [The side column](#the-side-column)
- [While the run is open](#while-the-run-is-open): the sweep, live Elapsed and Cost, the quiet state, motion, new widgets
- [Outside the page](#outside-the-page)
- [Accessibility](#accessibility)

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
| Working | "2 Draft", "2 of 7 steps done" | Claude's, "Last update 3s ago" | Nothing for now, "You can leave. The Tasks tab marks it when it needs you." |
| Stopping | as Working | Claude's, "Stopping" | Nothing for now |
| Stopped at a stop point | "1 Evaluate, done", "Draft waits for your go" | Yours, "Claude stopped" | Answer Claude, "Its question is right below." |
| Prompt pending | the step, "Paused inside the step" | Yours, "Claude asked" | what is asked, "The choices are right below." |
| Done for now | the step, done or paused | Nobody, "Claude is done for now" | Reply to continue, "Or end the session." |
| Failed | "5 Compile, failed" in `closed` | Nobody, the reason as plain text | Reply to resume, "Or leave it. Files so far stay." |
| Finished `/apply` | "All 7 steps" | Nobody, "The session is closed" | Read the documents, "Open the dossier, then send it yourself." |

While Claude works, the sweep runs along the Whose turn cell's bottom edge, and the Whose turn and What next notes change with the quiet state, as [While the run is open](#while-the-run-is-open) says.

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

This run's Cost and Elapsed are live while the run is open, as the next section says.

## While the run is open

The brief: a run showed no sign of life while Claude worked, and Cost and Elapsed read 0 until the end, so a live run and a hung one looked the same. The approved answer is the sweep on the Whose turn cell, a live Elapsed that runs from the send to the run's end, and a Cost that counts up as an estimate.

### Canvases

`RunLive.html`, chosen with `?state=working|quiet|turn|sheet&theme=light|dark`. Add `&motion=on` to watch the sweep and the clock tick in a browser. The PNGs are still frames.

| PNG (1280x820) | State |
| --- | --- |
| [RunLive-working.png](../RunLive-working.png), [dark](../RunLive-working-dark.png) | Draft under way, an update 3 s ago, the sweep partway across the Whose turn cell. |
| [RunLive-quiet.png](../RunLive-quiet.png) | No update for 3 min. The edge is a still dashed rule, and What next says long writes can be quiet. |
| [RunLive-turn.png](../RunLive-turn.png), [dark](../RunLive-turn-dark.png) | Claude asked a question inside Draft (AskUserQuestion). The band is Yours with no sweep, the question bar holds two choices, and Elapsed reads 9:31, past the 8:05 it read when Claude asked: the clock runs on during the user's turn. |
| [RunLive-sheet.png](../RunLive-sheet.png), [dark](../RunLive-sheet-dark.png) | Every state of the sweep, its key frames, and Cost and Elapsed beside each, marked ticks or holds. |

The screen-level loading, failed and empty states are unchanged: Tasks with no run is `UiPass-tasks-empty.png`, and the run screen itself has no loading view.

### Live figures

- **Elapsed** is measured on the clock from the user's send to the run's end. It ticks every second the whole time the run is open: while Claude works, while Claude waits on the user (a question, a permission prompt or a stop point), and while a stop settles. It holds once the run ends: done for now, finished, failed or stopped. A reply after that adds a new span from its send, so the figure goes on from where it held and never jumps back. A run left waiting overnight reads hours, which is the true time it stayed open.
- **Its form** is `m:ss` under an hour and `h:mm:ss` from there, such as 4:12 and 1:02:05, live and final alike. The `elapsed_text` test in `src/runs/application/story.rs` that expects "14m 20s" changes to 14:20, and so does the gallery's "Finished in 14m 20s" note in `tests/ui_kit_gallery.rs`.
- **Cost** counts up as the run goes, from each Claude message's token use and the model's price, and reads "About $1.24 of $10.00" while it is an estimate. Under the bar, one line in `Type::SMALL` and `muted`: "Estimated until Claude's turn ends." When the turn's result arrives, About and the line go and the exact figure shows. A pending prompt keeps About, since its turn has not ended. A process that ended with no result, such as after Force stop, keeps About. The bar fills in `text` as now, with no motion: it moves in steps as messages land.
- **Recent runs** shows the same live Cost without About, since the row has no room for it.
- **egui has no tabular figures**, so the ticking figures stay right-aligned in the side column, and their width changes at the left edge only.

### Quiet

An update is any line the run's process prints on its stream: a message, a tool call, a tool result. Jobdesk knows the process is up, and when the last update came. It does not know whether Claude is thinking, writing a long file or stuck, so the words stay calm and never guess.

| Time since the last update | Sweep | Whose turn note | What next note |
| --- | --- | --- | --- |
| Under 30 s | `active`, one pass every 3 s | "Last update 3s ago" | as the band table says |
| 30 s to 2 min | `muted`, one pass every 6 s | "No update for 48s" | as the band table says |
| 2 min and more | a still dashed rule in `muted` | "No update for 3 min", counted in whole minutes | "Long writes can be quiet. Stop run keeps the files so far." |

No quiet state uses `closed` or `turn` ink, and none starts or stops anything. One long Write of a CV can print nothing for minutes, so 2 min is not a failure. The thresholds stay at 30 s and 2 min.

### Where the sweep and the figures stop

| State | Sweep | Elapsed | Cost |
| --- | --- | --- | --- |
| Starting, the process up with no update yet | runs, note "Starting Claude Code" | ticks | $0.00 |
| Working, quiet | as [Quiet](#quiet) says | ticks | About |
| Your turn: a question, a permission prompt or a stop point | gone. The question bar is the one loud thing. | ticks | About while the turn is open (a pending prompt), exact once it ended (a stop point) |
| Stopping | still, a solid `muted` rule | ticks until the process confirms | About |
| Done for now, finished | gone | holds, the final figure | exact |
| Failed, stopped | gone | holds, the final figure | exact, or About when no result came |

### The sweep

A `MARK_RULE` segment, 30% of the cell's width, travels the Whose turn cell's bottom edge, from just outside its left to past its right. No track is drawn, so at rest the band looks as it does today.

- **Working**: `active`, 2.4 s across with ease in and out, then 0.6 s at rest, so one pass every 3 s.
- **Quiet, 30 s to 2 min**: `muted`, half speed, one pass every 6 s. It slows, it does not stop.
- **2 min and more**: the segment stops and the edge becomes a still dashed rule in `muted`, the whole cell wide. A still line reads "nothing is coming in" without reading "broken".
- **Stopping**: a still solid rule in `muted`.
- **Reduced motion**: a still solid rule, `active` while working, `muted` when quiet, dashed after 2 min.
- **Repaint**: about 30 frames a second, only while the sweep shows on screen. Elsewhere, and on the user's turn, the screen repaints once a second while Elapsed ticks, or not at all once the run ended.

Why: the band is the first thing read, and the cell that says Claude's is where "is it still going" is asked. One small moving line in the cell's own ink says the run is live without a fake percent.

### Motion rules

- One element moves on the screen, slowly. Nothing else animates while a run works: no knot, tally or tab pulses.
- It moves only while it is Claude's turn and the Tasks screen shows. Settings › Appearance › Reduce motion swaps it for its still form. Elapsed and the notes keep updating, since text that changes is not motion.
- No fade, glow or blur: flat fills and 1px-class shapes, as the rest of the kit.

### New widgets

| Widget | What it is | Tokens |
| --- | --- | --- |
| `live_sweep` | The moving segment on a band cell's bottom edge, with its quiet, long-quiet, stopping and reduced-motion forms | `MARK_RULE`; `active`, `muted`; new motion constants for the pass, the rest and the quiet pass, beside `MOTION_SECONDS` in `controls.rs` and `MOTION_SLIDE` |
| `estimate_note` | The one muted line under the Cost bar while it is an estimate | `Type::SMALL`, `muted` |

## Outside the page

- **The Tasks tab** carries a stamp-ink count chip of the runs that wait on the user. It reads "Tasks, 1 need you".
- **Today** deals a Your turn card for a run stopped on a question or a known stop point. Its tab and frame are in stamp ink. Its title is the stop's label, such as "/apply: proceed with drafting?", and its body is Claude's question as plain text. Its actions are Later (L), as on every card, and Open task. It has no Y and no Skip, so a card never resumes a run.

## Accessibility

- The band is the group "Where the run is", and its tally reads "1 of 7 steps done".
- The question bar's group reads "Your turn. Claude asks: …".
- A knot reads its number and state, such as "2, waiting for you".
- The sweep is decoration. The Whose turn cell reads "Claude's turn. Last update 3 seconds ago". Screen readers get the change only when the quiet state changes, never each second.
- Cost reads "About 1 dollar 24 of 10 dollars, estimated until Claude's turn ends". Elapsed is read on focus, never announced as it ticks.
