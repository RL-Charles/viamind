set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0120
test "$(git rev-parse HEAD)" = 2cbe6dd6e1bb1fbe9d19d1b2950313d12db902ed
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo build --workspace --locked
cargo build -p viamind-ui-egui --example pad_apertures_inspection --locked
python3 - <<'PY'
import hashlib,json,shutil,os
from pathlib import Path
base=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0120');target=Path('/home/charlesmowbray/.cache/viamind/build-target/debug');items=[]
for input_name,name in [('viamind','viamind-2cbe6dd6e1bb-fresh'),('examples/pad_apertures_inspection','aperture-inspection-2cbe6dd6e1bb-fresh')]:
 source=target/input_name;path=base/name
 with source.open('rb')as i,path.open('xb')as o:shutil.copyfileobj(i,o);o.flush();os.fsync(o.fileno())
 path.chmod(source.stat().st_mode)
 with path.open('rb')as f:sha=hashlib.file_digest(f,'sha256').hexdigest()
 previous=base/name.removesuffix('-fresh')
 with previous.open('rb')as f:oldsha=hashlib.file_digest(f,'sha256').hexdigest()
 items.append({'path':str(path),'source':'2cbe6dd6e1bb1fbe9d19d1b2950313d12db902ed','sha256':sha,'bytes':path.stat().st_size,'previous_binary_sha256':oldsha,'bit_identical_to_full_gate_binary':sha==oldsha})
with(base/'fresh-binaries.json').open('x')as f:json.dump(items,f,indent=2);f.write('\n')
PY
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/viamind-2cbe6dd6e1bb-fresh --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/catalog-fresh --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/catalog-fresh-report
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/viamind-2cbe6dd6e1bb-fresh --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/workflow-fresh
python3 docs/verification/t0120/production-probe.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/viamind-2cbe6dd6e1bb-fresh --repo /home/charlesmowbray/Development/viamind-t-0120 --directory /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/stdio-fresh --source 2cbe6dd6e1bb1fbe9d19d1b2950313d12db902ed
