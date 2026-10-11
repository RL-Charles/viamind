"""Actual no-display read-only CLI rejects setters before mutation."""
import hashlib,json,sys
from pathlib import Path
repo=Path(sys.argv[1]);binary=Path(sys.argv[2]);fixture=Path(sys.argv[3]);output=Path(sys.argv[4]);sys.path.insert(0,str(repo/'scripts'))
from mcp_reference import StdioClient
checks=[]
def check(name,ok=True):
 assert ok,name
 checks.append(name)
with StdioClient([str(binary),'--headless','--agent-read-only','--open',str(fixture)],repo)as c:
 c.initialize();tools=c.rpc('tools/list')['tools'];names=[t['name']for t in tools]
 check('actual read-only tool list includes immutable property query','query_copper_properties'in names)
 check('both setters are absent from actual read-only tool list',all(n not in names for n in ['set_track_properties','set_via_properties']))
 old=c.call('get_session_state');status=c.call('get_document_status')
 for kind,key in [('track','tracks'),('via','vias')]:
  id=next(iter(old['project']['board'][key]));v=c.call('query_copper_properties',{'target':{'kind':kind,'id':id}})
  check(kind+' read-only original and full cursor are exact',v['object'][kind]==old['project']['board'][key][id]and v['source_cursor']==old['cursor'])
  try:c.call('set_'+kind+'_properties',{})
  except ValueError as error:assert 'MCP protocol error during tools/call'in str(error)
  else:raise AssertionError('hidden setter must refuse')
  check(kind+' hidden mutation handler refuses unchanged',c.call('get_session_state')==old and c.call('get_document_status')==status)
 check('native input bytes remain unchanged',json.loads(fixture.read_text())['project']==old['project']);c.finish()
report={'passed':True,'checks':checks,'assertion_count':len(checks),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'fixture':str(fixture),'scope':'actual no-display headless readonly process; no native window/OS input'}
with output.open('x')as f:f.write(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
