# Jobdesk

Rust and egui desktop app for macOS and Windows: a GUI over the `ai-job-search` slash commands. It holds no workflow logic. Every task runs through the local `claude` CLI, nearly always as an unchanged slash command. Two things are not that: Install… on a community portal, whose plain-words task Jobdesk writes from validated parts, and the portal On/Off switch and Remove, which are Jobdesk's own writes, not tasks. First-run setup on macOS is not a task either: it fills only a new or empty folder. Nor is the folder update, which merges the ai-job-search release the signed update feed pins into the folder.

## Commands

```bash
cargo run -- --workspace /path/to/ai-job-search
cargo test                                              # all except paid live tests
UPDATE_SNAPSHOTS=1 cargo test --test screens --test ui_kit_gallery  # after an intended visual change
cargo test --test live_claude -- --ignored --nocapture  # real session, costs a few cents
cargo packager --release --formats app,dmg              # nsis on Windows
```

- Refresh snapshots on macOS only, so every baseline comes from one renderer.
- `UPDATE_SNAPSHOTS=1` skips changes under the 200-pixel tolerance. Use `UPDATE_SNAPSHOTS=force` to rewrite every baseline.
- `rustfmt.toml` sets the style: run `cargo fmt` before you commit. CI and `scripts/check.sh` fail on unformatted code.
- CI fails on any clippy warning: run `scripts/check.sh` before you commit.
- Commit subjects follow Conventional Commits, because CI rejects others and git-cliff writes `CHANGELOG.md` from them.

## Logs

- Run logs sit in the app's data folder: `~/Library/Application Support/Jobdesk/runs/` on macOS, `%APPDATA%\Jobdesk\data\runs\` on Windows. `intray/` beside `runs/` holds the tray decisions. `settings.json` beside them holds the preferences. `portals/community.json` holds the community portal list read last from GitHub. `updates/` holds Jobdesk's own downloads, one folder for each Jobdesk that started a download, which a later launch removes once that Jobdesk has closed, and an `apply-failed` note that the next launch reads once.
- On macOS, `onboarding.json` beside them is first-run setup's progress record: the chosen folder, the steps done, the ai-job-search version being set up and the day setup stopped. It is removed once the Computer step ends.
- `onboarding/` beside them holds the downloads of first-run setup, on macOS, and of the folder update, on macOS and Windows, one folder for each running Jobdesk. Setup removes the folder of a Jobdesk that has closed when it next opens, so on Windows a download cut off by a quit stays.
- `.jobdesk/` sits in the job search folder itself, not in the data folder, so `JOBDESK_DATA_DIR` does not move it. It holds `source.zip` and `source.json`, the ai-job-search release the folder was set up from or last updated to and its version and SHA-256: the base of the next folder update. `backup/` holds the last update only: the originals of the files it replaced or deleted, numbered, in `backup/files/`, its record `backup/update.json`, and `backup/skipped.json`, the release Undo undid. `update.json` is there only while an update runs, or after one stopped midway until the next open rolls it back. `update-stopped.json` is a stopped update's record that the rollback could not read or finish, set aside. `.open` is the lock every Jobdesk that has the folder open holds shared, and an update, an Undo or a rollback holds alone. An update stages in a hidden `.<name>.jobdesk-update-<pid>` folder beside the job search folder.
- The helpers first-run setup installs sit in `~/.jobdesk/python`, `~/.jobdesk/bun` and `~/.jobdesk/TinyTeX`, outside the data folder, so `JOBDESK_DATA_DIR` does not move them.
- `JOBDESK_DATA_DIR` set to an absolute path moves the whole data folder there, eframe's `app.ron` included, and the copy that Restart to update opens keeps it. An empty or relative value is ignored. Point it only at a fresh folder kept for Jobdesk, because Jobdesk empties its `updates/`, prunes its `runs/` and rewrites its `intray/decisions.json` and `portals/community.json`. It does not move the API key's keychain entry or the `claude` CLI's own sign-in.
- Each run has one `<run id>.jsonl`: a `jobdesk_meta` line, then the raw stream-json. A reopened run adds a `jobdesk_resume` line before its `jobdesk_user` line, and a stop adds a `jobdesk_stop` line. `ls -t` lists the newest first.
- The app writes no log of its own. A panic prints only to the terminal that started the app.
- These files, `.jobdesk/` and its backup included, hold the user's real job search. Read them only when asked, and never copy a value into a test, a memory or a commit.

## Layout

- `src/<feature>/` splits into `domain`, `application`, `infrastructure` and `ui`.
- `domain` and `application` stay free of egui, the file system and serialization crates.
- Only `application` calls across features, and only into another feature's `application`.
- A new feature joins `FEATURES` in `tests/architecture.rs`, which checks only the features listed.
- That test reads raw text: use `crate::` paths, and keep banned words like `egui` out of comments too. Each root `mod.rs` and all code after the first `#[cfg(test)]` are exempt.

## Hard rules

