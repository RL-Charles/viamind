"""Actual read-only headless process observes package previews but refuses mutation."""
import hashlib,json,sys
from pathlib import Path
repo,binary,fixture,output=map(Path,sys.argv[1:5]);sys.path.insert(0,str(repo/'scripts'))
from mcp_reference import StdioClient
checks=[]
def check(name,ok=True):
 assert ok,name
 checks.append(name)
with StdioClient([str(binary),'--headless','--agent-read-only','--open',str(fixture)],repo)as c:
 c.initialize();names=[t['name']for t in c.rpc('tools/list')['tools']]
 check('immutable annotation preview remains discoverable','preview_schematic_annotation'in names)
 check('annotation mutation is hidden','annotate_schematic'not in names)
 before=c.call('get_session_state');status=c.call('get_document_status');preview=c.call('preview_schematic_annotation',{'mode':'reset_all','start':5})
 check('readonly preview has twelve unit rows and two package owners',preview['symbols']==12 and preview['packages']==2 and len(preview['changes'])==12)
 check('readonly preview exact cursor and project',preview['source_cursor']==before['cursor']and preview['command']['expected_project']==before['project']['id'])
 check('readonly preview preserves complete session status',c.call('get_session_state')==before and c.call('get_document_status')==status)
 try:c.call('annotate_schematic',preview['command'])
 except ValueError as error:assert 'MCP protocol error during tools/call'in str(error)
 else:raise AssertionError('hidden setter must refuse')
 check('hidden command refuses unchanged',c.call('get_session_state')==before and c.call('get_document_status')==status)
 check('native input bytes unchanged',json.loads(fixture.read_text())['project']==before['project']);c.finish()
report={'passed':True,'checks':checks,'assertion_count':len(checks),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'fixture':str(fixture),'scope':'actual no-display headless readonly process'}
with output.open('x')as f:f.write(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
