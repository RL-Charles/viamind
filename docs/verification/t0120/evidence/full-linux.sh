set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0120
test "$(git rev-parse HEAD)" = 2cbe6dd6e1bb1fbe9d19d1b2950313d12db902ed
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo build -p viamind-ui-egui --example pad_apertures_inspection --locked
cp "$CARGO_TARGET_DIR/debug/viamind" /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/viamind-2cbe6dd6e1bb
cp "$CARGO_TARGET_DIR/debug/examples/pad_apertures_inspection" /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/aperture-inspection-2cbe6dd6e1bb
python3 - <<'INNER'
import hashlib,json
from pathlib import Path
base=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0120');items=[]
for name in ['viamind-2cbe6dd6e1bb','aperture-inspection-2cbe6dd6e1bb']:
 path=base/name
 with path.open('rb')as f:sha=hashlib.file_digest(f,'sha256').hexdigest()
 items.append({'path':str(path),'source':'2cbe6dd6e1bb1fbe9d19d1b2950313d12db902ed','sha256':sha,'bytes':path.stat().st_size})
with(base/'full-binaries.json').open('x')as f:json.dump(items,f,indent=2);f.write('\n')
INNER
python3 -m unittest discover -s scripts/tests -p 'test_mcp_reference.py'
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/viamind-2cbe6dd6e1bb --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/catalog --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/catalog-report
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/viamind-2cbe6dd6e1bb --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/workflow
python3 docs/verification/t0120/production-probe.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/viamind-2cbe6dd6e1bb --repo /home/charlesmowbray/Development/viamind-t-0120 --directory /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/stdio --source 2cbe6dd6e1bb1fbe9d19d1b2950313d12db902ed
