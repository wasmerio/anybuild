#!/usr/bin/env bash
# The full gate stack.
#
#   scripts/verify_rust.sh          # gates
#   scripts/verify_rust.sh --e2e    # + wasmer-mode e2e with the Rust binary
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== [1/5] Formatting ==="
cargo fmt --all -- --check
echo "workspace is formatted"

echo "=== [2/5] Build (deny warnings) ==="
RUSTFLAGS="-D warnings" cargo build --workspace --quiet
echo "workspace builds clean"

echo "=== [3/5] Clippy (deny warnings) ==="
cargo clippy --workspace --all-targets -- -D warnings
echo "clippy clean"

echo "=== [4/5] Gates (snapshots, legacy, configs, goldens, units) ==="
if command -v cargo-nextest >/dev/null; then
  cargo nextest run --workspace
else
  cargo test --workspace
fi
CARGO_TARGET_DIR="$PWD/target/sdk-consumer" \
  cargo check --locked --manifest-path fixtures/sdk-consumer/Cargo.toml --quiet
echo "external SDK consumer compiles"

echo "=== [5/5] CLI smoke ==="
version=$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)
target/debug/anybuild generate /nonexistent-anybuild-banner-check \
  >/dev/null 2>/tmp/anybuild-banner.txt || true
grep -q "Anybuild ${version}" /tmp/anybuild-banner.txt \
  && echo "banner ok (Anybuild ${version})" \
  || { echo "banner mismatch"; cat /tmp/anybuild-banner.txt; exit 1; }

if [[ "${1:-}" == "--e2e" ]]; then
  echo "=== e2e (wasmer mode) with the Rust binary ==="
  cargo nextest run --profile e2e -p anybuild-cli --test e2e --run-ignored all \
    -E 'test(/__wasmer__/) and not test(/cdn|hugo|python_streamlit|python_django|python_fastapi/)'
fi

echo "ALL GATES GREEN"
