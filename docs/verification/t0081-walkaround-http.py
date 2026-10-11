import json,urllib.request,sys,time,statistics,hashlib
from pathlib import Path
class Client:
 def __init__(self,port):
  self.url=f'http://127.0.0.1:{port}/mcp'; self.sid=None; self.seq=0
  self.rpc('initialize',{'protocolVersion':'2025-03-26','capabilities':{},'clientInfo':{'name':'codex-verification','version':'1.0'}})
  self.rpc('notifications/initialized',None,notification=True)
 def rpc(self,method,params=None,notification=False):
  self.seq+=1; obj={'jsonrpc':'2.0','method':method}
  if not notification: obj['id']=self.seq
  if params is not None: obj['params']=params
  headers={'Content-Type':'application/json','Accept':'application/json, text/event-stream'}
  if self.sid: headers['Mcp-Session-Id']=self.sid
  req=urllib.request.Request(self.url,data=json.dumps(obj).encode(),headers=headers)
  with urllib.request.urlopen(req,timeout=10) as r:
   self.sid=r.headers.get('Mcp-Session-Id',self.sid)
   raw=r.read().decode()
  if not raw.strip(): return None
  data=[l[5:].strip() for l in raw.splitlines() if l.startswith('data:')]
  if data: raw=data[-1]
  response=json.loads(raw)
  if 'error' in response: raise RuntimeError(response)
  return response.get('result')
 def call(self,name,args):
  r=self.rpc('tools/call',{'name':name,'arguments':args})
  if r.get('isError'): raise RuntimeError(r)
  return r.get('structuredContent') or json.loads(r['content'][0]['text'])
port=int(sys.argv[1]);a,b=Client(port),Client(port)
report={'passed':False,'port':port,'binary_sha256':hashlib.sha256(Path(sys.argv[2]).read_bytes()).hexdigest(),'checks':[]}
p=lambda x,y:{'x':x,'y':y};mm=lambda x,y:{'x_mm':x,'y_mm':y}
def check(label,ok):
 assert ok,label
 report['checks'].append(label)
def reject(tool,args):
 out=b.rpc('tools/call',{'name':tool,'arguments':args});assert out.get('isError'),out
 return out
state=a.call('get_session_state',{});project=state['project']['id']
a.call('set_board_outline',{'expected':{'project_id':project,'outline':None},'outline':{'outer':[mm(0,0),mm(40,0),mm(40,20),mm(0,20)],'cutouts':[]}})
def part(name,x,y,side,net):
 oid=a.call('add_part',{'designator':name,'value':'Original','footprint':'explicit','x_mm':x,'y_mm':y,'side':side,'rotation_deg':90})['created'][0]
 before=a.call('get_design',{})['board']['parts'][oid]
 a.call('set_part_pads',{'id':oid,'expected':before,'pads':[{'number':'1','x_mm':0,'y_mm':0,'width_mm':1.000001,'height_mm':1.000001,'net':net}]})
 return oid
ids=[part('J1',10.000123,10.000129,'top','N'),part('J2',30.000257,10.000129,'top','N')]
names={t['name'] for t in a.rpc('tools/list',{})['tools']};check('actual production plan and commit tools discovered',{'plan_pad_walkaround','route_pad_walkaround'}<=names)
def args():
 page=a.call('query_route_endpoints',{});rows={r['designator']:r for r in page['endpoints']}
 return {'expected_project':page['project_id'],'expected_cursor':page['source_cursor'],'start':rows['J1']['id'],'target':rows['J2']['id'],'grid':1000000,'width':250001}
barrier=a.call('add_track',{'start_x_mm':20,'start_y_mm':8,'end_x_mm':20,'end_y_mm':12,'width_mm':0.250001,'layer':'top','net':'BLOCK'})['created'][0]
expected=[p(10000123,10000129),p(10000000,10000000),p(10000000,7000000),p(21000000,7000000),p(21000000,10000000),p(30000000,10000000),p(30000257,10000129)]
request=args();before=a.call('get_session_state',{});t=time.perf_counter();plan=a.call('plan_pad_walkaround',request);report['first_http_plan_ms']=(time.perf_counter()-t)*1000
check('plan is immutable across complete session',a.call('get_session_state',{})==before)
check('independently expected exact detour and off-grid stubs',plan['plan']['path']['points']==expected and plan['plan']['path']['endpoint_stub_segments']==2)
check('complete checked snapshot and derived net side',plan['complete_scoped_preflight'] and plan['source_cursor']==before['cursor'] and plan['net']=='N' and plan['layer']=='top')
work=plan['plan']['work'];check('bounded work counters supplied',work['obstacle_boxes']==1 and work['graph_nodes']<=20000 and work['candidate_edges']<=100000 and work['intersection_tests']<=4000000)
t=time.perf_counter();out=a.call('route_pad_walkaround',request);report['first_http_commit_ms']=(time.perf_counter()-t)*1000;after=a.call('get_session_state',{});tracks=[after['project']['board']['tracks'][i] for i in out['created']]
check('recomputed commit equals checked snapshot geometry',len(tracks)==6 and all(t['start']==expected[i] and t['end']==expected[i+1] and t['layer']=='top' and t['net']=='N' for i,t in enumerate(tracks)))
check('entire route creates one session event',after['cursor']['sequence']==before['cursor']['sequence']+1)
for tool in ['plan_pad_walkaround','route_pad_walkaround']:
 reject(tool,request);check('second-agent stale '+tool+' leaves entire session unchanged',a.call('get_session_state',{})==after)
