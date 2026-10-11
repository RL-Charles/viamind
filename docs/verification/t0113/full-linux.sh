#!/bin/bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0113
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
exec 9>/tmp/viamind-workspace-gates.lock
flock 9
git rev-parse HEAD
find crates apps -name '*.rs' -exec touch {} +
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
cargo build -p viamind-ui-egui --example pcb_branches_inspection
mkdir /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/original-verified
strip -o /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/original-verified/viamind "$CARGO_TARGET_DIR/debug/viamind"
strip -o /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/original-verified/pcb-branches-inspection "$CARGO_TARGET_DIR/debug/examples/pcb_branches_inspection"
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/original-verified/viamind /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/original-verified/pcb-branches-inspection
