#!/usr/bin/env bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0122
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export TMPDIR=/home/charlesmowbray/.cache/viamind/tmp CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 PYTHONDONTWRITEBYTECODE=1
exec 9>/tmp/viamind-workspace-gates.lock
flock 9
git rev-parse HEAD
date -u
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --offline --locked --workspace --all-targets -- -D warnings
cargo test --offline --locked -p viamind-render
cargo test --offline --locked -p viamind-ui-egui --lib schematic::labels::tests
cargo test --offline --locked -p viamind-ui-egui --test schematic_labels --test schematic --test schematic_wires --test view_export
cargo test --offline --locked -p viamind-mcp --test schematic_views --test png_views --test schematic_wires
cargo build --offline --locked --workspace
cargo build --offline --locked -p viamind-ui-egui --example schematic_labels_inspection
cp "$CARGO_TARGET_DIR/debug/viamind" /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/viamind-combined-6157a2b
cp "$CARGO_TARGET_DIR/debug/examples/schematic_labels_inspection" /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/inspection-combined-6157a2b
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/viamind-combined-6157a2b /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/inspection-combined-6157a2b
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py -v
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/viamind-combined-6157a2b --check docs/generated --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/combined-catalog
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/viamind-combined-6157a2b --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/combined-workflow
uv run --no-project --with pillow python scripts/verify-schematic-labels-native.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/viamind-combined-6157a2b --repo /home/charlesmowbray/Development/viamind-t-0122 --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/combined-stdio.json
uv run --no-project --with pillow python scripts/verify-schematic-view.py /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/viamind-combined-6157a2b fixtures/views/schematic.viamind.json /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/combined-static-view
date -u
