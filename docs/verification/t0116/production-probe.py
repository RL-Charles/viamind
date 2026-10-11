#!/usr/bin/env python3
"""Exercise an explicitly owned complete-package project through initialized real MCP clients."""
import argparse,copy,hashlib,importlib.util,json,os,subprocess,sys
from pathlib import Path
sys.dont_write_bytecode=True
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',type=Path,required=True);p.add_argument('--repo',type=Path,required=True);p.add_argument('--native',type=Path,required=True);p.add_argument('--endpoint',required=True);p.add_argument('--output',type=Path,required=True)
a=p.parse_args();out=a.output.resolve();out.mkdir();binary=a.binary.resolve(strict=True);native=a.native.resolve(strict=True);repo=a.repo.resolve(strict=True)
sys.path.insert(0,str(repo/'scripts'));from mcp_reference import StdioClient
spec=importlib.util.spec_from_file_location('smoke',repo/'scripts/mcp-smoke.py');smoke=importlib.util.module_from_spec(spec);spec.loader.exec_module(smoke)
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
report={'passed':False,'checks':[],'binary':str(binary),'binary_sha256':digest(binary),'endpoint':a.endpoint,'scope':'Owned scratch project only; complete explicit package to one physical part, scoped static graph pin nets, no physical continuity/vendor acceptance.'};clients=[]
def check(name,condition):
    assert condition,name
    report['checks'].append(name)
