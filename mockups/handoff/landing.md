# Landing page

A Jobdesk landing page for GitHub Pages, as clean and engaging as a Stripe or Apple product page. Two options, A and B, drawn as full pages in `mockups/landing/`. Neither is approved yet.

- [Canvases](#canvases)
- [Option A, "The Desk"](#option-a-the-desk)
- [Option B, "The Product"](#option-b-the-product)
- [Rules for both](#rules-for-both)
- [Copy](#copy)
- [What a build needs](#what-a-build-needs)
- [Open](#open)

## Canvases

Each canvas is a plain static page: no `support.js`, no build step, and every path relative, so it opens from the file system as it would from Pages. Both read `?theme=light|dark` (the system theme otherwise) and `?motion=off`, which shows every moving part at rest. Render with `--file-access`, so the bundled fonts and the snapshots load, and always pass `theme=` and `motion=off`, because headless Chrome follows the Mac's own theme and stops the clock before the hero settles.

- Desktop: `scripts/render-canvas.sh --size 1440,900 --file-access mockups/landing/A.html mockups/landing/A-hero.png theme=light motion=off`. The full page uses the page's own height: 5587 for A and 5543 for B.
- Phone: headless Chrome makes no window under 500px wide, so `phone.html` shows a page in a 390px frame. Render it at `--size 500,844` with `page=A` (or `B`), `theme=light`, `motion=off` and `h=844`, then crop the middle with `sips -c 844 390 <png> --out <png>`. `phone.html` is a drawing aid, not part of the site.

PNGs per option: `-hero` (1440x900, light), `-hero-dark`, `-full` (1440 wide, the whole page, light) and `-phone` (390x844).

## Option A, "The Desk"

Apple-style storytelling in the app's own world. The gridded desk runs behind the whole page and holds still while it scrolls, as it does in the app. One family of objects carries every section: the card, the stamp and the paper.

1. **Menu**: the icon and name, then How it works, Screens, Source and Download, each opaque on the grid.
2. **Hero**: the headline centred in Zilla Slab at up to 96px, one line under it, two download buttons. Below, the Today deal at 1.26x: the Ready to send card, its stack edges, Coming up tucked behind with its Now chip, and the day stamp struck across the corner. On load the card is dealt in, then the stamp strikes. That is the page's one orchestrated moment.
3. **Four keys. One card on top.**: the four keys as big keycaps with what each does, beside a working deck of three fictional cards. Click a keycap, or focus the deck and type Y, L, N or U. Y lifts the card and shows the launcher with the card's command line and an inert Run. L sets it aside, N skips it, U brings back the last one. Each decision fills a tick on the day stamp and lands in Done today. Without JS the keys read as a list beside one static card.
4. **Your files, laid out on one desk.**: Jobs, Applications and Tasks as full snapshots on paper with the app's solid offset shadow, alternating sides, each with a heading and two sentences.
5. **Nothing runs until you start it.**: a band of `card` paper. Four facts on the left (the launcher, one run at a time, sign-in, permissions) and the launcher's side panel on the right, cropped from its snapshot, with Run in view.
6. **Download**: "Put your job search on a desk.", the two requirements, and a paper panel each for macOS and Windows with the installer's name, how to open it, and its signing.
7. **Footer**: source, releases, licence, "Built in Rust. Not made or endorsed by Anthropic."

Trade-offs: the most distinctive and the closest to the app and the README hero, and the deck demo makes the core idea tangible in seconds. It shows fewer screens, and its warmth costs some of the "serious product" read.

## Option B, "The Product"

Stripe-style precision. Plain `sheet` above, no grid, and Schibsted Grotesk at weight 700 for every heading. The app's own dark desk is the deep field the product sits on.

1. **Menu**: sticky, on `sheet` with a `line` hairline under it: the icon and name, Features, How it works, Trust, Source, Download.
2. **Hero**: the headline across the full width, "The desktop app for your ai‑job‑search commands.", then the line and the buttons side by side. Below, the Today snapshot as a flat window, crossing from `sheet` onto the deep field, with the launcher's side panel layered over its right edge. On load the window rises and the launcher slides in after it.
3. **Every screen reads your files.**: a 12-column grid of cells in four sizes, each a snapshot cropped where it says most, bleeding off the cell's lower right: the Jobs shortlist, a job's checks, the Tasks band and question, Applications' lanes, two text cells (dossiers, Profile) and Usage statistics.
4. **How it works**: on the deep field, three numbered steps, because they are a sequence: you pick a command (the command chips), your Claude Code runs it (the line Claude will run), your files change and the screens follow (the files `/apply` writes). A line joins the steps and draws once as they come into view.
5. **You stay in charge of every run.**: a two-column spec sheet of eight facts (starting a run, one at a time, sign-in, permissions, what it writes, your data, updates, signing), then one pulled line: "The app never sends an application for you."
6. **Download**: two panels, macOS and Windows, then the requirements and All releases.
7. **Footer**: as A.

Trade-offs: reads as a mature product at a glance and shows the most of the app. It leans on screenshots, so it is more generic in structure than A, and the crops need re-cutting whenever the screens change.

## Rules for both

- **Colours are palette tokens**, named as in `palette.rs`: `sheet`, `panel`, `raised`, `line`, `field`, `text`, `muted`, `primary`, `on_primary`, `drafted`, `active`, `turn`, `on_turn`, `grid`, `card`, `off`. Light uses the LIGHT set and dark the DARK set, swapped by `prefers-color-scheme` and `?theme=`. B's deep field is the DARK `sheet` and `grid` under the light theme, and the DARK `field` under the dark theme, with the DARK `text`, `muted` and `line` on it. No colour is outside the palette.
- **Contrast** follows the pairs `palette.rs` tests: every text ink sits on `sheet`, `card` or `panel` of its own theme, or on the DARK desk in its DARK ink, all at 4.5:1 or more. Buttons are `on_primary` on `primary`. Focus is a 2px `active` ring (`FOCUS_RING`), the DARK `active` on the deep field.
- **One magenta**. `turn` appears only where the app itself puts it: A's Now chip in Coming up, and inside the snapshots (tab counts, Your turn, the pink edges). No headline word is highlighted.
- **Type**: Zilla Slab for A's headings and both brand names, Schibsted Grotesk for everything else, and Courier Prime only for commands, file names and installer names, as in the app (`look.md`). All three load from `assets/fonts` with `font-display: swap`. No CDN, tracker or analytics.
- **No stock hero**: no tilted window, no grey blur. A deals the app's own objects with its solid offset shadow. B shows the window flat, with a solid offset shadow in the DARK `line`.
- **Real screens only**: every screen image is a committed snapshot from `tests/snapshots/`, fictional data from `tests/sample_workspace`. A crop sets the snapshot's pixel box as CSS variables, so a re-cut is one line. The 12px margin round each snapshot is cropped off.
- **Motion** answers the page's job: the deal and strike in A, the window and launcher in B, the step line in B, and A's deck responding to keys. `prefers-reduced-motion` stops all of it, and the page then shows each part at rest.
- **Accessibility**: one `h1`, `h2` per section, `h3` inside, in order. A skip link. Every snapshot has alt text that says what it shows. The hero desk in A is one image with a description. The deck's keys are real buttons, its keyboard handler listens only while the deck has focus, and an `aria-live` line says what each key did.
- **Phone**: at 390 wide every grid becomes one column, buttons go full width, A drops the slip, and B's launcher overlaps the window's lower right instead of its edge. No horizontal scroll at 390 or 1440 (measured: the page's scroll width equals the window's).
- **Hard rules, stated as they are**: card actions never start a run, Y opens the launcher and the user presses Run there, one run at a time, sign-in stays with the user's Claude Code, and tool permissions are allowed by default and listed, with prompts in Settings › Permissions. A's deck starts, sends and stores nothing.

## Copy

All facts come from `README.md` and `docs/guide.md`. No testimonials, counts, prices, ratings or logos.

- A headline: "Your job search, one decision at a time.", the README hero's. Line: "A macOS and Windows app for the ai‑job‑search commands. It runs them through your own Claude Code, and nothing runs until you start it."
- B headline: "The desktop app for your ai‑job‑search commands." Line: "Jobdesk deals your decisions one card at a time, keeps every job and application in view, and runs any slash command through your own Claude Code CLI. Nothing runs until you start it."
- "ai‑job‑search" uses non-breaking hyphens in running text, so it never breaks across lines.
- Download buttons say "Download for macOS" and "Download for Windows" and go to `https://github.com/Nicolas-Widart-SRL/jobdesk/releases/latest`. Source links go to `https://github.com/Nicolas-Widart-SRL/jobdesk`.
- Installer names keep `vX.Y.Z`, as the README does. macOS says Apple silicon, signed with a Developer ID and notarized by Apple. Windows says the installer is not signed yet, and to click More info, then Run anyway.
- "Uses your Claude Code" wording only. The footer says "Not made or endorsed by Anthropic.", as the README does.

## What a build needs

- **Where**: a `site/` folder at the repo root, published by a Pages workflow that uploads it as is (`actions/upload-pages-artifact`, then `actions/deploy-pages`), on pushes to `main` that touch `site/`. No build step.
- **What it copies** into `site/`: the chosen canvas as `index.html`, with its paths changed from `../../` to local ones; `assets/fonts/ZillaSlab-SemiBold.ttf`, `SchibstedGrotesk-Variable.ttf` and `CourierPrime-Bold.ttf` with their OFL texts; `assets/brand/jobdesk-icon-small.svg` for the icon and favicon; and the snapshots the page shows, as copies (A: `jobs`, `tracker`, `run_turn`, `launcher`, light and dark; B: `intray`, `launcher`, `jobs`, `run_turn`, `tracker`, `settings_usage`, light and dark). A check could compare the copies with `tests/snapshots/` so the site never shows a stale screen.
- **Image weight**: the snapshots are 1920x1230 PNGs. A build may convert them to WebP or AVIF at the shown size, with `width` and `height` set, and keep the hero image eager and the rest lazy.
- **Social card**: `assets/brand/jobdesk-social.png` as `og:image`, with `og:title` and `og:description` from the page's own title and line.
- **Not in the canvas**: the `?theme=` and `?motion=` switches are for renders. A build may keep `?theme=` and drop `?motion=`.

## Open

- Pick A or B, or a mix. A natural mix is A's hero and deck with B's feature grid and spec sheet.
- Domain: `nicolas-widart-srl.github.io/jobdesk`, or a custom domain.
- Courier Prime: kept for commands and installer names as in the app. The brief named two fonts. Drop it for Schibsted Grotesk if you prefer.
- A's deck shows three fictional cards. Keep it, or show a static card for a quieter page.
- Whether the page links the user guide (`docs/guide.md` on GitHub) and the changelog.
- The Windows note says the installer is not signed yet. Update the copy when that changes.
