set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0119
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
find crates apps -name '*.rs' -exec touch {} +
git rev-parse HEAD
cargo test -p viamind-fab --test footprint_artwork --test output_jobs
cargo test -p viamind-fab --lib release_inspection
cargo test -p viamind-render --test footprint_artwork
cargo test -p viamind-ui-egui --test footprint_artwork --test footprint_definition
cargo test -p viamind --test fabrication --test release_inspection
find crates apps -name '*.rs' -exec touch {} +
cargo build --workspace
cargo build -p viamind-ui-egui --example footprint_artwork_inspection
python - <<'PY'
import hashlib,json,os,subprocess
from pathlib import Path
root=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0119/combined-verified');root.mkdir()
target=Path(os.environ['CARGO_TARGET_DIR'])/'debug'
items={}
for name,src in [('viamind',target/'viamind'),('footprint-artwork-inspection',target/'examples/footprint_artwork_inspection')]:
 data=src.read_bytes();p=root/name
 with p.open('xb') as f:f.write(data);f.flush();os.fsync(f.fileno())
 p.chmod(0o755);items[name]={'path':str(p),'sha256':hashlib.sha256(data).hexdigest()}
with (root/'provenance.json').open('x') as f:json.dump({'source':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'binaries':items},f,indent=2);f.flush();os.fsync(f.fileno())
print(json.dumps(items))
PY
python scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0119/combined-verified/viamind --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0119/generated-combined
cp /home/charlesmowbray/.cache/viamind/native-artifacts/t0119/generated-combined/mcp-tools.json docs/generated/mcp-tools.json
cp /home/charlesmowbray/.cache/viamind/native-artifacts/t0119/generated-combined/mcp-tools.md docs/generated/mcp-tools.md
python scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0119/combined-verified/viamind --check docs/generated --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0119/catalog-check-combined
python scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0119/combined-verified/viamind --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0119/workflow-combined
python -m unittest discover -s scripts/tests -v
