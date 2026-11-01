# Today

Today deals one decision at a time: the card in the middle, Done today and Coming up in the rail. What it does for the user is in [docs/guide.md](../../docs/guide.md#today), and the queue rules are in [ARCHITECTURE.md](../../ARCHITECTURE.md#runtime).

Canvases, in `mockups/`: [Grid-today.png](../Grid-today.png), [Grid-today-dark.png](../Grid-today-dark.png) and [Grid-firstrun.png](../Grid-firstrun.png).

## The head

- **The day stamp** (`day_stamp`): two rings, one tick per card dealt today, filled as each is decided, and the date inside. A `sheet` disc sits under the outer ring, so no grid line crosses the date. With nothing dealt it shows a quiet empty ring.
- **The headline**, in this order: "Nothing to show yet" when nothing is left and a source failed, "All done for today", "{n} new matches to look at" when every card is a new match, else "{n} things need you". While the first snapshot loads: "Reading your job search…".
- **The subtitle**, while cards wait: "About {m} minutes.", then "{d} done so far today." once something is decided, then "{k} is past its deadline." when cards are late. With a source missing it ends "{Feature} are missing from this list." When nothing is left, it names the next deadline, or says "Nothing waits on you."
- The ranking rule sits in a tooltip on the position counter ("1 of 10"), not in the head.

## The card

The card is paper (`deal`), with the stack's edges under it.

- **The tab** names the kind in its tone, and the date in `muted`. A Your turn card's tab and frame take stamp ink. Claude asks and Check this ad cards take the warning frame.
- **The face**: the company above the role, the role in `Type::DEAL`, then "{location}. Found on {site}."
- **The verdict** names the weakest check in words, never a count of passes.
- **The checks** are a grid of label, value and an OK, Unclear or Fails tag.
- **The body scrolls.** The footer is pinned under it (`deal_footer`), so the actions stay in view at 1040x650 however long the body is.
- **The footer**: the primary action with Y, Skip with N, Later with L, then Open job ad. On the right, in `muted`, one line about what Y does, such as "Opens the drafting task. Nothing starts until you press Start." Under it, Show the command opens the command line in a code box. Showing it starts nothing.

## The rail

Done today, then Coming up, in a rail that sits outside the card's scroll area.

### Done today

The `fat_tally` of today's cards, then the undo row, then the log. With no decision yet, one line: "Each decision lands here. U undoes the last one."

The undo row (`undo_row`) heads Done today because the rail never scrolls with the card, so it stays in view at any window height. It shows the card's name and Undo with its U key, then a note: "Set aside until tomorrow", "Skipped. Why? Optional." or the decision's label. After a skip, the reason toggles Rate, Scope, Domain and Location sit under it and wrap inside the rail.

The row replaces the decision's plain log row, so a decision is never listed twice. Later is never logged, so its row heads the log only while it can be undone. With no offer the row is absent and leaves no gap.

It is not beside the card tab, because there it would have to leave the scroll area and would read as describing the next card. It is not a floating pill either, because a pill covers the card's actions once the body scrolls.

### Coming up

- First, when cards are set aside: "{n} comes back" with a Bring back button. Its tooltip names the cards. It only reorders the queue and never starts a run.
- Then overdue cards, each with "{n} days late" in the warning tone.
- Then the dated cards of the next two weeks, day by day. The card being read takes a solid Now tag in stamp ink and takes no click. A decided row reads in faint ink and ends in Done. Any other row pulls its card back on top.
- At most 6 rows, then "{n} more before {day}" or "Nothing else dated before {day}."
- With no dated card, one line: "No deadlines in the next two weeks."

## Other states

- **Clear**: "Want more to look at?" with Find new jobs… and, while postings wait for scoring, Score {n} new jobs…. Both only open the launcher.
- **Loading**: the empty stamp, "Reading your job search…", and `skeleton` blocks where the card and rail will be. No key acts.
- **First run**: while no folder is open, a paper card on the grid with the empty stamp, "Welcome to Jobdesk", three numbered steps (Choose folder… is the only button) and the folder promise. The other tabs and New task are dimmed.
- **A failed feature**: an `alert` under the head names what could not be read and what still works, and the feature's tab carries the `closed` square. Today still deals the rest.

## Motion

Only two things move. A decision slides the card `MOTION_SLIDE` toward the rail and fades it over `MOTION_TIME` while the next card is in place and its tick fills. When the last tick fills, the stamp settles once from `DAY_STAMP_SETTLE` to its size. `MOTION` in `tokens.rs` turns both off.

## Accessibility

- The stamp reads "Saturday 31 October. 3 of 11 decided today."
- Tabs read "Today, 8 need you" and "Applications, could not be read".
- A Coming up row reads its day, its text, then "now" on the card being read: "Thu 5, Meridian Rail closes, now".
- The undo row reads "Last decision on Harbor Bank: Skipped for scope. Press U to undo."
