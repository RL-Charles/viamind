set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0120
test "$(git rev-parse HEAD)" = 469df8fbc38839e25bd894bbe93febb114a1c3b3
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-doc --test pad_apertures --locked
cargo test -p viamind-engine --test pad_apertures --test part_commands --locked
cargo test -p viamind-fab --test pad_apertures --locked
cargo test -p viamind-render --test pad_apertures --locked
cargo test -p viamind-ui-egui --lib board_scene::aperture_tests --locked
cargo test -p viamind-ui-egui --test pad_apertures --test schematic_drag --test command_parity --locked
cargo test -p viamind-mcp --test pad_apertures --test footprint_definition --locked
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo build --workspace --locked
cp "$CARGO_TARGET_DIR/debug/viamind" /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/viamind-combined-469df8fb
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/viamind-combined-469df8fb --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/catalog-combined --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/catalog-combined-report
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/viamind-combined-469df8fb --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/workflow-combined
python3 docs/verification/t0120/production-probe.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/viamind-combined-469df8fb --repo /home/charlesmowbray/Development/viamind-t-0120 --directory /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/stdio-combined --source 469df8fbc38839e25bd894bbe93febb114a1c3b3
