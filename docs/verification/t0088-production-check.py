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
report={'passed':False,'source_commit':'85a33c04b991a8c4cc7dcb0f8c5bca7ddc09f6e6','port':port,'binary_sha256':hashlib.sha256(Path(sys.argv[2]).read_bytes()).hexdigest(),'checks':[]}
p=lambda x,y:{'x':x,'y':y};mm=lambda x,y:{'x_mm':x,'y_mm':y}
def check(label,ok):
 assert ok,label
 report['checks'].append(label)
def reject(tool,args):
 previous=a.call('get_session_state',{});out=b.rpc('tools/call',{'name':tool,'arguments':args});assert out.get('isError'),out
 assert a.call('get_session_state',{})==previous
 return out
def rect(x,y,xx,yy):return [mm(x,y),mm(xx,y),mm(xx,yy),mm(x,yy)]
def outline(cuts):
 state=a.call('get_session_state',{});return b.call('set_board_outline',{'expected':{'project_id':state['project']['id'],'outline':state['project']['board'].get('outline')},'outline':{'outer':rect(0,0,60,30),'cutouts':cuts}})
initial=a.call('get_session_state',{});assert not initial['project']['board']['parts'] and not initial['project']['board'].get('tracks') and not initial['project']['board'].get('vias'),'needs an unused production session'
outline([rect(34,20,36,22)])
for name,x,side in [('J1',10,'top'),('J2',50,'bottom')]:
 oid=a.call('add_part',{'designator':name,'value':'Original','footprint':'explicit','x_mm':x,'y_mm':10,'side':side,'rotation_deg':0})['created'][0]
 part=a.call('get_design',{})['board']['parts'][oid]
 a.call('set_part_pads',{'id':oid,'expected':part,'pads':[{'number':'1','x_mm':0.000123,'y_mm':0.000129,'width_mm':1.000001,'height_mm':1.000001,'net':'N'}]})
for x,layer in [(20,'top'),(40,'bottom')]:a.call('add_track',{'start_x_mm':x,'start_y_mm':8,'end_x_mm':x,'end_y_mm':12,'width_mm':0.25,'layer':layer,'net':'FOREIGN'})
def args():
 page=a.call('query_route_endpoints',{});rows={r['designator']:r for r in page['endpoints']}
 return {'expected_project':page['project_id'],'expected_cursor':page['source_cursor'],'start':rows['J1']['id'],'target':rows['J2']['id'],'via':p(30000000,10000000),'grid':1000000,'width':250001,'via_diameter':600000,'via_drill':300000}
