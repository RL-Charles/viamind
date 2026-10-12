set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0121
test "$(git rev-parse HEAD)" = a530c641afa2ee0ea4fabd158182b0fe7315dc4a
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
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo build --workspace --locked
echo GATE_BUILD_PASS
cp "$CARGO_TARGET_DIR/debug/viamind" $E/viamind-a530c641
sha256sum $E/viamind-a530c641 "$CARGO_TARGET_DIR/debug/examples/nonplated_holes_inspection" | tee $E/final4-binaries.sha256
python3 -m unittest discover -s scripts/tests -p 'test_mcp_reference.py'
python3 scripts/generate-mcp-reference.py --binary $E/viamind-a530c641 --check docs/generated --report $E/catalog-a530c641-report
python3 scripts/mcp-agent-workflow.py --binary $E/viamind-a530c641 --output $E/workflow-a530c641
python3 $E/production-probe.py --binary $E/viamind-a530c641 --repo /home/charlesmowbray/Development/viamind-t-0121 --directory $E/stdio-a530c641 --source a530c641afa2ee0ea4fabd158182b0fe7315dc4a
python3 $E/checked-production-probe.py --binary $E/viamind-a530c641 --repo /home/charlesmowbray/Development/viamind-t-0121 --native $E/stdio-a530c641/authored.viamind.json --directory $E/checked-stdio-a530c641 --source a530c641afa2ee0ea4fabd158182b0fe7315dc4a
PYTHONDONTWRITEBYTECODE=1 python3 -I docs/verification/t0121/verify-artifacts.py --retained
date -u +GATE_END_%FT%TZ
printf 'FINAL4_COMPLETE\n'
