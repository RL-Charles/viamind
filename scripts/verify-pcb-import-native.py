#!/usr/bin/env python3
"""Check owned production standalone PCB import; all writes are new temporary files."""
import sys
sys.dont_write_bytecode = True
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import select
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True)
parser.add_argument('--fixture', default=str(Path(__file__).resolve().parent.parent/'fixtures/import/original-two-layer.kicad_pcb'))
parser.add_argument('--report', required=True)
parser.add_argument('--url', help='Owned actual production desktop MCP URL; never target another session')
args = parser.parse_args()
binary = Path(args.binary).resolve()
source_bytes = Path(args.fixture).read_bytes()
directory = Path(tempfile.mkdtemp(prefix='viamind-t0083-native-'))
source = directory/'original.kicad_pcb'
source.write_bytes(source_bytes)
process = None
clients = []
sequence = 0
checks = []
report = {'passed':False,'checks':checks,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'transport':args.url or 'production headless stdio','fixture_sha256':hashlib.sha256(source_bytes).hexdigest()}
if not args.url:
    env = os.environ.copy()
    env.pop('DISPLAY',None)
    env.pop('WAYLAND_DISPLAY',None)
    process = subprocess.Popen([str(binary),'--headless'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env)


def rpc(method,params=None,notification=False):
    global sequence
    sequence += 1
    message = {'jsonrpc':'2.0','method':method,'params':params or {}}
    if not notification:
        message['id']=sequence
    process.stdin.write(json.dumps(message)+'\n')
    process.stdin.flush()
    if notification:
        return None
    deadline = time.monotonic()+30
    while time.monotonic()<deadline:
        assert select.select([process.stdout],[],[],max(0,deadline-time.monotonic()))[0],method
        response=json.loads(process.stdout.readline())
        if response.get('id')==sequence:
            assert 'error' not in response,response
            return response['result']
    raise AssertionError(method)


def call(name,arguments=None,error=False,peer=False):
    result=rpc('tools/call',{'name':name,'arguments':arguments or {}})
    assert bool(result.get('isError'))==error,result
    return result if error else result.get('structuredContent') or json.loads(result['content'][0]['text'])


if args.url:
    spec=importlib.util.spec_from_file_location('mcp_smoke',Path(__file__).resolve().parent/'mcp-smoke.py')
    module=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    clients=[module.McpClient(args.url,'native-pcb-import-a'),module.McpClient(args.url,'native-pcb-import-b')]
    def rpc(method,params=None,notification=False):
        assert not notification
        return clients[0].rpc(method,params or {})
    def call(name,arguments=None,error=False,peer=False):
        return clients[int(peer)].call(name,arguments or {},expect_error=error)


def check(name,condition):
    assert condition,name
    checks.append(name)


def apply(preview):
    return {'path':preview['path'],'expected_project':preview['expected_project'],'expected_cursor':preview['expected_cursor'],'expected_source_sha256':preview['source_sha256']}


try:
    if process:
        rpc('initialize',{'protocolVersion':'2025-03-26','capabilities':{},'clientInfo':{'name':'native-pcb-import','version':'1'}})
        rpc('notifications/initialized',notification=True)
    tools={t['name'] for t in rpc('tools/list')['tools']}
    check('bounded preview and guarded import tools discovered',{'preview_pcb_import','import_pcb'}<=tools)
    before=call('get_session_state')
    status=call('get_document_status')
    preview=call('preview_pcb_import',{'path':str(source)})
    check('repeat source preview deterministic and entire session read-only',call('preview_pcb_import',{'path':str(source)},peer=bool(args.url))==preview and call('get_session_state')==before and call('get_document_status')==status)
    check('exact supported geometry with explicit incomplete fidelity',preview['can_import'] and preview['report']['imported']=={'parts':1,'pads':2,'tracks':2,'vias':1,'outline_vertices':8} and not preview['report']['full_fidelity'] and preview['report']['omitted_lists']>0 and preview['report']['blocked_lists']==0)
    items=preview['report']['items']
    edge=next(i for i in items if i['token']=='gr_rect')
    check('outline source identity report-only while native object identity retained',edge['source_id'] and any(i['path'].startswith(edge['path']+'/') and i['token']=='uuid' and i['disposition']=='omitted' for i in items) and any(i['token']=='uuid' and i['disposition']=='imported' for i in items))
    call('preview_pcb_import',{'path':str(source),'extra':True},error=True)
    call('import_pcb',dict(apply(preview),expected_project='00000000-0000-4000-8000-000000000001'),error=True)
    check('invalid input and wrong project reject unchanged',call('get_session_state')==before and call('get_document_status')==status)
    source.write_bytes(source_bytes.replace(b'smd rect',b'smd roundrect'))
    blocked=call('preview_pcb_import',{'path':str(source)})
    call('import_pcb',apply(blocked),error=True)
    check('unsupported pad blocks whole conversion with actionable report',not blocked['can_import'] and blocked['report']['blocked_lists']>0 and any(i['token']=='pad' and i['disposition']=='blocked' and i['reason'] for i in blocked['report']['items']) and call('get_session_state')==before)
    source.write_bytes(source_bytes+b'\n')
    call('import_pcb',apply(preview),error=True)
    check('changed exact source bytes reject before replacement',call('get_session_state')==before and call('get_document_status')==status)
    source.write_bytes(source_bytes)
    call('add_part',{'designator':'KEEP1','value':'unsaved native','footprint':'original','x_mm':0,'y_mm':0},peer=bool(args.url))
    dirty=call('get_session_state')
    dirty_preview=call('preview_pcb_import',{'path':str(source)})
    call('import_pcb',apply(preview),error=True)
    call('import_pcb',apply(dirty_preview),error=True,peer=bool(args.url))
    check('both stale and fresh dirty-native requests preserve peer work',call('get_session_state')==dirty and call('get_document_status')['dirty'])
    keep=directory/'keep.viamind.json'
    call('save_project',{'path':str(keep)},peer=bool(args.url))
    keep_bytes=keep.read_bytes()
    imported=call('import_pcb',apply(dirty_preview),peer=bool(args.url))
    project=call('get_design')
    check('guarded replacement starts new unsaved native stream without source path',imported['project_id']!=dirty['project']['id'] and imported['cursor']['stream_id']!=dirty['cursor']['stream_id'] and imported['document']=={'path':None,'dirty':True} and not call('get_selection')['ids'])
    part=project['board']['parts']['20000000-0000-4000-8000-000000000001']
    pads=part['pads']
    pad=next(p for p in pads if p['number']=='1')
    check('native exact odd-Nm geometry and original IDs/nets preserved',part['position']=={'x':4000007,'y':5000011} and pad['position']=={'x':-500003,'y':250001} and pad['net']=='VIN' and len(project['board']['tracks'])==2 and len(project['board']['vias'])==1 and len(project['board']['outline']['cutouts'])==1)
    drc_before=call('get_session_state')
    drc=call('run_drc',{})
    coverage=drc['coverage']
    check('native bounded DRC checks imported objects without source-rule equivalence claim',coverage['checked_pads']==2 and coverage['checked_tracks']==2 and coverage['checked_vias']==1 and coverage['checked_holes']==1 and coverage['explicit_outline_checked'] and coverage['excluded_part_count']==0 and coverage['checks_not_performed'] and not drc['truncated'] and call('get_session_state')==drc_before)
    report['native_drc']=drc
    call('import_pcb',apply(dirty_preview),error=True)
    call('save_project',{},error=True)
    call('new_project',{},error=True)
    check('import cannot silently discard unsaved conversion or save to source',call('get_design')==project and call('get_document_status')=={'path':None,'dirty':True})
    call('update_part',{'id':part['id'],'designator':'U2'})
    call('undo')
    check('subsequent native edits undo exactly while import remains dirty',call('get_design')==project and call('get_document_status')['dirty'])
    native=directory/'imported.viamind.json'
    call('save_project',{'path':str(native)})
    old=call('get_session_state')['cursor']
    call('open_project',{'path':str(native),'expected_cursor':old},peer=bool(args.url))
    check('native save and same-project reopen preserve all imported model data',call('get_design')==project and call('get_session_state')['cursor']['stream_id']!=old['stream_id'] and not call('get_document_status')['dirty'])
    check('source and previous saved native bytes remain unchanged',source.read_bytes()==source_bytes and keep.read_bytes()==keep_bytes)
    report['saved_path']=str(native)
    report['fidelity_report']=preview['report']
    if process:
        process.stdin.close()
        check('clean production exit',process.wait(timeout=15)==0)
    report['passed']=True
finally:
    if process and process.poll() is None:
        process.terminate()
        process.wait(timeout=5)
    for client in clients:
        client.close()
    Path(args.report).write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
