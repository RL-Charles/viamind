#!/bin/bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0109
exec 9>/tmp/viamind-workspace-gates.lock
flock 9
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
date -u
git rev-parse HEAD
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-engine --lib auto_drc --locked
cargo test -p viamind-engine --test auto_drc --locked
cargo test -p viamind-mcp --test auto_drc_events --test drc --test read_only --locked
date -u
