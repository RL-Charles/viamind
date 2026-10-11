#!/usr/bin/env python3
"""Check owned production standalone footprint conversion; all writes are new temporary files."""
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
parser.add_argument('--fixture', default=str(Path(__file__).resolve().parent.parent/'fixtures/import/original-rect-smd.kicad_mod'))
parser.add_argument('--report', required=True)
parser.add_argument('--url', help='Owned actual production desktop MCP URL; never target another session')
args = parser.parse_args()
binary = Path(args.binary).resolve()
source_bytes = Path(args.fixture).read_bytes()
directory = Path(tempfile.mkdtemp(prefix='viamind-t0089-native-'))
source = directory/'original.kicad_mod'
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
    clients=[module.McpClient(args.url,'native-footprint-conversion-a'),module.McpClient(args.url,'native-footprint-conversion-b')]
    def rpc(method,params=None,notification=False):
        assert not notification
        return clients[0].rpc(method,params or {})
    def call(name,arguments=None,error=False,peer=False):
        return clients[int(peer)].call(name,arguments or {},expect_error=error)


def check(name,condition):
    assert condition,name
    checks.append(name)


def conversion(preview,destination):
    return {'path':preview['path'],'expected_source_sha256':preview['source_sha256'],'destination':str(destination)}


try:
    if process:
        rpc('initialize',{'protocolVersion':'2025-03-26','capabilities':{},'clientInfo':{'name':'native-footprint-conversion','version':'1'}})
        rpc('notifications/initialized',notification=True)
    tools={t['name'] for t in rpc('tools/list')['tools']}
    check('detached preview and conversion tools discovered',{'preview_footprint_conversion','convert_footprint_file','import_footprint'}<=tools)
    native=directory/'keep.viamind.json'
    call('save_project',{'path':str(native)})
    native_bytes=native.read_bytes()
    created=call('add_part',{'designator':'KEEP1','value':'unsaved native','footprint':'old','x_mm':3.000003,'y_mm':4.000007},peer=bool(args.url))
    part_id=created['created'][0]
    part=call('get_design')['board']['parts'][part_id]
    call('set_part_pads',{'id':part_id,'expected':part,'pads':[{'number':'1','x_mm':0,'y_mm':0,'width_mm':1,'height_mm':1,'net':'VIN'}]},peer=bool(args.url))
    call('set_selection',{'ids':[part_id]})
    before=call('get_session_state')
    status=call('get_document_status')
    check('fixture begins with dirty native work and shared selection',status['dirty'] and call('get_selection')['ids']==[part_id])
    preview=call('preview_footprint_conversion',{'path':str(source)})
    check('two-client repeat preview deterministic with full session unchanged',call('preview_footprint_conversion',{'path':str(source)},peer=bool(args.url))==preview and call('get_session_state')==before and call('get_document_status')==status)
    fp=preview['footprint']
    check('exact odd-Nm detached geometry and explicit incomplete fidelity',preview['can_convert'] and fp['name']=='OriginalRect2' and len(fp['pads'])==2 and fp['pads'][0]['position']=={'x':-500003,'y':250001} and fp['pads'][1]['position']=={'x':750007,'y':-125003} and fp['pads'][1]['width']==1000001 and fp['pads'][1]['height']==600003 and not preview['report']['full_fidelity'] and preview['report']['blocked_lists']==0 and preview['report']['omitted_lists']>0)
    check('source UUIDs report-only and no output IDs or nets',all(i['disposition']=='omitted' for i in preview['report']['items'] if i['token']=='uuid') and any(i['source_id'] for i in preview['report']['items']) and all(set(p)=={'number','position','width','height'} for p in fp['pads']))
    call('preview_footprint_conversion',{'path':str(source),'extra':True},error=True)
    output=directory/'converted.viamind-footprint.json'
    call('convert_footprint_file',dict(conversion(preview,output),expected_source_sha256='0'*64),error=True)
    check('invalid schema and digest reject without output or session mutation',not output.exists() and call('get_session_state')==before)
    source.write_bytes(source_bytes.replace(b'smd rect',b'smd circle'))
    blocked=call('preview_footprint_conversion',{'path':str(source)})
    call('convert_footprint_file',conversion(blocked,output),error=True)
    check('unsupported pads block whole conversion with actionable per-list reason',not blocked['can_convert'] and any(i['token']=='pad' and i['disposition']=='blocked' and i['reason'] for i in blocked['report']['items']) and not output.exists())
    source.write_bytes(source_bytes.replace(b'(net 0 "")',b'(net 1 "VIN")'))
    assigned=call('preview_footprint_conversion',{'path':str(source)})
    call('convert_footprint_file',conversion(assigned,output),error=True)
    check('meaningful source nets never silently become detached',not assigned['can_convert'] and assigned['report']['blocked_lists']>0 and not output.exists())
    source.write_bytes(source_bytes+b'\n')
    call('convert_footprint_file',conversion(preview,output),error=True)
    check('changed exact source bytes reject before new-file publication',not output.exists() and call('get_session_state')==before)
    source.write_bytes(source_bytes)
    call('convert_footprint_file',conversion(preview,source),error=True)
    call('convert_footprint_file',conversion(preview,native),error=True)
    existing=directory/'existing.json'; existing.write_bytes(b'preserve existing bytes')
    call('convert_footprint_file',conversion(preview,existing),error=True)
    alias=directory/'alias.json'; alias.symlink_to(existing)
    call('convert_footprint_file',conversion(preview,alias),error=True)
    check('source native existing and symlink destinations preserve all bytes',source.read_bytes()==source_bytes and native.read_bytes()==native_bytes and existing.read_bytes()==b'preserve existing bytes' and alias.is_symlink())
    converted=call('convert_footprint_file',conversion(preview,output),peer=bool(args.url))
    envelope=json.loads(output.read_text())
    check('new native library strict round-trip matches exact preview',envelope=={'format':'viamind-footprint','version':1,'footprint':fp} and converted['footprint']==fp and converted['source_sha256']==preview['source_sha256'])
    check('dirty work active path cursor history and shared selection remain unchanged',call('get_session_state')==before and call('get_document_status')==status and call('get_selection')['ids']==[part_id])
    output_bytes=output.read_bytes()
    call('convert_footprint_file',conversion(preview,output),error=True)
    check('repeated publication cannot clobber native library',output.read_bytes()==output_bytes and call('get_session_state')==before)
    old=before['project']['board']['parts'][part_id]
    call('import_footprint',{'expected_project':before['project']['id'],'expected':old,'path':str(output)})
    applied=call('get_design')['board']['parts'][part_id]
    pads=applied['pads']
    check('explicit subsequent application retains numbered pad identity net and placement',pads[0]['id']==old['pads'][0]['id'] and pads[0]['net']=='VIN' and pads[1]['id']!=old['pads'][0]['id'] and pads[1]['net'] is None and applied['position']==old['position'] and pads[0]['position']==fp['pads'][0]['position'])
    call('undo')
    check('explicit application is undoable independently of detached conversion',call('get_design')==before['project'] and call('get_document_status')==status and output.read_bytes()==output_bytes)
    check('source and saved native bytes remain unchanged through reuse',source.read_bytes()==source_bytes and native.read_bytes()==native_bytes)
    report['fidelity_report']=preview['report']; report['native_library_path']=str(output); report['native_library_sha256']=hashlib.sha256(output_bytes).hexdigest()
    if process:
        process.stdin.close()
        check('clean production exit',process.wait(timeout=15)==0)
    report['passed']=True
finally:
    if process and process.poll() is None:
        process.terminate(); process.wait(timeout=5)
    for client in clients: client.close()
    Path(args.report).write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
