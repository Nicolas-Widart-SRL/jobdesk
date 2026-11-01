# The look

The look every screen shares: a gridded desk, paper for what you read, one magenta for what waits on you. Token values live in `src/ui_kit/palette.rs` and `tokens.rs`, so this file names tokens and gives the reasons.

Canvases, in `mockups/`: [Grid-today.png](../Grid-today.png), [Grid-today-dark.png](../Grid-today-dark.png), [Grid-jobs.png](../Grid-jobs.png) and [Grid-tracker.png](../Grid-tracker.png), drawn from `Grid.html`.

## The desk and the paper

The desk is a `HAIRLINE` `grid` line every `GRID_PITCH` over `sheet`. It is painted once behind everything and anchored to the window's top-left, so it holds still while content scrolls. The grid is the desk, and anything you read in rows sits on paper.

- **Opaque**: the paper (`card`: the deal card, the job detail, the first-run card), the slips (`panel`: Done today, Coming up, This run), the Applications lanes and note, the turn band, the index cards and the stack edges. The grid stops at their edges.
- **On paper**: sections that hold rows of text, so no grid line runs between rows. These are the Jobs table, "The run so far", Setup's Profile and Portals, the Insights columns, the dossier sections and the command list. Inside paper the 2px section rule is dropped, and the table head keeps its `text` rule.
- **On the grid**: the menu, page heads (headline, subtitle, page actions), the day stamp, the Jobs segments, the position counter ("1 of 10") and one-line footnotes. Large or single-line text reads fine over the lines.
- **Controls on the grid are opaque**: plain buttons fill `card`, and fields keep `field`. The grid never runs through a control.

## The menu

One row: the brand, the tabs, then New task at the right as a plain button on paper (`shortcut_button`: `card` fill, `line` hairline, keys in `muted`). One row keeps the most height for the page, which matters most at the 1040x650 window the user works in, and it still fits at 1040 wide.

New task stays plain on purpose. A dark button would be a second primary on every screen, competing with the card's Y action and the Jobs and Applications primaries.

## The highlighter

The open tab's label carries a pale swipe in `mark`, `MARK_HEIGHT` tall under the lower half of the label and `MARK_OVERHANG` past each side, with a `MARK_RULE` rule in `turn` along its bottom. The label stays `text` in `Type::TAB_ON`. The tint gives the highlighter look, and the rule carries the brand magenta. Hover draws the same swipe in `raised` with no rule. Inactive tabs are `muted` on the grid.

A full magenta swipe fails: `text` on solid `turn` is 2.41:1 in light, and 1.9:1 in dark. The tint passes:

| | Light | Dark |
| --- | --- | --- |
| Label (`text`) on `mark` | 9.88:1 | 8.83:1 |
| `turn` rule on `mark` | 4.11:1 | 4.64:1 |
| `turn` rule on `sheet` | 5.70:1 | 6.98:1 |
| `mark` against `sheet` | 1.39:1, a tint, not a boundary | 1.50:1 |
| Count chip, `on_turn` on `turn` | 6.35:1 | 7.8:1 |
| `muted` tab label across a grid line | 6.8:1 | 6.96:1 |

Every non-text part that must be seen clears 3:1: the rule and the chip.

The count chip (`attention_count`) stays solid, `turn` with `on_turn`. It sits after the label, outside the swipe, so magenta never stacks on magenta. A feature that failed to load puts a `STATUS_DOT` square in `closed` there instead.

## The status bar

A `sheet` strip with a `line` hairline on top (`status_frame`). Its text and the theme switch never sit on grid lines, and it gives the window a bottom edge: the grid is the desk, and the strip is the desk's edge.

## Stamp ink

The stamp ink, the `turn` token, goes only on what waits on the user:

- the Today and Tasks tab counts, shown only above 0 (Jobs and Applications show no count);
- the Now tag in Coming up;
- the Your turn and Claude asks card tabs on Today;
- the square on a Jobs row that waits in Today;
- the `WAITS_EDGE` edge on an Applications card that waits in Today.

The highlighter is the one place stamp ink marks location, and its pale tint keeps it apart from the solid marks. Stamp ink never marks a status. Job statuses keep their own tones.

## Type

- Zilla Slab: headlines, card titles, lane titles and band values.
- Schibsted Grotesk: the UI and body text.
- Courier Prime: command lines and paths only.

## Jobs and Applications on the grid

- Jobs: the table and the job detail sit on paper. The segments and page actions sit on the grid.
- Applications: the note and the lanes are opaque panels, with the grid showing in the gutters between them. A lane title is a status square in the lane's tone, then the name in `Type::SECTION` (`lane_title`). Index cards fill `card`. Closed applications draw faded, at `FADED_OPACITY`.

## Words

Screens use plain words. The code keeps the feature names, as [ARCHITECTURE.md](../../ARCHITECTURE.md#features) says.

| Screen | Feature | Words on screen |
| --- | --- | --- |
| Today | `intray` | Card kinds: New match, Ready to send, Interview prep, Offer, Follow up, Close it?, Check this ad, Claude asks, Your turn |
| Jobs | `discovery` | Check tags: OK, Unclear, Fails. An unchecked ad gets no tag. |
| Applications | `tracking` | Lanes use the status words, with drafts as "Not sent yet" |
| Insights | `insights` | |
| Tasks | `runs` | Whose turn: Yours, Claude's, Nobody |
| Setup | `workspace` | |

The launcher opens from New task. A command action's label ends in "…", because it opens the launcher before anything runs.
