#!/usr/bin/env bash
# Runs the CI checks in a checkout and prints one line per check.
# On a failure it prints the shortest useful part of the output, not all of it.
# fmt runs in check mode: run cargo fmt to fix a failure.
#
#   scripts/check.sh            check this checkout
#   scripts/check.sh DIR        check the checkout at DIR, such as a builder's worktree
set -uo pipefail
cd "${1:-.}" && cd "$(git rev-parse --show-toplevel)" || exit 1

failed=0
log=$(mktemp)
trap 'rm -f "$log"' EXIT

if cargo fmt --check >"$log" 2>&1; then
  echo "fmt: ok"
else
  echo "fmt: FAIL, run cargo fmt"
  grep -E '^(Diff in|error)' "$log" | head -10
  failed=1
fi

if cargo test --quiet >"$log" 2>&1; then
  passed=$(grep -Eo '[0-9]+ passed' "$log" | awk '{s += $1} END {print s + 0}')
  echo "test: ok, $passed passed"
else
  echo "test: FAIL"
  grep -E '^test .* FAILED$|panicked at|^error' "$log" | head -20
  grep -E -A3 'panicked at' "$log" | grep -vE 'panicked at|^--$' | head -10
  ls tests/snapshots/*.diff.png 2>/dev/null | sed 's/^/snapshot differs: /'
  failed=1
fi

if cargo clippy --quiet --locked --all-targets -- -D warnings >"$log" 2>&1; then
  echo "clippy: ok"
else
  echo "clippy: FAIL"
  grep -E -A4 '^(warning|error)' "$log" | head -20
  failed=1
fi

exit "$failed"
