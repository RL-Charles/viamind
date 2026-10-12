set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0120
test "$(git rev-parse HEAD)" = 8ac6ae92675bab8f3f074b0e4168a3d5fdf1ed12
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-ui-egui --lib aperture_native --locked
cargo test -p viamind-ui-egui --test pad_apertures --locked
cargo build -p viamind-ui-egui --example pad_apertures_inspection --locked
