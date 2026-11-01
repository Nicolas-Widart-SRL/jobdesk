# Jobdesk design review

The user showed Jobdesk to someone who does not code. Their feedback:

- "In-tray" did not say what the tab holds.
- The app feels a bit bland.

This review walks every screen with that person in mind: someone who looks for a job, has not read the README, and does not know what a slash command, a CLI or a gate is.

**Sources**: the committed snapshots in `tests/snapshots/` (rendered by the app from `tests/sample_workspace`) and the copy in `src/`.
The live window I was given showed a real job search, not the sample workspace, so I used none of it. No value from it appears here or in the canvases.

**Canvases**: `Refresh.html`, rendered as `Refresh-*.png`. See the Refresh section of `README.md` for each scene.

## Findings, ranked by impact

Impact means how often a person meets the problem, multiplied by how much it stops them.

### 1. The tab and screen names describe how it is built, not what it is for

**What they see**: In-tray, Jobs, Tracker, Insights, Runs, Workspace, plus a button called "Ask or run".

**Why it hurts**:
- "In-tray" is office jargon from paper desks. Many people have never used one, and to others it reads as email.
- "Runs" and "Workspace" are developer words.
- "Ask or run" does not say what you get.
- The home screen is the one place a new person must understand at once.

**Fix**:

| Today | Proposed | Why |
| --- | --- | --- |
| In-tray | **Today** | It is the day's pass: the strip starts today, the rail says "Done today", and the headline counts what is left today. |
| Tracker | **Applications** | It is the list of applications. "Tracker" names the CSV file. |
| Runs | **Tasks** | Matches the button. A task is one thing Claude does for you. |
| Workspace | **Setup** | It shows whether your profile, tools and job sites are ready. |
| Ask or run ⌘K | **New task ⌘K** | Says what the button makes. The launcher already asks "What do you want to do?". |
| Jobs, Insights | unchanged | Both are plain words already. |

Code names (`intray`, `tracking`, `runs`, `workspace`) stay as they are. Only the labels change.

### 2. The card's buttons are below the fold at the default window size

**What they see**: at the default 1280x820 window, a new-match card ends at "Strengths". Draft application, Skip and Later sit under the fold (`intray_triage_light.png`), so they must scroll to act on the main screen.

**Why it hurts**: the whole screen exists to make one decision, and the decision buttons are the part you cannot see. Keyboard users never notice. Everyone else does.

**Fix**: pin the action row to the bottom of the card, outside its scroll area, the way the Runs dock does. Only the card's body scrolls. Every Refresh canvas does this.

### 3. The card speaks in pipeline words

**What they see** on the main card:
- "Triage", in red.
- "No freelance gate fails, but 1 of 3 need a look at the posting." Six rows show below it, so "1 of 3" matches nothing on screen.
- "Pass" and "Flag" tags.
- "Posting guard: clean".
- "Strong Fit / Triage score, from posting text only".
- "Y follows the suggestion".
- A typewriter line: "Opens /apply <the full job URL> in the launcher".

**Why it hurts**:
- "Triage" is a medical word.
- **Red is the "Closed" status ink** (rejected, withdrawn), so a new opportunity looks like a failure.
- "Gate" and "flag" mean nothing outside the code.
- The command line looks like an error log, and it is the last thing on the card.
- "Y follows the suggestion" repeats the Y on the button.

**Fix**:

| Today | Proposed |
| --- | --- |
| Triage (red) | **New match** (active blue) |
| Send | Ready to send |
| Prep | Interview prep |
| Follow-up | Follow up |
| Close | Close it? |
| Guard flag | **Check this ad** |
| Permission, Question | Claude asks |
| "No freelance gate fails, but 1 of 3 need a look at the posting." | "Good fit, 78 of 100. Five checks pass. The day rate is not in the ad, so check it before you apply." The sentence names the check, not a count. |
| Pass, Flag, Fail | **OK, Unclear, Fails** |
| Posting guard: clean | **Safety check: Nothing hidden in the ad** |
| Strengths, Gaps | Why it fits, What is missing |
| Strong Fit / Triage score, from posting text only | "Strong fit, 84 of 100. … Scored from the job ad alone." |
| Draft application (Y opens the launcher) | **Draft application…** The ellipsis is the macOS and Windows convention for "this opens a step before anything happens". |
| Y follows the suggestion | removed |
| Opens /apply https://… in the launcher | "Opens the drafting task. Nothing starts until you press Start." The command appears only after a click on **Show the command**. |
| Open posting | Open job ad |
| Cleared today: "Command opened" | "Draft application… opened", using the action's own name |

