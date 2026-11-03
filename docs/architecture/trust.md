# Trust

This page covers how Jobdesk renders untrusted text, keeps tracker paths inside the workspace, guards the API key, handles tool permissions and checks its own updates, and which residuals it accepts. The features it names are mapped in [ARCHITECTURE.md](../../ARCHITECTURE.md#features).

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
| Release notes from the update feed | Markdown with no image loading and no links, through `release_markdown`. A link shows as its words in body ink and never opens, an image as its alt text, and raw HTML as its text. Headings render at the section size, and hidden characters are removed before parsing. |

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

Paths read from the tracker must stay inside the workspace. `within_workspace` checks them, and it reads `/` and `\` as separators on every system.

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

## The API key

The API key has one path: the system keychain, then the environment of that task's `claude` process, as `ANTHROPIC_API_KEY`. When the key is read in a run is in [runs](runs.md#the-key-in-a-run). Test connection passes it the same way, in its own process's environment only.

- The key never goes on the command line, into a run log's `jobdesk_meta` line, or into `settings.json`.
- A saved key never goes onto the screen, which shows only `sk-ant-`, dots and, for a key long enough, its last four characters.
- The one exception is the key being typed in `secret_field`, shown in full only while the user presses Show.
- `secret_field` never copies its text to the clipboard and keeps no undo history.
- `ApiKey` in `shared_kernel` has no `Clone` and no `Display`, and its `Debug` prints no text, so a log line or a panic cannot carry it.

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

## Tool permissions

By default `RunDesk` allows every tool permission itself and records each one as an allowance row in the activity, so a run never stalls on a prompt nobody sees. Each step folds its allowances behind its toggle.

This trades away the prompt as a check against injected posting text. Postings rely on `posting_guard` in the workspace instead.

- Questions from `AskUserQuestion` always reach the user.
- The shell hands Settings › Permissions to `RunDesk::set_asks_permission`, which holds from the next poll on. With prompts on, every other `can_use_tool` request reaches the user as a permission prompt. Its detail shows a `Bash` command, or any other tool's input as JSON, cut past 20,000 bytes with a note of how many bytes it left out. The answer sends the complete input back.
- The CLI suggests saving allow rules to project settings. So `stream_json` rewrites every remembered rule to session scope, to keep the checkout unchanged.

## Notices

Notices other than allowances, such as why a run failed or stopped or which action was denied, show inline in the story. They render as plain text with hidden characters removed, one row per non-blank line.

The story keeps a notice's last 8 lines, because a stderr tail ends on its error, and shows the whole on hover.

## Updates

The `updates` feature reads the feed, downloads a build, checks its signature and applies it on Restart to update. None of it is a run: it starts no `claude`, uses no API key, writes no run log and is not counted in Usage statistics.

### Addresses and the network

- The feed is `latest.json` on the latest release, at the fixed address `FEED_URL`.
- The download's address and the release page's address are built from `REPOSITORY` and the version tag, such as `v0.2.0`. No address is ever read from the feed or the notes.
- `HttpsTransport` requests `https` only, after a redirect too, follows at most 5 redirects, checks certificates with the system's verifier and uses the proxy the environment names. It sends no cookie and no credential, only `Jobdesk/<version>` as its user agent.
- A failure shows in Jobdesk's own words, such as "the connection timed out" or "GitHub answered with the error 404". No text from the server reaches the screen.

### The feed and its limits

- The feed holds the version, the release date, the notes, and each build's signature and size. Any other field is never read.
- A feed over 256 KiB is refused whole, and notes are cut at 64 KiB.
- The version must be exactly three numbers, such as `0.2.0` or `v0.2.0`. Only a version newer than this copy is offered.
- A download stops at the size the feed gives, and never goes past 200 MiB.
- The feed request must end within 20 seconds. A download fails when its next bytes take over 30 seconds, when a minute brings under 16 KiB, or after an hour.

### Signature checks

Each copy checks a download against the update key built into it from `assets/update/jobdesk-update.pub`. The committed file is a placeholder, so a build that carries it has no key: it still checks the feed and shows a new version, but says it cannot check a download and offers only the release page. The Tag release workflow refuses to publish while the file is the placeholder.

- A download is written to a `.part` file in `updates/<pid>/` of the data folder, which on macOS only the user can read. It gets its final name only once its signature checks.
- The signature's trusted comment must name exactly one file, the versioned name of the build, such as `Jobdesk-v0.2.0-windows-x64-setup.exe`. So a signature of another build or another version fails.
- A file that fails its check is deleted and never applied.
- Restart to update reads the file once and checks those bytes again, against the signature of the release it was downloaded for, before it uses them. On macOS it unpacks those same bytes. On Windows it copies them into a new `apply-<pid>` folder, holds the copy open so nothing may write or delete it, checks the bytes read through that handle, then starts that copy.
- Only the `sign` job of `tag-release.yml` holds the private key, and only on a release. Before tagging, the publish job checks both signatures against the public key of the previous release, because that is the key installed copies trust, or at the first release with a real key, against this commit's own. How to create and change the key is in [CONTRIBUTING](../../CONTRIBUTING.md#the-update-key).

### Where an update may apply

- macOS: only a copy that runs from a `.app` bundle, in a folder where Jobdesk can make a folder beside it. A mounted disk image or a copy macOS moved aside to open cannot.
- Windows: only a copy with `uninstall.exe` beside it, which the installer puts there.
- The macOS archive must hold only `Jobdesk.app/`. A path that leaves it, a link that is absolute or climbs with `..`, a path through a link, a hard link or a device refuses the whole archive. Unpacked files drop their set-id bits and group and other write.
- The new bundle trades places with the old one in one step, with `renamex_np` and `RENAME_SWAP`. Where the disk cannot, the old one moves aside and comes back if the new one does not go in.
- When Restart to update cannot apply, Jobdesk writes an `apply-failed` note in `updates/` and opens the old copy again while it is still there. The next launch reads the note once. On Windows, once the setup has started, Jobdesk notes nothing more.
- At launch, `clear_downloads` empties `updates/`, but for that note and the folder of any Jobdesk that still runs. It removes links and never follows them. So a past session's download is never applied. On macOS it also removes what a Restart to update that died partway left beside the bundle.

### Accepted residuals

1. The feed itself is not signed. Whoever can change `latest.json` on the latest release can show any version number and notes, or hide a release. They cannot get a file applied without a signature from the update key.
2. Whoever holds the update key and its password, and can publish a release in this repository, can ship a build every copy installs.
3. The macOS app is not signed with a Developer ID, so macOS App Management may stop Jobdesk from replacing the bundle. Then the swap fails, the old copy opens again, and the next launch says the new copy could not be put in place.
4. The Windows setup, in its passive mode, closes every running `jobdesk.exe` without asking, so another Jobdesk window closes too, whatever it was doing. Restart to update waits only on the sessions of the window it runs in.
