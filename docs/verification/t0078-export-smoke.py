"""Verify actual production CSV CLI and native HTTP exports; use a NEW evidence directory."""
import argparse,csv,hashlib,importlib.util,json,os,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--project',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);p.add_argument('--endpoint',required=True);p.add_argument('--repo',type=Path,default=Path.cwd());a=p.parse_args()
root=a.directory.resolve();root.mkdir(parents=True,exist_ok=False);binary=a.binary.resolve();project=a.project.resolve();native_bytes=project.read_bytes();env=os.environ.copy();env.pop('DISPLAY',None);env.pop('WAYLAND_DISPLAY',None)
report={'passed':False,'checks':[],'production_binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'endpoint':a.endpoint,'outputs':{}}
def check(name,result):
 assert result,name
 report['checks'].append(name)
def cli(kind,path,overwrite=False):
 return subprocess.run([str(binary),'--export-csv',kind,'--open',str(project),'--output',str(path),*(['--overwrite'] if overwrite else [])],env=env,capture_output=True,text=True,timeout=30)
spec=importlib.util.spec_from_file_location('mcp_smoke',a.repo.resolve()/'scripts/mcp-smoke.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
reader=m.McpClient(a.endpoint,'desktop-csv-reader');editor=m.McpClient(a.endpoint,'desktop-csv-editor')
try:
 before=reader.call('get_session_state',{});report['native_source_cursor']=before['cursor'];status=reader.call('get_document_status',{});active=Path(status['path']);active_bytes=active.read_bytes()
 tools={t['name']:t for t in reader.rpc('tools/list')['tools']};schema=tools['export_assembly_csv']['inputSchema']
 check('shared assembly tool fields/default/kinds advertised',schema['required']==['kind','path'] and schema['properties']['kind']['enum']==['bom','placement'] and schema['properties']['overwrite']['default'] is False and schema['additionalProperties'] is False)
 for kind in ['bom','placement']:
  path=root/f'cli-{kind}.csv';check(kind+' production CLI succeeds',cli(kind,path).returncode==0);data=path.read_bytes()
  check(kind+' CLI NEW collision preserves bytes',cli(kind,path).returncode!=0 and path.read_bytes()==data)
  path.write_bytes(b'explicit overwrite sentinel');check(kind+' CLI explicit overwrite restores exact bytes',cli(kind,path,True).returncode==0 and path.read_bytes()==data)
  with path.open(newline='') as f:rows=list(csv.DictReader(f))
  check(kind+' independent CSV preserves Unicode quotes and CRLF',bool(rows) and all(r['Value']=='10k, "precision"\r\nµΩ' for r in rows))
  check(kind+' grouping or placement convention',rows[0]['Quantity']=='2' if kind=='bom' else rows[0]['X (mm)']=='30.000000' and rows[0]['Side']=='Bottom')
  dest=root/f'http-{kind}.csv';args={'kind':kind,'path':str(dest),'expected_cursor':before['cursor']};result=reader.call('export_assembly_csv',args)
  check(kind+' native HTTP matches CLI bytes/source cursor',dest.read_bytes()==data and result['exported_cursor']==before['cursor'] and result['component_count']==2)
  reader.call('export_assembly_csv',args,expect_error=True);check(kind+' native HTTP NEW collision preserves bytes',dest.read_bytes()==data)
  dest.write_bytes(b'explicit native overwrite');reader.call('export_assembly_csv',{**args,'overwrite':True});check(kind+' native HTTP explicit overwrite matches CLI',dest.read_bytes()==data)
  reader.call('export_assembly_csv',{'kind':kind,'path':str(active),'overwrite':True},expect_error=True);check(kind+' active native protected with explicit overwrite',active.read_bytes()==active_bytes)
  report['outputs'][kind]=hashlib.sha256(data).hexdigest()
 check('exports preserve full shared session and native status',reader.call('get_session_state',{})==before and reader.call('get_document_status',{})==status and project.read_bytes()==native_bytes)
 editor.call('add_part',{'designator':'R_CSV_GUARD','value':'1k','footprint':'0603','x_mm':1,'y_mm':1})
 stale=root/'stale.csv';reader.call('export_assembly_csv',{'kind':'bom','path':str(stale),'expected_cursor':before['cursor']},expect_error=True);check('second agent invalidates stale export without output',not stale.exists())
 editor.call('undo',{});check('second agent undo restores exact project and status',reader.call('get_design',{})==before['project'] and reader.call('get_document_status',{})==status)
 check('native clients have distinct sessions',reader.session!=editor.session)
 report['passed']=True
finally:
 reader.close();editor.close();(root/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
