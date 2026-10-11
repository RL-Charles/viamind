"""Production fabrication CLI/native HTTP parity; requires a NEW evidence directory."""
import argparse, hashlib, importlib.util, json, os, subprocess
from pathlib import Path
p=argparse.ArgumentParser()
for name in ['binary','project','directory','repo']:
 p.add_argument('--'+name,type=Path,required=True)
p.add_argument('--previous-binary',type=Path);p.add_argument('--endpoint',required=True);a=p.parse_args()
root=a.directory.resolve();root.mkdir(parents=True,exist_ok=False)
project=a.project.resolve();original=project.read_bytes();env=os.environ.copy()
for name in ['DISPLAY','WAYLAND_DISPLAY']:env.pop(name,None)
report={'passed':False,'checks':[],'endpoint':a.endpoint,'production_binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'previous_binary_sha256':hashlib.sha256(a.previous_binary.read_bytes()).hexdigest() if a.previous_binary else None,'outputs':{}}
def check(name,value):
 assert value,name
 report['checks'].append(name)
def cli(binary,path,mask=None):
 args=[str(binary.resolve()),'--export-gerber','--open',str(project),'--output',str(path)]
 if mask is not None:args+=['--mask-expansion-mm',str(mask)]
 return subprocess.run(args,env=env,capture_output=True,text=True,timeout=30)
def files(path):return {p.name:p.read_bytes() for p in path.iterdir()}
spec=importlib.util.spec_from_file_location('mcp_smoke',a.repo.resolve()/'scripts/mcp-smoke.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
reader=m.McpClient(a.endpoint,'fabrication-control-reader');editor=m.McpClient(a.endpoint,'fabrication-control-editor')
try:
 before=reader.call('get_session_state',{});status=reader.call('get_document_status',{});active=Path(status['path']);active_bytes=active.read_bytes();report['native_source_cursor']=before['cursor']
 tools={t['name']:t for t in reader.rpc('tools/list')['tools']};schema=tools['export_fabrication']['inputSchema']
 check('unchanged shared fabrication schema advertises NEW path and default mask',schema['required']==['path'] and schema['properties']['mask_expansion_mm']['default']==0 and schema['additionalProperties'] is False)
 for name,mask in [('default',None),('expanded',0.050001)]:
  dest=root/f'cli-{name}';check(name+' production CLI succeeds without display',cli(a.binary,dest,mask).returncode==0);expected=files(dest)
  check(name+' complete seven-file package and integer mask',len(expected)==7 and json.loads(expected['manifest.json'])['mask_expansion_nm']==(0 if mask is None else 50001))
  if a.previous_binary:
   old=root/f'prior-{name}';check(name+' previous production CLI succeeds',cli(a.previous_binary,old,mask).returncode==0)
   check(name+' all file bytes unchanged from previous verified production',files(old)==expected)
  check(name+' CLI collision preserves all files',cli(a.binary,dest,mask).returncode!=0 and files(dest)==expected)
  output=root/f'http-{name}';args={'path':str(output),'expected_cursor':before['cursor']}
  if mask is not None:args['mask_expansion_mm']=mask
  result=reader.call('export_fabrication',args)
  check(name+' native HTTP matches all CLI file bytes and full source cursor',files(output)==expected and result['exported_cursor']==before['cursor'] and result['manifest']==json.loads(expected['manifest.json']))
  reader.call('export_fabrication',args,expect_error=True);check(name+' HTTP NEW collision preserves all files',files(output)==expected)
  report['outputs'][name]={name:hashlib.sha256(data).hexdigest() for name,data in expected.items()}
 check('CLI active native destination rejects unchanged',cli(a.binary,project).returncode!=0 and project.read_bytes()==original)
 error=reader.call('export_fabrication',{'path':str(active)},expect_error=True)
 check('HTTP active native destination explicitly protected','active native project' in error and active.read_bytes()==active_bytes)
 for name,args in [('empty',{'path':''}),('negative',{'path':str(root/'negative'),'mask_expansion_mm':-1}),('too-large',{'path':str(root/'too-large'),'mask_expansion_mm':11}),('extra',{'path':str(root/'extra'),'overwrite':True})]:
  reader.call('export_fabrication',args,expect_error=True);check(name+' invalid request creates no directory',not(root/name).exists())
 sentinel=root/'existing-file';sentinel.write_bytes(b'preserve existing file')
 reader.call('export_fabrication',{'path':str(sentinel)},expect_error=True);check('existing file destination preserved',sentinel.read_bytes()==b'preserve existing file')
 missing=root/'missing'/'output';reader.call('export_fabrication',{'path':str(missing)},expect_error=True);check('missing parent produces no partial destination',not missing.exists() and not missing.parent.exists())
 check('all exports and failures preserve shared session/status/native',reader.call('get_session_state',{})==before and reader.call('get_document_status',{})==status and active.read_bytes()==active_bytes)
 editor.call('add_part',{'designator':'R_FAB_GUARD','value':'1k','footprint':'0603','x_mm':1,'y_mm':1})
 stale=root/'stale';error=reader.call('export_fabrication',{'path':str(stale),'mask_expansion_mm':-1,'expected_cursor':before['cursor']},expect_error=True)
 check('second agent stale guard precedes invalid mask without output','document changed' in error and not stale.exists())
 editor.call('undo',{});check('second agent undo restores project and status',reader.call('get_design',{})==before['project'] and reader.call('get_document_status',{})==status)
 check('native clients use distinct sessions',reader.session!=editor.session)
 check('no staging directories remain after success or failure',not any(p.name.startswith('.viamind-fab') for p in root.iterdir()))
 report['passed']=True
finally:
 reader.close();editor.close();(root/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