a.call('undo',{});check('one undo removes all new tracks and preserves obstacle',a.call('get_design',{})==before['project'])
a.call('redo',{});check('redo restores exact track IDs and geometry',a.call('get_design',{})==after['project']);a.call('undo',{})
for label,bad in [('zero grid',dict(args(),grid=0)),('zero width',dict(args(),width=0)),('same endpoint',dict(args(),target=args()['start'])),('collapsed domain',dict(args(),width=20000000))]:
 before=a.call('get_session_state',{})
 for tool in ['plan_pad_walkaround','route_pad_walkaround']: reject(tool,bad)
 check('atomic query and commit refusal '+label,a.call('get_session_state',{})==before)
old=args();b.call('update_part',{'id':ids[1],'side':'bottom'});before=a.call('get_session_state',{})
for tool in ['plan_pad_walkaround','route_pad_walkaround']:reject(tool,old)
check('real peer side edit invalidates both guards',a.call('get_session_state',{})==before)
for tool in ['plan_pad_walkaround','route_pad_walkaround']:reject(tool,args())
check('fresh incompatible layer intent rejected atomically',a.call('get_session_state',{})==before);b.call('undo',{})
# Existing same-net via holes still divert the route after removing foreign copper.
a.call('delete_track',{'id':barrier})
via=a.call('add_via',{'x_mm':20,'y_mm':10,'diameter_mm':1.000001,'drill_mm':0.300001,'net':'N'})['created'][0]
before=a.call('get_session_state',{});plan=a.call('plan_pad_walkaround',args())
check('same-net via drill hole adds an obstacle',plan['plan']['work']['obstacle_boxes']==1 and len(plan['plan']['path']['points'])>4)
check('same-net drill detour remains read-only',a.call('get_session_state',{})==before)
a.call('route_pad_walkaround',args());after=a.call('get_session_state',{});check('same-net hole detour commit passes complete scoped DRC',len(after['project']['board']['tracks'])>1)
a.call('undo',{});a.call('delete_via',{'id':via})
# Full-height foreign barrier leaves no static path.
a.call('add_track',{'start_x_mm':20,'start_y_mm':0,'end_x_mm':20,'end_y_mm':20,'width_mm':0.25,'layer':'top','net':'BLOCK'})
before=a.call('get_session_state',{})
for tool in ['plan_pad_walkaround','route_pad_walkaround']:reject(tool,args())
check('no-path refuses plan and commit without mutation',a.call('get_session_state',{})==before);a.call('undo',{})
# Restore short barrier and measure real small-board recomputing commit round trips.
a.call('add_track',{'start_x_mm':20,'start_y_mm':8,'end_x_mm':20,'end_y_mm':12,'width_mm':0.250001,'layer':'top','net':'BLOCK'})
times=[]
for _ in range(5):
 request=args();t=time.perf_counter();a.call('route_pad_walkaround',request);times.append((time.perf_counter()-t)*1000);a.call('undo',{})
report['warm_http_commit_ms']=times;report['warm_http_mean_ms']=statistics.mean(times)
a.call('route_pad_walkaround',args());before=a.call('get_session_state',{});a.call('query_connectivity',{'net':'N'});check('continuity query remains separate and immutable',a.call('get_session_state',{})==before)
path='/home/charlesmowbray/.cache/viamind/native-artifacts/t0081-route.viamind.json';a.call('save_project',{'path':path,'overwrite':True});old=args();a.call('open_project',{'path':path,'discard_dirty':False,'expected_cursor':old['expected_cursor']});before=a.call('get_session_state',{})
for tool in ['plan_pad_walkaround','route_pad_walkaround']:reject(tool,old)
check('same-project reopen changes stream and rejects both stale intents',a.call('get_session_state',{})==before and before['cursor']['stream_id']!=old['expected_cursor']['stream_id'])
report['passed']=True;Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0081-native-http.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
