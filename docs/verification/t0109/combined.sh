#!/bin/bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0109
exec 9>/tmp/viamind-workspace-gates.lock
flock 9
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
date -u
git rev-parse HEAD
test "$(git rev-parse HEAD)" = 1bd2221d98ec49b5c48f58d177386220817966ef
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-engine --lib auto_drc --locked
cargo test -p viamind-engine --test auto_drc --locked
cargo test -p viamind-mcp --test auto_drc_events --test drc --test read_only --locked
cargo build --workspace --locked
cp "$CARGO_TARGET_DIR/debug/viamind" /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/combined-1bd2221d98ec
strip --strip-debug /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/combined-1bd2221d98ec
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/combined-1bd2221d98ec --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/combined-catalog --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/combined-catalog-report
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/combined-1bd2221d98ec --check docs/generated --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/combined-catalog-drift
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/combined-1bd2221d98ec --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/combined-workflow
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0109/combined-1bd2221d98ec
date -u
