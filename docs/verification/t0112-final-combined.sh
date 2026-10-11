set -eu
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
git rev-parse HEAD
date -u
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-fab --lib --locked
cargo test -p viamind-ui-egui --lib release_inspection --locked
cargo test -p viamind-ui-egui --test release_inspection --locked
cargo test -p viamind-ui-egui --test command_parity --locked
cargo test -p viamind-mcp --test release_inspection --locked
cargo test -p viamind --test release_inspection --locked
cargo build --workspace --locked
strip -o /home/charlesmowbray/.cache/viamind/native-artifacts/t0112/production-final-combined-verified "$CARGO_TARGET_DIR/debug/viamind"
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0112/production-final-combined-verified
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0112/production-final-combined-verified --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0112/generated-final --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0112/catalog-final
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0112/production-final-combined-verified --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0112/workflow-final
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py -v
date -u
