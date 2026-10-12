set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0126
test "$(git rev-parse HEAD)" = ff180e3e0324dfbe7d925d250bf7c0e5131431bc
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/native-benchmark-target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
export TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
export VIAMIND_BUILD_COMMIT=ff180e3e0324dfbe7d925d250bf7c0e5131431bc
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --release --locked -p viamind-ui-egui --all-targets -- -D warnings
cargo test --release --locked -p viamind-ui-egui --lib canvas::tests:: -- --nocapture
cargo test --release --locked -p viamind-ui-egui --example gpu_parity -- --nocapture
cargo build --release --locked -p viamind-ui-egui --example gpu_parity
python - <<'PY'
import datetime,hashlib,json,os
from pathlib import Path
src=Path('/home/charlesmowbray/.cache/viamind/native-benchmark-target/release/examples/gpu_parity')
dest=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0126/trace-ff180e3');dest.mkdir()
data=src.read_bytes();binary=dest/'gpu_parity'
with binary.open('xb')as f:f.write(data);f.flush();os.fsync(f.fileno())
binary.chmod(0o755)
with (dest/'provenance.json').open('x')as f:json.dump({'source':'ff180e3e0324dfbe7d925d250bf7c0e5131431bc','binary':str(binary),'sha256':hashlib.sha256(data).hexdigest(),'built_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'diagnostic-only optional trace on original e7 product; fmt, release example strictlint, all retained and added example admission/ownership/privacy/layout/progress tests plus canvas manual/focus tests and release examplebuild; no fullworkspace gate claim'},f,indent=2);f.flush();os.fsync(f.fileno())
print('UNIQUE_DIAGNOSTIC_BINARY_READY',hashlib.sha256(data).hexdigest(),flush=True)
PY
