set -eu
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
git rev-parse HEAD
date -u
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-doc --test package_links --test symbol_package --locked
cargo test -p viamind-engine --lib pcb_branches::budget
cargo test -p viamind-engine --test package_component --test package_annotation --test symbol_package --test pcb_sync --test component_links --test pin_pad_links --locked
cargo test -p viamind-ui-egui --lib package_component::tests --locked
cargo test -p viamind-ui-egui --lib cross_probe::tests --locked
cargo test -p viamind-ui-egui --test package_annotation --test package_compact --test command_parity --locked
cargo test -p viamind-mcp --test package_annotation --test symbol_packages --test pcb_sync --test pin_pad_links --locked
cargo build --workspace --locked
cargo build -p viamind-ui-egui --example t0116_inspection --locked
cp --reflink=auto "$CARGO_TARGET_DIR/debug/viamind" /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/production-final-combined-verified
cp --reflink=auto "$CARGO_TARGET_DIR/debug/examples/t0116_inspection" /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/inspection-final-combined-verified
strip /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/production-final-combined-verified /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/inspection-final-combined-verified
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/production-final-combined-verified /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/inspection-final-combined-verified
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/production-final-combined-verified --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/combined-generated --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/combined-catalog-report
cp /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/combined-generated/mcp-tools.json docs/generated/mcp-tools.json
cp /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/combined-generated/mcp-tools.md docs/generated/mcp-tools.md
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/production-final-combined-verified --check docs/generated --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/combined-catalog-drift
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/production-final-combined-verified --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0116/combined-workflow-report
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py
date -u
