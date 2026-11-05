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

# git-cliff logs warnings on stderr even when it succeeds, so its stderr shows only on a failure.
cliff() {
  local err status=0
  err=$(mktemp)
  git cliff "$@" 2>"$err" || status=$?
  if [ "$status" -ne 0 ]; then
    cat "$err" >&2
  fi
  rm -f "$err"
  return "$status"
}

if [ -n "$(git status --porcelain)" ]; then
  echo "Working tree is not clean." >&2
  exit 1
fi

# An explicit bump raises the version even with nothing to release, so count the commits first.
commits=$(cliff --unreleased --context | jq '[.[].commits[]] | length')
tag=$(cliff --bumped-version --bump="$bump")
if [ "$commits" -eq 0 ] || git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  echo "No releasable commits since the last tag." >&2
  exit 1
fi
version=${tag#v}

git switch -c "release/$tag"
cliff --unreleased --bump="$bump" --prepend CHANGELOG.md
perl -pi -e 's/^version = "[^"]*"/version = "'"$version"'"/ if !$done && /^version = / && ($done = 1)' Cargo.toml
cargo update --workspace --quiet
git commit -q -am "chore(release): $tag"

echo "Created branch release/$tag."
if [ -z "${GITHUB_ACTIONS:-}" ]; then
  echo "Review CHANGELOG.md, then push the branch and open a pull request against main."
  echo "Merging it starts the Tag release workflow, which builds the installers, tags and publishes. Use Squash and merge, or a merge commit, not a rebase merge, and keep the subject chore(release): $tag. If the subject is edited, run the Tag release workflow by hand with the tag."
fi
