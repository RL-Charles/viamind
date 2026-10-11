set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0119
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
find crates apps -name '*.rs' -exec touch {} +
git rev-parse HEAD
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
find crates apps -name '*.rs' -exec touch {} +
cargo build -p viamind-ui-egui --example footprint_artwork_inspection
python - <<'PY'
import hashlib,json,os,subprocess
from pathlib import Path
root=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0119/inspection-final-verified');root.mkdir();src=Path(os.environ['CARGO_TARGET_DIR'])/'debug/examples/footprint_artwork_inspection';data=src.read_bytes();p=root/'footprint-artwork-inspection'
with p.open('xb') as f:f.write(data);f.flush();os.fsync(f.fileno())
p.chmod(0o755)
with (root/'provenance.json').open('x') as f:json.dump({'source':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'binary':str(p),'sha256':hashlib.sha256(data).hexdigest(),'scope':'Inspection-only outer wheel/effective clip assertions; product remains d09dfe0'},f,indent=2);f.flush();os.fsync(f.fileno())
print(hashlib.sha256(data).hexdigest())
PY
