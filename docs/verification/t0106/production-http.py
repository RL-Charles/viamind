from client import Client
from pathlib import Path
import hashlib,json,sys,time,copy
root=Path(__file__).parent
port=int(sys.argv[1]);binary=Path(sys.argv[2]);source=sys.argv[3]
a,b=Client(port),Client(port)
report={'passed':False,'port':port,'source_commit':source,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'checks':[],'latency_ms':{}}
def check(label,ok=True):
 assert ok,label
 report['checks'].append(label)
def state():return a.call('get_session_state',{})
def status():return a.call('get_document_status',{})
def guard(**kw):
 s=state();return {'expected_project':s['project']['id'],'expected_cursor':s['cursor'],**kw}
def reject(tool,args,contains=None):
 old,st=state(),status();r=b.rpc('tools/call',{'name':tool,'arguments':args})
 assert r.get('isError'),r
 if contains: assert contains in json.dumps(r),r
 assert state()==old and status()==st
 return r
def timed(label,tool,args):
 start=time.perf_counter();r=a.call(tool,args);report['latency_ms'][label]=(time.perf_counter()-start)*1000;return r
def track(x1,y1,x2,y2,net='N',layer='top',width=.5):
 return a.call('add_track',{'start_x_mm':x1,'start_y_mm':y1,'end_x_mm':x2,'end_y_mm':y2,'net':net,'layer':layer,'width_mm':width})['created'][0]
def pad(name,x,y,side='top',rotation=0):
 oid=a.call('add_part',{'designator':name,'value':'Terminal fixture','footprint':'explicit','x_mm':x,'y_mm':y,'side':side,'rotation_deg':rotation})['created'][0]
 part=state()['project']['board']['parts'][oid]
 a.call('set_part_pads',{'id':oid,'expected':part,'pads':[{'number':'1','x_mm':0,'y_mm':0,'width_mm':1,'height_mm':1,'net':'N'}]})
 return {'kind':'pad','part_id':oid,'pad_id':state()['project']['board']['parts'][oid]['pads'][0]['id']}
def open_fixture(path):
 a.call('open_project',{'path':str(path),'discard_dirty':True,'expected_cursor':state()['cursor']})
zero={'copper_clearance_mm':0,'edge_clearance_mm':0,'minimum_via_drill_mm':0,'minimum_annular_width_mm':0,'hole_clearance_mm':0,'hole_to_copper_clearance_mm':0,'max_violations':1000}
s=state();a.call('new_project',{'name':'Existing copper terminals HTTP proof','discard_dirty':True,'expected_cursor':s['cursor']})
s=state();a.call('set_board_outline',{'expected':{'project_id':s['project']['id'],'outline':None},'outline':{'outer':[{'x_mm':x,'y_mm':y} for x,y in [(0,0),(50,0),(50,40),(0,40)]],'cutouts':[]}})
tid=track(5.000001,10.000007,10.000003,10.000007)
vid=a.call('add_via',{'x_mm':30.000001,'y_mm':25.000003,'diameter_mm':1,'drill_mm':.3,'net':'N'})['created'][0]
pid=pad('J1',42,32,rotation=90)
unsupported=pad('J2',45,6,rotation=45)
a.call('add_part',{'designator':'LEGACY','value':'No invented pads','footprint':'preview only','x_mm':8,'y_mm':30})
a.call('set_named_net_rules',guard(rules=[{'net':'N','minimum_copper_clearance':0,'minimum_track_width':250000}]))
fixture=root/'terminals.viamind.json';a.call('save_project',{'path':str(fixture),'overwrite':False})
start={'kind':'track_end','track_id':tid,'end':'end'};target={'kind':'via','via_id':vid}
old,st=state(),status();page=a.call('query_route_terminals',{})
check('independent initialized clients discover all three terminal tools',all(n in [t['name'] for t in b.rpc('tools/list',{})['tools']] for n in ['query_route_terminals','route_terminal_polyline','route_terminal_posture']))
check('discovery returns exact track-end/via Nm with both via faces and unsupported pad reason',page['total']==6 and page['unsupported_terminals']==1 and page['omitted_legacy_parts']==1 and any(r['id']==start and r['center_nm']=={'x':10000003,'y':10000007} for r in page['terminals']) and sum(r['id']==target for r in page['terminals'])==2)
check('peer discovery full source cursor agrees and complete session/status stays unchanged',b.call('query_route_terminals',{})==page and page['source_cursor']==old['cursor'] and state()==old and status()==st)
rows=[]
for offset in range(6):
 p=b.call('query_route_terminals',{'offset':offset,'limit':1});rows+=p['terminals'];assert p['next_offset']==(offset+1 if offset<5 else None)
check('stable one-row pagination exactly reconstructs complete terminal page',rows==page['terminals'])
check('exact layer/net filters distinguish via face and net case',b.call('query_route_terminals',{'layer':'bottom','net':'N'})['terminals'][0]['id']==target and b.call('query_route_terminals',{'net':'n'})['total']==0)
check('maximum usize pagination offset saturates to empty final page',b.call('query_route_terminals',{'offset':18446744073709551615})['terminals']==[])
for label,args in [('zero page',{'limit':0}),('oversized page',{'limit':1001}),('oversized raw net',{'net':'x'*257}),('unknown query key',{'extra':True})]:
 reject('query_route_terminals',args);check(label+' refuses unchanged')
