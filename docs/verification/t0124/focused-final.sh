#!/bin/bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0124
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
flock /tmp/viamind-workspace-gates.lock bash -c '
set -euo pipefail
rg --files crates apps -g "*.rs" -0 | xargs -0 touch
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p viamind-ui-egui --lib sidebar_tests -- --nocapture
cargo test -p viamind-ui-egui --test schematic --test schematic_labels --test schematic_wires --test command_parity
cargo build -p viamind-ui-egui --example schematic_sidebar_inspection
cp "$CARGO_TARGET_DIR/debug/examples/schematic_sidebar_inspection" /home/charlesmowbray/.cache/viamind/native-artifacts/t0124/schematic-sidebar-inspection-targeted
'
