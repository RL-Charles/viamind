import json, os, select, subprocess, time, sys
from pathlib import Path
binary=Path(sys.argv[1]);fixture=Path(sys.argv[2]).resolve()
env=os.environ.copy();env.pop('DISPLAY',None);env.pop('WAYLAND_DISPLAY',None)
p=subprocess.Popen([str(binary),'--headless'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,text=True)
seq=0

def rpc(method,params=None,notification=False):
 global seq
 seq+=1;msg={'jsonrpc':'2.0','method':method,'params':params or {}}
 if not notification:msg['id']=seq
 p.stdin.write(json.dumps(msg)+'\n');p.stdin.flush()
 if notification:return
 deadline=time.monotonic()+15
 while time.monotonic()<deadline:
  assert select.select([p.stdout],[],[],max(0,deadline-time.monotonic()))[0],method
  response=json.loads(p.stdout.readline())
  if response.get('id')==seq:
   assert 'error' not in response,response
   return response['result']
 raise AssertionError(method)
def call(name,args=None):
 r=rpc('tools/call',{'name':name,'arguments':args or {}});assert not r.get('isError'),r
 return r.get('structuredContent') or json.loads(r['content'][0]['text'])
try:
 rpc('initialize',{'protocolVersion':'2025-03-26','capabilities':{},'clientInfo':{'name':'native-cross-probe','version':'1'}});rpc('notifications/initialized',notification=True)
 assert 'query_linked_components' in {t['name'] for t in rpc('tools/list')['tools']}
 call('open_project',{'path':str(fixture)})
 before=call('get_session_state');dirty=call('get_document_status')
 report=call('query_linked_components');assert report['total']==3 and all(r['status']=='linked' for r in report['components'])
 assert report['source_cursor']==before['cursor'] and call('query_linked_components')==report
 assert call('get_session_state')==before and call('get_document_status')==dirty
 first=report['components'][0];ids=[first['part_id'],first['symbol_id']]
 one=call('query_linked_components',{'object_ids':ids});assert one['total']==1 and one['components']==[first]
 page=call('query_linked_components',{'limit':1});assert page['next_offset']==1
 page=call('query_linked_components',{'limit':1,'offset':2});assert page['next_offset'] is None and page['components']==report['components'][2:]
 call('set_selection',{'ids':[first['part_id']]})
 selected=call('get_session_state');call('query_linked_components',{'object_ids':ids});assert call('get_session_state')==selected
 call('update_part',{'id':first['part_id'],'designator':'NATIVE1'})
 renamed=call('query_linked_components',{'object_ids':[first['symbol_id']]});assert renamed['components'][0]['part_id']==first['part_id'] and renamed['components'][0]['part_reference']=='NATIVE1'
 assert renamed['source_cursor']!=report['source_cursor']
 state=call('get_session_state');part=state['project']['board']['parts'][first['part_id']]
 call('link_schematic_symbol',{'expected_project':state['project']['id'],'expected_part':part,'target':{'kind':'unlink'}})
 assert call('query_linked_components',{'object_ids':[first['symbol_id']]})['components'][0]['status']=='unlinked_symbol'
 call('undo');assert call('query_linked_components',{'object_ids':[first['symbol_id']]})['components'][0]['status']=='linked'
 p.stdin.close();assert p.wait(timeout=15)==0
 print(json.dumps({'result':'passed','native_binary':str(binary),'fixture':str(fixture),'verified':['tool discovery','three explicit regulator links','stable project/cursor and repeated immutable report','dual-ID filtering without duplicate rows','deterministic pagination','shared selection preserved','renamed references retain UUID link','explicit unlink and own undo','clean exit']},indent=2))
finally:
 if p.poll() is None:p.terminate();p.wait(timeout=5)
