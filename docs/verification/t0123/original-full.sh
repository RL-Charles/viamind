set -euo pipefail
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target
export CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS=2
export TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
test "$(git rev-parse HEAD)" = 2d96cf0ba6ca226927245308eb4601ecfbaa62cc
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo build -p viamind-ui-egui --example t0123_inspection --locked
python3 - <<'PY'
from pathlib import Path
import hashlib, json, shutil
root=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0123/full-verified')
root.mkdir()
target=Path('/home/charlesmowbray/.cache/viamind/build-target/debug')
result={}
for name, source in [('viamind',target/'viamind'),('unrouted-inspection',target/'examples/t0123_inspection')]:
    destination=root/name
    shutil.copy2(source,destination)
    result[name]={'path':str(destination),'sha256':hashlib.file_digest(destination.open('rb'),'sha256').hexdigest()}
with (root/'binaries.json').open('x') as f: json.dump(result,f,indent=2)
print(json.dumps(result))
PY
