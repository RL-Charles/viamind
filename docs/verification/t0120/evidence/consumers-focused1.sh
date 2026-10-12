set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0120
test "$(git rev-parse HEAD)" = "29d494a94b91abc9d021d3541c142c8c1224b81f"
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
export TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
export PYTHONDONTWRITEBYTECODE=1
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-doc --test pad_apertures --locked
cargo test -p viamind-engine --test pad_apertures --test part_commands --locked
cargo test -p viamind-fab --test pad_apertures --test footprint_artwork --test output_jobs --test output_job_calibration --test output_job_population --locked
cargo test -p viamind-render --test pad_apertures --test footprint_artwork --locked
cargo test -p viamind-ui-egui --test pad_apertures --test footprint_definition --test footprint_artwork --locked
cargo test -p viamind-mcp --test footprint_definition --locked
