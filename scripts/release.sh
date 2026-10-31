#!/usr/bin/env bash
# Prepares a release commit on a new branch: picks the next version from the Conventional Commits
# since the last tag, prepends its section to CHANGELOG.md, and sets it in Cargo.toml and Cargo.lock.
# The release workflow (.github/workflows/release.yml) runs it, then pushes the branch and opens the PR.
# Needs git-cliff (https://git-cliff.org), jq and cargo.
#
# Usage: scripts/release.sh [auto|patch|minor|major]
# auto (the default) lets git-cliff pick the bump from the commits.
set -euo pipefail

bump=${1:-auto}
case "$bump" in
  auto | patch | minor | major) ;;
  *)
    echo "Usage: scripts/release.sh [auto|patch|minor|major]" >&2
    exit 2
    ;;
esac

cd "$(git rev-parse --show-toplevel)"

if [ -n "$(git status --porcelain)" ]; then
  echo "Working tree is not clean." >&2
  exit 1
fi

# An explicit bump raises the version even with nothing to release, so count the commits first.
commits=$(git cliff --unreleased --context 2>/dev/null | jq '[.[].commits[]] | length')
tag=$(git cliff --bumped-version --bump="$bump" 2>/dev/null)
if [ "$commits" -eq 0 ] || git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  echo "No releasable commits since the last tag." >&2
  exit 1
fi
version=${tag#v}

git switch -c "release/$tag"
git cliff --unreleased --bump="$bump" --prepend CHANGELOG.md 2>/dev/null
perl -pi -e 's/^version = "[^"]*"/version = "'"$version"'"/ if !$done && /^version = / && ($done = 1)' Cargo.toml
cargo update --workspace --quiet
git commit -q -am "chore(release): $tag"

echo "Created branch release/$tag."
if [ -z "${GITHUB_ACTIONS:-}" ]; then
  echo "Review CHANGELOG.md, then push the branch and open a pull request against main."
  echo "Merging it starts the Tag release workflow, which builds the installers, tags and publishes."
fi
