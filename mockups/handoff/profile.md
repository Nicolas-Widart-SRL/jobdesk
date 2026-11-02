# Profile

Brief: a new Profile tab, before Setup, where the user reads their profile by topic, asks Claude to update one topic through `/setup --section <topic>`, and edits any `##` or `###` block in place. File names stay out of the navigation. Jobdesk writes these files itself, one block per Save. Approved: a topic rail beside one topic on paper.

- [Canvas](#canvas)
- [Layout](#layout)
- [Topics](#topics)
- [Rules](#rules)
- [Setup's Profile panel](#setups-profile-panel)
- [Code home](#code-home)
- [New widgets](#new-widgets)
- [Later ideas](#later-ideas)

## Canvas

`Profile.html?state=…&theme=light|dark`, scope `.pf`, built on `Grid.html`'s shell. Each PNG is 1280x820.

| PNG | State |
| --- | --- |
| [Profile-view.png](../Profile-view.png) | Full: Experience picked, four roles as a CV list, two blanks to fill, Edit on every role, Add a role at the end. |
| [Profile-view-dark.png](../Profile-view-dark.png) | The same in dark. |
| [Profile-edit.png](../Profile-edit.png) | One role open in the editor. The other Edit buttons and Edit section are disabled. |
| [Profile-busy.png](../Profile-busy.png) | The editor while a run works: Save disabled with its reason on hover, and Update with Claude… disabled. |
| [Profile-conflict.png](../Profile-conflict.png) | The role changed on disk during the edit: the new lines, Use the new version, and Save mine over it. |
| [Profile-empty.png](../Profile-empty.png) | Goals and deal-breakers, still all placeholders: the empty state leads to Update with Claude…, with the command shown, then the topic's outline of blanks. |
| [Profile-loading.png](../Profile-loading.png) | Before the first read: "Reading your profile…", with the rail and paper in skeleton lines. |
| [Profile-failed.png](../Profile-failed.png) | One profile file is unreadable: the alert under the head, its six topics Hidden, and "hidden, not gone" on paper. The tab carries the failed square. |
| [Profile-updated.png](../Profile-updated.png) | After a task changed two topics: Identity and languages carries Updated on the rail. Experience is open, so its mark has cleared, and its head says what the task changed. |
| [Profile-setup.png](../Profile-setup.png) | The Setup screen, with its Profile panel shrunk to one summary line and Open Profile. The other panels are as built. |

## Layout

The rail is a slip of pickable rows in two groups: On your CV and What guides Claude. The groups tell the user which topics print on a CV and which only steer Claude. A fat tally above the rows gives one mark per topic: `text` when filled, `interview` when something is left to fill, `closed` when hidden. The topic sits on paper with its head (title, one line on what it is for, and its actions), the document scrolling under it, and a footnote strip that names the file. One topic stays in view, so the actions in the head always belong to what the user reads, and the rail shows what is left at a glance.

## Topics

Eleven topics, matched by the headings of the upstream templates. `TOPICS` in `src/profile/domain/topic.rs` holds the exact headings.

| Topic | Group | File and sections | `--section` |
| --- | --- | --- | --- |
| Identity and languages | On your CV | `01-candidate-profile.md` Identity and Languages, and the profile part of `CLAUDE.md` | `identity` |
| Experience | On your CV | `01` Professional Experience, one `###` per role | `experience` |
| Education | On your CV | `01` Education | `education` |
| Skills | On your CV | `01` Technical Skills, and `04-job-evaluation.md` skill match areas | `skills` |
| Projects and publications | On your CV | `01` Independent Projects, Publications and Awards | `publications` |
| References | On your CV | `01` References | `references` |
| Profile statements | On your CV | `05-cv-templates.md` | none |
| How I work | What guides Claude | `02-behavioral-profile.md` | `behavioral` |
| Goals and deal-breakers | What guides Claude | `04` career goals, motivation filters and commute tiers | `goals` |
| Interview stories | What guides Claude | `07-interview-prep.md` STAR stories | none |
| Job search | What guides Claude | `search-queries.md` | `search` |

`03-writing-style.md` and `06-cover-letter-templates.md` are not topics.

## Rules

**Reading**
- The document goes through `profile_markdown`, which is `markdown` with comment-only HTML hidden, as Trust says for profile text: no images, and links open only on click, only for `http` and `https`. Text in the document is `copyable`. The Edit buttons sit outside the copyable text, at the paper's right edge.
- `CLAUDE.md` shows only its profile part, the `###` blocks under `## Candidate Profile`, never its workflow rules, and only that part is editable. Each block joins its topic, and its sub-heading reads "Summary in CLAUDE.md" or "<heading> in CLAUDE.md".
- A `###` heading shaped `Title - Company (dates)` renders as a role (`role_entry`): the dates as written, in a fixed column, then the title, the company line, and the body. Any other `###` renders as a plain heading with its own Edit.
- A topic made of one `##` section puts that section's Edit section in the paper head. A topic made of several `##` sections, such as Projects and publications, shows each as a sub-heading with its own Edit section.
- A `[PLACEHOLDER]` token shows as written in the document body. Only a role's title, company and dates line draws it as a blank (`blank_mark`): "to fill", or "to fill: hint" when the token carries a hint. Counts come from one plain parse, `shared_kernel::placeholders`: a token counts where the page shows it, in prose or in code, never in link addresses, definitions or HTML. HTML blocks that hold only comments are hidden on the paper.
- Rail state words: "Filled" in `muted`, "N to fill" and "Not started" in `interview`, "Hidden" in `closed`. Not started means that the topic's sections show nothing but headings, blank lines and comments, so a fresh checkout reads "N to fill" for every topic.
- The footnote strip says where the topic lives: "Kept in", the file name in `faint_mono`, then the `##` names. On the right it shows the count to fill, or "Changed on disk" with its age during a conflict. It is the only place a file name appears on this screen.

**Talk to Claude**
- Update with Claude… is a `small_command_action` that opens the launcher with `/setup --section <topic>`. It never starts a run. The user starts it in the launcher, and it plays out in Tasks.
- It is disabled while a run works ("Another run is active. One run at a time keeps your files safe.") and while the topic's file cannot be read ("Fix the file first, so Claude does not write over it.").
- Interview stories and Profile statements have no `--section`, so they have no Claude action in v1. They are edited by hand only.
- A live test of `/setup --section experience` made a small targeted edit, not a rewrite, and touched only `CLAUDE.md` and `01-candidate-profile.md`. It took three turns and cost about $1.00.
- **Build check**: in headless mode Claude Code refused `Edit` on files under `.claude/skills/` even with `acceptEdits`. A live test showed that a Jobdesk run, which allows tool requests through `can_use_tool`, can edit the profile files under `.claude/`.

**Updated by Claude**
- When a run ends, Jobdesk compares each topic's blocks in the first snapshot read after it with the snapshot from the run's start. A topic whose block text changed gets an `Updated` `tag` in `Active` on its rail row, between the name and the state word. A file change that leaves a topic's blocks alone does not mark that topic, so a one-line edit marks one topic, not every topic in the file.
- Updated uses `active`, Claude's ink, as an outlined `tag`. It never uses stamp ink, because nothing waits on the user, and never a solid tag.
- Opening a marked topic clears its rail mark. While that topic stays open, its head carries one line under the lead: the same tag, then "by the last task, N minutes ago. It changed …", naming the changed blocks by heading (or "the section" for a `##` block). Leaving the topic removes the line.
- Marks live in memory only and are gone after a restart. A Save from the Profile screen never marks a topic, because it never happens while a run works. A change made outside Jobdesk while a run works is marked too, which the user accepted.
- The tag's hover says "Updated by the last task. Open the topic to see it."

**Quick edit**
- Each `###` block has a small Edit button, and each `##` block has Edit section. They show at rest on every block, not only on hover, so the user finds them without hunting.
- Edit swaps the block in place for the editor (`block_editor`) with the block's raw Markdown in the body face. The editor uses the body face because Courier stays for commands and paths.
- One editor is open at a time. While it is open, the other Edit buttons and Edit section are disabled ("Save or cancel the open edit first."). Add a role opens the editor on a new block with a `### Title - Company (dates)` stub, placed after the last role.
- Preview is a `small_toggle` that renders the draft as the view would.
- Save (`primary_shortcut_button`, Cmd Enter) replaces only that block's lines and keeps the rest of the file byte for byte. Cancel (Esc) closes the editor. As `back_pressed` says, the first Esc leaves the field. With unsaved text, Cancel, leaving the topic and leaving the tab each ask first, and Keep editing is the default.
- **Busy**: while any run works, Save is disabled with the reason on hover ("A task is running and may change this file. Save once it ends."). A line in `active` ink beside Cancel says the same thing, so a user who does not hover still sees why. The draft stays. When the run ends, Save stays disabled the same way until Jobdesk has read the files the run changed ("Jobdesk is reading the files the task changed. Save once that is done."), so a Save is never marked as the run's. That wait lasts five seconds at most: then Jobdesk reads the profile files itself.
- **Changed on disk**: just before writing, Save reads the block on disk again. When it differs from the text the editor opened with, an `interview` warning box opens at the top of the editor. It shows the block's new lines as plain, copyable text, with each added line marked by an `interview` edge. Use the new version replaces the draft with the file's text. Save becomes Save mine over it, a danger button, so the label says what Save does. Changes elsewhere in the file never cause a conflict, because Save replaces only the block. When the block is gone from the file, the box says so, and Save writes the draft back at the end of the block's section, or at the end of the file when that section is gone too. A new role whose section is gone goes at the end of the file.

**Other states**
- **Empty topic**: `empty_state` at the top of the paper. A sentence says what Claude will ask, and the actions are `primary_command_action` Update with Claude…, the button Write it myself (the editor on the whole section) and `reveal_command`. Under a hairline, "What this topic holds" shows the template at `FADED_OPACITY`, with its blanks. Without a `--section`, Write it myself is the primary and the only action.
- **Loading**: the head says "Reading your profile…", and the rail and the paper show `skeleton_lines` in their shapes.
- **Failed**: `alert` under the head names the file, says what is wrong and how to fix it, and says the rest still works. The topics from that file read Hidden, and the paper says "Your … is hidden, not gone". Jobdesk reads the file again when it changes, so no retry button is needed. The tab takes the `STATUS_DOT` square in `closed`. If none of the profile files is found, the screen shows one `empty_state` with Set up your profile…, which opens `/setup` (not drawn).
- **Dark**: `card` and `panel` are the same in dark, so the editor slip keeps a `line` hairline to stay visible on paper.

**Measures**: the rail takes `RUN_SIDE`'s width. The role date column takes `COMMAND_NAME_WIDTH`, which fits "2021 – present". Role bodies keep to `DOC_MEASURE`. The paper head uses `DEAL_PADDING` with the title in `Type::DEAL`. Role titles use `Type::SECTION` and bullets use `Type::TAKE`. A builder who would rather alias the two widths under profile names keeps their values.

## Setup's Profile panel

- The panel keeps its header and path note. Its per-file rows and Update buttons go. One line remains: the fat tally of topics, "8 of 11 topics filled", and `small_button` Open Profile at the right, which switches to the Profile tab. Open Profile opens no launcher, so it has no "…".
- When topics are hidden, the line adds "N hidden" in `closed`. When no profile file is found, the line says so and the button reads Set up your profile…, which opens `/setup`.
- `03-writing-style.md` and `06-cover-letter-templates.md` leave Setup with the per-file rows.
- Setup's promise line and first run's promise and step 2 say that Jobdesk never changes the folder. They change to say that Jobdesk changes it only when the user saves a Profile edit, and step 2 points at Profile instead of Setup.

## Code home

- A new `profile` feature, with its language: topic, block, placeholder, draft, conflict, updated mark. It reads and writes the profile files in the Topics table. It joins `FEATURES` in `tests/architecture.rs`.
- `placeholder_count` moves from `workspace`'s domain to `shared_kernel`, beside the parse that `profile` counts with and `ui_kit` draws role blanks from. Setup's summary line reads topic counts through `profile`'s application layer.
- The UI still reads the `Snapshot`. Save's re-read of the block and its write sit in `profile`'s infrastructure.

## New widgets

- **`role_entry`**: one role as a CV line. The dates column holds `Type::NOTE` in `muted`. Beside it, the title in `Type::SECTION` sits over the company in `muted` and the body through `markdown`, held to `DOC_MEASURE`. A small Edit button sits at the right edge, and a `line` hairline separates it from the role above.
- **`blank_mark`**: the placeholder look in a role's title, company and dates line, as a text format rather than a widget, so it flows in the line. It is `interview` ink in `Type::TAG`'s semibold face, with a `HAIRLINE` underline in `interview`.
- **`block_editor`**: a slip (`panel` with a `line` hairline, `RADIUS_PANEL`, `INSET`). A title row holds "Edit this role" and Preview. Then comes a multiline field on `field` (`FIELD_PADDING`, the focus ring while focused). The footer holds Save, Cancel, a note in `Type::NOTE` and "Unsaved changes" in `muted`. For a conflict it holds `prompt_box_tone` in `Interview`, with the changed lines in a `copyable` `code_box` and a `PICK_EDGE` mark in `interview` on each added line.
- **`danger_shortcut_button`**: `danger_button` with its chord, like `primary_shortcut_button`, for Save mine over it.

The rail is built from existing parts: `slip`, `group_title`, `pick_row` with the state word in `Type::NOTE` or `Type::TAG` and an optional `tag`, and the tally that takes runs of `(count, ink)` at the fat size. The Updated mark is the existing `tag` in `Tone::Active`.

## Later ideas

Out of scope now:
- A small upstream PR to `ai-job-search` could add `/setup` sections for interview stories and profile statements, and name the valid section values, so both topics can get Update with Claude….
- A CV PDF preview beside the topic.
- A history or diff of profile changes, including what the last task changed.
