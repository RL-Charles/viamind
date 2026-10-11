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
def reject(args):
 out=b.rpc('tools/call',{'name':'route_pad_posture','arguments':args});assert out.get('isError'),out
state=a.call('get_session_state',{});project=state['project']['id']
a.call('set_board_outline',{'expected':{'project_id':project,'outline':None},'outline':{'outer':[mm(0,0),mm(80,0),mm(80,50),mm(0,50)],'cutouts':[]}})
def part(name,x,y,side,net):
 oid=a.call('add_part',{'designator':name,'value':'Original','footprint':'explicit','x_mm':x,'y_mm':y,'side':side,'rotation_deg':90})['created'][0]
 before=a.call('get_design',{})['board']['parts'][oid]
 a.call('set_part_pads',{'id':oid,'expected':before,'pads':[{'number':'1','x_mm':0.000123,'y_mm':0.000257,'width_mm':2,'height_mm':2,'net':net}]})
 return oid
ids=[part('J1',10.000001,10.000003,'top','SIGNAL'),part('J2',60.000007,30.000011,'top','SIGNAL'),part('J3',10.000001,40.000003,'bottom','POWER'),part('J4',60.000007,30.000011,'bottom','POWER')]
check('actual native posture tool discovered','route_pad_posture' in {t['name'] for t in a.rpc('tools/list',{})['tools']})
before=a.call('get_session_state',{});page=a.call('query_route_endpoints',{});check('endpoint discovery is immutable',a.call('get_session_state',{})==before)
rows={r['designator']:r for r in page['endpoints']}
check('exact quarter-turn centers retained',rows['J1']['center_nm']==p(10_000_258,9_999_880) and rows['J2']['center_nm']==p(60_000_264,29_999_888))
def route(mode='octilinear',posture='primary',start='J1',target='J2',waypoints=[]):
 page=a.call('query_route_endpoints',{});rows={r['designator']:r for r in page['endpoints']}
 return {'expected_project':page['project_id'],'expected_cursor':page['source_cursor'],'start':rows[start]['id'],'target':rows[target]['id'],'grid':250000,'width':250000,'mode':mode,'posture':posture,'waypoints':waypoints}
for mode,posture,bend in [('cardinal','primary',p(60000000,10000000)),('cardinal','alternate',p(10000000,30000000)),('octilinear','primary',p(40000000,10000000)),('octilinear','alternate',p(30000000,30000000))]:
 args=route(mode,posture);before=a.call('get_session_state',{});t=time.perf_counter();out=a.call('route_pad_posture',args);elapsed=(time.perf_counter()-t)*1000
 report.setdefault('posture_http_commit_ms',[]).append({'mode':mode,'posture':posture,'ms':elapsed})
 after=a.call('get_session_state',{});board=after['project']['board'];tracks=[board['tracks'][i] for i in out['created']]
 expected=[rows['J1']['center_nm'],p(10000000,10000000),bend,p(60000000,30000000),rows['J2']['center_nm']]
 check(mode+' '+posture+' exact generated path and stubs',len(tracks)==4 and all(t['start']==expected[i] and t['end']==expected[i+1] and t['layer']=='top' and t['net']=='SIGNAL' for i,t in enumerate(tracks)))
 reject(args);check(mode+' '+posture+' second-agent stale rejects whole plan',a.call('get_session_state',{})==after)
 a.call('undo',{});check(mode+' '+posture+' one undo removes all planned segments',a.call('get_design',{})==before['project'])
 a.call('redo',{});check(mode+' '+posture+' redo preserves all generated IDs',a.call('get_design',{})==after['project']);a.call('undo',{})
for label,args in [('off-grid waypoint',route(waypoints=[p(1,1)])),('waypoint capacity',route(waypoints=[p(0,0)]*511)),('zero grid',dict(route(),grid=0)),('opposite side',route(start='J1',target='J3')),('same endpoint',route(start='J1',target='J1'))]:
 before=a.call('get_session_state',{});reject(args);check('atomic rejection '+label,a.call('get_session_state',{})==before)
old=route();a.call('update_part',{'id':ids[0],'side':'bottom'});before=a.call('get_session_state',{});reject(old);check('real concurrent side edit invalidates frozen plan',a.call('get_session_state',{})==before);a.call('undo',{})
# Foreign copper intercepts the generated straight leg.
a.call('add_track',{'start_x_mm':25,'start_y_mm':9,'end_x_mm':25,'end_y_mm':11,'width_mm':0.25,'layer':'top','net':'FOREIGN'})
before=a.call('get_session_state',{});reject(route());check('generated segment gets joint scoped clearance preflight',a.call('get_session_state',{})==before);a.call('undo',{})
times=[]
for _ in range(5):
 args=route();t=time.perf_counter();a.call('route_pad_posture',args);times.append((time.perf_counter()-t)*1000);a.call('undo',{})
report['warm_http_commit_ms']=times;report['warm_http_mean_ms']=statistics.mean(times)
a.call('route_pad_posture',route());a.call('route_pad_posture',route('cardinal','primary','J3','J4'))
before=a.call('get_session_state',{});a.call('query_connectivity',{'net':'SIGNAL'});check('connectivity remains a separate immutable API',a.call('get_session_state',{})==before)
a.call('save_project',{'path':'/tmp/viamind-t0077-route.viamind.json','overwrite':True})
report['passed']=True;Path('/tmp/viamind-t0077-native-http.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
