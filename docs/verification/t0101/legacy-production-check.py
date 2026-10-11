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
root=Path(__file__).parent
port=int(sys.argv[1]);a,b=Client(port),Client(port)
report={'passed':False,'source_commit':sys.argv[3],'port':port,'binary_sha256':hashlib.sha256(Path(sys.argv[2]).read_bytes()).hexdigest(),'checks':[]}
def check(label,ok):
 assert ok,label
 report['checks'].append(label)
def reject(tool,q):
 old=a.call('get_session_state',{});r=b.rpc('tools/call',{'name':tool,'arguments':q});assert r.get('isError'),r
 assert a.call('get_session_state',{})==old
 return r
def guarded(**kwargs):
 s=a.call('get_session_state',{});return {'expected_project':s['project']['id'],'expected_cursor':s['cursor'],**kwargs}
def replace(rows,client=a):return client.call('set_named_net_rules',guarded(rules=rows))
def rule(net,c,w):return {'net':net,'minimum_copper_clearance':c,'minimum_track_width':w}
def track(x,y,xx,yy,net,width=.200000,layer='top'):
 return a.call('add_track',{'start_x_mm':x,'start_y_mm':y,'end_x_mm':xx,'end_y_mm':yy,'width_mm':width,'layer':layer,'net':net})['created'][0]
def pad(name,x,y,side='top'):
 oid=a.call('add_part',{'designator':name,'value':'explicit pad','footprint':'explicit','x_mm':x,'y_mm':y,'side':side})['created'][0]
 part=a.call('get_design',{})['board']['parts'][oid]
 a.call('set_part_pads',{'id':oid,'expected':part,'pads':[{'number':'1','x_mm':0,'y_mm':0,'width_mm':1.000001,'height_mm':1.000001,'net':'N'}]})
 obj=a.call('get_design',{})['board']['parts'][oid]
 return {'part_id':oid,'pad_id':obj['pads'][0]['id']}
def drc(baseline=.2):return a.call('run_drc',{'copper_clearance_mm':baseline})['report']
initial=a.call('get_session_state',{});assert not initial['project']['board']['parts'] and not initial['project']['board'].get('tracks'),'requires unused production session'
tools={t['name'] for t in a.rpc('tools/list',{})['tools']}
check('production exposes immutable query and guarded undoable policy command',{'query_named_net_rules','set_named_net_rules'}<=tools)
q=a.call('query_named_net_rules',{});check('empty query preserves exact complete session and carries full cursor',q['document_net_rules']=={} and q['source_cursor']==initial['cursor'] and a.call('get_session_state',{})==initial)
reject('query_named_net_rules',{'unexpected':1});check('query refuses unknown input without mutation',True)
a.call('set_board_outline',{'expected':{'project_id':initial['project']['id'],'outline':None},'outline':{'outer':[{'x_mm':x,'y_mm':y} for x,y in [(-20,-20),(80,-20),(80,40),(-20,40)]],'cutouts':[]}})
id_a=track(0,0,1,0,'A');id_b=track(0,1,1,1,'B')
empty_report=drc();check('legacy empty report omits document policy provenance',not empty_report['violations'] and 'document_net_rules' not in empty_report)
unused=[rule('UNUSED',10_000_000,1)]
replace(unused);check('large unrelated named minimum only broadens search and does not change pair requirement',not drc()['violations'])
rows=unused+[rule('A',900_001,300_001),rule('B',700_001,200_000)]
before=a.call('get_session_state',{});request=guarded(rules=rows);t=time.perf_counter();out=a.call('set_named_net_rules',request);report['first_policy_http_ms']=(time.perf_counter()-t)*1000
policy=a.call('get_session_state',{});check('complete policy replacement is one atomic design event',len(out['deltas'])==1 and policy['cursor']['sequence']==before['cursor']['sequence']+1)
reject('set_named_net_rules',request);check('peer stale full cursor rejects all policy rows atomically',True)
q=a.call('query_named_net_rules',{});check('peer reads exact map and matching full source cursor',b.call('query_named_net_rules',{})==q and q['source_cursor']==policy['cursor'])
replace(rows);check('identical policy is a complete-session no-op',a.call('get_session_state',{})==policy)
r=drc(0);clearance=[v for v in r['violations'] if v['kind']=='copper_clearance'];width=[v for v in r['violations'] if v['kind']=='minimum_track_width']
check('far foreign pair beyond request baseline is found with exact pair max, not unrelated global max',len(clearance)==1 and clearance[0]['measured_clearance']==800000 and clearance[0]['required_clearance']==900001)
check('track width issue reports actual stored ID and odd-Nm enforced floor',len(width)==1 and width[0]['objects'][0]['track_id']==id_a and width[0]['measured_clearance']==200000 and width[0]['required_clearance']==300001)
check('DRC report distinguishes unchanged request baseline and captured document sources',r['rules']['copper_clearance']==0 and r['document_net_rules']==q['document_net_rules'])
a.call('undo',{});check('one undo restores exact previous policy and geometry',a.call('get_design',{})==before['project']);a.call('redo',{});check('redo restores exact odd-Nm policy without widening stored tracks',a.call('get_design',{})==policy['project'])
route=guarded(points=[{'x':10_000_000,'y':-10_000_000},{'x':20_000_000,'y':-10_000_000}],grid=1_000_000,width=300000,layer='top',net='A')
reject('route_polyline',route);check('free route one Nm below persisted width refuses without geometry or event',True)
route['width']=300001;t=time.perf_counter();a.call('route_polyline',route);report['equal_width_route_http_ms']=(time.perf_counter()-t)*1000
check('free route equality preserves the explicit exact requested width',any(t['width']==300001 and t['net']=='A' for t in a.call('get_design',{})['board']['tracks'].values()))
a.call('undo',{})
for label,bad in [('duplicate exact names',[rule('N',1,1),rule('N',2,2)]),('too many rows',[rule(f'N{i}',1,1) for i in range(129)]),('oversized name',[rule('x'*257,1,1)]),('zero width',[rule('N',1,0)]),('out of range copper',[rule('N',100000001,1)])]:
 reject('set_named_net_rules',guarded(rules=bad));check(label+' refuse entire request',True)
