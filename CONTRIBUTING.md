# Contributing

How to run, package, release and test Jobdesk. See [ARCHITECTURE.md](ARCHITECTURE.md) for the structure and [AGENTS.md](AGENTS.md) for the rules.

## Run

Needs Rust 1.95 or newer.

```bash
cargo run -- --workspace /path/to/ai-job-search
```

You can also set `JOBDESK_WORKSPACE`, or choose the folder in the app. It reopens the last folder.

## Package

```bash
cargo install cargo-packager --locked --version 0.11.8
```

```bash
cargo packager --release --formats app,dmg
```

Output lands in `target/packages`. On Windows use `--formats nsis`. Run the Installers workflow from the Actions tab to build both installers for a branch. It uploads them as the `dmg` and `setup` artifacts, named `Jobdesk-vX.Y.Z-<commit>-macos-arm64.dmg` and `Jobdesk-vX.Y.Z-<commit>-windows-x64-setup.exe`. Its `quick` option skips clippy and tests, and adds `-untested` after the commit in the names. A release build names them after the tag, with no commit: `Jobdesk-vX.Y.Z-macos-arm64.dmg` and `Jobdesk-vX.Y.Z-windows-x64-setup.exe`.

The `dmg` artifact also holds the app as `Jobdesk-…-macos-arm64.app.tar.gz`, the file a copy of Jobdesk downloads to update itself. On Windows that file is the setup. The Installers workflow never sees the [update key](#the-update-key), so what it builds is never signed for updates, and no copy will install it as an update. Only a release signs.

A local `cargo packager` run, and a build from the Actions tab, is ad-hoc signed (`signing-identity = "-"` in `Cargo.toml`) and not notarized, so macOS warns on other machines that the developer cannot be verified. A release build is signed with a Developer ID Application certificate and notarized by Apple, as [macOS signing](#macos-signing) says, so it opens without the "cannot verify the developer" and malware warnings. macOS may still show its one-time "downloaded from the Internet" confirmation on the first open. Windows warns because the installer is unsigned, and SmartScreen can keep warning after signing until the certificate builds a reputation. The app icons live in `assets/brand/`.

### macOS signing

Only a release signs. The Tag release workflow calls the Installers workflow with `sign: true` and passes it six secrets. That call runs the `package-macos-signed` job, the only job that sets `environment: release`. The six secrets are environment secrets of `release`, which accepts runs from `main` only. Only a workflow on `main` can read them: a change on an unmerged branch or in a pull request cannot, and neither can a fork. A manual run of the Installers workflow from the Actions tab has no `sign` input: it builds ad-hoc signed, as before, and never reaches the environment. When the call asks for a signed release and a secret is empty or missing, the build fails with an error that names it, and nothing is tagged. A release never ships ad-hoc signed.

The Tag release workflow runs on a push to `main`, not on the pull request event, because the environment matches the branch of the run, and a pull request event runs on `refs/pull/N/merge`. Do not add a `refs/pull/*/merge` rule to the environment: it would let any pull request of this repository reach the secrets.

| Secret | What it is |
| --- | --- |
| `APPLE_CERTIFICATE` | The Developer ID Application certificate with its private key, as a `.p12` file in base64. |
| `APPLE_CERTIFICATE_PASSWORD` | The password set when the `.p12` was exported. |
| `APPLE_SIGNING_IDENTITY` | The certificate name, such as `Developer ID Application: Your Name (TEAMID)`. |
| `APPLE_API_ISSUER` | The issuer ID of an App Store Connect API key. |
| `APPLE_API_KEY` | The key ID of that API key. |
| `APPLE_API_KEY_P8` | The contents of the `AuthKey_<key id>.p8` file of that key. |

To set them up:

1. Create the environment. On GitHub, open the repository, then Settings, then Environments, then New environment. Name it `release` and choose Configure. Under Deployment branches and tags, choose Selected branches and tags, choose Add deployment branch or tag rule, leave the type at Branch, enter `main` and choose Add rule. If the environment is missing when a release runs, GitHub creates it with no rule, so create it first.
2. Create the certificate. In Keychain Access, choose Certificate Assistant, then Request a Certificate From a Certificate Authority, and save the request to disk. At [developer.apple.com](https://developer.apple.com/account/resources/certificates/add), choose Developer ID Application, upload the request, download the `.cer` and open it to install it. Only the Account Holder can create this certificate.
3. Export it. In Keychain Access, under My Certificates, select the certificate with its private key under it. Choose File, then Export Items, save a `.p12` and set a password. `security find-identity -v -p codesigning` prints the name for `APPLE_SIGNING_IDENTITY`.
4. Create the API key. In App Store Connect, open Users and Access, then Integrations, then Team Keys, and generate a key with the Developer role. The page shows the issuer ID and the key ID. The `.p8` file downloads once.
5. Add the six secrets as environment secrets of `release`, not as repository secrets. A workflow on any branch can read a repository secret, and only a run from `main` can read these. Delete any repository or organization secret with one of these names, because the Tag release job that calls the Installers workflow has no environment, so it reads those, and only the environment should hold them:

   ```bash
   base64 -i cert.p12 | gh secret set APPLE_CERTIFICATE --env release
   gh secret set APPLE_CERTIFICATE_PASSWORD --env release
   gh secret set APPLE_SIGNING_IDENTITY --env release --body 'Developer ID Application: Your Name (TEAMID)'
   gh secret set APPLE_API_ISSUER --env release --body '<issuer id>'
   gh secret set APPLE_API_KEY --env release --body '<key id>'
   gh secret set APPLE_API_KEY_P8 --env release < AuthKey_XXXXXXXXXX.p8
   ```

   Use `--body` for the one-line values, so none of them ends in a newline: the build refuses an identity with a newline or a space at either end. The same secrets are under Settings, Environments, `release`, Environment secrets, Add environment secret. Delete the `.p12` and the `.p8` from your disk afterwards, or keep them in a password manager.

What the build does:

- cargo-packager 0.11.8 reads no signing identity from the environment, so the packaging step writes `APPLE_SIGNING_IDENTITY` over the `signing-identity` line of `Cargo.toml` in its own checkout. The default `-` stays in the repository for local runs.
- cargo-packager imports `APPLE_CERTIFICATE` into a temporary keychain, signs each binary and then the app with the hardened runtime and a secure timestamp, and deletes the keychain. It then notarizes the app with `notarytool` and staples the ticket to the app, before it builds the DMG. So the DMG holds the stapled app. The DMG is signed but not notarized.
- The update file `Jobdesk-vX.Y.Z-macos-arm64.app.tar.gz` is packed from that signed, stapled app. The ticket is a file in the bundle, so it survives the archive.
- The secrets reach only the packaging step, in its environment, and the API key file lives in a temporary folder until that step ends. The app is built in a step before it, and the packaging step replaces the `before-packaging-command` of `Cargo.toml` with `true` in the checkout, so cargo-packager starts no build while it holds the secrets. What protects the secrets is that only a run on `main` can read them, that the job builds the reviewed merge commit with `--locked`, a pinned cargo-packager version, and its actions pinned to commits. The two steps share one machine, so this is not a wall against a build script of a dependency: a malicious dependency that gets into `Cargo.lock` could run in the job and reach the secrets. Review each change to `Cargo.lock` as you would code that signs releases. Two more risks remain, both on the ephemeral runner: cargo-packager downloads the `create-dmg` script when it packages, pinned to a commit but fetched at run time, and that script runs with the certificate and its password in its environment. And cargo-packager runs `security import -P <password>`, so the certificate password is on a command line while that command runs.
- Before the job uploads anything, `scripts/verify-macos-app.sh` must pass on the app inside the DMG, mounted read-only as a user sees it, on the built app, and on the app unpacked from the update file: `codesign --verify --deep --strict`, the Developer ID authority, the hardened runtime and the timestamp, `stapler validate`, and `spctl --assess` accepting it as notarized. The DMG must pass `codesign --verify` too.
- The app has no entitlements file. This follows from reasoning, not from a test on a signed build: it uses Metal, the keychain through the Security framework and the `claude` process it starts, none of them needs an exception from the hardened runtime, and the ad-hoc builds already carry the runtime flag. The release build adds no new runtime behavior: the ad-hoc builds Jobdesk ships today already carry the `runtime` flag in `codesign -dv`. The signed build differs by the identity, the secure timestamp, the notarization and the stapled ticket. A launch test in CI is not possible, because the GitHub macOS runners have no reliable GPU or window server, so it would fail without a real defect. Open the first signed release by hand, on a Mac, and check that it starts and shows its window. That happens after the release is published. A bad release is recovered with a fix release: the update feed always points to the newest one, so installed copies move to it.

To check a download, mount the DMG and run:

```bash
scripts/verify-macos-app.sh /Volumes/Jobdesk/Jobdesk.app
```

Or by hand:

```bash
codesign --verify --deep --strict --verbose=2 /Applications/Jobdesk.app
codesign -dv --verbose=4 /Applications/Jobdesk.app 2>&1 | grep -E 'Authority|flags|TeamIdentifier'
xcrun stapler validate /Applications/Jobdesk.app
spctl --assess --type execute -vv /Applications/Jobdesk.app   # source=Notarized Developer ID
```

Going from ad-hoc to Developer ID changes the signature of the app. The optional Anthropic API key sits in a macOS keychain item that the signature of the app that saved it guards, so after the first signed update macOS may ask once to allow access. Choose Always Allow. Later releases keep the same signing identity, so the choice holds.

The two update key secrets, `JOBDESK_UPDATE_KEY` and `JOBDESK_UPDATE_KEY_PASSWORD`, are repository secrets today. They could move to the same `release` environment later, with the `sign` job of the Tag release workflow set to `environment: release`.

## Release

1. Run the Release workflow from the Actions tab on `main`. With the `auto` bump, the default, it picks the next version from the Conventional Commits since the last tag. Choose `patch`, `minor` or `major` to force one. It opens a `release/vX.Y.Z` pull request with the new `CHANGELOG.md` section and version.
2. Review the changelog on that branch, then merge the pull request. Squash and merge is the recommended method, and a merge commit works too. Keep the squash commit subject `chore(release): vX.Y.Z`, as the pull request title gives it. When the subject is edited at the merge, for example to `Release v0.3.1 (#52)`, the workflow does not recognise the merge and the run is skipped without an error: run the Tag release workflow by hand with the tag instead. The same holds for a rebase merge, which is not supported, because it can end on a commit that is not named `chore(release): vX.Y.Z`. The merge is a push to `main`, and that push starts the Tag release workflow.
3. The Tag release workflow starts only for the merge of a `release/vX.Y.Z` pull request: its first job asks GitHub which pull request the pushed commit merged, and ends the run, with every later job skipped, unless there is exactly one merged pull request of this repository from `release/vX.Y.Z` into `main` whose merge commit is the pushed one. Any other push to `main` starts no build. GitHub can list no pull request for a few seconds after a merge, so the check asks up to 5 times, 10 seconds apart. When the commit message looks like a release merge and GitHub still lists no pull request, the run fails with an error and starts no release: run the workflow by hand with the tag, from the Actions tab on `main`. The workflow tests and builds both installers on the merge commit, signs and notarizes the macOS app as [macOS signing](#macos-signing) says, then tags it and publishes the GitHub release with the changelog section as its notes and the installers attached.
   Between the build and the tag, a separate job signs the two update files under their release names with minisign. It alone gets the update key, and it runs no build. Then, before it tags, the workflow checks both signatures against the public key of the latest release before this one, since that is the key the installed copies trust. At the first release with updates, when no earlier release carries a real key, it checks against `assets/update/jobdesk-update.pub` of the commit it releases. Then `scripts/update-feed.sh` writes `latest.json`, the feed each copy of Jobdesk reads.
   Each release also signs the `ai-job-search` releases it is compatible with: the 1 to 5 entries of `assets/update/ai-job-search.json` at the release commit, as [Adding an ai-job-search release](#adding-an-ai-job-search-release) says. First-run setup and the folder update use the newest. The `source` job checks out the release commit and runs `scripts/ai-job-search.sh check`, which validates the list, downloads each entry's source zip from GitHub's fixed address, `https://codeload.github.com/ai-job-search-dev/ai-job-search/zip/refs/tags/vA.B.C`, and checks its hash. The job then requires the script's output to equal the list file and to be one JSON array of 1 to 5 entries, and downloads and hashes every zip again itself. The sign job checks the list's shape again before it writes the key, then signs, for each entry, the line `<sha256>  ai-job-search-vA.B.C.zip`, as `sha256sum` writes it, under that zip's name. The publish job checks that the list still equals the file at the release commit, downloads and hashes every zip again, checks each signature with the others, and checks that each trusted comment names exactly `file:ai-job-search-vA.B.C.zip`. When the list does not check, a zip does not download or a hash does not match, the run fails and nothing is tagged or published.
   The feed holds the version, the date, the notes, each build's signature and size, and the `ai-job-search` releases: `ai_job_search_releases`, newest first, each with its version, the zip's SHA-256 and the signature, and `ai_job_search`, the newest entry again, which copies of Jobdesk from before the list still read. It holds no address: the app builds each address from a version.

The first signed release is also the first run of the signed pipeline. If it fails, nothing is tagged. A failed build leaves no tag. Rerunning failed jobs rebuilds the same commit, so it helps with a flaky or infrastructure failure, and with a run that failed because the secrets or the `release` environment were missing: add them, then choose Re-run failed jobs, since the job reads its secrets when it starts. For a code failure, including a check of the signed pipeline that fails, merge the fix into `main` through a normal pull request, then run the Tag release workflow by hand with the tag. It builds the current `main` commit, then tags and publishes the same way.

The Release workflow needs the `RELEASE_TOKEN` secret: a fine-grained personal access token with Contents and Pull requests read and write on this repository. A pull request opened with the default token would not start CI.

`scripts/release.sh [auto|patch|minor|major]` prepares the same release branch locally from a clean working tree. It needs [git-cliff](https://git-cliff.org), `jq`, `perl` and `cargo`.

### Adding an ai-job-search release

`assets/update/ai-job-search.json` lists the `ai-job-search` releases this Jobdesk is compatible with, newest first: 1 to 5 entries, each a version and the SHA-256 of the source zip GitHub serves for that tag. A version is three numbers from 0 to 4294967295 with no leading zero, such as `1.7.5`, the form Jobdesk writes back and names the zip by, so the script and the release workflows refuse `01.7.5`. First-run setup and the folder update use the newest. Settings › About tells the user when the open folder, if it is not under git, has files from a release that the list of the latest Jobdesk release does not hold. `scripts/ai-job-search.sh` keeps the list, and needs `jq` 1.6 or newer, `curl`, `unzip`, and `sha256sum` or `shasum`.

To add a release:

1. Add it to the list. The script downloads the tag's source zip, hashes it and puts the entry in version order:

   ```bash
   scripts/ai-job-search.sh add vX.Y.Z
   ```

   To hash a zip you already have, pass its path after the version. It must be the zip GitHub serves for the tag, or the check and the release fail. A sixth entry drops the oldest, and the script prints the version it dropped. A release older than the oldest of a full list is refused. A version already listed with the same hash changes nothing, and with another hash fails.
2. Open a pull request with the changed list. It runs the ai-job-search release workflow (`.github/workflows/ai-job-search.yml`), which checks every release the list holds, the ones already there included. The workflow also runs on each pull request and each push to `main` that changes `src/`, `tests/ai_job_search_release.rs`, `scripts/ai-job-search.sh`, `Cargo.toml`, `Cargo.lock` or the workflow itself, so a change that makes Jobdesk need something new fails while a listed release lacks it. In one job and one build, it downloads each zip into a folder of its own, runs the ignored test `the_candidate_release_has_what_jobdesk_relies_on` in `tests/ai_job_search_release.rs` once for each, and names every release that fails. The test checks the zip against the hash in the list, holds it to the limits first-run setup holds every release to, and names every file it lacks: `.claude/commands/apply.md`, each other command of the launcher as a `.claude/commands/<name>.md` or as a skill the user can invoke (a `.claude/skills/<folder>/SKILL.md` with `name: <name>` or a `.claude/skills/<name>/SKILL.md` with no `name:`, where a skill of either form whose front matter sets `user-invocable` to false does not count, because the user cannot start it as a command), every profile file, the two template guides without `BEGIN ACTIVE-TEMPLATE`, an `/add-template` that holds it, and `.agents/skills/<folder>/SKILL.md` for each built-in portal. `/build-cv` may be missing. It starts no `claude` and costs nothing.
3. Test the release with Jobdesk by hand, then sign it off on the pull request and merge it. The workflow checks only that the files are there, not that the commands work.
4. The next Jobdesk release signs the entry and ships it in its feed. Until then, installed copies read the list of the last release.

To check a release before you add it, run the ai-job-search release workflow from the Actions tab with its version, such as `1.7.5`. Or run the same check locally, with the zip in a folder that exists:

```bash
scripts/ai-job-search.sh download vX.Y.Z /tmp/ai-job-search.zip
```

```bash
JOBDESK_CANDIDATE_ZIP=/tmp/ai-job-search.zip JOBDESK_CANDIDATE_VERSION=X.Y.Z cargo test --test ai_job_search_release -- --ignored
```

A version not in the list yet skips only the hash check.

To stop supporting a release, remove it in a pull request. Once the next Jobdesk release ships without it, About shows its line for a folder whose files come from that release. The script refuses to remove the last entry, since the list keeps at least one:

```bash
scripts/ai-job-search.sh remove vX.Y.Z
```

`scripts/ai-job-search.sh check` validates the list, and downloads and hashes every entry, as the release's `source` job does.

### The update key

Each copy of Jobdesk checks a download against the public key built into it, from `assets/update/jobdesk-update.pub`. Outside a key change, that file holds the public key of the key pair the Tag release workflow signs with. The private key and its password are the `JOBDESK_UPDATE_KEY` and `JOBDESK_UPDATE_KEY_PASSWORD` secrets.

The key pair was created on the maintainer's own computer, and a new one is created the same way. The command below asks for a password:

```bash
cargo packager signer generate --path ~/.jobdesk/update.key
```

It writes the private key to `~/.jobdesk/update.key` and the public key to `~/.jobdesk/update.key.pub`. The public key file is committed unchanged as `assets/update/jobdesk-update.pub`, through a pull request. The private key and its password are stored as secrets:

```bash
gh secret set JOBDESK_UPDATE_KEY < ~/.jobdesk/update.key
gh secret set JOBDESK_UPDATE_KEY_PASSWORD
```

- Keep an offline backup of the private key and its password. With the key lost, no release can reach the copies already installed, and every user has to install the next one by hand.
- Copies of releases made before the first signed release with updates cannot update themselves, so their users install that release by hand once.
- To change the key, first merge the new public key into `assets/update/jobdesk-update.pub`, and release with the old key still in the secrets: that release carries the new key and is signed with the old one. Only then replace both secrets with the new key. The release after it is signed with the new key, and the Tag release workflow checks it against the key the previous release carries. Copies that installed the release with the new key check the next ones against it, and the others install by hand.
- Each signature names the file it signs, such as `Jobdesk-v0.2.0-windows-x64-setup.exe`, and the app checks that name against the version it installs. To sign a file by hand, sign it under its release name: `base64 -d ~/.jobdesk/update.key > update.minisign.key`, then `minisign -S -s update.minisign.key -m Jobdesk-vX.Y.Z-windows-x64-setup.exe`, then delete `update.minisign.key`, and put the `.minisig` in base64 on one line in the `.sig`.
- Each `ai-job-search` entry's signature names its zip, not the file it signs: for each entry, write the line to `ai-job-search-vA.B.C.zip.sha256`, then sign it with `-t "timestamp:$(date +%s)"$'\t'"file:ai-job-search-vA.B.C.zip"`, as the sign job does.
- These signatures are not code signing. They show a download comes from this repository's releases. macOS code signing and notarization are separate: see [macOS signing](#macos-signing). Windows still warns, as [Package](#package) says.

## Test

```bash
cargo test
```

A real Claude session runs only on request and costs a few cents:

```bash
cargo test --test live_claude -- --ignored --nocapture
```

Among them, `test_connection_passes_on_the_claude_sign_in` runs Test connection on your Claude sign-in, for about a cent.

No test touches the real keychain: the API key tests use an in-memory fake store. On macOS, a debug build from `cargo run` asks for Keychain access again after each rebuild, because the Keychain ties the item's access to the binary.

Screen tests render every screen in both themes from `tests/sample_workspace`, which holds fictional data. After an intended visual change, refresh them:

```bash
UPDATE_SNAPSHOTS=1 cargo test --test screens --test ui_kit_gallery
```

Refresh snapshots on macOS only, so every baseline comes from one renderer. `UPDATE_SNAPSHOTS=1` skips changes under the 200-pixel tolerance. Use `UPDATE_SNAPSHOTS=force` to rewrite every baseline. A failing snapshot writes `<name>.new.png` and `<name>.diff.png` beside the baseline. Git ignores them, and CI uploads them as the `snapshots` artifact.

CI skips formatting, build and tests when a change touches only Markdown. The macOS job measures coverage with `cargo llvm-cov` and uploads the HTML report as the `coverage` artifact. CI also builds and tests on Windows: a job on `windows-latest` runs clippy with `-D warnings` and `cargo test`. Snapshot renders and compares run on macOS only, behind the `SNAPSHOTS` gate in `tests/screens.rs` and `tests/ui_kit_gallery.rs`. Other UI assertions in those files run on every system. Tests marked `#[cfg(windows)]` run only in the Windows job.

`.gitattributes` checks text out with LF on every system (`* text=auto eol=lf`) and keeps PNG and PDF files binary. So a test that needs a CRLF file builds it from the LF one itself.

To catch Windows-only clippy warnings before CI does, lint for Windows on your machine:

```bash
rustup target add x86_64-pc-windows-msvc
cargo clippy --target x86_64-pc-windows-msvc --all-targets --locked -- -D warnings
```

Miri runs weekly and on demand (Actions tab, "Miri", Run workflow) over the domain, application and shared_kernel tests, not on each pull request. To run it locally:

```bash
rustup toolchain install nightly --component miri,rust-src
cargo +nightly miri test --locked --lib -- ::domain:: ::application:: shared_kernel:: --skip shared_kernel::host_path
```

CodeQL scans the `actions` and `rust` code after a merge to main, weekly and on demand (Actions tab, "CodeQL", Run workflow), not on pull requests.

cargo-audit checks `Cargo.lock` in CI. To run it locally:

```bash
cargo install cargo-audit --locked
cargo audit
```

When the audit job fails on an advisory, update the dependency if a fixed release exists. Otherwise add the advisory id to `ignore` under `[advisories]` in `.cargo/audit.toml`, and create that file if it is not there yet. Put the reason in a comment beside it:

```toml
[advisories]
ignore = ["RUSTSEC-…"]  # why it does not apply
```

TruffleHog scans the whole git history for secrets in CI, and fails on a verified, unverified or unknown (check failed) one. To run it locally: `trufflehog git file://. --results=verified,unknown,unverified --fail --no-update`.

## Landing page

`site/` holds the landing page at [nicolas-widart-srl.github.io/jobdesk](https://nicolas-widart-srl.github.io/jobdesk/). It has no build step. The Pages workflow (`.github/workflows/pages.yml`) publishes the folder as is on each push to `main` that touches `site/`, and you can also run it from the Actions tab.

- The page repeats facts from `README.md`, `docs/guide.md` and `docs/architecture/trust.md`. When you change a fact the page states, change `site/index.html` too.
- The hero's card and launcher are drawn in HTML from the app's own strings: the `/apply` command line from `apply_line` in `src/intray/application/compose.rs`, and `/apply`'s field, budget and labels from `src/runs/domain/catalog.rs` and `src/runs/ui/launcher.rs`. When those change, redraw the hero. `mockups/handoff/landing.md` lists what the drawing takes from each file.
- Its other images are crops of `tests/snapshots/`, saved as lossless WebP in `site/img/`, light and dark. Each crop's box sits in the HTML comment beside its `.crop` element in `site/index.html`: the snapshot, then x, y and width x height in the snapshot's pixels, and any scaling. When a screen it shows changes, cut its crops again to those boxes and keep `width` and `height` on each `img` in step.
- The Windows "not signed yet" lines, in the download panel and the Signing row, change once the installer is signed.
- The design source is `mockups/landing/Landing.html`, with its notes in `mockups/handoff/landing.md`.

## Agent team

[AGENTS.md](AGENTS.md#team) describes the agent team. The main session runs as the `lead` agent in the main checkout only. Add the `agent` key to `.claude/settings.local.json` there, which git ignores, and keep whatever else the file holds:

```json
{ "agent": "lead" }
```

Never put it in `.claude/settings.json`. Every worktree holds that file, so a builder session started in a worktree would run as the lead, without the Edit tool.

A branch cut before this setup still sets `agent` in its own `.claude/settings.json`, so a builder thread there still runs as the lead. Rebase that branch onto `origin/main` once this setup has merged, then start a new builder thread, because a running thread keeps the agent it started with.

## OpenSpec

OpenSpec changes and specs live in `openspec/`. [AGENTS.md](AGENTS.md#team) says how they run through the team. OpenSpec 1.16.0 needs Node.js 22.12.0 or newer (its npm `engines` is `node >=22.12.0`). Install the CLI:

```bash
npm install -g @fission-ai/openspec@1.16.0
```

Check every change and spec:

```bash
openspec validate --all --strict
```

CI's Validate OpenSpec job runs the same check when a change touches `openspec/`.

`openspec update` regenerates `.claude/skills/openspec-*` and `.claude/commands/opsx/`. So team rules live in `.claude/agents/lead.md`, `.claude/agents/planner.md`, the `ship` and `build-step` skills in `.claude/skills/` and `openspec/config.yaml`, never in those generated files. In Claude Code, `.claude/settings.json` asks before `openspec init`, `openspec update` and `npx` runs of OpenSpec, and denies `openspec store` and the `--store` flag.

## Before you commit

- CI fails on any clippy warning: run `scripts/check.sh` before you commit.
- `rustfmt.toml` sets the style: run `cargo fmt` before you commit. CI and `scripts/check.sh` fail on unformatted code.
- Commit subjects follow Conventional Commits, because CI rejects others and git-cliff writes `CHANGELOG.md` from them.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the structure and the rules `tests/architecture.rs` enforces.
