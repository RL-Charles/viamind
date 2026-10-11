import argparse,copy,hashlib,importlib.util,json,os,select,subprocess,tempfile,time
from pathlib import Path
repo=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description='Verify an owned Viamind process against the linked regulator fixture.');parser.add_argument('--binary',required=True);parser.add_argument('--url');parser.add_argument('--fixture',default=str(repo/'fixtures/schematic/linked-regulator.viamind.json'));parser.add_argument('--report',required=True);args=parser.parse_args()
checks=[];proc=None;clients=[];seq=0
binary=Path(args.binary);fixture=Path(args.fixture)
def check(name,condition):
 assert condition,name
 checks.append(name)
if args.url:
 spec=importlib.util.spec_from_file_location('mcp_smoke',repo/'scripts/mcp-smoke.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
 clients=[module.McpClient(args.url,'pcb-sync-native-a'),module.McpClient(args.url,'pcb-sync-native-b')]
 def rpc(method,params=None):return clients[0].rpc(method,params)
 def call(name,params=None,error=False,peer=False):return clients[int(peer)].call(name,params or {},expect_error=error)
else:
 env=os.environ.copy();env.pop('DISPLAY',None);env.pop('WAYLAND_DISPLAY',None)
 proc=subprocess.Popen([str(binary),'--headless','--open',str(fixture)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env)
 def rpc(method,params=None,notification=False):
  global seq
  seq+=1;message={'jsonrpc':'2.0','method':method,'params':params or {}}
  if not notification:message['id']=seq
  proc.stdin.write(json.dumps(message)+'\n');proc.stdin.flush()
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
 rpc('initialize',{'protocolVersion':'2025-03-26','capabilities':{},'clientInfo':{'name':'pcb-sync-native','version':'1'}});rpc('notifications/initialized',notification=True)
report={'passed':False,'checks':checks,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'transport':args.url or 'production headless stdio'}
try:
 tools={t['name']:t for t in rpc('tools/list',{})['tools']}
 check('production link preview and sync discovery with object schemas',all(name in tools and tools[name]['inputSchema']['type']=='object' for name in ['preview_schematic_pcb_sync','synchronize_schematic_pcb','link_schematic_symbol']))
 if args.url:
  status=call('get_document_status');call('open_project',{'path':str(fixture),'discard_dirty':False,'expected_cursor':call('get_session_state')['cursor']})
 before=call('get_session_state');preview=call('preview_schematic_pcb_sync',{'direction':'forward'})
 check('immutable deterministic thirteen-change preview',len(preview['changes'])==13 and len(preview['nets'])==3 and call('preview_schematic_pcb_sync',{'direction':'forward'})==preview and call('get_session_state')==before)
 part_id=next(iter(before['project']['board']['parts']))
 call('update_part',{'id':part_id,'value':'Concurrent native edit'},peer=True)
 concurrent=call('get_session_state');call('synchronize_schematic_pcb',preview['command'],error=True)
 check('stale command preserves complete native session',call('get_session_state')==concurrent)
 start=time.perf_counter();preview=call('preview_schematic_pcb_sync',{'direction':'forward'});report['preview_ms']=(time.perf_counter()-start)*1000
 start=time.perf_counter();call('synchronize_schematic_pcb',preview['command']);report['apply_ms']=(time.perf_counter()-start)*1000
 after=call('get_design');parts={p['designator']:p for p in after['board']['parts'].values()}
 def net(ref,pin):return next(p['net'] for p in parts[ref]['pads'] if p['number']==pin)
 check('independent expected VIN 3V3 and GND groups',net('U1','1')==net('C1','1') and net('U1','3')==net('C2','1') and net('U1','2')==net('C1','2')==net('C2','2') and len({net('U1',str(n)) for n in range(1,4)})==3)
 normalized=copy.deepcopy(after)
 for ident,p in normalized['board']['parts'].items():
  old=concurrent['project']['board']['parts'][ident];p['designator']=old['designator'];p['value']=old['value']
  for pad,old_pad in zip(p['pads'],old['pads']):pad['net']=old_pad['net']
 check('forward changes preserve all geometry copper and schematic data',normalized==concurrent['project'])
 check('one transaction event',call('get_session_state')['cursor']['sequence']==concurrent['cursor']['sequence']+1)
 if args.url:
  state=call('get_session_state');call('undo',error=True,peer=True);check('peer cannot undo overlapping synchronization',call('get_session_state')==state)
 call('undo');check('one native undo restores all pad nets and properties',call('get_design')==concurrent['project']);call('redo');check('native redo restores complete update',call('get_design')==after)
 directory=Path(tempfile.mkdtemp(prefix='viamind-t0066-native-'));path=directory/'linked.viamind.json';call('save_project',{'path':str(path)});saved=json.loads(path.read_text())
 check('native schema6 preserves stable links and assigned nets',saved['version']==6 and saved['project']==after)
 old_cursor=call('get_session_state')['cursor'];call('open_project',{'path':str(path),'expected_cursor':old_cursor});reopened=call('get_session_state');call('synchronize_schematic_pcb',preview['command'],error=True)
 check('same UUID reopened stream rejects old command unchanged',call('get_session_state')==reopened and reopened['cursor']['stream_id']!=old_cursor['stream_id'])
 regulator=parts['U1'];call('update_part',{'id':regulator['id'],'designator':'U99','value':'Board revision'})
 back_before=call('get_design');back=call('preview_schematic_pcb_sync',{'direction':'back'});call('synchronize_schematic_pcb',back['command']);back_after=call('get_design')
 sid,sheet=next(iter(back_after['schematic']['sheets'].items()));symbol=sheet['content']['symbols'][regulator['schematic_symbol']]
 check('back annotation uses stable link after reference rename',symbol['designator']=='U99' and symbol['value']=='Board revision' and symbol['footprint']==regulator['footprint'] and back_after['board']==back_before['board'])
 normalized=copy.deepcopy(back_after)
 for ident,s in normalized['schematic']['sheets'][sid]['content']['symbols'].items():
  old=back_before['schematic']['sheets'][sid]['content']['symbols'][ident]
  for key in ['designator','value','footprint']:s[key]=old[key]
 check('back changes preserve pins topology positions and copper',normalized==back_before)
 call('undo');check('single native back undo restores whole schematic',call('get_design')==back_before)
 current=call('get_design');part=current['board']['parts'][regulator['id']]
 call('link_schematic_symbol',{'expected_project':current['id'],'expected_part':part,'target':{'kind':'unlink'}})
 check('explicit unlink affects only association',call('get_design')['board']['parts'][part['id']].get('schematic_symbol') is None)
 call('undo');check('native unlink undo restores stable association',call('get_design')==current)
 call('save_project',{'path':str(directory/'final.viamind.json')})
 report['saved_path']=str(directory/'final.viamind.json')
 if proc:
  proc.stdin.close();check('clean production process exit',proc.wait(timeout=15)==0)
 report['passed']=True
finally:
 if proc and proc.poll() is None:proc.terminate();proc.wait(timeout=5)
 for client in clients:client.close()
 Path(args.report).write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