tools={t['name'] for t in a.rpc('tools/list',{})['tools']};check('actual production fixed-via query and commit tools discovered',{'plan_pad_via_walkaround','route_pad_via_walkaround'}<=tools)
first=[p(10000123,10000129),p(10000000,10000000),p(10000000,7000000),p(21000000,7000000),p(21000000,10000000),p(30000000,10000000)]
second=[p(30000000,10000000),p(39000000,10000000),p(39000000,7000000),p(41000000,7000000),p(41000000,10000000),p(50000000,10000000),p(49999877,10000129)]
request=args();before=a.call('get_session_state',{});t=time.perf_counter();plan=a.call('plan_pad_via_walkaround',request);report['first_http_plan_ms']=(time.perf_counter()-t)*1000
check('complete session unchanged by combined checked query',a.call('get_session_state',{})==before)
check('independently expected exact top and bottom detours with offgrid endpoints',plan['before']['path']['points']==first and plan['after']['path']['points']==second)
check('same captured full cursor derived net and opposite layers',plan['source_cursor']==before['cursor'] and plan['net']=='N' and plan['start_layer']=='top' and plan['target_layer']=='bottom')
check('both sides charge substrate hole and layer obstacles in one budget',plan['work']['obstacle_boxes']==4 and plan['before']['work']['obstacle_boxes']==2 and plan['after']['work']['obstacle_boxes']==2)
check('joint complete preflight and fixed via dimensions reported',plan['complete_scoped_preflight'] and plan['via']==request['via'] and plan['via_diameter']==600000 and plan['via_drill']==300000)
check('combined work equals both legs',all(plan['work'][key]==plan['before']['work'][key]+plan['after']['work'][key] for key in plan['work']))
reverse=dict(request,start=request['target'],target=request['start']);rev=a.call('plan_pad_via_walkaround',reverse)
check('reverse bottom-to-top query derives both sides and exact attachments',rev['start_layer']=='bottom' and rev['target_layer']=='top' and rev['before']['path']['points'][0]==second[-1] and rev['before']['path']['points'][-1]==request['via'] and rev['after']['path']['points'][0]==request['via'] and rev['after']['path']['points'][-1]==first[0])
check('reverse query leaves complete session unchanged',a.call('get_session_state',{})==before)
t=time.perf_counter();out=a.call('route_pad_via_walkaround',request);report['first_http_commit_ms']=(time.perf_counter()-t)*1000;after=a.call('get_session_state',{})
ids=out['created'];tracks=[after['project']['board']['tracks'][i] for i in ids[:-1]];pairs=list(zip(first,first[1:]))+list(zip(second,second[1:]))
check('joint recompute commits checked eleven exact segments and one via',len(ids)==12 and all(t['start']==pair[0] and t['end']==pair[1] and t['layer']==('top' if i<5 else 'bottom') and t['net']=='N' for i,(t,pair) in enumerate(zip(tracks,pairs))))
v=after['project']['board']['vias'][ids[-1]];check('exact fixed new via assigned derived net',v['position']==request['via'] and v['diameter']==600000 and v['drill']==300000 and v['net']=='N')
check('both routed legs and via consume one event',after['cursor']['sequence']==before['cursor']['sequence']+1)
for tool in ['plan_pad_via_walkaround','route_pad_via_walkaround']:reject(tool,request);check('second client stale '+tool+' rejects atomically',a.call('get_session_state',{})==after)
a.call('undo',{});check('one creator undo removes both legs and via',a.call('get_design',{})==before['project']);a.call('redo',{});check('redo preserves all track and via IDs',a.call('get_design',{})==after['project']);a.call('undo',{})
for label,changes in [('offgrid fixed via',{'via':p(30000001,10000000)}),('joint via edge clearance',{'via':p(1000000,10000000),'via_diameter':3000000}),('joint annular width',{'via_diameter':400000}),('zero grid',{'grid':0}),('identical endpoints',{'target':args()['start']})]:
 q=dict(args(),**changes)
 for tool in ['plan_pad_via_walkaround','route_pad_via_walkaround']:reject(tool,q)
 check(label+' both tools refuse unchanged',True)
b.call('add_via',{'x_mm':30,'y_mm':10.5,'diameter_mm':0.6,'drill_mm':0.3,'net':'N'})
for tool in ['plan_pad_via_walkaround','route_pad_via_walkaround']:
 error=reject(tool,args());assert 'hole' in json.dumps(error),error
check('joint-only nearby same-net via hole spacing rejects both tools',True);b.call('undo',{})
old=args();outline([rect(34,18,36,22)])
for tool in ['plan_pad_via_walkaround','route_pad_via_walkaround']:reject(tool,old)
check('actual peer outline edit invalidates both captured guards',True)
for count in [32,33]:
 # Existing two foreign tracks contribute one box per leg; use31/32 cutouts
 # to exercise aggregate64 success then66 refusal through actual MCP.
 cuts=[rect(1+i,25,1.1+i,25.1) for i in range(count-1)];outline(cuts)
 if count==32:check('aggregate64 charged boxes complete at boundary',a.call('plan_pad_via_walkaround',args())['work']['obstacle_boxes']==64)
 else:
  for tool in ['plan_pad_via_walkaround','route_pad_via_walkaround']:reject(tool,args())
  check('aggregate66 boxes refuse entire checked query and commit',True)
outline([rect(34,20,36,22)]);times=[]
for _ in range(5):
 q=args();t=time.perf_counter();a.call('route_pad_via_walkaround',q);times.append((time.perf_counter()-t)*1000);a.call('undo',{})
report['warm_http_commit_ms']=times;report['warm_http_mean_ms']=statistics.mean(times)
a.call('route_pad_via_walkaround',args());before=a.call('get_session_state',{});a.call('query_connectivity',{'net':'N'});check('separate stored-endpoint connectivity query stays immutable',a.call('get_session_state',{})==before)
path='/home/charlesmowbray/.cache/viamind/native-artifacts/t0088-route.viamind.json';a.call('save_project',{'path':path,'overwrite':True});old=args();a.call('open_project',{'path':path,'discard_dirty':False,'expected_cursor':old['expected_cursor']});before=a.call('get_session_state',{})
for tool in ['plan_pad_via_walkaround','route_pad_via_walkaround']:reject(tool,old)
check('same-project reopen changes stream and refuses both old requests',a.call('get_session_state',{})==before and before['cursor']['stream_id']!=old['expected_cursor']['stream_id'])
report['passed']=True;Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0088-native-http.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
