import json,hashlib,subprocess,os,argparse
from pathlib import Path
from PIL import Image
parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--project',type=Path,required=True);parser.add_argument('--directory',type=Path,required=True);parser.add_argument('--source-head',default='unspecified');args=parser.parse_args()
binary=args.binary.resolve();project=args.project.resolve();directory=args.directory.resolve();directory.mkdir(parents=True,exist_ok=True);original=project.read_bytes();source_head=args.source_head
env=os.environ.copy();env.pop('DISPLAY',None);env.pop('WAYLAND_DISPLAY',None)
report={'passed':False,'checks':[],'binary':str(binary),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'source_head':source_head,'outputs':{}}
def check(name,condition):
 assert condition,name
 report['checks'].append(name)
for arg in ['--help','--version']:
 result=subprocess.run([str(binary),arg],capture_output=True,text=True,env=env,timeout=15);check(f'production {arg} works without display',result.returncode==0)
for view in ['board','schematic']:
 for fmt in ['svg','png']:
  path=directory / f'cli-{view}.{fmt}';assert not path.exists()
  args=[str(binary),'--export-'+('schematic-' if view=='schematic' else '')+fmt,'--open',str(project),'--output',str(path)]
  if view=='schematic':args+=['--sheet','00000000-0000-0000-0000-000000000066']
  result=subprocess.run(args,capture_output=True,text=True,env=env,timeout=30);check(f'production {view} {fmt} export succeeds',result.returncode==0 and path.is_file())
  content=path.read_bytes();report['outputs'][view+'_'+fmt]={'path':str(path),'sha256':hashlib.sha256(content).hexdigest(),'bytes':len(content)}
  if fmt=='png':
   im=Image.open(path);check(f'{view} PNG independently decodes RGBA at requested default size',im.mode=='RGBA' and im.size==(1600,1000))
  else:
   import xml.etree.ElementTree as ET
   root=ET.fromstring(content);check(f'{view} SVG independently parses viewport',root.tag.endswith('svg') and root.attrib['width']=='1600' and root.attrib['height']=='1000')
  result=subprocess.run(args,capture_output=True,text=True,env=env,timeout=30);check(f'production {view} {fmt} collision preserves exact bytes',result.returncode!=0 and path.read_bytes()==content)
check('production exports preserve native file',project.read_bytes()==original)
report['passed']=True;(directory/'cli-report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
