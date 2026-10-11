set -euo pipefail
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
test "$(git rev-parse HEAD)" = 4eec348b04cb2b3e40ab60d3a0a5f7bd7433ea34
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-engine --lib unrouted:: --locked
cargo test -p viamind-mcp --test unrouted --locked
cargo test -p viamind-ui-egui --lib unrouted:: --locked
cargo build --workspace --locked
python3 - <<'INNER'
from pathlib import Path
import shutil,json,hashlib
root=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0123/ci-verified');root.mkdir();p=root/'viamind';shutil.copy2('/home/charlesmowbray/.cache/viamind/build-target/debug/viamind',p)
with p.open('rb') as f:sha=hashlib.file_digest(f,'sha256').hexdigest()
(root/'binary.json').write_text(json.dumps({'path':str(p),'sha256':sha,'source':'4eec348b04cb2b3e40ab60d3a0a5f7bd7433ea34'},indent=2)+'\n');print(sha)
INNER
python3 scripts/generate-mcp-reference.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0123/ci-verified/viamind --check docs/generated
