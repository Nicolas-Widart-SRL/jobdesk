# Architecture

Jobdesk is a desktop view over an `ai-job-search` checkout. It follows domain-driven
design, packaged by feature. The checkout stays the source of truth: slash commands are
the only writers of its state, and Jobdesk never writes to it.

## Features

Each feature is a bounded context with its own language.

| Feature | Language | Reads |
| --- | --- | --- |
| `discovery` | job, lane, assessment, gate, signal | `job_scraper/seen_jobs.json` |
| `tracking` | application, status, bucket, follow-up, outcome | `job_search_tracker.csv`, `documents/applications/*/outcome.md` |
| `insights` | search report, funnel, skill gap, study order | tracking's application layer, `upskill/report-*.md` |
| `overview` | workflow stage, attention item | discovery's and tracking's application layers |
| `runs` | run, activity, prompt, decision, step rail, command catalog | the `claude` CLI stream, its own run logs |
| `dossier` | archive, document, findings, page | `documents/applications/*`, compiled PDFs, `company_research/`, tracking's and runs' application layers |
| `workspace` | workspace root, profile state, portal, tool check | the checkout folder, installed tools, file changes |

`shared_kernel` holds the few value types every feature agrees on, plus the host `PATH` lookup that `runs` and `workspace` both need. `ui_kit` holds the design
tokens and generic widgets. `shell` is the composition root: it loads every feature into one
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

## Trust

Posting text, notes and reports are third-party or generated data. Screens render them as
plain text, and a link opens only on click and only for `http` or `https`. Claude's replies and prep packs
render as Markdown with no image loading. Archived postings always render as plain text. Paths read
from the tracker must stay inside the workspace.

Jobdesk never bypasses permissions. Every `can_use_tool` request reaches the user with its input, marked as shortened past 20 KB. The CLI suggests saving allow rules to project settings, so `stream_json` rewrites every remembered rule to session scope to keep the checkout unchanged.
