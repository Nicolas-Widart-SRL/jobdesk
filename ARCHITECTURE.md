# Architecture

Jobdesk is a desktop view over an `ai-job-search` checkout. It follows domain-driven design, packaged by feature.

The checkout stays the source of truth, and slash commands write it. Jobdesk writes only the profile files, from the Profile screen's Save:

- one block at a time, at fixed paths under the root;
- never while a run works;
- only when the block on disk is unchanged, or the user chose Save mine over it.

Everything else Jobdesk writes stays in its data folder. How Save works is in [profile](docs/architecture/profile.md#save).

## Features

Each feature is a bounded context with its own language.

| Feature | Language | Reads |
| --- | --- | --- |
| `discovery` | job, lane, assessment, gate, signal, freelance terms, your terms, Jobs view filter | `job_scraper/seen_jobs.json`; the candidate profile and evaluation framework; settings' application layer, for the Jobs view filters |
| `tracking` | application, status, bucket, follow-up, outcome | `job_search_tracker.csv`; `documents/applications/*/outcome.md` |
| `insights` | search report, funnel, skill gap, study order | tracking's and discovery's application layers; the profile's topic counts; `upskill/report-*.md` |
| `intray` | card, decision, skip reason, queue | discovery's, tracking's, dossier's and runs' application layers; settings' application layer, for the queue rules; its own decisions file |
| `runs` | run, activity, prompt, decision, step rail, stop point, quick reply, step phrase, command catalog, usage (tasks only, from run logs) | the `claude` CLI stream; its own run logs; settings' application layer, for the run defaults and the cap; the saved API key, through the shell's key reader |
| `dossier` | archive, document, findings, page | `documents/applications/*`; compiled PDFs; `company_research/`; tracking's and runs' application layers |
| `profile` | topic, block, placeholder, draft, conflict, updated mark, CV document, staleness | the profile files that `TOPICS` in `profile/domain/topic.rs` names; the `## Candidate Profile` part of `CLAUDE.md`; the `.tex` and PDF files in `cv/` and their modification times; whether `.claude/commands/build-cv.md` exists; dossier's application layer, for the page cache; settings' application layer, for ages and sizes |
| `workspace` | workspace root, portal, tool check | the checkout folder; installed tools; file changes; profile's application layer |
| `settings` | preference, page, search hit, run default, command override, spend cap, retention, sign-in method, API key, connection test, Coming up range, Later length, skip reason list, Jobs view filter | its own `settings.json` in the data folder; the system keychain, through a port in `domain` and adapters in `infrastructure`; through the `Snapshot`: runs' and tracking's application layers and discovery's terms; through the shell: workspace's readiness, the count of jobs the Jobs view hides, the data folder's sizes and the tray decision count |

Screen names differ from feature names: `intray` is Today, `discovery` is Jobs, `tracking` is Applications, `dossier` is an application's dossier, `runs` is Tasks, `profile` is Profile, `workspace` is Setup, and `settings` is Settings. Runtime status is drawn by workspace's UI and placed in Settings by the shell.

Three modules are not features:

- `shared_kernel` holds the few value types every feature agrees on, such as `ModelChoice`, and these shared helpers:
  - the host `PATH` lookup that `runs` and `workspace` both need, with `system_program` for the programs that ship with Windows;
  - `process_tree`, which starts every console program through `no_window` and stops a run with everything it started;
  - `replace_file`, whose `rename_over` replaces a file and on Windows tries a held file again a few times;
  - `file_text`, the helpers for a byte order mark and for CRLF and lone CR line endings;
  - `visible_text`, the filter that drops hidden characters from untrusted text;
  - `workspace_path::within_workspace`, the text check that keeps a path read from the checkout inside it, which `dossier` and `profile` share;
  - `markdown::options()`, the one Markdown option set that the viewer and the closing-question parser share;
  - `placeholders`, the one parse that finds the setup tokens a page of profile text shows. `profile` counts them with it, so Setup and Insights count what the Profile screen shows, and `ui_kit` draws them as blanks in role lines.
- `ui_kit` is the single owner of the Ledger design system: its tokens and every widget that paints them. It knows no feature, and screens only arrange its widgets, which `tests/architecture.rs` enforces. Its rules are in [ui_kit](docs/architecture/ui-kit.md).
- `shell` is the composition root: it loads every feature into one snapshot and owns the window.

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

### The Snapshot

A worker thread rebuilds an immutable `Snapshot` and sends it to the UI thread. It rebuilds:

- when watched files change;
- every hour;
- at local midnight, so the app's today changes at midnight, not up to an hour later.

The UI never reads from disk.

### Loading and failed sources

A feature that fails to load reports an issue, and the others keep working. Its tab carries a mark, and an alert names what failed and what still works.

- Until the first snapshot arrives, every screen shows a loading view, not an empty one.
- A screen whose source failed shows the alert and says its content is hidden, never that it is empty.

Jobs, Applications, Insights, Profile, Setup and Settings › Runtime status learn both through `ScreenState`: whether their source is still being read, failed or ready, and which `Source`s failed, in `unread`. They draw the alert under their head. Only Jobs and Applications get `Load::Failed`, when their own source fails. The others get only `Reading` or `Ready`: Insights, Setup and Runtime status check `unread` to hide the parts a failed source feeds, and Profile hides a failed file's topics through its own `ProfileView`, and its CV documents through `CvShelf::Unread`. Today gets the alert and the missing sources through a context of its own, and draws them under its head too.

Tasks, the launcher and Dossier take no `ScreenState`: the shell draws the alert above their head. For Dossier the shell also draws the loading view and, when the tracker failed, the note that the application is hidden.

### PDF pages

PDF pages render on worker threads with `hayro`, a pure Rust rasterizer, so no PDF library ships beside the app. `PageStore` in `dossier`'s application layer holds them, and Profile's CV documents share it through `profile`'s application layer, so a PDF both screens show renders once.

- Pages are keyed by path and modification time, so a rebuilt PDF renders again. Both screens name a CV PDF by its path under the root, links unresolved, so they share its key.
- Each page renders at a fixed 2x scale, whatever size the screen draws it at.
- A render stops after 6 pages and counts the rest, so the screens say "the first 6 shown".
- At most 8 rendered documents stay in memory. The one asked for longest ago leaves first, and renders again when asked for.
- A failed render of a file changed in the last 30 seconds may only mean a task was still writing it. On each new snapshot, the shell forgets such a failure once the file has stood still 2 seconds, so it renders once more. A file that fails again stays failed.

### Where the details are

- Changing how a task starts, stops, signs in or waits on the user: read [runs](docs/architecture/runs.md).
- Changing profile topics, Save, Updated marks or CV documents: read [profile](docs/architecture/profile.md).
- Changing preferences, Usage statistics, the monthly cap, retention or how Jobdesk replaces its own files: read [settings](docs/architecture/settings.md).
- Changing the Today queue, decisions, skip reasons or Later: read [Today queue](docs/architecture/today-queue.md).
- Changing the freelance gates, Your terms, the Jobs view filters or a job's check rows: read [discovery](docs/architecture/discovery.md).
- Changing a `ui_kit` token or widget: read [ui_kit](docs/architecture/ui-kit.md).

## Trust

Posting text, notes and reports are third-party or generated data. Screens render them as plain text, and a link opens only on click and only for `http` or `https`.
Claude's replies and prep packs render as Markdown with no image loading.
The API key goes only from the system keychain into the environment of a `claude` process, and the whole key is masked in what that process prints.
By default `RunDesk` allows tool permissions and records them in the activity, and Settings › Permissions turns prompts on.

Changing how untrusted text renders, a path check, the API key's path or tool permissions: read [trust](docs/architecture/trust.md). It also lists the accepted residuals.
