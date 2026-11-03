# Settings

This page covers how preferences reach the window and the screens, what is derived from them, the update switch, the monthly cap, run log retention, and how Jobdesk replaces its own files. The features it names are mapped in [ARCHITECTURE.md](../../ARCHITECTURE.md#features). The sign-in method and the API key are in [runs](runs.md#sign-in).

## Preferences

Preferences live in `settings.json` in the app's data folder and apply at once:

- the theme;
- egui's zoom, for the text size;
- Reduce motion, which zeroes `ui_kit`'s motion time at runtime.

They reach the window at launch and when one changes, as Appearance changes it, never on every frame.

- With no file yet, as on the first launch after the theme left the status bar, Jobdesk keeps and saves the theme and zoom that `eframe` restored.
- A file that cannot be read gives the defaults and stays until the next change replaces it. The rail's foot tells that apart from a change that could not be saved.

### Today queue rules and Jobs view filters

The Today queue rules and the Jobs view filters live in `settings.json` too.

- The shell hands them to Today and Jobs from the preferences it holds in memory, never from disk.
- `InTray::set_rules` copies them only when they change.
- A saved list of skip reasons goes through the same rules as a typed one, so a hand-edited file cannot break them.

What the rules do is in [Today queue](today-queue.md) and [discovery](discovery.md#jobs-view-filters).

### The update switch

`check_updates` in `settings.json` holds Check for updates automatically, in Settings › About. It is on by default.

- Only `false` turns it off. A missing key, or a value of another type, reads as on.
- The shell hands a change to `Updates::set_auto` once, as it does Settings › Permissions. Check now looks whatever the switch says.
- The switch has one help for on and one for off. Search matches only the help it shows, through `Drawn::auto_check`. Latest version's help says the state of the check, so search does not look through it.

What the check does is in [trust](trust.md#updates).

## The Setup tab

The Setup tab's presence is derived on every frame, never stored.

- No folder chosen, or a required tool not found, keeps it in the menu.
- Profile files and optional tools never do.
- Tools not checked yet count as nothing.

## Usage statistics

Usage statistics reads the run logs through the `Snapshot`. The snapshot reloads when a task ends, that is, when the run desk goes from busy to idle.

## Run defaults and the monthly cap

The launcher starts from the run defaults, the command's override and the monthly cap, which the shell reads through `settings`' application layer.

- A command's override wins over the default for every task, and the catalog's budget is the last fallback.
- The cap sums this month's task spend the way Usage statistics does. So a resumed task counts on the day it started, and only the open folder's logs count.
- At or over the cap, Run stays disabled with its reason. Replies in a session already open still send.
- From a task's end until a snapshot read after it arrives, the month's spend counts as unread, so Run waits. Each reload ask is numbered, and a snapshot carries the last ask made before it read the logs.

## Retention

Run logs older than the retention are deleted at startup, judged by their last write. The retention defaults to Forever.

- A log from the current month is never deleted.
- Only regular `<run id>.jsonl` files directly in `runs/` are touched, never a link, a folder or another file.

## How `settings.json` and `decisions.json` are replaced

`settings.json` and `decisions.json` are replaced through `rename_over`, because their last writer wins anyway.

On Windows, a rename that another program blocks for a moment, such as an antivirus scan or a sync client, is tried again, up to 5 times 40 ms apart. These errors count as blocked:

| Error | Tried again |
| --- | --- |
| Sharing violation (32) | Always |
| Lock violation (33) | Always |
| Access denied (5) | Only while the target is a regular file without the read-only attribute |

An access list that denies the write reads the same as a held file, so it waits out the 5 tries before it fails.

Profile Save uses one plain rename instead: see [profile](profile.md#the-write).
