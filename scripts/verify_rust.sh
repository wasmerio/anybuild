#!/usr/bin/env bash
# The full gate stack.
#
#   scripts/verify_rust.sh          # gates
#   scripts/verify_rust.sh --e2e    # + wasmer-mode e2e with the Rust binary
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== [1/3] Build (deny warnings) ==="
RUSTFLAGS="-D warnings" cargo build --workspace --quiet
echo "workspace builds clean"

echo "=== [2/3] Gates (snapshots, legacy, configs, goldens, units) ==="
if command -v cargo-nextest >/dev/null; then
  cargo nextest run --workspace
else
  cargo test --workspace
fi

echo "=== [3/3] CLI smoke ==="
version=$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)
target/debug/shipit generate /nonexistent-shipit-banner-check \
  >/dev/null 2>/tmp/shipit-banner.txt || true
grep -q "Shipit ${version}" /tmp/shipit-banner.txt \
  && echo "banner ok (Shipit ${version})" \
  || { echo "banner mismatch"; cat /tmp/shipit-banner.txt; exit 1; }

if [[ "${1:-}" == "--e2e" ]]; then
  echo "=== e2e (wasmer mode) with the Rust binary ==="
  cargo nextest run --profile e2e -p shipit-e2e --run-ignored all \
    -E 'test(/__wasmer__/) and not test(/cdn|hugo|python_streamlit|python_django|python_fastapi/)'
fi

echo "ALL GATES GREEN"
