# Trust

This page covers how Jobdesk renders untrusted text, keeps tracker paths inside the workspace, guards the API key and handles tool permissions, and which residuals it accepts. The features it names are mapped in [ARCHITECTURE.md](../../ARCHITECTURE.md#features).

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

A link opens only on click, and only for `http` or `https`.

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
- The mask's dots and `sk-ant-` hold no quote or backslash, but for a long key its last four characters are the key's own. So a stream-json line stays JSON for a key with no `"` or `\` in it. `ApiKey::parse` does not reject those two, as it accepts any visible ASCII.
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
- With `ask_permission` on, which Settings › Permissions exposes, every `can_use_tool` request reaches the user with its input, marked as shortened past 20 KB.
- The CLI suggests saving allow rules to project settings. So `stream_json` rewrites every remembered rule to session scope, to keep the checkout unchanged.

## Notices

Notices other than allowances, such as why a run failed or stopped or which action was denied, show inline in the story. They render as plain text with hidden characters removed, one row per non-blank line.

The story keeps a notice's last 8 lines, because a stderr tail ends on its error, and shows the whole on hover.
