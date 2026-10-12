set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0121
test "$(git rev-parse HEAD)" = 9f900007676cbc0b23f61bac059a11968d540fb6
test -z "$(git status --porcelain=v1)"
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 PYTHONDONTWRITEBYTECODE=1 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
E=/home/charlesmowbray/.cache/viamind/native-artifacts/t0121
date -u +GATE_START_%FT%TZ
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
echo GATE_FMT_PASS
cargo clippy --workspace --all-targets --locked -- -D warnings
echo GATE_CLIPPY_PASS
cargo test --workspace --locked --no-fail-fast
echo GATE_TEST_PASS
cargo build -p viamind-ui-egui --example nonplated_holes_inspection --locked
cp "$CARGO_TARGET_DIR/debug/examples/nonplated_holes_inspection" $E/inspection-9f900007
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo build --workspace --locked
echo GATE_BUILD_PASS
cp "$CARGO_TARGET_DIR/debug/viamind" $E/viamind-9f900007
sha256sum $E/viamind-9f900007 $E/inspection-9f900007 | tee $E/final2-binaries.sha256
python3 -m unittest discover -s scripts/tests -p 'test_mcp_reference.py'
python3 scripts/generate-mcp-reference.py --binary $E/viamind-9f900007 --output $E/catalog-9f900007 --report $E/catalog-9f900007-report
python3 scripts/mcp-agent-workflow.py --binary $E/viamind-9f900007 --output $E/workflow-9f900007
python3 $E/production-probe.py --binary $E/viamind-9f900007 --repo /home/charlesmowbray/Development/viamind-t-0121 --directory $E/stdio-9f900007 --source 9f900007676cbc0b23f61bac059a11968d540fb6
python3 $E/checked-production-probe.py --binary $E/viamind-9f900007 --repo /home/charlesmowbray/Development/viamind-t-0121 --native $E/stdio-9f900007/authored.viamind.json --directory $E/checked-stdio-9f900007 --source 9f900007676cbc0b23f61bac059a11968d540fb6
date -u +GATE_END_%FT%TZ
printf 'FINAL2_COMPLETE\n'