Keep "posting" in docs. On screen, "job ad" is the everyday word.

### 4. The day strip shows 14 empty boxes when nothing is dated

**What they see**:
- On a day of new matches only, 13 of the 14 cells are blank, with "No deadlines in the next 14 days" across them.
- Today's cell says "6 to decide / 6 triage", which repeats the headline above it.
- The strip costs about 90px at the top of every Today screen.

**Why it hurts**:
- It is the most visible "empty" element of the app, and it says nothing.
- At the user's 1040x650 window, those 90px are about 15% of the window, taken from the card.
- Struck-through names ("Harbor Bank") read as cancelled, not done.

**Fix (recommended direction B)**: replace the strip with a **Coming up** list in the side column.
- One line per dated card: "Thu 5, Meridian Rail closes".
- The card being read is marked Now, and decided ones read Done in faint ink, not struck through.
- The set-aside return is its own row: "Sun 1, 1 comes back [Bring back]".
- With no dates, the whole list is one line: "No deadlines in the next two weeks."

Directions A and C keep a horizontal strip, 54 to 60px tall, without the duplicated counts.

### 5. The headline and subtitle explain the sorting rule, not the day

**What they see**: "10 decisions, about 15 minutes" and "Paused runs come first, then deadlines, then the rest."

**Why it hurts**:
- "Decisions" is abstract.
- The subtitle is the ranking algorithm, and it never changes. It says nothing about today.
- In the clear state, "Nothing needs you" sits above that same sorting rule, with "Tray clear" and `/scrape` in a box below.

**Fix**:
- Headline: "8 things need you". When every card is the same kind, say so: "6 new matches to look at".
- Subtitle: "About 12 minutes. 3 done so far today." When something is overdue: "1 is past its deadline."
- Clear state: "All done for today", then "11 decisions. Your next deadline is Thu 5 Nov, Meridian Rail Systems."
- Clear-state actions: "Find new jobs…" and "Score 2 new jobs…" as buttons, not a code box.
- The ranking rule moves to a tooltip on the position counter ("1 of 8").

### 6. Tab counts mean three different things

**What they see**: "In-tray 10", "Jobs 2" and "Tracker 7", in the same small grey type.

**Why it hurts**:
- In-tray counts what needs you. Jobs counts unscored postings. Tracker counts open applications.
- The same mark means three things, so none of them can be trusted at a glance.

**Fix**:
- Only counts that need you show on a tab: Today's pending count, and Tasks when a run waits for a reply. Both use the stamp ink.
- A tab whose data failed to load shows a small square in the closed ink.
- Jobs and Applications show their counts on their own screens, where they are labelled.

### 7. File names and command names everywhere

**What they see**:
- Status bar: "Slash commands stay the only writers of your files." on most screens.
- Jobs status bar: "Source: job_scraper/seen_jobs.json".
- Tracker subtitle: "Read from job_search_tracker.csv. Status changes go through /outcome."
- Jobs subtitle: "Triage scores from posting text only. /apply re-evaluates with company research."
- Job sources in Courier: "linkedin-…", "freehire-…".
- Dossier subtitle: the archive folder path in Courier.

**Why it hurts**:
- Each line is true and matters to the builder. To a job seeker, each is noise or, worse, a warning they cannot read.
- The status bar line is the most repeated sentence in the app, and it means nothing without the README.

**Fix**:

| Today | Proposed |
| --- | --- |
| Status bar promise | Move it to Setup, as "Jobdesk reads your folder and never changes it. Only a task you start can change it, through Claude Code on this computer." It also opens the first-run card. |
| Status bar | "Claude is ready", or "Claude Code is not installed" with a link to Setup |
| Jobs subtitle | "Scored from the job ad alone. Drafting an application also checks the company." |
| Applications subtitle | "7 open, 3 closed. 4 of them wait for you in Today." |
| Dossier subtitle | "Applied 7 Oct through a job site. Interview stage." The path moves behind a "Show in Finder" button. |
| Source column | "Found on: LinkedIn". Map portal ids to names, and fall back to the id. |
| Insights band | Profile, Find, Score, Apply, In progress, Result. These are the steps, not the command names. |

