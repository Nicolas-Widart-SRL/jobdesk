# Onboarding

First run for a non-technical job seeker who has Claude Code installed and signed in, no API key, and never opens a terminal. Jobdesk makes the folder, downloads ai-job-search, installs its helpers, builds the profile from the user's documents with the unchanged `/setup`, then hands over to Today for the first search. The design is a compact card wizard in a small window that grows once, at the Profile chapter. Claude's questions open in a sheet with one field per question, answered on the user's own time, and proposed changes open in a review card with a checkbox each.

## Contents

- [Canvases](#canvases)
- [Rules and why](#rules-and-why): window, words, spend, foot, folder, download, helpers, quitting midway, documents, the profile run, question sheet, review card, CV check, choice card, failure, recap, first search
- [New widgets](#new-widgets)
- [Doc changes](#doc-changes)
- [Build questions](#build-questions)

## Canvases

`Onboarding.html?state=…&theme=light|dark`. One centred card per step under `step_dots`, the setup stamp in the card's head, one primary per screen. Folder and Computer render at 720x560, the Profile chapter and the recap at 1280x820.

| State | What it shows | PNG |
| --- | --- | --- |
| `welcome` | First open: Set up from scratch (Recommended, about 40 minutes) or I already have an ai-job-search folder, then Start setup | `Onboarding-welcome.png`, `-dark` (720x560) |
| `resume` | Next open after quitting midway: Welcome back, the four steps with Folder and Computer done, Finish my profile, the day the user stopped | `Onboarding-resume.png` (720x560) |
| `place` | Save location: `Documents/Job search` with Change…, Save here | `Onboarding-place.png` (720x560) |
| `notempty` | The chosen folder has files: refused, with a new subfolder inside it suggested, Use this new folder | `Onboarding-notempty.png` (720x560) |
| `download` | Folder made, downloading ai-job-search with a bar, then checking, then unpacking | `Onboarding-download.png` (720x560) |
| `offline` | Download failed, no internet: folder still empty, Try again, Get help | `Onboarding-offline.png` (720x560) |
| `mismatch` | "This download doesn't match. Nothing was saved." with Try again | `Onboarding-mismatch.png` (720x560) |
| `checking` | Helpers by purpose: Python already here, Bun checking, LaTeX next; they install for this user only | `Onboarding-checking.png` (720x560) |
| `installing` | LaTeX installing with its size, time and a bar; each file checked | `Onboarding-installing.png` (720x560) |
| `toolfail` | LaTeX could not install: Try again, and Continue without PDFs as a quiet link | `Onboarding-toolfail.png` (720x560) |
| `docs` | Profile chapter, window grown: the drop zone for several documents, What comes next | `Onboarding-docs.png` |
| `dropped` | CV and LinkedIn export listed with Remove, a slim drop zone for more, Read my documents | `Onboarding-dropped.png` |
| `reading` | Claude reading: plain progress lines, the live sweep, Elapsed, Stop setup | `Onboarding-reading.png` |
| `sheet` | Claude's 13 questions, each with its own field; question 3 has sub-items a and b over one field; Your answers, Draft saved | `Onboarding-sheet.png`, `-dark` |
| `sheet-fallback` | No numbered questions found: Claude's message at the left, one large unnumbered field at the right | `Onboarding-sheet-fallback.png` |
| `review` | Claude's 16 proposed changes, each with a checkbox, all ticked; the slip shows 16 of 16 changes and the reply "Yes, apply these changes." | `Onboarding-review.png` |
| `review-skip` | The same with 3 and 7 unticked; the slip shows 14 of 16 changes and the reply "Apply all except 3 and 7." | `Onboarding-review-skip.png` |
| `working` | Claude working between batches: progress lines, "this can take a few minutes", So far | `Onboarding-working.png` |
| `followup` | Two short follow-up questions, in the same sheet | `Onboarding-followup.png` |
| `cvcheck` | The generated CV as pages, Looks good and Ask for a change | `Onboarding-cvcheck.png` |
| `choice` | The `AskUserQuestion` card, kept as a secondary state | `Onboarding-choice.png` |
| `stopped` | Setup stopped partway: the reason, what is kept, Pick up where it stopped, Finish later | `Onboarding-stopped.png` |
| `recap` | In the app: folder, computer, profile 9 of 11, CV made, job sites, Add a job site you use…, the time setup took | `Onboarding-recap.png`, `-dark` |
| `recap-nopdf` | The recap after Continue without PDFs: LaTeX not yet, CV written without its PDF, Setup stays in the menu | `Onboarding-recap-nopdf.png` |

`FirstSearch.html?state=today|launcher|jobs&theme=light|dark`, at 1280x820:

| State | PNG | What it shows |
| --- | --- | --- |
| `today` | `FirstSearch-today.png` | Today with nothing to decide: the clear-state offer panel in first-search words, Find new jobs… with Y, and a Your profile slip |
| `launcher` | `FirstSearch-launcher.png` | New task on `/scrape` as built, with Model and Spend at most side by side, and a note naming the first search. The user presses Run |
| `jobs` | `FirstSearch-jobs.png` | Jobs after the search: 14 new jobs, not scored, and a note with Score 14 new jobs… |

## Rules and why

### Window

- The first window is small, 720x560: welcome, resume, Folder and Computer. These steps hold a few lines each, and a small window reads as an installer, not an app to learn.
- The window grows once, when the Profile chapter opens, to 1280x820, the app's own default size, and keeps that size through the recap and into the app. Why: a 13-question batch at the reading measure (`DOC_MEASURE`) plus an answer field per question needs about 1100 by 700 points. The app's minimum, 1040x640, would fit but grows a second time at the recap; one grow is calmer than two.
- On a screen smaller than 1280x820, the window takes the screen's work area, never below the app's minimum.
- The Profile card keeps the wizard's look: centred, `step_dots` above it, the setup stamp in its head, one primary in its foot. It widens to 1120 and splits into a main column and a 340 side column (`SPLIT_SIDE_BASIS`, `SPLIT_GAP`).
- No menu and no tabs until the recap: nothing else is usable yet. The status strip on the welcome keeps Settings, so the theme can change.
- Choosing I already have an ai-job-search folder opens the folder picker, then the app as today, at its usual size.

### Words

The onboarding copy never says repo, clone, CLI, scrape, workspace, slash command, terminal, install path, hash, checksum, byte counts, tokens or cost.

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

ai-job-search keeps its name, as the user may already know it.

### Spend

- No spend figure or estimate shows on an onboarding screen, or in the first-search suggestions on Today and Jobs: no "About $N so far", no price on the welcome or the recap, no estimate in Today's offer or the Jobs note.
- Why: tasks use the user's Claude subscription by default, so a dollar figure there means nothing to this user and reads as a bill. Usage stays in Settings, and still counts every onboarding run and the first search.
- The launcher is unchanged. Opened from the first-search note, it keeps its Model and Spend at most fields, as it shows them for any task.
- The foot's muted line keeps the time, Elapsed, or says nothing. No plan note replaces the spend.

### The foot

- One primary per screen, first in the foot, then one plain button, then one muted line at the right, as the deal footer.
- While a step works, the primary stays in place, disabled, and the line says when it opens. While Claude works there is no primary, only Stop setup.
- Back shows only where going back undoes nothing: Save location, the helper check and the drop zone. A download or install shows Cancel, the profile run shows Stop setup.
- A quiet action is a `link_button` after the primary, in place of the plain button. Only Continue without PDFs uses it.
- In a Profile state where Claude's run is open, the muted line holds Elapsed, as Tasks counts it: "Elapsed 18:40".

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
- A missing helper's line names its size and time in plain words: Python "about 40 MB" and Bun "about 30 MB", each under a minute, and LaTeX "about 250 MB, the slow one: 5 to 15 minutes". The lead says LaTeX is the slow one and the user can step away. The figures are placeholders until measured.
- Each line ends in one tag: Checking… (`Plain`), Already here or Installed (`Hired`), Installing… (`Active`) with a bar and the time left, or Could not install (`Closed`) with the reason under the name. A helper not reached yet says Next in `muted`.
- Jobdesk checks first, and installs only after Install what is missing.
- When LaTeX fails: Try again is the primary, Continue without PDFs the quiet link. The paragraph says what still works and that Setup can install it later. Setup goes on to the Profile chapter; the CV check is skipped, since no PDF can be made.

### Quitting midway

- Quitting at any step is safe. Jobdesk keeps a small progress record in its data folder: the folder's path, which steps are done, the setup run's id, and the draft answers of an open question sheet. The welcome says "Quit any time: setup picks up where you stopped."
- The next open shows `resume` in place of the welcome: Welcome back, "Your folder and tools are ready. Let's finish your profile.", the four steps with the done ones ticked, and Finish my profile. The muted line names the day the user stopped.
- A half download or a half install is removed on the next open and that step starts again. A finished helper stays. The profile run resumes on its own session, as Pick up where it stopped does.
- The record is cleared when the recap opens.

### Documents

- The drop zone takes several files at once, by drop or Choose files…: the CV, a LinkedIn profile saved as a PDF, diplomas, reference letters, as PDF files. A Word file is refused with a line that says to save it as a PDF. A link opens the user guide's part on saving a LinkedIn profile as a PDF.
- Added files show in a `file_list`: a mini page, the file name as plain text with hidden characters removed, a kind menu, its page count, and Remove. A slim drop zone under it takes more.
- The kind menu says what each file is: CV, LinkedIn profile, Diploma, Reference letter or Project. Jobdesk guesses it from the file's name, and the user can change it until the file is copied. Once copied, the menu and Remove are disabled.
- Read my documents stays disabled until one file is added. The muted line counts them.
- Jobdesk copies each file into the matching subfolder of `documents/` (`cv/`, `linkedin/`, `diplomas/`, `references/` or `projects/`), never moves them. A name that already exists there gets a number added.
- The side column says what comes next in plain words: rounds of questions, answered on the user's own time and in any order, and that setup took one tester about 40 minutes, most of it writing answers.

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
| Applying the changes you approved | writes after the review card's reply |
| Making your CV | a write under `cv/` or a LaTeX build |
| Choosing job sites and search terms | writes under `.agents/skills/` or to `.claude/skills/job-scraper/search-queries.md` |

- The working line carries `live_sweep` and "Last update 4s ago", with the quiet rules from Tasks ("No update for 48s. Long writes can be quiet."). The lead says "This can take a few minutes", because Claude's turns took 1 to 3 minutes each.
- The side column, while Claude works, says the user can step away and keeps a So far slip: rounds answered and changes applied.

### Question sheet

`/setup` asks in plain text, in numbered batches, and never through `AskUserQuestion`. Each of Claude's messages that ends with numbered questions opens a question sheet. The sheet's title counts the questions ("Claude has 13 questions"), and the lead names the step and the round.

Claude's message renders as Markdown with no image loading and no links that open, headings capped at the section size, bold lines and numbered items kept, as Claude's replies render in Tasks ([Trust](../../docs/architecture/trust.md#rendering-untrusted-text)). Hidden characters are removed before parsing.

- **A field under each question.** Each numbered item of the message is followed by its own answer field, which starts one line high and grows with its text. A question whose text ends in "(optional)" shows "Optional. Leave it empty to skip." in its empty field; any other empty field shows "Your answer, or leave it empty to skip".
- **On the user's own time.** Claude's batch says to answer in any order and skip any. The user fills the fields in any order, over as long as they like. The draft saves as they type and survives closing Jobdesk.
- **One reply.** Send joins the fields into one numbered reply in Claude's order. An empty field is sent as "Skipped", so the numbers always match. Why one reply and not one message per question: each message is a full Claude turn of 1 to 3 minutes, and Claude may move on after the first one.
- **Sub-items.** Lettered sub-items (a, b) under a numbered question render inside that question, above its one field. The user answers them in that one field.
- **Finding the questions.** Jobdesk reads only ordered list items, numbered across the whole message.

Why a field per question: each answer sits under its own question, so the user never loses count across 13 items and two scrolls, an optional question is marked where it is asked, and the numbered reply is built for them. A real setup session's answer was one numbered reply of about 900 characters; the sheet builds exactly that shape of reply.

**Fallback (`sheet-fallback`).** When the message has no numbered items, or the numbers do not run in order, the sheet shows the message at the left with its own scroll and one large field at the right, unnumbered. The title says "Claude has a few questions", since there is nothing to count. The help under the field says the answer is one reply this time. Send answer sends the field as it is.

All sheets share:

- **Short answers.** The side slip says "Short answers are fine." and "An empty one is sent as Skipped." A `tally` counts the answered fields.
- **Save draft.** Answers save as the user types, to the progress record, and the slip says when ("Saved as you type, last at 14:32. Take your time: close Jobdesk and come back, and your answers wait here."). No Save button. The draft is cleared once Send succeeds.
- **Show all Claude wrote** opens the whole message as a `document` in a popover, so nothing Claude said is hidden by the sheet.
- **Send answers** with Cmd Enter is the primary; Stop setup is the plain button.
- The same sheet takes the short follow-ups ("Two quick checks"): two questions, two fields, the same Send.

### Review card

- When Claude's message proposes numbered changes and ends by asking to confirm, the card shows the proposal as a `document` in the main column, grouped by Claude's own section lines. The title says "Claude proposes changes to your profile" and the lead says nothing changes until the user says so.
- Each numbered change is a `change_check` row: a checkbox, Claude's number, the change. All start ticked. A click on the box or the words flips it. An unticked change's words go `muted`.
- The side slip counts the ticked ones ("16 of 16 changes") with a `tally`, and shows the reply Apply these changes sends, in `mono`, as it will go:
  - all ticked: "Yes, apply these changes."
  - some unticked: "Apply all except 3 and 7.", with the numbers in Claude's order. The slip says those stay out of the profile.
- Why checkboxes: Claude's proposal ends "Reply yes to apply all, or list the numbers to skip." The checkboxes build exactly that reply, so the user never types numbers.
- With every change unticked, Apply these changes is disabled, and the muted line says "Tick a change, or use Change something."
- Change something opens a field under the document with Send and Cancel, in place of the two buttons, and sends the user's words as the reply. The ticks are not sent with it.
- When Jobdesk cannot tell a proposal from a question batch, the message opens in the question sheet, which still reaches the user.

### CV check

- When `/setup` has built the CV PDF and asks the user to check it, the card shows its pages as page images, as Profile › CV documents draws them, with Open PDF in the side slip. Looks good sends the fixed reply "The CV looks good." Ask for a change opens a field, as Change something does.
- The PDF renders as page images only, as [Trust](../../docs/architecture/trust.md#cv-files) says.

### Choice card

- When Claude asks with `AskUserQuestion`, the card in stamp ink shows instead of the sheet: the question, each choice as a radio row with its label and description, Or write your own answer, Send answer and Decline, as in Tasks. The question always reaches the user.
- It is a secondary state: the real `/setup` never used it, but another run might.
- A permission prompt, with Settings › Permissions on, shows in the same place as in Tasks.

### Failure

- Every failure names what happened in plain words, what is kept, one way forward and Get help.
- Get help opens the user guide's First run section, on the product site.
- Setup stopped partway shows the reason as plain text with hidden characters removed. Pick up where it stopped replies on the same session, which resumes it as any run does. Finish later opens the app with what is done; the next open offers `resume`.

### Recap

- Five done lines: Folder saved (with Show in Finder), This computer is ready, Profile written N of 11 topics (with the `fat_tally` of filled topics), Your CV is made (with Open PDF), Job sites chosen.
- Then one optional line with a hairline `add` knot: "Use a job site that is not listed?" with Add a job site you use…. It opens the launcher on `/add-portal`, which asks for the site's address and writes a new search tool. It never starts a run. It is for users whose country's boards are not built in.
- Under the lines: "Claude worked on your profile for 41 minutes.", "for 1 minute." or "for under a minute.". The time is the run's Elapsed, as Tasks counts it, so the time spent writing answers is not in it. It comes from the run, never from Claude's closing message.
- **Without PDFs.** The computer line turns `fail` with a `Plain` Not yet tag and says LaTeX is not installed, so PDFs are not ready yet. The CV line says the CV is written and its PDF comes once LaTeX is installed. The Setup tab stays in the menu, and the muted line says so.
- Open Jobdesk opens Today, which suggests the first search.

### First search

- Today with nothing dealt shows the clear-state offer panel in its first-search words, with Find new jobs… as the primary with Y. Its steps give the time only: "It takes about 5 minutes." Y only opens the launcher on `/scrape`, with a `from_note`: "Your first search, from Today." The user presses Run.
- The launcher opened from that note is the launcher as built: Model and Spend at most side by side, the command, and Run. Only the `from_note` is new.
- When the search ends, Jobs opens by itself only while that run is still open in Tasks. Otherwise the Today offer turns into Score N new jobs….
- Jobs shows the New view with each job Not scored, and a note with Score N new jobs… as the primary, which only opens the launcher on `/rank`. The note gives the time only.
- Card actions never start a run, in every state drawn here.

## New widgets

| Widget | What it is | Tokens and widgets it uses |
| --- | --- | --- |
| `choice_card` | A pickable card: a radio, a title with an optional tag, a sentence. Picked takes the pick look. | `card`, `raised`, `line`, `text`; `RADIO`, `RADIO_DOT`; `RADIUS_PANEL`; `tag` |
| `step_dots` | One square per step with its name, joined by a hairline | `STATUS_DOT`; `text`, `off`, `line`, `muted`; `Type::SMALL` |
| setup stamp | `day_stamp` drawn with four ticks and the step inside, or the date on the welcome and the recap | `DAY_STAMP_RINGS`, `DAY_STAMP_TICK`; `text`, `off`, `line`, `sheet` |
| `drop_zone` | Paper with a dashed `line` edge, stacked mini pages, the words, Choose files…; a slim form under a file list | `card`, `line`; `SWEEP_DASH`; `MINI_PAGE_SIZE`, `page`, `page_line`, `page_head`; `button` |
| `file_list` | One row per added file: a mini page, the name as plain text, a kind menu, the page count, Remove. A copied row's menu and Remove are disabled | `paper`, `line`; `ROW_PADDING`, `INSET`; `Type::MONO`, `Type::NOTE`; egui `ComboBox` at `KIND_MENU_WIDTH`; small `button` |
| `setup_line` | A line on paper: a knot, a title, a line under it, and a tag or bar at the right, with an optional bar row | `knot`, `tag`, `bar`; `ROW_PADDING`, `INSET`; `live_sweep` on the working line |
| `question_sheet` | Claude's message as a `document`, each numbered item followed by a growing field, sub-items kept inside their item; falls back to the message beside one large field | `markdown`, `DOC_MEASURE`; `field`, `FIELD_HEIGHT`, `FIELD_GAP`; `Type::TAKE`, `muted` for the numbers |
| answers slip | The side slip of the sheet: count and `tally`, Short answers, Draft saved, Show all Claude wrote | `panel`; `Type::SECTION`; `TALLY_FAT`; `link_button` |
| `change_check` | A proposed change in the review card: a checkbox, Claude's number, the change's words. One control: a click on the box or the words flips it, Tab stops once, a screen reader hears one checkbox named by the words. Unticked, the words go `muted` | the checkbox `setting_check` paints (`CHECK_BOX`, its egui visuals); `markdown` at `DOC_MEASURE`; `muted`; `SETTING_CHECK_GAP` |
| review slip | The review card's side slip: "N of M changes" with a `tally`, and the reply Apply sends in `mono` | `panel`; `Type::SECTION`, `Type::MONO`; `TALLY_FAT`; `field` behind the reply |
| `cv_check` | The CV's pages as page images in the card, with the side slip's Open PDF | `PAGE_SIZE`, `PAGE_GAP`, `page`; the CV documents page renderer |
| `from_note` | A note at the top of the launcher's slip naming what opened it | `card`; `Type::NOTE`; `muted`, `text` |

`change_check` reuses the kit's checkbox paint (`paint_choice` with a check), not `setting_check` itself: a setting row carries a hairline above and the settings row padding, which a list of 16 changes does not want.

The choice card is `prompt_box_tone(Tone::Turn)` with the choices, field and buttons Tasks already uses. The alert is `alert` on a `panel` with a `closed` edge. The quiet Continue without PDFs is `link_button`.

In dark, `panel` and `card` are the same color, so a slip, alert or `ahead` knot on the Profile card takes `sheet` there.

## Doc changes

- `ARCHITECTURE.md`: a feature for first run (or a part of `workspace`): the progress record, download and check the ai-job-search release against the feed's pinned SHA-256, install the helpers into the home folder against pinned hashes, copy the documents, and start `/setup` through runs' application layer. Add it to the Features table and to `FEATURES` in `tests/architecture.rs`. The runtime note for the small first window, the one grow at the Profile chapter, and its sizes.
- `docs/guide.md`: a First run section before Today, which Get help opens. It covers the documents to add, how to save a LinkedIn profile as a PDF, the question sheet answered on your own time, ticking proposed changes, quitting and resuming, Continue without PDFs, and Add a job site you use…. It says the onboarding screens and Today's and Jobs' first-search suggestions show no spend, the launcher keeps Spend at most, and Settings › Usage holds the spend. Today's sentence on the welcome with three steps is replaced.
- `docs/architecture/trust.md`: the release zip checked against the feed's pinned SHA-256; each helper checked against a pinned hash; unpacking that stays inside the chosen folder; a non-empty folder refused; the setup messages, proposals and file names as untrusted text; the progress record and its draft answers in the data folder.
- `docs/architecture/runs.md`: the onboarding run: the fixed first reply that picks Path A, the fixed progress phrases, the joined numbered reply with "Skipped" for an empty field, the review card's built reply ("Yes, apply these changes." or "Apply all except N and M."), the fixed "The CV looks good.", resuming from the progress record, and answers sent on the same session.
- `AGENTS.md`, Logs: the progress record in the data folder holds the user's draft answers, read only when asked.
- `docs/architecture/ui-kit.md`: the new widgets above.

## Build questions

The build's planner settles each of these before the step that needs it.

1. **Numbered items and sub-items**: how Jobdesk finds Claude's questions as one ordered list numbered across the message, with lettered sub-items under them; check against the shapes of a few `/setup` transcripts.
2. **Proposal or question batch**: how Jobdesk tells a proposal from a question batch; a check on Claude's closing words is fragile, and one proposed test is numbered items with no question mark plus a closing request to confirm.
3. **Helper sizes and times**: measure the 40 MB, 30 MB and 250 MB and the LaTeX time with the chosen standalone Python, Bun and TinyTeX builds and the LaTeX packages the CV templates need.
4. **Word files**: whether `/setup` reads a `.docx` as is, or Jobdesk turns it into PDF or text first.
5. **No git in the folder**: a zip download has no `.git`, so `/setup` skips its public-fork check without a remote, and a later ai-job-search update cannot use git.
6. **Your profile slip on Today**: whether the new slip, with Open Profile, stays until the profile is full, or is dropped.
