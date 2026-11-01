<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/jobdesk-banner-dark.png">
    <img src="assets/brand/jobdesk-banner-light.png" alt="Jobdesk" width="640">
  </picture>
</h1>

Native macOS and Windows view over an [ai-job-search](https://github.com/ai-job-search-dev/ai-job-search) checkout, built with Rust and egui.

Today is the home screen: one queue of decisions, one card at a time, with paused runs first, then deadlines, then the rest. The day stamp counts what you decided today, and Coming up lists the next two weeks' deadlines, with a button to bring back what you set aside. Y opens the suggested task, L sets the card aside until tomorrow (Coming up shows how many, and its button brings them back to the front, after any paused run), N skips, U undoes the last decision. Until your next one, and only that day, it waits at the top of Done today, where a skip also takes its optional reason. A followed card's note names the task it opened, for example "Task opened: draft application". The card's actions stay pinned under it, and Show the command reveals the command line a task would run. Y opens the slash command in the launcher, and nothing runs until you start it. When nothing is left to decide, Today offers Find new jobs…. It shows a loading view while your files first load, and a welcome with three steps until you choose a folder. Permission, question, guard and Your turn cards have no Y action. Skip reasons stay in the app's data folder.

Click anywhere on a job, an application or a run to open it. On Jobs, Up and Down move to the next job, and Tab stops once on each row. A cut title shows in full under the pointer. Below 1100 pixels wide, the table leaves out Found on, which the detail still shows. Text in Claude's messages and questions, your replies, error alerts, command lines, company research, archived postings and follow-up drafts can be selected and copied. Copy says Copied.

Today, Jobs, Applications and Insights update live as commands change your files. A file that fails to load raises one alert that names what failed and what still works, and marks its tab. The screens it feeds say their content is hidden until the file is fixed, never that it is empty. The Tasks screen starts any slash command through your local `claude` CLI and shows its progress, questions, cost and history. Jobdesk allows tool permissions on its own for now and lists each one in the activity. A run reads like a report: Claude's messages come first, and each step is one plain sentence, with its actions a click away. A band at the top says where the run is, whose turn it is and what comes next. When Claude waits on you, the Tasks tab counts it, Recent runs tags the run Your turn, Today deals a card that opens the task, and Claude's question sits above the reply box, with one-click replies at known stop points such as /apply's proceed with drafting. They wait while you write a reply of your own. When Claude finishes a turn without asking anything, the band says it is done for now and the reply box stays under the story. End session asks first, and Keep it open is the default. New task (Cmd+K, Ctrl+K on Windows) opens the command launcher, even while you type in a field. In the launcher, Cmd+Enter (Ctrl+Enter on Windows) runs the task, and Esc goes back.

Setup shows whether your profile, tools, portals, templates and sync are ready. Optional tools never count against the ready total. The status bar says "Claude is ready", or "Claude Code is not installed". Before you choose a folder it says that Claude is installed, and that Jobdesk uses its sign-in and stores no passwords.

Open an application from Applications for its dossier: compiled PDFs, verification results, keyword coverage, evaluation, archived posting, prep packs, outcome notes and follow-up drafts. Verification, keywords and evaluation exist only in `/apply` output, so they appear for applications run through Jobdesk.

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
