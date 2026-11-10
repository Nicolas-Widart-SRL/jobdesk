# Portals

A new top-level Portals screen: the job boards Claude searches, in plain words for people who are not technical. Round 1 draws Your portals and a portal's detail in two layouts, A and B, with Browse community and Add a portal once.

- [Canvas and states](#canvas-and-states)
- [The screen](#the-screen)
- [Your portals](#your-portals)
- [A portal's detail](#a-portals-detail)
- [Browse community](#browse-community)
- [Add a portal](#add-a-portal)
- [Rules and why](#rules-and-why)
- [New widgets](#new-widgets)
- [Doc changes](#doc-changes)
- [Open](#open)

## Canvas and states

`Portals.html`, scope `.po`, on Grid.html's shell. Query: `?screen=yours|detail|browse|add&state=...&option=a|b&portal=linkedin|northfield&theme=light|dark`.

| PNG | Query | Shows |
| --- | --- | --- |
| `Portals-a-yours.png`, `-a-yours-dark.png` | `screen=yours&option=a` | A: the list beside LinkedIn's detail, light and dark |
| `Portals-a-northfield.png` | `screen=detail&portal=northfield&option=a` | A: a portal made by you, which ignores quotes and has Remove… |
| `Portals-a-remove.png` | `screen=detail&portal=northfield&state=remove&option=a` | A: Remove… asks first |
| `Portals-a-busy.png` | `screen=yours&state=busy&option=a` | A: a task runs, so the switches and the actions wait, with the reason on hover |
| `Portals-a-detail-busy.png` | `screen=detail&state=busy&option=a` | A: the same, with the reason on the detail's switch |
| `Portals-a-loading.png` | `screen=yours&state=loading&option=a` | A: reading the folder |
| `Portals-b-yours.png`, `-b-yours-dark.png` | `screen=yours&option=b` | B: the full-width table, light and dark, a row under the pointer |
| `Portals-b-busy.png` | `screen=yours&state=busy&option=b` | B: a task runs |
| `Portals-b-loading.png` | `screen=yours&state=loading&option=b` | B: reading the folder |
| `Portals-b-detail.png`, `-b-detail-dark.png` | `screen=detail&option=b` | B: LinkedIn as its own view, with a back link |
| `Portals-b-northfield.png`, `-b-remove.png`, `-b-detail-busy.png` | `screen=detail&option=b&portal=northfield`, `&state=remove`, `&state=busy` | B: made by you, Remove… asking, a task running |
| `Portals-nofolder.png` | `screen=yours&state=nofolder` | No job search folder chosen |
| `Portals-none.png` | `screen=yours&state=none` | A folder with no portal installed |
| `Portals-failed.png` | `screen=yours&state=failed` | `.agents/skills` could not be read |
| `Portals-browse.png`, `-browse-dark.png` | `screen=browse&state=cached` | The community list, saved 12 minutes ago |
| `Portals-browse-loading.png` | `screen=browse&state=loading` | The first read from GitHub |
| `Portals-browse-offline.png` | `screen=browse&state=offline` | GitHub unreachable, the saved list shown |
| `Portals-browse-limited.png` | `screen=browse&state=limited` | GitHub's rate limit, the saved list shown, Try again waits |
| `Portals-browse-nocache.png` | `screen=browse&state=nocache` | GitHub unreachable and no saved list yet |
| `Portals-browse-tip.png` | `screen=browse&state=tip` | What Install… does, on hover |
| `Portals-add-empty.png`, `-add-filled.png`, `-add-invalid.png` | `screen=add&state=...` | The form empty, filled with the command shown, and with an address that is not one |

The no-folder, none, failed, Browse and Add states are the same in both options.

## The screen

- The Portals tab sits after Profile and before Setup (`folder_tab`). Its failed mark is the `STATUS_DOT` square in `closed`, as on other tabs.
- The page head: Portals in `Type::H1`, a subtitle that counts the portals on, and Check all portals… (`command_action` on `/scrape health`) as the page action.
- Under the head, three `segmented` segments: Your portals with its count, Browse community, Add a portal.
- Search terms and the window of 14 days and about 20 results come from the folder, not from Jobdesk. The canvas uses the sample folder's fictional terms.

## Your portals

- One row per portal under `.agents/skills/*-search/`. The name is the board's plain name, never the folder name. Then the country, one sentence on what it searches, the switch, jobs found, the day of the last new job, the source tag, and the badges that apply.
- Two groups, On ("Claude searches these") then Off ("Claude skips these"). A row keeps its place when its switch flips, and moves to its group the next time the screen opens, so nothing jumps under the pointer.
- Off rows are quieter: their words at `FADED_OPACITY`, the switch at full ink.
- Source tags, a `row_tag` in `Tone::Plain`: Built-in (shipped with ai-job-search), Community (installed from the community list), Made by you (made with `/add-portal`). Badges in `Tone::Interview`, the Jobs screen's "Worth knowing" ink: Needs API key, Personal use only.
- Jobs found counts the entries in `job_scraper/seen_jobs.json` with that `portal`, and Last new job is the latest `first_seen` among them. With none: "No jobs yet" (A) or "None yet" in `faint` (B). If the jobs file cannot be read, both read as Setup's `hidden_note` says today.
- **A**: the list is a `panel` slip of pickable rows (`pick_row`, the `PICK_EDGE` edge on the picked one) beside the detail on paper, as Profile's rail sits beside its topic. Each row is three lines: name and country with the switch, the sentence, then jobs, last new job and tags.
- **B**: the list is a `pick_table` on paper over the full width: On or off, Portal (name over the sentence), Country, Jobs found, Last new job, Kind, Worth knowing, and a `›`. A click on a row, outside its switch, opens the detail view. A one-line footnote under the paper says what opening and the switch do.

## A portal's detail

- Head: the plain name in `Type::DEAL` (A) or `Type::H1` (B), the country and the sentence, the switch at the right, jobs found, the last new job and the tags.
- Actions, each a `small_command_action` that opens the launcher: Check this portal… on `/scrape health <folder name>`, and Change search terms… on `/setup --section search`. Remove… only on Community and Made by you portals. A built-in portal shows instead: "Built-in portals stay installed. Turn one off to skip it."
- How it searches: four rows in plain words, made from the portal's `Query syntax:` line and its search options. Exact phrases, Combine with OR, Location, and what each search finds. Each answer leads with a bold Yes, No or a short phrase, then one sentence.
- Searches it will run: the shared search terms, grouped by priority (a `panel` chip "Priority N", then the category's name), each term as this portal receives it. A portal that honours quotes shows multi-word terms in quotes, and one that ignores them shows them bare. The right column says it in words: These words together, These words in any order, One word. The lead says plainly that the terms are shared by every portal. The count says how many searches that makes.
- If it fails: the `site:` lines from the search file that name this portal's site, in a `code_box` (Courier, as typed syntax), with one line saying where they come from. With none, one sentence says so.
- Footnote: the portal's folder and the search file, as `path_label`.
- **A** puts all of it on one paper. **B** gives the head to the page, puts the searches on paper and How it searches and So far (jobs found, last new job) on slips beside it, under a `link` "‹ Your portals" that Esc also follows (`back_pressed`).

## Browse community

- A `search_field` that matches name, country and description, then the list's age at the right: "From GitHub, updated 12 min ago" with Refresh.
- A `note` slip with an `interview` square: community portals are code other people wrote, it runs next to your career data, and Install… opens a task in which Claude reads the code and says what it does before anything installs.
- Paper grouped by country, the user's market first and marked "your market", then Worldwide, then the rest by name. The market is the country of the user's Location filter. A market with no portal reads "No community portals for <country> yet", with Add a portal.
- Each entry: name over its description, owners, a tag (Maintained in `Tone::Plain`, "Unmaintained since YYYY-MM" in `Tone::Interview`), View code ↗, and Installed (`Tone::Hired`) or Install…. Installed matches by folder name.
- Install… is a `small_command_action_hinted`. It opens the launcher on a task that copies the portal into `.agents/skills/`, explains its code, then asks before it runs the CLI installer. Its hover says nothing runs until the user starts it.
- States: loading (grey lines), cached, offline with the saved list (an alert box in `interview`), rate-limited with the saved list and Try again disabled until GitHub's reset time, and offline with no saved list (empty state with Try again and a link to the list on GitHub).

## Add a portal

- A short form on paper: the job board's website, Country and Language of its job ads side by side, and One job title to test with. Hints start with "Such as", so an empty field never reads as filled.
- Build it with Claude… is the primary, a `primary_command_action` on `/add-portal <website>`. It waits for a website, with the reason beside it, and for one that is a web address. A typed address without a scheme gets `https://`.
- "Hide the command" shows the line in a code box, as `reveal_command` does.
- One line: it is a task, so it costs like any other and shows in Usage, and nothing runs until the user starts it.
- A `slip` beside it: What happens next, three numbered steps (`knot`), and one line on boards that need a sign-in or a key.

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
- **Portal row** (A only): `pick_row` with three lines and the switch at the top right.
- **Community row**: a `list_row` with four columns: name over sentence, owners, the tag, the actions.

## Doc changes

- `ARCHITECTURE.md`: the `workspace` feature gains the community list (fetch, cache, GitHub's rate limit), the switch's write to `SKILL.md` frontmatter and the move of a portal folder to the Trash. Or a new `portals` feature, joined to `FEATURES`, if the lead prefers. Add Portals to the screen names. Add the GitHub API to the network calls, next to the update feed.
- `docs/guide.md`: a Portals section, screen by screen, after Profile. Setup's portal table links to it (see Open).
- `docs/architecture/trust.md`: a row in Rendering untrusted text for community portal names, descriptions and owners, and for the names and sentences read from installed `SKILL.md` files.
- `AGENTS.md` hard rules: the switch and Remove are the two writes Jobdesk makes outside Profile, and both wait while a run works. The community list is not a run.
- `docs/architecture/ui-kit.md`: the `switch` widget, and when to use it rather than `setting_check`.

## Open

- **Option A or B.** A keeps the list in sight and matches Profile, but the list column is narrow, so each row takes three lines. B shows every fact in one row and gives the searches the full width, at the cost of a click and a back link.
- **Setup's portal table.** Suggest it stays as a summary and gains Open Portals, as its Profile panel has Open Profile. Check portal health and Add portal move to the Portals screen.
- **Remove.** Drawn as Jobdesk moving the folder to the Trash. Should it rather open a task?
- **Built-in portals.** Drawn with no Remove, only Off. Keep that?
- **Add a portal's fields.** `/add-portal` takes only the website. Do country, language and the test title go into the launcher line after it, or only into the first reply?
- **Where the switch's change shows.** Drawn as a row keeping its place until the screen opens again. Moving it at once is the other choice.
