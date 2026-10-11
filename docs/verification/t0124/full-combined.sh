set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0124
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
verified_source=33981e0532b7d0bba317d577d67d05bd017cb905
test "$(git rev-parse HEAD)" = "$verified_source"
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo build -p viamind-ui-egui --example schematic_sidebar_inspection --locked
cp "$CARGO_TARGET_DIR/debug/viamind" /home/charlesmowbray/.cache/viamind/native-artifacts/t0124/viamind-33981e0532b7
cp "$CARGO_TARGET_DIR/debug/examples/schematic_sidebar_inspection" /home/charlesmowbray/.cache/viamind/native-artifacts/t0124/sidebar-inspection-33981e0532b7
python3 - <<'INNER'
import hashlib,json
from pathlib import Path
p=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0124');items=[]
for name in ['viamind-33981e0532b7','sidebar-inspection-33981e0532b7']:
 f=p/name
 with f.open('rb') as stream: sha=hashlib.file_digest(stream,'sha256').hexdigest()
 items.append({'path':str(f),'sha256':sha,'source':'33981e0532b7d0bba317d577d67d05bd017cb905'})
(p/'combined-binaries.json').write_text(json.dumps(items,indent=2)+'\n')
INNER
python3 -m unittest discover -s scripts/tests -p 'test_mcp_reference.py'
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0124/viamind-33981e0532b7 --check docs/generated
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0124/viamind-33981e0532b7 --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0124/combined-workflow.json
