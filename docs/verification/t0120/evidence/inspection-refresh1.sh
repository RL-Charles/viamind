set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0120
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy -p viamind-ui-egui --example pad_apertures_inspection --locked -- -D warnings
cargo build -p viamind-ui-egui --example pad_apertures_inspection --locked
cp "$CARGO_TARGET_DIR/debug/examples/pad_apertures_inspection" /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/aperture-inspection-e30108f9
