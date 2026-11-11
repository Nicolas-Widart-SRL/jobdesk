#!/usr/bin/env bash
# Keeps assets/update/ai-job-search.json: the ai-job-search releases this Jobdesk is compatible with.
# The list is a JSON array, newest first, of 1 to 5 entries `{"version": "X.Y.Z", "sha256": "<64 lowercase hex>"}`:
# the SHA-256 of the source zip that GitHub serves for the release's tag. Versions have no v, are unique,
# and go strictly down by number order, so 1.10.0 sits above 1.9.0.
#
#   scripts/ai-job-search.sh download vX.Y.Z OUT   download the source zip of a release to OUT, and test it
#   scripts/ai-job-search.sh add vX.Y.Z [ZIP]      add a release, from ZIP when given or else downloaded
#   scripts/ai-job-search.sh remove vX.Y.Z         remove a release, never the last
#   scripts/ai-job-search.sh check                 validate the list, download and hash every entry,
#                                                  and print the list as compact JSON on stdout
#
# Every message but that JSON goes to stderr. The repository is fixed below. AI_JOB_SEARCH_LIST sets the list's path,
# which is assets/update/ai-job-search.json under the repository root by default, so a test can use a temporary one.
# `add` drops the oldest entry when it makes a 6th, refuses a release older than the oldest of a full list,
# and leaves the file as it is when the version is there already: with the same hash that is no change,
# with another hash an error. The file is written only on success, and all at once.
# Needs jq 1.6 or newer, curl, unzip, and sha256sum or shasum.
set -euo pipefail

repo="ai-job-search-dev/ai-job-search"
max=5
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
list=${AI_JOB_SEARCH_LIST:-$root/assets/update/ai-job-search.json}

scratch=""
tmp_list=""
cleanup() {
  if [ -n "$scratch" ]; then
    rm -rf "$scratch"
  fi
  if [ -n "$tmp_list" ]; then
    rm -f "$tmp_list"
  fi
}
trap cleanup EXIT

usage() {
  cat >&2 <<'USAGE'
usage: scripts/ai-job-search.sh download vX.Y.Z OUT
       scripts/ai-job-search.sh add vX.Y.Z [ZIP]
       scripts/ai-job-search.sh remove vX.Y.Z
       scripts/ai-job-search.sh check
USAGE
  exit 2
}

die() {
  echo "error: $*" >&2
  exit 1
}

# The jq function that turns a version into the list of its numbers, which is what orders versions.
vkey='def vkey: split(".") | map(tonumber);'

if ! command -v jq >/dev/null 2>&1; then
  die "jq is not installed"
fi

if command -v sha256sum >/dev/null 2>&1; then
  sha256_of() {
    sha256sum "$1" | cut -d ' ' -f 1
  }
elif command -v shasum >/dev/null 2>&1; then
  sha256_of() {
    shasum -a 256 "$1" | cut -d ' ' -f 1
  }
else
  die "neither sha256sum nor shasum is installed"
fi

