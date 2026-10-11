#!/usr/bin/env bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0096
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
export PYTHONDONTWRITEBYTECODE=1
flock -x /tmp/viamind-workspace-gates.lock bash -c '
set -euo pipefail
rg --files crates apps -g "*.rs" -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cp --reflink=auto "$CARGO_TARGET_DIR/debug/viamind" /home/charlesmowbray/.cache/viamind/native-artifacts/t0096/viamind
'
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py -v
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0096/viamind --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0096/generated --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0096/discovery-report
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0096/viamind --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0096/workflow
