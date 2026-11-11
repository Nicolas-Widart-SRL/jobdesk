#!/usr/bin/env bash
# Writes latest.json to stdout: the feed each copy of Jobdesk reads to learn of a new release.
# It holds the version, the release time, the notes, for each build its signature and its size, and the
# ai-job-search releases this one supports, 1 to 5 of them: for each its version, the SHA-256 of its source zip, and
# the signature of the two. They are written newest first as `ai_job_search_releases`, and the newest again as
# `ai_job_search`, the field an older Jobdesk reads.
# It holds no address: the app builds every download address from a version.
#
#   scripts/update-feed.sh VERSION NOTES_FILE DIR > latest.json
#
# VERSION is X.Y.Z, with or without the v. DIR holds both builds under their release names,
# each beside its .sig: the minisign signature the release's sign job made of it under that name, in base64 on one line:
#   Jobdesk-vX.Y.Z-macos-arm64.app.tar.gz and Jobdesk-vX.Y.Z-macos-arm64.app.tar.gz.sig
#   Jobdesk-vX.Y.Z-windows-x64-setup.exe and Jobdesk-vX.Y.Z-windows-x64-setup.exe.sig
# DIR also holds 1 to 5 ai-job-search pins, each beside its .sig, made the same way:
#   ai-job-search-vA.B.C.zip.sha256, the one line `<sha256>  ai-job-search-vA.B.C.zip` as sha256sum writes it,
#   and ai-job-search-vA.B.C.zip.sha256.sig, the sign job's signature of that line under the zip's name.
# The app rebuilds each line from the version and the hash in the feed, and checks each signature alone against it.
# PUB_DATE sets the release time, RFC 3339 in UTC, so a test fixture comes out the same each time.
# Needs jq 1.6 or newer.
set -euo pipefail

# The most ai-job-search releases a feed lists, which is also the most the app reads.
limit=5

if [ "$#" -ne 3 ]; then
  echo "usage: $0 VERSION NOTES_FILE DIR > latest.json" >&2
  exit 2
fi

version=${1#v}
notes=$2
dir=$3

if ! [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "error: version '$1' is not X.Y.Z" >&2
  exit 1
fi
if [ ! -f "$notes" ]; then
  echo "error: no notes file at $notes" >&2
  exit 1
fi

mac="$dir/Jobdesk-v$version-macos-arm64.app.tar.gz"
windows="$dir/Jobdesk-v$version-windows-x64-setup.exe"
for file in "$mac" "$mac.sig" "$windows" "$windows.sig"; do
  if [ ! -s "$file" ]; then
    echo "error: $file is missing or empty" >&2
    exit 1
  fi
done

size() {
  wc -c <"$1" | tr -d '[:space:]'
}

signature() {
  tr -d '[:space:]' <"$1"
}

# A release always pins at least one ai-job-search release and at most $limit, so a DIR with none, or with more, is refused.
shopt -s nullglob
pins=("$dir"/ai-job-search-v*.zip.sha256)
shopt -u nullglob
if [ "${#pins[@]}" -lt 1 ] || [ "${#pins[@]}" -gt "$limit" ]; then
  echo "error: $dir holds ${#pins[@]} ai-job-search-v*.zip.sha256 files, not 1 to $limit" >&2
  exit 1
fi

# One row for each pin: its version, its hash and its signature, apart by a space.
rows=""
for pin in "${pins[@]}"; do
  line=$(head -n 1 "$pin")
  if ! [[ $line =~ ^([0-9a-f]{64})\ \ (ai-job-search-v([0-9]+\.[0-9]+\.[0-9]+)\.zip)$ ]]; then
    echo "error: $pin is not one line '<sha256>  ai-job-search-vA.B.C.zip'" >&2
    exit 1
  fi
  pin_sha256=${BASH_REMATCH[1]}
  pin_name=${BASH_REMATCH[2]}
  pin_version=${BASH_REMATCH[3]}
  # The signed bytes are exactly the line the app rebuilds, so the file must hold that line and nothing else.
  if [ "$(basename "$pin")" != "$pin_name.sha256" ] || ! printf '%s\n' "$line" | cmp -s - "$pin"; then
    echo "error: $pin must be named after the zip it pins and hold its one line, ending in a newline" >&2
    exit 1
  fi
  if [ ! -s "$pin.sig" ]; then
    echo "error: $pin.sig is missing or empty" >&2
    exit 1
  fi
  rows+="$pin_version $pin_sha256 $(signature "$pin.sig")"$'\n'
done

# Newest first, by each number of the version. The feed is not signed, so the app sorts what it reads again.
rows=$(printf '%s' "$rows" | sort -t. -k1,1nr -k2,2nr -k3,3nr)

pub_date=${PUB_DATE:-$(date -u +%Y-%m-%dT%H:%M:%SZ)}

feed=$(jq -n \
  --arg version "$version" \
  --arg pub_date "$pub_date" \
  --rawfile notes "$notes" \
  --arg mac_signature "$(signature "$mac.sig")" \
  --argjson mac_size "$(size "$mac")" \
  --arg windows_signature "$(signature "$windows.sig")" \
  --argjson windows_size "$(size "$windows")" \
  --arg rows "$rows" \
  '($rows | split("\n") | map(split(" ") | { version: .[0], sha256: .[1], signature: .[2] })) as $releases
  | {
    version: $version,
    pub_date: $pub_date,
    notes: $notes,
    platforms: {
      "darwin-aarch64": { signature: $mac_signature, size: $mac_size },
      "windows-x86_64": { signature: $windows_signature, size: $windows_size }
    },
    ai_job_search: $releases[0],
    ai_job_search_releases: $releases
  }')

# The app refuses a feed over 256 KiB.
if [ "$(printf '%s\n' "$feed" | wc -c | tr -d '[:space:]')" -gt 262144 ]; then
  echo "error: latest.json is over 256 KiB, which the app refuses. Shorten the release notes." >&2
  exit 1
fi

printf '%s\n' "$feed"
