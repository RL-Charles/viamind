set -eu
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
git rev-parse HEAD
date -u
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-fab --lib --locked
cargo test -p viamind-ui-egui --lib release_inspection --locked
cargo test --workspace --locked
cargo build --workspace --locked
cargo build -p viamind-ui-egui --example t0112_inspection --locked
strip -o /home/charlesmowbray/.cache/viamind/native-artifacts/t0112/production-46ba551-verified "$CARGO_TARGET_DIR/debug/viamind"
strip -o /home/charlesmowbray/.cache/viamind/native-artifacts/t0112/inspection-46ba551-verified "$CARGO_TARGET_DIR/debug/examples/t0112_inspection"
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0112/production-46ba551-verified /home/charlesmowbray/.cache/viamind/native-artifacts/t0112/inspection-46ba551-verified
date -u
