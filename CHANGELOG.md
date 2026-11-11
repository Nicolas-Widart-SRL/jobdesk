# Changelog

All notable changes to this project are listed here.
<!-- git-cliff: end of header -->
## [0.5.0] - 2026-11-11

### Features

- Let agents drive the app with agent-desktop (#58)
- Give Jobdesk a product page on GitHub Pages (#60)
- See, switch and add job portals on a new Portals screen (#62)
- *(onboarding)* Set up a new Mac from scratch on first run (#65)
- *(folder-update)* Keep the job search folder current with each Jobdesk release, no git needed (#67)
- *(updates)* Support up to five tested ai-job-search releases, signed with each Jobdesk release (#68)
- *(onboarding)* Build the profile in the first run (#70)
- *(onboarding)* Suggest the first job search and its scoring after setup (#72)

### Bug fixes

- *(onboarding)* Check Python the way tasks run python3 (#66)
- *(release-check)* Count a release command only in shapes the check is sure of, and sign only after it passes (#71)

### Documentation

- *(trust)* Name the install task's risk under automatic tool approvals (#64)

## [0.4.0] - 2026-11-05

### Features

- Sign and notarize macOS releases (#55)

## [0.3.0] - 2026-11-04

### Features

- *(runs)* Show a task is running, with live Elapsed and Cost (#48)
- *(updates)* Check for, download and apply signed updates (#47)

### Bug fixes

- *(runs)* Ignore the exit of a closed session (#45)
- *(settings)* Read the API key from the keychain once per launch (#46)

## [0.2.0] - 2026-11-03

### Features

- *(settings)* Add How tasks sign in row to About (#41)
- *(profile)* Preview CV PDFs and show how fresh each is (#43)

### Bug fixes

- *(packaging)* Ad-hoc sign the macOS app (#42)

### Documentation

- Split ARCHITECTURE.md by topic (#40)
- Refresh README and docs, drop the auth review (#39)

## [0.1.0] - 2026-11-03

### Features

- Restyle every screen to match the Ledger mockups (#1)
- *(runs)* Allow tool permissions automatically and show when a run waits
- *(intray)* Show counts and a return chip on the horizon strip (#3)
- *(intray)* Move undo into the Cleared today rail (#5)
- *(runs)* Show why a tool failed in the run activity (#6)
- *(runs)* Show where a run is and when it is your turn (#8)
- Postmark redesign with plain screen names (#9)
- *(settings)* Add the Settings shell with General, Appearance and Runtime status (#21)
- *(settings)* Add Usage statistics (#24)
- *(profile)* Add the Profile screen with in-place block editing (#25)
- *(settings)* Add run defaults, monthly cap, permissions, data and privacy, about (#26)
- *(settings)* Add Providers with an API key and Test connection (#29)
- *(settings)* Add Your terms, Today queue and Jobs view (#30)

### Bug fixes

- *(ui_kit)* Render symbols like ⚠ through egui's emoji fonts
- *(runs)* Kill the claude process tree when a session ends
- UI pass for cursors, clicks and missing states (#12)
- *(runs)* Put the status badge last in Recent runs rows (#14)
- *(ui_kit)* Let run message tables use the full width (#16)
- *(ui)* Fill the width with document tables and centre list bullets (#28)
- Harden Windows support and test it in CI (#34)

### Documentation

- Rewrite README as a landing page with download buttons and social preview (#10)
- *(mockups)* Remove obsolete canvases and split the handoff README (#18)
- *(mockups)* Add Profile screen canvases and handoff (#19)
- *(mockups)* Add Settings screen canvases and handoff (#20)

