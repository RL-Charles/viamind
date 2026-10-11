#!/usr/bin/env bash
set -euo pipefail
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
git rev-parse HEAD
date -u
find crates apps -name '*.rs' -exec touch {} +
cargo fmt --all -- --check
cargo clippy --offline --locked --workspace --all-targets -- -D warnings
cargo build --offline --locked -p viamind-ui-egui --example copper_properties_inspection
mkdir -p /home/charlesmowbray/.cache/viamind/native-artifacts/t0111/inspection-rgba
cp "$CARGO_TARGET_DIR/debug/examples/copper_properties_inspection" /home/charlesmowbray/.cache/viamind/native-artifacts/t0111/inspection-rgba/inspection
strip --strip-debug /home/charlesmowbray/.cache/viamind/native-artifacts/t0111/inspection-rgba/inspection
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0111/inspection-rgba/inspection
git rev-parse HEAD > /home/charlesmowbray/.cache/viamind/native-artifacts/t0111/inspection-rgba/source.txt
date -u
