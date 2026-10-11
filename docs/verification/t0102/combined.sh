set -eu
cd /home/charlesmowbray/Development/viamind-t-0102
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
source_sha=$(git rev-parse --short=12 HEAD)
artifact_dir=/home/charlesmowbray/.cache/viamind/native-artifacts/t0102
git rev-parse HEAD
date -u
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-mcp --lib symbol_libraries --locked -- --nocapture
cargo test -p viamind-mcp --test symbol_libraries --locked -- --nocapture
cargo test -p viamind-ui-egui --test symbol_library --locked -- --nocapture
cargo build --workspace --locked
cp "$CARGO_TARGET_DIR/debug/viamind" "$artifact_dir/combined-viamind-$source_sha"
sha256sum "$artifact_dir/combined-viamind-$source_sha"
python3 scripts/generate-mcp-reference.py --binary "$artifact_dir/combined-viamind-$source_sha" --output "$artifact_dir/combined-generated-$source_sha" --report "$artifact_dir/combined-catalog-$source_sha"
python3 scripts/generate-mcp-reference.py --binary "$artifact_dir/combined-viamind-$source_sha" --check docs/generated --report "$artifact_dir/combined-catalog-check-$source_sha"
python3 scripts/mcp-agent-workflow.py --binary "$artifact_dir/combined-viamind-$source_sha" --output "$artifact_dir/combined-workflow-$source_sha"
python3 scripts/verify-symbol-definition-native.py --binary "$artifact_dir/combined-viamind-$source_sha" --report "$artifact_dir/combined-stdio-$source_sha.json"
date -u
