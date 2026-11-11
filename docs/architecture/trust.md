# Trust

This page covers how Jobdesk renders untrusted text, keeps tracker paths and CV files inside the workspace, guards the API key, handles tool permissions, sets up a first folder and its helpers, reads the community portal list, checks its own updates and updates the job search folder with them, and which residuals it accepts. The features it names are mapped in [ARCHITECTURE.md](../../ARCHITECTURE.md#features).

Posting text, notes and reports are third-party or generated data. On this page, "hidden characters" means control, escape, bidi and zero-width characters, which `visible_text` in `shared_kernel` drops.

## Rendering untrusted text

| Text | Renders as |
| --- | --- |
| Posting text, notes and reports | Plain text |
| Archived postings | Always plain text |
| Claude's replies and prep packs | Markdown with no image loading. On the Tasks screen they read as documents, with headings capped at the section size. |
| Claude's closing question, pinned above the reply box and shown on the Today card | Plain text, with Markdown markup and hidden characters removed |
| A failed tool's reason, and a load error's reason | Plain text, with hidden characters removed |
| A run's notices | Plain text, with hidden characters removed: see [Notices](#notices) |
| Test connection's error | Plain text: see [Masking](#masking) |
| PDFs, in the dossier and in CV documents | Page images only: see [CV files](#cv-files) |
| A task in plain words in the launcher, which may hold a name from GitHub | Plain text, read only, sent to Claude exactly as shown. A task with any character that hidden-character removal would drop, line breaks included, or with any variation selector (U+FE00 to U+FE0F, U+E0100 to U+E01EF), which can show as nothing after a letter, opens no launcher, so the text shown and the text sent never differ. |
| A community portal's name and sentence, from its `SKILL.md` on GitHub | Plain text on one line: line breaks and runs of space become one space, hidden characters and variation selectors are removed, a name keeps at most 60 characters and a sentence 200, and the screen cuts what does not fit with an ellipsis |
| A community portal's owners, from `CODEOWNERS` | Plain words, never links: only GitHub handles such as `@name` or `@org/team` are kept, at most 10 a line, cut with an ellipsis |
| An installed portal's name and sentence, from its own `SKILL.md` | Plain text on one line, with Markdown markup and hidden characters removed, cut with an ellipsis |
| Release notes from the update feed | Markdown with no image loading and no links, through `release_markdown`. A link shows as its words in body ink and never opens, an image as its alt text, and raw HTML as its text. Headings render at the section size, and hidden characters are removed before parsing. |
| File paths in the folder update's notice and details, from both release zips and the user's folder | Plain text on one line, never Markdown or a link: line breaks, tabs, hidden characters and variation selectors are removed, a path keeps at most 160 characters, cut with an ellipsis, and a list shows at most 40 paths and counts the rest. Nothing in the details opens. |

A link opens only on click, and only for `http` or `https`. Release notes have no link: their one way out is Open the release page, whose address Jobdesk builds itself.

## Profile text

Profile text renders through `profile_markdown`, which is `markdown` with HTML blocks that hold only comments left out.

- It loads no images, and a link opens only on click, only for `http` or `https`.
- Other HTML shows as its text.
- Headings render as plain text with hidden characters removed.
- Placeholders such as `[FIRST_NAME]` show as written in the body. Only a role's title, company and dates line, which `ui_kit` lays out itself, draws them as "to fill" blanks.
- The editor shows the block's raw text, and only Preview renders it, as the paper does.
- Only the `## Candidate Profile` part of `CLAUDE.md` reaches the view, never its workflow rules.

## Paths from the tracker

Paths read from the tracker must stay inside the workspace. `within_workspace` in `shared_kernel` checks them, and it reads `/` and `\` as separators on every system.

On every system it refuses:

- an empty path;
- a leading separator;
- a `..` part;
- a part that starts with a drive letter and `:`;
- a path that names the root itself.

On Windows only, it also refuses:

- any `:`, which names an alternate data stream;
- a part named like a device, such as `con.json`.

The check reads only the text of the path and never asks the file system. So the path it returns names a place under the workspace folder, but symbolic links are not resolved. The dossier's archive readers, and the PDF pages they lead to, follow links when they open the file, so a link inside the workspace, at the file or at a folder on its path, can lead the read outside the workspace.

## CV files

The files in `cv/` come from the checkout, and CV PDFs carry text tailored from postings.

- A CV PDF renders only as page images: no text layer, link, form or embedded action reaches the screen. The renderer's reason for a failure shows as plain text with hidden characters removed.
- The list skips names that start with a dot, are not `.tex` or `.pdf`, or hold a control or hidden character, so no row shows a name that looks like another file's.
- Each name goes through `within_workspace` as `cv/<name>` and must come back as that one part. Then every link is resolved: `cv/` must sit inside the resolved checkout, and the file must be a regular file inside the resolved `cv/`. A `cv/` that resolves outside shows nothing, and a file that does is skipped.
- Open PDF and Show in Finder check the name again when clicked. Open PDF hands the system viewer only a resolved file whose extension is `.pdf`, as one argument that no shell reads.
- `.claude/commands/build-cv.md` is checked only for existence, as a regular file inside the resolved checkout. Jobdesk never reads it.
- `/build-cv <stem>` is offered only for a stem of ASCII letters, digits, `_` and `-` that does not start with `-`, so the line it fills in carries no option and no second command.

The checks run once, before the work they guard. So, as for a profile save:

- a running task that swaps a file for a link between the click's check and the spawn can send Open PDF or Show in Finder elsewhere;
- pages render from the path the list was read with, so a file swapped for a link after that read can lead the render outside the workspace, as for the dossier.

## The API key

The API key has one path: the system keychain, read once per launch until a read succeeds, then Jobdesk's process memory for the session, then the environment of that task's `claude` process, as `ANTHROPIC_API_KEY`. When the key is read is in [runs](runs.md#how-a-task-gets-the-method). Test connection passes it the same way, in its own process's environment only.

- The key never goes on the command line, into a run log's `jobdesk_meta` line, or into `settings.json`.
- A saved key never goes onto the screen, which shows only `sk-ant-`, dots and, for a key long enough, its last four characters.
- The one exception is the key being typed in `secret_field`, shown in full only while the user presses Show.
- `secret_field` never copies its text to the clipboard and keeps no undo history.
- `ApiKey` in `shared_kernel` has no `Clone` and no `Display`, and its `Debug` prints no text, so a log line or a panic cannot carry it.
- The key held in memory is an `ApiKey` in `Providers`, never serialized. Remove drops it once the keychain has deleted it.
- The held key follows no change made to the keychain outside Jobdesk until it quits.

### Masking

- Each line a task's process prints on stdout has the whole key it was given replaced by the mask, before the run log and the events see it. So does its failure from stderr.
- The mask holds dots, `sk-ant-` when the key starts with it, and, for a long key, the key's last four characters. So an `sk-ant-` key with no `"` or `\` cannot break a stream-json line. `ApiKey::parse` accepts any visible ASCII, so it also takes keys whose mask can break a line. A key such as `1234567890123` can match a bare number. A key such as `nabc` can match right after an escape's backslash, so `"a\nabc"` becomes `"a\••••••••"`.
- Test connection's error, what Claude Code said, renders as plain text with hidden characters removed and the key masked wherever it appears, cut to 8 lines and 500 characters.
- A part of the key, or the key encoded some other way, is not caught.

### Accepted residuals

1. The key sits in `claude`'s own environment, so any process of the same user can read it: with `ps -Eww` on macOS, or from the process's environment block on Windows. Any tool the agent runs as the same user can equally read the Claude sign-in's credentials. Jobdesk sets `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB=1`, so Claude Code strips the key from the environment of the tools, hooks and MCP servers it starts.
2. An older Claude Code that ignores that variable hands the key to its tools. Jobdesk checks no version.
3. The scrub has side effects, under the API key only: Claude Code forces its permission mode to default, and tools lose variables such as `SSH_AUTH_SOCK`, `GH_TOKEN` and `GITHUB_TOKEN`, `XDG_*`, `CARGO_HOME`, `NPM_CONFIG_*` and any that look like secrets.
4. An inherited `ANTHROPIC_BASE_URL` is kept on purpose, for a proxy the user trusts, so the key goes to the host it names. A base URL set in the `env` of the workspace's project settings is not checked, because the CLI reads it itself. So a task that rewrites those settings could route the key of a later run to a host it picks.

   `key_route_refusal` in `shared_kernel` checks the inherited URL with these rules:
   - A task under the key and a key's Test connection start no process when that URL is set and not blank and is neither `https` nor `http` to `localhost`, `127.0.0.0/8` or `::1`. They say why without naming any value. Text that is no URL is refused too.
   - An `http` URL on this machine is refused as well when `HTTP_PROXY`, `http_proxy`, `ALL_PROXY` or `all_proxy` is set and not empty, whatever `NO_PROXY` says.

   When a key is passed, the provider switches and `ANTHROPIC_AUTH_TOKEN`, which would outrank the key, are also removed from the environment. `ENV_THAT_OUTRANKS_THE_KEY` in `shared_kernel` lists them.
5. After the first read, the key stays in Jobdesk's memory until Remove or quit, so a process of the same user that can read Jobdesk's memory can read it. That spares a keychain prompt at each task.
6. On macOS the keychain may still ask at the first read of each launch, until the user chooses Always Allow. A denied prompt is not held, so the next task's start or key test asks again, and Save key and Remove may prompt too. A release is signed with a Developer ID, so Always Allow survives its updates, except at the first signed release, whose signature differs from an ad-hoc one and asks once more. A build from source or from the Actions tab is ad-hoc signed, so Always Allow may not survive its update. The legacy keychain prompt offers no Touch ID.

## Tool permissions

By default `RunDesk` allows every tool permission itself and records each one as an allowance row in the activity, so a run never stalls on a prompt nobody sees. Each step folds its allowances behind its toggle.

This trades away the prompt as a check against injected posting text. Postings rely on `posting_guard` in the workspace instead.

- Questions from `AskUserQuestion` always reach the user.
- The shell hands Settings › Permissions to `RunDesk::set_asks_permission`, which holds from the next poll on. With prompts on, every other `can_use_tool` request reaches the user as a permission prompt. Its detail shows a `Bash` command, or any other tool's input as JSON, cut past 20,000 bytes with a note of how many bytes it left out. The answer sends the complete input back.
- The CLI suggests saving allow rules to project settings. So `stream_json` rewrites every remembered rule to session scope, to keep the checkout unchanged.

## Notices

Notices other than allowances, such as why a run failed or stopped or which action was denied, show inline in the story. They render as plain text with hidden characters removed, one row per non-blank line.

The story keeps a notice's last 8 lines, because a stderr tail ends on its error, and shows the whole on hover.

## First-run setup

On macOS, the `onboarding` feature makes the job search folder, fills it with the `ai-job-search` release, and installs the helpers its commands call. None of it is a run: it starts no `claude`, writes no run log and is not counted in Usage statistics. Every download goes through `HttpsTransport`, under the rules of [Addresses and the network](#addresses-and-the-network).

### The folder

- Setup uses only a folder that is missing or empty. A folder that holds anything else is refused, and setup suggests a new `Job search` folder inside it. A folder that is already an `ai-job-search` folder is refused too: the welcome's other choice opens it as it is. A folder that holds only Finder's `.DS_Store`, and on macOS its `.localized`, counts as empty, and setup deletes those two files.
- Setup looks at the folder again when Save here is pressed, and the rename that puts the release in place never replaces a folder that holds anything else. So a folder that filled meanwhile is never written over.
- The release zip must match the SHA-256 of [the ai-job-search pin](#the-ai-job-search-pin) before anything is unpacked. A zip that does not match, or that is larger than a release should be, is thrown away, and the folder stays empty.
- The whole archive is checked before anything is written. One entry that could lead outside the folder, a link or any other special file, an encrypted entry, two names one disk would take for the same name, or a top `.jobdesk` entry, spelled in any letter case or Unicode composition, refuses the whole archive.
- Files are written into a hidden `.<name>.jobdesk-<pid>` staging folder beside the chosen one, which then takes its place in one rename and keeps the permissions the empty folder had. Unpacked files keep their read and run bits, drop their set-id bits and group and other write, and stay readable and writable by the user.
- Setup writes the zip that matched and a record of its version and SHA-256 into the folder's `.jobdesk/`, as `source.zip` and `source.json`. What that record proves is in [the ai-job-search pin](#the-ai-job-search-pin).

### The helpers

- The helpers are Python, Bun and LaTeX. A copy already on the computer counts only when it works for `ai-job-search`. To learn this, setup runs the copies the tool check finds on the `PATH`, as [runs](runs.md#the-path) says. A check that cannot tell, such as one that takes too long, counts as missing.
- Python counts only through the first `python3` on the `PATH` that has a run bit set, the one a task runs. A file without a run bit is passed over, as a shell's search does. When that first runnable `python3` is a `/usr/bin` stub with no developer tools behind it, or a link to one, Python counts as missing. Otherwise it counts only when it runs as 3.10 or newer. A copy past it, or a `python`, never counts.
- Bun counts when it runs. LaTeX counts when `lualatex` and `xelatex` run and the `kpsewhich` beside each finds every pinned TeX Live package.
- A missing helper installs only after the user chooses Install what is missing. `onboarding/domain/helpers.rs` pins each of its files by version, SHA-256 and size, at an address built from a fixed host and the version: GitHub's release downloads for Python, Bun and TinyTeX, and the TeX Live historic archive for the packages. Every file of a helper must match before any of it is unpacked.
- A helper archive is refused whole for a path with `..` or a root, a path through a link, a link that leaves its folder, a hard link, a device, an entry written twice, or more than 20,000 entries or 512 MiB.
- The helpers install for this user only, in `~/.jobdesk`, the one folder of the home folder that Jobdesk owns: `~/.jobdesk/python`, `~/.jobdesk/bun` and `~/.jobdesk/TinyTeX`. Nothing needs an administrator password.
- Setup never writes, replaces or removes anything elsewhere in the home folder, such as `~/.bun` or `~/Library/TinyTeX`, and never changes another copy of a helper. In `~/.jobdesk` it touches only the helpers' places and their staging folders. How it puts an install in place, and replaces an earlier install of its own that does not work, is in [ARCHITECTURE](../../ARCHITECTURE.md).
- A `~/.jobdesk` that is a link, even to a folder, or anything but a real folder, is refused before any download, install, replace or cleanup: the install fails as something else taking its place, and setup writes nothing there, removes nothing there and never looks through it.
- The helpers' folders come first on the `PATH` of every task, Test connection, the tool check and Runtime status, before every other copy, as [runs](runs.md#the-path) says. So once setup installs a helper, tasks use its copy, not one the user had.

### The progress record

- Setup keeps `onboarding.json` in the data folder, readable only by the user. It holds the chosen folder, the steps done, the `ai-job-search` version being set up, and the day of the last change. A record larger than 64 KiB, or one Jobdesk did not write, reads as no record.
- Each save goes through a new temporary file, renamed over the record, so a crash or a power loss leaves the previous record whole.
- The download's worker records the version once the feed pins it, and the folder as made once the release has taken the folder's place, before the window hears. When setup opens on a record that lacks the folder made, but whose folder is an `ai-job-search` folder with a `.jobdesk/` that reads whole for the recorded version, the folder counts as made. A record that cannot be written costs only that resume: setup goes on with what it holds in memory, and says nothing more.
- The record is removed when the Computer step ends.
- Downloads wait in `onboarding/<pid>/` of the data folder. When setup opens, it deletes the downloads, staging folders and `-old` folders that a Jobdesk which no longer runs left. Each launch with a data folder also deletes such downloads, as the folder update starts, since its own download waits there too. It removes links and never follows them.

### Accepted residuals

1. The helper pins are built into Jobdesk, so whoever ships a Jobdesk build decides which Python, Bun and TeX Live files it installs.
2. To check a copy the user already has, setup runs it. A program placed on the `PATH` runs with the user's rights, as it would in their terminal.
3. Once installed, Jobdesk's helpers shadow the user's own copies for every task, newer ones included.
4. The `~/.jobdesk` checks refuse a link they find when they look. A program running as the user that swaps `~/.jobdesk` for a link between the check and the write could send an install, a replace or a cleanup elsewhere. A program with the same access as Jobdesk can already change or delete those files directly. But macOS privacy permissions, such as access to Documents, can differ between processes of the same user. A program without that access that swaps `~/.jobdesk` for a link at the right moment could make Jobdesk write its pinned helper files, or remove folders named like its own staging folders, inside a protected folder. What Jobdesk writes there is only its own helper files, checked against their pinned hashes. Directory-relative operations would close this gap, and Jobdesk does not use them yet.

## The community portal list

Browse community on the Portals screen reads the list of community portals from GitHub. It is not a run: it starts no `claude`, uses no API key, writes no run log and is not counted in Usage statistics.

- It reads when Browse comes on show and the saved list is missing or over an hour old, and when the user presses Refresh or Try again. Never on every frame, and never while GitHub asks Jobdesk to wait. After a failed read, Browse coming on show again reads nothing for a minute, so a read that keeps failing cannot spend GitHub's limit. Try again still reads at once.
- The REST API names the latest commit of `ai-job-search-dev/community-portals` and its file list: 2 requests a read, of the 60 an hour GitHub allows without sign-in. Each `SKILL.md` and `CODEOWNERS` comes from `raw.githubusercontent.com` at that commit, and an unchanged file is not fetched again.
- Requests go over https with the shared `https_config`, follow no redirect, send no cookie and no credential, and stop at fixed sizes and times. A failure shows in Jobdesk's own words. No text from the server reaches the screen.
- GitHub's rate limit keeps the saved list on screen and holds Refresh until its reset time. A 403 or 429 that names no time holds it for a minute.
- Only a path of the exact shape `portals/<country>/<folder>/SKILL.md` is read. The country is two lower-case letters or `global`, and the folder name holds only lower-case letters, digits and `-`.
- View code ↗ opens `https://github.com/ai-job-search-dev/community-portals/tree/main/portals/<country>/<folder>`, built from those validated parts, and only on click. Install… opens the launcher on a task in plain words built from the same address: Claude copies the portal into `.agents/skills/`, explains its code and asks before it installs anything. Jobdesk does not enforce that question, see the residual below. Nothing from the list's text goes into an address or a task. A task the launcher would refuse leaves Install… disabled with its reason.
- The list read last is saved in `portals/community.json` of the data folder, through a copy of its own. Every field is checked again when it is read back, as if it came from GitHub, and a file that fails a check reads as no saved list.
- Add a portal builds `/add-portal <website> country: …, language: …, test title: …` from what the user typed. The website must be an `http` or `https` address with a dotted host and no space or line break anywhere, so nothing after a space can reach `/add-portal` as an option. It gets `https://` when it does not start with a scheme. A field holding a character that shows nothing makes no line, and the form says which field.

### Accepted residuals

1. The portal's code is text Claude reads during the Install… task, so it could steer Claude past the request to ask first, like injected posting text. Then `python3 tools/install_portal_clis.py`, or another command, runs with no question. Nothing makes the question a condition of running the installer, and `RunDesk` allows the tool request itself by default. Once a portal is installed, `/scrape` runs its CLI under the same approvals anyway. Settings › Permissions shows the permission prompts Claude sends, but an allow rule in Claude Code's settings can approve a tool request before Claude sends one.

## Updates

The `updates` feature reads the feed, downloads a build, checks its signature and applies it on Restart to update. None of it is a run: it starts no `claude`, uses no API key, writes no run log and is not counted in Usage statistics. The job search folder updates with it, as [Folder update](#folder-update) says.

### Addresses and the network

- The feed is `latest.json` on the latest release, at the fixed address `FEED_URL`.
- The download's address and the release page's address are built from `REPOSITORY` and the version tag, such as `v0.2.0`. No address is ever read from the feed or the notes.
- The address of an `ai-job-search` source zip is built from `AI_JOB_SEARCH_REPOSITORY` and the pinned version's tag: GitHub's fixed address for the zip of that tag, such as `https://codeload.github.com/ai-job-search-dev/ai-job-search/zip/refs/tags/v1.4.0`.
- `https_config`, in `shared_kernel::https`, requests `https` only, after a redirect too, follows at most 5 redirects, checks certificates with the system's verifier and uses the proxy the environment names. It sends no cookie and no credential, only `Jobdesk/<version>` as its user agent. Every feature that reads the network starts from it, so the rules hold for each. The updates and first-run setup read through `HttpsTransport`, which adds fixed timeouts, and [the community portal list](#the-community-portal-list) follows no redirect at all.
- A failure shows in Jobdesk's own words, such as "the connection timed out" or "GitHub answered with the error 404". No text from the server reaches the screen.

### The feed and its limits

- The feed holds the version, the release date, the notes, each build's signature and size, and the `ai-job-search` pin. Any other field is never read.
- A feed over 256 KiB is refused whole, and notes are cut at 64 KiB.
- The version must be exactly three numbers, such as `0.2.0` or `v0.2.0`. Only a version newer than this copy is offered.
- A download stops at the size the feed gives, and never goes past 200 MiB.
- The feed request must end within 20 seconds. A download fails when its next bytes take over 30 seconds, when a minute brings under 16 KiB, or after an hour.

### The ai-job-search pin

The feed itself is not signed, so the release Jobdesk vouches for at first-run setup is pinned with a signature of its own. The `ai_job_search` field holds the version of an `ai-job-search` release, the SHA-256 of its source zip, and the update key's signature of the two.

- The Tag release workflow resolves the latest release of `ai-job-search-dev/ai-job-search`, downloads that tag's source zip from the fixed address, and hashes it. Its `sign` job signs the line `<sha256>  ai-job-search-v<version>.zip` and a line end, as `sha256sum` writes it, with a trusted comment that names `ai-job-search-v<version>.zip` alone. The publish job hashes the zip again, checks the signature like those of the builds, and publishes nothing when no release resolves or the hash changed.
- Jobdesk rebuilds that line from the version and the hash in the feed, and checks the signature against it with the update key built into it. So a signature of another version, another hash or another key fails.
- The version must be exactly three numbers, and the hash 64 lowercase hex digits.
- Setup keeps the release zip that matched the pin in the folder's `.jobdesk/`, with its version and SHA-256. That kept base is checked against its recorded SHA-256 before use. The record sits beside the zip, so the check catches a damaged copy, not a change by something that can write the folder, such as a run's tools: a use that needs to trust the base checks it against the signed pin. A record whose version is not exactly three numbers, or whose hash is not 64 lowercase hex digits, does not read. A link or an oversized file there is refused when looked at, with a short window before the file opens.
- A pin that is missing, does not read or fails its check is dropped, and only the pin: the update check reads the same release either way. A build with no update key has no pin, and does not read the feed for one.

### Signature checks

Each copy checks a download against the update key built into it from `assets/update/jobdesk-update.pub`, which, outside a key change, holds the public key releases are signed with. A build whose file holds no key it can read has no key: it still checks the feed and shows a new version, but says it cannot check a download and offers only the release page. The Tag release workflow refuses to publish while the file holds the word `PLACEHOLDER` in place of a key.

- A download is written to a `.part` file in `updates/<pid>/` of the data folder, which on macOS only the user can read. It gets its final name only once its signature checks.
- The signature's trusted comment must name exactly one file, the versioned name of the build, such as `Jobdesk-v0.2.0-windows-x64-setup.exe`. So a signature of another build or another version fails.
- A file that fails its check is deleted and never applied.
- Restart to update reads the file once and checks those bytes again, against the signature of the release it was downloaded for, before it uses them. On macOS it unpacks those same bytes. On Windows it copies them into a new `apply-<pid>` folder, holds the copy open so nothing may write or delete it, checks the bytes read through that handle, then starts that copy.
- Only the `sign` job of `tag-release.yml` holds the private key, and only on a release. Before tagging, the publish job checks both signatures against the public key of the previous release, because that is the key installed copies trust, or at the first release with a real key, against this commit's own. How to create and change the key is in [CONTRIBUTING](../../CONTRIBUTING.md#the-update-key).

### Where an update may apply

- macOS: only a copy that runs from a `.app` bundle, in a folder where Jobdesk can make a folder beside it. A mounted disk image or a copy macOS moved aside to open cannot. A build with an update key checks this at each launch: it makes a `.Jobdesk.app.probe-<pid>` folder beside the bundle and removes it at once.
- Windows: only a copy with `uninstall.exe` beside it, which the installer puts there.
- The macOS archive must hold only `Jobdesk.app/`. A path that leaves it, a link that is absolute or climbs with `..`, a path through a link, a hard link or a device refuses the whole archive. Unpacked files drop their set-id bits and group and other write.
- The new bundle trades places with the old one in one step, with `renamex_np` and `RENAME_SWAP`. Where the disk cannot, the old one moves aside and comes back if the new one does not go in.
- When Restart to update cannot apply, Jobdesk writes an `apply-failed` note in `updates/` and opens the old copy again while it is still there. The next launch reads the note once. On Windows, once the setup has started, Jobdesk notes nothing more.
- At launch, `clear_downloads` empties `updates/`, but for that note and the folder of any Jobdesk that still runs. It removes links and never follows them. So a past session's download is never applied. On macOS it also deletes, beside the bundle, the `.Jobdesk.app.update-`, `.old-` and `.probe-` folders of a Jobdesk that no longer runs: what a Restart to update or a launch check that died partway left there.

### Folder update

The `folder_update` feature merges the `ai-job-search` release the feed pins into the open job search folder, and undoes its last update. It is not a run: it starts no `claude`, uses no API key, writes no run log and is not counted in Usage statistics. When it runs and who waits for it is in [ARCHITECTURE](../../ARCHITECTURE.md#the-folder-update).

The release:

- Only a pin the update key checked reaches it. `SignedPin` is made only by the feed's read in the `updates` feature, and `LatestFeed`, which the folder update gets, carries only that pin. A build with no update key has no pin and updates no folder.
- It runs only while this Jobdesk is the latest version the feed names, so the pin is the one its own release was published with.
- The pin must be strictly newer than the folder's base. So a replayed feed, even with a pin the key once signed, never takes a folder back to an older release or the same one.
- The zip comes from the fixed address of the pinned tag, as [Addresses and the network](#addresses-and-the-network) says, through `HttpsTransport`. A download past 100 MiB is stopped and deleted.
- `read_verified` checks the new zip against the pin's SHA-256, then every entry, as for [setup's folder](#the-folder), before the update looks at a file it would change or writes anything. A zip that fails is not used, and nothing changes.

The folder:

- A folder that holds a `.git` entry of any kind, a folder, a file or a link, or one that cannot be looked at, is never updated.
- The base is the zip and record in `.jobdesk/`. It is checked only against its own record, as [the pin](#the-ai-job-search-pin) says, so it is untrusted input: whatever can write the folder can change which files the update takes as untouched, and so replaces or removes. Every file the update writes holds the signed release's bytes, the user's own, or a three-way merge of the two.
- A `.jobdesk` that is a link or not a folder refuses the update. No file of a release goes into `.jobdesk`, spelled in any letter case.
- Each path is looked at one folder at a time from the job search folder down, and a link is never followed. A file that is a link, has a link above it, or whose side copy would be one is skipped and listed, so nothing is read, written, renamed or deleted through a link. A folder where a file goes, a file where a folder goes, and a release's own file where a side copy goes are skipped as in the way.
- A side copy's name ends in the plain version, such as `CLAUDE.md.update-1.8.0`, so Claude Code never loads it as a command, an agent, a setting or a module.
- `.claude/settings.json` and `tools/security_guards.py` guard what Claude may do. In a conflict the new copy goes in place and the user's is saved beside it as `<path>.yours-<version>`, and a deleted one comes back when the release changed it.
- For a file the release renames in letter case or accent composition alone, the update never writes, renames or deletes the user's file under either name, since macOS and Windows take both for one file. A change it would make becomes a conflict under the new name, with the new copy beside it.

The staging folder, the journal and the backup:

- Every new file is written into a staging folder beside the job search folder, which on macOS only the user can read, and the original of each file it replaces or deletes goes into the backup, before the first rename.
- Each file must still hold what the plan read when it is staged, and again just before its rename. Otherwise the update stops, and what it did is rolled back. So a file saved while the update runs is never replaced, but for a save in the short gap between that last read and the rename.
- The lock in `.jobdesk/.lock` keeps two Jobdesks from updating, undoing or rolling back one folder at once.
- The journal and the backup's record live in the folder, where a run's tools can write, so each is read as untrusted input. A link, a file over 32 MiB, a hash that is not 64 lowercase hex digits, a path that is not a plain relative one or that names a part of `.jobdesk` other than the base's two files, or a backup name that is not a plain number refuses the whole record. A journal that does not read is set aside as `.jobdesk/update-stopped.json`, and nothing is put back from it.
- A rollback or an Undo puts back a file only while it still holds what the update wrote, by SHA-256, and only from an original in the backup whose SHA-256 is the one the record names. A file changed since stays as it is, and the notice lists it. A safety file and the `.yours-` copy saved beside it stay or go back together: while the file stays, for any reason, the copy holds the user's settings from before the update, so it stays and is listed too, and a copy changed since keeps the file. `.jobdesk`, `backup` and `backup/files` must be real folders, not links.
- Undo notes the release it undoes in `backup/skipped.json` before it changes anything, and refuses when it cannot. The gate skips only that very release, version and hash alike, so a note written by anything else can skip no other release and makes nothing apply.

The paths the notice shows come from both zips and the folder, so they render as [the table](#rendering-untrusted-text) says.

### Accepted residuals

1. The feed itself is not signed. Whoever can change `latest.json` on the latest release can show any version number and notes, or hide a release. They cannot get a file applied without a signature from the update key.
   For the `ai-job-search` pin, they can drop it, or put back the signed pin of an earlier Jobdesk release, so first-run setup offers an older `ai-job-search` release that the update key once signed. They cannot pin a zip the key never signed.
   For the folder update, they can only hold it back: by dropping the pin or naming another Jobdesk as the latest. A replayed pin updates a folder only to a release newer than its base.
2. Whoever holds the update key and its password, and can publish a release in this repository, can ship a build every copy installs.
3. A macOS build that is ad-hoc signed, one made from source or from the Actions tab, has no Developer ID, so macOS App Management may stop Jobdesk from replacing the bundle. Then the swap fails, the old copy opens again, and the next launch says the new copy could not be put in place. A release is signed with a Developer ID.
4. The Windows setup, in its passive mode, closes every running `jobdesk.exe` without asking, so another Jobdesk window closes too, whatever it was doing. Restart to update waits only on the sessions of the window it runs in.
5. The `ai-job-search` pin vouches for what GitHub served for the latest `ai-job-search` tag when the Jobdesk release ran, unread. Whoever could publish an `ai-job-search` release at that time decides what the pinned zip holds.
6. Every check the folder update makes of a file, or of a link on its path, comes just before the act it guards, never with it. A save, or a folder swapped for a link, that lands in the gap is not seen: the save is lost, and the link can send a rename or a removal outside the folder. A run's tools could do this at any moment, so the checks guard against what the user leaves in the folder, and they hold only because no update, Undo or rollback runs while a task or a Claude session is open.
7. On Windows, the folder update renames each file once, with no retry, unlike `rename_over`. A file another program holds open, such as an editor, a sync tool or an antivirus, fails the update, which then rolls back, or fails the rollback itself. Then the folder stays part updated until the next open's rollback tries again.
8. The journal lives in `.jobdesk/`, so whatever can write the folder can plant one, and the next open rolls it back. A record that passes the checks above can put a numbered file of the backup back over a file whose SHA-256 it names, or remove such a file. A record planted in the backup does the same when the user presses Undo. That is no new power: Claude's tools can already write the folder.
9. On a disk that tells letter case apart, a file the release renames in case alone stays under its old name, but the notice names the new one, and its side copy takes the new name too.
10. When the backup is gone, a link or not a real folder, the rollback puts back only what needs no original: it removes the files the update added, but for the `.yours-` copy beside a safety file it keeps, and keeps and lists every file it replaced or deleted and that copy, then sets the journal aside. If the update had already put its new base in place, that base now names the new release while the files it added are gone, so the next update takes them for files the user deleted and leaves them out. If not, the files it kept hold the new release over the old base, so the next update takes them for the user's changes.
