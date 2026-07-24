#!/usr/bin/env bash
# Regenerate the committed fixture gates from the current implementation.
#
# Order matters: manifest configs first (detection/config), then the
# generated Anybuild texts (examples/ goldens + manifest `anybuild` fields),
# then the plan snapshots (which evaluate the freshly-updated manifest).
#
# Review the resulting diff like any golden change. If you add or remove
# a case/example, also bump the pinned counts (EXPECTED_* consts in the
# gate tests) in the same commit.
set -euo pipefail
cd "$(dirname "$0")/.."

export ANYBUILD_UPDATE_FIXTURES=1

echo "==> configs (fixtures/manifest.json)"
cargo test -p anybuild config_differential::configs_match_python -- --nocapture

echo "==> generated Anybuild texts (examples/*/Anybuild + manifest)"
cargo test -p anybuild-cli --test goldens -- --nocapture

echo "==> plan snapshots (tests/plan_snapshots/)"
cargo test -p anybuild starlark_snapshots::plan_snapshots_match -- --nocapture

unset ANYBUILD_UPDATE_FIXTURES

echo "==> verifying gates against the regenerated fixtures"
cargo test -p anybuild config_differential::configs_match_python
cargo test -p anybuild-cli --test goldens
cargo test -p anybuild starlark_snapshots::plan_snapshots_match

echo
echo "Done. Review with: git diff fixtures/ tests/plan_snapshots/ examples/"
