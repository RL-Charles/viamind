set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0119
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
find crates apps -name '*.rs' -exec touch {} +
cargo test -p viamind-ui-egui --test parts stale_properties_require_reload_and_invalid_placement_keeps_the_form --locked -- --exact --nocapture
