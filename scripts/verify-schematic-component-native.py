import sys
sys.dont_write_bytecode=True
import argparse,copy,hashlib,importlib.util,json,os,select,subprocess,tempfile,time
from pathlib import Path
repo=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description='Verify an owned Viamind process against the linked regulator fixture.');parser.add_argument('--repo',default=str(repo));parser.add_argument('--binary',required=True);parser.add_argument('--url');parser.add_argument('--fixture',default=str(repo/'fixtures/schematic/regulator.viamind.json'));parser.add_argument('--report',required=True);args=parser.parse_args()
checks=[];proc=None;clients=[];seq=0
repo=Path(args.repo);binary=Path(args.binary);fixture=Path(args.fixture)
def check(name,condition):
 assert condition,name
 checks.append(name)
if args.url:
 spec=importlib.util.spec_from_file_location('mcp_smoke',repo/'scripts/mcp-smoke.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
 clients=[module.McpClient(args.url,'component-create-native-a'),module.McpClient(args.url,'component-create-native-b')]
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
 rpc('initialize',{'protocolVersion':'2025-03-26','capabilities':{},'clientInfo':{'name':'component-create-native','version':'1'}});rpc('notifications/initialized',notification=True)
report={'passed':False,'checks':checks,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'transport':args.url or 'production headless stdio'}
try:
 tools={t['name']:t for t in rpc('tools/list',{})['tools']}
 check('creation and preview discovery with object schemas',all(name in tools and tools[name]['inputSchema']['type']=='object' for name in ['preview_schematic_pcb_component','create_schematic_pcb_component']))
 if args.url:call('open_project',{'path':str(fixture),'discard_dirty':False,'expected_cursor':call('get_session_state')['cursor']})
 before=call('get_session_state')
 symbols={s['designator']:s for sheet in before['project']['schematic']['sheets'].values() for s in sheet['content']['symbols'].values()}
 def request(ref):
  current=call('get_session_state');count=len(symbols[ref]['pins']);footprint=json.loads((repo/f'fixtures/footprints/original-smd{count}.viamind-footprint.json').read_text())['footprint']
  return {'expected_project':current['project']['id'],'source_cursor':current['cursor'],'placement':{'symbol_id':symbols[ref]['id'],'footprint':footprint,'position':{'x':40000001 if ref=='U1' else (30000001 if ref=='C1' else 50000001),'y':40000003},'side':'bottom' if ref=='U1' else 'top','rotation_mdeg':90000 if ref=='U1' else 0}}
 req=request('U1');preview=call('preview_schematic_pcb_component',req)
 check('preview is deterministic and preserves full session',preview==call('preview_schematic_pcb_component',req,peer=bool(args.url)) and call('get_session_state')==before and len(preview['pads'])==3)
 peer=call('add_part',{'designator':'R99','value':'peer','footprint':'other','x_mm':5,'y_mm':5},peer=bool(args.url))['created'][0]
 concurrent=call('get_session_state');call('create_schematic_pcb_component',preview['command'],error=True)
 check('stale preview preserves peer edit and entire session',call('get_session_state')==concurrent)
 preview=call('preview_schematic_pcb_component',request('U1'))
 start=time.perf_counter();created=call('create_schematic_pcb_component',preview['command'])['created'][0];report['create_ms']=(time.perf_counter()-start)*1000
 after=call('get_session_state');part=after['project']['board']['parts'][created]
 check('one transaction creates linked exact bottom quarter-turn component',after['cursor']['sequence']==concurrent['cursor']['sequence']+1 and part['schematic_symbol']==symbols['U1']['id'] and part['position']=={'x':40000001,'y':40000003} and part['side']=='bottom' and part['rotation_mdeg']==90000)
 normalized=copy.deepcopy(after['project']);normalized['board']['parts'].pop(created)
 check('all existing geometry copper and schematic content preserved',normalized==concurrent['project'])
 if args.url:
  call('undo',peer=True);expected=copy.deepcopy(after['project']);expected['board']['parts'].pop(peer)
  check('peer own undo preserves independently created component',call('get_design')==expected)
  call('redo',peer=True);check('peer own redo restores only its independent component',call('get_design')==after['project'])
 call('undo');check('one creator undo removes whole linked component',call('get_design')==concurrent['project']);call('redo');check('redo preserves all new part and pad UUIDs',call('get_design')==after['project'])
 checkpoint=call('get_session_state');call('preview_schematic_pcb_component',request('U1'),error=True)
 check('already linked symbol explicitly rejects without session mutation',call('get_session_state')==checkpoint)
 bad=request('C1');bad['placement']['footprint']['pads'].pop();unchanged=call('get_session_state');call('preview_schematic_pcb_component',bad,error=True)
 check('invalid pin pad bijection leaves entire session unchanged',call('get_session_state')==unchanged)
 for ref in ['C1','C2']:
  preview=call('preview_schematic_pcb_component',request(ref));call('create_schematic_pcb_component',preview['command'])
 project=call('get_design');parts={p['designator']:p for p in project['board']['parts'].values()}
 def net(ref,n):return next(p['net'] for p in parts[ref]['pads'] if p['number']==n)
 check('independently expected VIN 3V3 and GND groups across three created components',net('U1','1')==net('C1','1') and net('U1','3')==net('C2','1') and net('U1','2')==net('C1','2')==net('C2','2') and len({net('U1',str(n)) for n in range(1,4)})==3)
 check('created parts require no further forward net synchronization',not call('preview_schematic_pcb_sync',{'direction':'forward'})['changes'])
 # Validate preview rejection after explicit unlink without changing unrelated geometry.
 current=call('get_session_state');regulator=current['project']['board']['parts'][created]
 call('link_schematic_symbol',{'expected_project':project['id'],'expected_part':regulator,'target':{'kind':'unlink'}})
 bad=request('U1');bad['placement']['footprint']['pads'].pop();unchanged=call('get_session_state');call('preview_schematic_pcb_component',bad,error=True)
 check('unlinked symbol with occupied board reference rejects without session mutation',call('get_session_state')==unchanged)
 call('undo');check('unlink undo restores exact component association',call('get_design')==project)
 directory=Path(tempfile.mkdtemp(prefix='viamind-t0072-native-'));path=directory/'created.viamind.json';call('save_project',{'path':str(path)});saved=json.loads(path.read_text())
 check('native schema6 persists all created links identities and nets',saved['version']==6 and saved['project']==project)
 # A preview for a new symbol cannot be prepared here; reuse a previous exact command
 # to demonstrate old stream rejection independently of project identity.
 call('undo');old_command=call('preview_schematic_pcb_component',request('C2'))['command']
 checkpoint_path=directory/'before-c2.viamind.json';call('save_project',{'path':str(checkpoint_path)})
 call('open_project',{'path':str(checkpoint_path),'expected_cursor':call('get_session_state')['cursor']});reopened=call('get_session_state')
 check('same reopened UUID has identical command project guard',old_command['expected']==reopened['project'])
 call('create_schematic_pcb_component',old_command,error=True)
 check('old stream alone refuses otherwise valid creation unchanged',call('get_session_state')==reopened)
 call('create_schematic_pcb_component',call('preview_schematic_pcb_component',request('C2'))['command'])
 final=directory/'final.viamind.json';call('save_project',{'path':str(final)});report['saved_path']=str(final)
 if proc:proc.stdin.close();check('clean production process exit',proc.wait(timeout=15)==0)
 report['passed']=True
finally:
 if proc and proc.poll() is None:proc.terminate();proc.wait(timeout=5)
 for client in clients:client.close()
 Path(args.report).write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
