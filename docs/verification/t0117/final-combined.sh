#!/bin/bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0117
: "${VM117_FROZEN:?literal final source must be supplied}"
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
exec 9>/tmp/viamind-workspace-gates.lock
flock 9
test "$(git rev-parse HEAD)" = "$VM117_FROZEN"
git rev-parse HEAD
find crates apps -name '*.rs' -exec touch {} +
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-engine --lib agent_policy --locked
cargo test -p viamind-engine --lib pcb_branches --locked
cargo test -p viamind-mcp --lib registry --locked
cargo test -p viamind-mcp --test agent_modes --test pcb_branches --test read_only --test symbol_packages --test schematic_component --locked
cargo test -p viamind-ui-egui --test agent_modes --test pcb_branches --test package_compact --test cross_probe --locked
cargo test -p viamind-ui-egui --lib package_component --locked
cargo test -p viamind-ui-egui --lib cross_probe --locked
cargo test -p viamind --test agent_approval --test read_only --locked
cargo build --workspace --locked
cargo build -p viamind-ui-egui --example agent_modes_inspection --locked
mkdir /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-verified
strip -o /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-verified/viamind "$CARGO_TARGET_DIR/debug/viamind"
strip -o /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-verified/agent-modes-inspection "$CARGO_TARGET_DIR/debug/examples/agent_modes_inspection"
chmod 755 /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-verified/viamind /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-verified/agent-modes-inspection
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-verified/viamind /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-verified/agent-modes-inspection
flock -u 9
python3 docs/verification/t0117/production-probe.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-verified/viamind --repo "$PWD" --directory /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-stdio
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-verified/viamind --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-generated --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-catalog-report
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-verified/viamind --check /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-generated --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-catalog-check
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-verified/viamind --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0117/final-workflow
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py
