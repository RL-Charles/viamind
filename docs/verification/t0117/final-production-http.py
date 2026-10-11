#!/usr/bin/env python3
"""Two clients on a fresh explicitly owned final-stack Approval desktop instance."""
import argparse,json,sys,importlib.util,hashlib
from pathlib import Path
from contextlib import ExitStack
sys.dont_write_bytecode=True
p=argparse.ArgumentParser();p.add_argument('--repo',type=Path,required=True);p.add_argument('--binary',type=Path,required=True);p.add_argument('--url',required=True);p.add_argument('--directory',type=Path,required=True);p.add_argument('--expected-native',type=Path,required=True);p.add_argument('--pid',type=int,required=True);a=p.parse_args()
assert a.pid>1
expected_exe=a.binary.resolve(strict=True)
live_exe=Path(f'/proc/{a.pid}/exe').resolve(strict=True)
assert live_exe==expected_exe,(live_exe,expected_exe)
binary_sha=hashlib.sha256(expected_exe.read_bytes()).hexdigest()
assert hashlib.sha256(Path(f'/proc/{a.pid}/exe').read_bytes()).hexdigest()==binary_sha
spec=importlib.util.spec_from_file_location('smoke',a.repo/'scripts/mcp-smoke.py');smoke=importlib.util.module_from_spec(spec);spec.loader.exec_module(smoke)
root=a.directory;root.mkdir();checks=[]
def check(label,condition):assert condition,label;checks.append(label)
with ExitStack() as stack:
 clients=[smoke.McpClient(a.url,'same approval client')for _ in range(2)]
 for c in clients:stack.callback(c.close)
 x,y=clients
 before=x.call('get_session_state',{});status=x.call('get_document_status',{});drc=x.call('get_auto_drc_event_state',{})
 native=Path(status['path']);assert native.resolve()==a.expected_native.resolve(strict=True);original=native.read_bytes()
 def invariant(label):check(label,x.call('get_session_state',{})==before and y.call('get_session_state',{})==before and x.call('get_document_status',{})==status and x.call('get_auto_drc_event_state',{})==drc and native.read_bytes()==original)
 check('actual desktop startup Approval policy shared by same-name clients',x.call('get_agent_mode',{})['mode']==y.call('get_agent_mode',{})['mode']=='approval')
 tools={t['name']:t for t in x.rpc('tools/list',{})['tools']};check('private candidate advertised and exact merge unavailable',{'create_pcb_branch','apply_pcb_branch','inspect_pcb_branch','discard_pcb_branch'}<=set(tools)and'merge_pcb_branch'not in tools)
 for name in ['add_part','undo','redo','set_selection','save_project','new_project','open_project','configure_auto_drc','set_agent_presence','save_symbol_template','save_footprint_definition','save_output_job','run_output_job','export_assembly_csv','export_fabrication','merge_pcb_branch']:
  check(name+' hidden',name not in tools)
  try:y.rpc('tools/call',{'name':name,'arguments':{'human':True,'mode':'live','readOnlyHint':True,'path':str(root/'forbidden')}})
  except RuntimeError as error:assert 'unknown tool' in str(error),str(error)
  else:raise AssertionError(name+' admitted')
  invariant(name+' direct forged call refused and preserves complete state')
 for name in ['render_board_view','render_board_png','render_schematic_view','render_schematic_png']:
  for path in [str(root/'forbidden'),None,42]:y.call(name,{'path':path},expect_error=True)
  invariant(name+' every supplied path refuses before handler/publication')
 branch=x.call('create_pcb_branch',{'expected_project':before['project']['id'],'expected_cursor':before['cursor'],'label':'actual desktop human approval'});bid=branch['branch_id'];invariant('desktop private branch creation unchanged')
 request={'branch_id':bid,'expected_revision':0,'commands':[{'name':'add_part','input':{'designator':'APPROVAL_PROOF','value':'private','footprint':'authored','x_mm':3.000003,'y_mm':4.000007}}]}
 y.call('apply_pcb_branch',request,expect_error=True);check('same reported client name cannot impersonate creator',y.call('get_pcb_branch_diff',{'branch_id':bid})['total']==0)
 applied=x.call('apply_pcb_branch',request);diff=y.call('get_pcb_branch_diff',{'branch_id':bid});check('peer observes exact proposed odd-Nm geometry and retained IDs',diff['total']==1 and diff['deltas'][0]['after']['position']=={'x':3000003,'y':4000007} and applied['created']==[diff['deltas'][0]['id']])
 y.call('inspect_pcb_branch',{'branch_id':bid},expect_error=True);y.call('discard_pcb_branch',{'branch_id':bid,'expected_revision':1},expect_error=True);invariant('apply diff and private-owner refusals preserve complete live state')
 x.call('discard_pcb_branch',{'branch_id':bid,'expected_revision':1});check('creator can reject private candidate without live acceptance',x.call('list_pcb_branches',{})==[]);invariant('desktop rejection preserves original session history status DRC and native')
 check('forbidden destination absent',not(root/'forbidden').exists())
report={'passed':True,'checks':checks,'transport':'two actual initialized same-name desktop HTTP clients','url':a.url,'binary_sha256':binary_sha,'pid':a.pid,'live_executable':str(live_exe),'native_path':str(native.resolve()),'native_sha256':hashlib.sha256(original).hexdigest(),'scope':'Startup Approval on actual desktop process; dynamic human change/accept/undo/redo verified separately via actual HTTP server and shared desktop controls'}
(root/'verification.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'passed':True,'checks':len(checks)}))
