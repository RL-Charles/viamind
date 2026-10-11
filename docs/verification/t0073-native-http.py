"""Run against a fresh owned native application: script PORT VERIFIED_BINARY."""
import json,sys,time,hashlib,statistics
from pathlib import Path
import urllib.request
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
def check(name,value):
 assert value,name
 report['checks'].append(name)
def reject(args):
 r=b.rpc('tools/call',{'name':'route_pad_via_polyline','arguments':args});assert r.get('isError'),r
 return r
mm=lambda x,y:{'x_mm':x,'y_mm':y}
p=lambda x,y:{'x':x,'y':y}
state=a.call('get_session_state',{});project=state['project']['id']
a.call('set_board_outline',{'expected':{'project_id':project,'outline':None},'outline':{'outer':[mm(0,0),mm(80,0),mm(80,50),mm(0,50)],'cutouts':[]}})
def part(name,x,y,side,rotation,net):
 r=a.call('add_part',{'designator':name,'value':'Connector','footprint':'explicit','x_mm':x,'y_mm':y,'side':side,'rotation_deg':rotation});oid=r['created'][0]
 before=a.call('get_design',{})['board']['parts'][oid]
 a.call('set_part_pads',{'id':oid,'expected':before,'pads':[{'number':'1','x_mm':0.000123,'y_mm':0.000257,'width_mm':2,'height_mm':2,'net':net}]})
 return oid
ids=[part('J1',10.000001,10.000003,'top',90,'SIGNAL'),part('J2',60.000007,10.000003,'bottom',90,'SIGNAL'),part('J3',10.000001,40.000003,'top',0,'POWER'),part('J4',60.000007,40.000003,'bottom',0,'POWER'),part('J5',35,25,'top',45,'UNSUPPORTED')]
check('actual native via tool discovered','route_pad_via_polyline' in {t['name'] for t in a.rpc('tools/list',{})['tools']})
before=a.call('get_session_state',{});page=a.call('query_route_endpoints',{});check('discovery preserves exact session',a.call('get_session_state',{})==before)
rows={row['designator']:row for row in page['endpoints']}
check('exact top and mirrored bottom quarter-turn centers',rows['J1']['center_nm']==p(10_000_258,9_999_880) and rows['J2']['center_nm']==p(60_000_264,10_000_126))
def route(start='J1',target='J2',via=p(35_000_000,20_000_000)):
 page=a.call('query_route_endpoints',{});rows={r['designator']:r for r in page['endpoints']}
 return {'expected_project':page['project_id'],'expected_cursor':page['source_cursor'],'start':rows[start]['id'],'target':rows[target]['id'],'via':via,'before':[p(20_000_000,15_000_000)],'after':[p(50_000_000,15_000_000)],'grid':250000,'width':250000,'via_diameter':600000,'via_drill':300000}
input=route();t=time.perf_counter();result=a.call('route_pad_via_polyline',input);report['first_http_commit_ms']=(time.perf_counter()-t)*1000
check('four track segments and one via committed atomically',len(result['created'])==5)
after=a.call('get_session_state',{});board=after['project']['board'];tracks=[board['tracks'][i] for i in result['created'][:4]];via=board['vias'][result['created'][4]]
check('derived two top and two bottom tracks and net',all(t['layer']==('top' if i<2 else 'bottom') and t['net']=='SIGNAL' for i,t in enumerate(tracks)))
check('exact off-grid attachment retained',tracks[0]['start']==rows['J1']['center_nm'] and tracks[-1]['end']==rows['J2']['center_nm'])
check('through via dimensions and position preserved',via['position']==input['via'] and via['diameter']==600000 and via['drill']==300000 and via['net']=='SIGNAL')
reject(input);check('second-agent stale route changes nothing',a.call('get_session_state',{})==after)
a.call('undo',{});check('one undo removes both legs and via',not a.call('get_design',{})['board'].get('tracks',{}) and not a.call('get_design',{})['board'].get('vias',{}));a.call('redo',{});check('one redo restores all IDs',a.call('get_design',{})==after['project'])
for label,args in [('off-grid via',dict(route(),via=p(1,1))),('off-grid interior',dict(route(),after=[p(1,1)])),('same-side pads',route('J1','J3')),('arbitrary-angle target',route('J1','J5')),('undersized annulus',dict(route(),via_diameter=400000)),('invalid drill',dict(route(),via_drill=600000)),('zero length',dict(route(),after=[p(35000000,20000000)]))]:
 before=a.call('get_session_state',{});reject(args);check('atomic rejection '+label,a.call('get_session_state',{})==before)
# Same-net copper clearance is exempt, physical drill-hole overlap still rejects.
a.call('add_via',{'x_mm':35.1,'y_mm':20,'diameter_mm':0.6,'drill_mm':0.3,'net':'SIGNAL'})
before=a.call('get_session_state',{});reject(route());check('nearby same-net via physical hole overlap rejects all copper',a.call('get_session_state',{})==before);a.call('undo',{})
old=route('J3','J4',p(35_000_000,30_000_000));a.call('update_part',{'id':ids[3],'side':'top'});before=a.call('get_session_state',{});reject(old);check('concurrent side edit invalidates frozen proposal',a.call('get_session_state',{})==before);a.call('undo',{})
times=[]
for _ in range(5):
 args=route('J3','J4',p(35_000_000,30_000_000));args['before']=[p(20_000_000,35_000_000)];args['after']=[p(50_000_000,35_000_000)];t=time.perf_counter();a.call('route_pad_via_polyline',args);times.append((time.perf_counter()-t)*1000);a.call('undo',{})
report['small_board_http_commit_ms']=times;report['small_board_http_mean_ms']=statistics.mean(times)
a.call('route_pad_via_polyline',args|{'expected_cursor':a.call('get_session_state',{})['cursor']})
check('pad-only conservative guide counts remain after both via routes',a.call('query_connectivity',{'net':'SIGNAL'})['guide_count']==1 and a.call('query_connectivity',{'net':'POWER'})['guide_count']==1)
a.call('save_project',{'path':'/tmp/viamind-t0073-route.viamind.json','overwrite':True})
report['passed']=True;Path('/tmp/viamind-t0073-live-report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
