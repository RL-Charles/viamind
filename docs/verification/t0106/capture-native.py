"""Launch only the owned immutable inspection through Cua; read back and verify self-close."""
import datetime,json,os,subprocess,time
from pathlib import Path
root=Path(__file__).resolve().parent
binary=root/'combined-native/inspection'
source=(root/'combined-native/source.txt').read_text().strip()
for name,width,height in [('wide-final',1600,1000),('compact-final',640,400)]:
    launch=root/('launch-'+name)
    command=[str(binary),str(root/'terminals.viamind.json'),str(root/(name+'.p6')),str(width),str(height)]
    import shlex
    launch.write_text('#!/bin/sh\nexec '+shlex.join(command)+' > '+shlex.quote(str(root/(name+'-runtime.log')))+' 2>&1\n')
    launch.chmod(0o755)
    started=datetime.datetime.now(datetime.timezone.utc).isoformat()
    result=subprocess.run(['cua-driver','launch_app',json.dumps({'name':str(launch)})],capture_output=True,text=True,timeout=60,check=True)
    data=json.loads(result.stdout)
    (root/(name+'-launch.json')).write_text(json.dumps(data,indent=2)+'\n')
    deadline=time.monotonic()+20
    while time.monotonic()<deadline:
        if (root/(name+'.p6')).exists() and not Path('/proc/'+str(data['pid'])).exists():break
        time.sleep(.1)
    assert (root/(name+'.p6')).exists(),name
    assert not Path('/proc/'+str(data['pid'])).exists(),(name,'did not self-close')
    lines=[json.loads(line) for line in (root/(name+'-runtime.log')).read_text().splitlines() if line.startswith('{')]
    capture=lines[-1]
    assert capture['pixels']==[width,height] and capture['pid']==data['pid']
    assert capture['full_session_unchanged'] and capture['document_status_unchanged']
    assert len(capture['controls'])==3
    (root/(name+'-provenance.json')).write_text(json.dumps({'source':source,'launched_utc':started,'launch':data,'native':capture,'process_after_capture':'absent; self-closed','scope':'Inspection-only actual egui pointer preparation and native WGPU Screenshot. Logical focus retained; no OS-input/compositor-capture claim.'},indent=2)+'\n')
    print(name,data['pid'],'self-closed',capture['ui_passes'],'UI passes',flush=True)
