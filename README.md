<p align="center">
  <img src="assets/brand/jobdesk-social.png" alt="Jobdesk: your job search, one decision at a time. A desktop app for the ai-job-search commands." width="830">
</p>

<h1 align="center">Jobdesk</h1>

Jobdesk is a native macOS and Windows app, built in Rust, over the [ai-job-search](https://github.com/ai-job-search-dev/ai-job-search) slash commands. It runs them through your own Claude Code CLI.

<p align="center">
  <a href="https://github.com/Nicolas-Widart-SRL/jobdesk/releases/latest"><img src="https://img.shields.io/badge/Download_for_macOS-12263f?style=for-the-badge" alt="Download for macOS"></a>
  <a href="https://github.com/Nicolas-Widart-SRL/jobdesk/releases/latest"><img src="https://img.shields.io/badge/Download_for_Windows-12263f?style=for-the-badge" alt="Download for Windows"></a>
  <br>
  <a href="https://nicolas-widart-srl.github.io/jobdesk/">Website</a> · <a href="https://github.com/Nicolas-Widart-SRL/jobdesk/releases">All releases</a>
</p>

## What it does

- **Today** deals your decisions one card at a time: Y opens the suggested task, L sets it aside (until tomorrow by default), N skips, U undoes.
- **Jobs, Applications and Insights** update live as commands change your files.
- **Tasks** starts any slash command and shows its progress, Claude's questions, the cost and the history.
- **Profile** shows what Claude reads about you, topic by topic, opens the setup task for one topic, lets you edit a block in place, and previews your CV PDFs with how fresh each is.
- **Portals** shows the job boards Claude searches, turns one on or off, moves one you no longer want to the Trash (the Recycle Bin on Windows), lists the community's portals to install, and has Claude build one for a board you name.
- **Application dossiers** gather the PDFs, verification, keyword coverage, evaluation, prep packs and follow-ups of each application.
- **Settings** shows what your tasks cost and the terms Jobs checks against. It sets the theme, the Today queue, the Jobs view filters, how tasks sign in, run defaults, a monthly cap, tool permissions and how long run logs stay.
- **Nothing runs until you start it**, and apart from first-run setup, which fills only a new or empty folder, Jobdesk changes your checkout only when you save a Profile edit, switch a portal on or off, or move one to the Trash (the Recycle Bin on Windows).

<p align="center">
  <img src="tests/snapshots/intray_light.png" alt="The Today screen in the light theme, with one decision card, Done today and the Coming up deadlines." width="800">
</p>

## Install

On macOS, a fresh setup needs only the [Claude Code](https://code.claude.com/docs/en/overview) CLI, installed and signed in. On first launch, Jobdesk makes your job search folder, downloads ai-job-search into it, and installs whichever of Python, Bun and LaTeX this Mac lacks, for your user only, in `~/.jobdesk`. See [First run](docs/guide.md#first-run).

On Windows, or to open an ai-job-search folder you already have, you need:

- the [Claude Code](https://code.claude.com/docs/en/overview) CLI, installed, and either signed in on its own or given an Anthropic Console API key that you save in Settings › Providers and Jobdesk passes to it;
- an [ai-job-search](https://github.com/ai-job-search-dev/ai-job-search) checkout.

Download the installer for your system from the [latest release](https://github.com/Nicolas-Widart-SRL/jobdesk/releases/latest):

- **macOS** (Apple silicon): open `Jobdesk-vX.Y.Z-macos-arm64.dmg` and drag Jobdesk to Applications.
- **Windows**: run `Jobdesk-vX.Y.Z-windows-x64-setup.exe`.

The macOS app in a release is signed with a Developer ID and notarized by Apple, so it opens without the "cannot verify the developer" or malware warning. macOS may still ask you once to confirm that you downloaded it from the Internet. The Windows installer is not signed yet, so SmartScreen shows a warning:

- **Windows**: click More info, then Run anyway.
- **macOS**, for a release from before signing and for a build made with the Installers workflow on GitHub: the app is ad-hoc signed, and macOS blocks the first open. Try to open Jobdesk once, then go to System Settings, Privacy & Security, and click Open Anyway. On macOS 15 and later, right-click Open no longer bypasses this.

On first launch, macOS offers to set up from scratch or to open your ai-job-search folder. Windows asks you to choose your ai-job-search folder.

From the first release with updates on, Jobdesk tells you in the status bar when a new version is out. Download fetches it and checks its signature, and Restart to update installs it and opens Jobdesk again on the same folder. A copy from before that release does not update itself: install the new version by hand once.

On macOS, if your copy predates signing, note these points when you move to a signed release:

- The signed release opens without the "cannot verify the developer" warning, so you no longer need Open Anyway.
- If you saved an API key, macOS may ask once to allow access to it in the keychain, because the app's signature changed. Choose Always Allow. Later releases keep the same signing identity, so the choice holds.

If Restart to update cannot put the signed release in place, download its installer and install it yourself.

## Privacy and trust

Slash commands write your checkout. Apart from first-run setup, which fills only a new or empty folder, Jobdesk writes only the profile block you save, a portal's on or off switch, and a portal you remove, which it moves to the Trash (the Recycle Bin on Windows). Setup checks each file it downloads against Jobdesk's own record of it before it uses it. By default Jobdesk allows the commands' tool permissions without asking and lists each one in the activity, so a posting written to trick Claude could get a command run before you see it. Settings › Permissions turns prompts on. The workspace checks postings with `posting_guard` before scoring them. Run logs, which include the text of your runs, and your decisions stay in the app's data folder and are readable only by your user. Jobdesk uses Claude Code's sign-in and keeps no password of its own. The one exception is an optional Anthropic Console API key you save in Settings, which stays in the system keychain. Jobdesk asks GitHub for its latest release when it opens and about every 6 hours, sending only its version. Turn this off in Settings › About. Portals › Browse community reads the community portal list from GitHub, also sending only its version, when you open it and the saved copy is over an hour old, or when you press Refresh.

## Authentication

Jobdesk runs your installed Claude Code CLI. Tasks use Claude Code's own sign-in, or an Anthropic Console API key you paste in Settings › Providers, kept in the system keychain and passed only to the Claude Code process of each task. It is not made or endorsed by Anthropic.

## More

- [docs/guide.md](docs/guide.md): every screen in detail.
- [CONTRIBUTING.md](CONTRIBUTING.md): build, package, release and test.
- [ARCHITECTURE.md](ARCHITECTURE.md): how the code is structured.
- [CHANGELOG.md](CHANGELOG.md): what changed in each release.

## License

Jobdesk is released under the [MIT License](LICENSE).
