set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0020-baseline
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/native-benchmark-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1 VIAMIND_BUILD_COMMIT=e7f60cf489ddb673ad416f991fb86c398c61a2ed
find crates apps -name '*.rs' -exec touch {} +
cargo build --release --locked -p viamind-ui-egui --example native_benchmark --example gpu_parity
python /home/charlesmowbray/.cache/viamind/native-artifacts/t0020-current/retain-build.py
python /home/charlesmowbray/.cache/viamind/native-artifacts/t0020-current/run-baseline.py
