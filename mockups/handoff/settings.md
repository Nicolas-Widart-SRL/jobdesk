# Settings

Brief: a Settings screen with grouped sections, including signing tasks in with an Anthropic Console API key kept in the system keychain. DeepTutor's settings were the starting point, redrawn in Jobdesk's own look. Approved.

- [Canvas](#canvas)
- [Layout](#layout)
- [Opening and closing](#opening-and-closing)
- [Pages](#pages)
- [The Setup tab](#the-setup-tab)
- [Code home](#code-home)
- [New widgets](#new-widgets)
- [Doc changes](#doc-changes)

## Canvas

`Settings.html?state=…&theme=light|dark`, scope `.st`, built on `Grid.html`'s shell. Each PNG is 1280x820.

| PNG | State |
| --- | --- |
| [Settings-usage.png](../Settings-usage.png) | Usage statistics in full: the period toggles, four figures, cost per application sent, the year of days with one day's tooltip, By command and By model. |
| [Settings-usage-dark.png](../Settings-usage-dark.png) | The same in dark, where the day squares keep four steps. |
| [Settings-usage-empty.png](../Settings-usage-empty.png) | No tasks yet: the empty state, with a faded blank year under it. |
| [Settings-loading.png](../Settings-loading.png) | Usage before the logs are read: "Reading your run logs…" and skeleton blocks shaped like the page. |
| [Settings-providers.png](../Settings-providers.png) | Claude sign-in in use: the last task's sign-in, the three ways in, and the sign-in detail with Test connection. |
| [Settings-key-entry.png](../Settings-key-entry.png) | Typing a key: the masked field, Show, Save key, and the keychain note. |
| [Settings-key-saved.png](../Settings-key-saved.png) | The key saved and in use: its last four characters, Replace, Remove, and the passed test. |
| [Settings-key-saved-dark.png](../Settings-key-saved-dark.png) | The same in dark. |
| [Settings-key-failed.png](../Settings-key-failed.png) | The test failed: what Claude Code said, as plain text, and how to fix it. Tasks stay on the Claude sign-in. |
| [Settings-terms.png](../Settings-terms.png) | Your terms, read only: contract, day-rate floor and four commute tiers, with their source files. |
| [Settings-terms-missing.png](../Settings-terms-missing.png) | The profile states no day-rate floor: Not stated, and what the check does instead. |
| [Settings-terms-failed.png](../Settings-terms-failed.png) | The profile file cannot be read: the alert, "hidden, not gone", and Update with Claude… disabled with its reason. |
| [Settings-general.png](../Settings-general.png) | Folder, start screen, notifications. |
| [Settings-appearance.png](../Settings-appearance.png) | Theme, text size, reduce motion. |
| [Settings-queue.png](../Settings-queue.png) | Today queue: the Coming up range, how long Later lasts, the skip reasons list. |
| [Settings-jobs.png](../Settings-jobs.png) | Jobs view filters, with the count they hide. |
| [Settings-defaults.png](../Settings-defaults.png) | Run defaults: the model, the budget, the per-command table and the monthly cap. |
| [Settings-cap.png](../Settings-cap.png) | The launcher once the cap is reached: Run disabled, its reason on hover, a link to the cap. |
| [Settings-perms.png](../Settings-perms.png) | Permissions: the two choices, and the two rules that hold either way. |
| [Settings-runtime.png](../Settings-runtime.png) | Runtime status: Setup's content, with Claude Code's version and sign-in on top. |
| [Settings-data.png](../Settings-data.png) | Data and privacy at the default Forever, with the Clear tray decisions popover open. |
| [Settings-about.png](../Settings-about.png) | Versions and links. |
| [Settings-search.png](../Settings-search.png) | A search for "spend": the rail keeps only the matching pages, and the matched words carry the highlighter. |
| [Settings-nav.png](../Settings-nav.png) | The menu with the Setup tab while a required item is not ready, and without it once all are. |

## Layout

- **A full screen.** Settings replaces the menu and the page. Back to app (Esc) stands on the desk above the rail, the rail is a `slip`, and the page sits on paper beside it. The status bar stays, with its Settings entry shown as on. This keeps the most height for the page, and nothing behind it competes.
- **The rail**: `RUN_SIDE` wide. A slip with the title "Settings", the search field, then four groups in `group_title`: Personal, Job search, Models and providers, System. Each row is a `pick_row` with the page name and, at its right, one state word in `Type::NOTE`. Words, not icons, because the kit has no icon set and a word says more: "$41.20 in Oct", "1 not stated" in `interview`, "Key refused" and "1 missing" in `closed`, "Hidden" in `closed` when a source failed. The foot says changes apply at once and stay on this computer.
- **The page**: `paper` with the head (`Type::H1` title, one lead line in `Type::TAKE` `muted`, actions at its right), a hairline, then the body, which scrolls with its bar at rest. A page whose values come from the checkout keeps a footnote strip naming the files, as Profile does.
- **Sub-sections**: a `Type::SECTION` heading, an optional note in `Type::NOTE`, then `setting_row`s with a `line` hairline above each. Inside paper there is no 2px section rule.
- **Panels inside paper** (the figure band, notes, the provider detail) keep a `line` hairline, because `card` and `panel` are one colour in dark.
- **Choices** of two to five values are a `toggle_group`, one on. Lists of more values use egui's combo box on `field`, as the launcher does. On and off is egui's checkbox at `CHECK_BOX`, with its label and help to its right.
- **Words**: only an action that opens the launcher ends in "…", so it is "Change folder", not "Change…", and the launcher's button stays "Run". Mac words switch on Windows: Cmd to Ctrl, Show in Finder to Show in Explorer, Keychain to Credential Manager. Commands keep their real names, so the scraping command is `/scrape`.

## Opening and closing

- Cmd , (Ctrl , on Windows) opens Settings from anywhere, even while a field has focus, as Cmd K does. The visible control is `status_button` "Settings Cmd ," at the right of the status bar, in the slot the theme switch leaves.
- Esc closes it, after `back_pressed`'s rule that the first Esc leaves a focused field. So does Back to app. Settings reopens on the page last open.
- A running task keeps running while Settings is open. The tabs are hidden, so when a task waits on the user the status bar's line says "A task waits for you" after a `STATUS_DOT` square in `turn` (not drawn).

## Pages

**General**: the folder as a path in `Type::MONO`, with Show in Finder and Change folder (the folder picker that Setup uses). Open on is a combo of the main tabs, Today by default. Notifications: "When Claude waits on you", on by default, and "When a task finishes", off. The OS asks for permission the first time one is sent.

**Appearance**: Theme (System, Light, Dark) moves here from the status bar. Text size steps 90, 100, 110, 125 and 150%, through egui's zoom, and Cmd + and Cmd − still work. Reduce motion makes `MOTION_TIME` zero at runtime, and it is on by itself when the OS asks for less motion.

**Usage statistics**
- Counts tasks only, built from Jobdesk's run logs: each run's result line gives cost, tokens in and out, cache reads and the models used. Nothing is fetched. Test connection is not a task and writes no run log, so its cost never shows here.
- Period toggles in the head: the current month (named, "October"), 90 days, This year, All time. They drive the figures and both tables.
- `figure_band`: Spent, with a `slip_bar` against the monthly cap when one is set; Tokens in, with out in the note; Cache hits, as cache reads over all input tokens; Tasks, with how many finished, stopped and failed.
- **Per application sent**: task spend in the period divided by the applications marked sent in the period, which come from tracking's application layer. Every task counts, not only drafting. One line under the band, with Open Applications. It is hidden when none was sent.
- **Every day**: `day_heatmap`, always the last 12 months whatever the period, one square per day, counting tasks per day, not spend, in four steps. Today has a `text` ring. Hover gives the day, the tasks and the spend. It shows activity, not a status, so it uses neutral inks, never stamp ink.
- **By command** (command in mono, tasks, spent, share as a `bar`) and **By model** (model, tasks, tokens in and out, spent), side by side, and one under the other when the page is narrower than both.
- Empty: `empty_state` "No tasks yet" with Find new jobs… and New task…, then the blank year at `FADED_OPACITY`. Loading: "Reading your run logs…" with `skeleton` blocks in the band, the year and the tables.

**Your terms**: read only
- Contract, day-rate floor and the commute tiers, read from the profile that discovery's freelance gate already reads, through discovery's application layer. Values sit at the row's right in `Type::STRONG`, with no field around them.
- A note says that Jobdesk never changes them. Head actions: Edit in Profile (switches to the Profile tab) and Update with Claude… (`small_command_action`, opens the launcher on `/setup --section goals`, never starts a run). Show the command reveals the line.
- Not stated: an outlined `tag` in `interview` and what the check does instead ("reads Unclear instead of guessing"). The rail says "1 not stated".
- Failed: `alert` under the head and "Your terms are hidden, not gone". Parts read from a file that still loads stay, faded. Update with Claude… is disabled with Profile's reason.

**Today queue**: the Coming up range (7, 14 or 30 days, 14 by default). The range also sets which dated cards rank before the rest. How long Later lasts (until tomorrow by default, 3 days, until Monday). The skip reasons are an editable list: each reason with Remove, then a field and Add reason, disabled while the field is empty. Past skips keep their reasons, and a reason stays optional.

**Jobs view**: hide jobs that fail the day-rate, commute or language check, hide jobs scored under a number (every list on Jobs), hide jobs the safety check flagged. Filters hide rows only and never change a job's lane, because `/rank` owns the lists. Jobs counts what it hides and offers Show them. A flagged ad still deals on Today as Check this ad.

**Run defaults**
- Model: a combo of the launcher's list. Its first entry keeps the launcher's name, "Claude default", so one choice has one name.
- Spend at most, USD: empty by default, so each command starts at its suggested budget from the command catalog. An amount starts every task at that amount. The monthly cap is a separate setting.
- Per command: a small table of rows (command in mono, model combo, budget field, Remove), then Add a command, which picks from the catalog. A row beats both defaults.
- Monthly cap: a checkbox with an amount. Spend this month comes from the same totals as Usage. At or over the cap, the launcher's Run is disabled through `enabled_when` with the reason ("This month's tasks cost $X, over your $Y cap. Run opens again on 1 November."). A strong line above Run says the same, as the busy line does, and a link opens Run defaults. A running task is not stopped. Quick replies on a live session still send, because they continue a task already paid for.

**Permissions**: two radio rows. "Allow and note in the activity" (default, today's behaviour) or "Ask me before each tool", which turns on `ask_permission`. "Either way" holds two fixed rows with `Plain` tags: questions from `AskUserQuestion` always reach the user, and a remembered rule lasts only until the task ends and is never saved to the folder. A line under them says what allowing costs, as the guide does.

**Runtime status**: Setup's content, with its promise line. Claude Code on top (the version and the sign-in from the last task, with a link to Providers), then Profile's one-line summary, Environment ("4 of 5 ready", optional tools never counted), Portals, Templates, Sync and Reset, as built. Head actions: Check again and Look for more in your documents….

**Data and privacy**: the data folder path in mono with Show in Finder, and its size on disk split by run logs and tray decisions. Keep run logs for 30 days, 90 days or Forever, Forever by default, so no history is lost on update. A shorter time deletes older logs at startup, and Usage counts only the logs kept. Clear tray decisions is a small danger button that opens `confirm_popover` with the count: Keep them is the default and takes focus, and Esc keeps them.

**About**: the Jobdesk version with Copy details (version, OS, Claude Code version, for a bug report), the Claude Code version, and links to the auth review, the changelog and the licences. One line says Jobdesk runs Claude Code and is not made by Anthropic. No Anthropic or Claude logos anywhere.

**Search**: `search_field` in the rail filters the rows to pages with a match, each with "N matches". It searches labels and help lines. The open page marks matched words with `hit_mark`. Esc clears the search first, then closes Settings. With no match, the rail shows "No setting matches" and the page stays as it was (not drawn).

## The Setup tab

- The Setup tab shows in the main menu whenever a required item is not ready, so through first onboarding too, and the first-run welcome's step 2 points to it.
- Once every required item is ready, the tab leaves the menu, and Settings › System › Runtime status shows the same content.
- The rule is derived, never stored: a required item that goes missing later brings the tab back. Optional tools never keep it.

## Code home

- Usage is a read model in `runs`' application layer, because `runs` owns the logs. Settings reads it, tracking's applications sent, discovery's terms and workspace's checks, each through their application layers.
- The planner picks the notifications crate and checks it on macOS and Windows.

## New widgets

- **`setting_row`**: label in `Type::STRONG` and help in `Type::NOTE` `muted` on the left, held to `DOC_MEASURE`, and the control on the right, centred. A `line` hairline sits above. The checkbox form puts the box first at `CHECK_BOX`. A compact form with the label column at `COMMAND_NAME_WIDTH` serves the provider detail.
- **`figure_band`**: `flow_band` without arrows or tallies. Equal cells on `panel` with a `line` hairline around them and between cells, `NODE_PADDING`, label in `Type::TAB` `muted`, figure in `Type::FIGURE`, note in `Type::NOTE` `muted`, and an optional `slip_bar` under the figure.
- **`day_heatmap`**: 53 week columns by 7 days of `STATUS_DOT` squares at no radius, `TALLY_GAP_Y` apart. The steps are `raised` (none), `line` (1), `faint` (2 or 3) and `text` (4 or more). Today gets a `text` ring outside a `card` gap. Month labels and Mon, Wed, Fri sit in `Type::SMALL` `muted`. To its right, in `Type::NOTE` `muted`, are what a square means, the days with a task, and the legend. Hover gives a tooltip. It takes no click.
- **`search_field`**: a `field` with a hint in `faint`, Esc to clear and a Clear `link` while it holds text.
- **`hit_mark`**: a text format, like `blank_mark`. It puts the `mark` swipe without its `turn` rule under matched words. The highlighter means "here", which is what a search hit is.
- **`status_button`**: a button at `STATUS_SEGMENT_HEIGHT` and `STATUS_SEGMENT_PADDING` in the status bar, label and keys in `Type::SMALL`, `raised` with `text` ink when on.
- **`confirm_popover`**: the End session popover moved into the kit. A strong question, one `muted` line, the safe primary focused first, a danger button, and "Esc keeps …" in `faint`.
- **`toggle_group`**: a row of `small_toggle`s with `TOGGLE_GAP` that reports as one radio group to screen readers.

## Doc changes

- **ARCHITECTURE.md Runtime**: Later, the Coming up range and the dealing range become settings, with today's values as defaults.
- **docs/guide.md**: Today says Later and skip reasons are set in Settings.
