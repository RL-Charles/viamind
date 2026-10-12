set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0126
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 PYTHONDONTWRITEBYTECODE=1
export TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
git rev-parse HEAD; git diff --stat
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo test --locked -p viamind-ui-egui --example gpu_parity -- --nocapture
cargo clippy --locked -p viamind-ui-egui --example gpu_parity --profile test -- -D warnings
echo FIX_FOCUSED1_PASS
