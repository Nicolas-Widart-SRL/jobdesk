<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/jobdesk-banner-dark.png">
    <img src="assets/brand/jobdesk-banner-light.png" alt="Jobdesk" width="640">
  </picture>
</h1>

Native macOS and Windows view over an [ai-job-search](https://github.com/ai-job-search-dev/ai-job-search) checkout, built with Rust and egui.

In-tray is the home screen: one queue of decisions, one card at a time, with paused runs first, then deadlines, then the rest. Y follows the suggestion, L sets the card aside until tomorrow, N skips, U undoes. Y opens the slash command in the launcher, and nothing runs until you start it. Permission, question and guard cards have no Y action. Skip reasons stay in the app's data folder.

In-tray, Jobs, Tracker and Insights update live as commands change your files. The Runs screen starts any slash command through your local `claude` CLI and shows its progress, questions, cost and history. Jobdesk allows tool permissions on its own for now and lists each one in the activity. When Claude finishes a step, reply to continue or press End session. Ask or run (Cmd+K, Ctrl+K on Windows) opens the command launcher, even while you type in a field.

Workspace shows whether your profile, tools, portals, templates and sync are ready. Optional tools never count against the ready total. The status bar names your Claude CLI version, or says when `claude` was not found.

Open an application from Tracker for its dossier: compiled PDFs, verification results, keyword coverage, evaluation, archived posting, prep packs, outcome notes and follow-up drafts. Verification, keywords and evaluation exist only in `/apply` output, so they appear for applications run through Jobdesk.

Jobdesk itself never writes to your checkout. Slash commands stay the only writers. Because Jobdesk allows their tool requests without asking, a posting written to trick Claude could get a command run before you see it. The workspace checks postings with `posting_guard` before scoring them. It stores no credentials and uses the login of your installed Claude CLI. Run logs, which include the text of your runs, and your tray decisions are kept in the app's data folder and are readable only by your user.

Only one run works at a time, because two commands could overwrite each other's files. A session waiting for your reply does not block a new run.

## Run

Needs Rust 1.95 or newer.

```bash
cargo run -- --workspace /path/to/ai-job-search
```

You can also set `JOBDESK_WORKSPACE`, or choose the folder in the app. It reopens the last folder.

## Package

```bash
cargo install cargo-packager --locked --version 0.11.8
```

```bash
cargo packager --release --formats app,dmg
```

Output lands in `target/packages`. On Windows use `--formats nsis`. Run the Installers workflow from the Actions tab to build both installers for a branch.

These builds are not signed with a developer identity. macOS and Windows warn on other machines until they are signed and notarized. The app icons live in `assets/brand/`.

## Release

1. Run the Release workflow from the Actions tab on `main`. It picks the next version from the Conventional Commits since the last tag, and opens a `release/vX.Y.Z` pull request with the new `CHANGELOG.md` section and version.
2. Review the changelog on that branch, then merge the pull request.
3. The Tag release workflow builds both installers on the merge commit, then tags it and publishes the GitHub release with the changelog section as its notes.

The Release workflow needs the `RELEASE_TOKEN` secret: a fine-grained personal access token with Contents and Pull requests read and write on this repository. A pull request opened with the default token would not start CI.

`scripts/release.sh` prepares the same release branch locally. It needs [git-cliff](https://git-cliff.org) and `jq`.

## Authentication

Jobdesk runs your installed Claude Code CLI and uses the authentication you set up there. It is not made or endorsed by Anthropic. Read [docs/auth-review.md](docs/auth-review.md) before distributing it.

## Test

```bash
cargo test
```

A real Claude session runs only on request and costs a few cents:

```bash
cargo test --test live_claude -- --ignored --nocapture
```

Screen tests render every screen in both themes from `tests/sample_workspace`, which holds fictional data. After an intended visual change, refresh them:

```bash
UPDATE_SNAPSHOTS=1 cargo test --test screens
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for the structure and the rules `tests/architecture.rs` enforces.
