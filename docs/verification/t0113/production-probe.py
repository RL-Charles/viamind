#!/usr/bin/env python3
"""Actual initialized branch lifecycle on an explicitly owned scratch instance."""
import argparse,sys,json,importlib.util,time
from pathlib import Path
from contextlib import ExitStack
sys.dont_write_bytecode=True
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary',type=Path,required=True);p.add_argument('--repo',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);p.add_argument('--url')
a=p.parse_args();sys.path.insert(0,str(a.repo/'scripts'))
from mcp_reference import StdioClient,digest,read_regular
root=a.directory.resolve();root.mkdir();checks=[]
def check(name,condition):
 assert condition,name
 checks.append(name)
with ExitStack() as stack:
 if a.url:
  spec=importlib.util.spec_from_file_location('smoke',a.repo/'scripts/mcp-smoke.py');smoke=importlib.util.module_from_spec(spec);spec.loader.exec_module(smoke)
  clients=[smoke.McpClient(a.url,'same branch client'),smoke.McpClient(a.url,'same branch client')]
  for client in clients:stack.callback(client.close)
  def call(name,args=None,error=False,peer=False):return clients[int(peer)].call(name,args or {},expect_error=error)
 else:
  client=stack.enter_context(StdioClient([str(a.binary),'--headless'],a.repo));client.initialize()
  def call(name,args=None,error=False,peer=False):return client.call(name,args or {},expect_error=error)
 initial=call('get_session_state');call('new_project',{'name':'Owned PCB branch proof','discard_dirty':True,'expected_cursor':initial['cursor']})
 native=root/'owned.viamind.json';call('save_project',{'path':str(native),'overwrite':False});native_bytes=read_regular(native)
 before=call('get_session_state');status=call('get_document_status');drc=call('get_auto_drc_event_state')
 def invariant(name):check(name,call('get_session_state')==before and call('get_document_status')==status and read_regular(native)==native_bytes and call('get_auto_drc_event_state')==drc)
 def create(label='authored candidate'):
  s=call('get_session_state');return call('create_pcb_branch',{'expected_project':s['project']['id'],'expected_cursor':s['cursor'],'label':label})
 b=create();bid=b['branch_id'];invariant('creation preserves full live session native status and DRC journal')
 check('initial exact source and independent revision',b['revision']==0 and b['changed_objects']==0 and b['source_cursor']==before['cursor'] and not b['busy'] and not b['stale'])
 commands=[{'name':'add_part','input':{'designator':'U1','value':'authored','footprint':'authored','x_mm':1.000003,'y_mm':-2.000007}}, {'name':'add_track','input':{'start_x_mm':0,'start_y_mm':1,'end_x_mm':5,'end_y_mm':1,'width_mm':0.3,'layer':'top','net':'N'}}, {'name':'add_via','input':{'x_mm':5,'y_mm':1,'diameter_mm':0.8,'drill_mm':0.3,'net':'N'}}]
 request={'branch_id':bid,'expected_revision':0,'commands':commands}
 if a.url:
  call('apply_pcb_branch',request,error=True,peer=True);call('inspect_pcb_branch',{'branch_id':bid},error=True,peer=True)
  check('same-name peer cannot apply or inspect candidate',call('get_pcb_branch_diff',{'branch_id':bid},peer=True)['total']==0)
 applied=call('apply_pcb_branch',request);b=applied['branch'];invariant('private component track and via preserve complete live state')
 diff=call('get_pcb_branch_diff',{'branch_id':bid,'limit':1},peer=True)
 check('deterministic bounded pagination',diff['total']==3 and diff['next_offset']==1 and len(diff['deltas'])==1 and diff['deltas'][0]['kind']=='part')
 full=call('get_pcb_branch_diff',{'branch_id':bid,'limit':128});ids={d['kind']:d['id'] for d in full['deltas']};uid=ids['part']
 check('candidate exact odd Nm and retained generated identities',full['deltas'][0]['after']['position']=={'x':1000003,'y':-2000007} and set(applied['created'])==set(ids.values()))
 move={'name':'move_part','input':{'id':uid,'x_mm':-3.000001,'y_mm':4.000009}}
 changed=call('apply_pcb_branch',{'branch_id':bid,'expected_revision':1,'commands':[move]});b=changed['branch']
 check('private revision advances original source unchanged coalesces object',b['revision']==2 and b['changed_objects']==3 and b['source_cursor']==before['cursor'])
 snapshot=call('inspect_pcb_branch',{'branch_id':bid})['board']
 for label,req in [
 ('partial duplicate batch',{'branch_id':bid,'expected_revision':2,'commands':[{'name':'add_part','input':{'designator':'U2','value':'partial probe','footprint':'authored','x_mm':2,'y_mm':3}}]*2}),
 ('forbidden document action',{'branch_id':bid,'expected_revision':2,'commands':[{'name':'new_project','input':{}}]}),
 ('no-op',{'branch_id':bid,'expected_revision':2,'commands':[move]}),
 ('wrong revision',{'branch_id':bid,'expected_revision':1,'commands':[move]}),
 ('excess malformed commands',{'branch_id':bid,'expected_revision':2,'commands':[None]*65}),
 ('excess serialized input',{'branch_id':bid,'expected_revision':2,'commands':[{'name':'add_part','input':{'value':'x'*65536}}]}),
 ]:
  call('apply_pcb_branch',req,error=True)
  state=call('inspect_pcb_branch',{'branch_id':bid})
  check(label+' refuses atomically',state['board']==snapshot and state['branch']['revision']==2 and not state['branch']['busy'])
 invariant('every private refusal and observation preserves full session and DRC journal')
 merge={'branch_id':bid,'expected_revision':2,'expected_project':b['project_id'],'expected_cursor':b['source_cursor']}
 if a.url:
  call('merge_pcb_branch',merge,error=True,peer=True);call('discard_pcb_branch',{'branch_id':bid,'expected_revision':2},error=True,peer=True)
  check('same-name peer cannot merge or discard',call('inspect_pcb_branch',{'branch_id':bid})['board']==snapshot)
 configured=call('configure_auto_drc',{'enabled':True,'debounce_ms':0})
 for _ in range(250):
  ready=call('get_auto_drc_status')
  if ready['phase'] in ['ready','skipped','failed']:break
  time.sleep(0.02)
 check('ordinary automatic DRC enabled before merge',ready['phase']=='ready')
 committed=call('merge_pcb_branch',merge);after=call('get_session_state')
 for _ in range(250):
  merged_drc=call('get_auto_drc_status')
  if merged_drc['phase']=='ready' and merged_drc['source_cursor']==after['cursor']:break
  time.sleep(0.02)
 check('merge schedules normal automatic DRC on committed cursor',merged_drc['phase']=='ready' and merged_drc['source_cursor']==after['cursor'] and merged_drc['generation']>configured['generation'])
 check('one merge keeps all generated identities and exact candidate',after['project']['board']==snapshot and after['cursor']['sequence']==before['cursor']['sequence']+1 and set(committed['created'])==set(ids.values()))
 call('merge_pcb_branch',merge,error=True)
 call('undo');check('one own undo restores original project exactly',call('get_session_state')['project']==before['project'])
 call('redo');check('one own redo restores exact candidate and identities',call('get_session_state')['project']['board']==snapshot)
 check('merge undo and redo never implicitly save native file',read_regular(native)==native_bytes)
 stale=create('will become stale');stale=call('apply_pcb_branch',{'branch_id':stale['branch_id'],'expected_revision':0,'commands':[{'name':'move_part','input':{'id':uid,'x_mm':9,'y_mm':10}}]})['branch'];call('move_part',{'id':uid,'x_mm':7,'y_mm':8},peer=bool(a.url));peer=call('get_session_state')
 call('merge_pcb_branch',{'branch_id':stale['branch_id'],'expected_revision':1,'expected_project':stale['project_id'],'expected_cursor':stale['source_cursor']},error=True)
 check('peer edit stale merge preserves full peer session',call('get_session_state')==peer and call('get_pcb_branch_diff',{'branch_id':stale['branch_id']})['branch']['stale'])
 call('new_project',{'name':'replacement','discard_dirty':True,'expected_cursor':peer['cursor']});replacement=call('get_session_state')
 call('apply_pcb_branch',{'branch_id':stale['branch_id'],'expected_revision':1,'commands':commands},error=True)
 check('replacement refuses old worker source and preserves full session',call('get_session_state')==replacement)
 call('discard_pcb_branch',{'branch_id':stale['branch_id'],'expected_revision':1})
 slots=[create('bounded slot '+str(i)) for i in range(4)]
 s=call('get_session_state');call('create_pcb_branch',{'expected_project':s['project']['id'],'expected_cursor':s['cursor'],'label':'fifth'},error=True)
 check('four slots refuse fifth without peer eviction',all(call('get_pcb_branch_diff',{'branch_id':slot['branch_id']})['branch']['revision']==0 for slot in slots))
 for slot in slots:call('discard_pcb_branch',{'branch_id':slot['branch_id'],'expected_revision':0})
 check('discarding every slot preserves replacement session',call('get_session_state')==replacement)
report={'passed':True,'checks':checks,'binary_sha256':digest(a.binary),'transport':a.url or 'actual initialized stdio','scope':'Owned scratch document only; no lease wall-clock/cadence or enforced approval security claim','native_original_sha256':digest(native)}
(root/'verification.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
