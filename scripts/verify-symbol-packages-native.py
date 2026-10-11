#!/usr/bin/env python3
"""Real owned-process native multi-unit workflow; no external symbol assets."""
import sys
sys.dont_write_bytecode=True
import argparse,importlib.util,json,os,tempfile
from pathlib import Path
from contextlib import ExitStack
from copy import deepcopy
parser=argparse.ArgumentParser();parser.add_argument('--binary',required=True);parser.add_argument('--report',required=True);parser.add_argument('--url');parser.add_argument('--repo',required=True);args=parser.parse_args()
sys.path.insert(0,str(Path(args.repo)/'scripts'))
from mcp_reference import StdioClient,digest,read_regular
checks=[]
def check(name,condition):
    if not condition:raise AssertionError(name)
    checks.append(name)
binary=Path(args.binary).resolve(strict=True);sha=digest(binary)
source=Path(args.repo)/'fixtures/symbol-packages/original-quad.viamind-symbol-package.json';original=read_regular(source)
with tempfile.TemporaryDirectory(prefix='viamind-package-proof-') as temporary,ExitStack() as stack:
    directory=Path(temporary)
    if args.url:
        spec=importlib.util.spec_from_file_location('smoke',Path(args.repo)/'scripts/mcp-smoke.py');smoke=importlib.util.module_from_spec(spec);spec.loader.exec_module(smoke)
        clients=[smoke.McpClient(args.url,'package-proof-a'),smoke.McpClient(args.url,'package-proof-b')]
        for c in clients:stack.callback(c.close)
        def call(name,args=None,error=False,peer=False):return clients[int(peer)].call(name,args or {},expect_error=error)
        tools=clients[0].rpc('tools/list',{})['tools']
    else:
        client=stack.enter_context(StdioClient([str(binary),'--headless'],directory));client.initialize();tools=client.discover()['tools']
        def call(name,args=None,error=False,peer=False):return client.call(name,args or {},expect_error=error)
    check('all five shared package tools advertised',{'load_symbol_package','save_symbol_package','place_symbol_package','delete_symbol_package','query_symbol_packages'}<={t['name']for t in tools})
    initial=call('get_session_state');call('new_project',{'name':'Original complete five-unit proof','expected_cursor':initial['cursor'],'discard_dirty':True})
    state=call('get_session_state');project=state['project']['id']
    created=call('edit_schematic_sheet',{'expected_project':project,'name':'Complete units','edits':[]});sheet_id=created['created'][0]
    native=directory/'protected.viamind.json';call('save_project',{'path':str(native),'overwrite':False});native_bytes=read_regular(native)
    part=call('add_part',{'designator':'KEEP1','value':'Preserved peer board','footprint':'Original','x_mm':1.000003,'y_mm':2.000007},peer=True);call('set_selection',{'ids':part['created']})
    before=call('get_session_state');status=call('get_document_status');template=call('load_symbol_package',{'path':str(source)},peer=True)
    check('authored complete quad plus power loads ordered 5 units/14 pins',len(template['units'])==5 and sum(len(u['pins'])for u in template['units'])==14 and [u['key']for u in template['units']]==['A','B','C','D','P'])
    check('detached load preserves full dirty session/native bytes',call('get_session_state')==before and call('get_document_status')==status and read_regular(native)==native_bytes)
    output=directory/'new-package.json';saved=call('save_symbol_package',{'template':template,'path':str(output)})
    check('NEW save exact complete unit/pin counts',saved['unit_count']==5 and saved['pin_count']==14)
    check('second client/fresh stdio reload exact definition',call('load_symbol_package',{'path':str(output)},peer=True)==template)
    check('detached save preserves complete session/status',call('get_session_state')==before and call('get_document_status')==status)
    bytes_before=read_regular(output)
    for path in [output,native]:call('save_symbol_package',{'template':template,'path':str(path)},error=True,peer=True)
    check('collision and active-native refusal preserve files',read_regular(output)==bytes_before and read_regular(native)==native_bytes)
    for units,needle in [([None]*33,'2..32'),([{'pins':[None]*1025},{'pins':[None]}],'1024')]:
        response=call('save_symbol_package',{'path':str(directory/'never.json'),'template':{'units':units}},error=True)
        check('borrowed '+needle+' boundary before malformed row',needle in json.dumps(response) and not(directory/'never.json').exists())
    g=call('snapshot_symbol_sheet',{'sheet_id':sheet_id});proposal={**g,'template':template,'designator':'U1','placements':[{'key':u['key'],'position':{'x':i*20000000+1,'y':3},'rotation_quarters':i%4,'mirrored':bool(i%2)}for i,u in enumerate(template['units'])]}
    result=call('place_symbol_package',proposal);placed=call('get_session_state');content=placed['project']['schematic']['sheets'][sheet_id]['content']
    check('complete placement one sheet delta and event',len(result['deltas'])==1 and placed['cursor']['sequence']==before['cursor']['sequence']+1)
    check('five units share one reference and complete explicit membership',len(content['packages'])==1 and len(content['symbols'])==5 and {s['designator']for s in content['symbols'].values()}=={'U1'})
    report=call('query_symbol_packages',peer=True);group=report['packages'][0]
    check('two clients observe complete ordered group/pin mapping',report==call('query_symbol_packages') and len(group['units'])==5 and sum(len(u['pins'])for u in group['units'])==14)
    ids={group['package_id']};numbers=set()
    for i,u in enumerate(group['units']):
        ids.add(u['symbol_id'])
        for pin in u['pins']:
            p=pin['pin'];ids.add(p['id']);numbers.add(p['number']);x,y=p['position']['x'],p['position']['y']
            if i%2:x=-x
            x,y=[(x,y),(y,-x),(-x,-y),(-y,x)][i%4]
            check('exact Nm endpoint '+p['number'],pin['position']=={'x':i*20000000+1+x,'y':3+y})
    check('all fresh native identities and distinct exact package numbers',len(ids)==20 and numbers=={str(i)for i in range(1,15)})
    check('power unit explicit and no implicit pin joins',group['units'][4]['role']=='power' and call('query_schematic_connectivity',{'limit':100})['total_nets']==14)
    check('read-only query/ERC preserve full placed session',call('run_erc') is not None and call('get_session_state')==placed)
    call('place_symbol_package',proposal,error=True);check('stale complete cursor/sheet refuses mutation',call('get_session_state')==placed)
    call('undo');check('single user undo restores full schematic and keeps board selection',call('get_session_state')['project']==before['project'] and call('get_session_state')['selection']==before['selection'])
    call('redo');check('redo restores every stable package/symbol/pin identity',call('get_session_state')['project']==placed['project'])
    g=call('snapshot_symbol_sheet',{'sheet_id':sheet_id});call('delete_symbol_package',{**g,'package_id':group['package_id']})
    deleted=call('get_session_state');check('complete deletion removes all units together',not deleted['project']['schematic']['sheets'][sheet_id]['content']['symbols'] and not deleted['project']['schematic']['sheets'][sheet_id]['content'].get('packages'))
    call('undo');check('complete deletion undo restores exact entire project',call('get_session_state')['project']==placed['project'])
    call('save_project',{'path':str(directory/'grouped.viamind.json'),'overwrite':False});file=json.loads(read_regular(directory/'grouped.viamind.json'));check('grouped native feature11 persisted',file.get('version')==11)
    cursor=call('get_session_state')['cursor'];call('open_project',{'path':str(directory/'grouped.viamind.json'),'discard_dirty':False,'expected_cursor':cursor});check('native reopen preserves complete grouped project',call('get_session_state')['project']==placed['project'])
    check('original source bytes and prior native protected',read_regular(source)==original and read_regular(native)==native_bytes)
    if not args.url:client.finish()
check('retained production executable unchanged',digest(binary)==sha)
report={'passed':True,'binary_sha256':sha,'transport':args.url or 'owned headless stdio','scope':'Complete explicit five-unit native package; no implicit hidden/shared pins, package annotation or physical PCB aggregation','checks':checks}
with open(args.report,'x')as f:json.dump(report,f,indent=2);f.flush();os.fsync(f.fileno())
print(json.dumps({'passed':True,'checks':len(checks),'report':args.report}))
