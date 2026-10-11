set -eu
cd /home/charlesmowbray/Development/viamind-t-0110
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
git rev-parse HEAD
date -u
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo build -p viamind-ui-egui --example compact_symbol_inspection --locked
source_sha=$(git rev-parse --short=12 HEAD)
artifact_dir=/home/charlesmowbray/.cache/viamind/native-artifacts/t0110
cp "$CARGO_TARGET_DIR/debug/viamind" "$artifact_dir/viamind-$source_sha"
cp "$CARGO_TARGET_DIR/debug/examples/compact_symbol_inspection" "$artifact_dir/inspection-$source_sha"
strip --strip-debug "$artifact_dir/viamind-$source_sha" "$artifact_dir/inspection-$source_sha"
sha256sum "$artifact_dir/viamind-$source_sha" "$artifact_dir/inspection-$source_sha"
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py
python3 scripts/generate-mcp-reference.py --binary "$artifact_dir/viamind-$source_sha" --output "$artifact_dir/catalog-$source_sha" --report "$artifact_dir/catalog-report-$source_sha"
python3 scripts/generate-mcp-reference.py --binary "$artifact_dir/viamind-$source_sha" --check docs/generated --report "$artifact_dir/catalog-check-$source_sha"
python3 scripts/mcp-agent-workflow.py --binary "$artifact_dir/viamind-$source_sha" --output "$artifact_dir/workflow-$source_sha"
python3 scripts/verify-symbol-definition-native.py --binary "$artifact_dir/viamind-$source_sha" --report "$artifact_dir/detached-stdio-$source_sha.json"
python3 scripts/verify-symbol-publication-native.py --binary "$artifact_dir/viamind-$source_sha" --report "$artifact_dir/publication-stdio-$source_sha.json"
date -u
