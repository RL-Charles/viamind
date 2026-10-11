set -eu
cd /home/charlesmowbray/Development/viamind-t-0102
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
source_sha=$(git rev-parse --short=12 HEAD)
git rev-parse HEAD
date -u
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo test -p viamind-ui-egui --test symbol_library actual_definition_field_identity --locked -- --nocapture
date -u

# Continue to required full Linux gates only if the actual identity oracle passes.
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo build -p viamind-ui-egui --example t0102_inspection --locked
cp "$CARGO_TARGET_DIR/debug/viamind" /home/charlesmowbray/.cache/viamind/native-artifacts/t0102/viamind-${source_sha}
cp "$CARGO_TARGET_DIR/debug/examples/t0102_inspection" /home/charlesmowbray/.cache/viamind/native-artifacts/t0102/inspection-${source_sha}
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0102/viamind-${source_sha} /home/charlesmowbray/.cache/viamind/native-artifacts/t0102/inspection-${source_sha}
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py -v
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0102/viamind-${source_sha} --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0102/generated-${source_sha} --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0102/catalog-report-${source_sha}
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0102/viamind-${source_sha} --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0102/workflow-${source_sha}
date -u
python3 scripts/verify-symbol-definition-native.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0102/viamind-${source_sha} --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0102/stdio-${source_sha}.json
