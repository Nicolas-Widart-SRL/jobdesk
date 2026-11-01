# Mockups

This folder holds HTML canvases of the current look: Postmark on a gridded desk, with the magenta highlighter on the open tab. Each canvas is a sketch for builders, not the source of truth. Token values live in `src/ui_kit/palette.rs` and `tokens.rs`, and widgets in `src/ui_kit/`. Git history keeps the older Ledger rounds.

## Building a canvas

`Grid.html` and `UiPass.html` load `app.css` and `support.js`, and scope their own rules in their `<style>` (`.gp`, `.up`). `Social.html` loads only `support.js` and scopes its rules to `.so`. `app.css` still carries Ledger-era rules and token values, and the canvases override them in their scope. So start a new canvas from `Grid.html`'s scoped styles, with its own scope class. Render it as `.claude/agents/designer.md` step 5 says, at 1280x820.

## Canvases

| Canvas | Query string | PNGs | Handoff file |
| --- | --- | --- | --- |
| `Grid.html` | `?screen=today\|jobs\|tracker\|turn\|firstrun&theme=light\|dark` | 6 `Grid-*.png` | [look](handoff/look.md), [today](handoff/today.md), [tasks](handoff/tasks.md) |
| `UiPass.html` | `?scene=states\|jobs\|failed\|loading\|empty&theme=light\|dark` | 6 `UiPass-*.png` | [ui-pass](handoff/ui-pass.md) |
| `Social.html` | none | `Social.png`, at 1280x640 | [social-preview](handoff/social-preview.md) |

## Handoff files

- **The look**: the desk and the paper, the menu, the highlighter, turn ink, type and words every screen shares → see [handoff/look.md](handoff/look.md). Open it before drawing any screen.
- **Today**: the head and day stamp, the deal card, Done today and Coming up, the other states, motion and accessibility → see [handoff/today.md](handoff/today.md). Open it when you change the Today screen.
- **Tasks**: the head, the turn band, the question bar, the run's story and the side column → see [handoff/tasks.md](handoff/tasks.md). Open it when you change the Tasks screen or how a run asks for the user.
- **UI pass**: the rules for hover, selected, pressed, disabled, links, copyable text, and loading, failed and empty states → see [handoff/ui-pass.md](handoff/ui-pass.md). Open it before adding or changing a clickable widget or an empty, loading or failed state.
- **Social preview**: the GitHub preview and README hero, its copy and its decisions → see [handoff/social-preview.md](handoff/social-preview.md). Open it when you redraw the preview or change the README hero.

A new round writes or updates one handoff file per screen or topic it changes, and its line here (`.claude/agents/designer.md` step 6).
