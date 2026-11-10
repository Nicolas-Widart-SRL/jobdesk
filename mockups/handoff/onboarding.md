# Onboarding

First run for a non-technical job seeker who has Claude Code installed and signed in, no API key, and never opens a terminal. Jobdesk makes the folder, downloads ai-job-search, installs its helpers, builds the profile from the user's documents with the unchanged `/setup`, then hands over to Today for the first search. The user picked option B, the compact card wizard. Round 2 draws B only, fitted to the shape of a real `/setup` session: plain-text question batches, a proposal to confirm, and a CV to check.

## Contents

- [Canvases](#canvases)
- [Rules and why](#rules-and-why): window, words, foot, folder, download, helpers, quitting midway, documents, the profile run, question sheet, review and CV check, choice card, failure, recap, first search
- [New widgets](#new-widgets)
- [Doc changes](#doc-changes)
- [Open](#open)

## Canvases

`Onboarding.html?state=…&theme=light|dark`. One centred card per step under `step_dots`, the setup stamp in the card's head, one primary per screen. Folder and Computer render at 720x560, the Profile chapter and the recap at 1280x820.

| State | What it shows | PNG |
| --- | --- | --- |
| `welcome` | First open: Set up from scratch (Recommended, about 40 minutes) or I already have an ai-job-search folder, then Start setup | `Onboarding-welcome.png`, `-dark` (720x560) |
| `resume` | Next open after quitting midway: Welcome back, the four steps with Folder and Computer done, Finish my profile, the spend so far | `Onboarding-resume.png` (720x560) |
| `place` | Save location: `Documents/Job search` with Change…, Save here | `Onboarding-place.png` (720x560) |
| `notempty` | The chosen folder has files: refused, with a new subfolder inside it suggested, Use this new folder | `Onboarding-notempty.png` (720x560) |
| `download` | Folder made, downloading ai-job-search with a bar, then checking, then unpacking | `Onboarding-download.png` (720x560) |
| `offline` | Download failed, no internet: folder still empty, Try again, Get help | `Onboarding-offline.png` (720x560) |
| `mismatch` | "This download doesn't match. Nothing was saved." with Try again | `Onboarding-mismatch.png` (720x560) |
| `checking` | Helpers by purpose: Python already here, Bun checking, LaTeX next; they install for this user only | `Onboarding-checking.png` (720x560) |
| `installing` | LaTeX installing with its size, time and a bar; each file checked | `Onboarding-installing.png` (720x560) |
| `toolfail` | LaTeX could not install: Try again, and Continue without PDFs as a quiet link | `Onboarding-toolfail.png` (720x560) |
| `docs` | Profile chapter, window grown: the drop zone for several documents, What comes next, Your Claude plan | `Onboarding-docs.png` |
| `dropped` | CV and LinkedIn export listed with Remove, a slim drop zone for more, Read my documents | `Onboarding-dropped.png` |
| `reading` | Claude reading: plain progress lines, the live sweep, spend and Elapsed, Stop setup | `Onboarding-reading.png` |
| `sheet-b1` | Question sheet B1: Claude's 13 questions at the left, one large pre-numbered answer field at the right | `Onboarding-sheet-b1.png` |
| `sheet-b2` | Question sheet B2 (recommended): each question with its own field, optional ones marked, Your answers, Draft saved | `Onboarding-sheet-b2.png`, `-dark` |
| `review` | Claude's proposed changes, grouped by section, Apply these changes and Change something | `Onboarding-review.png` |
| `working` | Claude working between batches: progress lines, "this can take a few minutes", So far | `Onboarding-working.png` |
| `followup` | Two short follow-up questions, in the B2 sheet | `Onboarding-followup.png` |
| `cvcheck` | The generated CV as pages, Looks good and Ask for a change | `Onboarding-cvcheck.png` |
| `choice` | The `AskUserQuestion` card, kept as a secondary state | `Onboarding-choice.png` |
| `stopped` | Setup stopped partway: the reason, what is kept, Pick up where it stopped, Finish later | `Onboarding-stopped.png` |
| `recap` | In the app: folder, computer, profile 9 of 11, CV made, job sites, Add a job site you use…, time and spend | `Onboarding-recap.png`, `-dark` |
| `recap-nopdf` | The recap after Continue without PDFs: LaTeX not yet, CV written without its PDF, Setup stays in the menu | `Onboarding-recap-nopdf.png` |

`FirstSearch.html?state=today|launcher|jobs&theme=light|dark`, unchanged this round, at 1280x820:

| State | PNG | What it shows |
| --- | --- | --- |
| `today` | `FirstSearch-today.png` | Today with nothing to decide: the clear-state offer panel in first-search words, Find new jobs… with Y, and a Your profile slip |
| `launcher` | `FirstSearch-launcher.png` | New task on `/scrape`, with a note naming the first search. The user presses Run |
| `jobs` | `FirstSearch-jobs.png` | Jobs after the search: 14 new jobs, not scored, and a note with Score 14 new jobs… |

## Rules and why

### Window

- The first window is small, 720x560: welcome, resume, Folder and Computer. These steps hold a few lines each, and a small window reads as an installer, not an app to learn.
- The window grows once, when the Profile chapter opens, to 1280x820, the app's own default size, and keeps that size through the recap and into the app. Why: a 13-question batch at the reading measure (`DOC_MEASURE`) plus an answer field per question needs about 1100 by 700 points. At 720x560 the user sees two or three questions at a time and writes a 900-character reply into a narrow box. The app's minimum, 1040x640, would fit but grows a second time at the recap; one grow is calmer than two.
- On a screen smaller than 1280x820, the window takes the screen's work area, never below the app's minimum.
- The Profile card keeps B's look: centred, `step_dots` above it, the setup stamp in its head, one primary in its foot. It widens to 1120 and splits into a main column and a 340 side column (`SPLIT_SIDE_BASIS`, `SPLIT_GAP`).
- No menu and no tabs until the recap: nothing else is usable yet. The status strip on the welcome keeps Settings, so the theme can change.
- Choosing I already have an ai-job-search folder opens the folder picker, then the app as today, at its usual size.

### Words

The copy never says repo, clone, CLI, scrape, workspace, slash command, terminal, install path, hash, checksum or byte counts.

| Instead of | It says |
| --- | --- |
| repository, checkout, workspace | your job search folder |
| clone, release zip | download ai-job-search |
| SHA-256, checksum | Jobdesk's own record of it |
| Python, Bun, LaTeX as the headline | what each is for, the name under it in `muted` |
| `/setup` | building your profile |
| `/scrape` | Find new jobs |
| `/rank` | Score N new jobs |
| `/add-portal` | Add a job site you use… |
| tokens, cost | uses your Claude plan, About $N so far |

ai-job-search keeps its name, as the user may already know it.

### The foot

- One primary per screen, first in the foot, then one plain button, then one muted line at the right, as the deal footer.
- While a step works, the primary stays in place, disabled, and the line says when it opens. While Claude works there is no primary, only Stop setup.
- Back shows only where going back undoes nothing: Save location, the helper check and the drop zone. A download or install shows Cancel, the profile run shows Stop setup.
- A quiet action is a `link_button` after the primary, in place of the plain button. Only Continue without PDFs uses it.
- In every Profile state the muted line holds the run's spend so far and Elapsed: "About $2.18 so far, on your Claude plan · 18:40".

### Folder

- The default is a new `Job search` folder in the user's Documents folder, shown with `path_label`. Change… opens the system folder picker.
- A chosen folder that exists and holds anything is refused: an alert says Jobdesk sets up only in a new or empty folder, so nothing of the user's is overwritten. The card suggests a new `Job search` folder inside the chosen one, with a `Plain` Suggested tag, and Use this new folder as the primary. Change… stays. An empty folder is used as it is.
- A folder that is already an ai-job-search folder is refused the same way. The muted line points to the welcome's I already have an ai-job-search folder.

### Download

- Four lines: Folder made, Downloading ai-job-search, Checking the download, Unpacking it into your folder. Progress is lines and a bar, never bytes.
- Jobdesk checks the zip against the SHA-256 that Jobdesk's own release feed pins for that ai-job-search release, before anything is unpacked. The note under the lines says so in plain words: "compares the download with its own record of version 1.7.5".
- A mismatch is a plain failure: the alert reads "This download doesn't match. Nothing was saved.", the zip is deleted, the folder stays empty, Try again downloads afresh. The Checking line shows a `Closed` No match tag.
- Offline: the alert says what happened and that the folder is still empty, with Try again and Get help.
- Unpacking refuses any entry that leads outside the folder.

### Helpers

- Three lines, in this order: Keeps your tracker (Python), Searches job sites (Bun), Makes your CV and cover letter PDFs (LaTeX). The purpose is the headline, so the names never need explaining.
- All three install in the user's home folder, with no administrator password: a standalone Python build, Bun in its own home folder, and TinyTeX for LaTeX. Each file is checked against a hash Jobdesk pins. The note under the lines says they install for this user only and nothing else on the computer changes. No password note appears anywhere.
- A missing helper's line names its size and time in plain words: Python "about 40 MB" and Bun "about 30 MB", each under a minute, and LaTeX "about 250 MB, the slow one: 5 to 15 minutes". The lead says LaTeX is the slow one and the user can step away.
- Each line ends in one tag: Checking… (`Plain`), Already here or Installed (`Hired`), Installing… (`Active`) with a bar and the time left, or Could not install (`Closed`) with the reason under the name. A helper not reached yet says Next in `muted`.
- Jobdesk checks first, and installs only after Install what is missing.
- When LaTeX fails: Try again is the primary, Continue without PDFs the quiet link. The paragraph says what still works and that Setup can install it later. Setup goes on to the Profile chapter; the CV check is skipped, since no PDF can be made.

### Quitting midway

- Quitting at any step is safe. Jobdesk keeps a small progress record in its data folder: the folder's path, which steps are done, the setup run's id, and the draft answers of an open question sheet. The welcome says "Quit any time: setup picks up where you stopped."
- The next open shows `resume` in place of the welcome: Welcome back, "Your folder and tools are ready. Let's finish your profile.", the four steps with the done ones ticked, and Finish my profile. The muted line names the day the user stopped and the spend so far.
- A half download or a half install is removed on the next open and that step starts again. A finished helper stays. The profile run resumes on its own session, as Pick up where it stopped does.
- The record is cleared when the recap opens.

### Documents

- The drop zone takes several files at once, by drop or Choose files…: the CV, a LinkedIn profile saved as a PDF, diplomas, reference letters, as PDF or Word files. A link opens the user guide's part on saving a LinkedIn profile as a PDF.
- Added files show in a `file_list`: a mini page, the file name as plain text with hidden characters removed, its page count, and Remove. A slim drop zone under it takes more.
- Read my documents stays disabled until one file is added. The muted line counts them.
- Jobdesk copies the files into the folder's `documents/`, never moves them. A name that already exists there gets a number added.
- The side column says what comes next in plain words: rounds of questions, answered in one go, and that setup took one tester about 40 minutes, most of it writing answers. It names no price: the Claude plan pays, and Jobdesk shows the spend as it goes.

### The profile run

- Read my documents starts the unchanged `/setup` as a normal run: one run at a time, its own run log, counted in Usage, listed in Tasks afterwards.
- `/setup` opens by asking, in plain text, which of its three paths to take. Jobdesk answers that first message itself with a fixed reply that picks the documents-folder path (Path A). The user never sees the path question.
- While Claude works, the card shows plain progress lines, never Claude's text or a tool row. The lines come from a fixed phrase table keyed on tool and path, like the step rail's sentences:

| Line | Lit by |
| --- | --- |
| Documents copied into your folder | Jobdesk's own copy, before the run starts |
| Reading your documents | a read under `documents/` |
| Comparing your CV with your other documents | Claude's first message after the reads, before batch 1 |
| Writing your profile | writes to `CLAUDE.md` and `.claude/skills/job-application-assistant/` |
| Applying the changes you approved | writes after the Apply these changes reply |
| Making your CV | a write under `cv/` or a LaTeX build |
| Choosing job sites and search terms | writes under `.agents/skills/` or to `.claude/skills/job-scraper/search-queries.md` |

- The working line carries `live_sweep` and "Last update 4s ago", with the quiet rules from Tasks ("No update for 48s. Long writes can be quiet."). The lead says "This can take a few minutes", because Claude's turns took 1 to 3 minutes each.
- The spend is the run's real spend so far, "About $N so far, on your Claude plan", as This run counts it, with Elapsed. No estimate of the whole setup shows anywhere, and the welcome gives only the time.
- The side column, while Claude works, says the user can step away and keeps a So far slip: rounds answered and changes applied.

### Question sheet

`/setup` asks in plain text, in numbered batches, and never through `AskUserQuestion`. Each of Claude's messages that ends with numbered questions opens a question sheet. The sheet's title counts the questions ("Claude has 13 questions"), and the lead names the step and the round.

Claude's message renders as Markdown with no image loading and no links that open, headings capped at the section size, bold lines and numbered items kept, as Claude's replies render in Tasks ([Trust](../../docs/architecture/trust.md#rendering-untrusted-text)). Hidden characters are removed before parsing.

Two answer layouts are drawn:

- **B1, one large field.** The message sits at the left at the reading measure, with its own scroll. At the right, one large field that Jobdesk pre-fills with the numbers, "1." to "13.", one per line. Send sends the field as it is.
- **B2, a field under each question (recommended).** Each numbered item of the message is followed by its own answer field, which starts one line high and grows with its text. A question whose text ends in "(optional)" shows "Optional. Leave it empty to skip." in its empty field. On Send, Jobdesk joins the answers into one numbered reply in Claude's order, and an empty one is sent as "No answer", so the numbers always match.

Why B2: each answer sits under its own question, so the user never loses count across 13 items and two scrolls, and an optional question is marked where it is asked. The real user wrote one numbered reply of about 900 characters; B2 builds exactly that reply. B1 asks the user to keep numbers aligned by hand, and one missing line shifts every later answer.

B2's cost is that Jobdesk must find the numbered items in Claude's Markdown. It reads only ordered list items, numbered across the whole message. When the message has no numbered items, or the numbers do not run in order, the sheet falls back to B1's single field, unnumbered, under the message.

Both layouts share:

- **Short answers.** The side slip says "Short answers are fine." and how many are left and optional. A `tally` counts the answered fields.
- **Save draft.** Answers save as the user types, to the progress record, and the slip says when ("Saved as you type, last at 14:32. You can close Jobdesk and come back."). No Save button: a user who takes 15 minutes never has to remember one. The draft is cleared once Send succeeds.
- **Show all Claude wrote** opens the whole message as a `document` in a popover, so nothing Claude said is hidden by the sheet.
- **Send answers** with Cmd Enter is the primary; Stop setup is the plain button.
- The same sheet takes the short follow-ups ("Two quick checks"): two questions, two fields, the same Send.

### Review and CV check

- **Review card.** When Claude's message proposes changes and ends by asking to confirm, the card shows the proposal as a `document` in the main column, grouped by Claude's own section lines, with its numbered items. The title says "Claude proposes changes to your profile" and the lead says nothing changes until the user says so.
- Apply these changes sends a fixed reply, "Yes, apply these changes.", which the side slip shows in `mono` before the user presses it. Change something opens a field under the document with Send and Cancel, in place of the two buttons, and sends the user's words as the reply.
- Jobdesk tells a proposal from a question batch by Claude's numbered items and its closing request to confirm. When it cannot tell, the message opens in the question sheet's B1 fallback, which still reaches the user.
- **CV check.** When `/setup` has built the CV PDF and asks the user to check it, the card shows its pages as page images, as Profile › CV documents draws them, with Open PDF in the side slip. Looks good sends the fixed reply "The CV looks good." Ask for a change opens a field, as Change something does.
- The PDF renders as page images only, as [Trust](../../docs/architecture/trust.md#cv-files) says.

### Choice card

- When Claude asks with `AskUserQuestion`, the card in stamp ink from round 1 shows instead of the sheet: the question, each choice as a radio row with its label and description, Or write your own answer, Send answer and Decline, as in Tasks. The question always reaches the user.
- It is a secondary state: the real `/setup` never used it, but another run might.
- A permission prompt, with Settings › Permissions on, shows in the same place as in Tasks.

### Failure

- Every failure names what happened in plain words, what is kept, one way forward and Get help.
- Get help opens the user guide's First run section, on the product site.
- Setup stopped partway shows the reason as plain text with hidden characters removed. Pick up where it stopped replies on the same session, which resumes it as any run does. Finish later opens the app with what is done; the next open offers `resume`.

### Recap

- Five done lines: Folder saved (with Show in Finder), This computer is ready, Profile written N of 11 topics (with the `fat_tally` of filled topics), Your CV is made (with Open PDF), Job sites chosen.
- Then one optional line with a hairline `add` knot: "Use a job site that is not listed?" with Add a job site you use…. It opens the launcher on `/add-portal`, which asks for the site's address and writes a new search tool. It never starts a run. It is for users whose country's boards are not built in.
- Under the lines: "Setup took 41 minutes and used $4.12 of your Claude plan." Every figure comes from the `Snapshot` and the run log, never from Claude's closing message.
- **Without PDFs.** The computer line turns `fail` with a `Plain` Not yet tag and says LaTeX is not installed, so PDFs are not ready yet. The CV line says the CV is written and its PDF comes once LaTeX is installed. The Setup tab stays in the menu, and the muted line says so.
- Open Jobdesk opens Today, which suggests the first search.

### First search

- Today with nothing dealt shows the clear-state offer panel in its first-search words, with Find new jobs… as the primary with Y. Y only opens the launcher on `/scrape`, with a `from_note`: "Your first search, from Today." The user presses Run.
- When the search ends, Jobs opens by itself only while that run is still open in Tasks. Otherwise the Today offer turns into Score N new jobs….
- Jobs shows the New view with each job Not scored, and a note with Score N new jobs… as the primary, which only opens the launcher on `/rank`.
- Card actions never start a run, in every state drawn here.

## New widgets

| Widget | What it is | Tokens and widgets it uses |
| --- | --- | --- |
| `choice_card` | A pickable card: a radio, a title with an optional tag, a sentence. Picked takes the pick look. | `card`, `raised`, `line`, `text`; `RADIO`, `RADIO_DOT`; `RADIUS_PANEL`; `tag` |
| `step_dots` | One square per step with its name, joined by a hairline | `STATUS_DOT`; `text`, `off`, `line`, `muted`; `Type::SMALL` |
| setup stamp | `day_stamp` drawn with four ticks and the step inside, or the date on the welcome and the recap | `DAY_STAMP_RINGS`, `DAY_STAMP_TICK`; `text`, `off`, `line`, `sheet` |
| `drop_zone` | Paper with a dashed `line` edge, stacked mini pages, the words, Choose files…; a slim form under a file list | `card`, `line`; `SWEEP_DASH`; `MINI_PAGE_SIZE`, `page`, `page_line`, `page_head`; `button` |
| `file_list` | One row per added file: a mini page, the name as plain text, the page count, Remove | `paper`, `line`; `ROW_PADDING`, `INSET`; `Type::MONO`, `Type::NOTE`; small `button` |
| `setup_line` | A line on paper: a knot, a title, a line under it, and a tag or bar at the right, with an optional bar row | `knot`, `tag`, `bar`; `ROW_PADDING`, `INSET`; `live_sweep` on the working line |
| `question_sheet` | Claude's message as a `document`, each numbered item followed by a growing field; falls back to one field | `markdown`, `DOC_MEASURE`; `field`, `FIELD_HEIGHT`, `FIELD_GAP`; `Type::TAKE`, `muted` for the numbers |
| answers slip | The side slip of the sheet: count and `tally`, Short answers, Draft saved, Show all Claude wrote | `panel`; `Type::SECTION`; `TALLY_FAT`; `link_button` |
| `cv_check` | The CV's pages as page images in the card, with the side slip's Open PDF | `PAGE_SIZE`, `PAGE_GAP`, `page`; the CV documents page renderer |
| `from_note` | A note at the top of the launcher's slip naming what opened it | `card`; `Type::NOTE`; `muted`, `text` |

The choice card is `prompt_box_tone(Tone::Turn)` with the choices, field and buttons Tasks already uses. The alert is `alert` on a `panel` with a `closed` edge. The quiet Continue without PDFs is `link_button`.

In dark, `panel` and `card` are the same color, so a slip, alert or `ahead` knot on the Profile card takes `sheet` there.

## Doc changes

- `ARCHITECTURE.md`: a feature for first run (or a part of `workspace`): the progress record, download and check the ai-job-search release against the feed's pinned SHA-256, install the helpers into the home folder against pinned hashes, copy the documents, and start `/setup` through runs' application layer. Add it to the Features table and to `FEATURES` in `tests/architecture.rs`. The runtime note for the small first window, the one grow at the Profile chapter, and its sizes.
- `docs/guide.md`: a First run section before Today, which Get help opens. It covers the documents to add, how to save a LinkedIn profile as a PDF, the question sheet, quitting and resuming, Continue without PDFs, and Add a job site you use…. Today's sentence on the welcome with three steps is replaced.
- `docs/architecture/trust.md`: the release zip checked against the feed's pinned SHA-256; each helper checked against a pinned hash; unpacking that stays inside the chosen folder; a non-empty folder refused; the setup messages, proposals and file names as untrusted text; the progress record and its draft answers in the data folder.
- `docs/architecture/runs.md`: the onboarding run: the fixed first reply that picks Path A, the fixed progress phrases, the joined numbered reply, the fixed confirm replies (Apply these changes, Looks good), resuming from the progress record, and answers sent on the same session.
- `AGENTS.md`, Logs: the progress record in the data folder holds the user's draft answers, read only when asked.
- `docs/architecture/ui-kit.md`: the new widgets above.

## Open

Picked so far: option B, and the lead's recommendations on the download check, home-folder installs, quitting midway, a non-empty folder, Continue without PDFs, Get help and Jobs opening only while the run is open. Still drawn: B1 and B2 for the question sheet; B2 is recommended. To settle:

1. **B1 or B2** for the question sheet.
2. **Finding the numbered items.** B2 needs Claude's questions as one ordered list numbered across the message. Check this against a few real `/setup` transcripts' shapes, and whether sub-items (a, b) under a question should share its one field.
3. **Telling a proposal from a question batch** for the review card. A check on Claude's closing words is fragile. Proposed: a message whose numbered items hold no question mark and that ends by asking to confirm opens the review card; any other message opens the sheet.
4. **Sizes and times.** The 40 MB, 30 MB and 250 MB and the LaTeX time are placeholders, to be measured with the chosen standalone Python, Bun and TinyTeX builds and the LaTeX packages the CV templates need.
5. **Spend at most for setup.** The run used far more than the old $2 guess. Setup's cap must sit well above a real setup, or setup runs without one; a cap that stops it partway lands on `stopped`.
6. **Word files.** Whether `/setup` reads a `.docx` as is, or Jobdesk turns it into PDF or text first.
7. **The first search's figures.** `FirstSearch.html` still says about $0.40 for the search and $0.60 for scoring. Under this round's rule they become "uses your Claude plan" until measured.
8. **The launcher's button** stays Run, its built name.
9. **The Your profile slip on Today**, with Open Profile, is new: keep it until the profile is full, or drop it.
10. **No git in the folder.** A zip download has no `.git`. `/setup` skips its public-fork check without a remote, and updating ai-job-search later (out of scope here) can no longer use git.
