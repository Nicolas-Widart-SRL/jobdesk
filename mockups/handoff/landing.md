# Landing page

A Jobdesk landing page for GitHub Pages, as clean and engaging as a Stripe or Apple product page. The user approved "The Product" in round 1: Stripe-style precision on plain `sheet`, with the app's own dark desk as the deep field the product sits on. Round 2 applies the review's group C (hero copy, hero image, a shorter key section, the hero key line) and the approved copy fixes. The user approved round 2 with hero B, drawn in HTML, and this file describes only that design.

- [Canvas](#canvas)
- [Round 2: what changed and why](#round-2-what-changed-and-why)
- [The hero](#the-hero)
- [Sections](#sections)
- [Your own API key](#your-own-api-key)
- [Copy](#copy)
- [Rules](#rules)
- [Decided](#decided)
- [What a build needs](#what-a-build-needs)
- [Doc changes](#doc-changes)

## Canvas

`mockups/landing/Landing.html` is a plain static page: no `support.js`, no build step, and every path relative, so it opens from the file system. Its markup and CSS follow `site/index.html` as built at the start of round 2 (the tablet and phone fixes, the stepped connector, the 1200 breakpoint, the download labels and lines), with the canvas's `../../` paths and render switches kept. Render switches, for renders only:

- `?theme=light|dark` overrides the system theme. `?motion=off` shows every moving part at rest. Always pass both, because headless Chrome follows the Mac's theme and stops the clock before the hero settles.
- `?measure=1` adds a badge with the page's scroll width, the window's width and the page's height, for checks; never commit a render with it.

Render with `scripts/render-canvas.sh --file-access`, so the bundled fonts and the snapshots load:

- `Landing-hero.png` and `Landing-hero-dark.png`: `--size 1440,900`, `theme=light` or `theme=dark`, `motion=off`.
- `Landing-hero-1024.png`: `--size 1024,900`, light.
- `Landing-phone.png`, 390x1760, light, tall enough to show the whole hero and the top of the launcher: render `mockups/landing/phone.html` at `--size 500,1920` with `h=1760 top=80`, then `sips -c 1760 390`. Headless Chrome makes no window under 500px wide, so `phone.html` frames the page at 390 and passes `theme`, `motion` and `measure` through. It is a drawing aid, not part of the site.
- `Landing-full.png`: the whole page at `--size 1440,5848`, the page's own height in a window that tall (`?measure=1` reads it; the hero grows to 860px when the window is taller than 928px, so measure at the full height). Measure again when the page changes length.

Checked at 1440, 1280, 1024, 768, 560 and 390, light and dark: the page's scroll width equals the window's.

## Round 2: what changed and why

- **C1, hero copy.** The old line named the integration before the value ("The desktop app for your ai‑job‑search commands."). The hero now leads with what the user does, then says in a smaller line that Jobdesk is the desktop app for ai‑job‑search users, a GUI over its slash commands, with ai‑job‑search linked.
- **C2, hero image.** The old hero paired a Ready to send card with a `/scrape` launcher, a different action. The hero now shows one journey: a Today card for a new match and the `/apply` launcher its own Y opens, with that card's line under Claude will run. Both are drawn in HTML from the app's own parts, not screenshots.
- **C3, the key section.** It was long and slowed the story. The heading and the Providers screenshot stay as large as before; the lede now carries the point; the five facts fold into a `<details>` "What Jobdesk does with your key", with a link to `docs/architecture/trust.md#the-api-key`. Trust's Tool permissions row stays as it was.
- **C4, the hero key line.** "Run tasks on your own Anthropic API key, kept only in your system keychain, or on Claude Code's own sign-in." with the link "How the key is kept" to the key section. The key comes first, Claude Code's own sign-in is named as the other way, the key is never called optional or required, and no plan or its limits is named.
- **Hero requirements.** Now "Needs an Apple silicon Mac or 64-bit Windows, the Claude Code CLI and an ai‑job‑search checkout." The key line right above already names both ways to run tasks, so the requirements no longer repeat them. The CLI stays a plain requirement.
- **Copy fixes.** "Anthropic Console" becomes "Claude Console". The Usage cell no longer says the statistics count spend: the cost is Claude Code's estimate at API list price, and on a Claude plan it is not your bill; the monthly cap stays. How it works is now "Your commands, your Claude Code CLI, your key."
- **1440 made deliberate.** In the site the two long download labels stacked at full column width and pushed the window about 90px lower than the approved canvas. The hero is now split in two at 1440 (see [The hero](#the-hero)), so the downloads stack in the words column by design and nothing below them moves.
- **500 to 640 wide.** The site set the deep field to a fixed 300px at the bottom of `.stage`, so on a tall phone window it started below the window's bottom and the window sat on plain `sheet`. In this hero the deep field is the desk's own background, from the desk's top edge to its bottom at every width, so the card always sits on it.

## The hero

**"Pick the job. See the command. Press Run."** Three short sentences that are the journey itself. Lede: "Your job search comes to you one decision at a time." then, in `text` weight 600, "Y on a new match opens the launcher with the line Claude will run, and Claude drafts your application only once you press Run." Then the credit line, the downloads, "Both buttons open the latest release on GitHub." with View the source, and under a `line` hairline the key line (C4) and the requirements.

- **Layout at 1440 to 1024**: two columns, words left (5 of 11 parts) and the desk right (6 parts), 64px apart. The hero is at least `min(100vh - 68px, 860px)` tall and the words sit centred in it. The deep field fills the desk from the menu to the hero's bottom and bleeds off the page's right edge. The desk has 40px above and below and 48px on its left; the launcher hangs 24px past the card's right edge. The line is `clamp(38px, 4.6vw, 66px)`, at most 9.5em wide, so it breaks after "See" and after "command." at 1440. The two downloads stack in the words column.
- **Under 1200**: the desk's left padding drops to 32px and the launcher tucks in to the card's right edge, so it never leaves the page.
- **1000 and under**: one column, the words, then the desk full width with the deep field running edge to edge; the card and launcher keep to 620px.
- **640 and under**: the downloads go full width and wrap their labels; the Y hop stacks above the launcher, without its dashed line.
- **The journey**, drawn at the app's own size from its tokens and widgets: the Today card for the new match at Meridian Rail Systems (New match, Closes in 5 days; the score stamp 84 in `hired`, Job ad only under it; the title in Zilla Slab as the app sets it; Brussels, hybrid; the fit line "Strong fit, 84 of 100. All six checks pass."; three of its six checks with OK tags; Draft application… on Y, Skip on N, Later on L) and, under it, the launcher that Y opens: `/apply`, "Evaluate a posting, then draft and review documents", its multiline field holding `https://meridian.example/jobs/1`, Model, Spend at most 5.00, Claude will run `/apply https://meridian.example/jobs/1`, Run. A Y key cap with a dashed `deep line` and "opens the launcher" leads from the card to the launcher. The line is `apply_line` in `src/intray/application/compose.rs`; the field, budget and labels come from `/apply` in `src/runs/domain/catalog.rs` and `src/runs/ui/launcher.rs`. The fictional job is `meridian` in `tests/sample_workspace/job_scraper/seen_jobs.json`.
- **Caption**: "A new match on Today, and the launcher its Y opens. Drawn from the app's own parts, on fictional data."
- **Phone**: the drawing reflows, not scales: the checks go to one column (a container query at 470px), the actions wrap, and every line stays at its desktop size, about 14 to 16px.
- **Light and dark**: the card and launcher take the page's tokens, so they follow the theme with no extra images.

**New widgets** (canvas only, for the site): the launcher's form panel (`.lx`: `panel`, `line`, `field`, `muted`, `text`, `primary`, `on_primary`, Courier Prime for the command and the line, `INSET`, `HEADER_PADDING`, `BODY_PADDING`, `ROW_GAP`, `CODE_PADDING`, `BUTTON_HEIGHT`), the Today card (`.tc`: `card`, `line`, `active` for the kind, `hired` for the stamp and OK tags, `primary` for Draft application…, `CARD_TAB_PADDING`, `KEY_SIZE`) and the Y hop (`deep text`, `deep line`). They are drawings of existing app widgets, not new app widgets.

## Sections

Schibsted Grotesk at weight 700 for every heading. Plain `sheet` above, no grid.

1. **Menu**: sticky, on `sheet` with a `line` hairline under it: the icon and name, Features, How it works, Your API key, Trust, Guide, Source, then Download.
2. **Hero**: above.
3. **A desk for ai‑job‑search.**: who does the work and what Jobdesk is, then a 12-column grid of cells in four sizes, each a snapshot cropped where it says most: the Jobs shortlist, a job's checks, the Tasks band and question, Applications' lanes, two text cells (dossiers, Profile) and Usage statistics. This section is the page's first real screen.
4. **Your commands, your Claude Code CLI, your key.**: on the deep field, three numbered steps, because they are a sequence: you pick a command, your Claude Code runs it on your key or its own sign-in, your files change and the screens follow. A line joins the steps and draws once as they come into view; in one column a short line runs from each number to the next.
5. **Bring your own Anthropic API key.**: see [Your own API key](#your-own-api-key).
6. **You stay in charge of every run.**: a two-column spec sheet of eight facts (one column per fact under 1200), then one pulled line: "The app never sends an application for you."
7. **Download**: "Download Jobdesk.", two panels with their buttons on the last line, "Both buttons open the latest release on GitHub.", the requirements, then All releases, User guide, Changelog.
8. **Footer**: Source on GitHub, All releases, User guide, Changelog, MIT License, then "A desktop app for ai‑job‑search, built in Rust. Not made or endorsed by Anthropic."

The page shows no failure case. The check tags OK, Unclear and Fails stay, because they are a job's result, not the app failing.

## Your own API key

The user asked for the bring-your-own-key option to be prominent. The page makes no legal claim. The key appears in seven places:

1. The menu link Your API key.
2. The hero key line (C4), linking to the key section with "How the key is kept".
3. The How it works lede: "your own Anthropic API key or Claude Code's sign-in".
4. How it works step 2: "on your own API key or Claude Code's sign-in".
5. The key section: its heading, its lede, the disclosure's five facts and the Providers screenshot.
6. Trust's Sign-in row: "Tasks use your API key or Claude Code's own sign-in."
7. The download requirements line: "an Anthropic API key or Claude Code's sign-in".

Wherever the page names both ways to run tasks (2, 3, 4, 6, 7), the key comes first. The hero requirements line does not name the key (see Round 2).

The section, two columns, centred on each other: left, the heading, then "Paste a key from your Claude Console in Settings › Providers. Tasks then run on that key and bill its Console account." and, weight 600, "Jobdesk keeps it in your system keychain and hands it only to Claude Code." Under it a native `<details>` between two `line` hairlines, summary "What Jobdesk does with your key", closed by default, holding the five facts and "The full account, with what the key is still exposed to: The API key, in Trust", linking to `docs/architecture/trust.md#the-api-key`. Right, the API key panel of Settings › Providers from `settings_key_saved_*`, with a fictional key shown in part. One column under 1000.

The five facts match `AGENTS.md`'s hard rule and `trust.md` (The API key):

- **Stored only in your keychain**: the macOS Keychain or the Windows Credential Manager, never a file. Read from there, held in memory until quit or Remove.
- **Passed only to Claude Code**: in the `claude` process's environment. Never on a command line or in a log, and never on screen in full.
- **Shown only in part**: `sk-ant-`, dots and the last four characters. Only the key being typed shows in full, while the user presses Show.
- **Tested before it is used**: one fixed question, no tools, an empty temporary folder, capped at $0.01. Tasks switch to the key only once the test passes.
- **Yours to remove**: Remove deletes it from the keychain and puts tasks back on Claude Code's sign-in.

The page does not say the key is unreadable by other programs: `trust.md` accepts that a process of the same user can read it from `claude`'s environment, and the disclosure links there.

## Copy

All facts come from `README.md` and `docs/guide.md`, the key's from `trust.md`, and the hero journey from the code named above. No testimonials, counts, prices, ratings or logos.

- **Headline**: "Pick the job. See the command. Press Run."
- **Credit line**: "Jobdesk is the desktop app for ai‑job‑search users: a GUI over its slash commands, run through your own Claude Code."
- **Hero key line**: "Run tasks on your own Anthropic API key, kept only in your system keychain, or on Claude Code's own sign-in." then the link "How the key is kept" to `#key`.
- **Hero requirements**: "Needs an Apple silicon Mac or 64-bit Windows, the Claude Code CLI and an ai‑job‑search checkout." then Setup steps.
- **Features head**: "A desk for ai‑job‑search." Lede unchanged: it credits the project before Jobdesk.
- **Credit**: ai‑job‑search links to `https://github.com/ai-job-search-dev/ai-job-search` in the hero's credit line, the hero requirements, the features lede, the download requirements and the footer.
- **Links**: downloads go to `https://github.com/Nicolas-Widart-SRL/jobdesk/releases/latest`, source to `https://github.com/Nicolas-Widart-SRL/jobdesk`, the guide to `.../blob/main/docs/guide.md`, the changelog to `.../blob/main/CHANGELOG.md`, Setup steps to `https://github.com/Nicolas-Widart-SRL/jobdesk#install`, and the key's details to `.../blob/main/docs/architecture/trust.md#the-api-key`.
- **Download buttons**: "Download for macOS (Apple silicon)" and "Download for Windows (x64)". Both open the latest release, and a line says so: "Both buttons open the latest release on GitHub." In the hero, View the source follows it.
- **Installers** keep `vX.Y.Z`. macOS says Apple silicon, signed with a Developer ID and notarized by Apple. Windows says the installer is not signed yet, and to click More info, then Run anyway.
- **Usage cell**: "Usage statistics show tokens, cache hits and each task's cost, from Jobdesk's own run logs on your computer. The cost is Claude Code's estimate at API list price: on a Claude plan, it is not your bill. Set a monthly cap in Settings."
- **How it works head**: "Your commands, your Claude Code CLI, your key."
- **Console**: "Claude Console", never "Anthropic Console". The key is still an "Anthropic API key".
- "ai‑job‑search" uses non-breaking hyphens in running text.
- **Tool permissions** row, unchanged: it states the risk as `README.md`'s Privacy and trust does.
- The command chips in How it works sit under "Some of the commands".
- "Your own Claude Code" wording only, no Anthropic or Claude logo, and no Claude plan or its limits named. The footer says "Not made or endorsed by Anthropic."

## Rules

- **Colours are palette tokens**, named as in `palette.rs`: `sheet`, `panel`, `raised`, `line`, `field`, `text`, `muted`, `primary`, `on_primary`, `active`, `turn`, `on_turn`, `grid`, `card`, and `hired` for the card's stamp and OK tags. Light uses the LIGHT set and dark the DARK set. The deep field is the DARK `sheet` and `grid` under the light theme and the DARK `field` under the dark theme, with the DARK `text`, `muted` and `line` on it.
- **Contrast** follows the pairs `palette.rs` tests. Buttons are `on_primary` on `primary`. Focus is a 2px `active` ring (`FOCUS_RING`), the DARK `active` on the deep field.
- **One magenta**: `turn` appears only inside the snapshots, where the app puts it. No headline word is highlighted.
- **Type**: Schibsted Grotesk for the page, Zilla Slab for the brand name and the drawn card's title, as the app sets it, and Courier Prime Bold only for commands, file names, installer names, `claude` and `sk-ant-`. All three load from `fonts/` with `font-display: swap`. No CDN, tracker or analytics.
- **No stock hero**: cards and panels lie flat, with a solid offset shadow in the DARK `line`, no tilt and no grey blur.
- **Screens**: every screen image below the hero is a committed snapshot from `tests/snapshots/`, fictional data from `tests/sample_workspace`, cropped by CSS variables. The hero's card and launcher are HTML drawings of app widgets, on fictional data, and the caption says so.
- **Motion**: the hero's card rises and the launcher slides in after it, and the step line draws once. `prefers-reduced-motion` stops both.
- **Accessibility**: one visible `h1`, `h2` per section, `h3` inside. A skip link. The card and the launcher are each one `role="img"` with an `aria-label` that says what it shows; the Y hop is `aria-hidden`, since both labels say what Y does. The disclosure is a native `<details>` with a visible summary and focus ring.
- **Phone**: one column, the menu keeps only the brand and Download, the downloads go full width and wrap their labels, and the hero's drawing reflows, never scaled down.
- **Hard rules, stated as they are**: card actions never start a run, Y opens the launcher and the user presses Run there, one run at a time, Jobdesk offers no login of its own, and tool permissions are allowed without asking by default, with prompts in Settings › Permissions.

## Decided

- **The design**: round 1's page, approved by the user. Round 2 changes only what this file lists.
- **Group C**: the user agreed to C1 to C4 and the copy fixes above. C4's line and its rules come from the user's terms check.
- **Hero B**: the user chose this hero, "Pick the job. See the command. Press Run.", with the journey drawn on the deep field.
- **Drawn, not snapshots**: the site ships the HTML drawing of the card and the launcher. No new app snapshots are added for the hero.
- **Hero requirements**: the line without the key, as above.
- **Courier Prime** stays for slash commands, file names and installer names.
- **Address**: the default `https://nicolas-widart-srl.github.io/jobdesk/`, no custom domain.
- **Guide and changelog**: linked in the menu (Guide), the download row and the footer.

## What a build needs

The builder brings `site/index.html` in line with the canvas. Everything below the hero already matches the canvas except the C changes listed here.

- **Hero**: replace the site's hero (the line "The desktop app for your ai‑job‑search commands.", the Today window and the `/scrape` launcher on `.stage`) with hero B: the `.split` of words and desk, the `.tc` card, the `.hop` with the Y key cap and the `.lx` launcher, and the caption, with the copy and the 1440, 1200, 1000 and 640 layouts in [The hero](#the-hero). Copy the `.lx`, `.tc`, `.yhop`, `.desk` and `@container` rules from the canvas, and the rise and slide animations under `prefers-reduced-motion: no-preference`.
- **Hero key line**: "Run tasks on your own Anthropic API key, kept only in your system keychain, or on Claude Code's own sign-in." with the link "How the key is kept" to `#key`, in place of "Bring your own Anthropic API key, stored only in your system keychain. How".
- **Hero requirements**: "Needs an Apple silicon Mac or 64-bit Windows, the Claude Code CLI and an ai‑job‑search checkout." then Setup steps, in place of the line that names the key.
- **The 1440 layout**: the two-column split at 1440 to 1024 replaces the old intro grid, so the downloads stack in the words column and the hero no longer pushes anything down.
- **The 500 to 640 field fix**: drop `.stage`, `.stage::before` and its 640 override (`top: auto; height: 300px`). The deep field is the `.desk::before` background, `inset: 0 -50vw 0 0` beside the words and `inset: 0 -50vw` at 1000 and under, so it starts at the desk's top at every width.
- **Key section**: fold the five facts into `<details>` with the summary "What Jobdesk does with your key", closed by default, and end it with "The full account, with what the key is still exposed to:" and the link "The API key, in Trust" to `https://github.com/Nicolas-Widart-SRL/jobdesk/blob/main/docs/architecture/trust.md#the-api-key`. The lede becomes the canvas's lede, with "Claude Console" and the weight-600 sentence.
- **Console**: "Claude Console" instead of "Anthropic Console" everywhere on the page.
- **Usage cell**: the Usage line in [Copy](#copy), which says the figure is Claude Code's estimate at API list price, not your bill on a Claude plan.
- **How it works heading**: "Your commands, your Claude Code CLI, your key."
- **Images**: the hero needs no image. Delete `site/img/today-*.webp` and `site/img/launcher-*.webp` once nothing references them. The other images stay WebP at the shown size, with `width` and `height` set, all lazy.
- **Switches**: the site keeps only `?theme=`; it takes none of the canvas's `?motion=` or `?measure=`.
- **Copy tracks the docs**: a change to `README.md`, `docs/guide.md` or `trust.md` that the page states changes the page too. The hero journey tracks `compose.rs`, `catalog.rs` and `launcher.rs`.
- **Signing**: the Windows "not signed yet" lines change once the installer is signed.

## Doc changes

- `README.md` (Privacy and trust, and Authentication) and `docs/guide.md` (Settings › Providers) still say "Anthropic Console" for where the key comes from; the page says "Claude Console". Change them to "Claude Console".
- `docs/guide.md`'s Usage statistics entry does not say the cost is Claude Code's estimate at API list price and is not the bill on a Claude plan, which the page states. Add it there.
