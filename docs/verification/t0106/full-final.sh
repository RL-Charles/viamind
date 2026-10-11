#!/usr/bin/env bash
set -euo pipefail
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
artifact=/home/charlesmowbray/.cache/viamind/native-artifacts/t0106/verified-native-v3
mkdir "$artifact"
git rev-parse HEAD | tee "$artifact/source.txt"
date -u
find crates apps -name '*.rs' -exec touch {} +
cargo fmt --all -- --check
cargo test --offline --locked -p viamind-engine --test route_terminals
cargo test --offline --locked -p viamind-mcp --test route_terminals
cargo test --offline --locked -p viamind-ui-egui --test terminal_routing
cargo clippy --offline --locked --workspace --all-targets -- -D warnings
cargo test --offline --locked --workspace
cargo build --offline --locked --workspace
cargo build --offline --locked -p viamind-ui-egui --example terminal_routing_inspection
cp --reflink=auto "$CARGO_TARGET_DIR/debug/viamind" "$artifact/viamind"
cp --reflink=auto "$CARGO_TARGET_DIR/debug/examples/terminal_routing_inspection" "$artifact/inspection"
strip --strip-debug "$artifact/viamind" "$artifact/inspection"
sha256sum "$artifact/viamind" "$artifact/inspection"
python3 scripts/generate-mcp-reference.py --binary "$artifact/viamind" --output "$artifact/generated" --report "$artifact/catalog-discovery"
python3 scripts/mcp-agent-workflow.py --binary "$artifact/viamind" --output "$artifact/workflow"
date -u
