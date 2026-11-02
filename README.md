<p align="center">
  <img src="assets/brand/jobdesk-social.png" alt="Jobdesk: your job search, one decision at a time. A desktop app for the ai-job-search commands." width="830">
</p>

<h1 align="center">Jobdesk</h1>

Jobdesk is a native macOS and Windows app, built in Rust, over the [ai-job-search](https://github.com/ai-job-search-dev/ai-job-search) slash commands. It runs them through your own Claude Code CLI.

<p align="center">
  <a href="https://github.com/Nicolas-Widart-SRL/jobdesk/releases/latest"><img src="https://img.shields.io/badge/Download_for_macOS-12263f?style=for-the-badge" alt="Download for macOS"></a>
  <a href="https://github.com/Nicolas-Widart-SRL/jobdesk/releases/latest"><img src="https://img.shields.io/badge/Download_for_Windows-12263f?style=for-the-badge" alt="Download for Windows"></a>
  <br>
  <a href="https://github.com/Nicolas-Widart-SRL/jobdesk/releases">All releases</a>
</p>

## What it does

- **Today** deals your decisions one card at a time: Y opens the suggested task, L sets it aside until tomorrow, N skips, U undoes.
- **Jobs, Applications and Insights** update live as commands change your files.
- **Tasks** starts any slash command and shows its progress, Claude's questions, the cost and the history.
- **Application dossiers** gather the PDFs, verification, keyword coverage, evaluation, prep packs and follow-ups of each application.
- **Nothing runs until you start it**, and Jobdesk never writes to your checkout.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="tests/snapshots/intray_dark.png">
    <img src="tests/snapshots/intray_light.png" alt="The Today screen, with one decision card and the Coming up deadlines." width="800">
  </picture>
</p>

## Install

You need:

- the [Claude Code](https://code.claude.com/docs/en/overview) CLI, installed and signed in;
- an [ai-job-search](https://github.com/ai-job-search-dev/ai-job-search) checkout.

Download the installer for your system from the [latest release](https://github.com/Nicolas-Widart-SRL/jobdesk/releases/latest):

- **macOS** (Apple silicon): open `Jobdesk-vX.Y.Z-macos-arm64.dmg` and drag Jobdesk to Applications.
- **Windows**: run `Jobdesk-vX.Y.Z-windows-x64-setup.exe`.

The builds are not signed yet, so both systems warn on first open:

- **macOS** blocks the first open. Try to open Jobdesk once, then go to System Settings, Privacy & Security, and click Open Anyway. On macOS 15 and later, right-click Open no longer bypasses this.
- **Windows** SmartScreen shows a warning. Click More info, then Run anyway.

On first launch, choose your ai-job-search folder.

## Privacy and trust

Slash commands are the only writers of your checkout. By default Jobdesk allows their tool permissions without asking and lists each one in the activity, so a posting written to trick Claude could get a command run before you see it. Settings › Permissions turns prompts on. The workspace checks postings with `posting_guard` before scoring them. Run logs, which include the text of your runs, and your decisions stay in the app's data folder and are readable only by your user. Jobdesk stores no credentials.

## Authentication

Jobdesk runs your installed Claude Code CLI and uses the authentication you set up there. It is not made or endorsed by Anthropic. Read [docs/auth-review.md](docs/auth-review.md) before distributing it.

## More

- [docs/guide.md](docs/guide.md): every screen in detail.
- [CONTRIBUTING.md](CONTRIBUTING.md): build, package, release and test.
- [ARCHITECTURE.md](ARCHITECTURE.md): how the code is structured.
- [CHANGELOG.md](CHANGELOG.md): what changed in each release.
