# Portals

A new top-level Portals screen: the job boards Claude searches, in plain words for people who are not technical. Approved: the list on a slip beside the picked portal's detail on paper, as Profile's rail sits beside its topic, with Browse community and Add a portal as the other two views. Setup's portal table shrinks to one summary line with Open Portals.

- [Canvas and states](#canvas-and-states)
- [Decisions](#decisions)
- [The screen](#the-screen)
- [Your portals](#your-portals)
- [A portal's detail](#a-portals-detail)
- [Browse community](#browse-community)
- [Add a portal](#add-a-portal)
- [Setup's Portals panel](#setups-portals-panel)
- [Rules and why](#rules-and-why)
- [New widgets](#new-widgets)

## Canvas and states

`Portals.html`, scope `.po`, on Grid.html's shell. Query: `?screen=yours|detail|browse|add&state=...&portal=linkedin|northfield&theme=light|dark`.

Setup is drawn in one place only, `Profile.html?state=setup`, which already draws Setup for its Profile panel. Its `&portals=full|none|unread` picks the Portals line.

| PNG | Query | Shows |
| --- | --- | --- |
| `Portals-yours.png`, `-yours-dark.png` | `screen=yours` | The list beside LinkedIn's detail, light and dark |
| `Portals-northfield.png` | `screen=detail&portal=northfield` | A portal made by you, which ignores quotes and has Remove… |
| `Portals-remove.png` | `screen=detail&portal=northfield&state=remove` | Remove… asks first |
| `Portals-busy.png` | `screen=yours&state=busy` | A task runs, so the switches and the actions wait, with the reason on hover |
| `Portals-detail-busy.png` | `screen=detail&state=busy` | The same, with the reason on the detail's switch |
| `Portals-loading.png` | `screen=yours&state=loading` | Reading the folder |
| `Portals-nofolder.png` | `screen=yours&state=nofolder` | No job search folder chosen |
| `Portals-none.png` | `screen=yours&state=none` | A folder with no portal installed |
| `Portals-failed.png` | `screen=yours&state=failed` | `.agents/skills` could not be read |
| `Portals-browse.png`, `-browse-dark.png` | `screen=browse&state=cached` | The community list, saved 12 minutes ago |
| `Portals-browse-loading.png` | `screen=browse&state=loading` | The first read from GitHub |
| `Portals-browse-offline.png` | `screen=browse&state=offline` | GitHub unreachable, the saved list shown |
| `Portals-browse-limited.png` | `screen=browse&state=limited` | GitHub's rate limit, the saved list shown, Try again waits |
| `Portals-browse-nocache.png` | `screen=browse&state=nocache` | GitHub unreachable and no saved list yet |
| `Portals-browse-tip.png` | `screen=browse&state=tip` | What Install… does, on hover |
| `Portals-add-empty.png` | `screen=add&state=empty` | The form empty, Build it with Claude… waiting for a website |
| `Portals-add-filled.png` | `screen=add&state=filled` | Every field filled, with the command shown |
| `Portals-add-partial.png` | `screen=add&state=partial` | Website and country only: the empty fields stay off the line |
| `Portals-add-invalid.png` | `screen=add&state=invalid` | An address that is not one |
| `Profile-setup.png` | `Profile.html?state=setup` | Setup: "4 of 8 portals on · 538 jobs found" and Open Portals |
| `Profile-setup-noportals.png` | `Profile.html?state=setup&portals=none` | Setup: no portal installed |
| `Profile-setup-unread.png` | `Profile.html?state=setup&portals=unread` | Setup: the saved jobs cannot be read, so jobs found hide |

## Decisions

- **List and detail side by side.** The list stays in sight while the user reads a portal, and the screen matches Profile. Each row takes three lines because the list column is narrow.
- **Setup keeps a summary, not the table.** The Portals screen now owns the portals, so Setup only says whether they are ready, as its Profile panel does.
- **Remove moves the folder to the Trash, after Jobdesk asks.** The user can put it back, and nothing is lost by one click.
- **Built-in portals have no Remove, only Off.** They ship with ai-job-search, and Off already makes Claude skip one.
- **Add a portal puts its answers on the launcher line.** `/add-portal` skips the questions its arguments answer, so the user is not asked twice.
- **A flipped switch keeps its row's place until the screen opens again.** Nothing jumps under the pointer.

## The screen

- The Portals tab sits after Profile and before Setup (`folder_tab`). Its failed mark is the `STATUS_DOT` square in `closed`, as on other tabs.
- The page head: Portals in `Type::H1`, a subtitle that counts the portals on, and Check all portals… (`command_action` on `/scrape health`) as the page action.
- Under the head, three `segmented` segments: Your portals with its count, Browse community, Add a portal.
- Search terms and the window of 14 days and about 20 results come from the folder, not from Jobdesk. The canvas uses the sample folder's fictional terms.

## Your portals

- The list is a `panel` slip of pickable rows (`pick_row`, the `PICK_EDGE` edge on the picked one) beside the picked portal's detail on paper. The first portal is picked when the screen opens.
- One row per portal under `.agents/skills/*-search/`, three lines: the board's plain name (never the folder name) and its country with the switch at the right, one sentence on what it searches, then jobs found, the day of the last new job, the source tag and the badges that apply.
- Two groups, On ("Claude searches these") then Off ("Claude skips these"). A row keeps its place when its switch flips, and moves to its group the next time the screen opens.
- Off rows are quieter: their words at `FADED_OPACITY`, the switch at full ink.
- Source tags, a `row_tag` in `Tone::Plain`: Built-in (shipped with ai-job-search), Community (installed from the community list), Made by you (made with `/add-portal`). Badges in `Tone::Interview`, the Jobs screen's "Worth knowing" ink: Needs API key, Personal use only.
- Jobs found counts the entries in `job_scraper/seen_jobs.json` with that `portal`, and Last new job is the latest `first_seen` among them. With none: "No jobs yet". If the jobs file cannot be read, both hide behind a `hidden_note`, as Setup's does.

## A portal's detail

- One paper. Head: the plain name in `Type::DEAL`, the country and the sentence, the switch at the right, jobs found, the last new job and the tags.
- Actions, each a `small_command_action` that opens the launcher: Check this portal… on `/scrape health <folder name>`, and Change search terms… on `/setup --section search`. Remove… only on Community and Made by you portals. A built-in portal shows instead: "Built-in portals stay installed. Turn one off to skip it."
- How it searches: four rows in plain words, made from the portal's `Query syntax:` line and its search options. Exact phrases, Combine with OR, Location, and what each search finds. Each answer leads with a bold Yes, No or a short phrase, then one sentence.
- Searches it will run: the shared search terms, grouped by priority (a `panel` chip "Priority N", then the category's name), each term as this portal receives it. A portal that honours quotes shows multi-word terms in quotes, and one that ignores them shows them bare. The right column says it in words: These words together, These words in any order, One word. The lead says plainly that the terms are shared by every portal. The count says how many searches that makes.
- If it fails: the `site:` lines from the search file that name this portal's site, in a `code_box` (Courier, as typed syntax), with one line saying where they come from. With none, one sentence says so.
- Footnote: the portal's folder and the search file, as `path_label`.

## Browse community

- A `search_field` that matches name, country and description, then the list's age at the right: "From GitHub, updated 12 min ago" with Refresh.
- A `note` slip with an `interview` square: community portals are code other people wrote, it runs next to your career data, and Install… opens a task in which Claude reads the code and says what it does before anything installs.
- Paper grouped by country, the user's market first and marked "your market", then Worldwide, then the rest by name. The market is the country of the user's Location filter. A market with no portal reads "No community portals for <country> yet", with Add a portal.
- Each entry: name over its description, owners, a tag (Maintained in `Tone::Plain`, "Unmaintained since YYYY-MM" in `Tone::Interview`), View code ↗, and Installed (`Tone::Hired`) or Install…. Installed matches by folder name.
- Install… is a `small_command_action_hinted`. It opens the launcher on a task that copies the portal into `.agents/skills/`, explains its code, then asks before it runs the CLI installer. Its hover says nothing runs until the user starts it.
- States: loading (grey lines), cached, offline with the saved list (an alert box in `interview`), rate-limited with the saved list and Try again disabled until GitHub's reset time, and offline with no saved list (empty state with Try again and a link to the list on GitHub).

## Add a portal

- A short form on paper: the job board's website, then Country and Language of its job ads side by side, then One job title to test with. The last three are marked Optional in `muted` `Type::NOTE`. Hints start with "Such as", so an empty field never reads as filled.
- Build it with Claude… is the primary, a `primary_command_action`. It waits for a website, with the reason beside it, and for one that is a web address. A typed address without a scheme gets `https://`.
- The command is `/add-portal <website>`, then the filled fields after it as `country: …`, `language: …`, `test title: …`, joined by ", " in that order. A field left empty stays off the line. Such as `/add-portal https://jobs.sample-board.example country: Belgium, language: French, test title: Lead Backend Engineer`, or `/add-portal https://jobs.sample-board.example country: Belgium` with only the country filled.
- Each field is trimmed and its line breaks removed, so the command stays one line.
- "Hide the command" shows the line in a code box that wraps, as `reveal_command` does.
- The lead says what is filled in here is not asked again. The test title's help says Claude asks in the task for any field left empty.
- One line: it is a task, so it costs like any other and shows in Usage, and nothing runs until the user starts it.
- A `slip` beside it: What happens next, three numbered steps (`knot`): the task opens with the address and the answers filled in, Claude builds and tests and asks only for what was left empty, and the portal joins Your portals tagged Made by you. Then one line on boards that need a sign-in or a key.

## Setup's Portals panel

- The panel keeps its header and `.agents/skills` path note. The table and its Check portal health and Add portal buttons go: both live only on the Portals screen.
- One line remains, as the Profile panel's: `lead_line` "4 of 8 portals on", then "· 538 jobs found" in `muted`, and `small_button` Open Portals at the right, which switches to the Portals tab. Open Portals opens no launcher, so it has no "…".
- Jobs found is the sum over all installed portals, on or off.
- No portal installed: the line reads "No portals installed." in `muted`, and Open Portals stays, so Browse community and Add a portal are one click away.
- The saved jobs cannot be read: the count of portals on stays, and "· Jobs found are hidden while your saved jobs cannot be read." replaces the jobs, as a `hidden_note`.
- On the canvas the Profile and Portals panels stack in the main column beside the Environment slip, each as tall as its line.
- Settings › Runtime status shows the same line in its Portals section, because it always shows what Setup shows.

## Rules and why

- **Nothing runs by itself.** Every action that reaches Claude ends in "…" and opens the launcher. The switch and Remove… are the only things Jobdesk does itself.
- **The switch edits one line.** Off writes `enabled: false` into the portal's `SKILL.md` frontmatter, and On removes it, so a portal with no line is on. It waits while a task runs, with the reason on hover, because one run works at a time and `/scrape` reads the same files. Check this portal…, Change search terms…, Remove… and Check all portals… wait too.
- **Remove asks first**, in a `confirm_popover`, with Keep it as the default. It moves the folder to the Trash, never deletes it, and the jobs the portal found stay in Jobs.
- **Community text is untrusted.** Names, descriptions and owners from GitHub render as plain text with hidden characters removed, on one line, cut with an ellipsis. Owners are words, never links. View code ↗ opens an address Jobdesk builds from the community repository and the folder name, never one from the fetched text, and only on click. The installed portals' names and sentences, from their own `SKILL.md`, render the same way.
- **Plain words.** No folder names, flags or frontmatter keys on screen, except the paths in the footnote and the Google lines, which are typed syntax.
- **Terms are shared.** The detail never suggests per-portal terms. Change search terms… edits the one shared file.
- **Checking the community list is not a run.** It starts no `claude`, writes no run log and is not counted in Usage, and it caches the last good list in the data folder.

## New widgets

- **`switch`**: on or off in one click, for a setting that acts at once, with its word after it. A track `CHECK_BOX` tall and twice as wide at `RADIUS_CARD`, a square knob at `RADIUS_STAMP` inside a `HAIRLINE` edge. On: track and edge `text`, knob `field`, word in `Type::STRONG`. Off: edge and knob `muted`, track `field`, word `muted` in `Type::NOTE`. Disabled at `DISABLED_OPACITY` with `enabled_when`'s reason. Focus ring as every kit widget. A screen reader hears a switch named by its row, on or off. A `setting_check` would also work, but a checkbox reads as a form waiting to be saved, and this acts at once.
- **Portal row**: `pick_row` with three lines and the switch at the top right.
- **Community row**: a `list_row` with four columns: name over sentence, owners, the tag, the actions.
