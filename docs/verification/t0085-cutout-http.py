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
def set_outline(cutouts):
 state=a.call('get_session_state',{});before=state['project']['board'].get('outline')
 return b.call('set_board_outline',{'expected':{'project_id':state['project']['id'],'outline':before},'outline':{'outer':[mm(0,0),mm(40,0),mm(40,20),mm(0,20)],'cutouts':cutouts}})
def rect(x0,y0,x1,y1):return [mm(x0,y0),mm(x1,y0),mm(x1,y1),mm(x0,y1)]
set_outline([rect(19,8,21,12)])
for name,x in [('J1',10.000123),('J2',30.000257)]:
 oid=a.call('add_part',{'designator':name,'value':'Original','footprint':'explicit','x_mm':x,'y_mm':10.000129,'side':'top','rotation_deg':90})['created'][0]
 before=a.call('get_design',{})['board']['parts'][oid]
 a.call('set_part_pads',{'id':oid,'expected':before,'pads':[{'number':'1','x_mm':0,'y_mm':0,'width_mm':1.000001,'height_mm':1.000001,'net':'N'}]})
names={t['name'] for t in a.rpc('tools/list',{})['tools']};check('actual production shared cutout plan and commit tools discovered',{'plan_pad_walkaround','route_pad_walkaround'}<=names)
def args():
 page=a.call('query_route_endpoints',{});rows={r['designator']:r for r in page['endpoints']}
 return {'expected_project':page['project_id'],'expected_cursor':page['source_cursor'],'start':rows['J1']['id'],'target':rows['J2']['id'],'grid':1000000,'width':250001}
expected=[p(10000123,10000129),p(10000000,10000000),p(10000000,7000000),p(22000000,7000000),p(22000000,10000000),p(30000000,10000000),p(30000257,10000129)]
request=args();before=a.call('get_session_state',{});t=time.perf_counter();plan=a.call('plan_pad_walkaround',request);report['first_http_plan_ms']=(time.perf_counter()-t)*1000
check('complete session unchanged by cutout planning',a.call('get_session_state',{})==before)
check('independent exact seven-point cutout detour and off-grid stubs',plan['plan']['path']['points']==expected and plan['plan']['path']['endpoint_stub_segments']==2)
check('complete scoped preflight and substrate box counted',plan['complete_scoped_preflight'] and plan['plan']['work']['obstacle_boxes']==1)
check('derived side net and full captured cursor',plan['layer']=='top' and plan['net']=='N' and plan['source_cursor']==before['cursor'])
copper=dict(request,rules={'copper_clearance_mm':3.0});check('copper clearance alone does not alter substrate route',a.call('plan_pad_walkaround',copper)['plan']['path']['points']==expected)
for edge,y in [(0.874997,7000000),(0.874998,6000000)]:
 q=dict(request,rules={'edge_clearance_mm':edge});result=a.call('plan_pad_walkaround',q)
 check(f'odd half-width and strict1Nm edge margin at {edge}',result['plan']['path']['points'][2]==p(10000000,y))
check('clearance variation queries all remain immutable',a.call('get_session_state',{})==before)
t=time.perf_counter();out=a.call('route_pad_walkaround',request);report['first_http_commit_ms']=(time.perf_counter()-t)*1000;after=a.call('get_session_state',{})
tracks=[after['project']['board']['tracks'][i] for i in out['created']]
check('recomputed six-segment commit equals checked exact path',len(tracks)==6 and all(track['start']==expected[i] and track['end']==expected[i+1] and track['net']=='N' and track['layer']=='top' for i,track in enumerate(tracks)))
check('all cutout route segments use one session event',after['cursor']['sequence']==before['cursor']['sequence']+1)
for tool in ['plan_pad_walkaround','route_pad_walkaround']:
 reject(tool,request);check('second-agent stale '+tool+' rejects unchanged',a.call('get_session_state',{})==after)
a.call('undo',{});check('one creator undo removes entire cutout route',a.call('get_design',{})==before['project']);a.call('redo',{});check('redo preserves exact segment IDs and geometry',a.call('get_design',{})==after['project']);a.call('undo',{})
old=args();set_outline([rect(19,6,21,14)]);before=a.call('get_session_state',{})
for tool in ['plan_pad_walkaround','route_pad_walkaround']:reject(tool,old)
check('real peer outline edit invalidates both old guards',a.call('get_session_state',{})==before)
check('fresh edited-cutout plan recomputes different geometry',a.call('plan_pad_walkaround',args())['plan']['path']['points']!=expected)
for label,cutouts in [('nonrectangular cutout',[[mm(19,8),mm(21,8),mm(20,12)]]),('blocked edge corridors',[rect(19,0.000001,21,19.999999)]),('start inside cutout',[rect(9.9,9.9,10.1,10.1)])]:
 set_outline(cutouts);before=a.call('get_session_state',{});request=args()
 for tool in ['plan_pad_walkaround','route_pad_walkaround']:reject(tool,request)
 check(label+' query and commit refuse atomically',a.call('get_session_state',{})==before)
set_outline([rect(14,8,16,12),rect(24,8,26,12)]);before=a.call('get_session_state',{});plan=a.call('plan_pad_walkaround',args())
expected_multi=[p(10000123,10000129),p(10000000,10000000),p(10000000,7000000),p(30000000,7000000),p(30000000,10000000),p(30000257,10000129)]
check('multiple substrate cutouts share counted exact detour',plan['plan']['work']['obstacle_boxes']==2 and plan['plan']['path']['points']==expected_multi)
check('multiple-cutout checked snapshot stays immutable',a.call('get_session_state',{})==before)
a.call('route_pad_walkaround',args());a.call('undo',{});check('multiple-cutout route one undo preserves substrate',a.call('get_design',{})==before['project'])
set_outline([rect(19,8,21,12)]);times=[]
for _ in range(5):
 request=args();t=time.perf_counter();a.call('route_pad_walkaround',request);times.append((time.perf_counter()-t)*1000);a.call('undo',{})
report['warm_http_commit_ms']=times;report['warm_http_mean_ms']=statistics.mean(times)
a.call('route_pad_walkaround',args());before=a.call('get_session_state',{});a.call('query_connectivity',{'net':'N'});check('separate continuity query remains immutable',a.call('get_session_state',{})==before)
path='/home/charlesmowbray/.cache/viamind/native-artifacts/t0085-route.viamind.json';a.call('save_project',{'path':path,'overwrite':True});old=args();a.call('open_project',{'path':path,'discard_dirty':False,'expected_cursor':old['expected_cursor']});before=a.call('get_session_state',{})
for tool in ['plan_pad_walkaround','route_pad_walkaround']:reject(tool,old)
check('same-project reopened stream invalidates cutout plan and commit',a.call('get_session_state',{})==before and before['cursor']['stream_id']!=old['expected_cursor']['stream_id'])
report['passed']=True;Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0085-native-http.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
