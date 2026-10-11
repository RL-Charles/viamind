"""Reproduce bounded manufacturing CLI/native HTTP output and atomic failure checks."""
import argparse,csv,hashlib,importlib.util,json,os,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--baseline',type=Path,required=True);p.add_argument('--project',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);p.add_argument('--endpoint');p.add_argument('--repo',type=Path,default=Path.cwd());config=p.parse_args()
root=config.directory.resolve();root.mkdir(parents=True,exist_ok=False)
binary=config.binary.resolve();baseline=config.baseline.resolve();project=config.project.resolve();env=os.environ.copy();env.pop('DISPLAY',None);env.pop('WAYLAND_DISPLAY',None)
report={'passed':False,'checks':[],'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'baseline_sha256':hashlib.sha256(baseline.read_bytes()).hexdigest(),'outputs':{}}
def check(name,condition):
 assert condition,name
 report['checks'].append(name)
def run(program,args):return subprocess.run([str(program),*map(str,args)],capture_output=True,text=True,env=env,timeout=30)
def contents(path):
 if path.is_dir():return {p.name:p.read_bytes() for p in path.iterdir()}
 return {path.name:path.read_bytes()}
original=project.read_bytes();expected={}
for kind in ['bom','placement','fabrication']:
 flag=['--export-gerber','--mask-expansion-mm','0.050001'] if kind=='fabrication' else ['--export-csv',kind]
 old=root/('old-'+kind);new=root/('new-'+kind)
 check(kind+' baseline CLI succeeds',run(baseline,[*flag,'--open',project,'--output',old]).returncode==0)
 check(kind+' bounded CLI succeeds',run(binary,[*flag,'--open',project,'--output',new]).returncode==0)
 a=list(contents(old).values()) if kind!='fabrication' else contents(old)
 b=list(contents(new).values()) if kind!='fabrication' else contents(new)
 check(kind+' exact prior format bytes preserved',a==b);expected[kind]=b
 before=contents(new);check(kind+' repeated collision preserves bytes',run(binary,[*flag,'--open',project,'--output',new]).returncode!=0 and contents(new)==before)
 report['outputs'][kind]={name:hashlib.sha256(data).hexdigest() for name,data in contents(new).items()}
 if kind!='fabrication':
  with new.open(newline='') as f: rows=list(csv.DictReader(f))
  check(kind+' independently parsed escaped Unicode text',bool(rows) and all('µΩ' in row['Value'] and '\r\n' in row['Value'] and '"' in row['Value'] for row in rows))
bad=json.loads(original);next(iter(bad['project']['board']['parts'].values()))['value']='x'*4097;badpath=root/'overbudget.viamind.json';badpath.write_text(json.dumps(bad));sentinel=root/'preserved.csv';sentinel.write_text('preserve')
check('overbudget CSV rejects before overwrite',run(binary,['--export-csv','bom','--open',badpath,'--output',sentinel,'--overwrite']).returncode!=0 and sentinel.read_text()=='preserve')
check('overbudget fabrication rejects without output',run(binary,['--export-gerber','--open',badpath,'--output',root/'rejected-fab']).returncode!=0 and not (root/'rejected-fab').exists())
check('production native input unchanged',project.read_bytes()==original)
if config.endpoint:
 spec=importlib.util.spec_from_file_location('mcp_smoke',config.repo.resolve()/'scripts/mcp-smoke.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
 a=m.McpClient(config.endpoint,'manufacturing-reader');b=m.McpClient(config.endpoint,'manufacturing-editor')
 try:
  before=a.call('get_session_state',{});status=a.call('get_document_status',{})
  for kind in ['bom','placement','fabrication']:
   path=root/('native-'+kind);args={'path':str(path),'expected_cursor':before['cursor']};tool='export_fabrication' if kind=='fabrication' else 'export_assembly_csv'
   args.update({'mask_expansion_mm':0.050001} if kind=='fabrication' else {'kind':kind})
   result=a.call(tool,args);check(kind+' native exact source cursor',result['exported_cursor']==before['cursor'])
   data=contents(path) if kind=='fabrication' else list(contents(path).values());check(kind+' native/production CLI bytes identical',data==expected[kind]);a.call(tool,args,expect_error=True)
  check('native read-only exports preserve full session/status',a.call('get_session_state',{})==before and a.call('get_document_status',{})==status)
  a.call('export_assembly_csv',{'kind':'bom','path':status['path'],'overwrite':True},expect_error=True);check('native active path preserved',Path(status['path']).read_bytes()==original)
  b.call('add_part',{'designator':'R_MFG_GUARD','value':'10k','footprint':'0603','x_mm':1,'y_mm':1})
  for tool,extra in [('export_assembly_csv',{'kind':'bom'}),('export_fabrication',{})]:
   target=root/(tool+'-stale');a.call(tool,{'path':str(target),'expected_cursor':before['cursor'],**extra},expect_error=True);check(tool+' other agent invalidates stale snapshot without output',not target.exists())
  b.call('undo',{});check('native editor undo restores exact project',a.call('get_design',{})==before['project']);check('distinct native sessions',a.session!=b.session)
  part_id=next(iter(before['project']['board']['parts']));b.call('update_part',{'id':part_id,'value':'x'*4097,'expected':before['project']['board']['parts'][part_id]})
  budget_state=a.call('get_session_state',{});preserved=(root/'native-bom').read_bytes()
  error=a.call('export_assembly_csv',{'kind':'bom','path':str(root/'native-bom'),'overwrite':True},expect_error=True);check('native overbudget CSV preserves existing output','4096' in error and (root/'native-bom').read_bytes()==preserved)
  rejected=root/'native-overbudget-fab';error=a.call('export_fabrication',{'path':str(rejected)},expect_error=True);check('native overbudget fabrication leaves no output','4096' in error and not rejected.exists())
  check('native budget failures preserve edited session',a.call('get_session_state',{})==budget_state)
  b.call('undo',{});check('native budget-edit undo restores project/status',a.call('get_design',{})==before['project'] and a.call('get_document_status',{})==status)

 finally:a.close();b.close()
report['passed']=True;(root/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
