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

A local `cargo packager` run, and a build from the Actions tab, is ad-hoc signed (`signing-identity = "-"` in `Cargo.toml`) and not notarized, so macOS warns on other machines that the developer cannot be verified. A release build is signed with a Developer ID Application certificate and notarized by Apple, as [macOS signing](#macos-signing) says, so it opens with no warning. Windows warns because the installer is unsigned, and SmartScreen can keep warning after signing until the certificate builds a reputation. The app icons live in `assets/brand/`.

### macOS signing

Only a release signs. The Tag release workflow calls the Installers workflow with `sign: true` and passes it six secrets. No other run gets them: a manual run from the Actions tab, a branch and a fork build ad-hoc signed, as before. When the call asks for a signed release and a secret is missing, the build fails with an error that names it, and nothing is tagged. A release never ships ad-hoc signed.

| Secret | What it is |
| --- | --- |
| `APPLE_CERTIFICATE` | The Developer ID Application certificate with its private key, as a `.p12` file in base64. |
| `APPLE_CERTIFICATE_PASSWORD` | The password set when the `.p12` was exported. |
| `APPLE_SIGNING_IDENTITY` | The certificate name, such as `Developer ID Application: Your Name (TEAMID)`. |
| `APPLE_API_ISSUER` | The issuer ID of an App Store Connect API key. |
| `APPLE_API_KEY` | The key ID of that API key. |
| `APPLE_API_KEY_P8` | The contents of the `AuthKey_<key id>.p8` file of that key. |

To create them:

1. Create the certificate. In Keychain Access, choose Certificate Assistant, then Request a Certificate From a Certificate Authority, and save the request to disk. At [developer.apple.com](https://developer.apple.com/account/resources/certificates/add), choose Developer ID Application, upload the request, download the `.cer` and open it to install it. Only the Account Holder can create this certificate.
2. Export it. In Keychain Access, under My Certificates, select the certificate with its private key under it. Choose File, then Export Items, save a `.p12` and set a password. `security find-identity -v -p codesigning` prints the name for `APPLE_SIGNING_IDENTITY`.
3. Create the API key. In App Store Connect, open Users and Access, then Integrations, then Team Keys, and generate a key with the Developer role. The page shows the issuer ID and the key ID. The `.p8` file downloads once.
4. Store the secrets:

   ```bash
   base64 -i cert.p12 | pbcopy
   gh secret set APPLE_CERTIFICATE            # paste, then press Enter and Ctrl-D
   gh secret set APPLE_CERTIFICATE_PASSWORD
   gh secret set APPLE_SIGNING_IDENTITY
   gh secret set APPLE_API_ISSUER
   gh secret set APPLE_API_KEY
   gh secret set APPLE_API_KEY_P8 < AuthKey_XXXXXXXXXX.p8
   ```

   Delete the `.p12` and the `.p8` from your disk afterwards, or keep them in a password manager.

What the build does:

- cargo-packager 0.11.8 reads no signing identity from the environment, so the packaging step writes `APPLE_SIGNING_IDENTITY` over the `signing-identity` line of `Cargo.toml` in its own checkout. The default `-` stays in the repository for local runs.
- cargo-packager imports `APPLE_CERTIFICATE` into a temporary keychain, signs each binary and then the app with the hardened runtime and a secure timestamp, and deletes the keychain. It then notarizes the app with `notarytool` and staples the ticket to the app, before it builds the DMG. So the DMG holds the stapled app. The DMG is signed but not notarized.
- The update file `Jobdesk-vX.Y.Z-macos-arm64.app.tar.gz` is packed from that signed, stapled app. The ticket is a file in the bundle, so it survives the archive.
- The secrets reach only the packaging step, in its environment, and the API key file lives in a temporary folder until that step ends. The app is built in an earlier step with no secrets.
- Before the job uploads anything, `scripts/verify-macos-app.sh` must pass on the built app and on the app unpacked from the update file: `codesign --verify --deep --strict`, the Developer ID authority, the hardened runtime and the timestamp, `stapler validate`, and `spctl --assess` accepting it as notarized. The DMG must pass `codesign --verify` too.
- The app needs no entitlements. It uses Metal, the keychain through the Security framework and the `claude` process it starts, and none of them needs an exception from the hardened runtime. The ad-hoc builds already ran with it.

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

## Release

1. Run the Release workflow from the Actions tab on `main`. With the `auto` bump, the default, it picks the next version from the Conventional Commits since the last tag. Choose `patch`, `minor` or `major` to force one. It opens a `release/vX.Y.Z` pull request with the new `CHANGELOG.md` section and version.
2. Review the changelog on that branch, then merge the pull request.
3. The Tag release workflow tests and builds both installers on the merge commit, signs and notarizes the macOS app as [macOS signing](#macos-signing) says, then tags it and publishes the GitHub release with the changelog section as its notes and the installers attached.
   Between the build and the tag, a separate job signs the two update files under their release names with minisign. It alone gets the update key, and it runs no build. Then, before it tags, the workflow checks both signatures against the public key of the latest release before this one, since that is the key the installed copies trust. At the first release with updates, when no earlier release carries a real key, it checks against `assets/update/jobdesk-update.pub` of the commit it releases. Then `scripts/update-feed.sh` writes `latest.json`, the feed each copy of Jobdesk reads.
   The feed holds the version, the date, the notes, and each build's signature and size. It holds no address: the app builds each address from the version.

A failed build leaves no tag. Rerunning failed jobs rebuilds the same commit, so it helps only with a flaky or infrastructure failure. For a code failure, merge the fix into `main` through a normal pull request, then run the Tag release workflow by hand with the tag. It builds the current `main` commit, then tags and publishes the same way.

The Release workflow needs the `RELEASE_TOKEN` secret: a fine-grained personal access token with Contents and Pull requests read and write on this repository. A pull request opened with the default token would not start CI.

`scripts/release.sh [auto|patch|minor|major]` prepares the same release branch locally from a clean working tree. It needs [git-cliff](https://git-cliff.org), `jq`, `perl` and `cargo`.

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
