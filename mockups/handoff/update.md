# Updates

Brief: tell the user a new Jobdesk is out, with its notes and a way to download it, then apply it with a restart that keeps the folder and settings. The status bar carries it, the notes open in a popover over the desk, and Settings › About holds the switch and Check now. Approved.

- [Canvases](#canvases)
- [The status bar](#the-status-bar)
- [The release notes](#the-release-notes)
- [Settings › About](#settings--about)
- [Out of scope](#out-of-scope)
- [New widgets](#new-widgets)
- [Doc changes](#doc-changes)

## Canvases

`Update.html?state=…&theme=light|dark`, scope `.ud`, on `Grid.html`'s shell. `Settings.html?state=about…`, scope `.st`. Each PNG is 1280x820.

| PNG | State |
| --- | --- |
| [Update-available.png](../Update-available.png) | Today, with "Jobdesk 0.2.0 is out" and Download in the bar. |
| [Update-notes.png](../Update-notes.png), [dark](../Update-notes-dark.png) | The notes popover open over the desk, Download focused. |
| [Update-downloading.png](../Update-downloading.png) | Progress, share and size done, Cancel. |
| [Update-ready.png](../Update-ready.png) | Downloaded and verified: Restart to update. |
| [Update-notes-ready.png](../Update-notes-ready.png) | The popover once ready: what Restart does, then Restart to update. |
| [Update-ready-busy.png](../Update-ready-busy.png), [dark](../Update-ready-busy-dark.png) | Tasks with a run working: Restart disabled, its reason on hover. |
| [Update-failed.png](../Update-failed.png) | The download stopped, with why and Retry. |
| [Update-bad-signature.png](../Update-bad-signature.png) | The signature check failed: deleted, not installed, Retry. |
| [Settings-about.png](../Settings-about.png) | About with Updates: the switch on, Up to date, Check now. |
| [Settings-about-checking.png](../Settings-about-checking.png) | The switch off, Check now pressed: Checking…, Check now disabled. |
| [Settings-about-failed.png](../Settings-about-failed.png) | The check failed: the reason, Check failed beside About on the rail. |
| [Settings-about-update.png](../Settings-about-update.png) | 0.2.0 is out: What's new opens the notes popover, Download 0.2.0. |
| [Settings-about-ready-waits.png](../Settings-about-ready-waits.png), [dark](../Settings-about-ready-waits-dark.png) | Settings open while a task waits on the user: "A task waits for you" on the left, the update group on the right, Restart disabled with its reason. |

States map to the shared ones: loading is Checking…, empty is Up to date (nothing in the bar), failed is Check failed or a failed download.

## The status bar

- **Where**: left is Claude and tasks (`status_line`, `status_waits`, `status_note`). Right is Jobdesk itself: the update group, a `status_divider`, then Settings. The group sits just left of Settings because both are about the app, not the job search.
- **One group, one state at a time**: a square and words that open the notes (`status_mark_button`), then at most one action (`status_action`).

| State | Square | Words | Action |
| --- | --- | --- | --- |
| Available | `text` | Jobdesk 0.2.0 is out | Download |
| Downloading | none, a bar instead | Downloading 0.2.0, bar, share and MB (`status_progress`) | Cancel (`status_button`) |
| Ready | `hired` | 0.2.0 is downloaded and checked | Restart to update |
| Download failed | `closed` | Download of 0.2.0 stopped at 41%: the reason | Retry |
| Signature failed | `closed` | 0.2.0 failed its signature check. It was deleted, not installed. | Retry |

- **No stamp ink**: an update is not something that waits on the user, so stamp ink (`turn`) never marks it. The square follows `status_line`: `hired` for good, `closed` for failed, and `text` for news.
- **Quiet when nothing is new**: up to date, checking, and a failed automatic check show nothing in the bar. Only About says them. A failed Check now shows in About too.
- **Restart never swaps the app mid-task**: Restart to update is disabled while a Claude session is open, running or only waiting on a reply, as Change folder is. A session cut off mid-reply would lose the user's place, so a waiting session counts too. `enabled_when` says why on hover: "A task is running. Restart once it finishes, so it is not cut off halfway." or "A task waits for your reply in Tasks. End its session there, then restart." Restart goes through the normal quit, so an unsaved Profile draft still asks first.
- **Settings open**: the tabs hide, the bar stays. A task that waits takes the left (`status_waits`), and the update group keeps the right.
- **Narrow windows**: the group's words are cut first, with the full text on hover. The action and Settings never cut.
- **Nothing installs on its own**: Download only downloads and verifies. Only Restart to update applies.

## The release notes

The notes open in a popover over the desk, from the bar's mark button or About's What's new, so the user stays on their screen.

- **Popover**: egui's popup look, as `confirm_popover` draws it, anchored above the mark button with their right edges level. Head: "Jobdesk 0.2.0" in `Type::CALL_TITLE`, then the release day and "You have 0.1.0." The notes sit on paper (`card`, `line` hairline), at a fixed height, scrolling with the bar shown at rest. Below: the trust line in `faint`, then Download 0.2.0 (primary, focused on open), its size, and Open the release page. Once ready, a slip says what Restart does, and Restart to update takes Download's place. Esc and a click outside close it. The mark button is `on` while it is open.
- **Untrusted text**: the notes come from the network. They render as Markdown with no image loading and no links: headings held to `Type::SECTION`, bold and lists kept, a link shown as its words in body ink (never `link` style, never clickable), an image as its alt text in `muted`, raw HTML as plain text, hidden characters removed. The trust line says it: "From the GitHub release. Its links and images do not open here."
- **The one link out**: Open the release page is built by Jobdesk from the version tag on a fixed `https` GitHub address, never read from the notes or the feed.
- **Words**: the action is "Download 0.2.0" in the popover and About, and "Download" in the bar, where the version is already beside it. "Restart to update" is the same everywhere.

## Settings › About

About holds the updates, because the version already lives there. The section **Updates** sits between This copy and Read more.

- `setting_check` "Check for updates automatically", on by default. Its help says when (at launch and about every 6 hours), from where (the latest release on GitHub), and that it downloads nothing. Off, the help says Jobdesk looks only on Check now.
- `setting_row` "Latest version": its help carries the last check time, and its control the state: Up to date (`tag`, `hired`) with Check now; Checking… with Check now disabled; Check failed (`tag`, `closed`) with the reason in `closed` (plain text, hidden characters removed) and Check now; 0.2.0 is out with What's new (opens the notes popover) and Download 0.2.0; ready, with what Restart does and Restart to update.
- The rail word beside About: the version, "0.2.0 is out", "0.2.0 ready", or "Check failed" in `closed`.
- Search finds Updates, Check for updates automatically and Latest version.

## Out of scope

- **Skip this version**: not drawn and not built in this change. The bar shows a new version until the user installs it.

## New widgets

- `status_mark_button`: a `status_button` led by a `STATUS_DOT` square in a given ink, its words in `text`. Opens the notes, `on` while they are open. `STATUS_SEGMENT_HEIGHT`, `STATUS_SEGMENT_PADDING`, `raised` when on, `panel` on hover.
- `status_action`: a plain button at `STATUS_SEGMENT_HEIGHT`: `card` fill, `line` hairline, `RADIUS_BUTTON`, `Type::SMALL` in `text`. Disabled as the kit paints it, with `enabled_when`. The one bordered control in the bar, so the eye finds the next step.
- `status_divider`: a `line` hairline, `HAIRLINE` wide, between the update group and Settings.
- `status_progress`: words, then `bar` at a fixed width in `text` on `off`, `BAR_HEIGHT` tall, then the share and size in `muted` tabular figures.
- `release_notes_popover`: the popover above, built like `confirm_popover`.
- `release_markdown` in `text.rs`: `markdown` with links as words and images as alt text, for the notes only.

## Doc changes

- `ARCHITECTURE.md`: a new feature for updates (feed check, download, signature check, apply), listed in the Features table and in `FEATURES` in `tests/architecture.rs`. The shell draws the bar group and the notes popover, and Settings › About draws the section. The switch is a new key in `settings.json`, on by default.
- `docs/architecture/trust.md`: a Rendering untrusted text row, "Release notes from the update feed: Markdown with no image loading and no links. A link shows as its words, an image as its alt text." Also say that Open the release page is built from the version tag, and that a file that fails its signature check is deleted and never applied.
- `docs/guide.md`: in Settings, About's Updates section. In Setup and the status bar, the update group, its states and the notes popover. In One run at a time, Restart to update waits while a Claude session is open, even one only waiting on a reply.
- `AGENTS.md` Hard rules, with "One run works at a time": an update applies only from Restart to update, and never while a Claude session is open.
- `docs/architecture/ui-kit.md`: the new status-bar widgets and `release_notes_popover`, and that the bar's right side is Jobdesk's own: the update group, then Settings.
- **For the planner**: check whether the Windows NSIS installer shows its own window during Restart to update. If it does, the guide should say so, and the popover's ready slip may need to.
