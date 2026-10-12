#!/bin/bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0127
exec 9>/tmp/viamind-workspace-gates.lock
flock 9
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target TMPDIR=/home/charlesmowbray/.cache/viamind/tmp CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 PYTHONDONTWRITEBYTECODE=1
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
git rev-parse HEAD; git status --short
cargo fmt --all -- --check
cargo test -p viamind-doc --lib schematic_bus
cargo test -p viamind-engine --test footprint_holes_foundation --test schematic --test symbol_library
