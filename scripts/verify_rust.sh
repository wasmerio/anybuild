#!/usr/bin/env bash
# The full Rust-migration gate stack, in dependency order.
#
#   scripts/verify_rust.sh          # everything except e2e
#   scripts/verify_rust.sh --e2e    # also run local-mode e2e vs the Rust binary
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== [1/6] Python suite (the oracle) ==="
uv run pytest tests/ --ignore=tests/test_e2e.py -q | tail -1

echo "=== [2/6] Fixture dump (fresh from Python) ==="
uv run python scripts/dump_rust_fixtures.py | tail -1

echo "=== [3/6] Rust build (deny warnings) ==="
RUSTFLAGS="-D warnings" cargo build --workspace --quiet
echo "workspace builds clean"

echo "=== [4/6] Rust tests: snapshots, legacy, config differential, units ==="
cargo test --workspace -- --nocapture 2>&1 \
  | grep -E "matched|evaluate|test result" | grep -v "0 passed; 0 failed"

echo "=== [5/6] CLI differential (generate + plan, both binaries) ==="
uv run python scripts/cli_differential.py

echo "=== [6/6] Version banner matches version.py ==="
version=$(grep -m1 '^version = ' src/shipit/version.py | cut -d'"' -f2)
# A nonexistent path errors after the banner prints — side-effect free.
target/debug/shipit generate /nonexistent-shipit-banner-check \
  >/dev/null 2>/tmp/shipit-banner.txt || true
grep -q "Shipit ${version}" /tmp/shipit-banner.txt \
  && echo "banner ok (Shipit ${version})" \
  || { echo "banner mismatch"; cat /tmp/shipit-banner.txt; exit 1; }

if [[ "${1:-}" == "--e2e" ]]; then
  echo "=== e2e (local mode) against the Rust binary ==="
  cargo build --release --quiet
  SHIPIT_BIN="$PWD/target/release/shipit" \
    uv run pytest -m e2e tests/test_e2e.py -x -q -n 4
fi

echo "ALL GATES GREEN"
