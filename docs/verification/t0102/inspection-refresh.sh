set -eu
cd /home/charlesmowbray/Development/viamind-t-0102
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
git rev-parse HEAD
date -u
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build -p viamind-ui-egui --example t0102_inspection --locked
source_sha=$(git rev-parse --short=12 HEAD)
cp "$CARGO_TARGET_DIR/debug/examples/t0102_inspection" /home/charlesmowbray/.cache/viamind/native-artifacts/t0102/inspection-${source_sha}
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0102/inspection-${source_sha}
date -u
