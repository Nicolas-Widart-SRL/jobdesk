# Landing page

A Jobdesk landing page for GitHub Pages, as clean and engaging as a Stripe or Apple product page. The user approved this design, "The Product": Stripe-style precision on plain `sheet`, with the app's own dark desk as the deep field the product sits on.

- [Canvas](#canvas)
- [Sections](#sections)
- [Your own API key](#your-own-api-key)
- [Copy](#copy)
- [Rules](#rules)
- [Decided](#decided)
- [What a build needs](#what-a-build-needs)
- [Doc changes](#doc-changes)

## Canvas

`mockups/landing/Landing.html` is a plain static page: no `support.js`, no build step, and every path relative, so it opens from the file system as it would from Pages. It reads `?theme=light|dark` (the system theme otherwise) and `?motion=off`, which shows every moving part at rest. Render with `--file-access`, so the bundled fonts and the snapshots load, and always pass `theme=` and `motion=off`, because headless Chrome follows the Mac's own theme and stops the clock before the hero settles.

- `Landing-hero.png` and `Landing-hero-dark.png`, 1440x900: `scripts/render-canvas.sh --size 1440,900 --file-access mockups/landing/Landing.html mockups/landing/Landing-hero.png theme=light motion=off`, and the same with `theme=dark`.
- `Landing-full.png`, the whole page at 1440 wide, light: the same command at `--size 1440,6614`, the page's own height. Measure it again when the page changes length.
- `Landing-phone.png`, 390x844, light: headless Chrome makes no window under 500px wide and leaves the bottom of the window unpainted, so `mockups/landing/phone.html` shows `Landing.html` in a centred 390px frame. Render `phone.html` at `--size 500,1004` with `theme=light motion=off h=844 top=80`, then crop the middle with `sips -c 844 390 <png> --out <png>`. `phone.html` is a drawing aid kept only for this render, not part of the site.

## Sections

Schibsted Grotesk at weight 700 for every heading. Plain `sheet` above, no grid.

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
- **Features head**: "A desk for ai‑job‑search." Lede: "ai‑job‑search is a set of Claude Code slash commands that find postings, score them and draft your applications. Jobdesk is a desktop GUI over them: each screen shows what the commands keep in your checkout, and follows along live as they run." It credits the project before Jobdesk, and avoids "Every screen reads your files.", which the user found intrusive.
- **Credit**: ai‑job‑search links to `https://github.com/ai-job-search-dev/ai-job-search` in the hero line, the features lede, the download requirements and the footer.
- **Links**: downloads go to `https://github.com/Nicolas-Widart-SRL/jobdesk/releases/latest`, source to `https://github.com/Nicolas-Widart-SRL/jobdesk`, the guide to `.../blob/main/docs/guide.md` and the changelog to `.../blob/main/CHANGELOG.md`.
- **Installers** keep `vX.Y.Z`, as the README does. macOS says Apple silicon, signed with a Developer ID and notarized by Apple. Windows says the installer is not signed yet, and to click More info, then Run anyway.
- "ai‑job‑search" uses non-breaking hyphens in running text, so it never breaks across lines.
- **Tool permissions** row: "Allowed without asking by default, and listed in the activity once they run, so a posting written to trick Claude could get a command run before you see it. Settings › Permissions turns prompts on." It states the risk as `README.md`'s Privacy and trust does, and never says you see each one before it runs.
- The command chips in How it works sit under "Some of the commands", because ai‑job‑search has more than the seven shown.
- "Your own Claude Code" wording only, no Anthropic or Claude logo. The footer says "Not made or endorsed by Anthropic.", as the README does.

## Rules

- **Colours are palette tokens**, named as in `palette.rs`: `sheet`, `panel`, `raised`, `line`, `field`, `text`, `muted`, `primary`, `on_primary`, `active`, `turn`, `on_turn`, `grid`, `card`. Light uses the LIGHT set and dark the DARK set, swapped by `prefers-color-scheme` and `?theme=`. The deep field is the DARK `sheet` and `grid` under the light theme, and the DARK `field` under the dark theme, with the DARK `text`, `muted` and `line` on it. The site then looks like the app it sells.
- **Contrast** follows the pairs `palette.rs` tests: every text ink sits on `sheet`, `card` or `panel` of its own theme, or on the DARK desk in its DARK ink, all at 4.5:1 or more. Buttons are `on_primary` on `primary`. Focus is a 2px `active` ring (`FOCUS_RING`), the DARK `active` on the deep field.
- **One magenta**: `turn` appears only inside the snapshots, where the app itself puts it. No headline word is highlighted, so the highlighter keeps its one meaning.
- **Type**: Schibsted Grotesk for the page, Zilla Slab only for the brand name, and Courier Prime Bold only for commands, file names, installer names, `claude` and `sk-ant-`, as in the app (`look.md`). All three load from the bundled files in `assets/fonts` with `font-display: swap`. No CDN, tracker or analytics, so the page sends nothing anywhere.
- **No stock hero**: the window lies flat, with a solid offset shadow in the DARK `line`, no tilt and no grey blur.
- **Real screens only**: every screen image is a committed snapshot from `tests/snapshots/`, fictional data from `tests/sample_workspace`. A crop sets the snapshot's pixel box as CSS variables, so a re-cut is one line. The 12px margin round each snapshot is cropped off.
- **Motion**: the window and launcher on load, and the step line once. `prefers-reduced-motion` stops both, and the page shows each part at rest.
- **Accessibility**: one `h1`, `h2` per section, `h3` inside, in order. A skip link. Every snapshot has alt text that says what it shows.
- **Phone**: at 390 wide every grid becomes one column, the menu keeps only the brand and Download, buttons go full width, and the launcher overlaps the window's lower right. No horizontal scroll at 390 or 1440 (measured: the page's scroll width equals the window's).
- **Hard rules, stated as they are**: card actions never start a run, Y opens the launcher and the user presses Run there, one run at a time, Jobdesk offers no login of its own, and tool permissions are allowed without asking by default and listed once they run, with prompts in Settings › Permissions.

## Decided

- **The design**: this page as drawn, approved by the user.
- **Courier Prime** stays for slash commands, file names and installer names, as in the app.
- **Address**: the default `https://nicolas-widart-srl.github.io/jobdesk/`, no custom domain.
- **Guide and changelog**: the page links both, in the menu (Guide), the download row and the footer.

## What a build needs

- **Where**: a `site/` folder at the repo root, published by a Pages workflow that uploads it as is (`actions/upload-pages-artifact`, then `actions/deploy-pages`), on pushes to `main` that touch `site/`. No build step. The site lives at the default address above, so every path stays relative. The repo's homepage URL can point there too.
- **What it copies** into `site/`: `Landing.html` as `index.html`, with its paths changed from `../../` to local ones; `assets/fonts/ZillaSlab-SemiBold.ttf`, `SchibstedGrotesk-Variable.ttf` and `CourierPrime-Bold.ttf` with their OFL texts; `assets/brand/jobdesk-icon-small.svg` for the icon and favicon; and, light and dark, the snapshots `intray`, `launcher`, `jobs`, `run_turn`, `tracker`, `settings_usage` and `settings_key_saved`, at the crops the canvas sets. A check could compare the copies with `tests/snapshots/` so the site never shows a stale screen.
- **Image weight**: the snapshots are 1920x1230 PNGs. A build may convert them to WebP or AVIF at the shown size, with `width` and `height` set, and keep the hero image eager and the rest lazy.
- **Head**: `<link rel="canonical">` and `og:url` on the address above, `assets/brand/jobdesk-social.png` as `og:image`, and `og:title` and `og:description` from the page's own title and line.
- **Copy tracks the docs**: every fact on the page comes from `README.md` and `docs/guide.md`, and the key's from `trust.md`. A change to one of those that the page states changes the page too.
- **Signing**: the Windows "not signed yet" lines, in the download panel and the Signing row, change once the installer is signed.
- **Not in the canvas**: the `?theme=` and `?motion=` switches are for renders. A build may keep `?theme=` and drop `?motion=`.

## Doc changes

- `README.md`: link the site near the top, beside the download links.
- `CONTRIBUTING.md`: say that `site/` holds the landing page, which copies snapshots and states facts from `README.md` and `docs/guide.md`, so a change to either checks the page.
