#!/bin/bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0119
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
find crates apps -name '*.rs' -exec touch {} +
git rev-parse HEAD
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
cargo build -p viamind-ui-egui --example footprint_artwork_inspection
python3 - <<'PY'
import hashlib,json,os,shutil,subprocess
from pathlib import Path
root=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0119/full-verified');target=Path(os.environ['CARGO_TARGET_DIR'])/'debug';source=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();binaries={}
for src,name in [(target/'viamind','viamind'),(target/'examples/footprint_artwork_inspection','footprint-artwork-inspection')]:
 dest=root/name
 with src.open('rb')as r,dest.open('xb')as w:shutil.copyfileobj(r,w);w.flush();os.fsync(w.fileno())
 dest.chmod(0o755);binaries[name]={'path':str(dest),'sha256':hashlib.sha256(dest.read_bytes()).hexdigest()}
with (root/'provenance.json').open('x')as f:json.dump({'source':source,'binaries':binaries,'gates':'fmtcheck strictworkspacealltargetclippy fullworkspace tests/build plus inspection build'},f,indent=2);f.flush();os.fsync(f.fileno())
print(json.dumps(binaries))
PY
