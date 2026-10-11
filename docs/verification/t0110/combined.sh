set -eu
cd /home/charlesmowbray/Development/viamind-t-0110
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
source_sha=$(git rev-parse --short=12 HEAD)
artifact_dir=/home/charlesmowbray/.cache/viamind/native-artifacts/t0110
git rev-parse HEAD
date -u
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-doc --lib symbol_library::publication_tests --locked -- --nocapture
cargo test -p viamind-engine --lib symbol_library::publication_tests --locked -- --nocapture
cargo test -p viamind-engine --lib native_publication_waits_for_in_flight_save --locked -- --nocapture
cargo test -p viamind-engine --test symbol_library --locked -- --nocapture
cargo test -p viamind-mcp --lib symbol_libraries --locked -- --nocapture
cargo test -p viamind-mcp --test symbol_libraries --locked -- --nocapture
cargo test -p viamind-ui-egui --test symbol_library --locked -- --nocapture
cargo test -p viamind-ui-egui --test symbol_compact --locked -- --nocapture
cargo build --workspace --locked
cp "$CARGO_TARGET_DIR/debug/viamind" "$artifact_dir/combined-viamind-$source_sha"
strip --strip-debug "$artifact_dir/combined-viamind-$source_sha"
sha256sum "$artifact_dir/combined-viamind-$source_sha"
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py
python3 scripts/generate-mcp-reference.py --binary "$artifact_dir/combined-viamind-$source_sha" --output "$artifact_dir/combined-generated-$source_sha" --report "$artifact_dir/combined-catalog-$source_sha"
python3 scripts/generate-mcp-reference.py --binary "$artifact_dir/combined-viamind-$source_sha" --check docs/generated --report "$artifact_dir/combined-catalog-check-$source_sha"
python3 scripts/mcp-agent-workflow.py --binary "$artifact_dir/combined-viamind-$source_sha" --output "$artifact_dir/combined-workflow-$source_sha"
python3 scripts/verify-symbol-definition-native.py --binary "$artifact_dir/combined-viamind-$source_sha" --report "$artifact_dir/combined-detached-stdio-$source_sha.json"
python3 scripts/verify-symbol-publication-native.py --binary "$artifact_dir/combined-viamind-$source_sha" --report "$artifact_dir/combined-publication-stdio-$source_sha.json"
date -u
