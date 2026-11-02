# Mockups

This folder holds HTML canvases of the current look: Postmark on a gridded desk, with the magenta highlighter on the open tab. Each canvas is a sketch for builders, not the source of truth. Token values live in `src/ui_kit/palette.rs` and `tokens.rs`, and widgets in `src/ui_kit/`. Git history keeps the pre-Postmark rounds.

## Building a canvas

`Grid.html`, `UiPass.html`, `Profile.html` and `Settings.html` load `app.css` and `support.js`, and scope their own rules in their `<style>` (`.gp`, `.up`, `.pf`, `.st`). `Social.html` loads only `support.js` and scopes its rules to `.so`. `app.css` still carries pre-Postmark rules and token values, and `Grid.html`'s scoped styles override them. So start a new canvas from `Grid.html`'s scoped styles on top of `app.css`, with its own scope class. Render it as `.claude/agents/designer.md` step 5 says, at 1280x820.

## Canvases

| Canvas | Query string | PNGs | Handoff file |
| --- | --- | --- | --- |
| `Grid.html` | `?screen=today\|jobs\|tracker\|turn\|firstrun&theme=light\|dark` | 6 `Grid-*.png` | [look](handoff/look.md), [today](handoff/today.md), [tasks](handoff/tasks.md) |
| `UiPass.html` | `?scene=states\|jobs\|failed\|loading\|empty&theme=light\|dark` | 6 `UiPass-*.png` | [ui-pass](handoff/ui-pass.md) |
| `Profile.html` | `?state=view\|edit\|busy\|conflict\|empty\|loading\|failed\|updated\|setup&theme=light\|dark` | 10 `Profile-*.png` | [profile](handoff/profile.md) |
| `Settings.html` | `?state=usage\|usage-empty\|loading\|providers\|key-entry\|key-saved\|key-failed\|terms\|terms-missing\|terms-failed\|general\|appearance\|queue\|jobs\|defaults\|cap\|perms\|runtime\|data\|about\|search\|nav&theme=light\|dark` | 24 `Settings-*.png` | [settings](handoff/settings.md) |
| `Social.html` | none | `Social.png`, at 1280x640 | [social-preview](handoff/social-preview.md) |

## Handoff files

- **The look**: the desk and the paper, the menu, the highlighter, stamp ink, type and words every screen shares → see [handoff/look.md](handoff/look.md). Open it before drawing any screen.
- **Today**: the head and day stamp, the deal card, Done today and Coming up, the other states, motion and accessibility → see [handoff/today.md](handoff/today.md). Open it when you change the Today screen.
- **Tasks**: the head, the turn band, the question bar, the run's story and the side column → see [handoff/tasks.md](handoff/tasks.md). Open it when you change the Tasks screen or how a run asks for the user.
- **UI pass**: the rules for hover, selected, pressed, disabled, links, copyable text, and loading, failed and empty states → see [handoff/ui-pass.md](handoff/ui-pass.md). Open it before adding or changing a clickable widget or an empty, loading or failed state.
- **Profile**: the Profile screen: topics and their files, Update with Claude…, the Updated mark, block editing, its states, Setup's Profile panel and the `profile` feature → see [handoff/profile.md](handoff/profile.md). Open it when you change the Profile screen or how Jobdesk writes profile files.
- **Settings**: the full-screen Settings, its rail and pages, the API key in the keychain and Test connection, usage statistics, when the Setup tab shows, new widgets and doc changes → see [handoff/settings.md](handoff/settings.md). Open it when you change Settings, how tasks sign in, or a preference it holds.
- **Social preview**: the GitHub preview and README hero, its copy and its decisions → see [handoff/social-preview.md](handoff/social-preview.md). Open it when you redraw the preview or change the README hero.

A new round writes or updates one handoff file per screen or topic it changes, its canvas's row in the Canvases table, and its line under Handoff files (`.claude/agents/designer.md` step 6).
