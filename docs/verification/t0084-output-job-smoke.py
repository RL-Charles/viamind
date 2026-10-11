"""Exact production CLI/native HTTP saved-job proof. Requires a NEW evidence directory."""
import argparse, hashlib, importlib.util, json, os, subprocess
from pathlib import Path
p=argparse.ArgumentParser()
for key in ['binary','project','directory','repo']:p.add_argument('--'+key,type=Path,required=True)
p.add_argument('--endpoint',required=True);p.add_argument('--previous-binary',type=Path);a=p.parse_args()
root=a.directory.resolve();root.mkdir(parents=True,exist_ok=False)
project=a.project.resolve();original=project.read_bytes();env=os.environ.copy()
for key in ['DISPLAY','WAYLAND_DISPLAY']:env.pop(key,None)
report={'passed':False,'checks':[],'endpoint':a.endpoint,'production_binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'outputs':{}}
def check(name,value):
 assert value,name
 report['checks'].append(name)
def cli(args,binary=None):return subprocess.run([str((binary or a.binary).resolve()),*map(str,args)],env=env,capture_output=True,text=True,timeout=60)
def files(path):return {str(p.relative_to(path)):p.read_bytes() for p in path.rglob('*') if p.is_file()}
def run(job,path):return cli(['--run-output-job',job,'--open',project,'--output',path])
spec=importlib.util.spec_from_file_location('smoke',a.repo/'scripts/mcp-smoke.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
reader=m.McpClient(a.endpoint,'output-job-reader');editor=m.McpClient(a.endpoint,'output-job-editor')
try:
 before=reader.call('get_session_state',{});status=reader.call('get_document_status',{});active=Path(status['path']);active_bytes=active.read_bytes();report['native_source_cursor']=before['cursor']
 tools={t['name']:t for t in reader.rpc('tools/list')['tools']}
 check('three shared job tools discovered',all(n in tools for n in ['load_output_job','save_output_job','run_output_job']))
 for name,mask in [('default',0.0),('expanded',0.050001)]:
  job={'format':'viamind-output-job','version':1,'name':'Manufacturing λ '+name,'profile':'generic_outer_2layer','mask_expansion_mm':mask};job_path=root/(name+'-job.json')
  reader.call('save_output_job',{'path':str(job_path),'job':job});check(name+' canonical saved job roundtrip',reader.call('load_output_job',{'path':str(job_path)})['job']==job and job_path.read_bytes().endswith(b'\n'))
  prior=job_path.read_bytes();reader.call('save_output_job',{'path':str(job_path),'job':job},expect_error=True);check(name+' NEW job save collision preserves bytes',job_path.read_bytes()==prior)
  out=root/('cli-'+name);result=run(job_path,out);check(name+' production CLI succeeds without display',result.returncode==0);expected=files(out)
  check(name+' fixed eleven files and exact saved descriptor',len(expected)==11 and expected['job.json']==prior)
  fab=json.loads(expected['fabrication/manifest.json']);check(name+' integer mask unchanged',fab['mask_expansion_nm']==(0 if mask==0 else 50001))
  for kind in ['bom','placement']:
   standalone=root/(name+'-'+kind+'.csv');result=cli(['--export-csv',kind,'--open',project,'--output',standalone]);check(name+' '+kind+' standalone accepted bytes unchanged',result.returncode==0 and standalone.read_bytes()==expected['assembly/'+kind+'.csv'])
  standalone=root/('standalone-fab-'+name);args=['--export-gerber','--open',project,'--output',standalone,'--mask-expansion-mm',str(mask)];result=cli(args)
  check(name+' all standalone fabrication bytes unchanged',result.returncode==0 and {'fabrication/'+k:v for k,v in files(standalone).items()}=={k:v for k,v in expected.items() if k.startswith('fabrication/')})
  if a.previous_binary:
   prior_fab=root/('prior-fab-'+name);args[args.index('--output')+1]=prior_fab;result=cli(args,a.previous_binary)
   check(name+' prior verified fabrication bytes unchanged',result.returncode==0 and files(prior_fab)==files(standalone))
  http=root/('http-'+name);value=reader.call('run_output_job',{'job_path':str(job_path),'path':str(http),'expected_cursor':before['cursor']});actual=files(http)
  check(name+' native HTTP ten format/config files identical to production CLI', {k:v for k,v in actual.items() if k!='release-manifest.json'}=={k:v for k,v in expected.items() if k!='release-manifest.json'})
  manifest=json.loads(actual['release-manifest.json']);cli_manifest=json.loads(expected['release-manifest.json']);cli_manifest['exported_cursor']=before['cursor']
  check(name+' manifest parity with actual native source cursor',manifest==cli_manifest and value['manifest']==manifest and value['exported_cursor']==before['cursor'] and value['file_count']==11)
  check(name+' manifest file lengths and both family exclusions',all(len(actual[k])==v for k,v in manifest['files'].items()) and any('component origins' in e for e in manifest['exclusions']) and any('sourcing' in e for e in manifest['exclusions']))
  reader.call('run_output_job',{'job_path':str(job_path),'path':str(http)},expect_error=True);check(name+' HTTP NEW root collision preserves all files',files(http)==actual)
  check(name+' CLI NEW root collision preserves all files',run(job_path,out).returncode!=0 and files(out)==expected)
  report['outputs'][name]={k:hashlib.sha256(v).hexdigest() for k,v in actual.items()}
 job_path=root/'default-job.json'
 reader.call('save_output_job',{'path':str(active),'job':job},expect_error=True);reader.call('run_output_job',{'job_path':str(job_path),'path':str(active)},expect_error=True)
 check('save/run active native preserved',active.read_bytes()==active_bytes and run(job_path,project).returncode!=0 and project.read_bytes()==original)
 for name,data in [('duplicate',b'{"format":"viamind-output-job","version":1,"name":"a","name":"b","profile":"generic_outer_2layer"}'),('unknown',b'{"format":"viamind-output-job","version":1,"name":"a","profile":"generic_outer_2layer","script":"bad"}'),('oversize',b' '*65537),('deep',b'['*200+b']'*200),('wrong-version',b'{"format":"viamind-output-job","version":2,"name":"a","profile":"generic_outer_2layer"}')]:
  bad=root/(name+'.json');bad.write_bytes(data);output=root/(name+'-output');reader.call('load_output_job',{'path':str(bad)},expect_error=True);reader.call('run_output_job',{'job_path':str(bad),'path':str(output)},expect_error=True);check(name+' job rejected without release',not output.exists())
 sentinel=root/'sentinel';sentinel.write_bytes(b'preserve');reader.call('run_output_job',{'job_path':str(job_path),'path':str(sentinel)},expect_error=True);check('existing file root preserved',sentinel.read_bytes()==b'preserve')
 alias=root/'native-alias';alias.symlink_to(active);reader.call('run_output_job',{'job_path':str(job_path),'path':str(alias)},expect_error=True);check('existing symlink root preserved',alias.is_symlink() and active.read_bytes()==active_bytes)
 missing=root/'missing'/'release';reader.call('run_output_job',{'job_path':str(job_path),'path':str(missing)},expect_error=True);check('missing parent creates no partial root',not missing.exists() and not missing.parent.exists())
 link=root/'job-link';link.symlink_to(job_path);reader.call('load_output_job',{'path':str(link)},expect_error=True);check('job symlink refused',link.is_symlink())
 fifo=root/'job-fifo';os.mkfifo(fifo);reader.call('load_output_job',{'path':str(fifo)},expect_error=True);check('FIFO job rejected without blocking',fifo.exists())
 check('all exports/failures preserve full session/status/native',reader.call('get_session_state',{})==before and reader.call('get_document_status',{})==status and active.read_bytes()==active_bytes)
 editor.call('add_via',{'x_mm':1,'y_mm':2,'diameter_mm':0.6,'drill_mm':0.3});stale=root/'stale';error=reader.call('run_output_job',{'job_path':str(job_path),'path':str(stale),'expected_cursor':before['cursor']},expect_error=True)
 check('two-agent full cursor guard rejects without output','document changed' in error and not stale.exists());editor.call('undo',{});check('undo restores project/status',reader.call('get_design',{})==before['project'] and reader.call('get_document_status',{})==status)
 check('native clients have distinct sessions',reader.session!=editor.session);check('ordinary failures clean staging',not any(p.name.startswith(('.viamind-output-job-','.viamind-job-')) for p in root.iterdir()))
 report['passed']=True
finally:
 reader.close();editor.close();(root/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
