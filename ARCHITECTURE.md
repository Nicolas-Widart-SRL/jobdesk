# Architecture

Jobdesk is a desktop view over an `ai-job-search` checkout. It follows domain-driven
design, packaged by feature. The checkout stays the source of truth, and slash commands write
it. Jobdesk writes only the profile files, from the Profile screen's Save: one block at a time, at
fixed paths under the root, never while a run works, and only when the block on disk is unchanged
or the user chose Save mine over it. Everything else Jobdesk writes stays in its data folder.

## Features

Each feature is a bounded context with its own language.

| Feature | Language | Reads |
| --- | --- | --- |
| `discovery` | job, lane, assessment, gate, signal, freelance terms | `job_scraper/seen_jobs.json`, the candidate profile and evaluation framework |
| `tracking` | application, status, bucket, follow-up, outcome | `job_search_tracker.csv`, `documents/applications/*/outcome.md` |
| `insights` | search report, funnel, skill gap, study order | tracking's application layer, discovery's application layer, the profile's topic counts, `upskill/report-*.md` |
| `intray` | card, decision, skip reason, queue | discovery's, tracking's, dossier's and runs' application layers, its own decisions file |
| `runs` | run, activity, prompt, decision, step rail, stop point, quick reply, step phrase, command catalog, usage (tasks only, from run logs) | the `claude` CLI stream, its own run logs |
| `dossier` | archive, document, findings, page | `documents/applications/*`, compiled PDFs, `company_research/`, tracking's and runs' application layers |
| `profile` | topic, block, placeholder, draft, conflict, updated mark | the profile files that `TOPICS` in `profile/domain/topic.rs` names, and the `## Candidate Profile` part of `CLAUDE.md` |
| `workspace` | workspace root, portal, tool check | the checkout folder, installed tools, file changes, profile's application layer |
| `settings` | preference, page, search hit | its own `settings.json` in the data folder, workspace's readiness through the shell, runs' and tracking's application layers through the `Snapshot` |

Screen names differ from feature names: `intray` is Today, `tracking` is Applications, `runs` is Tasks, `profile` is Profile, `workspace` is Setup, and `settings` is Settings. Runtime status is drawn by workspace's UI and placed in Settings by the shell.

`shared_kernel` holds the few value types every feature agrees on, plus the host `PATH` lookup that `runs` and `workspace` both need, `visible_text`, the filter that drops hidden characters from untrusted text, `markdown::options()`, the one Markdown option set that the viewer and the closing-question parser share, and `placeholders`, the one parse that finds the setup tokens a page of profile text shows. `profile` counts them with it, so Setup and Insights count what the Profile screen shows, and `ui_kit` draws them as blanks in role lines. `ui_kit` is the single owner of
the Ledger design system: colour, spacing, radius, stroke and type tokens, plus every widget that paints them. Colour is for job status, plus one stamp ink for what waits on the user. The window is a gridded desk in both themes, and anything read in rows sits on paper. It knows no feature, and screens only arrange its widgets, which `tests/architecture.rs` enforces. Every clickable kit widget shows the pointing hand, a hover, a pressed and a focus state, and paints its own disabled look with the reason on hover. A picked row or card adds an ink edge to the hover fill. Labels are not selectable, so a click on a row's text picks the row. Only documents, code boxes, alerts and load errors, your replies, Claude's questions shown whole and their choices, the research note, the archived posting, drafts and the folder path in Settings › General can be selected and copied, and none of them sits inside something clickable. `tests/ui_kit_gallery.rs` snapshots every component in both themes. `shell` is the composition root: it loads every feature into one
snapshot and owns the window.

## Layers

```
src/<feature>/
  domain        entities, value objects, rules, repository ports
  application   use cases and read models, the feature's public API
  infrastructure  adapters that implement the ports
  ui            egui screens
```

| Layer | May depend on |
| --- | --- |
| `domain` | own `domain`, `shared_kernel` |
| `application` | own `domain` and `application`, other features' `application`, `shared_kernel` |
| `infrastructure` | own `domain` and `infrastructure`, `shared_kernel` |
| `ui` | own `application` and `ui`, `ui_kit`, `shared_kernel` |

`domain` and `infrastructure` are private modules of their feature, so the compiler stops other
features from reaching them. Each feature's `mod.rs` wires its adapters and exposes one load
function. `tests/architecture.rs` enforces the remaining rules, including that `domain` and
`application` never touch egui, the file system or serialization crates.

## Runtime

PDF pages render on worker threads with `hayro`, a pure Rust rasterizer, so no PDF library ships beside the app. A rebuilt PDF renders again because pages are keyed by path and modification time.

A worker thread rebuilds an immutable `Snapshot` when watched files change, every hour and at
local midnight, and sends it to the UI thread. So the app's today changes at midnight, not up to an hour later. The UI never reads from disk. A feature that fails to load reports an issue and the
others keep working. Its tab carries a mark, and an alert names what failed and what still works. Until the first snapshot arrives, every screen shows a loading view, not an empty one. A screen whose source failed shows the alert under its head and says its content is hidden, never that it is empty. The shell tells each screen both through `ScreenState`: whether its source is still being read, failed or ready, and which `Source`s failed, for a screen fed by more than one.

