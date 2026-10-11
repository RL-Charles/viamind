set -euo pipefail
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
test "$(git rev-parse HEAD)" = 2d96cf0ba6ca226927245308eb4601ecfbaa62cc
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo build --workspace --locked
cargo build -p viamind-ui-egui --example t0123_inspection --locked
python3 - <<'PY'
from pathlib import Path
import hashlib,json,shutil
root=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0123/full-verified');root.mkdir()
target=Path('/home/charlesmowbray/.cache/viamind/build-target/debug');result={}
for name,source in [('viamind',target/'viamind'),('unrouted-inspection',target/'examples/t0123_inspection')]:
 destination=root/name;shutil.copy2(source,destination)
 with destination.open('rb') as f:sha=hashlib.file_digest(f,'sha256').hexdigest()
 result[name]={'path':str(destination),'sha256':sha}
with (root/'binaries.json').open('x') as f:json.dump(result,f,indent=2)
print(json.dumps(result))
PY