### 8. There is no first run

**What they see**: one small slip, "Choose your ai-job-search folder / Jobdesk reads your checkout and never changes it. / Choose folder".

**Why it hurts**:
- It does not say what the app does, or what the folder is.
- Nothing follows it. After choosing a folder with an empty profile, Today shows "Tray clear" and `/scrape`.

**Fix (`Refresh-B-firstrun.png`)**: a welcome card with three steps.
1. Choose your job search folder.
2. Check your profile, in Setup.
3. Find your first jobs, which opens the search task.

- It says once, in plain words, what Jobdesk will and will not do.
- After step 1, Today shows the remaining steps until the profile is filled in and one search has run.

### 9. The screen has one level of loudness

**What they see**:
- Light theme: desk `#bdccdc`, sheet `#eff3f8`, folder `#dfe7f0`, field `#fafcfe`. Four blue-greys within about 15% lightness of each other.
- The primary button and text are the same navy.
- Colour appears only on status tags.
- The decision card is `panel` on `sheet`, a 6% step, so it barely lifts off the page.

**Why it hurts**: this is the "bland" in the feedback.
- Nothing on the main screen says "start here".
- The card, the strip and the rail share one tone and one weight.
- The eye lands on the 44px headline and then has nowhere to go.
- In dark, the planes are better separated, so the light theme reads flatter than the dark one.

**Fix**: see Visual directions below. In short:
- The card becomes white paper.
- One accent ink means "this needs you", and nothing else.
- A navy frame gives the app an edge.
- The day stamp is the one memorable element.

### 10. Labels that need decoding

| Screen | Today | Proposed |
| --- | --- | --- |
| Jobs | Below threshold, Excluded, Could not fetch | Low score, Ruled out, Could not open |
| Jobs | Expired 0 | Hidden when 0 |
| Jobs | Signals | Worth knowing |
| Jobs | Find new postings, Score new postings | Find new jobs…, Score 2 new jobs… |
| Applications | Drafted (lane) | Not sent yet |
| Applications | "49d quiet, 2 of 2 follow-ups" | "No reply in 49 days, followed up twice" |
| Applications | Bare "70" | "fit 70" |
| Applications | Withdrawn | You withdrew |
| Applications | "3 applications quiet 10+ days can get a follow-up. 0 quiet 60+ days ready to resolve." | "3 applications have had no reply for 10 days or more. A short follow-up often gets one." Drop any clause whose count is 0. |
| Insights | "6/7 skill files filled", "new of 10 seen", "Calibrate" | "6 of 7 profile parts filled", "2 new of 10 found", "Tune scoring" |
| Insights | Status bar: "Drafted rows stay out of every statistic" | Footnote under the funnel: "Applications not sent yet are not counted." |
| Tasks (Runs) | Step rail "0 Fetch" | Number from 1. The RunTurn round already redraws this rail. |
| Tasks (Runs) | "Ready for reply", "End session" | Covered by the RunTurn round ("Your turn"). |
| Tasks (Runs) | "All commands / Every action is the unchanged slash command. Forms only build its arguments." | "What Claude can do for you". Each row leads with the plain name ("Find new jobs"), with the command in faint type after it. |
| Tasks (Runs) | "Sent to Claude", "Runs in your repo checkout, with your project settings and permissions." | "Claude will run", "Runs in your job search folder." |
| Tasks (Runs) | "Budget cap, USD" | "Spend at most, USD" |
| Setup | `01-candidate-profile.md` in Courier as the row name | "Your profile" first, with the file in faint type under it |
| Setup | "Find signals" | "Look for more in your documents…" |
| Setup | "bun / Portal CLIs / Missing" | "Job site search: missing bun. How to install" |
| Setup | "Sign-in is known after the first run." | "You will see which account Claude uses after the first task." |
| Dossier | "Outcome and follow-ups" | Results and follow-ups |
| Dossier | "Keyword coverage, from CV text layer" | "Keywords from the ad, found in your CV" |

### 11. Smaller things

