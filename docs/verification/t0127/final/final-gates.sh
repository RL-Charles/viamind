#!/bin/bash
# Usage: final-gates.sh <expected-head-sha> <tag>
set -euo pipefail
SRC=$1; TAG=$2
cd /home/charlesmowbray/Development/viamind-t-0127
test "$(git rev-parse HEAD)" = "$SRC"
test -z "$(git status --porcelain=v1)"
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 PYTHONDONTWRITEBYTECODE=1 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
E=/home/charlesmowbray/.cache/viamind/native-artifacts/t0127
exec 9>/tmp/viamind-workspace-gates.lock
flock 9
date -u +GATE_LOCK_%FT%TZ
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
echo GATE_FMT_PASS
cargo clippy --workspace --all-targets --locked -- -D warnings
echo GATE_CLIPPY_PASS
cargo test --workspace --locked --no-fail-fast
echo GATE_TEST_PASS
cargo build -p viamind-ui-egui --example t0127_inspection --locked
cp "$CARGO_TARGET_DIR/debug/examples/t0127_inspection" $E/inspection-$TAG
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo build --workspace --locked
echo GATE_BUILD_PASS
cp "$CARGO_TARGET_DIR/debug/viamind" $E/viamind-$TAG
sha256sum $E/viamind-$TAG $E/inspection-$TAG | tee $E/binaries-$TAG.sha256
test -z "$(git status --porcelain=v1)"
date -u +GATE_UNLOCK_%FT%TZ
flock -u 9
python3 -m unittest discover -s scripts/tests -p 'test_mcp_reference.py'
python3 scripts/generate-mcp-reference.py --binary $E/viamind-$TAG --check docs/generated --report $E/catalog-$TAG-report
python3 scripts/mcp-agent-workflow.py --binary $E/viamind-$TAG --output $E/workflow-$TAG
python3 scripts/verify-schematic-buses-native.py --repo /home/charlesmowbray/Development/viamind-t-0127 --binary $E/viamind-$TAG --directory $E/stdio-$TAG
test -z "$(git status --porcelain=v1)"
date -u +GATE_END_%FT%TZ
echo FINAL_GATES_COMPLETE
