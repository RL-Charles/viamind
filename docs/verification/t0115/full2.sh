set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0115
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
find crates apps -name '*.rs' -exec touch {} +
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

cargo test --workspace
cargo build --workspace
cargo build -p viamind-ui-egui --example package_annotation_inspection
cp "$CARGO_TARGET_DIR/debug/viamind" /home/charlesmowbray/.cache/viamind/native-artifacts/t0115/viamind-full-source
cp "$CARGO_TARGET_DIR/debug/examples/package_annotation_inspection" /home/charlesmowbray/.cache/viamind/native-artifacts/t0115/package-annotation-inspection-full-source
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0115/viamind-full-source /home/charlesmowbray/.cache/viamind/native-artifacts/t0115/package-annotation-inspection-full-source
