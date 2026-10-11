#!/usr/bin/env python3
"""Exercise detached native files and separate guarded reuse on an owned engine."""
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
report = {'passed': False, 'checks': checks, 'binary_sha256': digest(binary), 'transport': args.url or 'owned production headless stdio', 'scope': 'Detached rectangular SMD local geometry; separate guarded component reuse. No graphics, paste/mask, courtyard or fabrication checks.'}

def check(name, condition):
    if not condition:
        raise AssertionError(name)
    checks.append(name)

with tempfile.TemporaryDirectory(prefix='viamind-definition-native-') as temporary, ExitStack() as stack:
    directory = Path(temporary)
    if args.url:
        spec = importlib.util.spec_from_file_location('mcp_smoke', Path(__file__).with_name('mcp-smoke.py'))
        smoke = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(smoke)
        clients = [smoke.McpClient(args.url, 'definition-native-a'), smoke.McpClient(args.url, 'definition-native-b')]
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
    check('detached load/save and separate guarded reuse tools discovered', {'load_footprint_definition', 'save_footprint_definition', 'import_footprint'} <= {t['name'] for t in tools})
    initial = call('get_session_state')
    call('new_project', {'name': 'Original detached definition proof', 'expected_cursor': initial['cursor'], 'discard_dirty': True})
    native = directory / 'native.viamind.json'
    call('save_project', {'path': str(native), 'overwrite': False})
    native_bytes = read_regular(native)
    created = call('add_part', {'designator': 'KEEP1', 'value': 'explicit dirty work', 'footprint': 'Old', 'x_mm': 3.000003, 'y_mm': 4.000007}, peer=bool(args.url))
    part_id = created['created'][0]
    original = call('get_design')['board']['parts'][part_id]
    call('set_part_pads', {'id': part_id, 'expected': original, 'pads': [{'number': '1', 'x_mm': 0, 'y_mm': 0, 'width_mm': 1, 'height_mm': 1, 'net': 'VIN'}]}, peer=bool(args.url))
    call('set_selection', {'ids': [part_id]})
    before = call('get_session_state')
    status = call('get_document_status')
    check('dirty native work and shared selection established', status['dirty'] and before['selection']['ids'] == [part_id])
    footprint = {'name': 'Original odd Nm definition', 'pads': [
        {'number': '1', 'position': {'x': -500003, 'y': 250001}, 'width': 1200003, 'height': 800001},
        {'number': '2', 'position': {'x': 750007, 'y': -125003}, 'width': 600003, 'height': 1000001},
    ]}
    path = directory / 'new-footprint.json'
    input = {'footprint': footprint, 'path': str(path)}
    saved = call('save_footprint_definition', input)
    check('new named native definition published with exact two-pad count', saved['path'] == str(path) and saved['name'] == footprint['name'] and saved['pad_count'] == 2)
    library_bytes = read_regular(path)
    check('strict native envelope preserves exact asymmetric odd-Nm centers and dimensions', json.loads(library_bytes) == {'format': 'viamind-footprint', 'version': 1, 'footprint': footprint})
    check('second client reads the same detached definition', call('load_footprint_definition', {'path': str(path)}, peer=bool(args.url)) == footprint)
    check('detached file operations preserve complete session and document status', call('get_session_state') == before and call('get_document_status') == status)
    call('save_footprint_definition', input, error=True, peer=bool(args.url))
    check('repeated NEW publication preserves previous bytes', read_regular(path) == library_bytes)
    call('load_footprint_definition', {'path': str(path), 'unexpected': True}, error=True)
    call('save_footprint_definition', dict(input, unexpected=True), error=True)
    check('unknown request fields reject without changing session', call('get_session_state') == before)
    for name, mutate in [('duplicate number', lambda f: f['pads'][1].update(number='1')), ('zero dimension', lambda f: f['pads'][0].update(width=0)), ('coordinate overflow', lambda f: f['pads'][0]['position'].update(x=10000000001)), ('unknown pad semantics', lambda f: f['pads'][0].update(net='VIN'))]:
        invalid = json.loads(json.dumps(footprint)); mutate(invalid)
        destination = directory / (name.replace(' ', '-') + '.json')
        call('save_footprint_definition', {'footprint': invalid, 'path': str(destination)}, error=True)
        check(name + ' blocks publication unchanged', not destination.exists() and call('get_session_state') == before)
    alias = directory / 'alias.json'; alias.symlink_to(path)
    call('save_footprint_definition', {'footprint': footprint, 'path': str(alias)}, error=True)
    call('save_footprint_definition', {'footprint': footprint, 'path': str(native)}, error=True)
    check('symlink and active native destinations preserve bytes', alias.is_symlink() and read_regular(path) == library_bytes and read_regular(native) == native_bytes)
    fifo = directory / 'fifo'; os.mkfifo(fifo)
    call('load_footprint_definition', {'path': str(fifo)}, error=True)
    call('load_footprint_definition', {'path': 'x' * 4097}, error=True)
    check('FIFO and oversized paths reject without mutating work', call('get_session_state') == before and call('get_document_status') == status)
    empty = directory / 'empty.json'
    call('save_footprint_definition', {'footprint': {'name': 'Explicit empty', 'pads': []}, 'path': str(empty)})
    check('explicit named empty definition remains valid', call('load_footprint_definition', {'path': str(empty)}) == {'name': 'Explicit empty', 'pads': []})
    old = before['project']['board']['parts'][part_id]
    apply = {'expected_project': before['project']['id'], 'expected': old, 'path': str(path)}
    call('import_footprint', apply)
    after = call('get_design')['board']['parts'][part_id]
    check('explicit reuse retains existing numbered pad UUID and net', after['pads'][0]['id'] == old['pads'][0]['id'] and after['pads'][0]['net'] == 'VIN')
    check('new numbered pad has fresh identity and no inferred net', after['pads'][1]['id'] != old['pads'][0]['id'] and after['pads'][1]['net'] is None)
    check('exact local geometry and component placement retained', after['pads'][0]['position'] == footprint['pads'][0]['position'] and after['pads'][0]['width'] == 1200003 and after['position'] == old['position'])
    checkpoint = call('get_session_state')
    call('import_footprint', apply, error=True, peer=bool(args.url))
    check('stale exact part guard rejects atomically', call('get_session_state') == checkpoint)
    call('undo')
    check('one explicit reuse undo restores exact original project', call('get_design') == before['project'])
    call('redo')
    check('redo restores exact reused component identities', call('get_design')['board']['parts'][part_id] == after)
    check('detached library and saved native bytes unchanged through reuse', read_regular(path) == library_bytes and read_regular(native) == native_bytes)
    report['native_library'] = json.loads(library_bytes)
    report['native_library_sha256'] = hashlib.sha256(library_bytes).hexdigest()
    if not args.url:
        client.finish(); check('owned production child exits cleanly', client.process.returncode == 0)
    check('production executable digest unchanged', digest(binary) == report['binary_sha256'])
    report['passed'] = True
with open(args.report, 'x') as stream:
    json.dump(report, stream, indent=2); stream.write('\n'); stream.flush(); os.fsync(stream.fileno())
print(json.dumps({'passed': True, 'checks': len(checks), 'report': args.report}))
