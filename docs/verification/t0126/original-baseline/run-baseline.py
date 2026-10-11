import datetime,hashlib,json,os,shlex,subprocess,threading,time
from pathlib import Path
home=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0020-current');verified=home/'verified'
def utc():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def write_new(path,value):
 with path.open('x')as f:json.dump(value,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def system():
 monitors=json.loads(subprocess.check_output(['hyprctl','-j','monitors'],text=True))
 return {'utc':utc(),'cpu_model':next(line.split(':',1)[1].strip()for line in Path('/proc/cpuinfo').read_text().splitlines()if line.startswith('model name')),'logical_cpus':os.cpu_count(),'loadavg':Path('/proc/loadavg').read_text().strip(),'meminfo':{k:v for k,v in (line.split(':',1)for line in Path('/proc/meminfo').read_text().splitlines())if k in ['MemTotal','MemAvailable','SwapTotal','SwapFree']},'kernel':subprocess.check_output(['uname','-srmo'],text=True).strip(),'display_environment':{k:os.environ.get(k)for k in ['DISPLAY','WAYLAND_DISPLAY','XDG_SESSION_TYPE','WGPU_BACKEND']},'monitors':[{k:m.get(k)for k in ['name','description','width','height','refreshRate','x','y','scale','transform','focused']}for m in monitors],'cpu_processes':subprocess.check_output(['ps','-eo','comm,%cpu,%mem','--sort=-%cpu'],text=True).splitlines()[:13]}
print('WAITING_FOR_EXPLICIT_QUIET_WINDOW: unique release copies ready',flush=True)
deadline=time.monotonic()+900
while not(home/'quiet-ready.json').exists():
 if time.monotonic()>deadline:raise RuntimeError('quiet admission absent; no native benchmark launched')
 time.sleep(1)
admission=json.loads((home/'quiet-ready.json').read_text());assert admission['admitted']is True
write_new(home/'system-before.json',system())
provenance=json.loads((verified/'provenance.json').read_text())
def run(name,output,timeout):
 binary=verified/name;assert hashlib.sha256(binary.read_bytes()).hexdigest()==provenance['binaries'][name]['sha256']
 wrapper=home/('launch-'+name);runtime=home/(name+'-runtime.log')
 wrapper.write_text('#!/bin/sh\nexec '+shlex.join([str(binary),str(output)])+' > '+shlex.quote(str(runtime))+' 2>&1\n');wrapper.chmod(0o755)
 started=utc();result=subprocess.run(['cua-driver','launch_app',json.dumps({'name':str(wrapper)})],capture_output=True,text=True,timeout=60)
 assert result.returncode==0,(result.stdout,result.stderr);launch=json.loads(result.stdout);write_new(home/(name+'-launch.json'),launch)
 pid=launch['pid'];end=time.monotonic()+timeout
 while Path('/proc/'+str(pid)).exists():
  if time.monotonic()>end:raise RuntimeError((name,'did not selfclose; preserve process',pid))
  time.sleep(.2)
 assert output.exists(),(name,'missing report',runtime.read_text())
 report=json.loads(output.read_text());assert hashlib.sha256(binary.read_bytes()).hexdigest()==provenance['binaries'][name]['sha256']
 return {'start_utc':started,'complete_utc':utc(),'pid':pid,'process_after':'absent; own example selfclosed','binary':str(binary),'binary_sha256':provenance['binaries'][name]['sha256'],'report':str(output)},report
parity_run,parity=run('gpu_parity',home/'parity.json',180)
assert parity['passed']and len(parity['cases'])==5 and all(c['drawn']>0 for c in parity['cases']),parity
stricter=all(c['max_channel_difference']<=2 and c['pixels_over_2_channels']==0 for c in parity['cases'])
print(json.dumps({'stage':'parity-complete','builtin_pass':parity['passed'],'historic_max2_zero_over2':stricter,'cases':parity['cases']}),flush=True)
load_samples=[];stop=threading.Event()
def sample():
 while not stop.is_set():
  load_samples.append({'utc':utc(),'loadavg':Path('/proc/loadavg').read_text().strip()});stop.wait(2)
thread=threading.Thread(target=sample);thread.start()
try:cadence_run,cadence=run('native_benchmark',home/'cadence.json',300)
finally:stop.set();thread.join()
assert cadence['objects']==100000 and cadence['completed_native_frames']==1260 and cadence['sample_frames_per_phase']==360 and cadence['warmup_frames_per_phase']==60
assert cadence['gpu_timestamp_instrumentation']is False and cadence['canvas_renderer']=='persistent_gpu_layer_uniforms'
assert [p['name']for p in cadence['phases']]==['fit_pan','fit_pan_zoom','zoomed_8x_pan']
for phase in cadence['phases']:
 assert phase['frame_interval']['samples']==360 and phase['drawn_min']>0
 expected=phase['frame_interval']['throughput_fps']>=59.5 and phase['frame_interval']['p95_ms']<=16.9;assert phase['meets_60fps']==expected
assert cadence['phases'][0]['drawn_min']==cadence['phases'][0]['drawn_max']==100000
write_new(home/'system-after.json',system());write_new(home/'load-samples.json',load_samples)
report={'source':provenance['source'],'spec':'docs/specs/native-canvas-benchmark.md','quiet_admission':admission,'parity_run':parity_run,'cadence_run':cadence_run,'parity_builtin_pass':parity['passed'],'historic_stricter_parity_max2_zero_over2':stricter,'strict_all_three_phases_pass':all(p['meets_60fps']for p in cadence['phases']),'cadence':cadence,'scope':'One unchanged ordinary current release baseline on synthetic100k; existing five-case parity on synthetic5000 plus via/quad/guide. Default native session DPI, no screenshot/input during timing, unchanged objects/thresholds. Native cadence is not physical scanout. Existing historical GPU timestamps remain separate.'}
write_new(home/'verification.json',report);print(json.dumps({'stage':'baseline-complete','strict_all_three_phases_pass':report['strict_all_three_phases_pass'],'phases':cadence['phases']}),flush=True)
