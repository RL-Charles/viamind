#!/bin/bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0124
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
flock /tmp/viamind-workspace-gates.lock bash -c '
set -euo pipefail
rg --files crates apps -g "*.rs" -0 | xargs -0 touch
cargo test -p viamind-ui-egui sidebar_tests -- --nocapture
'
