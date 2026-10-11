#!/usr/bin/env bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0122
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
exec 9>/tmp/viamind-workspace-gates.lock
flock 9
source_sha=$(git rev-parse --short=12 HEAD)
artifact_dir=/home/charlesmowbray/.cache/viamind/native-artifacts/t0122
git rev-parse HEAD
date -u
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --offline --locked --workspace --all-targets -- -D warnings
cargo test --offline --locked --workspace
cargo build --offline --locked --workspace
cargo build --offline --locked -p viamind-ui-egui --example schematic_labels_inspection
cp "$CARGO_TARGET_DIR/debug/viamind" "$artifact_dir/viamind-$source_sha"
cp "$CARGO_TARGET_DIR/debug/examples/schematic_labels_inspection" "$artifact_dir/inspection-$source_sha"
sha256sum "$artifact_dir/viamind-$source_sha" "$artifact_dir/inspection-$source_sha"
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py -v
python3 scripts/generate-mcp-reference.py --binary "$artifact_dir/viamind-$source_sha" --check docs/reference --report "$artifact_dir/catalog-$source_sha"
python3 scripts/mcp-agent-workflow.py --binary "$artifact_dir/viamind-$source_sha" --output "$artifact_dir/workflow-$source_sha"
python3 scripts/verify-schematic-view.py "$artifact_dir/viamind-$source_sha" fixtures/views/schematic.viamind.json "$artifact_dir/static-view-$source_sha"
date -u
