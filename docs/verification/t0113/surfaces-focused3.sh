#!/bin/bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0113
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
exec 9>/tmp/viamind-workspace-gates.lock
flock 9
find crates apps -name '*.rs' -exec touch {} +
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p viamind-engine --lib pcb_branches -- --nocapture
cargo test -p viamind-mcp --lib pcb_branches -- --nocapture
cargo test -p viamind-mcp --test pcb_branches -- --nocapture
cargo test -p viamind-ui-egui --test pcb_branches -- --nocapture
cargo test -p viamind-ui-egui --test command_parity -- --nocapture
