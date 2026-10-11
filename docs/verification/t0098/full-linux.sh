set -eu
cd /home/charlesmowbray/Development/viamind-t-0098
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
printf 'T98 full source '
git rev-parse HEAD
date -u
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo build -p viamind-ui-egui --example footprint_definition_inspection --locked
cp "$CARGO_TARGET_DIR/debug/viamind" /home/charlesmowbray/.cache/viamind/native-artifacts/t0098/viamind-4d460fe
cp "$CARGO_TARGET_DIR/debug/examples/footprint_definition_inspection" /home/charlesmowbray/.cache/viamind/native-artifacts/t0098/inspection-4d460fe
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0098/viamind-4d460fe /home/charlesmowbray/.cache/viamind/native-artifacts/t0098/inspection-4d460fe
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py -v
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0098/viamind-4d460fe --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0098/generated-4d460fe --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0098/catalog-report-4d460fe
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0098/viamind-4d460fe --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0098/workflow-4d460fe
date -u
