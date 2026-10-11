#!/usr/bin/env bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0118
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
expected=$1
flock /tmp/viamind-workspace-gates.lock bash -s -- "$expected" <<'GATE'
set -euo pipefail
test "$(git rev-parse HEAD)" = "$1"
test -z "$(git status --porcelain --untracked-files=normal -- crates apps scripts)"
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo build --workspace --locked
cargo build -p viamind-ui-egui --example command_hooks_inspection --locked
python3 - <<'PY'
import hashlib,json,pathlib,shutil,subprocess
root=pathlib.Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0118/full-verified');root.mkdir()
target=pathlib.Path('/home/charlesmowbray/.cache/viamind/build-target/debug');bins={}
for name,p in [('viamind',target/'viamind'),('command-hooks-inspection',target/'examples/command_hooks_inspection')]:
 dst=root/name;shutil.copy2(p,dst);bins[name]={'path':str(dst),'sha256':hashlib.file_digest(dst.open('rb'),'sha256').hexdigest()}
report={'source':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_tree':subprocess.check_output(['git','rev-parse','HEAD^{tree}'],text=True).strip(),'binaries':bins,'gates':'fmtcheck strictworkspacealltargetclippy fullworkspace tests/build plus inspection build'}
with(root/'provenance.json').open('x')as f:json.dump(report,f,indent=2)
print(json.dumps(report))
PY
GATE
