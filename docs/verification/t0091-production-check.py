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
report={'passed':False,'source_commit':sys.argv[3],'port':port,'binary_sha256':hashlib.sha256(Path(sys.argv[2]).read_bytes()).hexdigest(),'checks':[]}
def check(label,ok):
 assert ok,label
 report['checks'].append(label)
def reject(tool,q):
 old=a.call('get_session_state',{});r=b.rpc('tools/call',{'name':tool,'arguments':q});assert r.get('isError'),r
 assert a.call('get_session_state',{})==old
 return r
def request(selector=None):
 s=a.call('get_session_state',{})
 return {'expected_project':s['project']['id'],'expected_cursor':s['cursor'],'tracks':selector or {'source':'net','net':'N','layer':None}}
mm=lambda x,y:{'x_mm':x,'y_mm':y}
initial=a.call('get_session_state',{});assert not initial['project']['board']['parts'] and not initial['project']['board'].get('tracks'),'requires unused session'
a.call('set_board_outline',{'expected':{'project_id':initial['project']['id'],'outline':None},'outline':{'outer':[mm(-30,-20),mm(60,-20),mm(60,40),mm(-30,40)],'cutouts':[]}})
ids=[]
def track(x,y,xx,yy,layer='top',net='N',width=.250001):
 i=a.call('add_track',{'start_x_mm':x,'start_y_mm':y,'end_x_mm':xx,'end_y_mm':yy,'width_mm':width,'layer':layer,'net':net})['created'][0];ids.append(i);return i
for x,xx in [(-15,0),(0,15)]:track(x,0,xx,0)
for y,yy in [(-10,0),(0,10)]:track(30,y,30,yy,'bottom')
for x,xx in [(-15,0),(0,15)]:track(x,15,xx,15)
track(0,15,0,25)
track(45,20,50,25,'bottom')
for x,xx in [(-15,0),(0,15)]:track(x,35,xx,35,'bottom')
a.call('add_via',{'x_mm':5,'y_mm':35.150001,'diameter_mm':.600003,'drill_mm':.300001,'net':None})
for name,x,y in [('J1',-15,0),('J2',15,0),('J3',20,0)]:
 oid=a.call('add_part',{'designator':name,'value':'explicit pad','footprint':'explicit','x_mm':x,'y_mm':y})['created'][0]
 part=a.call('get_design',{})['board']['parts'][oid]
 a.call('set_part_pads',{'id':oid,'expected':part,'pads':[{'number':'1','x_mm':0,'y_mm':0,'width_mm':1,'height_mm':1,'net':'N'}]})
a.call('add_part',{'designator':'R1','value':'legacy preview coverage exclusion','footprint':'R','x_mm':45,'y_mm':30})
tools={t['name'] for t in a.rpc('tools/list',{})['tools']}
check('both cleanup query and command discovered by actual production HTTP',{'plan_cardinal_track_cleanup','cleanup_cardinal_tracks'}<=tools)
q=request();before=a.call('get_session_state',{});guides=a.call('query_connectivity',{'net':'N'})['guides'];t=time.perf_counter();plan=a.call('plan_cardinal_track_cleanup',q);report['first_http_plan_ms']=(time.perf_counter()-t)*1000
check('immutable checked preview preserves complete session snapshot',a.call('get_session_state',{})==before)
check('checked plan retains captured project and full event cursor',plan['project_id']==q['expected_project'] and plan['source_cursor']==q['expected_cursor'])
check('exact supported counters distinguish merged branch diagonal and all-net hole chains',plan['counts']['selected_tracks']==10 and plan['counts']['merged_chains']==2 and plan['counts']['deleted_tracks']==2 and plan['counts']['unsupported_diagonal_tracks']==1 and plan['counts']['retained_branch_joints']>=1 and plan['counts']['retained_hole_chains']==1)
check('actual scoped coverage discloses legacy exclusion and unperformed checks',plan['complete_scoped_preflight'] and plan['coverage']['excluded_part_count']==1 and plan['coverage']['checks_not_performed'])
expected=[({'x':-15000000,'y':0},{'x':15000000,'y':0},'top',ids[:2]),({'x':30000000,'y':-10000000},{'x':30000000,'y':10000000},'bottom',ids[2:4])]
check('independently expected exact cardinal union and smallest surviving IDs',all(any(r['after']['start']==p and r['after']['end']==e and r['after']['layer']==layer and r['after']['width']==250001 and r['after']['id']==min(group) and {t['id'] for t in r['before']}==set(group) for r in plan['replacements']) for p,e,layer,group in expected))
for layer,count in [('top',1),('bottom',1)]:
 r=a.call('plan_cardinal_track_cleanup',request({'source':'net','net':'N','layer':layer}));check(layer+' explicit net layer filter independently checked',r['counts']['merged_chains']==count)