The profile reaches the `Snapshot` as topics. `TOPICS` in `profile/domain/topic.rs` names each topic's sections by the headings of the upstream templates, and from `CLAUDE.md` only the `###` blocks under `## Candidate Profile` join their topics. A topic is Hidden when a file that feeds it cannot be read, Not started when its sections show nothing but headings, blank lines and comments, else N to fill or Filled by the tokens `placeholders` counts, so a fresh checkout reads N to fill for every topic. Insights and Setup count topics, not files, through `topic_counts`. Save goes through `ProfileEditor`. While a run works it returns before reading anything. Otherwise it reads the file again, finds the block by its heading and compares it with the text the editor opened. When the block changed or is gone, it writes nothing and the editor shows the conflict, until the user chooses Save mine over it. Then it replaces only that block's bytes and writes through `profile`'s infrastructure, `ProfileFiles`, which knows only the fixed profile paths under the root. It refuses a file whose links resolve outside the root and a read-only file. On Unix it also refuses a file another user owns, because the rename would hand it to the current user, and a file the current user cannot open for writing, as an access list can deny. It writes a copy beside the file with the file's permission bits, syncs it and renames it over the file, so a crash leaves the old text or the new, never half. Each save names its copy `<file>.jobdesk-save-<process id>-<number>` and creates it only where nothing is, so two Jobdesk windows saving one file at once never write into or remove each other's copy. A save removes its own copy when it fails. A save interrupted before its rename, by power loss or a force quit, leaves its copy, which holds the profile's text and escapes a `.gitignore` line naming the profile file. So each save first removes the copies of the same file that are named exactly like one, are regular files, not links, by `symlink_metadata`, and were last changed over an hour ago. A save takes well under a second, so a live save loses its copy this way only when it stalls for an hour, and then its rename fails and it reports the failure. The rename does not compare again, so when another program, such as an editor, a sync tool or a `claude` session in a terminal, writes the file between the compare and the rename, Save replaces that write without warning: the guard on a working run covers only Jobdesk's own runs. ACLs, extended attributes and other hard links do not follow. The root check runs once, before the write: the standard library has no folder-relative open, so a folder swapped for a link between the check and the write could still send the write outside the root. Updated marks compare the profile in the `Snapshot` when a run starts working with the first `Snapshot` read after it stops, by `read_at`. When none arrives within `RUN_READ_WAIT`, five seconds, the Shell reads the profile files itself and compares that. A change made outside Jobdesk while a run works is marked too, and a Save never is: Save waits through the run and that read. `UpdatedMarks` keeps them in memory only. Opening a topic clears its rail mark.

Preferences live in `settings.json` in the app's data folder and apply at once: the theme, egui's zoom for the text size, and Reduce motion, which zeroes `ui_kit`'s motion time at runtime. They reach the window at launch and when one changes, as Appearance changes it, never on every frame. With no file yet, as on the first launch after the theme left the status bar, the theme and zoom eframe restored are kept and saved. A file that cannot be read gives the defaults and stays until the next change replaces it, and the rail's foot tells that apart from a change that could not be saved. The Setup tab's presence is derived on every frame, never stored: no folder chosen, or a required tool not found, keeps it in the menu. Profile files and optional tools never do, and tools not checked yet count as nothing. Usage statistics reads the run logs through the `Snapshot`, and the snapshot reloads when a task ends, that is, when the run desk goes from busy to idle.

Each run owns one `claude -p` child process, with the stream-json protocol over stdin and stdout. A supervisor thread owns the child and a reader thread turns stdout lines into domain events. The UI thread applies them once per frame through `RunDesk::poll`. Dropping a session ends its process.

Claude waits on the user when a prompt is pending, even while the run stops, or when a turn ended with a known stop point or a closing question (the last top-level paragraph of its last message, ending in "?"). A turn that ends with neither is done for now, wherever the rail stands: the band says nobody's turn, a step left part-way reads paused and does not count as done, and the reply field stays under the story. `runs::application::wants_reply` holds that rule. It drives the count on the Tasks tab, and the band's Yours cell and waiting step. `intray` deals a Your turn card from `turn_card`, which follows `wants_reply` with rules of its own: none for a pending prompt, which gets the paused run's card, none while another run works, and none once every step is done. Its only action is Open task. Each activity row keeps the step the rail was on when it arrived, so the Tasks screen folds rows under their step. Each step's plain sentence comes from a fixed phrase table in the command catalog (`catalog::phrases_for`), keyed by tool and path, never from Claude's text or a posting's. The technical rows open with "Show N actions", or "Show details" for a step with no actions but other folded rows. The rail is display only and never steers the run. It infers the step from what a tool does: file tools by path, scripts by the program actually run, agents by type. Quick replies are fixed text from the command catalog (`catalog::stop_points`), sent through the same path as a typed reply on the same session. They never start a run, and they stay disabled while another run works or the reply field holds a draft. `RunDesk::reply` refuses any reply while another run is busy, even on a live session. End session asks first, and Esc keeps the session open.

