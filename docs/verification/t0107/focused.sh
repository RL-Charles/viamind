set -eu
cd /home/charlesmowbray/Development/viamind-t-0107
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
git rev-parse HEAD
date -u
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-doc --lib symbol_library::publication_tests --locked -- --nocapture
cargo test -p viamind-engine --lib symbol_library::publication_tests --locked -- --nocapture
cargo test -p viamind-engine --lib native_publication_waits_for_in_flight_save --locked -- --nocapture
cargo test -p viamind-engine --test symbol_library --locked -- --nocapture
date -u
