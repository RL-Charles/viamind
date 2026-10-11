from client import Client
import json,sys,time,hashlib
from pathlib import Path
root=Path(__file__).parent
repo=Path(sys.argv[4]); a,b=Client(int(sys.argv[1])),Client(int(sys.argv[1]))
report={'passed':False,'port':int(sys.argv[1]),'source_commit':sys.argv[3],'binary_sha256':hashlib.sha256(Path(sys.argv[2]).read_bytes()).hexdigest(),'checks':[],'latency_ms':{}}
def check(label,ok):
 assert ok,label
 report['checks'].append(label)
def state():return a.call('get_session_state',{})
def guard(**kw):
 s=state();return {'expected_project':s['project']['id'],'expected_cursor':s['cursor'],**kw}
def rule(net='N',drill=300001,annulus=150001):return {'net':net,'minimum_copper_clearance':0,'minimum_track_width':1,'minimum_via_drill':drill,'minimum_annular_width':annulus}
def replace(rows,client=a):return client.call('set_named_net_rules',guard(rules=rows))
def reject(tool,args,contains=None):
 old=state();r=b.rpc('tools/call',{'name':tool,'arguments':args});assert r.get('isError'),r
 if contains:assert contains in json.dumps(r),r
 assert state()==old
 return r
def timed(label,tool,args):
 start=time.perf_counter();r=a.call(tool,args);report['latency_ms'][label]=(time.perf_counter()-start)*1000;return r
def open_owned(path):
 s=state();return a.call('open_project',{'path':str(path),'discard_dirty':True,'expected_cursor':s['cursor']})
def save(path):a.call('save_project',{'path':str(path),'overwrite':False});return path.read_bytes()
def pad(name,x,y,side):
 oid=a.call('add_part',{'designator':name,'value':'via floor endpoint','footprint':'explicit','x_mm':x,'y_mm':y,'side':side})['created'][0]
 part=state()['project']['board']['parts'][oid]
 a.call('set_part_pads',{'id':oid,'expected':part,'pads':[{'number':'1','x_mm':0,'y_mm':0,'width_mm':1,'height_mm':1,'net':'N'}]})
 return {'part_id':oid,'pad_id':state()['project']['board']['parts'][oid]['pads'][0]['id']}
zero={'copper_clearance_mm':0,'edge_clearance_mm':0,'minimum_via_drill_mm':0,'minimum_annular_width_mm':0,'hole_clearance_mm':0,'hole_to_copper_clearance_mm':0,'max_violations':1000}
# Continue on our isolated retained application, replacing only its own verification scratch.
s=state();a.call('new_project',{'name':'Named via minima HTTP proof','discard_dirty':True,'expected_cursor':s['cursor']})
s=state();a.call('set_board_outline',{'expected':{'project_id':s['project']['id'],'outline':None},'outline':{'outer':[{'x_mm':x,'y_mm':y} for x,y in [(0,0),(100,0),(100,100),(0,100)]],'cutouts':[]}})
start,target=pad('J1',10,10,'top'),pad('J2',60,10,'bottom')
replace([rule(drill=0,annulus=0)])
base=root/'legacy-zero-via.viamind.json'; baseline_bytes=save(base)
check('zero via fields omit canonical policy keys and retain native schema8',json.loads(baseline_bytes)['version']==8 and 'minimum_via_drill' not in json.loads(baseline_bytes)['project']['board']['net_rules']['N'])
old=state();req=guard(rules=[rule()]);out=timed('set_rules','set_named_net_rules',req)
policy=state();check('both exact odd-Nm via floors replace policy in one event',len(out['deltas'])==1 and policy['cursor']['sequence']==old['cursor']['sequence']+1)
check('independent peer query returns exact full cursor and via minima',b.call('query_named_net_rules',{})==a.call('query_named_net_rules',{}) and a.call('query_named_net_rules',{})['document_net_rules']['N']['minimum_annular_width']==150001)
reject('set_named_net_rules',req);check('stale peer policy replace rejects whole record',True)
for label,row in [('negative drill',rule(drill=-1)),('oversized drill',rule(drill=100000001)),('negative annulus',rule(annulus=-1)),('oversized annulus',rule(annulus=100000001))]:
 reject('set_named_net_rules',guard(rules=[row]));check(label+' rejects unchanged complete session',True)
