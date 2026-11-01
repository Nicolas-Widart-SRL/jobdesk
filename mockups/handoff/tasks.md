# Tasks

The Tasks screen shows one run: where it is, whose turn it is and what comes next, then the story of what Claude did. It answers one complaint: people ended runs because they did not see that Claude was waiting for them. What the user sees is in [docs/guide.md](../../docs/guide.md#tasks). When Claude waits on the user, and how the rail and quick replies work, is in [ARCHITECTURE.md](../../ARCHITECTURE.md#runtime). How messages and questions render is in [ARCHITECTURE.md](../../ARCHITECTURE.md#trust).

Canvas, in `mockups/`: [Grid-tasks-turn.png](../Grid-tasks-turn.png), an `/apply` run stopped after Evaluate.

## Principles

1. **One loud thing.** Stamp ink appears only when the run needs the user.
2. **A report, not a log.** Claude's messages are the main content.
3. **Plain words first.** Technical rows open only on request.
4. **Say each thing once.** The band says whose turn it is, so the head carries only the title and the actions.

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

## Outside the page

- **The Tasks tab** carries a stamp-ink count chip of the runs that wait on the user. It reads "Tasks, 1 need you".
- **Today** deals a Your turn card for a run stopped on a question or a known stop point. Its tab and frame are in stamp ink. Its title is the stop's label, such as "/apply: proceed with drafting?", and its body is Claude's question as plain text. Its actions are Later (L), as on every card, and Open task. It has no Y and no Skip, so a card never resumes a run.

## Accessibility

- The band is the group "Where the run is", and its tally reads "1 of 7 steps done".
- The question bar's group reads "Your turn. Claude asks: …".
- A knot reads its number and state, such as "2, waiting for you".