# Prints the version of a tag `vX.Y.Z`, and refuses anything else.
version_of() {
  if ! [[ $1 =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    die "'$1' is not a version like v1.2.3"
  fi
  printf '%s\n' "${1#v}"
}

new_scratch() {
  if [ -z "$scratch" ]; then
    scratch=$(mktemp -d "${TMPDIR:-/tmp}/ai-job-search.XXXXXX")
  fi
}

# Downloads the source zip of the release tagged $1 to $2, and tests that it is a zip.
download() {
  local tag=$1 out=$2
  version_of "$tag" >/dev/null
  if ! curl --fail --location --silent --show-error --proto '=https' --proto-redir '=https' --max-time 300 \
    --output "$out" "https://codeload.github.com/$repo/zip/refs/tags/$tag"; then
    rm -f "$out"
    die "could not download $tag"
  fi
  if ! unzip -tq "$out" >/dev/null 2>&1; then
    rm -f "$out"
    die "the download of $tag is not a zip"
  fi
}

# Succeeds when the JSON in file $1 is a well-formed list.
list_is_valid() {
  jq -e --argjson max "$max" "$vkey"'
    type == "array" and length >= 1 and length <= $max
    and all(.[];
      type == "object" and (keys == ["sha256", "version"])
      and (.version | type == "string") and (.version | test("^[0-9]+\\.[0-9]+\\.[0-9]+$"))
      and (.sha256 | type == "string") and (.sha256 | test("^[0-9a-f]{64}$")))
    and ([.[].version | vkey] as $keys
      | all(range(1; $keys | length); $keys[. - 1] > $keys[.]))
  ' "$1" >/dev/null 2>&1
}

require_valid_list() {
  if [ ! -f "$list" ]; then
    die "no list at $list"
  fi
  if ! list_is_valid "$list"; then
    die "$list is not a list of 1 to $max releases, newest first, each with a version X.Y.Z and a lowercase SHA-256"
  fi
}

# Writes the list JSON in $1 to the list's path, pretty printed, once it has proved valid.
write_list() {
  local dir
  dir=$(dirname "$list")
  mkdir -p "$dir"
  tmp_list=$(mktemp "$list.XXXXXX")
  printf '%s\n' "$1" | jq --indent 2 . >"$tmp_list"
  if ! list_is_valid "$tmp_list"; then
    die "the new list is not valid, so $list is unchanged"
  fi
  mv "$tmp_list" "$list"
  tmp_list=""
}

cmd_download() {
  [ "$#" -eq 2 ] || usage
  download "$1" "$2"
}

cmd_add() {
  [ "$#" -ge 1 ] && [ "$#" -le 2 ] || usage
  local tag=$1 zip=${2:-} version current
  version=$(version_of "$tag")

  # A missing file or an empty list is where the first release goes.
  if [ ! -f "$list" ] || jq -e 'type == "array" and length == 0' "$list" >/dev/null 2>&1; then
    current='[]'
  else
    require_valid_list
    current=$(cat "$list")
  fi

  if [ -n "$zip" ]; then
    [ -f "$zip" ] || die "no zip at $zip"
    unzip -tq "$zip" >/dev/null 2>&1 || die "$zip is not a zip"
  else
    new_scratch
    zip=$scratch/source.zip
    download "$tag" "$zip"
  fi
  local sha
  sha=$(sha256_of "$zip")

  local known
  known=$(jq -r --arg v "$version" "$vkey"' ($v | vkey) as $k | .[] | select((.version | vkey) == $k) | .sha256' <<<"$current")
  if [ -n "$known" ]; then
    if [ "$known" = "$sha" ]; then
      echo "$version is in the list with this hash already, so the list is unchanged" >&2
      return 0
    fi
    die "$version is in the list with the hash $known, not $sha, so the list is unchanged"
  fi

  local older
  older=$(jq -r --arg v "$version" --argjson max "$max" "$vkey"' length >= $max and (($v | vkey) < (.[-1].version | vkey))' <<<"$current")
  if [ "$older" = "true" ]; then
    die "$version is older than the oldest of the $max releases in the list, so it is not added"
  fi

  local sorted dropped
  sorted=$(jq --arg v "$version" --arg s "$sha" "$vkey"' (. + [{version: $v, sha256: $s}]) | sort_by(.version | vkey) | reverse' <<<"$current")
  dropped=$(jq -r --argjson max "$max" '.[$max:][]?.version' <<<"$sorted")
  write_list "$(jq --argjson max "$max" '.[:$max]' <<<"$sorted")"
  echo "added $version ($sha)" >&2
  if [ -n "$dropped" ]; then
    echo "dropped $dropped, the oldest, to keep $max" >&2
  fi
}

cmd_remove() {
  [ "$#" -eq 1 ] || usage
  local version
  version=$(version_of "$1")
  require_valid_list
  local current found
  current=$(cat "$list")
  found=$(jq -r --arg v "$version" "$vkey"' ($v | vkey) as $k | [.[] | select((.version | vkey) == $k)] | length' <<<"$current")
  if [ "$found" -eq 0 ]; then
    die "$version is not in the list"
  fi
  if [ "$(jq 'length' <<<"$current")" -le 1 ]; then
    die "$version is the last release in the list, which must keep one"
  fi
  write_list "$(jq --arg v "$version" "$vkey"' ($v | vkey) as $k | map(select((.version | vkey) != $k))' <<<"$current")"
  echo "removed $version" >&2
}

cmd_check() {
  [ "$#" -eq 0 ] || usage
  require_valid_list
  new_scratch
  local failed=0 version expected actual
  while IFS=$'\t' read -r version expected; do
    echo "checking $version" >&2
    if ! (download "v$version" "$scratch/$version.zip"); then
      failed=1
      continue
    fi
    actual=$(sha256_of "$scratch/$version.zip")
    rm -f "$scratch/$version.zip"
    if [ "$actual" != "$expected" ]; then
      echo "error: $version hashes to $actual, but the list says $expected" >&2
      failed=1
    fi
  done < <(jq -r '.[] | [.version, .sha256] | @tsv' "$list")
  if [ "$failed" -ne 0 ]; then
    exit 1
  fi
  jq -c . "$list"
}

if [ "$#" -lt 1 ]; then
  usage
fi
command=$1
shift
case "$command" in
  download) cmd_download "$@" ;;
  add) cmd_add "$@" ;;
  remove) cmd_remove "$@" ;;
  check) cmd_check "$@" ;;
  *) usage ;;
esac