for tool in ['route_pad_via_polyline','plan_pad_via_walkaround','route_pad_via_walkaround']:
 for label,dia,drill in [('one-Nm drill below',600002,300000),('half-Nm annulus below',600002,300001)]:
  q=guard(start=start,target=target,via={'x':35000000,'y':20000000},grid=1000000,width=250000,via_diameter=dia,via_drill=drill,rules=zero)
  if tool=='route_pad_via_polyline':q.update(before=[{'x':20000000,'y':15000000}],after=[{'x':50000000,'y':15000000}])
  reject(tool,q,'no resizing');check(tool+' '+label+' refuses without any copper/event',True)
manual=guard(start=start,target=target,via={'x':35000000,'y':20000000},before=[{'x':20000000,'y':15000000}],after=[{'x':50000000,'y':15000000}],grid=1000000,width=250000,via_diameter=600003,via_drill=300001,rules=zero)
old=state();timed('manual_via_equal','route_pad_via_polyline',manual);after=state()
via=next(iter(after['project']['board']['vias'].values()))
check('manual route equality preserves explicit dimensions and groups tracks/via into one event',via['diameter']==600003 and via['drill']==300001 and len(after['project']['board']['tracks'])==4 and after['cursor']['sequence']==old['cursor']['sequence']+1)
a.call('undo',{});check('manual equality one undo restores exact board and policy',state()['project']==old['project']);a.call('redo',{});check('manual equality redo restores identical via/track IDs',state()['project']==after['project']);a.call('undo',{})
walk=guard(start=start,target=target,via={'x':35000000,'y':20000000},grid=1000000,width=250000,via_diameter=600003,via_drill=300001,rules=zero)
old=state();plan=timed('plan_via_equal','plan_pad_via_walkaround',walk)
check('fixed-via equality returns only complete joint checked immutable plan with actual document floors',state()==old and plan['complete_scoped_preflight'] and plan['document_net_rules']==old['project']['board']['net_rules'])
timed('commit_via_equal','route_pad_via_walkaround',walk);after=state();via=next(iter(after['project']['board']['vias'].values()))
check('fixed-via equality commits exact dimensions without implicit resizing',via['diameter']==600003 and via['drill']==300001 and after['cursor']['sequence']==old['cursor']['sequence']+1)
a.call('undo',{});check('fixed-via joint undo restores original pads/policy with no new copper',state()['project']==old['project'])
# Stronger caller baseline remains enforced over weaker matching document minima.
strong={**zero,'minimum_via_drill_mm':.300002}
reject('plan_pad_via_walkaround',{**guard(**{k:v for k,v in walk.items() if k not in ['expected_project','expected_cursor']}),'rules':strong},'no resizing')
check('stronger saved request drill baseline cannot be weakened by document floor',True)
# Stored primitive geometry may violate policy: shared immutable DRC finds exact object IDs.
v=a.call('add_via',{'x_mm':80,'y_mm':80,'diameter_mm':.600002,'drill_mm':.300001,'net':'N'})['created'][0]
old=state();r=a.call('run_drc',zero)['report'];issues=[i for i in r['violations'] if i['kind']=='minimum_annular_width']
check('half-Nm stored radial annulus produces stable-ID exact required-floor DRC issue',len(issues)==1 and issues[0]['objects'][0]['via_id']==v and issues[0]['required_clearance']==150001)
check('immutable DRC reports zero request baselines plus actual full document via policy',state()==old and r['rules']['minimum_via_drill']==0 and r['document_net_rules']==old['project']['board']['net_rules'])
a.call('undo',{})
via_fixture=root/'via-rules.viamind.json';save(via_fixture);native=json.loads(via_fixture.read_text());check('nonzero via minima require canonical native schema10',native['version']==10 and native['project']['board']['net_rules']['N']['minimum_via_drill']==300001)
old_request=guard(rules=[rule()]);open_owned(via_fixture)
reject('set_named_net_rules',old_request);check('same-project reopen stream rejects stale via rule command',True)
for version in [8,9]:
 bad={**native,'version':version};path=root/f'bad-via-header-{version}.viamind.json';path.write_text(json.dumps(bad)+'\n');reject('open_project',{'path':str(path),'discard_dirty':True,'expected_cursor':state()['cursor']});check('nonzero via minima reject older native header'+str(version)+' unchanged',True)
