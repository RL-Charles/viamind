"""Two actual initialized desktop HTTP clients; only this lane's owned fixture."""
import copy,hashlib,json,sys,time
from pathlib import Path
from client import Client
root=Path(__file__).parent
port=int(sys.argv[1]);binary=Path(sys.argv[2]);source=sys.argv[3]
a,b=Client(port),Client(port)
report={'passed':False,'source_commit':source,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'port':port,'checks':[],'latency_ms':{}}
def check(label,ok=True):
 assert ok,label
 report['checks'].append(label)
def state():return a.call('get_session_state',{})
def status():return a.call('get_document_status',{})
def open_file(path):a.call('open_project',{'path':str(path),'discard_dirty':True,'expected_cursor':state()['cursor']})
def preview(start=5,mode='reset_all',client=a):return client.call('preview_schematic_annotation',{'start':start,'mode':mode,'order':'x_then_y'})
def reject(tool,args,phrase=None):
 before,st=state(),status();response=b.rpc('tools/call',{'name':tool,'arguments':args});assert response.get('isError'),response
 if phrase:assert phrase in json.dumps(response),response
 assert state()==before and status()==st
 return response
def refs(p):return {k:s['designator']for sheet in p['schematic']['sheets'].values()for k,s in sheet['content']['symbols'].items()}
def oid(n):return f'00000000-0000-0000-0000-{n:012x}'
fixture=root/'packages.viamind.json';open_file(fixture)
original=state()['project'];before,st=state(),status();observed=preview();peer=preview(client=b)
check('independent clients discover immutable preview and guarded annotation',all(n in [t['name']for t in b.rpc('tools/list',{})['tools']]for n in ['preview_schematic_annotation','annotate_schematic']))
check('both previews exactly agree with complete cursor and unchanged session status',observed==peer and observed['source_cursor']==before['cursor']and observed['command']['expected_cursor']==before['cursor']and state()==before and status()==st)
check('twelve actual symbols and two packages produce twelve member rows',observed['symbols']==12 and observed['packages']==2 and len(observed['changes'])==12)
check('first package ordered explicit A B C D P including power unit',[(c['package_id'],c['unit_key'],c['after'])for c in observed['changes'][:5]]==[(oid(600),k,'U5')for k in ['A','B','C','D','P']])
check('ordinary symbol rows omit package metadata',observed['changes'][5]['symbol_id']==oid(301)and 'package_id'not in observed['changes'][5]and 'unit_key'not in observed['changes'][5])
start=time.perf_counter();result=a.call('annotate_schematic',observed['command']);report['latency_ms']['whole_package_apply']=(time.perf_counter()-start)*1000
changed=state()['project'];reference=refs(changed)
check('both packages each consume one number while singles interleave',all(reference[oid(n)]=='U5'for n in range(100,105))and all(reference[oid(n)]=='U7'for n in range(200,205))and reference[oid(301)]=='U6'and reference[oid(302)]=='U8')
restored=copy.deepcopy(changed)
for sheet in restored['schematic']['sheets'].values():
 for sid,s in sheet['content']['symbols'].items():s['designator']=refs(original)[sid]