def manual(**kw):return guard(start=start,target=target,layer='top',width=250000,grid=250000,interior=[{'x':20000000,'y':20000000}],rules=zero,**kw)
def posture(mode='cardinal',orientation='primary'):
 return guard(start=start,target=target,layer='top',width=250000,grid=250000,waypoints=[],mode=mode,posture=orientation,rules=zero)
for label,changes in [('wrong outer layer',{'layer':'bottom'}),('coincident terminal identity',{'target':start}),('missing terminal',{'target':{'kind':'via','via_id':'11111111-1111-4111-8111-111111111111'}}),('unsupported arbitrary pad',{'start':unsupported}),('one-Nm below document width',{'width':249999}),('offgrid interior',{'interior':[{'x':20000001,'y':20000000}]}),('invalid strict identity',{'target':{**target,'position':{'x':1,'y':1}}}),('oversized raw interior',{'interior':[{'x':0,'y':0}]*1024})]:
 reject('route_terminal_polyline',{**manual(),**changes});check(label+' refuses whole route/session/status')
old=state();request=manual();out=timed('manual_commit','route_terminal_polyline',request);after=state()
new=[after['project']['board']['tracks'][i] for i in out['created']]
check('exact manual attachments and explicit equality width commit in one event',len(new)==2 and new[0]['start']=={'x':10000003,'y':10000007} and new[-1]['end']=={'x':30000001,'y':25000003} and all(t['width']==250000 for t in new) and after['cursor']['sequence']==old['cursor']['sequence']+1)
check('existing terminal track via and pads remain byte-identical',after['project']['board']['tracks'][tid]==old['project']['board']['tracks'][tid] and after['project']['board']['vias']==old['project']['board']['vias'] and after['project']['board']['parts']==old['project']['board']['parts'])
check('independent peer sees exact same committed design',b.call('get_session_state',{})['project']==after['project'])
reject('route_terminal_polyline',request);check('stale full cursor refuses duplicate commit')
a.call('undo',{});check('one undo removes every manual segment and preserves policy/terminal IDs',state()['project']==old['project']);a.call('redo',{});check('one redo restores identical segment IDs',state()['project']==after['project']);a.call('undo',{})
for mode,orientation,expected in [('cardinal','primary',[(10000003,10000007),(10000000,10000000),(30000000,10000000),(30000000,25000000),(30000001,25000003)]),('cardinal','alternate',[(10000003,10000007),(10000000,10000000),(10000000,25000000),(30000000,25000000),(30000001,25000003)]),('octilinear','primary',[(10000003,10000007),(10000000,10000000),(15000000,10000000),(30000000,25000000),(30000001,25000003)]),('octilinear','alternate',[(10000003,10000007),(10000000,10000000),(25000000,25000000),(30000000,25000000),(30000001,25000003)])]:
 old=state();out=timed(mode+'_'+orientation,'route_terminal_posture',posture(mode,orientation));now=state();new=[now['project']['board']['tracks'][i] for i in out['created']]
 pairs=[({'x':x,'y':y},{'x':xx,'y':yy}) for (x,y),(xx,yy) in zip(expected,expected[1:])]
 check(mode+' '+orientation+' independently expected exact bends and offgrid stubs',[(t['start'],t['end']) for t in new]==pairs)
 a.call('undo',{});assert state()['project']==old['project']
# A supported explicit pad can attach to the existing via with no inferred width.
old=state();out=timed('pad_to_existing_via','route_terminal_polyline',{**manual(),'start':pid,'interior':[]});now=state()
check('explicit quarter-turn pad identity reaches existing via using ordinary shared command',len(out['created'])==1 and now['project']['board']['tracks'][out['created'][0]]['start']=={'x':42000000,'y':32000000});a.call('undo',{});assert state()['project']==old['project']
# Two new foreign crossing tracks force complete joint DRC truncation refusal.
track(18,0.5,18,30,'FOREIGN',width=.25);track(22,0.5,22,30,'FOREIGN',width=.25)
reject('route_terminal_polyline',{**manual(),'rules':{**zero,'max_violations':1}},'preflight incomplete');check('truncated joint DRC refuses every proposed segment and event')
a.call('undo',{});a.call('undo',{})
old_request=manual();a.call('set_named_net_rules',guard(rules=[{'net':'N','minimum_copper_clearance':0,'minimum_track_width':250001}]))
reject('route_terminal_polyline',old_request);check('concurrent policy edit invalidates captured route full cursor')
reject('route_terminal_polyline',manual());check('fresh zero-baseline route still cannot weaken new persisted document minimum');a.call('undo',{})
old_request=manual();old_project=state()['project'];open_fixture(fixture)
check('same-project reopen preserves terminal identities with a new stream',state()['project']==old_project and state()['cursor']['stream_id']!=old_request['expected_cursor']['stream_id'])
reject('route_terminal_polyline',old_request);check('same-UUID reopen rejects stale initial stream unchanged')
check('final native fixture and peer observer match preserved terminal source',state()['project']==json.loads(fixture.read_text())['project'] and b.call('get_session_state',{})==state())
report.update(passed=True,assertion_count=len(report['checks']),fixture=str(fixture),final_state=state(),final_status=status())
(root/'production-http.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='final_state'},indent=2))
