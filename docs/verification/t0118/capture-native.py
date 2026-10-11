import argparse,datetime,hashlib,json,os,pathlib,shlex,subprocess,time
p=argparse.ArgumentParser();p.add_argument('--verified',type=pathlib.Path,required=True);p.add_argument('--directory',type=pathlib.Path,required=True);a=p.parse_args();root=a.directory.resolve();root.mkdir();verified=a.verified.resolve();binary=verified/'command-hooks-inspection';source=json.loads((verified/'provenance.json').read_text())['source']
for mode in ['wide','veto','recovery']:
 name='hooks-'+mode;command=[str(binary),mode,str(root/(name+'.p6')),str(root/(name+'.viamind.json')),str(root/(name+'-state.json'))];launch=root/('launch-'+name)
 launch.write_text('#!/bin/sh\nunset WAYLAND_DISPLAY WAYLAND_SOCKET\nexec '+shlex.join(command)+' > '+shlex.quote(str(root/(name+'-runtime.log')))+' 2>&1\n');launch.chmod(0o755)
 started=datetime.datetime.now(datetime.timezone.utc).isoformat();r=subprocess.run(['cua-driver','launch_app',json.dumps({'name':str(launch)})],capture_output=True,text=True,timeout=60);assert r.returncode==0,(r.stdout,r.stderr);data=json.loads(r.stdout)
 with(root/(name+'-launch.json')).open('x')as f:json.dump(data,f,indent=2)
 deadline=time.monotonic()+30
 while time.monotonic()<deadline:
  if(root/(name+'.p6')).exists()and not pathlib.Path('/proc/'+str(data['pid'])).exists():break
  time.sleep(.1)
 log=(root/(name+'-runtime.log')).read_text();assert(root/(name+'.p6')).exists(),log;assert not pathlib.Path('/proc/'+str(data['pid'])).exists(),log
 records=[json.loads(line)for line in log.splitlines()if line.startswith('{')];capture=records[-1];assert capture['pid']==data['pid'] and capture['full_session_unchanged']and capture['document_status_unchanged']and capture['native_bytes_unchanged']and capture['self_close_after_capture']and len(capture['controls_and_effective_clips'])==2
 with(root/(name+'-provenance.json')).open('x')as f:json.dump({'source':source,'binary':str(binary),'binary_sha256':hashlib.file_digest(binary.open('rb'),'sha256').hexdigest(),'launched_utc':started,'launch':data,'native':capture,'process_after_capture':'absent; self-closed','scope':'Actual local egui input preparation and production UI native WGPU Screenshot. Original Color32 RGBA retained, no OS input/compositor/opacity/cadence claim.'},f,indent=2)
 print(name,data['pid'],'self-closed',flush=True)
