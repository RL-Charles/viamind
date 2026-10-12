set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0121
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 PYTHONDONTWRITEBYTECODE=1 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
git rev-parse HEAD; git diff --stat
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo test --locked -p viamind-ui-egui --test pad_apertures
echo APERTURE_ORACLE1_PASS
