#!/usr/bin/env bash
# Checks that a Jobdesk.app is signed with a Developer ID Application certificate, with the hardened runtime and a
# secure timestamp, and that it is notarized with the ticket stapled. It fails on the first check that does not hold.
# The Installers workflow runs it on a release build. To check a download, run it on the app from the DMG
# (a mounted DMG is at /Volumes/Jobdesk) or on the unpacked update.
#
#   scripts/verify-macos-app.sh /Volumes/Jobdesk/Jobdesk.app
set -euo pipefail

if [ "$#" -ne 1 ] || [ ! -d "$1" ]; then
  echo "usage: $0 path/to/Jobdesk.app" >&2
  exit 2
fi
app=$1

fail() {
  echo "::error::$app: $1"
  exit 1
}

echo "== codesign --verify"
codesign --verify --deep --strict --verbose=2 "$app" || fail "the signature does not verify."

echo "== codesign -dv"
details=$(codesign -dv --verbose=4 "$app" 2>&1)
echo "$details"
grep -q '^Authority=Developer ID Application: ' <<<"$details" || fail "it is not signed with a Developer ID Application certificate."
grep -Eq '^CodeDirectory .*flags=0x[0-9a-f]+\([^)]*runtime' <<<"$details" || fail "the hardened runtime is off."
grep -q '^Timestamp=' <<<"$details" || fail "the signature has no secure timestamp."

echo "== stapler validate"
xcrun stapler validate "$app" || fail "no valid notarization ticket is stapled."

echo "== spctl --assess"
assessed=$(spctl --assess --type execute -vv "$app" 2>&1) || {
  echo "$assessed"
  fail "Gatekeeper rejects it."
}
echo "$assessed"
grep -q '^source=Notarized Developer ID' <<<"$assessed" || fail "Gatekeeper does not see it as notarized."

echo "$app: signed, notarized and stapled."
