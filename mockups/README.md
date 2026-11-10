# Mockups

This folder holds HTML canvases of the current look: Postmark on a gridded desk, with the magenta highlighter on the open tab. Each canvas is a sketch for builders, not the source of truth. Token values live in `src/ui_kit/palette.rs` and `tokens.rs`, and widgets in `src/ui_kit/`. Git history keeps the pre-Postmark rounds.

## Building a canvas

`Grid.html`, `RunLive.html`, `UiPass.html`, `Profile.html`, `ProfileCv.html`, `Settings.html`, `Update.html`, `Onboarding.html` and `FirstSearch.html` load `app.css` and `support.js`, and scope their own rules in their `<style>` (`.gp`, `.rl`, `.up`, `.pf`, `.pc`, `.st`, `.ud`, `.ob`, `.fs`). `Social.html` loads only `support.js` and scopes its rules to `.so`. `app.css` still carries pre-Postmark rules and token values, and `Grid.html`'s scoped styles override them. So start a new canvas from `Grid.html`'s scoped styles on top of `app.css`, with its own scope class. Render it with `scripts/render-canvas.sh`, as `.claude/agents/designer.md` step 5 says, at 1280x820.

## Canvases

| Canvas | Query string | PNGs | Handoff file |
| --- | --- | --- | --- |
| `Grid.html` | `?screen=today\|jobs\|tracker\|turn\|firstrun&theme=light\|dark` | 6 `Grid-*.png` | [look](handoff/look.md), [today](handoff/today.md), [tasks](handoff/tasks.md) |
| `RunLive.html` | `?state=working\|quiet\|turn\|sheet&theme=light\|dark`, `&motion=on` to animate | 7 `RunLive-*.png` | [tasks](handoff/tasks.md) |
| `UiPass.html` | `?scene=states\|jobs\|failed\|loading\|empty&theme=light\|dark` | 6 `UiPass-*.png` | [ui-pass](handoff/ui-pass.md) |
| `Profile.html` | `?state=view\|edit\|busy\|conflict\|empty\|loading\|failed\|updated\|setup&theme=light\|dark` | 10 `Profile-*.png` | [profile](handoff/profile.md) |
| `ProfileCv.html` | `?state=full\|source\|busy\|missing\|notex\|failed\|loading\|empty\|unread\|nocommand&theme=light\|dark` | 11 `ProfileCv-*.png` | [profile](handoff/profile.md) |
| `Settings.html` | `?state=usage\|usage-empty\|loading\|providers\|key-entry\|key-saved\|key-failed\|terms\|terms-missing\|terms-failed\|general\|appearance\|queue\|jobs\|defaults\|cap\|perms\|runtime\|data\|about\|about-checking\|about-failed\|about-update\|about-ready-waits\|search\|nav&theme=light\|dark` | 29 `Settings-*.png` | [settings](handoff/settings.md), [update](handoff/update.md) |
| `Update.html` | `?state=available\|notes\|notes-ready\|downloading\|ready\|ready-busy\|failed\|bad-signature&theme=light\|dark` | 10 `Update-*.png` | [update](handoff/update.md) |
| `Onboarding.html` | `?state=welcome\|resume\|place\|notempty\|download\|offline\|mismatch\|checking\|installing\|toolfail\|docs\|dropped\|reading\|sheet-b1\|sheet-b2\|review\|working\|followup\|cvcheck\|choice\|stopped\|recap\|recap-nopdf&theme=light\|dark` | 26 `Onboarding-*.png`: welcome, resume, Folder and Computer at 720x560, the Profile chapter and the recap at 1280x820 | [onboarding](handoff/onboarding.md) |
| `FirstSearch.html` | `?state=today\|launcher\|jobs&theme=light\|dark` | 3 `FirstSearch-*.png` | [onboarding](handoff/onboarding.md) |
| `Social.html` | none | `Social.png`, at 1280x640 | [social-preview](handoff/social-preview.md) |
| `landing/Landing.html` | `?theme=light\|dark&motion=off`; phone through `landing/phone.html?theme=light&motion=off&h=1760&top=80` | 5 `landing/Landing-*.png`: `-hero` and `-hero-dark` at 1440x900, `-hero-1024` at 1024x900, `-phone` at 390x1760 and `-full` at 1440x5848 | [landing](handoff/landing.md) |

## Handoff files

- **The look**: the desk and the paper, the menu, the highlighter, stamp ink, type and words every screen shares → see [handoff/look.md](handoff/look.md). Open it before drawing any screen.
- **Today**: the head and day stamp, the deal card, Done today and Coming up, the other states, motion and accessibility → see [handoff/today.md](handoff/today.md). Open it when you change the Today screen.
- **Tasks**: the head, the turn band, the question bar, the run's story, the side column, and how an open run shows it is alive (the sweep, live Elapsed from send to run end, the Cost estimate, the quiet state) → see [handoff/tasks.md](handoff/tasks.md). Open it when you change the Tasks screen, how a run asks for the user, what moves while Claude works, or how Cost and Elapsed count.
- **UI pass**: the rules for hover, selected, pressed, disabled, links, copyable text, and loading, failed and empty states → see [handoff/ui-pass.md](handoff/ui-pass.md). Open it before adding or changing a clickable widget or an empty, loading or failed state.
- **Profile**: the Profile screen: topics and their files, Update with Claude…, the Updated mark, block editing, its states, Setup's Profile panel, the `profile` feature, and CV documents (the CV PDF preview: a list column beside the pages, its four tags, the `/build-cv <name>` launcher action, and the line that replaces it when the folder has no `/build-cv` command) → see [handoff/profile.md](handoff/profile.md). Open it when you change the Profile screen, how Jobdesk writes profile files, or how it shows CV PDFs.
- **Settings**: what Settings still lacks: notifications, Reduce motion following the OS, and the licences link → see [handoff/settings.md](handoff/settings.md). Open it when you add one of them.
- **Updates**: the update group in the status bar, the release notes popover and how the notes render, Restart while a Claude session is open, and Settings › About's Updates section → see [handoff/update.md](handoff/update.md). Open it when you change how Jobdesk updates itself or the right side of the status bar.
- **Onboarding**: round 2 of first run, the compact card wizard (B): the window that grows once at the Profile chapter, the download and helper checks, resuming after a quit, the documents drop zone, the question sheet (B1 one field, B2 a field per question, recommended), the review and CV check cards, the recap with Add a job site you use…, the shared first search, and the open questions → see [handoff/onboarding.md](handoff/onboarding.md). Open it when you change or build first run, or how Jobdesk sets up ai-job-search.
- **Social preview**: the GitHub preview and README hero, its copy and its decisions → see [handoff/social-preview.md](handoff/social-preview.md). Open it when you redraw the preview or change the README hero.
- **Landing page**: the GitHub Pages landing page: round 2's approved hero B (the journey drawn from the app's parts), the shorter API key section, the copy fixes, every place the key appears, the rules, and what a `site/` build needs → see [handoff/landing.md](handoff/landing.md). Open it when you change or build the landing page.

A new round writes or updates one handoff file per screen or topic it changes, its canvas's row in the Canvases table, and its line under Handoff files (`.claude/agents/designer.md` step 6).
