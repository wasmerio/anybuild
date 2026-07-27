#!/usr/bin/env bash
set -euo pipefail

# Run the Rust CLI's `generate` command for each directory under examples/
# Continues on errors and prints a summary at the end.

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

failures=()
shopt -s nullglob
for d in "$ROOT_DIR"/examples/*; do
  [ -d "$d" ] || continue
  echo "=== Generating for $d ==="
  if ! cargo run --quiet -- generate "$d"; then
    echo "--- Generation failed for $d" >&2
    failures+=("$d")
  fi
  echo
done
shopt -u nullglob

if [ ${#failures[@]} -gt 0 ]; then
  echo "Completed with failures in:"
  for f in "${failures[@]}"; do
    echo " - $f"
  done
else
  echo "All generations succeeded."
fi
