import sys
sys.dont_write_bytecode=True
import argparse,copy,hashlib,importlib.util,json,os,select,subprocess,tempfile,time,uuid
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--repo',required=True);parser.add_argument('--binary',required=True);parser.add_argument('--url');parser.add_argument('--report',required=True);args=parser.parse_args()
repo=Path(args.repo);binary=Path(args.binary);directory=Path(tempfile.mkdtemp(prefix='viamind-t0076-native-'))
newid=lambda:str(uuid.uuid4())
parts={}
for ref,net,x,y,side,rotation in [('J1','STRAIGHT',10000123,10000129,'bottom',90000),('J2','STRAIGHT',30000257,10000129,'bottom',90000),('J3','VIA',10000000,30000000,'top',0),('J4','VIA',30000000,30000000,'bottom',0),('J5','UNKNOWN',10000000,50000000,'top',45000),('J6','UNKNOWN',30000000,50000000,'top',0)]:
 pid=newid();parts[pid]={'id':pid,'designator':ref,'value':'original','footprint':'original-SMD','position':{'x':x,'y':y},'rotation_mdeg':rotation,'side':side,'pads':[{'id':newid(),'number':'1','position':{'x':123 if rotation==90000 else 0,'y':257 if rotation==90000 else 0},'width':1000000,'height':1000000,'net':net}]}
fixture=directory/'original.viamind.json';fixture.write_text(json.dumps({'format':'viamind','version':4,'project':{'id':newid(),'name':'Original endpoint continuity example','board':{'id':newid(),'parts':parts,'tracks':{},'vias':{},'outline':{'outer':[{'x':0,'y':0},{'x':80000000,'y':0},{'x':80000000,'y':70000000},{'x':0,'y':70000000}],'cutouts':[]}},'schematic':{'sheets':{}}}}))
checks=[];proc=None;clients=[];seq=0
report={'passed':False,'checks':checks,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'transport':args.url or 'production headless stdio','fixture':str(fixture)}
def check(name,condition):
 assert condition,name
 checks.append(name)
