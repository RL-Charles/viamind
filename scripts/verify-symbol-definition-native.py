#!/usr/bin/env python3
"""Exercise stable native symbol definitions and bounded writer requests on an owned engine."""
import sys
sys.dont_write_bytecode = True
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
from contextlib import ExitStack

parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True)
parser.add_argument('--report', required=True)
parser.add_argument('--url', help='Owned production desktop only; starts a new project in this engine')
args = parser.parse_args()
from mcp_reference import StdioClient, digest, read_regular
binary = Path(args.binary).resolve(strict=True)
checks = []
report = {'passed': False, 'checks': checks, 'binary_sha256': digest(binary), 'transport': args.url or 'owned production headless stdio', 'scope': 'Detached single-unit rectangular symbol writer and borrowed predecode limits. No multi-unit, graphics or placed-instance updates.'}

def check(name, condition):
    if not condition:
        raise AssertionError(name)
    checks.append(name)

with tempfile.TemporaryDirectory(prefix='viamind-symbol-fields-') as temporary, ExitStack() as stack:
    directory = Path(temporary)
    if args.url:
        spec = importlib.util.spec_from_file_location('mcp_smoke', Path(__file__).with_name('mcp-smoke.py'))
        smoke = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(smoke)
        clients = [smoke.McpClient(args.url, 'symbol-identity-a'), smoke.McpClient(args.url, 'symbol-identity-b')]
        for client in clients: stack.callback(client.close)
        def call(name, arguments=None, error=False, peer=False):
            return clients[int(peer)].call(name, arguments or {}, expect_error=error)
        tools = clients[0].rpc('tools/list', {})['tools']
    else:
        client = stack.enter_context(StdioClient([str(binary), '--headless'], directory))
        client.initialize()
        tools = client.discover()['tools']
        def call(name, arguments=None, error=False, peer=False):
            return client.call(name, arguments or {}, expect_error=error)
    check('native symbol load/save and separate guarded placement advertised', {'load_symbol_template','save_symbol_template','place_symbol_template'} <= {t['name'] for t in tools})
    initial = call('get_session_state')
    call('new_project', {'name':'Original symbol writer proof','expected_cursor':initial['cursor'],'discard_dirty':True})
    native = directory / 'protected.viamind.json'
    call('save_project', {'path':str(native),'overwrite':False})
    native_bytes = read_regular(native)
    created = call('add_part', {'designator':'KEEP1','value':'dirty work retained','footprint':'Original','x_mm':1.000003,'y_mm':2.000007}, peer=bool(args.url))
    call('set_selection', {'ids':created['created']})
    before = call('get_session_state')
    status = call('get_document_status')
    source = Path(__file__).resolve().parents[1] / 'fixtures/symbols/original-regulator.viamind-symbol.json'
    source_bytes = read_regular(source)
    template = call('load_symbol_template', {'path':str(source)}, peer=bool(args.url))
    check('independent source load preserves dirty session', call('get_session_state') == before and call('get_document_status') == status and status['dirty'])
    template['name'] = 'Original retained symbol fields'
    template['value'] = 'Explicit detached odd Nm'
    template['pins'] = template['pins'][1:]
    template['pins'][0]['name'] = 'Retained input'
    template['pins'][0]['position'] = {'x':1000003,'y':-2000007}
    template['pins'][0]['electrical'] = 'input'
    path = directory / 'new-symbol.json'
    saved = call('save_symbol_template', {'template':template,'path':str(path)})
    bytes_before = read_regular(path)
    check('NEW definition reports exact explicit name/count/path', saved['name'] == template['name'] and saved['pin_count'] == len(template['pins']) and Path(saved['path']) == path)
    check('two-client/fresh stdio load retains exact ordered template', call('load_symbol_template', {'path':str(path)}, peer=bool(args.url)) == template)
    check('valid write leaves full dirty/native/shared selection unchanged', call('get_session_state') == before and call('get_document_status') == status and read_regular(native) == native_bytes)
    no_output = directory / 'never.json'
    cases = [
      ({'path':str(no_output),'template':{'name':'Explicit','pins':[None]*1025}}, '1024 pins before typed decoding'),
      ({'path':str(no_output),'template':{'name':'a'*4097,'pins':[None]}}, '4096 UTF-8 bytes per field'),
      ({'path':str(no_output),'template':{'name':'Explicit','pins':[{'number':'a'*4097}]}}, '4096 UTF-8 bytes per field'),
      ({'path':str(no_output),'template':{'name':'x','pins':[{'number':'','name':'a'*4096}]*64+[None]}}, '256 KiB aggregate'),
      ({'path':'a'*4097}, '4096 UTF-8 bytes before typed decoding'),
      ({'path':'é'*2049}, '4096 UTF-8 bytes before typed decoding'),
    ]
    for index,(arguments, expected) in enumerate(cases):
        response = call('save_symbol_template', arguments, error=True, peer=bool(args.url))
        check('borrowed limit precedes malformed decoding '+str(index), expected in json.dumps(response))
    for arguments in [
      {'template':template,'path':str(path)},
      {'template':template,'path':str(native)},
      {'template':template,'path':str(no_output),'extra':True},
      {'template':{'name':'Explicit','value':'','width':1000000,'height':1000000,'pins':[{'number':{'nested':'forbidden'}}]},'path':str(no_output)},
    ]:
        call('save_symbol_template', arguments, error=True)
        check('strict lifecycle/unknown/typed semantics preserve prior files '+str(len(checks)), call('get_session_state') == before and call('get_document_status') == status and not no_output.exists() and read_regular(path) == bytes_before and read_regular(native) == native_bytes)
    check('original source definition bytes preserved', read_regular(source) == source_bytes)
    check('all borrowed refusals leave complete session and status intact', call('get_session_state') == before and call('get_document_status') == status)
    if not args.url: client.finish()
check('retained production executable unchanged', digest(binary) == report['binary_sha256'])
report['passed'] = True
with open(args.report, 'x') as file:
    json.dump(report, file, indent=2)
    file.flush()
    os.fsync(file.fileno())
print(json.dumps({'passed':True,'checks':len(checks),'report':args.report}))
