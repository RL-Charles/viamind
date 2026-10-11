"""Exact production CLI + two independent native HTTP sourcing agents."""
import argparse,csv,hashlib,importlib.util,json,os,subprocess
from pathlib import Path
p=argparse.ArgumentParser()
for key in ['binary','previous-binary','project','baseline-project','directory','repo']:p.add_argument('--'+key,type=Path,required=True)
p.add_argument('--endpoint',required=True);a=p.parse_args();root=a.directory.resolve();root.mkdir(parents=True,exist_ok=False)
env=os.environ.copy()
for key in ['DISPLAY','WAYLAND_DISPLAY']:env.pop(key,None)
report={'passed':False,'checks':[],'endpoint':a.endpoint,'production_binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'outputs':{}}
def check(name,value):
 assert value,name
 report['checks'].append(name)
def cli(args,binary=None):return subprocess.run([str((binary or a.binary).resolve()),*map(str,args)],env=env,capture_output=True,text=True,timeout=60)
def files(path):return {str(p.relative_to(path)):p.read_bytes() for p in path.rglob('*') if p.is_file()}
def parsed(path):
 with path.open(newline='',encoding='utf-8') as f:return list(csv.DictReader(f))
spec=importlib.util.spec_from_file_location('smoke',a.repo/'scripts/mcp-smoke.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
reader=m.McpClient(a.endpoint,'sourcing-reader');editor=m.McpClient(a.endpoint,'sourcing-editor')
try:
 source=a.project.resolve();source_bytes=source.read_bytes();baseline=a.baseline_project.resolve()
 state=reader.call('get_session_state',{});design=state['project'];ids=sorted(design['board']['parts']);reader.call('set_selection',{'ids':[ids[0]]});state=reader.call('get_session_state',{});status=reader.call('get_document_status',{})
 report['initial_cursor']=state['cursor'];report['active_native_path']=status['path']
 tools={t['name']:t for t in reader.rpc('tools/list')['tools']}
 check('typed sourcing setter and query/export tools discovered',all(n in tools for n in ['set_part_assembly','query_parts','export_assembly_csv']))
 check('distinct agent sessions',reader.session!=editor.session)
 page=reader.call('query_parts',{'limit':1000})['parts'];check('query returns exact explicit metadata',all(p['assembly']==design['board']['parts'][p['id']]['assembly'] for p in page))
 for kind in ['bom','placement','sourcing_bom']:
  path=root/(kind+'.csv');result=cli(['--export-csv',kind,'--open',source,'--output',path]);check(kind+' actual production no-display CLI succeeds',result.returncode==0)
  native=root/('http-'+kind+'.csv');value=reader.call('export_assembly_csv',{'kind':kind,'path':str(native),'expected_cursor':state['cursor']})
  check(kind+' native HTTP bytes equal production CLI',native.read_bytes()==path.read_bytes() and value['exported_cursor']==state['cursor'])
  prior=path.read_bytes();reader.call('export_assembly_csv',{'kind':kind,'path':str(path)},expect_error=True);check(kind+' NEW collision preserves bytes',path.read_bytes()==prior)
  path.write_bytes(b'replace only explicitly');reader.call('export_assembly_csv',{'kind':kind,'path':str(path),'overwrite':True});check(kind+' explicit overwrite preserves exact format',path.read_bytes()==prior)
  report['outputs'][kind]=hashlib.sha256(prior).hexdigest()
  if kind!='sourcing_bom':
   old=root/('prior-'+kind+'.csv');result=cli(['--export-csv',kind,'--open',baseline,'--output',old],a.previous_binary);check(kind+' previous verified bytes unchanged despite sourcing/DNP',result.returncode==0 and old.read_bytes()==prior)
 rows=parsed(root/'sourcing_bom.csv');check('independent CSV parser sees exact Unicode punctuation and identifiers',all(r['Manufacturer']=='User μ, "Example"' and r['MPN']=='USER-ENTERED-MPN' for r in rows))
 check('population groups are explicit and neither component is omitted',{r['Population'] for r in rows}=={'Populate','Do not populate'} and sum(int(r['Quantity']) for r in rows)==len(ids))
 check('old BOM remains all-component inventory',sum(int(r['Quantity']) for r in parsed(root/'bom.csv'))==len(ids))
 fab=root/'fab';prior=root/'prior-fab';r=cli(['--export-gerber','--open',source,'--output',fab]);s=cli(['--export-gerber','--open',baseline,'--output',prior],a.previous_binary);check('seven fabrication bytes exclude sourcing/population',r.returncode==s.returncode==0 and files(fab)==files(prior))
 svg=root/'board.svg';old_svg=root/'prior.svg';r=cli(['--export-svg','--open',source,'--output',svg]);s=cli(['--export-svg','--open',baseline,'--output',old_svg],a.previous_binary);check('board SVG bytes exclude sourcing/population',r.returncode==s.returncode==0 and svg.read_bytes()==old_svg.read_bytes())
 job={'format':'viamind-output-job','version':1,'name':'Sourcing exclusion proof','profile':'generic_outer_2layer','mask_expansion_mm':0};job_path=root/'job.json';reader.call('save_output_job',{'path':str(job_path),'job':job});current=root/'job-current';old=root/'job-prior';r=cli(['--run-output-job',job_path,'--open',source,'--output',current]);s=cli(['--run-output-job',job_path,'--open',baseline,'--output',old],a.previous_binary);new_files=files(current);old_files=files(old);check('current saved-job profile ten output/config bytes unchanged',r.returncode==s.returncode==0 and len(new_files)==11 and {k:v for k,v in new_files.items() if k!='release-manifest.json'}=={k:v for k,v in old_files.items() if k!='release-manifest.json'})
 original=design['board']['parts'][ids[0]]
 for metadata in [{'mpn':''},{'mpn':' x'},{'mpn':'x\n'},{'mpn':'é'*257},{'unexpected':'x'}]:reader.call('set_part_assembly',{'id':ids[0],'assembly':metadata},expect_error=True)
 reader.call('set_part_assembly',{'id':ids[0]},expect_error=True)
 reader.call('set_part_assembly',{'id':ids[0],'assembly':None,'expected_project':'00000000-0000-0000-0000-000000000000'},expect_error=True)
 check('invalid fields/missing required clear/project guard preserve full session',reader.call('get_session_state',{})==state)
 reader.call('export_assembly_csv',{'kind':'sourcing_bom','path':status['path'],'overwrite':True},expect_error=True);check('explicit overwrite always protects active native',source.read_bytes()==source_bytes)
 metadata=dict(original['assembly']);metadata['mpn']='USER-EDITED-MPN'
 editor.call('set_part_assembly',{'id':ids[0],'assembly':metadata,'expected':original,'expected_project':design['id']});after=reader.call('get_session_state',{})
 check('second agent observes exact metadata and one event',after['project']['board']['parts'][ids[0]]['assembly']==metadata and after['cursor']['sequence']==state['cursor']['sequence']+1)
 reader.call('set_part_assembly',{'id':ids[0],'assembly':None,'expected':original,'expected_project':design['id']},expect_error=True)
 stale=root/'stale.csv';reader.call('export_assembly_csv',{'kind':'sourcing_bom','path':str(stale),'expected_cursor':state['cursor']},expect_error=True)
 check('stale full Part and full cursor reject atomically without output',reader.call('get_session_state',{})==after and not stale.exists())
 editor.call('undo',{});check('undo restores exact initial project and clean native status',reader.call('get_design',{})==design and reader.call('get_document_status',{})==status)
 editor.call('redo',{});check('redo restores edited metadata',reader.call('get_design',{})==after['project']);editor.call('undo',{})
 saved=root/'saved-schema7.json';reader.call('save_project',{'path':str(saved),'overwrite':False});saved_data=json.loads(saved.read_text());check('native save uses schema7 and exact metadata',saved_data['version']==7 and saved_data['project']==design)
 reader.call('open_project',{'path':str(source),'discard_dirty':False});reader.call('open_project',{'path':str(saved),'discard_dirty':False});check('native reopen restores metadata exactly',reader.call('get_design',{})==design)
 reader.call('export_assembly_csv',{'kind':'sourcing_bom','path':str(saved),'overwrite':True},expect_error=True);check('reopened active native protected',json.loads(saved.read_text())==saved_data)
 for id in ids:
  original=reader.call('get_design',{})['board']['parts'][id];reader.call('set_part_assembly',{'id':id,'assembly':None,'expected':original,'expected_project':design['id']})
 cleared=root/'cleared-schema4.json';reader.call('save_project',{'path':str(cleared),'overwrite':False});cleared_data=json.loads(cleared.read_text());check('clearing last metadata falls back to schema4 without assembly keys',cleared_data['version']==4 and all('assembly' not in p for p in cleared_data['project']['board']['parts'].values()))
 reader.call('open_project',{'path':str(source),'discard_dirty':False});reader.call('set_selection',{'ids':[ids[0]]});check('original active project and source bytes restored',reader.call('get_design',{})==design and source.read_bytes()==source_bytes)
 check('ordinary exports leave no staged CSV/output-job artifacts',not any(p.name.startswith(('.viamind-output-job-','.viamind-csv-')) for p in root.iterdir()))
 report['final_cursor']=reader.call('get_session_state',{})['cursor'];report['passed']=True
finally:
 reader.close();editor.close();(root/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
