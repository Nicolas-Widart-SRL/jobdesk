# Landing page

A Jobdesk landing page for GitHub Pages, as clean and engaging as a Stripe or Apple product page. The user picked option B, "The Product", and this round reworked its copy. B is not approved yet. Option A stays drawn, unchanged, until it is.

- [Canvases](#canvases)
- [Option B, "The Product"](#option-b-the-product)
- [Your own API key](#your-own-api-key)
- [Copy](#copy)
- [Rules](#rules)
- [Option A, "The Desk"](#option-a-the-desk)
- [What a build needs](#what-a-build-needs)
- [Open](#open)

## Canvases

Each canvas is a plain static page: no `support.js`, no build step, and every path relative, so it opens from the file system as it would from Pages. Both read `?theme=light|dark` (the system theme otherwise) and `?motion=off`, which shows every moving part at rest. Render with `--file-access`, so the bundled fonts and the snapshots load, and always pass `theme=` and `motion=off`, because headless Chrome follows the Mac's own theme and stops the clock before the hero settles.

- Desktop: `scripts/render-canvas.sh --size 1440,900 --file-access mockups/landing/B.html mockups/landing/B-hero.png theme=light motion=off`. The full page uses the page's own height: 6614 for B and 5587 for A.
- Phone: headless Chrome makes no window under 500px wide and leaves the bottom of the window unpainted, so `phone.html` shows a page in a centred 390px frame. Render it at `--size 500,1004` with `page=B` (or `A`), `theme=light`, `motion=off`, `h=844` and `top=80`, then crop the middle with `sips -c 844 390 <png> --out <png>`. `phone.html` is a drawing aid, not part of the site.

PNGs per option: `-hero` (1440x900, light), `-hero-dark`, `-full` (1440 wide, the whole page, light) and `-phone` (390x844).

## Option B, "The Product"

Stripe-style precision. Plain `sheet` above, no grid, and Schibsted Grotesk at weight 700 for every heading. The app's own dark desk is the deep field the product sits on.

1. **Menu**: sticky, on `sheet` with a `line` hairline under it: the icon and name, Features, How it works, Your API key, Trust, Guide, Source, then Download.
2. **Hero**: the headline across the full width. Under it, the line with the ai‑job‑search link, and the key line: a key glyph, "Bring your own Anthropic API key, stored only in your system keychain." and a How link to the key section. The download buttons and View the source sit to the right. Below, the Today snapshot as a flat window, crossing from `sheet` onto the deep field, with the launcher's side panel layered over its right edge. On load the window rises and the launcher slides in after it.
3. **A desk for ai‑job‑search.**: who does the work and what Jobdesk is, then a 12-column grid of cells in four sizes, each a snapshot cropped where it says most, bleeding off the cell's lower right: the Jobs shortlist, a job's checks, the Tasks band and question, Applications' lanes, two text cells (dossiers, Profile) and Usage statistics.
4. **Your commands, your Claude Code, your key.**: on the deep field, three numbered steps, because they are a sequence: you pick a command (the command chips), your Claude Code runs it on your key or its own sign-in (the line Claude will run), your files change and the screens follow (the files `/apply` writes). A line joins the steps and draws once as they come into view.
5. **Bring your own Anthropic API key.**: see [Your own API key](#your-own-api-key).
6. **You stay in charge of every run.**: a two-column spec sheet of eight facts (starting a run, one at a time, sign-in, permissions, what it writes, your data, updates, signing), then one pulled line: "The app never sends an application for you."
7. **Download**: "Download Jobdesk.", two panels, macOS and Windows, the requirements, then a row of links: All releases, User guide, Changelog.
8. **Footer**: Source on GitHub, All releases, User guide, Changelog, MIT License, then "A desktop app for ai‑job‑search, built in Rust. Not made or endorsed by Anthropic."

The page shows no failure case: no failed load, error or "what still works" copy, in the text, the captions or the alt text. The check tags OK, Unclear and Fails stay, because they are a job's result, not the app failing.

## Your own API key

The user asked for the bring-your-own-key option to be prominent, because Anthropic's terms expect it. The page makes no legal claim. It shows the key in four places: the hero's key line, a menu link, the how-it-works step and its lede, and a section of its own after How it works. Trust's Sign-in row and the download requirements name the key first, with Claude Code's sign-in as the other way.

The section: a lede ("Paste a key from your Anthropic Console in Settings › Providers. Tasks then run on that key and bill its Console account. Jobdesk keeps it on your computer and hands it only to Claude Code."), five facts on the left, and the API key panel of Settings › Providers on the right, cropped from `settings_key_saved_*`, with a fictional key shown in part.

Every fact matches `AGENTS.md`'s hard rule and `docs/architecture/trust.md` (The API key):

- **Stored only in your keychain**: the macOS Keychain or the Windows Credential Manager, never a file. Read from there, held in memory until quit or Remove.
- **Passed only to Claude Code**: in the `claude` process's environment. Never on a command line or in a log, and never on screen in full.
- **Shown only in part**: `sk-ant-`, dots and the last four characters. Only the key being typed shows in full, while the user presses Show.
- **Tested before it is used**: one fixed question, no tools, an empty temporary folder, capped at $0.01. Tasks switch to the key only once the test passes.
- **Yours to remove**: Remove deletes it from the keychain and puts tasks back on Claude Code's sign-in.

The page does not say the key is unreadable by other programs: `trust.md` accepts that a process of the same user can read it from `claude`'s environment.

## Copy

All facts come from `README.md` and `docs/guide.md`, and the key's from `trust.md`. No testimonials, counts, prices, ratings or logos.

- **Headline**: "The desktop app for your ai‑job‑search commands."
- **Hero line**: "Jobdesk puts the ai‑job‑search slash commands on a desk. It deals your decisions one card at a time and runs any command through your own Claude Code. Nothing runs until you start it."
- **Features head**: "A desk for ai‑job‑search." Lede: "ai‑job‑search is a set of Claude Code slash commands that find postings, score them and draft your applications. Jobdesk is a desktop GUI over them: each screen shows what the commands keep in your checkout, and follows along live as they run." It replaces "Every screen reads your files.", which the user found intrusive, and credits the project before Jobdesk.
- **Credit**: ai‑job‑search links to `https://github.com/ai-job-search-dev/ai-job-search` in the hero line, the features lede, the download requirements and the footer.
- **Links**: downloads go to `https://github.com/Nicolas-Widart-SRL/jobdesk/releases/latest`, source to `https://github.com/Nicolas-Widart-SRL/jobdesk`, the guide to `.../blob/main/docs/guide.md` and the changelog to `.../blob/main/CHANGELOG.md`.
- **Installers** keep `vX.Y.Z`, as the README does. macOS says Apple silicon, signed with a Developer ID and notarized by Apple. Windows says the installer is not signed yet, and to click More info, then Run anyway.
- "ai‑job‑search" uses non-breaking hyphens in running text, so it never breaks across lines.
- "Your own Claude Code" wording only, no Anthropic or Claude logo. The footer says "Not made or endorsed by Anthropic.", as the README does.

## Rules

- **Colours are palette tokens**, named as in `palette.rs`: `sheet`, `panel`, `raised`, `line`, `field`, `text`, `muted`, `primary`, `on_primary`, `active`, `turn`, `on_turn`, `grid`, `card`. Light uses the LIGHT set and dark the DARK set, swapped by `prefers-color-scheme` and `?theme=`. The deep field is the DARK `sheet` and `grid` under the light theme, and the DARK `field` under the dark theme, with the DARK `text`, `muted` and `line` on it.
- **Contrast** follows the pairs `palette.rs` tests: every text ink sits on `sheet`, `card` or `panel` of its own theme, or on the DARK desk in its DARK ink, all at 4.5:1 or more. Buttons are `on_primary` on `primary`. Focus is a 2px `active` ring (`FOCUS_RING`), the DARK `active` on the deep field.
- **One magenta**: `turn` appears only inside the snapshots, where the app itself puts it. No headline word is highlighted.
- **Type**: Schibsted Grotesk for the page, Zilla Slab only for the brand name, and Courier Prime only for commands, file names, installer names, `claude` and `sk-ant-`, as in the app (`look.md`). All three load from `assets/fonts` with `font-display: swap`. No CDN, tracker or analytics.
- **No stock hero**: the window lies flat, with a solid offset shadow in the DARK `line`, no tilt and no grey blur.
- **Real screens only**: every screen image is a committed snapshot from `tests/snapshots/`, fictional data from `tests/sample_workspace`. A crop sets the snapshot's pixel box as CSS variables, so a re-cut is one line. The 12px margin round each snapshot is cropped off.
- **Motion**: the window and launcher on load, and the step line once. `prefers-reduced-motion` stops both, and the page shows each part at rest.
- **Accessibility**: one `h1`, `h2` per section, `h3` inside, in order. A skip link. Every snapshot has alt text that says what it shows.
- **Phone**: at 390 wide every grid becomes one column, the menu keeps only the brand and Download, buttons go full width, and the launcher overlaps the window's lower right. No horizontal scroll at 390 or 1440 (measured: the page's scroll width equals the window's).
- **Hard rules, stated as they are**: card actions never start a run, Y opens the launcher and the user presses Run there, one run at a time, Jobdesk offers no login of its own, and tool permissions are allowed by default and listed, with prompts in Settings › Permissions.

## Option A, "The Desk"

Not picked. Kept unchanged in `A.html` and its PNGs until B is approved, then removed. It tells the story on the app's gridded desk: the dealt Today card and the struck day stamp in the hero, a working Y, L, N, U deck, three full screens, the "Nothing runs until you start it." band and download. None of this round's copy changes are in A.

## What a build needs

- **Where**: a `site/` folder at the repo root, published by a Pages workflow that uploads it as is (`actions/upload-pages-artifact`, then `actions/deploy-pages`), on pushes to `main` that touch `site/`. No build step. The site lives at the default address, `https://nicolas-widart-srl.github.io/jobdesk/`, with no custom domain, so every path stays relative.
- **What it copies** into `site/`: `B.html` as `index.html`, with its paths changed from `../../` to local ones; `assets/fonts/ZillaSlab-SemiBold.ttf`, `SchibstedGrotesk-Variable.ttf` and `CourierPrime-Bold.ttf` with their OFL texts; `assets/brand/jobdesk-icon-small.svg` for the icon and favicon; and, light and dark, the snapshots `intray`, `launcher`, `jobs`, `run_turn`, `tracker`, `settings_usage` and `settings_key_saved`. A check could compare the copies with `tests/snapshots/` so the site never shows a stale screen.
- **Image weight**: the snapshots are 1920x1230 PNGs. A build may convert them to WebP or AVIF at the shown size, with `width` and `height` set, and keep the hero image eager and the rest lazy.
- **Head**: `<link rel="canonical">` and `og:url` on the address above, `assets/brand/jobdesk-social.png` as `og:image`, and `og:title` and `og:description` from the page's own title and line.
- **Not in the canvas**: the `?theme=` and `?motion=` switches are for renders. A build may keep `?theme=` and drop `?motion=`.

## Open

- Approve B as drawn, so the next round removes A and finalizes.
- Courier Prime: kept for commands, file names and installer names as in the app. The brief named two fonts. Drop it for Schibsted Grotesk if you prefer.
- The Windows note says the installer is not signed yet. Update the copy when that changes.
