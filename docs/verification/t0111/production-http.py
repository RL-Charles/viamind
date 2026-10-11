"""Owned production native HTTP fixture; two initialized independent clients."""
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
def reject(tool,args,contains=None):
 old,st=state(),status();r=b.rpc('tools/call',{'name':tool,'arguments':args})
 assert r.get('isError'),r
 if contains:assert contains in json.dumps(r),r
 assert state()==old and status()==st
 return r
def timed(label,tool,args):
 start=time.perf_counter();r=a.call(tool,args);report['latency_ms'][label]=(time.perf_counter()-start)*1000;return r
def guard(**kw):
 s=state();return {'expected_project':s['project']['id'],'expected_cursor':s['cursor'],**kw}
zero={'copper_clearance_mm':0,'edge_clearance_mm':0,'minimum_via_drill_mm':0,'minimum_annular_width_mm':0,'hole_clearance_mm':0,'hole_to_copper_clearance_mm':0,'max_violations':1000}
s=state();a.call('new_project',{'name':'Existing copper properties HTTP proof','discard_dirty':True,'expected_cursor':s['cursor']})
s=state();a.call('set_board_outline',{'expected':{'project_id':s['project']['id'],'outline':None},'outline':{'outer':[{'x_mm':x,'y_mm':y}for x,y in [(0,0),(50,0),(50,50),(0,50)]],'cutouts':[]}})
tid=a.call('add_track',{'start_x_mm':5.000003,'start_y_mm':10.000007,'end_x_mm':15.000001,'end_y_mm':10.000007,'width_mm':.25,'layer':'top','net':'N'})['created'][0]
vid=a.call('add_via',{'x_mm':30.000001,'y_mm':25.000003,'diameter_mm':1,'drill_mm':.3,'net':'N'})['created'][0]
for designator,x in [('J1',5.000003),('J2',15.000001)]:
 oid=a.call('add_part',{'designator':designator,'value':'Exact pad anchor','footprint':'explicit','x_mm':x,'y_mm':10.000007})['created'][0]
 a.call('set_part_pads',{'id':oid,'expected':state()['project']['board']['parts'][oid],'pads':[{'number':'1','x_mm':0,'y_mm':0,'width_mm':1,'height_mm':1,'net':'N'}]})
a.call('set_named_net_rules',guard(rules=[{'net':'N','minimum_copper_clearance':100001,'minimum_track_width':250001,'minimum_via_drill':300001,'minimum_annular_width':150001}]))
fixture=root/'properties.viamind.json';a.call('save_project',{'path':str(fixture),'overwrite':False})
def query(kind,id):return {'target':{'kind':kind,'id':id}}
def track(**kw):return guard(id=tid,expected=state()['project']['board']['tracks'][tid],width=350001,layer='bottom',net='N',rules=zero,**kw)
def via(**kw):return guard(id=vid,expected=state()['project']['board']['vias'][vid],diameter=900003,drill=400001,net='N',rules=zero,**kw)
check('two initialized clients discover property query and both guarded commands',all(n in [t['name']for t in b.rpc('tools/list',{})['tools']]for n in ['query_copper_properties','set_track_properties','set_via_properties']))
for tool,request in [('set_track_properties',track()),('set_via_properties',via())]:
 schema=next(t['inputSchema']for t in b.rpc('tools/list',{})['tools']if t['name']==tool)
 check(tool+' advertises explicit required nullable net at both levels','net'in schema['required']and schema['properties']['net']['type']==['string','null']and 'net'in schema['properties']['expected']['required'])
 for original in [False,True]:
  missing=copy.deepcopy(request)
  if original:del missing['expected']['net']
  else:del missing['net']
  reject(tool,missing);check(tool+(' missing original net'if original else' missing replacement net')+' refuses unchanged')
old,st=state(),status()
for kind,id,key in [('track',tid,'tracks'),('via',vid,'vias')]:
 observed=a.call('query_copper_properties',query(kind,id));peer=b.call('query_copper_properties',query(kind,id))
 check(kind+' immutable peer query returns exact original, full cursor and document policy',observed==peer and observed['object'][kind]==old['project']['board'][key][id] and observed['source_cursor']==old['cursor'] and observed['document_net_rules']==old['project']['board']['net_rules'] and state()==old and status()==st)