try:
    clients=[smoke.McpClient(a.endpoint,'Complete package creator'),smoke.McpClient(a.endpoint,'Complete package peer')]
    def call(name,args=None,peer=False,error=False):return clients[int(peer)].call(name,args or {},expect_error=error)
    tools={t['name'] for t in clients[0].rpc('tools/list',{})['tools']}
    check('actual initialized discovery exposes complete shared package operations',{'preview_package_pcb_component','create_package_pcb_component','link_schematic_package','query_linked_components','query_linked_pin_pads','preview_schematic_pcb_sync','synchronize_schematic_pcb'}<=tools)
    owned=json.loads(native.read_bytes())['project'];assert owned['name']=='T116 complete package owned proof' and not owned['board']['parts'];report['initial_native_sha256']=digest(native)
    old=call('get_session_state');check('owned scratch identity is verified before lifecycle replacement',old['project']['id']==owned['id'] and old['project']['name']==owned['name']);call('open_document',{'path':str(native),'discard_dirty':True,'expected_cursor':old['cursor']});base=call('get_session_state')
    check('two actual clients share coherent owned initial project',call('get_session_state',peer=True)==base)
    project=base['project'];assert project['name']=='T116 complete package owned proof';assert not project['board']['parts'];sheet_id=next(iter(project['schematic']['sheets']))
    template=json.loads((repo/'fixtures/symbol-packages/original-quad.viamind-symbol-package.json').read_text())['package']
    proposal=call('snapshot_symbol_sheet',{'sheet_id':sheet_id});proposal.update(template=template,designator='U1',placements=[{'key':u['key'],'position':{'x':i*20_000_000+1,'y':3},'rotation_quarters':i%4,'mirrored':bool(i%2)} for i,u in enumerate(template['units'])]);call('place_symbol_package',proposal)
    placed=call('get_session_state');content=placed['project']['schematic']['sheets'][sheet_id]['content'];package_id=next(iter(content['packages']));package=content['packages'][package_id]
    check('actual package placement retains five complete explicit units and fourteen distinct pins',len(package['units'])==5 and len(content['symbols'])==5 and sum(len(s['pins']) for s in content['symbols'].values())==14)
    footprint={'name':'Authored-14pad','pads':[{'number':str(n),'position':{'x':n*2_000_000,'y':257},'width':800001,'height':600003} for n in range(14,0,-1)]}
    def request():return {'expected_project':placed['project']['id'],'source_cursor':call('get_session_state')['cursor'],'placement':{'package_id':package_id,'footprint':footprint,'position':{'x':40000001,'y':30000003},'side':'bottom','rotation_mdeg':90000}}
    good=request();preview=call('preview_package_pcb_component',good);check('complete preview is immutable deterministic with all unit and actual pin provenance',call('get_session_state')==placed and preview==call('preview_package_pcb_component',good) and preview['unit_count']==5 and len(preview['pins'])==14 and len({r['net'] for r in preview['pins']})==14)
    for mode in ('missing','extra','duplicate','wrong-footprint','noncardinal'):
        bad=copy.deepcopy(good)
        if mode=='missing':bad['placement']['footprint']['pads'].pop()
        elif mode=='extra':bad['placement']['footprint']['pads'].append(dict(bad['placement']['footprint']['pads'][0],number='15'))
        elif mode=='duplicate':bad['placement']['footprint']['pads'][0]['number']='1'
        elif mode=='wrong-footprint':bad['placement']['footprint']['name']='Unassigned-other'
        else:bad['placement']['rotation_mdeg']=45000
        check('actual complete preview atomically refuses '+mode,bool(call('preview_package_pcb_component',bad,error=True)) and call('get_session_state')==placed)
    call('add_part',{'designator':'PEER1','value':'keep','footprint':'F','x_mm':1,'y_mm':1},peer=True);call('undo',peer=True);peer=call('get_session_state')
    check('actual equal-design peer edit undo invalidates full-cursor prepared creation',peer['project']==placed['project'] and peer['cursor']!=placed['cursor'] and bool(call('create_package_pcb_component',preview['command'],error=True)) and call('get_session_state')==peer)
    preview=call('preview_package_pcb_component',request());created=call('create_package_pcb_component',preview['command']);part_id=created['created'][0];physical=call('get_session_state');part=physical['project']['board']['parts'][part_id]
    check('actual create produces exactly one physical owner preserving all schematic identities',len(created['created'])==1 and len(created['deltas'])==1 and len(physical['project']['board']['parts'])==1 and part['schematic_package']==package_id and 'schematic_symbol' not in part and physical['project']['schematic']==placed['project']['schematic'])
    check('actual source package remains disconnected even for explicit power role',len({p['net'] for p in part['pads']})==14)
    mapping=call('query_linked_components');check('actual mapping has one physical row and five explicit unit records',mapping['total']==1 and mapping['components'][0]['package_id']==package_id and len(mapping['components'][0]['package_units'])==5)
    for identity in [part_id,package_id,*[u['symbol_id'] for u in package['units']]]:
        row=call('query_linked_components',{'object_ids':[identity]},peer=True);term=call('query_linked_pin_pads',{'component_id':identity},peer=True)
        check('actual stable owner/terminal query '+identity,row['components']==mapping['components'] and not row['unknown_ids'] and term['coverage']=='linked' and term['total']==14)
        for r in term['terminals']:
            n=int(r['number']);assert r['board_position']=={'x':40000258,'y':30000003+n*2_000_000};symbol=content['symbols'][r['symbol_id']];pin=next(p for p in symbol['pins'] if p['id']==r['pin_id']);x,y=pin['position']['x'],pin['position']['y'];x=-x if symbol['mirrored'] else x
            for _ in range(symbol['rotation_quarters']):x,y=y,-x
            assert r['schematic_position']=={'x':symbol['position']['x']+x,'y':symbol['position']['y']+y};assert r['package_id']==package_id and r['unit_key']==next(u['key'] for u in package['units'] if u['symbol_id']==symbol['id']) and r['navigable']
    check('independent signed integer geometry oracle verifies every pin and bottom90 pad',call('get_session_state')==physical)
    check('actual peer cannot undo another clients package owner',bool(call('undo',peer=True,error=True)) and call('get_session_state')==physical)
    call('undo');check('actual creator undo removes only the complete physical owner',call('get_design')==placed['project']);call('redo');check('actual creator redo restores identical Part and pad IDs',call('get_design')==physical['project'])
    call('update_part',{'id':part_id,'designator':'PCB9','value':'Explicit PCB value'},peer=True);before_back=call('get_session_state');back=call('preview_schematic_pcb_sync',{'direction':'back'});check('back preview declares every actual member and remains immutable',len(back['changes'])==10 and {r['unit_key'] for r in back['changes']}=={'A','B','C','D','P'} and call('get_session_state')==before_back)
    call('synchronize_schematic_pcb',back['command']);annotated=call('get_design');check('actual back annotation updates all shared fields preserving membership geometry board',all(s['designator']=='PCB9' and s['value']=='Explicit PCB value' and s['footprint']=='Authored-14pad' for s in annotated['schematic']['sheets'][sheet_id]['content']['symbols'].values()) and annotated['board']==before_back['project']['board'] and annotated['schematic']['sheets'][sheet_id]['content']['packages']==content['packages'])
    call('undo');check('actual back annotation undo restores exact project',call('get_design')==before_back['project']);call('undo',peer=True);check('peer own value undo restores original complete owner',call('get_design')==physical['project'])
    checkpoint=call('get_session_state');link={'expected':checkpoint['project'],'source_cursor':checkpoint['cursor'],'part_id':part_id,'package_id':None};call('link_schematic_package',link)
    unlinked=call('get_design');expected=copy.deepcopy(checkpoint['project']);expected['board']['parts'][part_id].pop('schematic_package');check('explicit package unlink changes only stable association',unlinked==expected)
    stale=call('get_session_state');check('stale original package guard refuses unchanged session',bool(call('link_schematic_package',link,error=True)) and call('get_session_state')==stale);call('undo');check('undo relinks exact stable package owner',call('get_design')==physical['project'])
    final_native=out/'complete-package.viamind.json';saved=call('save_document',{'path':str(final_native)});native_doc=json.loads(final_native.read_bytes());check('actual native save uses feature12 and exact complete project',native_doc['version']==12 and native_doc['project']==physical['project'] and not saved['dirty'])
    before_open=call('get_session_state');call('open_document',{'path':str(final_native)});reopened=call('get_session_state');check('actual native reopen retains package association IDs and resets stream',reopened['project']==physical['project'] and reopened['cursor']['stream_id']!=before_open['cursor']['stream_id'])
    with StdioClient([str(binary),'--headless','--agent-read-only','--open',str(final_native)],out,timeout=90) as client:
        client.initialize();names={t['name'] for t in client.rpc('tools/list',{})['tools']};check('actual read-only stdio exposes package observations and hides mutations','preview_package_pcb_component' in names and 'query_linked_pin_pads' in names and 'create_package_pcb_component' not in names and 'link_schematic_package' not in names)
        readonly=client.call('get_session_state',{});check('actual stdio independent native query matches all physical mapping facts',client.call('query_linked_pin_pads',{'component_id':package_id})['terminals']==call('query_linked_pin_pads',{'component_id':package_id})['terminals'] and client.call('get_session_state',{})==readonly);client.finish()
    check('two production HTTP clients agree after exact native reopen',call('get_session_state',peer=True)==call('get_session_state'))
    report.update(passed=True,check_count=len(report['checks']),final_native=str(final_native),native_sha256=digest(final_native),package_id=package_id,part_id=part_id,project_id=physical['project']['id'])
finally:
    for client in clients:client.close()
    with (out/'report.json').open('x') as f:json.dump(report,f,indent=2);f.flush();os.fsync(f.fileno())
print(json.dumps({'passed':report['passed'],'checks':len(report['checks']),'output':str(out)}))
