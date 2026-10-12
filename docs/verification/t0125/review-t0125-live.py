import sys,importlib.util,json,hashlib
from pathlib import Path
sys.dont_write_bytecode=True
repo=Path('/home/charlesmowbray/Development/viamind-t-0125')
spec=importlib.util.spec_from_file_location('mcp',repo/'scripts/mcp-smoke.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
x=m.McpClient('http://127.0.0.1:7962/mcp','independent reviewer');y=m.McpClient('http://127.0.0.1:7962/mcp','independent reviewer')
checks=[]
def check(name,p):
 assert p,name
 checks.append(name)
def state(c):return [c.call(t,{}) for t in ['get_session_state','get_document_status','get_auto_drc_event_state']]
def uid(n):return f'00000000-0000-0000-0000-{n:012x}'
def arg(s):return dict(expected_project=s[0]['project']['id'],source_cursor=s[0]['cursor'],sheet_id=uid(502),symbol_id=uid(503),delta=dict(x=3000000,y=5000000))
def groups(c,old=None):
 g=c.call('query_schematic_connectivity',{'offset':0,'limit':100});assert g.get('next_offset') is None
 members={v['id'] for n in g['nets'] for v in n['members']}
 return sorted((sorted((v['sheet_id'],v['id'],v['kind']) for v in n['members'] if old is None or v['id'] in old),sorted(json.dumps(z,sort_keys=True) for z in n['names'])) for n in g['nets']),members
try:
 before=state(x);native=Path(before[1]['path']);raw=native.read_bytes();pg,old=groups(x);a=arg(before)
 check('owned fixture identity',native==Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0125/production-owned.viamind.json') and before[0]['project']==json.loads((repo/'fixtures/schematic/connected-drag.viamind.json').read_bytes())['project'])
 check('retained live executable identity',Path('/proc/3537034/exe').resolve()==Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0125/final-verified/viamind'))
 r=x.call('preview_schematic_symbol_drag',a)
 check('two-client deterministic source immutability',r==y.call('preview_schematic_symbol_drag',a) and state(x)==state(y)==before and native.read_bytes()==raw)
 check('independent target and exact bend count',r['target_position']==dict(x=23000000,y=25000000) and r['added_wire_count']==3 and len(r['segments'])==6)
 y.call('add_part',{'designator':'R_REVIEW_PEER','value':'','footprint':'F','x_mm':0,'y_mm':0});peer=state(x)
 x.call('drag_schematic_symbol',a,expect_error=True)
 check('peer change invalidates full guarded cursor',state(x)==state(y)==peer)
 fresh=arg(peer);x.call('drag_schematic_symbol',fresh);moved=state(y)
 check('one event keeps whole PCB exact',moved[0]['cursor']['sequence']==peer[0]['cursor']['sequence']+1 and moved[0]['project']['board']==peer[0]['project']['board'])
 check('independent old UUID partition and label facts',groups(x,old)[0]==pg)
 x.call('undo',{});check('origin undo preserves peer part',state(y)[0]['project']==peer[0]['project'])
 y.call('undo',{});final=state(x)
 check('peer origin undo restores complete original and native bytes',final[0]['project']==before[0]['project'] and final[1]['path']==before[1]['path'] and native.read_bytes()==raw)
 check('honest four-event advancement',final[0]['cursor']['stream_id']==before[0]['cursor']['stream_id'] and final[0]['cursor']['sequence']==before[0]['cursor']['sequence']+4)
 receipt=dict(passed=True,checks=checks,before_cursor=before[0]['cursor'],after_cursor=final[0]['cursor'],native_sha256=hashlib.sha256(raw).hexdigest(),scope='Independent two same-name real production HTTP clients, owned fixture, no OS input.')
 with Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0121/review-t0125-live.json').open('x') as f:json.dump(receipt,f,indent=2)
 print(json.dumps(receipt))
finally:x.close();y.close()
