set -eu
cd /home/charlesmowbray/Development/viamind-t-0110
export CARGO_TARGET_DIR=/tmp/claude-1000/-home-charlesmowbray-Development-opensource-altium/c7e09c2c-0a14-48ab-b987-697c122c1f03/scratchpad/stack/target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
git rev-parse HEAD
date -u
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo test -p viamind-ui-egui --test symbol_compact --locked -- --nocapture --test-threads=1
cargo test -p viamind-ui-egui --test symbol_library --locked -- --nocapture --test-threads=1
date -u
