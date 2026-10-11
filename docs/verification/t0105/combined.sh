#!/bin/bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0105
exec 9>/tmp/viamind-workspace-gates.lock
flock 9
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
date -u
git rev-parse HEAD
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-engine --lib agent_presence --locked
cargo test -p viamind-mcp --test agent_presence --locked
cargo test -p viamind-ui-egui --test agent_presence --locked
cargo test -p viamind --test read_only --locked
cargo build --workspace --locked
presence_revision=$(git rev-parse --short=12 HEAD)
presence_cache=/home/charlesmowbray/.cache/viamind/native-artifacts/t0105
cp "$CARGO_TARGET_DIR/debug/viamind" "$presence_cache/production-combined-$presence_revision"
strip --strip-debug "$presence_cache/production-combined-$presence_revision"
python3 scripts/generate-mcp-reference.py --binary "$presence_cache/production-combined-$presence_revision" --output "$presence_cache/catalog-combined-$presence_revision" --report "$presence_cache/catalog-report-combined-$presence_revision"
python3 scripts/generate-mcp-reference.py --binary "$presence_cache/production-combined-$presence_revision" --check "$presence_cache/catalog-combined-$presence_revision" --report "$presence_cache/catalog-drift-combined-$presence_revision"
python3 scripts/mcp-agent-workflow.py --binary "$presence_cache/production-combined-$presence_revision" --output "$presence_cache/workflow-combined-$presence_revision"
sha256sum "$presence_cache/production-combined-$presence_revision"
date -u
