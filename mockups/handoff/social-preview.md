# Social preview

The GitHub social preview and the root README's hero, at GitHub's 1280x640. The README shows it about 830px wide in both page themes, and it can show as small as 400px wide. The built copy is `assets/brand/jobdesk-social.png`.

## Canvas

[Social.png](../Social.png), drawn from `Social.html`. It has one state and one theme, and no query string. Its rules are scoped to `.so`. It loads Zilla Slab and Schibsted Grotesk from `assets/fonts`, so it renders offline. Render it with `--window-size=1280,640 --hide-scrollbars --allow-file-access-from-files`, so the local fonts load.

Left: the app icon and name, the headline, one line saying what Jobdesk is, and four labels. Right: the Today desk at 1.25x: the deal card with its tab and stack edges, the Coming up slip behind it, and the day stamp across the card's corner.

## Copy

- **Headline**: "Your job search, one decision at a time.", broken after "search," and "decision" so each line holds one idea. The card beside it shows exactly that: one decision on top of a stack.
- **Line**: "A desktop app for the ai-job-search commands. Nothing runs until you start it." The first sentence says what it is. The second is the promise that matters most to someone handing their job search to an agent, and it is the hard rule behind Y.
- **Labels**: macOS, Windows, Built in Rust, Uses your Claude Code. They are facts, not features. "Uses your Claude Code" says it runs on the user's own install, with no Anthropic or Claude logo and no hint of endorsement.

## Decisions

- **Light only.** The image is opaque, on its own grid, so it needs no dark twin. Navy on paper sits well on both GitHub page themes, and GitHub's social preview takes one image anyway.
- **The app's objects on the desk, not a window screenshot.** A tilted window with a soft shadow is the stock hero. Here the card, the slip and the stamp lie on the grid the app paints, tilted 1 to 2 degrees like dealt papers, with the app's solid offset shadow, not a grey blur. The menu is left out, so the only mark is the app icon.
- **The stamp is struck across the card's top-right corner**, rotated -12 degrees, like a postmark. It is the one bold move, and it names the look.
- **One magenta**: the Now chip in Coming up, in `turn`, pointing at the same job as the card. Nothing else uses `turn` or `mark`. The headline gets no highlighter, because a highlighted word is the stock headline trick.
- **Sizes that hold at 400px**: the headline is 78px, so about 24px at 400 wide. The line is 25px and the labels 18px.
- **The safe margin**: everything stays 40px inside each edge.
