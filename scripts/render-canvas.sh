#!/usr/bin/env bash
# Renders a mockup canvas to a PNG with headless Google Chrome.
# A canvas reads its state and theme from the query string, so no copy or edit of the canvas is needed.
# Set CHROME to the browser's path when Chrome is not in /Applications or ~/Applications.
# --size sets the window size, 1280,820 by default.
# --file-access lets the page read other local files, which a canvas with local fonts needs, such as Social.html.
# Leave it off otherwise, so a canvas's script cannot read files beside it.
#
#   scripts/render-canvas.sh [--size W,H] [--file-access] <canvas.html> <out.png> [key=value ...]
#   scripts/render-canvas.sh mockups/Grid.html mockups/Grid-today-dark.png screen=today theme=dark
#   scripts/render-canvas.sh --size 1280,640 --file-access mockups/Social.html mockups/Social.png
set -euo pipefail

usage() {
  echo "usage: scripts/render-canvas.sh [--size W,H] [--file-access] <canvas.html> <out.png> [key=value ...]" >&2
  exit 2
}

size=1280,820
file_access=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --size)
      [[ $# -ge 2 && "$2" =~ ^[0-9]+,[0-9]+$ ]] || usage
      size=$2
      shift 2
      ;;
    --file-access)
      file_access=1
      shift
      ;;
    -*) usage ;;
    *) break ;;
  esac
done
[[ $# -ge 2 ]] || usage

canvas=$1
out=$2
shift 2

# Chrome's URL parser drops tabs and line breaks, so it would open another file and screenshot its error page.
control_path() {
  echo "render-canvas: canvas path holds a control character" >&2
  exit 2
}
[[ "$canvas" != *[[:cntrl:]]* ]] || control_path
if [[ "$canvas" != *.html || ! -f "$canvas" ]]; then
  echo "render-canvas: no canvas at $canvas" >&2
  exit 2
fi
abs_canvas="$(cd "$(dirname "$canvas")" && pwd)/$(basename "$canvas")"
[[ "$abs_canvas" != *[[:cntrl:]]* ]] || control_path
if [[ "$out" != *.png ]]; then
  echo "render-canvas: output must end in .png: $out" >&2
  exit 2
fi

pairs=()
for pair in "$@"; do
  if [[ ! "$pair" =~ ^[A-Za-z0-9_-]+=[A-Za-z0-9_.,-]*$ ]]; then
    echo "render-canvas: bad query pair $pair" >&2
    exit 2
  fi
  pairs+=("$pair")
done

if [[ -n "${CHROME:-}" ]]; then
  if [[ ! -x "$CHROME" ]]; then
    echo "render-canvas: CHROME=$CHROME is not executable" >&2
    exit 1
  fi
  chrome=$CHROME
else
  chrome=
  for candidate in \
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
    "$HOME/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"; do
    if [[ -x "$candidate" ]]; then
      chrome=$candidate
      break
    fi
  done
  if [[ -z "$chrome" ]]; then
    echo "render-canvas: Google Chrome not found; install it or set CHROME" >&2
    exit 1
  fi
fi

mkdir -p "$(dirname "$out")"
abs_out="$(cd "$(dirname "$out")" && pwd)/$(basename "$out")"

# Percent-encode the path, % first, so Chrome reads none of it as URL syntax. / stays as is.
path=${abs_canvas//\%/%25}
path=${path//\\/%5C}
path=${path// /%20}
path=${path//\#/%23}
path=${path//\?/%3F}
url="file://$path"
if [[ ${#pairs[@]} -gt 0 ]]; then
  query=$(IFS='&'; echo "${pairs[*]}")
  url="$url?$query"
fi

profile=$(mktemp -d)
log=$(mktemp)
trap 'rm -rf "$profile"; rm -f "$log"' EXIT

args=(--headless "--user-data-dir=$profile" "--window-size=$size"
  --virtual-time-budget=3000 --hide-scrollbars)
if [[ $file_access -eq 1 ]]; then
  args+=(--allow-file-access-from-files)
fi

rm -f "$abs_out"
status=0
"$chrome" "${args[@]}" "--screenshot=$abs_out" "$url" >"$log" 2>&1 || status=$?

if [[ $status -ne 0 || ! -s "$abs_out" ]]; then
  echo "render-canvas: Chrome wrote no PNG" >&2
  tail -n 20 "$log" >&2
  exit 1
fi

echo "rendered $out at ${size/,/x}"
