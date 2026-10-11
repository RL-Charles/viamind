"""Launch only owned immutable native editor inspections; verify fresh readback/self-close."""
import datetime,json,os,shlex,subprocess,time
from pathlib import Path
root=Path(__file__).resolve().parent
binary=root/'package-annotation-inspection-full-source';source=(root/'full-source.txt').read_text().strip() 
for name,width,height,kind in [('annotation-wide',1600,1000,None),('annotation-compact',640,400,None)]:
 launch=root/('launch-'+name);command=[str(binary),str(root/'packages.viamind.json'),str(root/(name+'.p6')),str(width),str(height)]
 launch.write_text('#!/bin/sh\nunset WAYLAND_DISPLAY WAYLAND_SOCKET\nexec '+shlex.join(command)+' > '+shlex.quote(str(root/(name+'-runtime.log')))+' 2>&1\n');launch.chmod(0o755)
 started=datetime.datetime.now(datetime.timezone.utc).isoformat();result=subprocess.run(['cua-driver','launch_app',json.dumps({'name':str(launch)})],capture_output=True,text=True,timeout=60,check=True);data=json.loads(result.stdout);(root/(name+'-launch.json')).write_text(json.dumps(data,indent=2)+'\n')
 deadline=time.monotonic()+30
 while time.monotonic()<deadline:
  if (root/(name+'.p6')).exists() and not Path('/proc/'+str(data['pid'])).exists():break
  time.sleep(.1)
 assert (root/(name+'.p6')).exists(),(name,(root/(name+'-runtime.log')).read_text())
 assert not Path('/proc/'+str(data['pid'])).exists(),(name,'did not self-close')
 lines=[json.loads(line)for line in (root/(name+'-runtime.log')).read_text().splitlines()if line.startswith('{')];capture=lines[-1]
 assert 400<=capture['pixels'][0] and 300<=capture['pixels'][1] and capture['pixels'][0]*capture['pixels'][1]<=16777216 and capture['pid']==data['pid'];assert capture['full_session_unchanged']and capture['document_status_unchanged'];assert len(capture['controls'])==3
 (root/(name+'-provenance.json')).write_text(json.dumps({'source':source,'launched_utc':started,'launch':data,'native':capture,'process_after_capture':'absent; self-closed','scope':'Inspection-only actual egui options/input and immutable shared preview and native WGPU Screenshot. Logical focus retained; no OS-input/compositor-capture or cadence claim.'},indent=2)+'\n')
 print(name,data['pid'],'self-closed',capture['ui_passes'],'UI passes',flush=True)
