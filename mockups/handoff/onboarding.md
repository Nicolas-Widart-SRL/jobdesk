# Onboarding

First run for a non-technical job seeker who has Claude Code installed and signed in, no API key, and never opens a terminal. Jobdesk makes the folder, downloads ai-job-search, installs its helpers, builds the profile from a CV with the unchanged `/setup`, then hands over to Today for the first search. The flow reads as a short story of steps, not a terminal. Round 1 draws two options, A and B, for steps 1 to 5, and one shared set for steps 6 and 7.

## Contents

- [Canvases](#canvases)
- [The two options](#the-two-options)
- [Rules and why](#rules-and-why): window, words, folder, download, helpers, profile run, questions, failure, recap, first search
- [New widgets](#new-widgets)
- [Doc changes](#doc-changes)
- [Open](#open)

## Canvases

`Onboarding.html?option=a|b&state=…&theme=light|dark`. Small-window states render at 720x560, full-window states at 1280x820. The welcome is small in both options. A is full window from Save location on. B stays small until the recap, which is full window.

| State | What it shows | A PNG | B PNG |
| --- | --- | --- | --- |
| `welcome` | First open: Set up from scratch (Recommended) or I already have an ai-job-search folder, then Start setup | `Onboarding-a-welcome.png`, `-dark` (720x560) | `Onboarding-b-welcome.png`, `-dark` (720x560) |
| `place` | Save location: `Documents/Job search` with Change…, Save here | `Onboarding-a-place.png` | `Onboarding-b-place.png` |
| `download` | Folder made, downloading ai-job-search with a bar, unpacking next | `Onboarding-a-download.png` | `Onboarding-b-download.png` |
| `offline` | Download failed: could not reach the internet, folder still empty, Try again, Get help | `Onboarding-a-offline.png` | `Onboarding-b-offline.png` |
| `checking` | Helpers by purpose: Python already here, Bun checking, LaTeX next | `Onboarding-a-checking.png` | `Onboarding-b-checking.png` |
| `installing` | LaTeX installing with a bar and time left, the password note before macOS asks | `Onboarding-a-installing.png` | `Onboarding-b-installing.png` |
| `toolfail` | LaTeX could not install: Try again, Continue without PDFs, Get help | `Onboarding-a-toolfail.png` | `Onboarding-b-toolfail.png` |
| `cv` | Drop zone empty, the cost note, Read my CV disabled | `Onboarding-a-cv.png` | `Onboarding-b-cv.png` |
| `reading` | CV added, Claude working: plain progress lines, the live sweep, About cost and Elapsed, Stop setup | `Onboarding-a-reading.png` | `Onboarding-b-reading.png` |
| `ask-choice` | A question card from `AskUserQuestion`: two choices and Or write your own answer | `Onboarding-a-ask-choice.png`, `-dark` | `Onboarding-b-ask-choice.png`, `-dark` |
| `ask-text` | A question card from a plain closing question: a text field and Show all Claude wrote | `Onboarding-a-ask-text.png` | `Onboarding-b-ask-text.png` |
| `stopped` | Setup failed partway: the reason, what is kept, Pick up where it stopped, Finish later | `Onboarding-a-stopped.png` | `Onboarding-b-stopped.png` |
| `recap` | Folder, computer, profile 9 of 11, job sites, the cost, Open Jobdesk | `Onboarding-a-recap.png`, `-dark` | `Onboarding-b-recap.png`, `-dark` (1280x820) |

`FirstSearch.html?state=today|launcher|jobs&theme=light|dark`, shared by both options, at 1280x820:

| State | PNG | What it shows |
| --- | --- | --- |
| `today` | `FirstSearch-today.png` | Today with nothing to decide: the clear-state offer panel in first-search words, Find new jobs… with Y, and a Your profile slip |
| `launcher` | `FirstSearch-launcher.png` | New task on `/scrape`, with a note naming the first search. The user presses Run |
| `jobs` | `FirstSearch-jobs.png` | Jobs after the search: 14 new jobs, not scored, and a note with Score 14 new jobs… |

## The two options

- **A, Story rail.** The window grows to the app's size at Start setup. A slip on the left lists the four chapters, Folder, Computer, Profile and Done, each a `chapter_row`: a knot, the name and one line. Past chapters show their result, such as the folder's path. The open chapter takes the pick look, and its knot turns `turn` while Claude asks and `closed` on a failure. The open chapter sits on paper at the right, with an eyebrow ("2 of 4 · Computer"), an `H1` title, a lead and the foot. Why: the whole story is in view at every step, and the done lines are a running recap. It costs a full window for a few lines of content.
- **B, Compact card.** The small window keeps one centred card per step, under `step_dots`. The card's head carries the setup stamp: the day stamp's rings with four ticks, one per step, and "Step 2 of 4" inside. At the recap the window grows into the app, with the menu dimmed, and the recap sits as a card on Today, where the old welcome card sat. Why: it feels lighter and closer to an installer, and the move into the app marks the end. It shows less of what came before, so a failure screen has to say more on its own.
- **Both** share every word, the foot, the lines and the question card, so a choice between them is a choice of frame only.

## Rules and why

### Window

- The first window is small, 720x560, for the welcome in both options. The app's own minimum, 1040x640, applies only once the app opens: A at Start setup, B at Open Jobdesk.
- Choosing I already have an ai-job-search folder opens the folder picker, then the app as today, at its usual size. Start setup reads Choose folder… while that choice is picked.
- No menu and no tabs during setup: nothing else is usable yet. The status strip keeps Settings, so the theme can change.

### Words

The copy never says repo, clone, CLI, scrape, workspace, slash command, terminal, install path or byte counts.

| Instead of | It says |
| --- | --- |
| repository, checkout, workspace | your job search folder |
| clone, release zip | download ai-job-search |
| Python, Bun, LaTeX as the headline | what each is for, the name under it in `muted` |
| `/setup` | building your profile |
| `/scrape` | Find new jobs |
| `/rank` | Score N new jobs |
| tokens, cost | uses your Claude plan, about $N of usage |

ai-job-search keeps its name, as the user may already know it.

### The foot

- One primary per screen, first in the foot, then one plain button, then one muted line at the right, as the deal footer.
- While a step works, the primary stays in place, disabled, and the line says when it opens. This keeps the button from jumping and says what comes next.
- Back shows only where going back undoes nothing: Save location, the helper check and the drop zone. A download or install shows Cancel, and the profile run shows Stop setup.

### Folder

- The default is a new `Job search` folder in the user's Documents folder, shown with `path_label` as `Documents/Job search`. Change… opens the system folder picker.
- Jobdesk makes the folder, then downloads the source zip of the latest ai-job-search release and unpacks it there.
- Progress is three lines (Folder made, Downloading ai-job-search, Unpacking it into your folder), not bytes. The version and "From the ai-job-search page on GitHub" sit in a `Type::NOTE` line under them.
- Offline: an alert says what happened and that the folder is still empty, with Try again and Get help.

### Helpers

- Three lines, in this order: Keeps your tracker (Python), Searches job sites (Bun), Makes your CV and cover letter PDFs (LaTeX). The purpose is the headline, so the names never need explaining.
- Each line ends in one tag: Checking… (`Plain`), Already here or Installed (`Hired`), Installing… (`Active`) with a bar and the time left, or Could not install (`Closed`) with the reason under the name. A helper not reached yet says Next in `muted`.
- Jobdesk checks first, and installs only after Install what is missing.
- The password note shows before the system asks, in a `note` with a `text` square, not stamp ink: nothing waits on the user yet. It says the system window asks, and that Jobdesk never sees the password.
- No command, path or log line is shown. A failed install keeps its log for Get help, not on screen.

### The profile run

- The drop zone takes one PDF or Word file, by drop or Choose a file…. Read my CV stays disabled until a file is added.
- The cost shows before the run starts, in plain words: it uses the Claude plan, about how long and how much, and the cap Jobdesk stops it at, which is the run's Spend at most.
- Read my CV starts the unchanged `/setup` on its single-CV path, as a normal run: one run at a time, its own run log, counted in Usage, and listed in Tasks afterwards.
- While Claude works, the card shows plain progress lines, never Claude's text or a tool row. The lines come from a fixed phrase table keyed on tool and path, like the step rail's sentences:

| Line | Lit by |
| --- | --- |
| Reading your CV | a read of the CV file |
| Writing your profile | writes to `CLAUDE.md` and `.claude/skills/job-application-assistant/` |
| Choosing job sites for your country | writes under `.agents/skills/` |
| Setting your job search terms | a write to `.claude/skills/job-scraper/search-queries.md` |

- The working line carries `live_sweep` and "Last update 3s ago", with the quiet rules from Tasks. Under the lines, About $N so far and Elapsed, as This run counts them.

### Questions

- A question replaces nothing: it shows as a card in the step's place, the same `prompt_box_tone` in stamp ink as the question bar in Tasks, with a solid Claude asks tag. Stamp ink shows only here and on the chapter's knot, since only a question waits on the user.
- From `AskUserQuestion`: the question, then each choice as a radio row with its label and description, then Or write your own answer. Send answer and Decline, as in Tasks. The question always reaches the user.
- From a plain closing question: only that question, then a text field and Send. Show all Claude wrote opens the whole message as a `document` in a popover, so nothing Claude said is hidden.
- All of it is untrusted: the question, the labels and the descriptions render as plain text with Markdown markup and hidden characters removed, as [Trust](../../docs/architecture/trust.md#rendering-untrusted-text) says for the closing question.
- The lead counts answers ("2 questions answered so far"), never a total, since `/setup` decides how many it asks.
- A permission prompt, with Settings › Permissions on, shows in the same place as in Tasks. It is not drawn: setup runs before anyone opens Settings.

### Failure

- Every failure names what happened in plain words, what is kept, one way forward and Get help.
- Setup stopped partway shows the reason as plain text with hidden characters removed. Pick up where it stopped replies on the same session, which resumes it as any run does. Finish later opens the app with what is done, and Profile shows the topics still empty.

### Recap

- Four done lines: Folder saved (with Show in Finder), This computer is ready, Profile written N of 11 topics (with the `fat_tally` of filled topics), Job sites chosen. Then the run's cost and time. Every figure comes from the `Snapshot` and the run log, never from Claude's closing message.
- Open Jobdesk opens Today. The muted line says Today suggests the first search.

### First search

- Today with nothing dealt shows the clear-state offer panel in its first-search words: Find your first jobs, the three steps, and Find new jobs… as the primary with Y, then "Opens the search task. Nothing starts until you press Run." and Show the command. Y only opens the launcher on `/scrape`.
- The launcher opens on `/scrape` with a `from_note` at the top of its slip: "Your first search, from Today." The user presses Run.
- When the search ends, Jobs shows the New view with each job Not scored, and a note above it with Score N new jobs… as the primary. The head keeps Find new jobs… as a plain button, so the page has one primary. Score N new jobs… only opens the launcher on `/rank`.
- Card actions never start a run, in every state drawn here.

## New widgets

| Widget | What it is | Tokens and widgets it uses |
| --- | --- | --- |
| `choice_card` | A pickable card: a radio, a title with an optional tag, a sentence. Picked takes the pick look. | `card`, `raised`, `line`, `text`; `RADIO`, `RADIO_DOT`; `RADIUS_PANEL`; `tag` |
| `chapter_row` (A) | A chapter in the rail: a knot, the name, one line | `knot`; `Type::SECTION`, `Type::NOTE`; `PICK_EDGE`; `pick_look` |
| `step_dots` (B) | One square per step with its name, joined by a hairline | `STATUS_DOT`; `text`, `off`, `line`, `muted`; `Type::SMALL` |
| setup stamp (B) | `day_stamp` drawn with four ticks and the step inside, or the date on the welcome and the recap | `DAY_STAMP_RINGS`, `DAY_STAMP_TICK`; `text`, `off`, `line`, `sheet` |
| `drop_zone` | Paper with a dashed `line` edge, a mini page, the words, and Choose a file… | `card`, `line`; `SWEEP_DASH`; `MINI_PAGE_SIZE`, `page`, `page_line`, `page_head`; `button` |
| `setup_line` | A line on paper: a knot, a title, a line under it, and a tag or bar at the right, with an optional bar row | `knot`, `tag`, `bar`; `ROW_PADDING`, `INSET`; `live_sweep` on the working line |
| `from_note` | A note at the top of the launcher's slip naming what opened it | `card`; `Type::NOTE`; `muted`, `text` |

The question card is `prompt_box_tone(Tone::Turn)` with the choices, field and buttons Tasks already uses. The alert is `alert` on a `panel` with a `closed` edge.

## Doc changes

- `ARCHITECTURE.md`: a feature for first run (or a part of `workspace`): download and unpack the ai-job-search release, check and install the helpers, and start `/setup` through runs' application layer. Add it to the Features table and to `FEATURES` in `tests/architecture.rs`. The runtime note for the small first window and its minimum size.
- `docs/guide.md`: a First run section before Today, and Today's sentence on the welcome with three steps replaced. Setup and the status bar: the status line during setup.
- `docs/architecture/trust.md`: how the release zip and each installer are checked, unpacking that stays inside the chosen folder, the system password prompt, and the setup questions as untrusted text.
- `docs/architecture/runs.md`: the onboarding run: how the CV reaches `/setup`, the fixed progress phrases, and answers sent on the same session.
- `docs/architecture/ui-kit.md`: the new widgets above.

## Open

Picked so far: nothing. Still drawn: A and B. To settle:

1. **A or B.**
2. **Checking the download.** The ai-job-search release has no signed assets, only GitHub's source zip. Options: trust HTTPS to GitHub alone; pin a SHA-256 for each release in Jobdesk's own update feed; or check a signed tag. Unpacking must refuse any entry that leads outside the folder.
3. **Where each helper comes from and how it is checked.** For example the python.org package, Bun's own release, and BasicTeX (about 100 MB) or MacTeX (about 5 GB) or TinyTeX for LaTeX, which decides the time left and whether the PDFs' fonts are there. Packages could be checked by their Apple signature, scripts by a pinned hash.
4. **Which installs ask for the password.** The note is drawn on LaTeX only. If Python's package also needs it, the note moves up so it shows once.
5. **Quitting midway.** What is kept and where the next launch resumes: a half download is removed, a finished helper stays, and the profile run resumes from its run log. Needs a small setup state in the data folder.
6. **A folder that already exists** at the chosen place: empty is fine; an ai-job-search folder could open as it is; anything else asks for another place.
7. **How the CV reaches `/setup`.** `/setup` opens by asking which of its three paths to take, as plain text. Jobdesk could copy the CV into `documents/cv/` and answer that first question itself with a fixed reply naming the single-CV path and the file. Also: can the run read a Word file as is, or does Jobdesk turn it into PDF or text first?
8. **The cost figures.** About $2 for setup, $5 as its cap, $0.40 for the first search and $0.60 for scoring are placeholders, to be measured on the sample folder.
9. **Continue without PDFs.** Can setup finish without LaTeX? Setup's tab would then stay in the menu, since LaTeX is a required tool.
10. **Get help.** Where it goes: a page of the guide on the product site, or a prefilled issue.
11. **Jobs opening by itself** when the first search ends. Proposed: only when the user still has that run open in Tasks. Otherwise the Today offer turns into Score N new jobs….
12. **The launcher's button** stays Run, its built name, though the brief says Start.
13. **The Your profile slip on Today**, with Open Profile, is new: keep it until the profile is full, or drop it.
14. **No git in the folder.** A zip download has no `.git`. `/setup` skips its public-fork check without a remote, and its closing note about git is not shown, but updating ai-job-search later (out of scope here) can no longer use git.
