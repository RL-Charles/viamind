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
cargo test --workspace --locked
cargo build --workspace --locked
cp "$CARGO_TARGET_DIR/debug/viamind" /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/production-b83293fb964f
strip --strip-debug /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/production-b83293fb964f
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/production-b83293fb964f --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/catalog --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/catalog-report
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/production-b83293fb964f --check /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/catalog --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/catalog-drift
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/production-b83293fb964f --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/workflow
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/production-b83293fb964f
date -u
