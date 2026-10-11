#!/usr/bin/env bash
set -euo pipefail
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
artifact=/home/charlesmowbray/.cache/viamind/native-artifacts/t0101/final-combined
mkdir "$artifact"
git rev-parse HEAD | tee "$artifact/source.txt"
date -u
find crates apps -name '*.rs' -exec touch {} +
cargo fmt --all -- --check
cargo clippy --offline --locked --workspace --all-targets -- -D warnings
cargo test --offline --locked -p viamind-doc --test net_rules --test assembly_metadata --test save_load
cargo test --offline --locked -p viamind-engine --test net_rules --test assembly
cargo test --offline --locked -p viamind-mcp --test output_jobs --test read_only
cargo test --offline --locked -p viamind-ui-egui --test net_rules --test assembly_placement
cargo build --offline --locked --workspace
cp --reflink=auto "$CARGO_TARGET_DIR/debug/viamind" "$artifact/viamind"
strip --strip-debug "$artifact/viamind"
sha256sum "$artifact/viamind"
python3 scripts/generate-mcp-reference.py --binary "$artifact/viamind" --output "$artifact/generated" --report "$artifact/discovery-report"
python3 scripts/mcp-agent-workflow.py --binary "$artifact/viamind" --output "$artifact/workflow"
date -u
