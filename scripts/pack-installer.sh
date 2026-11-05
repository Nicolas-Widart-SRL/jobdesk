#!/usr/bin/env bash
# Moves the one installer cargo-packager built into dist/ under its release name, and, on macOS, packs the app as the
# .tar.gz that a copy of Jobdesk downloads to update itself. The Installers workflow runs it after cargo packager.
# It reads its input from the environment:
#
#   BUILT        the glob of the installer in target/packages, such as '*.dmg'
#   NAME         the release name of the installer in dist/
#   BUNDLE       the app folder in target/packages to pack, such as Jobdesk.app. Empty on Windows.
#   UPDATE_NAME  the release name of the .tar.gz in dist/. Not read when BUNDLE is empty.
set -euo pipefail

# bash 3.2 on macOS has no mapfile; compgen exits 1 on no match.
files=()
while IFS= read -r f; do
  if [ -n "$f" ]; then files+=("$f"); fi
done < <(compgen -G "target/packages/$BUILT" || true)
if [ "${#files[@]}" -ne 1 ]; then
  echo "::error::Expected one $BUILT in target/packages, found ${#files[@]}."
  exit 1
fi
mkdir -p dist
mv "${files[0]}" "dist/$NAME"
# The app holds one top folder, Jobdesk.app/, with no macOS metadata, as the app's unpacker expects.
if [ -n "${BUNDLE:-}" ]; then
  if [ ! -d "target/packages/$BUNDLE" ]; then
    echo "::error::No $BUNDLE in target/packages."
    exit 1
  fi
  COPYFILE_DISABLE=1 tar --no-mac-metadata --no-xattrs -czf "dist/$UPDATE_NAME" -C target/packages "$BUNDLE"
fi
