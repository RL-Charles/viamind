#!/usr/bin/env bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0122
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 PYTHONDONTWRITEBYTECODE=1
exec 9>/tmp/viamind-workspace-gates.lock
flock 9
date -u
git rev-parse HEAD
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p viamind-ui-egui --lib schematic::labels::tests
cargo test -p viamind-ui-egui --test schematic_labels
cargo test -p viamind-ui-egui --test schematic
cargo test -p viamind-ui-egui --test schematic_wires
date -u
