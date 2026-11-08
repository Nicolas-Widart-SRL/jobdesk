# Landing page

A Jobdesk landing page for GitHub Pages, as clean and engaging as a Stripe or Apple product page. The user approved "The Product" in round 1: Stripe-style precision on plain `sheet`, with the app's own dark desk as the deep field the product sits on. Round 2 applies the review's group C (hero copy, hero image, a shorter key section, the hero key line) and the approved copy fixes, and draws two hero options, A and B. Everything below the hero is shared.

- [Canvas](#canvas)
- [Round 2: what changed and why](#round-2-what-changed-and-why)
- [Hero option A](#hero-option-a)
- [Hero option B](#hero-option-b)
- [Sections](#sections)
- [Your own API key](#your-own-api-key)
- [Copy](#copy)
- [Rules](#rules)
- [Decided](#decided)
- [What a build needs](#what-a-build-needs)
- [Doc changes](#doc-changes)
- [Open](#open)

## Canvas

`mockups/landing/Landing.html` is a plain static page: no `support.js`, no build step, and every path relative, so it opens from the file system. Its markup and CSS follow `site/index.html` as built at the start of round 2 (the tablet and phone fixes, the stepped connector, the 1200 breakpoint, `.stage::before` for the deep field, the download labels and lines), with the canvas's `../../` paths and render switches kept. Render switches, for renders only:

- `?hero=a|b` picks the hero option, `a` by default. Both heroes sit in the markup; the other one is hidden.
- `?theme=light|dark` overrides the system theme. `?motion=off` shows every moving part at rest. Always pass both, because headless Chrome follows the Mac's theme and stops the clock before the hero settles.
- `?details=open` opens the key's disclosure. `?measure=1` adds a badge with the page's scroll width, the window's width and the page's height, for checks; never commit a render with it.

Render with `scripts/render-canvas.sh --file-access`, so the bundled fonts and the snapshots load. For each option `X` in `A` and `B` (query `hero=a` or `hero=b`):

- `Landing-X-hero.png` and `Landing-X-hero-dark.png`: `--size 1440,900`, `theme=light` or `theme=dark`, `motion=off`.
- `Landing-X-hero-1024.png`: `--size 1024,900`, light.
- `Landing-X-phone.png`, 390x1760, light, tall enough to show the whole hero: render `mockups/landing/phone.html` at `--size 500,1920` with `h=1760 top=80`, then `sips -c 1760 390`. Headless Chrome makes no window under 500px wide, so `phone.html` frames the page at 390 and passes `hero`, `theme`, `motion`, `details` and `measure` through. It is a drawing aid, not part of the site.
- `Landing-full.png`: the whole page with hero A at `--size 1440,6724`, the page's own height (`?measure=1` reads it). Measure again when the page changes length.

Checked at 1440, 1280, 1024, 768, 560, 390, light and dark: the page's scroll width equals the window's for both options.

## Round 2: what changed and why

- **C1, hero copy.** The old line named the integration before the value ("The desktop app for your ai‑job‑search commands."). Both options now lead with what the user gets, then say in a smaller line that Jobdesk is the desktop app for ai‑job‑search users, a GUI over its slash commands, with ai‑job‑search linked.
- **C2, hero image.** The old hero paired a Ready to send card with a `/scrape` launcher, a different action. Each option now shows one journey: a Today card and the launcher its own Y opens, with that card's line under Claude will run. A uses the real Today window and draws the launcher in HTML. B draws both in HTML. See the two options for why, and [What a build needs](#what-a-build-needs) for the snapshots that would replace the drawings.
- **C3, the key section.** It was long and slowed the story. The heading and the Providers screenshot stay as large as before; the lede now carries the point; the five facts fold into a `<details>`, with a link to `docs/architecture/trust.md#the-api-key`. Trust's Tool permissions row stays as it was.
- **C4, the hero key line.** "Run tasks on your own Anthropic API key, kept only in your system keychain, or on Claude Code's own sign-in." with the link "How the key is kept" to the key section. The key comes first, Claude Code's own sign-in is named as the other way, the key is never called optional or required, and no plan or its limits is named.
- **Hero requirements.** Now "Needs an Apple silicon Mac or 64-bit Windows, the Claude Code CLI and an ai‑job‑search checkout." The key line right above already names both ways to run tasks, so saying it twice in one block slowed the hero. The CLI stays a plain requirement.
- **Copy fixes.** "Anthropic Console" becomes "Claude Console". The Usage cell no longer says the statistics count spend: the cost is Claude Code's estimate at API list price, and on a Claude plan it is not your bill; the monthly cap stays. How it works is now "Your commands, your Claude Code CLI, your key."
- **1440 made deliberate.** In the site the two long download labels stacked at full column width and pushed the window about 90px lower. A now sets the line in the left 8 of 12 columns with the downloads stacked in a 4-column right rail that ends on the line's baseline; the rail continues under a `line` hairline with the key line and the requirements. The window starts about 35px lower than the round 1 canvas, on purpose, for the key line's new length. B splits the hero in two at 1440 and needs no stacking trick.
- **500 to 640 wide.** The site put the deep field's top 300px above the stage's bottom, so it started below the window when the window was taller. The field now starts at a fixed share of the card's own height, worked out from the viewport width, so it sits behind the card at any phone width.

## Hero option A

**"Your job search, one decision at a time."**, the README's line. Lede: what Today deals (a new match to draft, an application to send, an offer to answer), then, in `text` weight 600, "Press Y and the launcher shows the line Claude will run. Nothing runs until you press Run."

- **Layout at 1440 and 1280**: line left (8 columns), downloads stacked right (4 columns) on the line's baseline, then the lede and credit line left, the key line and requirements right under a hairline. Then the window across the edge of the deep field, as in round 1.
- **1100 and under**: the line spans both columns, the lede sits left and the key rail right, and the downloads run side by side under both. **1000 and under**: one column, in reading order: line, lede, downloads, key line, requirements.
- **The journey**: the real `intray_*` snapshot, the Harbor Bank application Ready to send with Mark as sent… on Y, and over its right edge the launcher that Y opens: `/outcome`, its field holding `harbor bank`, Model, Spend at most 2.00, Claude will run `/outcome harbor bank`, Runs in your job search folder, Run. The card's own line comes from `send_card` in `src/intray/application/compose.rs`. The caption says the app never sends for you, which this card is about.
- **Phone**: not the window scaled down. Two closer crops of the same snapshot: the card's head (tab, company, title, documents, the take) and its action bar, with the card's empty middle cut out; the launcher overlaps their lower right at 88% width. The card's title reads at about 20px, its small lines at about 11px.
- **Light and dark**: both snapshots exist (`intray_light`, `intray_dark`), so the window follows the theme.
- **Trade-off**: real pixels, so the window cannot drift from the app, and the hero keeps round 1's approved composition. The cost: on a phone the card's small text is at the edge of readable, and the launcher is a drawing until a builder adds its snapshot.

## Hero option B

**"Pick the job. See the command. Press Run."** Three short sentences that are the journey itself. Lede: "Your job search comes to you one decision at a time." then, weight 600, "Y on a new match opens the launcher with the line Claude will run, and Claude drafts your application only once you press Run."

- **Layout at 1440 to 1024**: two columns, words left (5 of 11 parts) and a desk right (6 parts): the deep field fills the right half from the menu to the hero's bottom and bleeds off the page's right edge. The hero fills the window's height, up to 860px. The words carry the downloads stacked, the fine line, then under a hairline the key line and the requirements. **1000 and under**: the words, then the desk full width.
- **The journey**, drawn at the app's own size from its tokens and widgets: the Today card for the new match at Meridian Rail Systems (New match, Closes in 5 days; the score stamp 84 in `hired`; the title in Zilla Slab as the app sets it; the fit line; three of its six checks with OK tags; Draft application… on Y, Skip on N, Later on L) and, under it, the launcher that Y opens: `/apply`, its multiline field holding `https://meridian.example/jobs/1`, Model, Spend at most 5.00, Claude will run `/apply https://meridian.example/jobs/1`, Run. A Y key cap with a dashed `deep line` leads from the card to the launcher. The line is `apply_line` in `src/intray/application/compose.rs`; the field, budget and labels come from `/apply` in `src/runs/domain/catalog.rs` and `src/runs/ui/launcher.rs`. The fictional job is `meridian` in `tests/sample_workspace/job_scraper/seen_jobs.json`.
- **Phone**: the drawing reflows, not scales: the checks go to one column (a container query at 470px), the actions wrap, and every line stays at its desktop size, about 14 to 16px.
- **Trade-off**: sharp and readable at every width and theme, and it tells the drafting story, the reason people use ai‑job‑search. The cost: it is a drawing, not a screenshot, so it can drift from the app and breaks round 1's "real screens only" rule for the hero; the caption says "Drawn from the app's own parts". It also moves the window out of the hero, so the first real screen is the Features grid.

**New widgets** (canvas only, for the site): the launcher's form panel (`.lx`: `panel`, `line`, `field`, `muted`, `text`, `primary`, `on_primary`, Courier Prime for the command and the line, `INSET`, `HEADER_PADDING`, `BODY_PADDING`, `ROW_GAP`, `CODE_PADDING`, `BUTTON_HEIGHT`) in both options, and in B the Today card (`.tc`: `card`, `line`, `active` for the kind, `hired` for the stamp and OK tags, `primary` for Draft application…, `CARD_TAB_PADDING`, `KEY_SIZE`) and the Y hop (`deep text`, `deep line`). They are drawings of existing app widgets, not new app widgets.

## Sections

Schibsted Grotesk at weight 700 for every heading. Plain `sheet` above, no grid.

1. **Menu**: sticky, on `sheet` with a `line` hairline under it: the icon and name, Features, How it works, Your API key, Trust, Guide, Source, then Download.
2. **Hero**: option A or B, above.
3. **A desk for ai‑job‑search.**: who does the work and what Jobdesk is, then a 12-column grid of cells in four sizes, each a snapshot cropped where it says most: the Jobs shortlist, a job's checks, the Tasks band and question, Applications' lanes, two text cells (dossiers, Profile) and Usage statistics.
4. **Your commands, your Claude Code CLI, your key.**: on the deep field, three numbered steps, because they are a sequence: you pick a command, your Claude Code runs it on your key or its own sign-in, your files change and the screens follow. A line joins the steps and draws once as they come into view; in one column a short line runs from each number to the next.
5. **Bring your own Anthropic API key.**: see [Your own API key](#your-own-api-key).
6. **You stay in charge of every run.**: a two-column spec sheet of eight facts (one column per fact under 1200), then one pulled line: "The app never sends an application for you."
7. **Download**: "Download Jobdesk.", two panels with their buttons on the last line, "Both buttons open the latest release on GitHub.", the requirements, then All releases, User guide, Changelog.
8. **Footer**: Source on GitHub, All releases, User guide, Changelog, MIT License, then "A desktop app for ai‑job‑search, built in Rust. Not made or endorsed by Anthropic."

The page shows no failure case. The check tags OK, Unclear and Fails stay, because they are a job's result, not the app failing.

## Your own API key

The user asked for the bring-your-own-key option to be prominent. The page makes no legal claim. The key appears in these places:

1. The menu link Your API key.
2. The hero key line (C4), linking to the key section.
3. The How it works lede: "your own Anthropic API key or Claude Code's sign-in".
4. How it works step 2: "on your own API key or Claude Code's sign-in".
5. The key section: its heading, its lede, the disclosure's five facts and the Providers screenshot.
6. Trust's Sign-in row: "Tasks use your API key or Claude Code's own sign-in."
7. The download requirements line: "an Anthropic API key or Claude Code's sign-in".

Wherever the page names both ways to run tasks (2, 3, 4, 6, 7), the key comes first. The hero requirements line no longer names the key (see Round 2).

The section, two columns, centred on each other: left, the heading, then "Paste a key from your Claude Console in Settings › Providers. Tasks then run on that key and bill its Console account." and, weight 600, "Jobdesk keeps it in your system keychain and hands it only to Claude Code." Under it a disclosure between two `line` hairlines, "What Jobdesk does with your key", closed by default, holding the five facts and "The full account, with what the key is still exposed to: The API key, in Trust". Right, the API key panel of Settings › Providers from `settings_key_saved_*`, with a fictional key shown in part. One column under 1000.

The five facts match `AGENTS.md`'s hard rule and `trust.md` (The API key):

- **Stored only in your keychain**: the macOS Keychain or the Windows Credential Manager, never a file. Read from there, held in memory until quit or Remove.
- **Passed only to Claude Code**: in the `claude` process's environment. Never on a command line or in a log, and never on screen in full.
- **Shown only in part**: `sk-ant-`, dots and the last four characters. Only the key being typed shows in full, while the user presses Show.
- **Tested before it is used**: one fixed question, no tools, an empty temporary folder, capped at $0.01. Tasks switch to the key only once the test passes.
- **Yours to remove**: Remove deletes it from the keychain and puts tasks back on Claude Code's sign-in.

The page does not say the key is unreadable by other programs: `trust.md` accepts that a process of the same user can read it from `claude`'s environment, and the disclosure links there.

## Copy

All facts come from `README.md` and `docs/guide.md`, the key's from `trust.md`, and the hero journeys from the code named above. No testimonials, counts, prices, ratings or logos.

- **Headline**: option A or B, above.
- **Credit line**, both options: "Jobdesk is the desktop app for ai‑job‑search users: a GUI over its slash commands, run through your own Claude Code."
- **Features head**: "A desk for ai‑job‑search." Lede unchanged: it credits the project before Jobdesk.
- **Credit**: ai‑job‑search links to `https://github.com/ai-job-search-dev/ai-job-search` in the hero's credit line, the hero requirements, the features lede, the download requirements and the footer.
- **Links**: downloads go to `https://github.com/Nicolas-Widart-SRL/jobdesk/releases/latest`, source to `https://github.com/Nicolas-Widart-SRL/jobdesk`, the guide to `.../blob/main/docs/guide.md`, the changelog to `.../blob/main/CHANGELOG.md`, Setup steps to `https://github.com/Nicolas-Widart-SRL/jobdesk#install`, and the key's details to `.../blob/main/docs/architecture/trust.md#the-api-key`.
- **Download buttons**: "Download for macOS (Apple silicon)" and "Download for Windows (x64)". Both open the latest release, and a line says so: "Both buttons open the latest release on GitHub." In the hero, View the source follows it.
- **Installers** keep `vX.Y.Z`. macOS says Apple silicon, signed with a Developer ID and notarized by Apple. Windows says the installer is not signed yet, and to click More info, then Run anyway.
- **Usage cell**: "Usage statistics show tokens, cache hits and each task's cost, from Jobdesk's own run logs on your computer. The cost is Claude Code's estimate at API list price: on a Claude plan, it is not your bill. Set a monthly cap in Settings."
- **Console**: "Claude Console", never "Anthropic Console". The key is still an "Anthropic API key".
- "ai‑job‑search" uses non-breaking hyphens in running text.
- **Tool permissions** row, unchanged: it states the risk as `README.md`'s Privacy and trust does.
- The command chips in How it works sit under "Some of the commands".
- "Your own Claude Code" wording only, no Anthropic or Claude logo, and no Claude plan or its limits named. The footer says "Not made or endorsed by Anthropic."

## Rules

- **Colours are palette tokens**, named as in `palette.rs`: `sheet`, `panel`, `raised`, `line`, `field`, `text`, `muted`, `primary`, `on_primary`, `active`, `turn`, `on_turn`, `grid`, `card`, and `hired` for B's stamp and OK tags. Light uses the LIGHT set and dark the DARK set. The deep field is the DARK `sheet` and `grid` under the light theme and the DARK `field` under the dark theme, with the DARK `text`, `muted` and `line` on it.
- **Contrast** follows the pairs `palette.rs` tests. Buttons are `on_primary` on `primary`. Focus is a 2px `active` ring (`FOCUS_RING`), the DARK `active` on the deep field.
- **One magenta**: `turn` appears only inside the snapshots, where the app puts it. No headline word is highlighted.
- **Type**: Schibsted Grotesk for the page, Zilla Slab for the brand name and, in B's drawn card, the card title as the app sets it, and Courier Prime Bold only for commands, file names, installer names, `claude` and `sk-ant-`. All three load from `assets/fonts` with `font-display: swap`. No CDN, tracker or analytics.
- **No stock hero**: windows and panels lie flat, with a solid offset shadow in the DARK `line`, no tilt and no grey blur.
- **Screens**: every screen image is a committed snapshot from `tests/snapshots/`, fictional data from `tests/sample_workspace`, cropped by CSS variables. The hero's launcher (A) and card and launcher (B) are drawings until their snapshots exist, and say so in their captions or alt text.
- **Motion**: the hero's window or card rises and the launcher slides in after it, and the step line draws once. `prefers-reduced-motion` stops both.
- **Accessibility**: one visible `h1`, `h2` per section, `h3` inside. A skip link. Each drawing is one `role="img"` with an `aria-label` that says what it shows; A's phone crops are `aria-hidden`, since the whole window's alt text says it. The disclosure is a native `<details>` with a visible summary and focus ring.
- **Phone**: one column, the menu keeps only the brand and Download, the downloads go full width and wrap their labels, and the hero shows close crops (A) or reflowed drawings (B), never the whole window scaled down.
- **Hard rules, stated as they are**: card actions never start a run, Y opens the launcher and the user presses Run there, one run at a time, Jobdesk offers no login of its own, and tool permissions are allowed without asking by default, with prompts in Settings › Permissions.

## Decided

- **The design**: round 1's page, approved by the user. Round 2 changes only what this file lists.
- **Group C**: the user agreed to C1 to C4 and the copy fixes above. C4's line and its rules come from the user's terms check.
- **Courier Prime** stays for slash commands, file names and installer names.
- **Address**: the default `https://nicolas-widart-srl.github.io/jobdesk/`, no custom domain.
- **Guide and changelog**: linked in the menu (Guide), the download row and the footer.

## What a build needs

- **Where**: `site/`, published as is by the Pages workflow. Keep the chosen hero only and drop the `?hero=`, `?details=`, `?measure=` and `?motion=` switches; `?theme=` may stay.
- **Hero A** copies `intray_*` at two more crops for the phone (the card's head, x 62 y 306 w 700 h 262, and its action bar, x 62 y 956 w 700 h 98, in the snapshot's pixels) and draws the `/outcome` launcher in HTML, or, better, a builder adds snapshots `launcher_outcome_light` and `launcher_outcome_dark` to `tests/screens.rs`: Today on the Harbor Bank Ready to send card, Y pressed, the launcher open on `/outcome harbor bank`. The site then crops their form panel.
- **Hero B** draws the card and the `/apply` launcher in HTML, or a builder adds `intray_triage_dark` (only the light one exists) and `launcher_apply_light` and `launcher_apply_dark` (the triage card's Y pressed, the launcher on `/apply https://meridian.example/jobs/1`) and the site uses crops: the drawing would then become screenshots, at the cost of B's phone readability.
- **Image weight**: convert the snapshots to WebP at the shown size, with `width` and `height` set, the hero image eager and the rest lazy.
- **Copy tracks the docs**: a change to `README.md`, `docs/guide.md` or `trust.md` that the page states changes the page too. The hero journeys track `compose.rs`, `catalog.rs` and `launcher.rs`.
- **Signing**: the Windows "not signed yet" lines change once the installer is signed.

## Doc changes

- `README.md` and `docs/guide.md` still say "Anthropic Console" for where the key comes from; the page now says "Claude Console". Align them, or the page, in one direction.
- `docs/guide.md`'s Usage statistics entry does not say the cost is Claude Code's estimate at API list price and is not a Claude plan's bill, which the page now states. Add it there.

## Open

- **A or B**: which hero ships. A keeps the real window and round 1's composition, with a closer but small-text phone crop and a drawn launcher. B tells the drafting journey in a sharp, reflowing drawing, at the cost of a drawn hero instead of a screenshot.
- **Snapshots or drawings**: for the chosen option, should a builder add the snapshots named in [What a build needs](#what-a-build-needs) so the hero shows real pixels, or ship the HTML drawing?
- Picked so far: group C, the C4 line and the copy fixes. Still undecided: the two points above.
