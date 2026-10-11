set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0124
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
test "$(git rev-parse HEAD)" = "6dc7e40ba6fdd02caae34641aec956d1bf78bb6c"
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-ui-egui --lib compact_sidebar --locked -- --nocapture
