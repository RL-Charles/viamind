import datetime,hashlib,json,os,shlex,subprocess,time
from pathlib import Path
home=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0126')
binary=home/'trace-c0e2ccd3/gpu_parity';provenance=json.loads((binary.parent/'provenance.json').read_text())
def write_new(path,value):
 with path.open('x')as f:json.dump(value,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
assert hashlib.sha256(binary.read_bytes()).hexdigest()==provenance['sha256']
write_new(home/'hardened-capture-ready.json',{'binary':str(binary),'sha256':provenance['sha256'],'source':provenance['source'],'scope':'ONE optional hardened harness validation: same five cases/criteria; manual camera/completed-frame/root-pixel/token/input admission; no historical cause/renderer/cadence closure','utc':datetime.datetime.now(datetime.timezone.utc).isoformat()})
deadline=time.monotonic()+900
while not(home/'hardened-quiet-ready.json').exists():
 if time.monotonic()>deadline:raise RuntimeError('quietadmission absent; no native launched')
 time.sleep(1)
admission=json.loads((home/'hardened-quiet-ready.json').read_text())
assert admission['admitted']is True
assert hashlib.sha256(binary.read_bytes()).hexdigest()==provenance['sha256']
write_new(home/'hardened-system.json',{'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'loadavg':Path('/proc/loadavg').read_text().strip(),'monitors':json.loads(subprocess.check_output(['hyprctl','-j','monitors'],text=True)),'display':{k:os.environ.get(k)for k in['DISPLAY','WAYLAND_DISPLAY','XDG_SESSION_TYPE','WGPU_BACKEND']}})
report=home/'hardened-report.json';directory=home/'hardened-pairs';wrapper=home/'launch-hardened-trace';runtime=home/'hardened-runtime.log'
assert not report.exists()and not directory.exists()
with wrapper.open('x')as f:f.write('#!/bin/sh\nexec '+shlex.join([str(binary),str(report),'--trace-dir',str(directory),'--defer-trace-io','--progress-watchdog','--hardened'])+' > '+shlex.quote(str(runtime))+' 2>&1\n');f.flush();os.fsync(f.fileno())
wrapper.chmod(0o755)
started=time.monotonic()
r=subprocess.run(['cua-driver','launch_app',json.dumps({'name':str(wrapper)})],capture_output=True,text=True,timeout=60)
assert r.returncode==0,(r.stdout,r.stderr)
launch=json.loads(r.stdout);write_new(home/'hardened-launch.json',launch);pid=launch['pid'];end=started+210
while Path('/proc/'+str(pid)).exists():
 if time.monotonic()>end:raise RuntimeError(('ownedtrace didnotselfclose; preserveprocess',pid))
 time.sleep(.2)
assert hashlib.sha256(binary.read_bytes()).hexdigest()==provenance['sha256']
write_new(home/'hardened-completed.json',{'pid':pid,'selfclosed':True,'elapsed_seconds':time.monotonic()-started,'binary_sha256':provenance['sha256'],'report_exists':report.exists(),'trace_exists':(directory/'trace.json').exists(),'runtime':runtime.read_text(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'Hardened harness validation only; pixel/provenance outcomes reported independently; historical cause/stall/performance remain unresolved'})
print((home/'hardened-completed.json').read_text(),flush=True)
