#!/usr/bin/env bash
# Renders a mockup canvas to a PNG with headless Google Chrome.
# A canvas reads its state and theme from the query string, so no copy or edit of the canvas is needed.
# Set CHROME to the browser's path when Chrome is not in /Applications or ~/Applications.
#
#   scripts/render-canvas.sh [--size W,H] <canvas.html> <out.png> [key=value ...]
#   scripts/render-canvas.sh mockups/Grid.html mockups/Grid-today-dark.png screen=today theme=dark
#   scripts/render-canvas.sh --size 1280,640 mockups/Social.html mockups/Social.png
set -euo pipefail

usage() {
  echo "usage: scripts/render-canvas.sh [--size W,H] <canvas.html> <out.png> [key=value ...]" >&2
  exit 2
}

size=1280,820
if [[ "${1:-}" == "--size" ]]; then
  [[ $# -ge 2 && "$2" =~ ^[0-9]+,[0-9]+$ ]] || usage
  size=$2
  shift 2
fi
[[ $# -ge 2 ]] || usage

canvas=$1
out=$2
shift 2

if [[ "$canvas" != *.html || ! -f "$canvas" ]]; then
  echo "render-canvas: no canvas at $canvas" >&2
  exit 2
fi
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
abs_canvas="$(cd "$(dirname "$canvas")" && pwd)/$(basename "$canvas")"
abs_out="$(cd "$(dirname "$out")" && pwd)/$(basename "$out")"

url="file://${abs_canvas// /%20}"
if [[ ${#pairs[@]} -gt 0 ]]; then
  query=$(IFS='&'; echo "${pairs[*]}")
  url="$url?$query"
fi

profile=$(mktemp -d)
log=$(mktemp)
trap 'rm -rf "$profile"; rm -f "$log"' EXIT

rm -f "$abs_out"
status=0
"$chrome" --headless "--user-data-dir=$profile" "--window-size=$size" \
  --virtual-time-budget=3000 --hide-scrollbars --allow-file-access-from-files \
  "--screenshot=$abs_out" "$url" >"$log" 2>&1 || status=$?

if [[ $status -ne 0 || ! -s "$abs_out" ]]; then
  echo "render-canvas: Chrome wrote no PNG" >&2
  tail -n 20 "$log" >&2
  exit 1
fi

echo "rendered $out at ${size/,/x}"
