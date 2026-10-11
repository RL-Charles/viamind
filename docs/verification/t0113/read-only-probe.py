#!/usr/bin/env python3
"""Actual initialized startup read-only admission for PCB branches."""
import argparse,json,sys
from pathlib import Path
sys.dont_write_bytecode=True
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--repo',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
sys.path.insert(0,str(a.repo/'scripts'))
from mcp_reference import StdioClient,digest
checks=[]
with StdioClient([str(a.binary),'--headless','--agent-read-only'],a.repo) as c:
 c.initialize();names=set();params={};seen=set()
 for _ in range(64):
  page=c.rpc('tools/list',params)
  for t in page['tools']:
   assert t['name'] not in names;names.add(t['name'])
  if 'nextCursor' not in page:break
  cursor=page['nextCursor'];assert cursor not in seen;seen.add(cursor);params={'cursor':cursor}
 else:raise AssertionError('discovery page bound')
 assert {'list_pcb_branches','get_pcb_branch_diff'}<=names;checks.append('two observation tools advertised')
 before=c.call('get_session_state');status=c.call('get_document_status');drc=c.call('get_auto_drc_event_state')
 assert c.call('list_pcb_branches')==[];checks.append('actual initialized empty read-only list')
 for name in ['create_pcb_branch','apply_pcb_branch','inspect_pcb_branch','merge_pcb_branch','discard_pcb_branch']:
  assert name not in names
  try:c.rpc('tools/call',{'name':name,'arguments':{}})
  except ValueError as e:assert str(e)=='MCP protocol error during tools/call'
  else:raise AssertionError('hidden handler admitted '+name)
  checks.append(name+' undiscoverable and protocol-refused')
 assert c.call('get_session_state')==before and c.call('get_document_status')==status and c.call('get_auto_drc_event_state')==drc;checks.append('all observations and refusals preserve complete session status and DRC journal')
 c.finish()
report={'passed':True,'checks':checks,'binary_sha256':digest(a.binary),'transport':'actual initialized CLI headless read-only stdio'}
with a.output.open('x') as f:json.dump(report,f,indent=2);f.write('\n')
print(json.dumps(report,indent=2))