if args.url:
 spec=importlib.util.spec_from_file_location('mcp_smoke',repo/'scripts/mcp-smoke.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
 clients=[module.McpClient(args.url,'continuity-native-a'),module.McpClient(args.url,'continuity-native-b')]
 def rpc(method,params=None):return clients[0].rpc(method,params or {})
 def call(name,params=None,error=False,peer=False):return clients[int(peer)].call(name,params or {},expect_error=error)
 call('open_project',{'path':str(fixture),'discard_dirty':False,'expected_cursor':call('get_session_state')['cursor']})
else:
 env=os.environ.copy();env.pop('DISPLAY',None);env.pop('WAYLAND_DISPLAY',None)
 proc=subprocess.Popen([str(binary),'--headless','--open',str(fixture)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env)
 def rpc(method,params=None,notification=False):
  global seq
  seq+=1;request={'jsonrpc':'2.0','method':method,'params':params or {}}
  if not notification:request['id']=seq
  proc.stdin.write(json.dumps(request)+'\n');proc.stdin.flush()
  if notification:return
  deadline=time.monotonic()+20
  while time.monotonic()<deadline:
   assert select.select([proc.stdout],[],[],max(0,deadline-time.monotonic()))[0],method
   response=json.loads(proc.stdout.readline())
   if response.get('id')==seq:
    assert 'error' not in response,response
    return response['result']
  raise AssertionError(method)
 def call(name,params=None,error=False,peer=False):
  result=rpc('tools/call',{'name':name,'arguments':params or {}});assert bool(result.get('isError'))==error,result
  if error:return result
  return result.get('structuredContent') or json.loads(result['content'][0]['text'])
 rpc('initialize',{'protocolVersion':'2025-03-26','capabilities':{},'clientInfo':{'name':'continuity-native','version':'1'}});rpc('notifications/initialized',notification=True)
try:
 tools={t['name'] for t in rpc('tools/list')['tools']}
 check('shared query and exact pad/via routing tools discovered',{'query_connectivity','query_nets','route_pad_polyline','route_pad_via_polyline'}<=tools)
 before=call('get_session_state');status=call('get_document_status')
 def guides(net,peer=False):return call('query_connectivity',{'net':net},peer=peer)
 check('three independent initial required connections',all(guides(n)['guide_count']==1 for n in ['STRAIGHT','VIA','UNKNOWN']))
 check('arbitrary rotation explicitly unsupported and fallback absent',guides('UNKNOWN')['copper_proof']['unsupported_named_pads']==1 and not guides('STRAIGHT')['copper_proof']['budget_fallback'])
 check('repeated peer queries preserve session dirty state and ordering',guides('STRAIGHT')==guides('STRAIGHT',peer=bool(args.url)) and call('get_session_state')==before and call('get_document_status')==status)
 page=call('query_route_endpoints',{'net':'STRAIGHT'});route={'expected_project':page['project_id'],'expected_cursor':page['source_cursor'],'start':page['endpoints'][0]['id'],'target':page['endpoints'][1]['id'],'interior':[],'grid':250000,'width':250000}
 start=time.perf_counter();call('route_pad_polyline',route);report['same_side_route_ms']=(time.perf_counter()-start)*1000
 straight=call('get_session_state');r=guides('STRAIGHT',peer=bool(args.url))
 check('exact off-grid mirrored quarter-turn chain clears only its guide',r['guide_count']==0 and r['terminal_count']==2 and guides('VIA')['guide_count']==1 and guides('UNKNOWN')['guide_count']==1)
 check('net summaries and graph query share residual guide counts',{n['name']:n['guide_count'] for n in call('query_nets')['nets']}=={'STRAIGHT':0,'VIA':1,'UNKNOWN':1})
 check('readonly proof does not create events history or dirty changes',call('get_session_state')==straight)
 call('route_pad_polyline',route,error=True,peer=bool(args.url));check('stale peer routing leaves entire session unchanged',call('get_session_state')==straight)
 if args.url:
  pid=page['endpoints'][0]['id']['part_id'];call('update_part',{'id':pid,'x_mm':40},peer=True)
  check('peer movement restores now-unproven connection guide',guides('STRAIGHT')['guide_count']==1)
  call('undo',peer=True);check('peer own undo restores route and clears guide',call('get_design')==straight['project'] and guides('STRAIGHT')['guide_count']==0)
 call('undo');check('one creator undo restores exact model and guide',call('get_design')==before['project'] and guides('STRAIGHT')['guide_count']==1)
 call('redo');check('redo retains exact route identity and clears guide',call('get_design')==straight['project'] and guides('STRAIGHT')['guide_count']==0)
 page=call('query_route_endpoints',{'net':'VIA'});call('route_pad_via_polyline',{'expected_project':page['project_id'],'expected_cursor':page['source_cursor'],'start':page['endpoints'][0]['id'],'target':page['endpoints'][1]['id'],'via':{'x':20000000,'y':30000000},'before':[],'after':[],'grid':250000,'width':250000,'via_diameter':600000,'via_drill':300000})
 joined=call('get_session_state');r=guides('VIA',peer=bool(args.url))
 check('opposite-side through-via chain clears guide with analyzed proof',r['guide_count']==0 and r['copper_proof']['analyzed_vias']==1 and r['copper_proof']['analyzed_tracks']==3)
 check('unsupported isolated connection remains visible after both routes',guides('UNKNOWN')['guide_count']==1)
 call('undo');check('one via-route undo restores cross-side guide',guides('VIA')['guide_count']==1 and guides('STRAIGHT')['guide_count']==0)
 call('redo');check('via route redo retains exact model and clears guide',call('get_design')==joined['project'] and guides('VIA')['guide_count']==0)
 final=directory/'final.viamind.json';call('save_project',{'path':str(final)});saved=json.loads(final.read_text());check('native save preserves every identity and current copper',saved['project']==joined['project'])
 call('open_project',{'path':str(final),'expected_cursor':call('get_session_state')['cursor']});check('reopen derives same residual guides from persisted copper',guides('STRAIGHT')['guide_count']==0 and guides('VIA')['guide_count']==0 and guides('UNKNOWN')['guide_count']==1)
 report['saved_path']=str(final)
 if proc:proc.stdin.close();check('clean headless process exit',proc.wait(timeout=15)==0)
 report['passed']=True
finally:
 if proc and proc.poll() is None:proc.terminate();proc.wait(timeout=5)
 for c in clients:c.close()
 Path(args.report).write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