ends=[pad('J1',20,10),pad('J2',50,10)];other=pad('J3',50,25,'bottom');track(35,8,35,12,'FOREIGN',.250001)
walk=guarded(start=ends[0],target=ends[1],grid=1_000_000,width=300001)
baseline=a.call('plan_pad_walkaround',walk)
rows+= [rule('N',1100001,300001),rule('FOREIGN',1500000,1)];replace(rows)
walk=guarded(start=ends[0],target=ends[1],grid=1_000_000,width=300001);old=a.call('get_session_state',{});plan=a.call('plan_pad_walkaround',walk)
check('walkaround query is immutable and retains exact document policy provenance',a.call('get_session_state',{})==old and plan['document_net_rules']==old['project']['board']['net_rules'])
expected=[{'x':20000000,'y':10000000},{'x':20000000,'y':6000000},{'x':37000000,'y':6000000},{'x':37000000,'y':10000000},{'x':50000000,'y':10000000}]
check('foreign-net larger floor expands integer obstacle boxes to independently expected detour',plan['plan']['path']['points']==expected and baseline['plan']['path']['points']!=expected)
reject('plan_pad_walkaround',{**walk,'width':300000});reject('route_pad_walkaround',{**walk,'width':300000});check('checked same-side plan and command both refuse below-minimum width',True)
t=time.perf_counter();a.call('route_pad_walkaround',walk);report['named_walkaround_commit_http_ms']=(time.perf_counter()-t)*1000
after=a.call('get_session_state',{});check('checked detour commit is one event with all explicit compliant widths',after['cursor']['sequence']==old['cursor']['sequence']+1 and all(t['width']>=300001 for t in after['project']['board']['tracks'].values() if t['net']=='N'))
a.call('undo',{});check('one undo restores exact pre-route board and policy',a.call('get_design',{})==old['project'])
via=guarded(start=ends[0],target=other,via={'x':35000000,'y':25000000},grid=1000000,width=300001,via_diameter=700001,via_drill=300001)
old=a.call('get_session_state',{});joint=a.call('plan_pad_via_walkaround',via)
check('two-side checked planner retains same captured policy and complete joint preflight',joint['complete_scoped_preflight'] and joint['document_net_rules']==old['project']['board']['net_rules'] and a.call('get_session_state',{})==old)
reject('plan_pad_via_walkaround',{**via,'width':300000});reject('route_pad_via_walkaround',{**via,'width':300000});check('both joint-via APIs refuse below-minimum legs atomically',True)
a.call('route_pad_via_walkaround',via);a.call('undo',{});check('two-side joint commit undoes all tracks and via with exact policy preserved',a.call('get_design',{})==old['project'])
low=[track(10,30,20,30,'N',.25),track(20,30,30,30,'N',.25)]
cleanup=guarded(tracks={'source':'ids','ids':low})
reject('plan_cardinal_track_cleanup',cleanup);reject('cleanup_cardinal_tracks',cleanup);check('cleanup joint checker rejects undersized survivor without deleting originals',True)
high=[track(10,34,20,34,'N',.300001),track(20,34,30,34,'N',.300001)]
cleanup=guarded(tracks={'source':'ids','ids':high});old=a.call('get_session_state',{});clean=a.call('plan_cardinal_track_cleanup',cleanup)
check('compliant cleanup retains captured minima and complete scoped result',clean['document_net_rules']==old['project']['board']['net_rules'] and clean['counts']['deleted_tracks']==1)
a.call('cleanup_cardinal_tracks',cleanup);a.call('undo',{});check('cleanup one undo preserves exact original IDs and document floors',a.call('get_design',{})==old['project'])
fixture=root/'rules.viamind.json';a.call('save_project',{'path':str(fixture),'overwrite':True});native=json.loads(fixture.read_text());check('nonempty native policy saves schema8 exact odd-Nm minima',native['version']==8 and native['project']['board']['net_rules']==old['project']['board']['net_rules'])
old_request=guarded(rules=rows);a.call('open_project',{'path':str(fixture),'discard_dirty':False,'expected_cursor':old_request['expected_cursor']});reopened=a.call('get_session_state',{})
reject('set_named_net_rules',old_request);check('same UUID reopen changes stream and refuses old policy edit',reopened['project']['id']==old_request['expected_project'] and reopened['cursor']['stream_id']!=old_request['expected_cursor']['stream_id'])
replace([]);clear=root/'cleared.viamind.json';a.call('save_project',{'path':str(clear),'overwrite':True});native=json.loads(clear.read_text());check('clear restores pre-policy feature schema and omitted policy bytes',native['version']==4 and 'net_rules' not in native['project']['board'])
a.call('undo',{})
# A separate authored supported fabrication board verifies the merged checked-job path.
repo=Path(sys.argv[4]);checked_native=json.loads((repo/'fixtures/fabrication/asymmetric.viamind.json').read_text())
checked_native['version']=8;checked_native['project']['board']['net_rules']={'GND':{'minimum_copper_clearance':0,'minimum_track_width':250000}}
checked_fixture=root/'checked-rules.viamind.json';checked_fixture.write_text(json.dumps(checked_native,indent=2)+'\n')
state=a.call('get_session_state',{});a.call('open_project',{'path':str(checked_fixture),'discard_dirty':True,'expected_cursor':state['cursor']})
zero={'copper_clearance_mm':0,'edge_clearance_mm':0,'minimum_via_drill_mm':0,'minimum_annular_width_mm':0,'hole_clearance_mm':0,'hole_to_copper_clearance_mm':0,'max_violations':1000}
job={'format':'viamind-output-job','version':1,'name':'Captured document floors','profile':'checked_outer_2layer','mask_expansion_mm':0,'checks':zero}
job_path=root/'checked-job.json';a.call('save_output_job',{'path':str(job_path),'job':job})
old=a.call('get_session_state',{});release=root/'checked-release';t=time.perf_counter();released=a.call('run_output_job',{'job_path':str(job_path),'path':str(release),'expected_cursor':old['cursor']});report['checked_job_http_ms']=(time.perf_counter()-t)*1000
validation=json.loads((release/'validation/drc.json').read_text())
check('actual checked release uses exact width equality and complete unchanged captured board',released['file_count']==12 and a.call('get_session_state',{})==old and validation['exported_cursor']==old['cursor'] and not validation['report']['violations'])
check('checked validation distinguishes zero saved request baselines and exact document floors',validation['requested_rules']['copper_clearance_mm']==0 and validation['report']['rules']['copper_clearance']==0 and validation['report']['document_net_rules']==old['project']['board']['net_rules'])
replace([rule('GND',0,250001)]);refused=root/'checked-undersize-refused'
reject('run_output_job',{'job_path':str(job_path),'path':str(refused),'expected_cursor':a.call('get_session_state',{})['cursor']})
check('zero saved baselines cannot bypass one-Nm track-width floor and publish no release',not refused.exists())
a.call('undo',{});check('one policy undo restores exact checked source geometry and floors',a.call('get_design',{})==old['project'])
left=track(5,4,6,4,'GND',.25);right=track(5,5.05,6,5.05,'FOREIGN',.25)
replace([rule('GND',800001,250000)])
r=drc(0);issues=[v for v in r['violations'] if v['kind']=='copper_clearance' and {o.get('track_id') for o in v['objects']}=={left,right}]
check('actual zero-baseline checker finds checked-board far pair at800000Nm with800001Nm document floor',len(issues)==1 and issues[0]['measured_clearance']==800000 and issues[0]['required_clearance']==800001)
refused=root/'checked-clearance-refused';reject('run_output_job',{'job_path':str(job_path),'path':str(refused),'expected_cursor':a.call('get_session_state',{})['cursor']})
check('actual saved zero copper baseline cannot weaken document clearance or publish a release',not refused.exists())
check('checked job refusals leave no hidden staging directory',not list(root.glob('.viamind-output-job-*')))
report['checked_fixture']=str(checked_fixture);report['checked_release']=str(release);report['passed']=True;report['fixture']=str(fixture);report['assertion_count']=len(report['checks']);(root/'production-http.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
