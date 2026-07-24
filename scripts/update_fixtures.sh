#!/usr/bin/env bash
# Regenerate the committed fixture gates from the current implementation.
#
# Order matters: manifest configs first (detection/config), then the
# generated Shipit texts (examples/ goldens + manifest `shipit` fields),
# then the plan snapshots (which evaluate the freshly-updated manifest).
#
# Review the resulting diff like any golden change. If you add or remove
# a case/example, also bump the pinned counts (EXPECTED_* consts in the
# gate tests) in the same commit.
set -euo pipefail
cd "$(dirname "$0")/.."

export SHIPIT_UPDATE_FIXTURES=1

echo "==> configs (fixtures/manifest.json)"
cargo test -p shipit-providers --test config_differential -- --nocapture

echo "==> generated Shipit texts (examples/*/Shipit + manifest)"
cargo test -p shipit-cli --test goldens -- --nocapture

echo "==> plan snapshots (tests/plan_snapshots/)"
cargo test -p shipit-starlark --test snapshots -- --nocapture

unset SHIPIT_UPDATE_FIXTURES

echo "==> verifying gates against the regenerated fixtures"
cargo test -p shipit-providers --test config_differential
cargo test -p shipit-cli --test goldens
cargo test -p shipit-starlark --test snapshots

echo
echo "Done. Review with: git diff fixtures/ tests/plan_snapshots/ examples/"