- **The stack of card edges** is Ledger's best idea, and it carries "8 behind this one" well. Keep it.
  - Drop the Courier line under it. The position "1 of 8" now sits in the tab row, and the edges already show the depth.
- **Courier** should appear only when the user asks to see a command or a path.
  - Today it also draws "9 behind this one", the launcher trace, file names in Setup and portal ids.
  - It makes the app look like a terminal where nothing is code.
- **The score stamp** (double-ruled 84) is distinctive and reads as a verdict. Keep it. It now carries the fit word in the sentence beside it, not a separate "Strong Fit" label.
- **Undo** works and is in view, after the TrayUndo round. Its note now uses plain verbs ("Skipped, too far").
- **The guard card** says "This card has no one-key way forward, on purpose." That talks about the design, not the job.
  - Say: "This ad contains hidden text meant for an AI. Read what was found before you keep it." Then show the verdict as plain text, as today.
- **The welcome slip and the error alert** use the same `alert` red text without a frame, so a load failure looks like a line of copy.
  - `Refresh-B-today-failed.png` frames it with the closed ink and names what still works.

## Visual directions

All three keep egui's means: flat fills, 1px strokes, per-corner radii, painter lines and circles, text and the hard offset shadow Ledger already uses. None needs blur, gradients or soft shadows. C's grid is a set of 1px lines painted once behind the central panel.

| | A. Ledger, tidied | B. Postmark (recommended) | C. Graph pad |
| --- | --- | --- | --- |
| Idea | Today's look, with the hierarchy and copy fixed | Ledger in a navy frame, with one ink that means "this needs you" | An engineer's pad: green grid paper, graphite, a highlighter |
| Palette | Existing tokens. The card uses `page` white. | Frame `#12263f` (the light theme's ink as a fill), frame tab `#233b59`, stamp `#b0246a` (light) and `#f48cc0` (on navy and in dark), cards `page` white. Status inks unchanged. | Pad `#e4ece0`, grid `#cbdac4`, sheet `#f9fbf7`, graphite `#1d2a25`, muted `#44534c`, highlighter `#f2e35c` |
| Type | Zilla Slab and Schibsted Grotesk. Courier only on request. | The same faces. The headline drops from 44 to 36 so the stamp and headline share one line. | Atkinson Hyperlegible Next for everything, a face made for low-vision readability |
| Accent use | Status inks only | Stamp ink only for what blocks on you: Today's count, the Now mark, cards in Applications that wait in Today, and Claude asks or Your turn cards | Highlighter under today, the open tab and the Today count |
| Signature | The card stack and a tally of the day | **The day stamp**: a postmark with today's date, whose ring holds one tick per card dealt today, filled as you decide | The grid, with highlighter strokes |
| Texture or illustration | None | The postmark's double ring. No illustration. | The 20px grid on the desk |
| Motion | None new | One moment, answering the user: on a decision the card slides 12px toward the rail and fades out (150 ms), and one ring tick fills. When the last tick fills, the stamp settles from 104% to 100% once. A setting turns motion off. | The highlighter draws under a newly opened tab (120 ms) |
| Less bland? | A little. Clearer, but still one blue-grey. | Yes. The frame gives an edge, the white card lifts, and the magenta is rare enough to mean something. | Yes, and the most different. It risks reading as a notebook app, not a job desk. |
| Builder cost | Small | Medium | Large: new palette in both themes, a new face, every snapshot |

**Why B**:
- It answers both pieces of feedback without throwing Ledger away. The faces, the stamp, the tallies, the folder tabs and the status inks all stay.
- The one new idea has a meaning. The magenta marks only what needs you, so a glance at any screen answers "is anything waiting on me?".
- The day ring makes progress visible without a gamified score.
- The navy frame borrows the dark theme's values, so the frame is the same in both themes. That is the strongest identity move, and it costs one frame fill.
- It reuses the stamp magenta the RunTurn round is testing for "Your turn", so the two rounds converge on one accent.
- It is the only direction where the copy, the layout and the look fix the same thing: telling a non-technical person what to do next.

**What B does not do**:
- It adds no illustration, mascot or gradient.
- The empty states use words and two buttons, not pictures. A picture would be the decoration the user's tester would notice first and need least.

## Doc changes (for the lead)

See the Refresh section of `README.md` for the exact wording.