- One run works at a time, because two commands could overwrite each other's files.
- An update applies only from Restart to update, and never while a Claude session is open, even one only waiting on a reply. Checking and downloading an update are not runs: they start no `claude`, write no run log and are not counted in Usage. See [Updates](docs/architecture/trust.md#updates).
- Card actions never start a run. Y opens the launcher, and the user starts the run there.
- Tool permissions are allowed and noted in the activity by default, and Settings › Permissions turns prompts on. `AskUserQuestion` always reaches the user, and remembered rules stay session-scoped. See [Tool permissions](docs/architecture/trust.md#tool-permissions).
- Authentication stays with the `claude` CLI, and Jobdesk offers no login. The one exception is an optional Anthropic Console API key the user pastes in Settings › Providers. It is stored only in the system keychain, read from it once per launch, until a read succeeds, and then held in process memory for the session, and passed only in the environment of the `claude` process, never when an inherited `ANTHROPIC_BASE_URL` is neither `https` nor `http` to this machine without a proxy variable. It never goes in a file, a log, a command line, a panic or a `Debug` impl, and a saved key is never on screen in full. Where the whole key shows up in what a task's process prints, the run log, the activity and the failure get its mask. The one exception is the key being typed, while the user presses Show.
- Apart from first-run setup, which fills only a new or empty folder, Jobdesk writes the checkout only from Profile's Save, from the Portals screen (a portal's switch and Remove), and through the folder update and its Undo. Each waits while a task runs, and Save, the switch and Remove also wait while the folder update works. Remove moves the portal's folder to the Trash (the Recycle Bin on Windows) after the user confirms, and never deletes it.
- The folder update comes only from an update check, automatic or Check now, even when no newer Jobdesk is out. It applies the newest ai-job-search release the signed feed pins, checked by SHA-256, only when this Jobdesk is the feed's latest, that pin is newer than the folder's base and the folder has no `.git`. It never runs while a task or a Claude session is open, and every task and reply, Profile's Save, the portal changes, Test connection, Restart to update and Change folder wait while it works. A conflict keeps the user's file and saves the new copy beside it, except in the two safety files, `.claude/settings.json` and `tools/security_guards.py`. See [Folder update](docs/architecture/trust.md#folder-update).
- Reading the community portal list from GitHub is not a run: it starts no `claude`, writes no run log and is not counted in Usage. See [The community portal list](docs/architecture/trust.md#the-community-portal-list).
- Test connection is not a run. It uses no tools, runs in an empty temporary folder, is capped at $0.01, writes no run log, is not counted in Usage, and is disabled while a task runs.
- Postings, notes and reports are untrusted: render them as [Trust](docs/architecture/trust.md#rendering-untrusted-text) says.
- The UI reads the `Snapshot`, never the disk.
- `tests/sample_workspace` holds fictional data only, because snapshots are committed.
- An agent-desktop session opens a copy of the sample workspace with a fresh `JOBDESK_DATA_DIR`, and starts no run.

## Team

- The main session in the main checkout runs as the `lead` agent in `.claude/agents/lead.md`, set up as [CONTRIBUTING.md](CONTRIBUTING.md#agent-team) says, and it writes no code or docs. A session in an agent worktree is not the lead, unless its branch is older than this setup: see the same section. Every code change, even a one-line fix, goes to a builder.
  Every docs-only change, one that touches only Markdown in `README.md`, `AGENTS.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md` or `docs/`, goes to `docs-writer`.
- A small, low-risk change goes through the `quick-fix` skill. Everything else goes through the `ship` skill.
- The main checkout stays on `main` and clean. Every edit happens in an agent worktree, except the lead's plan files in `.claude/plans/`.
- `reviewer` reviews each risky step and, in `ship`, the whole change before the pull request. CodeRabbit reviews the pull request, and the lead answers it through `ship`.
- An agent drives the app only when its brief says to drive the app, and then follows the `drive-app` skill (agent-desktop, macOS only)
  as `main` holds it: `git show origin/main:.claude/skills/drive-app/SKILL.md`. Before `main` holds it, the brief must name a file path and say, in the lead's words, that the file is reviewed.
  Then that file is the skill, followed instead of the `origin/main` copy.
- OpenSpec (`openspec/`, `/opsx:*`) runs through the same flow: a propose, update, archive or sync is one builder step in a worktree, `/opsx:apply` is a `ship` change,
  and the lead runs only read-only `openspec` subcommands. See `.claude/agents/lead.md`.

## Details

- [ARCHITECTURE.md](ARCHITECTURE.md): read before adding a feature or changing layers. Its Runtime section points to the file for each other topic.
- [docs/architecture/runs.md](docs/architecture/runs.md): read before changing how a task starts, stops, signs in or waits on the user.
- [docs/architecture/trust.md](docs/architecture/trust.md): read before changing how untrusted text renders, the API key's path, tool permissions or the folder update.
- [README.md](README.md): the landing page and install guide.
- [docs/guide.md](docs/guide.md): user-facing behavior, screen by screen.
- [CONTRIBUTING.md](CONTRIBUTING.md): running, packaging, releasing and testing.
