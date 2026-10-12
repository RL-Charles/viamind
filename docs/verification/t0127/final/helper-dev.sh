#!/bin/bash
# Scoped developer build of the inspection helper only (not a gate).
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0127
exec 9>/tmp/viamind-workspace-gates.lock
flock 9
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target TMPDIR=/home/charlesmowbray/.cache/viamind/tmp CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
git rev-parse HEAD; git status --short
cargo build -p viamind-ui-egui --example t0127_inspection --locked
cp "$CARGO_TARGET_DIR/debug/examples/t0127_inspection" /home/charlesmowbray/.cache/viamind/native-artifacts/t0127/inspection-dev-$1