The queue is derived, never stored. `intray` deals cards from the latest snapshot and the live runs on every frame, then hides what the user decided. Paused runs rank first, then dates within 14 days, then the rest. Coming up lists the dated cards of the next 14 days, overdue ones first. When none is dated it says so in one line. The card being read stays on top, so new work lands behind it. The one exception is cards brought back from later: they deal on top, right after paused runs, in rank order among themselves, until each is decided. That lasts only for that day and lives in memory, not in `decisions.json`. Decisions and skip reasons live in `decisions.json` in the app's data folder. The last decision can be undone until the next one, on the day it was made. Its row heads the Done today rail, which sits outside the card's scroll area, so it stays in view at any window height. A followed card returns the next day, so a command that never ran asks again.

Later is not a decision on the card. It sets the card aside until the next day, which stands in for the morning pass until standing orders exist. A card set aside adds no log entry, keeps its timeline mark and leaves a paused run paused. Its mark pulls it back to the top. Coming up's first row counts the cards set aside, names them on hover, and brings them all back on top from its button. When nothing is left to decide, the end panel offers new work: "Find new jobs…" and, while postings wait for scoring, "Score {n} new jobs…". Both only open the launcher, so the user starts the run there. With cards set aside, a "Bring them back now" link in the panel does the same as Coming up's button. Bringing them back replaces the card being read. Card keys change when the underlying state moves on, so a decision never outlives what it decided.

No card action runs anything by itself: Y opens the launcher on the slash command, and the user starts it there. Permission, question, guard and Your turn cards have no Y action.

`/rank` stores no contract, day rate or commute fields, so `discovery` reads them from what it does store: the title, location, strengths and gaps. A gate with nothing to read flags, because silence is not a pass. Failed freelance gates suggest a skip and never change a job's lane, because `/rank` owns the shortlist. The day-rate floor comes from `01-candidate-profile.md` and the commute tiers from `04-job-evaluation.md`. With neither, the gates flag instead of guessing.

A job's check rows and the sentence that sums them up come from `JobChecks` in discovery's application layer. Each row has a typed kind, which gives both its label and its wording. Today's card and the Jobs detail both read it, so the two screens word a job the same way. Whether the user kept a flagged ad lives in `intray`, so the shell hands Jobs the kept job keys from intray's application layer, beside what waits in Today. An ad the safety check never saw reads as unchecked, not as flagged.

## Trust

Posting text, notes and reports are third-party or generated data. Screens render them as
plain text, and a link opens only on click and only for `http` or `https`. Claude's replies and prep packs
render as Markdown with no image loading. On the Tasks screen they read as documents, with headings capped at the section size. Claude's closing question, pinned above the reply box and shown on the Today card, renders as plain text with Markdown markup and hidden characters removed. Archived postings always render as plain text. Paths read
from the tracker must stay inside the workspace. The reason for a failed tool renders as plain text, with control, escape, bidi and zero-width characters removed. A load error's reason renders as plain text, with control, escape, bidi and zero-width characters removed.

Profile text renders through `profile_markdown`, which is `markdown` with HTML blocks that hold only comments left out: no images, and a link opens only on click, only for `http` or `https`. Other HTML shows as its text. Headings render as plain text with hidden characters removed. Placeholders such as `[FIRST_NAME]` show as written in the body. Only a role's title, company and dates line, which `ui_kit` lays out itself, draws them as "to fill" blanks. The editor shows the block's raw text, and only Preview renders it, as the paper does. Only the `## Candidate Profile` part of `CLAUDE.md` reaches the view, never its workflow rules.

By default `RunDesk` allows every tool permission itself and records each one as an allowance row in the activity, so a run never stalls on a prompt nobody sees. Each step folds its allowances behind its toggle. Other notices, such as why a run failed or stopped or which action was denied, show inline in the story as plain text with hidden characters removed, one row per non-blank line. The story keeps a notice's last 8 lines, because a stderr tail ends on its error, and shows the whole on hover. This trades away the prompt as a check against injected posting text. Postings rely on `posting_guard` in the workspace instead. Questions from `AskUserQuestion` always reach the user. With `ask_permission` on, which a setting will expose later, every `can_use_tool` request reaches the user with its input, marked as shortened past 20 KB. The CLI suggests saving allow rules to project settings, so `stream_json` rewrites every remembered rule to session scope to keep the checkout unchanged.

A pending prompt stays until it is answered or withdrawn, or the process ends. A turn's end does not clear it, because a background subagent can still be waiting on it. Background subagents also keep the run busy after its turn ends, because they can still write files.
