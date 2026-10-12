set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0120
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo test -p viamind-doc --lib pad_apertures --locked
cargo test -p viamind-doc --test pad_apertures --test footprint --test footprint_artwork --locked
cargo test -p viamind-engine --test pad_apertures --test footprint_artwork --test footprint --test pads --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
