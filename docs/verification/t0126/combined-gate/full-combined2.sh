set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0126
test "$(git rev-parse HEAD)" = 570ba277ae241789c8b5a7b90a492b29171dcde6
test -z "$(git status --porcelain -- crates apps Cargo.toml Cargo.lock docs/generated scripts)"
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 PYTHONDONTWRITEBYTECODE=1
export TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
export VIAMIND_BUILD_COMMIT=570ba277ae241789c8b5a7b90a492b29171dcde6
date -u
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --locked -p viamind-ui-egui --example gpu_parity --profile test -- -D warnings
cargo test --workspace --locked
cargo test --locked -p viamind-ui-egui --example gpu_parity -- --nocapture
echo RETOUCH_BEFORE_NORMAL_BUILD; date -u
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo build --workspace --locked
B=/home/charlesmowbray/.cache/viamind/native-artifacts/t0126/combined2-570ba277; mkdir $B
cp --no-clobber /home/charlesmowbray/.cache/viamind/build-target/debug/viamind $B/viamind
sha256sum $B/viamind | tee $B/viamind.sha256
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py -v
python3 scripts/generate-mcp-reference.py --binary $B/viamind --check docs/generated --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0126/combined2-catalog-report
python3 scripts/mcp-agent-workflow.py --binary $B/viamind
date -u
echo COMBINED2_FULL_GATE_PASS_BINARY $(cut -d' ' -f1 $B/viamind.sha256) 570ba277ae241789c8b5a7b90a492b29171dcde6
