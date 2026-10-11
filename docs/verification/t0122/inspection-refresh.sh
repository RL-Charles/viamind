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
cargo clippy --offline --locked -p viamind-ui-egui --example schematic_labels_inspection -- -D warnings
cargo test --offline --locked -p viamind-ui-egui --test schematic_labels
cargo build --offline --locked -p viamind-ui-egui --example schematic_labels_inspection
cp "$CARGO_TARGET_DIR/debug/examples/schematic_labels_inspection" /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/inspection-fa9793b
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/inspection-fa9793b
date -u
