#!/bin/bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0125
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p viamind-engine --lib schematic_drag::tests -- --nocapture
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo build --workspace
cargo build -p viamind-ui-egui --example t0125_inspection
python3 - <<'COPY'
from pathlib import Path
import hashlib, subprocess, json
out=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0125/final-verified');out.mkdir()
records={}
for name,source in [('viamind','debug/viamind'),('connected-drag-inspection','debug/examples/t0125_inspection')]:
 target=out/name
 with target.open('xb') as f:
  with (Path('/home/charlesmowbray/.cache/viamind/build-target')/source).open('rb') as g:
   while block:=g.read(1024*1024):f.write(block)
  f.flush()
 target.chmod(0o755)
 subprocess.run(['strip',str(target)],check=True)
 with target.open('rb') as f: records[name]={'path':str(target),'sha256':hashlib.file_digest(f,'sha256').hexdigest()}
with (out/'identity.json').open('x') as f:json.dump({'source':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'binaries':records},f,indent=2)
COPY
python3 -B scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0125/final-verified/viamind --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0125/catalog-final --report /home/charlesmowbray/.cache/viamind/native-artifacts/t0125/catalog-final-report
python3 -B scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0125/final-verified/viamind --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0125/workflow-final
python3 -B -m unittest discover -s scripts/tests -v
