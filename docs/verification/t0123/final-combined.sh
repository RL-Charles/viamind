set -euo pipefail
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
test "$(git rev-parse HEAD)" = 6dbbbd6$(git rev-parse HEAD | cut -c8-)
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-engine unrouted:: --locked
cargo test -p viamind-mcp --test unrouted --locked
cargo test -p viamind-ui-egui unrouted:: --lib --locked
cargo test -p viamind-ui-egui --test unrouted --test command_parity --locked
cargo build --workspace --locked
cargo build -p viamind-ui-egui --example t0123_inspection --locked
python3 - <<'INNER'
from pathlib import Path
import hashlib,json,shutil
root=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0123/final-verified');root.mkdir()
target=Path('/home/charlesmowbray/.cache/viamind/build-target/debug');result={}
for name,source in [('viamind',target/'viamind'),('unrouted-inspection',target/'examples/t0123_inspection')]:
 destination=root/name;shutil.copy2(source,destination)
 with destination.open('rb') as f:sha=hashlib.file_digest(f,'sha256').hexdigest()
 result[name]={'path':str(destination),'sha256':sha}
with (root/'binaries.json').open('x') as f:json.dump(result,f,indent=2)
print(json.dumps(result))
INNER
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0123/final-verified/viamind --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0123/catalog-final
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0123/final-verified/viamind --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0123/workflow-final
python3 -m unittest discover -s scripts/tests -p test_mcp_reference.py
