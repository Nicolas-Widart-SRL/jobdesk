# Architecture

Jobdesk is a desktop view over an `ai-job-search` checkout. It follows domain-driven
design, packaged by feature. The checkout stays the source of truth: slash commands are
the only writers of its state, and Jobdesk never writes to it.

## Features

Each feature is a bounded context with its own language.

| Feature | Language | Reads |
| --- | --- | --- |
| `discovery` | job, lane, assessment, gate, signal, freelance terms | `job_scraper/seen_jobs.json`, the candidate profile and evaluation framework |
| `tracking` | application, status, bucket, follow-up, outcome | `job_search_tracker.csv`, `documents/applications/*/outcome.md` |
| `insights` | search report, funnel, skill gap, study order | tracking's application layer, discovery's application layer, the profile count, `upskill/report-*.md` |
| `intray` | card, decision, skip reason, queue | discovery's, tracking's, dossier's and runs' application layers, its own decisions file |
| `runs` | run, activity, prompt, decision, step rail, command catalog | the `claude` CLI stream, its own run logs |
| `dossier` | archive, document, findings, page | `documents/applications/*`, compiled PDFs, `company_research/`, tracking's and runs' application layers |
| `workspace` | workspace root, profile state, portal, tool check | the checkout folder, installed tools, file changes |

`shared_kernel` holds the few value types every feature agrees on, plus the host `PATH` lookup that `runs` and `workspace` both need. `ui_kit` is the single owner of
the Ledger design system: colour, spacing, radius, stroke and type tokens, plus every widget that paints them. It knows no feature, and screens only arrange its widgets, which `tests/architecture.rs` enforces. `tests/ui_kit_gallery.rs` snapshots every component in both themes. `shell` is the composition root: it loads every feature into one
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

A worker thread rebuilds an immutable `Snapshot` when watched files change and sends it to the
UI thread. The UI never reads from disk. A feature that fails to load reports an issue and the
others keep working.

Each run owns one `claude -p` child process, with the stream-json protocol over stdin and stdout. A supervisor thread owns the child and a reader thread turns stdout lines into domain events. The UI thread applies them once per frame through `RunDesk::poll`. Dropping a session ends its process.

The queue is derived, never stored. `intray` deals cards from the latest snapshot and the live runs on every frame, then hides what the user decided. Paused runs rank first, then dates within 14 days, then the rest. The card being read stays on top, so new work lands behind it. Decisions and skip reasons live in `decisions.json` in the app's data folder. A followed card returns the next day, so a command that never ran asks again.

Later is not a decision on the card. It sets the card aside until the next day, which stands in for the morning pass until standing orders exist. A card set aside adds no log entry, keeps its timeline mark and leaves a paused run paused. Its mark pulls it back to the top. Card keys change when the underlying state moves on, so a decision never outlives what it decided.

No card action runs anything by itself: Y opens the launcher on the slash command, and the user starts it there. Permission, question and guard cards have no Y action.

`/rank` stores no contract, day rate or commute fields, so `discovery` reads them from what it does store: the title, location, strengths and gaps. A gate with nothing to read flags, because silence is not a pass. Failed freelance gates suggest a skip and never change a job's lane, because `/rank` owns the shortlist. The day-rate floor comes from `01-candidate-profile.md` and the commute tiers from `04-job-evaluation.md`. With neither, the gates flag instead of guessing.

## Trust

Posting text, notes and reports are third-party or generated data. Screens render them as
plain text, and a link opens only on click and only for `http` or `https`. Claude's replies and prep packs
render as Markdown with no image loading. Archived postings always render as plain text. Paths read
from the tracker must stay inside the workspace.

By default `RunDesk` allows every tool permission itself and adds a notice to the activity for each one, so a run never stalls on a prompt nobody sees. This trades away the prompt as a check against injected posting text. Postings rely on `posting_guard` in the workspace instead. Questions from `AskUserQuestion` always reach the user. With `ask_permission` on, which a setting will expose later, every `can_use_tool` request reaches the user with its input, marked as shortened past 20 KB. The CLI suggests saving allow rules to project settings, so `stream_json` rewrites every remembered rule to session scope to keep the checkout unchanged.

A pending prompt stays until it is answered or withdrawn, or the process ends. A turn's end does not clear it, because a background subagent can still be waiting on it. Background subagents also keep the run busy after its turn ends, because they can still write files.