# Strict duplicate detection in the new scalar fields remains in original typed visitor.
raw=via_fixture.read_text().replace('"minimum_via_drill": 300001','"minimum_via_drill": 300001, "minimum_via_drill": 300002')
assert raw!=via_fixture.read_text();path=root/'duplicate-via-key.viamind.json';path.write_text(raw);reject('open_project',{'path':str(path),'discard_dirty':True,'expected_cursor':state()['cursor']});check('duplicate nonzero via field key refuses without last-wins native parsing',True)
replace([rule(drill=0,annulus=0)]);clear=root/'cleared-via-to-schema8.viamind.json';cleared=save(clear)
check('clearing only via floors restores exact authored legacy schema8 native bytes',cleared==baseline_bytes)
# Actual saved checked-job uses same captured policy and zero caller thresholds.
fixture=json.loads((repo/'fixtures/fabrication/asymmetric.viamind.json').read_text());fixture['version']=10;fixture['project']['board']['net_rules']={'GND':{'minimum_copper_clearance':0,'minimum_track_width':250000,'minimum_via_drill':300000,'minimum_annular_width':250000}}
checked=root/'checked-via-rules.viamind.json';checked.write_text(json.dumps(fixture,indent=2)+'\n');open_owned(checked)
job={'format':'viamind-output-job','version':1,'name':'Document via minima equality','profile':'checked_outer_2layer','mask_expansion_mm':0,'checks':zero};jobpath=root/'checked-via-job.json';a.call('save_output_job',{'path':str(jobpath),'job':job})
old=state();release=root/'checked-via-release';out=timed('checked_via_equal','run_output_job',{'job_path':str(jobpath),'path':str(release),'expected_cursor':old['cursor']});validation=json.loads((release/'validation/drc.json').read_text());manifest=json.loads((release/'release-manifest.json').read_text())
check('checked job exact drill/annulus equality publishes complete twelve-file release without mutating design',out['file_count']==12 and state()==old and not validation['report']['violations'])
check('checked validation retains zero caller thresholds and actual captured document via floors',validation['requested_rules']['minimum_via_drill_mm']==0 and validation['requested_rules']['minimum_annular_width_mm']==0 and validation['report']['document_net_rules']==old['project']['board']['net_rules'])
check('checked validation and release manifest identify same captured full cursor',validation['exported_cursor']==old['cursor'] and manifest['exported_cursor']==old['cursor'])
for label,drill,ann in [('one-Nm-drill',300001,250000),('one-Nm-annulus',300000,250001)]:
 replace([{'net':'GND','minimum_copper_clearance':0,'minimum_track_width':250000,'minimum_via_drill':drill,'minimum_annular_width':ann}]);dest=root/('checked-refused-'+label)
 reject('run_output_job',{'job_path':str(jobpath),'path':str(dest),'expected_cursor':state()['cursor']});check(label+' document floor cannot be bypassed by zero saved request or publish output',not dest.exists());a.call('undo',{});check(label+' one undo restores exact saved-source geometry/policy',state()['project']==old['project'])
check('all checked refusals leave no hidden output-job staging',not list(root.glob('.viamind-output-job-*')))
# Final inspection fixture contains the exact odd-Nm values, and no local draft changes it.
open_owned(via_fixture)
report.update(passed=True,assertion_count=len(report['checks']),fixture=str(via_fixture),checked_release=str(release),final_state=state())
(root/'production-via-http.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='final_state'},indent=2))