r=a.call('plan_cardinal_track_cleanup',request({'source':'ids','ids':ids[:2]}));check('explicit stored IDs select only chosen chain',r['counts']['selected_tracks']==2 and r['counts']['deleted_tracks']==1)
check('all selector queries preserve full session',a.call('get_session_state',{})==before)
t=time.perf_counter();out=a.call('cleanup_cardinal_tracks',q);report['first_http_commit_ms']=(time.perf_counter()-t)*1000;after=a.call('get_session_state',{})
check('one atomic event replaces and deletes both chains',len(after['project']['board']['tracks'])==8 and len(out['deltas'])==4 and after['cursor']['sequence']==before['cursor']['sequence']+1)
check('stored-endpoint guide IDs and endpoints remain unchanged',a.call('query_connectivity',{'net':'N'})['guides']==guides)
check('unsupported branch diagonal and hole-contact tracks unchanged',all(after['project']['board']['tracks'][i]==before['project']['board']['tracks'][i] for i in ids[4:]))
for tool in ['plan_cardinal_track_cleanup','cleanup_cardinal_tracks']:
 reject(tool,q);check('peer stale '+tool+' refuses without partial changes',True)
a.call('undo',{});check('one undo restores exact original IDs and geometry',a.call('get_design',{})==before['project']);a.call('redo',{});check('redo reuses deterministic surviving IDs and exact model',a.call('get_design',{})==after['project']);a.call('undo',{})
for label,selector in [('duplicate IDs',{'source':'ids','ids':[ids[0],ids[0]]}),('no IDs',{'source':'ids','ids':[]}),('oversized exact net',{'source':'net','net':'N'*513,'layer':None})]:
 for tool in ['plan_cardinal_track_cleanup','cleanup_cardinal_tracks']:reject(tool,request(selector))
 check(label+' refuse both tools atomically',True)
old=request();b.call('add_part',{'designator':'PEER','value':'peer mutation','footprint':'R','x_mm':50,'y_mm':-10})
for tool in ['plan_cardinal_track_cleanup','cleanup_cardinal_tracks']:reject(tool,old)
check('real peer edit invalidates captured plan and command',True);b.call('undo',{})
times=[]
for _ in range(5):
 q=request();t=time.perf_counter();a.call('cleanup_cardinal_tracks',q);times.append((time.perf_counter()-t)*1000);a.call('undo',{})
report['warm_http_commit_ms']=times;report['warm_http_mean_ms']=statistics.mean(times)
fixture='/home/charlesmowbray/.cache/viamind/native-artifacts/t0091-cleanup.viamind.json';a.call('save_project',{'path':fixture,'overwrite':True});old=request();a.call('open_project',{'path':fixture,'discard_dirty':False,'expected_cursor':old['expected_cursor']});now=a.call('get_session_state',{})
for tool in ['plan_cardinal_track_cleanup','cleanup_cardinal_tracks']:reject(tool,old)
check('same-project reopen changes stream and rejects old requests',now['cursor']['stream_id']!=old['expected_cursor']['stream_id'])
q=request();a.call('cleanup_cardinal_tracks',q);no_op=request();previous=a.call('get_session_state',{});r=a.call('plan_cardinal_track_cleanup',no_op);a.call('cleanup_cardinal_tracks',no_op)
check('already-clean supported copper is a checked no-op without event',r['counts']['deleted_tracks']==0 and a.call('get_session_state',{})==previous)
a.call('undo',{});report['passed']=True;report['fixture']=fixture
Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0091-native-http.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
