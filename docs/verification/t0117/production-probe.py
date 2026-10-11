#!/usr/bin/env python3
"""Initialize the actual application in each startup mode on owned scratch input."""
import argparse,json,sys,hashlib
from pathlib import Path
sys.dont_write_bytecode=True
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--repo',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);a=p.parse_args()
sys.path.insert(0,str(a.repo/'scripts'))
from mcp_reference import StdioClient,digest
root=a.directory.resolve();root.mkdir();checks=[]
def check(name,condition):
 assert condition,name
 checks.append(name)
def discover(c):
 names={};params={};seen=set()
 for _ in range(64):
  page=c.rpc('tools/list',params)
  for tool in page['tools']:
   assert tool['name'] not in names;names[tool['name']]=tool
  if 'nextCursor' not in page:return names
  cursor=page['nextCursor'];assert cursor not in seen;seen.add(cursor);params={'cursor':cursor}
 raise AssertionError('too many tool pages')
native=root/'protected.viamind.json';original=(a.repo/'fixtures/views/asymmetric.viamind.json').read_bytes();native.write_bytes(original)
flags=[('live',[]),('read_only',['--agent-read-only']),('approval',['--agent-approval'])]
sets={}
for mode,extra in flags:
 with StdioClient([str(a.binary),'--headless',*extra,'--open',str(native)],a.repo) as c:
  c.initialize();tools=discover(c);sets[mode]=set(tools)
  status=c.call('get_agent_mode');check(mode+' actual startup mode and immutable ceiling',status['mode']==mode and status['startup_locked']==(mode=='read_only'))
  before=c.call('get_session_state');doc=c.call('get_document_status');drc=c.call('get_auto_drc_event_state')
  def invariant(label):check(label,c.call('get_session_state')==before and c.call('get_document_status')==doc and c.call('get_auto_drc_event_state')==drc and native.read_bytes()==original)
  if mode=='live':
   c.call('add_part',{'designator':'LIVE_PROOF','value':'actual process','footprint':'authored','x_mm':3,'y_mm':4})
   after=c.call('get_session_state');check('default live actual agent edit remains one transaction',after['cursor']['sequence']==before['cursor']['sequence']+1 and native.read_bytes()==original)
  else:
   for name in sorted(sets['live']-set(tools)):
    try:c.rpc('tools/call',{'name':name,'arguments':{'mode':'live','human':True,'readOnlyHint':True,'path':str(root/'must-not-exist')}})
    except ValueError as e:assert str(e)=='MCP protocol error during tools/call'
    else:raise AssertionError('forbidden production handler admitted '+name)
    invariant(mode+' '+name+' hidden, forged request refused before live/native mutation')
   for name in ['render_board_view','render_board_png','render_schematic_view','render_schematic_png']:
    check(mode+' '+name+' schema excludes publication', 'path' not in tools[name]['inputSchema']['properties'])
    for path in [str(root/'must-not-exist'),None,42]:c.call(name,{'path':path},expect_error=True)
    invariant(mode+' '+name+' rejects every path presence before handler')
   check(mode+' forbidden publication destination absent',not (root/'must-not-exist').exists())
  if mode=='approval':
   private={'create_pcb_branch','apply_pcb_branch','inspect_pcb_branch','discard_pcb_branch'}
   check('approval exact additional private-candidate inventory',set(tools)==sets['read_only']|private)
   branch=c.call('create_pcb_branch',{'expected_project':before['project']['id'],'expected_cursor':before['cursor'],'label':'actual CLI proposal'})
   bid=branch['branch_id'];invariant('actual candidate creation preserves complete state')
   applied=c.call('apply_pcb_branch',{'branch_id':bid,'expected_revision':0,'commands':[{'name':'add_part','input':{'designator':'PROPOSAL','value':'private','footprint':'authored','x_mm':3.000003,'y_mm':4.000007}}]})
   diff=c.call('get_pcb_branch_diff',{'branch_id':bid});check('actual private odd-Nm geometry and generated ID',diff['total']==1 and diff['deltas'][0]['after']['position']=={'x':3000003,'y':4000007} and applied['created']==[diff['deltas'][0]['id']])
   invariant('actual private apply inspect diff preserve complete state')
   c.call('discard_pcb_branch',{'branch_id':bid,'expected_revision':1});check('actual creator rejection removes candidate',c.call('list_pcb_branches')==[]);invariant('actual rejection preserves complete state')
  c.finish()
report={'passed':True,'checks':checks,'binary_sha256':digest(a.binary),'transport':'three actual initialized CLI processes; headless stdio','tool_counts':{k:len(v) for k,v in sets.items()},'native_sha256':hashlib.sha256(original).hexdigest(),'headless_acceptance':'unavailable; human desktop acceptance covered by original UI controller and actual HTTP server tests'}
with (root/'verification.json').open('x') as f:json.dump(report,f,indent=2);f.write('\n')
print(json.dumps({'passed':True,'checks':len(checks),'tool_counts':report['tool_counts']}))
