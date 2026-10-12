"""Actual production callback, independent of generic job-matrix fixture callbacks."""
import argparse,copy,hashlib,json,os,sys
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--repo',type=Path,required=True);p.add_argument('--native',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);p.add_argument('--source',required=True);a=p.parse_args();a.directory.mkdir()
sys.dont_write_bytecode=True;sys.path.insert(0,str(a.repo/'scripts'));from mcp_reference import StdioClient
r={'passed':False,'source':a.source,'binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'checks':[]}
try:
 with StdioClient([str(a.binary),'--headless','--open',str(a.native)],a.directory,timeout=60)as c:
  c.initialize()
  def call(name,args=None,error=False):return c.call(name,args or {},expect_error=error)
  def state():return call('get_session_state')
  def check(label,condition):assert condition,label;r['checks'].append(label)
  before=state();part_id=next(k for k,v in before['project']['board']['parts'].items()if v.get('holes'))
  checks={k:0 for k in ['copper_clearance_mm','edge_clearance_mm','minimum_via_drill_mm','minimum_annular_width_mm','hole_clearance_mm','hole_to_copper_clearance_mm']};checks['max_violations']=1000
  job={'format':'viamind-output-job','version':6,'name':'Production checked NPTH','profile':'checked_outer_2layer','mask_expansion_mm':0,'include_npth':True,'checks':checks};jobpath=a.directory/'checked.job.json';call('save_output_job',{'path':str(jobpath),'job':job})
  s,st=state(),call('get_document_status');refused=a.directory/'overlap-refused';response=call('run_output_job',{'job_path':str(jobpath),'path':str(refused),'expected_cursor':s['cursor']},True)
  check('actual checked callback refuses overlapping NPTH before publication',not refused.exists()and state()==s and call('get_document_status')==st and 'violations' in json.dumps(response))
  holes=copy.deepcopy(s['project']['board']['parts'][part_id]['holes']);holes[0]['center']={'x':0,'y':8000000}
  call('set_part_holes',{'expected_project':s['project']['id'],'expected_cursor':s['cursor'],'expected':s['project']['board']['parts'][part_id],'holes':holes})
  s,st=state(),call('get_document_status');release=a.directory/'checked-release';call('run_output_job',{'job_path':str(jobpath),'path':str(release),'expected_cursor':s['cursor']})
  validation=json.loads((release/'validation/drc.json').read_bytes());observed=call('inspect_output_release',{'path':str(release)})['observed_nonplated_holes'];expected=len(s['project']['board']['vias'])+sum(len(v.get('holes',[]))for v in s['project']['board']['parts'].values())
  check('actual checked job succeeds with complete plated and NPTH coverage',validation['status']=='passed_for_scoped_checks'and validation['exported_geometry']['holes']==expected and validation['report']['coverage']['checked_holes']==expected and validation['report']['violations']==[])
  check('independent saved reader observes actual checked NPTH and plated output',observed['physical_drills']==1 and observed['plated_via_drills']==len(s['project']['board']['vias']))
  check('checked export and independent inspection preserve full session/status',state()==s and call('get_document_status')==st)
  call('undo');check('one creator undo restores exact initial whole project',state()['project']==before['project']);c.finish()
  r.update(passed=True,check_count=len(r['checks']),validation=validation,observed=observed)
finally:
 with(a.directory/'checked-production.json').open('x')as f:json.dump(r,f,indent=2);f.flush();os.fsync(f.fileno())
print(json.dumps({'passed':r['passed'],'checks':len(r['checks'])}))
