# Social preview

The Twitter card, the GitHub social preview and the root README's hero, at GitHub's 1280x640. The README shows it about 830px wide in both page themes, and it can show as small as 400px wide. The built copies are `site/jobdesk-social.png` and `assets/brand/jobdesk-social.png`.

## Canvas

[Social.png](../Social.png), drawn from `Social.html`. It has one state and one theme, and no query string. Its rules are scoped to `.so`. It loads Zilla Slab and Schibsted Grotesk from `assets/fonts`, so it renders offline. Render it with `scripts/render-canvas.sh --size 1280,640 --file-access mockups/Social.html mockups/Social.png`, where `--file-access` lets the local fonts load.

Left: the app icon and name, the headline in two sizes, and four labels. Right: the Today desk at 1.25x: the deal card with its tab and stack edges, the Coming up slip behind it, and the day stamp across the card's corner.

## Copy

The card speaks to job seekers who are not technical, in plain words. Many visitors see it on Twitter first and then land on the landing page, so its headline is the landing page's headline, word for word. A visitor who clicks lands on the words they just read and knows they are in the right place.

- **Headline**: "Find jobs that fit your terms." at 78px, broken after "that". Then "Claude drafts the application when you say go." at 44px in `muted`, broken after "application". Both are in the slab.
- **Line**: none. The second sentence already says that nothing happens until you say go, and a line under it would be a third size of text.
- **Labels**: Free, Mac, Windows, Uses your Claude Code. They are facts, not features. "Free" is true: Jobdesk is free and open source under the MIT License, while the tasks use the user's own Claude Code. "Mac" and "Windows" are the words the landing page uses for Apple silicon Macs and 64-bit Windows. "Uses your Claude Code" says it runs on the user's own install, with no Anthropic or Claude logo and no hint of endorsement.
- **Dropped**: "Built in Rust" and "ai-job-search commands". They are jargon to a job seeker and say nothing about what Jobdesk does for them.

## Decisions

- **Light only.** The image is opaque, on its own grid, so it needs no dark twin. Navy on paper sits well on both GitHub page themes, and GitHub's social preview takes one image anyway.
- **The app's objects on the desk, not a window screenshot.** A tilted window with a soft shadow is the stock hero. Here the card, the slip and the stamp lie on the grid the app paints, tilted 1 to 2 degrees like dealt papers, with the app's solid offset shadow, not a grey blur. The menu is left out, so the only mark is the app icon.
- **The stamp is struck across the card's top-right corner**, rotated -12 degrees, like a postmark. It is the one bold move, and it names the look.
- **One magenta**: the Now chip in Coming up, in `turn`, pointing at the same job as the card. Nothing else uses `turn` or `mark`. The headline gets no highlighter, because a highlighted word is the stock headline trick.
- **Sizes that hold at 400px**: the headline is 78px, so about 24px at 400 wide. The second sentence is 44px, about 14px, and the labels 18px.
- **The desk fits as it is**: "application" ends about 50px left of the card, so nothing is trimmed.
- **The safe margin**: everything stays 40px inside each edge.

## Doc changes

For the build step:

- Copy `mockups/Social.png` to `site/jobdesk-social.png` and to `assets/brand/jobdesk-social.png`.
- In `site/index.html`, set `og:image:alt` to "Jobdesk: Find jobs that fit your terms. Claude drafts the application when you say go."
- In `README.md` line 2, set the hero `<img alt>` to the same text.
