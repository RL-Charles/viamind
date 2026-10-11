#!/usr/bin/env python3
"""Check both symbol publication tools against an explicitly owned scratch engine."""
import sys
sys.dont_write_bytecode = True
import argparse
from contextlib import ExitStack
import importlib.util
import json
import os
from pathlib import Path
import tempfile

from mcp_reference import StdioClient, digest, read_regular

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--report', type=Path, required=True)
parser.add_argument('--url', help='Owned scratch desktop only; replaces this engine project')
args = parser.parse_args()
binary = args.binary.resolve(strict=True)
checks = []
report = {'passed':False,'checks':checks,'binary_sha256':digest(binary),'transport':args.url or 'owned production headless stdio','scope':'Actual bounded native save/export and strict refusal/session preservation. Deterministic between-stage lifecycle races are separate engine tests, not a production scheduling claim.'}

def check(name, condition):
    if not condition:
        raise AssertionError(name)
    checks.append(name)

with tempfile.TemporaryDirectory(prefix='viamind-symbol-publication-') as temporary, ExitStack() as stack:
    directory = Path(temporary)
    if args.url:
        spec = importlib.util.spec_from_file_location('mcp_smoke', Path(__file__).with_name('mcp-smoke.py'))
        smoke = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(smoke)
        clients = [smoke.McpClient(args.url,'symbol-publication-a'),smoke.McpClient(args.url,'symbol-publication-b')]
        for client in clients:
            stack.callback(client.close)
        def call(name, arguments=None, error=False, peer=False):
            return clients[int(peer)].call(name, arguments or {}, expect_error=error)
        tools = clients[0].rpc('tools/list',{})['tools']
    else:
        client = stack.enter_context(StdioClient([str(binary),'--headless'],directory))
        client.initialize()
        tools = client.discover()['tools']
        def call(name, arguments=None, error=False, peer=False):
            return client.call(name,arguments or {},expect_error=error)
    check('both writers and native loader advertised', {'save_symbol_template','export_symbol_template','load_symbol_template'} <= {tool['name'] for tool in tools})
    fixture = Path(__file__).resolve().parents[1]/'fixtures/schematic/regulator.viamind.json'
    source_bytes = read_regular(fixture)
    native = directory/'owned-regulator.viamind.json'
    native.write_bytes(source_bytes)
    initial = call('get_session_state')
    call('open_project',{'path':str(native),'expected_cursor':initial['cursor'],'discard_dirty':True})
    created = call('add_part',{'designator':'KEEP1','value':'dirty peer work','footprint':'Original','x_mm':1.000003,'y_mm':-2.000007},peer=bool(args.url))
    call('set_selection',{'ids':created['created']})
    before = call('get_session_state')
    status = call('get_document_status')
    check('owned native fixture opens with dirty peer work retained', status['dirty'] and read_regular(native)==source_bytes)
    sheet = next(sheet for sheet in before['project']['schematic']['sheets'].values() if (sheet.get('content') or {}).get('symbols'))
    symbol = next(symbol for symbol in sheet['content']['symbols'].values() if symbol['designator']=='U1')
    expected = {key:symbol[key] for key in ['value','width','height']}
    expected['footprint'] = symbol.get('footprint')
    expected['name'] = 'Original guarded publication'
    expected['pins'] = [{key:pin[key] for key in ['number','name','position','electrical']} for pin in symbol['pins']]
    output = directory/'exported-symbol.json'
    proposal = {'expected_project':before['project']['id'],'source_cursor':before['cursor'],'sheet_id':sheet['id'],'symbol_id':symbol['id'],'name':expected['name'],'path':str(output)}
    result = call('export_symbol_template',proposal)
    exported_bytes = read_regular(output)
    check('export reports exact captured cursor/name/count/path',result['exported_cursor']==before['cursor'] and result['name']==expected['name'] and result['pin_count']==len(expected['pins']) and Path(result['path'])==output)
    check('peer loader returns exact detached ordered source fields',call('load_symbol_template',{'path':str(output)},peer=bool(args.url))==expected)
    check('export retains complete dirty session/status/native bytes',call('get_session_state')==before and call('get_document_status')==status and read_regular(native)==source_bytes)
    detached = directory/'detached-symbol.json'
    saved = call('save_symbol_template',{'template':expected,'path':str(detached)},peer=bool(args.url))
    check('detached NEW save retains original exact native bytes',read_regular(detached)==exported_bytes and Path(saved['path'])==detached)
    check('detached save preserves complete dirty session and status',call('get_session_state')==before and call('get_document_status')==status)
    for tool,arguments in [
        ('export_symbol_template',{**proposal,'path':str(native)}),
        ('save_symbol_template',{'template':expected,'path':str(native)}),
        ('export_symbol_template',proposal),
        ('save_symbol_template',{'template':expected,'path':str(detached)}),
    ]:
        refused = call(tool,arguments,error=True,peer=bool(args.url))
        check(tool+' native/collision refusal '+str(len(checks)),bool(refused) and call('get_session_state')==before and call('get_document_status')==status and read_regular(native)==source_bytes and read_regular(output)==exported_bytes and read_regular(detached)==exported_bytes)
    for tool in ['save_symbol_template','export_symbol_template']:
        alias = directory/(tool+'-native-alias.json')
        alias.symlink_to(native)
        arguments = {**proposal,'path':str(alias)} if tool=='export_symbol_template' else {'template':expected,'path':str(alias)}
        refused = call(tool,arguments,error=True)
        check(tool+' canonical native alias rejected', 'active native' in json.dumps(refused) and alias.is_symlink() and read_regular(native)==source_bytes and call('get_session_state')==before and call('get_document_status')==status)
    call('add_part',{'designator':'PEER2','value':'newer work','footprint':'Original','x_mm':3,'y_mm':4},peer=bool(args.url))
    changed = call('get_session_state')
    changed_status = call('get_document_status')
    stale_path = directory/'never-stale.json'
    stale = call('export_symbol_template',{**proposal,'path':str(stale_path)},error=True)
    check('stale export cursor rejects before destination publication','document changed' in json.dumps(stale) and not stale_path.exists())
    check('stale refusal preserves newer peer session/status/files',call('get_session_state')==changed and call('get_document_status')==changed_status and read_regular(native)==source_bytes and read_regular(output)==exported_bytes)
    check('original repository fixture bytes preserved',read_regular(fixture)==source_bytes)
    if not args.url:
        client.finish()
check('retained production executable unchanged',digest(binary)==report['binary_sha256'])
report['passed']=True
with args.report.open('x') as output:
    json.dump(report,output,indent=2)
    output.flush()
    os.fsync(output.fileno())
print(json.dumps({'passed':True,'checks':len(checks),'report':str(args.report)}))
