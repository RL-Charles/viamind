#!/bin/bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0117
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
exec 9>/tmp/viamind-workspace-gates.lock
flock 9
git rev-parse HEAD
find crates apps -name '*.rs' -exec touch {} +
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo build -p viamind-ui-egui --example agent_modes_inspection --locked
mkdir /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-verified
install -m755 "$CARGO_TARGET_DIR/debug/viamind" /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-verified/viamind
install -m755 "$CARGO_TARGET_DIR/debug/examples/agent_modes_inspection" /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-verified/agent-modes-inspection
strip /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-verified/viamind /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-verified/agent-modes-inspection
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-verified/viamind /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-verified/agent-modes-inspection
flock -u 9
python3 docs/verification/t0117/production-probe.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-verified/viamind --repo "$PWD" --directory /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-stdio
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-verified/viamind --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-generated --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-catalog-report
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-verified/viamind --check /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-generated --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-catalog-check
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-verified/viamind --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/original-workflow
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py
