#!/bin/bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0113
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
exec 9>/tmp/viamind-workspace-gates.lock
flock 9
git rev-parse HEAD
find crates apps -name '*.rs' -exec touch {} +
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p viamind-engine --lib pcb_branches -- --nocapture
cargo test -p viamind-mcp --lib pcb_branches -- --nocapture
cargo test -p viamind-mcp --test pcb_branches -- --nocapture
cargo test -p viamind-ui-egui --test pcb_branches -- --nocapture
cargo test -p viamind-ui-egui --test command_parity -- --nocapture
cargo test -p viamind-ui-egui --test copper_properties -- --nocapture
cargo test -p viamind-fab --lib release_inspection -- --nocapture
cargo build --workspace
cargo build -p viamind-ui-egui --example pcb_branches_inspection
mkdir /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/final-verified
strip -o /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/final-verified/viamind "$CARGO_TARGET_DIR/debug/viamind"
strip -o /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/final-verified/pcb-branches-inspection "$CARGO_TARGET_DIR/debug/examples/pcb_branches_inspection"
export PYTHONDONTWRITEBYTECODE=1
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/final-verified/viamind --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/final-catalog --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/final-catalog-report
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/final-verified/viamind --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/final-workflow
python3 docs/verification/t0113/production-probe.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/final-verified/viamind --repo "$PWD" --directory /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/final-production-stdio
python3 docs/verification/t0113/read-only-probe.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/final-verified/viamind --repo "$PWD" --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/final-read-only.json
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0113/final-verified/*
