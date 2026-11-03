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

These builds are not signed with a developer identity. macOS warns on other machines until the app is signed with a Developer ID and notarized. Windows warns because the installer is unsigned, and SmartScreen can keep warning after signing until the certificate builds a reputation. The app icons live in `assets/brand/`.

## Release

1. Run the Release workflow from the Actions tab on `main`. With the `auto` bump, the default, it picks the next version from the Conventional Commits since the last tag. Choose `patch`, `minor` or `major` to force one. It opens a `release/vX.Y.Z` pull request with the new `CHANGELOG.md` section and version.
2. Review the changelog on that branch, then merge the pull request.
3. The Tag release workflow tests and builds both installers on the merge commit, then tags it and publishes the GitHub release with the changelog section as its notes and the installers attached.

A failed build leaves no tag. Rerunning failed jobs rebuilds the same commit, so it helps only with a flaky or infrastructure failure. For a code failure, merge the fix into `main` through a normal pull request, then run the Tag release workflow by hand with the tag. It builds the current `main` commit, then tags and publishes the same way.

The Release workflow needs the `RELEASE_TOKEN` secret: a fine-grained personal access token with Contents and Pull requests read and write on this repository. A pull request opened with the default token would not start CI.

`scripts/release.sh [auto|patch|minor|major]` prepares the same release branch locally from a clean working tree. It needs [git-cliff](https://git-cliff.org), `jq`, `perl` and `cargo`.

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

## Before you commit

- CI fails on any clippy warning: run `scripts/check.sh` before you commit.
- `rustfmt.toml` sets the style: run `cargo fmt` before you commit. CI and `scripts/check.sh` fail on unformatted code.
- Commit subjects follow Conventional Commits, because CI rejects others and git-cliff writes `CHANGELOG.md` from them.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the structure and the rules `tests/architecture.rs` enforces.
