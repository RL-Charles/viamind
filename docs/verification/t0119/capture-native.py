"""Owned inspection-only production UI and WGPU readback, no desktop input injection."""
import argparse,datetime,hashlib,json,os,shlex,subprocess,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--native',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);p.add_argument('--verified',type=Path,required=True);a=p.parse_args();root=a.directory.resolve();root.mkdir();verified=a.verified.resolve();binary=verified/'footprint-artwork-inspection';source=json.loads((verified/'provenance.json').read_text())['source']
for name,width,height,mode in [('artwork-wide',1600,1000,'editor'),('artwork-compact',640,400,'editor'),('board-wide',1600,1000,'board')]:
 command=[str(binary),str(a.native.resolve()),str(root/(name+'.p6')),str(width),str(height),mode];launch=root/('launch-'+name)
 launch.write_text('#!/bin/sh\nunset WAYLAND_DISPLAY WAYLAND_SOCKET\nexec '+shlex.join(command)+' > '+shlex.quote(str(root/(name+'-runtime.log')))+' 2>&1\n');launch.chmod(0o755)
 started=datetime.datetime.now(datetime.timezone.utc).isoformat();result=subprocess.run(['cua-driver','launch_app',json.dumps({'name':str(launch)})],capture_output=True,text=True,timeout=60);assert result.returncode==0,(result.stdout,result.stderr);data=json.loads(result.stdout);(root/(name+'-launch.json')).write_text(json.dumps(data,indent=2)+'\n')
 deadline=time.monotonic()+30
 while time.monotonic()<deadline:
  if (root/(name+'.p6')).exists()and not Path('/proc/'+str(data['pid'])).exists():break
  time.sleep(.1)
 assert (root/(name+'.p6')).exists(),(name,(root/(name+'-runtime.log')).read_text());assert not Path('/proc/'+str(data['pid'])).exists(),(name,'host did not self-close')
 lines=[json.loads(line)for line in(root/(name+'-runtime.log')).read_text().splitlines()if line.startswith('{')];capture=lines[-1]
 assert capture['pid']==data['pid']and capture['full_session_unchanged']and capture['document_status_unchanged']and capture['self_close_after_capture'];assert len(capture['controls'])==(0 if mode=='board'else 3)
 with(root/(name+'-provenance.json')).open('x')as f:json.dump({'source':source,'binary':str(binary),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'launched_utc':started,'launch':data,'native':capture,'process_after_capture':'absent; self-closed','scope':'Inspection-only actual egui preparation and native WGPU Screenshot, current native footer responses by safe widget IDs. Logical focus retained; no OS input, compositor capture, opacity or cadence claim.'},f,indent=2);f.flush();os.fsync(f.fileno())
 print(name,data['pid'],'self-closed',capture['ui_passes'],'UI passes',flush=True)
