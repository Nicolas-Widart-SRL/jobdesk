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
| `runs` | run, activity, prompt, decision, step rail, stop point, quick reply, step phrase, command catalog | the `claude` CLI stream, its own run logs |
| `dossier` | archive, document, findings, page | `documents/applications/*`, compiled PDFs, `company_research/`, tracking's and runs' application layers |
| `workspace` | workspace root, profile state, portal, tool check | the checkout folder, installed tools, file changes |

Screen names differ from feature names: `intray` is Today, `tracking` is Applications, `runs` is Tasks, and `workspace` is Setup.

`shared_kernel` holds the few value types every feature agrees on, plus the host `PATH` lookup that `runs` and `workspace` both need, `visible_text`, the filter that drops hidden characters from untrusted text, and `markdown::options()`, the one Markdown option set that the viewer and the closing-question parser share. `ui_kit` is the single owner of
the Ledger design system: colour, spacing, radius, stroke and type tokens, plus every widget that paints them. Colour is for job status, plus one stamp ink for what waits on the user. The window is a gridded desk in both themes, and anything read in rows sits on paper. It knows no feature, and screens only arrange its widgets, which `tests/architecture.rs` enforces. `tests/ui_kit_gallery.rs` snapshots every component in both themes. `shell` is the composition root: it loads every feature into one
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
others keep working. Its tab carries a mark, and an alert names what failed and what still works.

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

By default `RunDesk` allows every tool permission itself and records each one as an allowance row in the activity, so a run never stalls on a prompt nobody sees. Each step folds its allowances behind its toggle. Other notices, such as why a run failed or stopped or which action was denied, show inline in the story as plain text with hidden characters removed, one row per non-blank line. The story keeps a notice's last 8 lines, because a stderr tail ends on its error, and shows the whole on hover. This trades away the prompt as a check against injected posting text. Postings rely on `posting_guard` in the workspace instead. Questions from `AskUserQuestion` always reach the user. With `ask_permission` on, which a setting will expose later, every `can_use_tool` request reaches the user with its input, marked as shortened past 20 KB. The CLI suggests saving allow rules to project settings, so `stream_json` rewrites every remembered rule to session scope to keep the checkout unchanged.

A pending prompt stays until it is answered or withdrawn, or the process ends. A turn's end does not clear it, because a background subagent can still be waiting on it. Background subagents also keep the run busy after its turn ends, because they can still write files.
