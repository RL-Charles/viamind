#!/bin/bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0127
exec 9>/tmp/viamind-workspace-gates.lock
flock 9
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target
export TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 PYTHONDONTWRITEBYTECODE=1
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
git rev-parse HEAD; git status --short
date -u
cargo fmt --all -- --check
cargo test -p viamind-doc --test footprint_holes
cargo test -p viamind-doc --lib schematic_bus
cargo test -p viamind-engine --lib schematic_budget
cargo test -p viamind-engine --lib schematic_drag
cargo test -p viamind-engine --lib schematic_buses
cargo test -p viamind-engine --test part_holes
cargo test -p viamind-render --test schematic_buses
cargo test -p viamind-render --test schematic
date -u
