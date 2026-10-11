set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0115
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
find crates apps -name '*.rs' -exec touch {} +
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p viamind-engine --lib annotation_bounds
cargo test -p viamind-engine --test annotation --test package_annotation --test symbol_package
cargo test -p viamind-mcp --test annotation --test package_annotation --test symbol_packages
cargo test -p viamind-ui-egui --test annotation --test package_annotation --test package_compact
cargo build --workspace
cp "$CARGO_TARGET_DIR/debug/viamind" /home/charlesmowbray/.cache/viamind/native-artifacts/t0115/viamind-combined
strip /home/charlesmowbray/.cache/viamind/native-artifacts/t0115/viamind-combined
sha256sum /home/charlesmowbray/.cache/viamind/native-artifacts/t0115/viamind-combined
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0115/viamind-combined --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0115/combined-generated --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0115/combined-catalog-report
cp /home/charlesmowbray/.cache/viamind/native-artifacts/t0115/combined-generated/mcp-tools.json docs/generated/mcp-tools.json
cp /home/charlesmowbray/.cache/viamind/native-artifacts/t0115/combined-generated/mcp-tools.md docs/generated/mcp-tools.md
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0115/viamind-combined --check docs/generated
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0115/viamind-combined
