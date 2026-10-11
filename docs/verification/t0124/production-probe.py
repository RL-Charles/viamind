#!/usr/bin/env python3
"""Owned two-client schematic command parity; no OS input or local draft claim."""
import argparse,copy,hashlib,importlib.util,json,os,time
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__)
for name in ('repo','binary','fixture','report'):p.add_argument('--'+name,type=Path,required=True)
p.add_argument('--endpoint',required=True);p.add_argument('--source',required=True);a=p.parse_args()
spec=importlib.util.spec_from_file_location('smoke',a.repo/'scripts/mcp-smoke.py');smoke=importlib.util.module_from_spec(spec);spec.loader.exec_module(smoke)
raw=a.fixture.read_bytes();report={'passed':False,'source':a.source,'endpoint':a.endpoint,'binary':str(a.binary),'binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'checks':[],'scope':'Real desktop HTTP command parity and snapshot preservation; local draft/caret checks are actual egui tests, not MCP or OS input'};clients=[]
def check(name,ok):
 assert ok,name
 report['checks'].append(name)
try:
 clients=[smoke.McpClient(a.endpoint,'T124 creator'),smoke.McpClient(a.endpoint,'T124 peer')]
 def call(name,args=None,peer=False,error=False):return clients[int(peer)].call(name,args or {},expect_error=error)
 def state():return call('get_session_state')
 def status():return call('get_document_status')
 def reject(name,args):
  before,st=state(),status();error=call(name,args,error=True);check(name+' refuses atomically',bool(error) and state()==before and status()==st)
 tools=[];cursor=None
 while True:
  page=clients[0].rpc('tools/list',{} if cursor is None else {'cursor':cursor});tools.extend(page['tools']);cursor=page.get('nextCursor')
  if cursor is None:break
  assert len(tools)<512
 names={t['name']for t in tools}
 check('discover exact current schematic command and observation tools',{'edit_schematic_sheet','query_schematic','get_session_state','get_document_status','undo','redo'}<=names and len(names)==len(tools))
 report['tool_count']=len(tools)
 initial=state();check('verify isolated owned fixture before edits',initial['project']['name']=='T124 compact sidebar owned proof')
 check('two clients agree on complete initial session',call('get_session_state',peer=True)==initial)
 st=status();call('query_schematic',peer=True);call('get_design',peer=True);check('read-only observations preserve full session and document status',state()==initial and status()==st)
 project=initial['project'];sheet_id=next(iter(project['schematic']['sheets']));sheet=project['schematic']['sheets'][sheet_id];symbol_id=next(iter(sheet['content']['symbols']));symbol=copy.deepcopy(sheet['content']['symbols'][symbol_id])
 symbol.pop('pins',None)
 old=sheet['content']['symbols'][symbol_id];symbol['pins']=[{k:v for k,v in pin.items()if k!='id'}for pin in old['pins']]
 symbol['position']={'x':12000003,'y':-7}
 request={'expected_project':project['id'],'sheet_id':sheet_id,'expected':sheet,'name':sheet['name'],'edits':[{'kind':'symbol','symbol':symbol}]}
 start=time.perf_counter();result=call('edit_schematic_sheet',request);report['commit_latency_ms']=(time.perf_counter()-start)*1000
 changed=state();expected=copy.deepcopy(project);expected['schematic']['sheets'][sheet_id]['content']['symbols'][symbol_id]['position']=symbol['position']
 check('exact Nm schematic edit changes only selected symbol position',changed['project']==expected)
 check('one sheet delta and one event',len(result['deltas'])==1 and changed['cursor']['sequence']==initial['cursor']['sequence']+1)
 check('pin UUIDs, electrical fields and board all preserved',changed['project']['schematic']['sheets'][sheet_id]['content']['symbols'][symbol_id]['pins']==old['pins'] and changed['project']['board']==project['board'])
 check('peer immediately sees full exact changed session',call('get_session_state',peer=True)==changed)
 reject('edit_schematic_sheet',request)
 bad=copy.deepcopy(request);bad['expected']=changed['project']['schematic']['sheets'][sheet_id];bad['edits'][0]['symbol']['width']=0;reject('edit_schematic_sheet',bad)
 wrong=copy.deepcopy(bad);wrong['expected_project']='00000000-0000-0000-0000-000000000001';reject('edit_schematic_sheet',wrong)
 call('undo');check('one creator undo restores exact project',state()['project']==project)
 call('redo');check('one creator redo restores exact changed project',state()['project']==expected)
 current=state();noop=copy.deepcopy(request);noop['expected']=current['project']['schematic']['sheets'][sheet_id];call('edit_schematic_sheet',noop);check('exact snapshot no-op emits no event',state()==current)
 peer_result=call('add_part',{'designator':'R_T124_PEER','value':'keep','footprint':'legacy','x_mm':2,'y_mm':3},peer=True);peer_id=peer_result['created'][0]
 call('undo');after=state()['project'];check('creator selective undo preserves unrelated peer board edit',after['schematic']==project['schematic'] and peer_id in after['board']['parts'])
 call('redo');check('creator redo preserves peer board edit and exact schematic',state()['project']['schematic']==expected['schematic'] and peer_id in state()['project']['board']['parts'])
 call('undo',peer=True);check('peer can undo only its board edit',state()['project']==expected)
 call('undo');check('final creator undo restores exact original project',state()['project']==project)
 final=state();st=status();call('query_schematic',peer=True);check('final shared read-only snapshot and status preserved',call('get_session_state',peer=True)==final and state()==final and status()==st)
 check('original scratch native bytes never overwritten',a.fixture.read_bytes()==raw)
 report.update(passed=True,check_count=len(report['checks']),final_state=final,final_status=st)
finally:
 for client in clients:client.close()
 with a.report.open('x')as f:json.dump(report,f,indent=2);f.flush();os.fsync(f.fileno())
print(json.dumps({'passed':report['passed'],'checks':len(report['checks']),'tools':report.get('tool_count'),'report':str(a.report)}))
