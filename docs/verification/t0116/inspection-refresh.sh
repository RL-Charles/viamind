set -eu
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
git rev-parse HEAD
date -u
cargo fmt --all -- --check
cargo clippy -p viamind-ui-egui --example t0116_inspection --locked -- -D warnings
cargo build -p viamind-ui-egui --example t0116_inspection --locked
cp --reflink=auto "$CARGO_TARGET_DIR/debug/examples/t0116_inspection" /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/inspection-scroll-verified
strip /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/inspection-scroll-verified
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/inspection-scroll-verified
date -u
