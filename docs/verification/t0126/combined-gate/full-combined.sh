set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0126
test "$(git rev-parse HEAD)" = f4320d2f0a235456f33d2be6173b8e999b0a1723
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 PYTHONDONTWRITEBYTECODE=1
export TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
export VIAMIND_BUILD_COMMIT=f4320d2f0a235456f33d2be6173b8e999b0a1723
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --locked -p viamind-ui-egui --example gpu_parity -- --nocapture
cargo build --workspace --locked
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py -v
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/build-target/debug/viamind --check docs/generated --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0126/combined-catalog-report
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/build-target/debug/viamind
python3 - <<'COPY'
import datetime,hashlib,json,os
from pathlib import Path
src=Path('/home/charlesmowbray/.cache/viamind/build-target/debug/viamind')
dest=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0126/combined-f4320d2f');dest.mkdir()
data=src.read_bytes();binary=dest/'viamind'
with binary.open('xb')as f:f.write(data);f.flush();os.fsync(f.fileno())
binary.chmod(0o755)
with (dest/'provenance.json').open('x')as f:json.dump({'source':'f4320d2f0a235456f33d2be6173b8e999b0a1723','base':'88c3ec12db48e7fff0aae84ca0d96bbf65ab668c','binary':str(binary),'sha256':hashlib.sha256(data).hexdigest(),'built_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'Combined full four Linux workspace gates,22 gpu_parity diagnostic tests,13 bounded catalog-tool tests,exact generated reference and isolated workflow; no new native capture,renderer correction,historical cause or cadence claim'},f,indent=2);f.flush();os.fsync(f.fileno())
print('COMBINED_FULL_GATE_PASS_BINARY',hashlib.sha256(data).hexdigest(),flush=True)
COPY
