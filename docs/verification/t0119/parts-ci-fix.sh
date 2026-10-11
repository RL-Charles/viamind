set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0119
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
find crates apps -name '*.rs' -exec touch {} +
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-ui-egui --test parts --locked
