# Jobdesk

Rust and egui desktop app for macOS and Windows: a GUI over the `ai-job-search` slash commands. It holds no workflow logic. Every action runs an unchanged slash command through the local `claude` CLI.

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
- Code is hand-formatted with no rustfmt config, so match the surrounding lines and skip `cargo fmt`.
- CI fails on any clippy warning: run `scripts/check.sh` before you commit.
- Commit subjects follow Conventional Commits, because CI rejects others and git-cliff writes `CHANGELOG.md` from them.

## Layout

- `src/<feature>/` splits into `domain`, `application`, `infrastructure` and `ui`.
- `domain` and `application` stay free of egui, the file system and serialization crates.
- Only `application` calls across features, and only into another feature's `application`.
- A new feature joins `FEATURES` in `tests/architecture.rs`, which checks only the features listed.
- That test reads raw text: use `crate::` paths, and keep banned words like `egui` out of comments too. Each root `mod.rs` and all code after the first `#[cfg(test)]` are exempt.

## Hard rules

- Slash commands are the only writers of the checkout. Jobdesk writes only to its data folder.
- One run works at a time, because two commands could overwrite each other's files.
- Card actions never start a run. Y opens the launcher, and the user starts the run there.
- Tool permissions are allowed automatically and noted in the activity, until a setting can turn prompts back on. `AskUserQuestion` always reaches the user, and remembered rules stay session-scoped. See [Trust](ARCHITECTURE.md#trust).
- Authentication stays with the `claude` CLI. Jobdesk offers no login and handles no credentials.
- Postings, notes and reports are untrusted: render them as [Trust](ARCHITECTURE.md#trust) says.
- The UI reads the `Snapshot`, never the disk.
- `tests/sample_workspace` holds fictional data only, because snapshots are committed.

## Team

- The main session runs as the `lead` agent in `.claude/agents/lead.md`, and it writes no code. Every code change, even a one-line fix, goes to a builder through the `ship` skill.

## Details

- [ARCHITECTURE.md](ARCHITECTURE.md): read before adding a feature or changing layers, runs or trust handling.
- [docs/auth-review.md](docs/auth-review.md): read before changing authentication, naming or distribution.
- [README.md](README.md): user-facing behavior and packaging setup.