for label,args in [('unknown query key',{'target':{'kind':'track','id':tid,'width':1}}),('unsupported pad identity',{'target':{'kind':'pad','id':tid}}),('missing identity',query('via','11111111-1111-4111-8111-111111111111'))]:reject('query_copper_properties',args);check(label+' refuses unchanged')
before_guides=a.call('query_connectivity',{'net':'N'});check('exact original top track proves pad endpoint chain with zero guides',before_guides['guide_count']==0)
request=track();old=state();out=timed('track_commit','set_track_properties',request);now=state();t=now['project']['board']['tracks'][tid]
check('track exact odd-Nm width and layer commit retain ID and endpoints',t['width']==350001 and t['layer']=='bottom' and t['id']==tid and t['start']==old['project']['board']['tracks'][tid]['start'] and t['end']==old['project']['board']['tracks'][tid]['end'] and not out['created'])
check('changing explicit copper side restores conservative guide without changing pad identities',a.call('query_connectivity',{'net':'N'})['guide_count']==1 and now['project']['board']['parts']==old['project']['board']['parts'])
check('one track design event and exact delta before/after',now['cursor']['sequence']==old['cursor']['sequence']+1 and len(out['deltas'])==1 and out['deltas'][0]['before']==old['project']['board']['tracks'][tid] and out['deltas'][0]['after']==t)
check('peer reads the same committed track and all unrelated geometry unchanged',b.call('get_design',{})==now['project'] and now['project']['board']['vias']==old['project']['board']['vias'] and now['project']['board']['net_rules']==old['project']['board']['net_rules'])
reject('set_track_properties',request);check('stale full sequence refuses duplicate property commit')
a.call('undo',{});check('one undo restores exact pre-track project',state()['project']==old['project']);check('undo restores exact conservative stored-endpoint proof',a.call('query_connectivity',{'net':'N'})==before_guides);a.call('redo',{});check('one redo restores exact track and stable UUID',state()['project']==now['project'])
request=via();old=state();out=timed('via_commit','set_via_properties',request);now=state();v=now['project']['board']['vias'][vid]
check('through via odd diameter/drill retain exact center and identity',v['diameter']==900003 and v['drill']==400001 and v['id']==vid and v['position']==old['project']['board']['vias'][vid]['position'] and not out['created'])
check('one via event and same peer committed project',now['cursor']['sequence']==old['cursor']['sequence']+1 and b.call('get_design',{})==now['project'])
a.call('undo',{});check('one undo restores exact pre-via project',state()['project']==old['project']);a.call('redo',{});check('one redo restores exact via and UUID',state()['project']==now['project'])
for label,tool,args,phrase in [
 ('below document track width','set_track_properties',{**track(),'width':250000},'track-width'),
 ('below document via drill','set_via_properties',{**via(),'drill':300000},None),
 ('half-Nm below doubled annular minimum','set_via_properties',{**via(),'diameter':700002,'drill':400001},None),
 ('nonpositive track width','set_track_properties',{**track(),'width':0},None),
 ('drill equals outer diameter','set_via_properties',{**via(),'drill':900003},None),
 ('unknown movement key','set_track_properties',{**track(),'start':{'x':0,'y':0}},None),
 ('oversized raw replacement net','set_track_properties',{**track(),'net':'x'*257},'256'),
 ('oversized raw expected net','set_track_properties',{**track(),'expected':{**track()['expected'],'net':'x'*257}},'256'),
 ('wrong expected object','set_track_properties',{**track(),'expected':{**track()['expected'],'width':1}},'track changed'),
 ('exact track no-op','set_track_properties',{**track(),'width':350001},'no property changes'),
 ('exact via no-op','set_via_properties',via(),'no property changes')]:reject(tool,args,phrase);check(label+' refuses full state/status unchanged')
old=state();out=timed('annulus_equality','set_via_properties',{**via(),'diameter':700003,'drill':400001});check('exact doubled annular equality accepted without implicit resizing',state()['project']['board']['vias'][vid]['diameter']==700003);a.call('undo',{});assert state()['project']==old['project']
old=state();timed('unassigned_track','set_track_properties',{**track(),'net':None});check('explicit unassigned net retained without inference',state()['project']['board']['tracks'][tid]['net'] is None);a.call('undo',{});assert state()['project']==old['project']
old_request=track();a.call('set_named_net_rules',guard(rules=[{'net':'N','minimum_copper_clearance':100001,'minimum_track_width':350002,'minimum_via_drill':300001,'minimum_annular_width':150001}]))
reject('set_track_properties',old_request);check('peer rule edit invalidates old full cursor');reject('set_track_properties',track());check('fresh zero request cannot weaken new document minimum');a.call('undo',{})
# Foreign bottom-side copper crossed only after layer change.
fid=a.call('add_track',{'start_x_mm':10,'start_y_mm':9,'end_x_mm':10,'end_y_mm':11,'width_mm':.25,'layer':'top','net':'FOREIGN'})['created'][0]
reject('set_track_properties',{**track(),'layer':'top','width':400001},'preflight failed');check('joint candidate DRC rejects foreign copper on changed layer');a.call('undo',{})
# Same-net holes still participate in hole spacing.
a.call('add_via',{'x_mm':30.7,'y_mm':25.000003,'diameter_mm':.6,'drill_mm':.3,'net':'N'})
reject('set_via_properties',{**via(),'diameter':2000001,'drill':1200001},'hole');check('same-net hole-spacing failure rejects via atomically');a.call('undo',{})
old_request=track();a.call('save_project',{'path':str(root/'edited.viamind.json'),'overwrite':False});a.call('open_project',{'path':str(root/'edited.viamind.json'),'discard_dirty':True,'expected_cursor':state()['cursor']})
check('same UUID reopen retains exact object IDs but replaces stream',state()['project']['id']==old_request['expected_project'] and state()['cursor']['stream_id']!=old_request['expected_cursor']['stream_id']);reject('set_track_properties',old_request);check('old stream refuses after same UUID reopen')
a.call('open_project',{'path':str(fixture),'discard_dirty':True,'expected_cursor':state()['cursor']})
check('final native fixture and independent observer are identical',b.call('get_session_state',{})==state() and state()['project']==json.loads(fixture.read_text())['project'])
report.update(passed=True,assertion_count=len(report['checks']),fixture=str(fixture),final_state=state(),final_status=status())
(root/'production-http.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items()if k!='final_state'},indent=2))
