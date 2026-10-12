set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0121
test "$(git rev-parse HEAD)" = 0b3b377f3e47393faaecf2b5a386ae47fefc7663
test -z "$(git status --porcelain=v1)"
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 PYTHONDONTWRITEBYTECODE=1 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
E=/home/charlesmowbray/.cache/viamind/native-artifacts/t0121
date -u +GATE_START_%FT%TZ
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
echo GATE_FMT_PASS
cargo clippy --workspace --all-targets --locked -- -D warnings
echo GATE_CLIPPY_PASS
cargo test --workspace --locked
echo GATE_TEST_PASS
cargo build -p viamind-ui-egui --example nonplated_holes_inspection --locked
cp "$CARGO_TARGET_DIR/debug/examples/nonplated_holes_inspection" $E/inspection-0b3b377f
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo build --workspace --locked
echo GATE_BUILD_PASS
cp "$CARGO_TARGET_DIR/debug/viamind" $E/viamind-0b3b377f
sha256sum $E/viamind-0b3b377f $E/inspection-0b3b377f | tee $E/final1-binaries.sha256
python3 -m unittest discover -s scripts/tests -p 'test_mcp_reference.py'
python3 scripts/generate-mcp-reference.py --binary $E/viamind-0b3b377f --output $E/catalog-0b3b377f --report $E/catalog-0b3b377f-report
python3 scripts/mcp-agent-workflow.py --binary $E/viamind-0b3b377f --output $E/workflow-0b3b377f
python3 $E/production-probe.py --binary $E/viamind-0b3b377f --repo /home/charlesmowbray/Development/viamind-t-0121 --directory $E/stdio-0b3b377f --source 0b3b377f3e47393faaecf2b5a386ae47fefc7663
python3 $E/checked-production-probe.py --binary $E/viamind-0b3b377f --repo /home/charlesmowbray/Development/viamind-t-0121 --native $E/stdio-0b3b377f/authored.viamind.json --directory $E/checked-stdio-0b3b377f --source 0b3b377f3e47393faaecf2b5a386ae47fefc7663
date -u +GATE_END_%FT%TZ
printf 'FINAL1_COMPLETE\n'
