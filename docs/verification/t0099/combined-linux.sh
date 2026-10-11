#!/usr/bin/env bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0099
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
flock -x /tmp/viamind-workspace-gates.lock bash -c '
set -euo pipefail
git rev-parse HEAD
date -u
rg --files crates apps -g "*.rs" -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-mcp --test read_only --locked
cargo test -p viamind --test read_only --locked
cargo test -p viamind-ui-egui --test agent_mode --locked
cargo build --workspace --locked
cp --reflink=auto "$CARGO_TARGET_DIR/debug/viamind" /home/charlesmowbray/.cache/viamind/native-artifacts/t0099/production-combined-959fb1f
strip --strip-debug /home/charlesmowbray/.cache/viamind/native-artifacts/t0099/production-combined-959fb1f
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0099/production-combined-959fb1f
date -u
'
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0099/production-combined-959fb1f --check docs/generated --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0099/combined-catalog
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0099/production-combined-959fb1f --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0099/combined-workflow
