import datetime,hashlib,json,os,subprocess
from pathlib import Path
home=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0020-current');dest=home/'verified';dest.mkdir()
source=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();assert source=='e7f60cf489ddb673ad416f991fb86c398c61a2ed'
report={'source':source,'source_tree':subprocess.check_output(['git','rev-parse','HEAD^{tree}'],text=True).strip(),'rustc':subprocess.check_output(['rustc','-Vv'],text=True),'built_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'binaries':{},'build':'cargo build --release --locked -p viamind-ui-egui --example native_benchmark --example gpu_parity','target':os.environ['CARGO_TARGET_DIR']}
for name in ['native_benchmark','gpu_parity']:
 data=(Path(os.environ['CARGO_TARGET_DIR'])/'release/examples'/name).read_bytes();p=dest/name
 with p.open('xb')as f:f.write(data);f.flush();os.fsync(f.fileno())
 p.chmod(0o755);report['binaries'][name]={'path':str(p),'sha256':hashlib.sha256(data).hexdigest()}
with(dest/'provenance.json').open('x')as f:json.dump(report,f,indent=2);f.flush();os.fsync(f.fileno())
with(home/'build-ready.json').open('x')as f:json.dump(report,f,indent=2);f.flush();os.fsync(f.fileno())
print(json.dumps({'stage':'unique-release-binaries-ready','source':source,'binaries':report['binaries']}),flush=True)
