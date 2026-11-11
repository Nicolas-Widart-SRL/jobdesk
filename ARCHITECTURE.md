# Architecture

Jobdesk is a desktop view over an `ai-job-search` checkout. It follows domain-driven design, packaged by feature.

The checkout stays the source of truth, and slash commands write it. Jobdesk writes only the profile files, from the Profile screen's Save:

- one block at a time, at fixed paths under the root;
- never while a run works;
- only when the block on disk is unchanged, or the user chose Save mine over it.

Two more changes come from the Portals screen, and neither happens while a run works:

- a portal's switch writes the `enabled:` line in its `SKILL.md` frontmatter, the same way a profile save writes;
- Remove…, once the user answers its ask, moves a portal's folder under `.agents/skills` to the system Trash (the Recycle Bin on Windows). It never deletes one, and a built-in portal has no Remove.

Everything else Jobdesk writes stays in its data folder, with five exceptions:

- Restart to update replaces Jobdesk itself.
- The folder update merges the newest of the ai-job-search releases a Jobdesk update pins into a job search folder that keeps its base in `.jobdesk/`, and its Undo puts the last update back. See [The folder update](#the-folder-update).
- On macOS, a copy that runs from a `.app` bundle writes beside the bundle at each launch. A build with an update key makes and removes a `.Jobdesk.app.probe-<pid>` folder there, to learn whether an update may apply. Every build deletes the `.Jobdesk.app.update-`, `.old-` and `.probe-` folders that a Jobdesk which no longer runs left there. See [Where an update may apply](docs/architecture/trust.md#where-an-update-may-apply).
- First-run setup makes the job search folder the user chose, only when it is missing or empty. A folder that holds only Finder's `.DS_Store` file, and on macOS its `.localized` file, counts as empty, and setup deletes those files. It unpacks the newest ai-job-search release the signed feed pins into a hidden `.<name>.jobdesk-<pid>` folder beside it, after the zip matches the SHA-256 the feed pins for it, writes the zip and a record of its version and SHA-256 into that folder's `.jobdesk/`, gives that folder the empty folder's permissions, and renames it over the empty one. A release that holds a top `.jobdesk` entry, spelled in any letter case or Unicode composition, is refused whole. When setup opens, it deletes such folders that a Jobdesk which no longer runs left, but only beside the folder `onboarding.json` records, not beside other folders setup used before. Its Profile chapter then copies the PDFs the user adds into `documents/<kind>/` of that folder, as new files only, before `/setup` runs. See [First-run setup](docs/architecture/trust.md#first-run-setup).
- On macOS, first-run setup installs a missing helper in `~/.jobdesk`, the one folder of the home folder that Jobdesk owns: a standalone Python in `~/.jobdesk/python`, Bun in `~/.jobdesk/bun`, and TinyTeX with the TeX Live packages the CV and cover letter templates need in `~/.jobdesk/TinyTeX`. A helper counts as already here only when it works for ai-job-search: the first `python3` on the `PATH`, the one a task runs, is not a `/usr/bin` stub with no developer tools behind it and runs as 3.10 or newer, Bun runs, and `lualatex` and `xelatex` run and the `kpsewhich` beside each finds the file of every pinned package. For an engine that is a link, as in a folder of links into a TeX Live, the `kpsewhich` beside the file it links to counts first, then the one beside the link. A check that cannot tell, such as one that takes too long, counts as missing. `onboarding/domain/helpers.rs` pins each file by version, SHA-256 and size, at an address built from a fixed host and the version. Every file of a helper must match before any is unpacked. They are unpacked into a hidden `.<name>.jobdesk-<pid>` staging folder beside the helper's place, and renamed onto it. When the place is taken, setup checks the helper again: another Jobdesk may have installed it meanwhile, and then that install stays and the staging folder goes. Otherwise the place holds an earlier install of Jobdesk's own that does not work. It is renamed to `.<name>.jobdesk-<pid>-old`, then the staging folder is renamed onto the place and the old install is removed, its links but never what they point to. No install is ever half removed while another Jobdesk checks it. Setup never writes, replaces or removes anything elsewhere in the home folder, such as `~/.bun` or `~/Library/TinyTeX`, and in `~/.jobdesk` it touches only the helpers' places and their staging folders. When `~/.jobdesk` is a link, even to a folder, or anything but a real folder, an install fails before its download, and setup writes, removes and looks through nothing there. A failed install removes a `~/.jobdesk` it made, while it is still empty. The next open deletes the staging folders and the `-old` folders a Jobdesk which no longer runs left, only in a real `~/.jobdesk`. Nothing needs an administrator password.

How Save works is in [profile](docs/architecture/profile.md#save).

## Features

Each feature is a bounded context with its own language.

| Feature | Language | Reads |
| --- | --- | --- |
| `discovery` | job, lane, assessment, gate, signal, freelance terms, your terms, Jobs view filter | `job_scraper/seen_jobs.json`; the candidate profile and evaluation framework; settings' application layer, for the Jobs view filters |
| `tracking` | application, status, bucket, follow-up, outcome | `job_search_tracker.csv`; `documents/applications/*/outcome.md` |
| `insights` | search report, funnel, skill gap, study order | tracking's and discovery's application layers; the profile's topic counts; `upskill/report-*.md` |
| `intray` | card, decision, skip reason, queue | discovery's, tracking's, dossier's and runs' application layers; settings' application layer, for the queue rules; its own decisions file |
| `onboarding` | progress record, step, folder place, release download, download phase, staging folder, helper, artifact, install phase, outcome, document, document kind, copy stop, profile run, profile phase, batch, proposal, draft, progress line | its own `onboarding.json` and `onboarding/` in the data folder; the chosen folder, and the `.jobdesk/` setup writes into it, which keeps the release zip that matched the pin and its version and SHA-256 as the base of a later update of the folder; the ai-job-search release zip from GitHub; the pinned helper files from GitHub and the TeX Live historic archive; the helpers' places in `~/.jobdesk`; the PDFs the user adds, copied into `documents/<kind>/` of the folder; runs' application layer, for the `/setup` run, its start values, its replies and its prompts; profile's application layer, for the CV PDF the CV check shows and its pages; updates' application layer, for the source pin; workspace's check for an ai-job-search folder, its tool check and its run of a program beside a tool, wired in its `mod.rs` |
| `runs` | run, activity, prompt, decision, step rail, stop point, quick reply, step phrase, command catalog, usage (tasks only, from run logs) | the `claude` CLI stream; its own run logs; settings' application layer, for the run defaults and the cap; the saved API key, through the shell's key reader |
| `dossier` | archive, document, findings, page | `documents/applications/*`; compiled PDFs; `company_research/`; tracking's and runs' application layers |
| `profile` | topic, block, placeholder, draft, conflict, updated mark, CV document, staleness | the profile files that `TOPICS` in `profile/domain/topic.rs` names; the `## Candidate Profile` part of `CLAUDE.md`; the `.tex` and PDF files in `cv/` and their modification times; whether `.claude/commands/build-cv.md` exists; dossier's application layer, for the page cache; settings' application layer, for ages and sizes |
| `portals` | portal, manifest, origin, badge, quotes, search plan, term, community list, owners, add line | each `.agents/skills/*/SKILL.md`; `.claude/skills/job-scraper/search-queries.md`; discovery's application layer, for the jobs each portal found; the community list on GitHub, without sign-in, through the REST API for the latest commit and the file list (2 requests a read, of GitHub's 60 an hour) and `raw.githubusercontent.com` for each `SKILL.md` and `CODEOWNERS` at that commit, with no redirect followed; the list read last, saved in `portals/community.json` of the data folder |
| `workspace` | workspace root, tool check | the checkout folder; installed tools; file changes; profile's application layer; portals' application layer, for Setup's portals |
| `settings` | preference, page, search hit, run default, command override, spend cap, retention, sign-in method, API key, connection test, Coming up range, Later length, skip reason list, Jobs view filter, update switch | its own `settings.json` in the data folder; the system keychain, through a port in `domain` and adapters in `infrastructure`; through the `Snapshot`: runs' and tracking's application layers and discovery's terms; through the shell: workspace's readiness, the count of jobs the Jobs view hides, the data folder's sizes, the tray decision count and, for About, the update view from updates' application layer |
| `updates` | release, version, feed, download, signature, update key, install block, restart plan, apply failure, ai-job-search source pin, latest feed | `latest.json` on the latest GitHub release, and the build the feed names for this computer; the update key built into the app; its own `updates/` folder in the data folder |
| `folder_update` | base, release, merge action, conflict, side copy, upstream wins, plan, report, journal, backup, rollback, undo, gate, outcome, phase | the job search folder, its `.git` entry, and its `.jobdesk/`: the kept release zip and its record as the base, the journal of an update that runs, and the backup and record of the last update, with the note of the release Undo undid; the staging folders beside the folder; the pinned ai-job-search release zip from GitHub, downloaded into this Jobdesk's own folder of `onboarding/` in the data folder; updates' application layer, for the latest Jobdesk version and the source pins that the last update check read |

Screen names differ from feature names: `intray` is Today, `discovery` is Jobs, `tracking` is Applications, `dossier` is an application's dossier, `runs` is Tasks, `profile` is Profile, `portals` is Portals, `workspace` is Setup, `settings` is Settings, and `onboarding` is first run: the welcome, its Folder and Computer steps, the Profile chapter and the recap. Runtime status is drawn by workspace's UI and placed in Settings by the shell. `updates` has no screen: its UI draws the update group and the release notes, which the shell places at the right of the status bar, and Settings › About draws its Updates section from the same view.

Three modules are not features:

- `shared_kernel` holds the few value types every feature agrees on, such as `ModelChoice`, and these shared helpers:
  - the host `PATH` lookup that `runs` and `workspace` both need, with the helper folders first and macOS's developer stubs skipped, except that the tool check takes Python as a task runs `python3` (see [runs](docs/architecture/runs.md#the-path)), and `system_program` for the programs that ship with Windows;
  - `process_tree`, which starts every console program through `no_window` and stops a run with everything it started;
  - `replace_file`, whose `rename_over` replaces a file and on Windows tries a held file again a few times;
  - `own_folder`, the `<pid>` folder each running Jobdesk claims and locks in `updates/` and `onboarding/`, `reclaim`, which makes and locks a claimed folder again once it has gone, and the cleanup that leaves another running Jobdesk's folder alone;
  - `safe_save::replace_through_copy`, which replaces a text file in the checkout through a synced copy beside it and one plain rename, never retried, and removes the copies an interrupted save left. A profile save and a portal switch both write this way;
  - `https`, the one network client. `https_config` sets the shared rules: https only, at most 5 redirects, the system's certificates and the environment's proxy. `HttpsTransport` adds fixed timeouts and its own failure words that each feature maps to its own, and `TimedBody` reports a body that stalls as timed out. Only `infrastructure` and a feature's wiring in its root `mod.rs` use it: the update feed and builds, first-run setup's downloads and a folder update's release zip go through `HttpsTransport`, and the community portal list starts from `https_config` and follows no redirect;
  - `download`, the `HttpsFetcher` that downloads a file over `https` into the running Jobdesk's own `<pid>` folder, refuses more bytes than the caller allows, and deletes the part file whatever ends it short;
  - `release_zip`, the checks of an ai-job-search release zip: its SHA-256 first, then every entry before any is read. A path that could leave the top folder, a link or other special file, two names one disk takes for the same, a top `.jobdesk` entry, or more than 100 MB or 20,000 entries refuses the whole zip. `read_verified` gives every file in path order with the top folder stripped. The folder update reads both its base and the new release through it, and setup writes the same checked entries into its staging folder;
  - `file_text`, the helpers for a byte order mark and for CRLF and lone CR line endings;
  - `visible_text`, the filter that drops hidden characters from untrusted text;
  - `workspace_path::within_workspace`, the text check that keeps a path read from the checkout inside it, which `dossier` and `profile` share;
  - `markdown::options()`, the one Markdown option set that the viewer and the closing-question parser share;
  - `installed_source`, the release a job search folder was set up from, kept in the folder's `.jobdesk/` as `source.zip` and `source.json`, on macOS readable only by the user. A read refuses a link and a zip larger than setup downloads, and gives nothing when the record's version or hash breaks the pin's rule or the zip's SHA-256 is not the one its record holds. The record catches damage, not a change by something that can write the folder;
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

Jobs, Applications, Insights, Profile, Portals, Setup and Settings › Runtime status learn both through `ScreenState`: whether their source is still being read, failed or ready, and which `Source`s failed, in `unread`. They draw the alert under their head. Only Jobs, Applications and Portals get `Load::Failed`, when their own source fails. The others get only `Reading` or `Ready`: Insights, Setup and Runtime status check `unread` to hide the parts a failed source feeds, and Profile hides a failed file's topics through its own `ProfileView`, and its CV documents through `CvShelf::Unread`. Today gets the alert and the missing sources through a context of its own, and draws them under its head too.

Tasks, the launcher and Dossier take no `ScreenState`: the shell draws the alert above their head. For Dossier the shell also draws the loading view and, when the tracker failed, the note that the application is hidden.

### The first window

First run's welcome, Folder and Computer steps show in a small window of 720x560 points, so they read as an installer, not an app to learn. The window grows once, at the hand-over, when the Profile chapter opens: to 1280x820, the app's own default size, or to the screen when that is smaller, never below the app's smallest, 1040x640. It keeps that size through the recap and into the app, because a 13-question sheet needs about 1100x700, and one grow is calmer than two.

- The shell opens first run only on macOS, only for a launch with no folder asked for or remembered, and only with an absolute home folder and a processor setup has helper files for. Until the recap opens, the shell does not remember the setup folder, so each such launch opens first run again, on Welcome back once the folder is made, with Finish my profile once the Computer step is done. Opening another folder clears that, and the other folder is remembered. Windows shows the welcome and the folder picker.
- On its first frame the window lowers its smallest size, then takes 720x560, whatever size eframe restored. First run has no menu until the recap: only the brand shows above it.
- The download, the helper check and the installs run on worker threads. `Onboarding::poll` takes their news each frame, and a worker wakes the window when it has some. In the Profile chapter, reading and copying the documents run on workers too, and `ProfileRun::poll` runs each frame after `RunDesk::poll`: it takes their news, starts `/setup` once the copy ends, and reads the chapter's phase from the run, never from the disk. See [runs](docs/architecture/runs.md#the-first-run-profile-run).
- The status bar keeps Settings, and says "Claude Code is installed. Jobdesk uses it for every task." once the tool check finds `claude`. It never mentions an API key on these screens. Settings needs 1040x640, so while it shows over the welcome, Folder or Computer step the window takes the app's size and smallest as the hand-over does, and closing it gives back 720x560 around the same centre. Over the Profile chapter and the recap the window already has the app's size, and Settings changes nothing.
- Setup hands over whenever a folder opens: once the Computer step is done, from I already have an ai-job-search folder, or from Settings › Change folder. The folder opens, on the Profile chapter after the Computer step and on Today otherwise, the window grows to 1280x820, or to the screen when it is smaller, never below 1040x640, around the centre it had, and the tool check runs again, so Setup and Runtime status find the helpers setup installed. It runs once more when the shell joins a stopped worker, since an install past its last cancel check can still put its helper in place after the hand-over's check. From Settings › Change folder, Settings stays open with Today behind it, until Back to app or Esc.
- The hand-over never waits for setup's worker. It sets the worker's cancel flag, and the shell keeps the worker's thread and joins it on the first frame after it has finished. A download or an install ends short at its next check and cleans up as Cancel does, but a read, a connection or a helper check under way ends first, which a stalled network or a hung TeX check can stretch to tens of seconds. The worker writes only its own folder, that folder's staging folder, `~/.jobdesk`, and its downloads and `onboarding.json` in the data folder, never the folder that opened. One stopped after the release took its folder's place may still record the Folder step as done. Quitting does not wait for it either, as for the app's other worker threads: the process ends them. A worker cut off this way can leave part files in `<data>/onboarding/<pid>/`, a hidden staging folder beside the folder it made, and staging folders in `~/.jobdesk`. Only setup opening clears these, and setup does not open again once a folder is remembered, so they can stay for good. A part file can be as large as a whole download, TinyTeX's included. They are safe to delete.
- Each size change keeps the window's centre, title bar included, so opening and closing Settings over first run leaves the window where it was.

### The folder update

Each Jobdesk release pins 1 to 5 compatible ai-job-search releases, and `folder_update` merges the newest into the open job search folder. It is not a run: it starts no `claude`, writes no run log and is not counted in Usage. Its checks and residuals are in [trust](docs/architecture/trust.md#folder-update), and what the user sees is in [the guide](docs/guide.md#updating-your-job-search-folder).

- At each open of a folder, a launch's, Change folder's or first-run setup's hand-over, `FolderUpdate::opened` makes its rollback due. `follow` runs it on a worker once the folder may be written. It rolls back an update that stopped midway, and finds an Undo that did not finish. A rollback that another Jobdesk window keeps out by having the folder open ends `RollbackFailed(OpenElsewhere)`, which the notice says, and `follow` looks every `HOLD_RETRY` whether the folder can be held alone, starting nothing else meanwhile, and runs the rollback once it can.
- Then `follow` starts one update pass for each check that read the feed, once per folder and answer in a launch. Every such check counts, even one that reads the same as the last, so a failed pass is tried again after the next check. With the automatic check off, only Check now reads the feed.
- A pass first rolls back an update that stopped midway, as the open does, and ends there when it did. Once a rollback of a folder has run or failed, at an open or in a pass, no pass starts for that folder for the rest of the launch, so its notice stays, and so does the backup that holds the originals of any file it kept. One that another Jobdesk kept out ran nothing, and holds nothing back. An update that fails and whose own rollback keeps files holds the folder back the same way. Then it asks the gate in this order, and stops at the first that fails: no Undo is left unfinished, the folder is there, it holds no `.git` entry of any kind, its base reads, this Jobdesk is the latest the feed names, the feed holds a signed pin, the newest pin is newer than the base, and that pin is not the release Undo noted. A gate that fails says nothing in the status bar. Settings › About says why for a folder under git, a folder with no base and a base that does not read.
- `FolderUpdate::support_line` gives Settings › About one more line, under that one, when the base's version is not among the signed pins of the last feed read: the folder's files are from a version Jobdesk no longer supports. It says nothing with no record of the base that reads, no feed read yet or no signed pin, and nothing for a folder under git, since git updates its files and leaves the record as it was. The worker reads the version from `.jobdesk/source.json` alone, with `BaseReader::record`, never the kept zip: at the open's look, and again after each pass and each Undo. A pass or an Undo whose worker panicked leaves no version, so the line says nothing until the next open's look.
- The pass downloads the pinned zip into this Jobdesk's own folder of `<data>/onboarding/`, takes its bytes and deletes the part file. The update gets the base as the gate read it, and once it holds the folder alone it ends as up to date, skipped or busy, changing nothing, when the base or Undo's note moved since. `domain::merge::decide` decides each file of either release from three copies: the base, the folder's and the new release's.
- Every Jobdesk holds the lock `.jobdesk/.open` of the folder it has open, shared, from `opened` until the next open, and the update, its Undo and its rollback each hold it alone, so none runs while another Jobdesk has the folder open. The update writes every new file into a hidden `.<name>.jobdesk-update-<pid>` staging folder beside the job search folder, and the original of each file it replaces or deletes into a backup there. Only then does it touch the folder: the backup takes the last update's place in `.jobdesk/backup/`, the journal `.jobdesk/update.json` is written, and each staged file is renamed into place, the new base last. The journal then becomes the backup's record. A failure before the first rename leaves the folder as it was, and one after it rolls the folder back from the journal.
- Undo is offered only for the update that ended in this launch, until a task starts, the notice is dismissed or another folder opens. An Undo that did not finish is offered at each open and for the whole launch, since finishing it keeps a task's edits. No update runs until it finishes.
- The shell's `folder_writable` lets the update, its Undo and its rollback start only when no Claude session is open, even one only waiting on a reply, first-run setup has handed over and its worker has ended, its Profile chapter is not copying documents into the folder, and the window is not closing for Restart to update. While `is_working` holds, the desk starts no task and sends no reply, the Profile chapter's Read my documents and Pick up where it stopped wait, and so do Profile's Save, the portal switch and Remove, Test connection, Restart to update and Change folder wait. See [runs](docs/architecture/runs.md#while-the-folder-updates).
- Quitting does not wait for it. A worker that panics ends as a disk failure with nothing to undo. Either way, the next open rolls back an update whose journal is still there. A download cut off by a quit leaves its part file in `<data>/onboarding/<pid>/`. No launch clears `onboarding/`, since another Jobdesk may be making its own folder there before it has locked it, so only setup opening, on macOS, clears the part file, and it is safe to delete. Setup opening has that same gap: it can delete the folder another running Jobdesk's folder update has just made, before that Jobdesk locks it. The download under way then fails with nothing changed, and before each download the folder update makes and locks its own folder again if it has gone, through `own_folder::reclaim`, so the next check downloads as usual.

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
- Changing how Jobdesk checks, downloads, verifies or applies its own updates: read [trust](docs/architecture/trust.md#updates).
- Changing first-run setup's folder, download, helpers, documents or progress record: read [trust](docs/architecture/trust.md#first-run-setup).
- Changing how first run starts `/setup`, reads its messages or replies to it: read [runs](docs/architecture/runs.md#the-first-run-profile-run).
- Changing the folder update, its merge, journal, backup or Undo: read [trust](docs/architecture/trust.md#folder-update).

## Trust

Posting text, notes and reports are third-party or generated data. Screens render them as plain text, and a link opens only on click and only for `http` or `https`.
Claude's replies and prep packs render as Markdown with no image loading.
The API key goes only from the system keychain, read once per launch, until a read succeeds, and held in memory, into the environment of a `claude` process, and the whole key is masked in what that process prints.
By default `RunDesk` allows tool permissions and records them in the activity, and Settings › Permissions turns prompts on.
An update downloads only over https from addresses built from its version, and applies only when its signature checks against the key built into Jobdesk.
First-run setup unpacks only into a new or empty folder, and installs only helper files that match the SHA-256 Jobdesk pins.
Its Profile chapter renders `/setup`'s messages as Markdown with no image loading and no link that opens, and builds every reply from fixed text, the user's words or Claude's numbers.
The folder update applies only the ai-job-search release whose SHA-256 the update key signed in the feed, keeps the user's file in a conflict, but for the two safety files, and shows every path as plain text with hidden characters removed.
The community portal list is read over https from GitHub, without sign-in, and its words render on one line with hidden characters removed. View code ↗ and Install… are built from a validated country and folder name, never from the list's text.

Jobdesk itself makes four kinds of network request, none of them a run: the update feed and its builds, the community portal list and the folder update's release zip, all from GitHub, and first-run setup's downloads, the ai-job-search release from GitHub and the helpers from GitHub and the TeX Live historic archive. Every other request comes from a `claude` process Jobdesk starts: a task's, which is a run, and Test connection's, which is not.

Changing how untrusted text renders, a path check, the API key's path, tool permissions, first-run setup, updates or the folder update: read [trust](docs/architecture/trust.md). It also lists the accepted residuals.
