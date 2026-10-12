set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0120
test "$(git rev-parse HEAD)" = b3cf88f1c661899bdadade348af5570e448daba0
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
cp "$CARGO_TARGET_DIR/debug/viamind" /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/viamind-combined-b3cf88f1
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/viamind-combined-b3cf88f1 --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/catalog-final --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/catalog-final-report
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/viamind-combined-b3cf88f1 --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/workflow-final
python3 docs/verification/t0120/production-probe.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/viamind-combined-b3cf88f1 --repo /home/charlesmowbray/Development/viamind-t-0120 --directory /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/stdio-final --source b3cf88f1c661899bdadade348af5570e448daba0