check('all member identities pins roles keys package maps and board unchanged',restored==original)
check('one sheet delta and one design event',len(result['deltas'])==1 and not result['created']and state()['cursor']['sequence']==before['cursor']['sequence']+1)
check('peer reads exact committed whole package project',b.call('get_design',{})==changed)
reject('annotate_schematic',observed['command'],'session changed');check('stale sequence refuses without changes')
a.call('undo',{});check('one undo restores all units and original complete project',state()['project']==original)
a.call('redo',{});check('one redo restores all units and original identities',state()['project']==changed)
noop=preview();before,st=state(),status();check('already current package plan has no changes',noop['changes']==[]);a.call('annotate_schematic',noop['command']);check('no-op creates no event history or status change',state()==before and status()==st)
a.call('undo',{})
command=preview()['command'];missing=copy.deepcopy(command);del missing['expected_cursor'];reject('annotate_schematic',missing,'requires expected_cursor');check('grouped legacy cursor omission refuses')
wrong=copy.deepcopy(command);next(iter(wrong['expected_sheets'].values()))['name']='Wrong original';reject('annotate_schematic',wrong,'schematic changed');check('exact expected sheet still guarded alongside cursor')
poison=copy.deepcopy(command);next(iter(poison['expected_sheets'].values()))['content']['symbols']={f'poison{i}':None for i in range(2001)};reject('annotate_schematic',poison,'2000 symbols');check('malformed excess symbols counted before secondary typed decode')
poison=copy.deepcopy(command);next(iter(poison['expected_sheets'].values()))['content']['packages'][oid(600)]['units']=[None]*10001;reject('annotate_schematic',poison,'10000');check('malformed excess member rows counted before typed decode')
poison=copy.deepcopy(command);next(iter(poison['expected_sheets'].values()))['content']['packages'][oid(600)]['template_name']='x'*(256*1024+1);reject('annotate_schematic',poison,'256 KiB');check('raw package strings charged before secondary typed copy')
b.call('add_part',{'designator':'PEER1','value':'Unrelated board change','footprint':'explicit','x_mm':1,'y_mm':1});reject('annotate_schematic',command,'session changed');check('unrelated board edit invalidates full cursor despite equal expected sheets');b.call('undo',{})
command=preview()['command'];open_file(fixture);check('same UUID reopen replaces stream',state()['project']['id']==command['expected_project']and state()['cursor']['stream_id']!=command['expected_cursor']['stream_id']);reject('annotate_schematic',command,'session changed');check('same UUID reopened stream refuses old proposal')
# Existing numeric aliases reserve once for one package, conflict across owners.
alias=json.loads(fixture.read_text());c=next(iter(alias['project']['schematic']['sheets'].values()))['content']
for u in c['packages'][oid(600)]['units']:c['symbols'][u['symbol_id']]['designator']='u0007'
alias_path=root/'retained-alias.viamind.json';alias_path.write_text(json.dumps(alias,indent=2)+'\n');open_file(alias_path);retained=preview(1,'keep_existing');check('retained package number reserved once with exact original spelling',len(retained['changes'])==7 and all(c.get('package_id')!=oid(600)for c in retained['changes']))
a.call('annotate_schematic',retained['command']);check('whole retained package preserves exact spelling',all(refs(state()['project'])[oid(n)]=='u0007'for n in range(100,105)))
for u in c['packages'][oid(700)]['units']:c['symbols'][u['symbol_id']]['designator']='U7'
alias_path=root/'conflicting-alias.viamind.json';alias_path.write_text(json.dumps(alias,indent=2)+'\n');open_file(alias_path);reject('preview_schematic_annotation',{'mode':'keep_existing'},'ambiguous retained');check('numeric alias conflict across distinct owners refuses immutable preview')
open_file(fixture);a.call('annotate_schematic',preview()['command']);saved=root/'annotated.viamind.json';a.call('save_project',{'path':str(saved),'overwrite':False});check('native save uses schema11 preserving explicit membership',json.loads(saved.read_text())['version']==11)
old=copy.deepcopy(json.loads(saved.read_text()));old['version']=10;old_path=root/'old-header.viamind.json';old_path.write_text(json.dumps(old)+'\n');reject('open_project',{'path':str(old_path),'discard_dirty':True,'expected_cursor':state()['cursor']},'version 11');check('old native header rejects package data unchanged')
open_file(saved);check('native roundtrip preserves complete committed project',state()['project']==json.loads(saved.read_text())['project'])
open_file(fixture);before,st=state(),status();check('final peer and original fixture exactly preserved',b.call('get_session_state',{})==before and before['project']==json.loads(fixture.read_text())['project']and status()==st)
report.update(passed=True,assertion_count=len(report['checks']),fixture=str(fixture),final_state=state(),final_status=status())
(root/'production-http.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items()if k!='final_state'},indent=2))
