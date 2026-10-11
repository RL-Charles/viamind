#!/usr/bin/env python3
"""Bounded production CLI and actual two-client HTTP observation on owned inputs."""
import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import uuid
from contextlib import ExitStack
from urllib.parse import urlparse

sys.dont_write_bytecode = True
p = argparse.ArgumentParser()
p.add_argument('--repo', type=Path, required=True)
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--directory', type=Path, required=True)
p.add_argument('--url')
p.add_argument('--pid', type=int)
p.add_argument('--expected-native', type=Path)
a = p.parse_args()
sys.path.insert(0, str(a.repo / 'scripts'))
from mcp_reference import StdioClient, digest

root = a.directory.resolve()
root.mkdir()
checks = []
def check(label, condition):
    assert condition, label
    checks.append(label)
def publish(name, value):
    with (root / name).open('x') as f:
        json.dump(value, f, indent=2, ensure_ascii=False)
        f.write('\n')
def inventory(c):
    found = {}
    params = {}
    for _ in range(64):
        page = c.rpc('tools/list', params)
        for t in page['tools']:
            assert t['name'] not in found
            found[t['name']] = t
        if 'nextCursor' not in page:
            return found
        params = {'cursor': page['nextCursor']}
    raise AssertionError('tool list did not terminate')
def source(c):
    return (c.call('get_session_state', {}), c.call('get_document_status', {}), c.call('get_auto_drc_event_state', {}))
def verify_result(result, state):
    check('result has complete original project/full cursor', result['project_id'] == state[0]['project']['id'] and result['source_cursor'] == state[0]['cursor'])
    check('single response has all7 case-sensitive net definitions', [n['name'] for n in result['nets']] == ['CONNECTED', 'ONE', 'ORPHAN', 'RULE_ONLY', 'UNKNOWN', 'UNROUTED', 'VIA'])
    rows = {n['name']: n for n in result['nets']}
    check('interior horizontal pad attachment actually connects', rows['CONNECTED']['status'] == 'connected' and rows['CONNECTED']['terminal_islands'] == 1)
    check('same-net via annulus joins both actual sides', rows['VIA']['status'] == 'connected' and rows['VIA']['terminal_islands'] == 1)
    check('actual disjoint pads are unrouted without guides', rows['UNROUTED']['unrouted'] is True and rows['UNROUTED']['terminal_islands'] == 2)
    check('unsupported pose is unknown with null unrouted state', rows['UNKNOWN']['status'] == 'unknown' and rows['UNKNOWN']['unrouted'] is None and rows['UNKNOWN']['reasons'] == ['unsupported_pad'])
    check('one-terminal and copper-only/rule-only definitions disclose no assessed obligation', all(rows[n]['status'] == 'insufficient_terminals' and rows[n]['unrouted'] is False for n in ['ONE','ORPHAN','RULE_ONLY']))
    check('whole-board coverage and total statuses truthful', result['complete'] is False and (result['connected_count'], result['unrouted_count'], result['unknown_count'], result['insufficient_terminal_count']) == (2,1,1,3))
    check('no pagination or guide-derived output', not any(n in result for n in ['next_after','next_offset','guide_count']) and all('guide_count' not in n for n in result['nets']))

fixture = json.loads((a.repo / 'fixtures/connectivity/one-call.viamind.json').read_text())
native = root / 'protected.viamind.json'
with native.open('x') as f:
    json.dump(fixture, f, ensure_ascii=False)
    f.write('\n')
original = native.read_bytes()
baseline_rows = None
for mode, flags in [('live', []), ('approval', ['--agent-approval']), ('read_only', ['--agent-read-only'])]:
    with StdioClient([str(a.binary), '--headless', *flags, '--open', str(native)], a.repo) as c:
        c.initialize()
        tools = inventory(c)
        check(mode + ' discovers observation in actual startup mode', 'query_unrouted_nets' in tools and c.call('get_agent_mode', {})['mode'] == mode)
        schema = tools['query_unrouted_nets']['inputSchema']
        check(mode + ' strict parameterless schema', schema.get('additionalProperties') is False and schema.get('properties', {}) == {})
        state = source(c)
        result = c.call('query_unrouted_nets', {})
        verify_result(result, state)
        if baseline_rows is None:
            baseline_rows = result['nets']
        check(mode + ' identical rows across startup modes', result['nets'] == baseline_rows)
        check(mode + ' repeat observation is deterministic', c.call('query_unrouted_nets', {}) == result)
        c.call('query_unrouted_nets', {'limit': 1}, expect_error=True)
        c.call('query_unrouted_nets', {'path': str(root / 'forbidden')}, expect_error=True)
        check(mode + ' observations and invalid arguments preserve full session/status/autoDRC/native', source(c) == state and native.read_bytes() == original and not (root / 'forbidden').exists())
        publish(mode + '-result.json', result)
        c.finish()

# A valid detached native fixture beyond the old1000-net page, on owned input.
large = copy.deepcopy(fixture)
large['project']['board'] = {'parts': {}, 'tracks': {}, 'vias': {}}
prototype = next(iter(fixture['project']['board']['parts'].values()))
for i in range(1201):
    for terminal in range(2):
        part = copy.deepcopy(prototype)
        part['id'] = str(uuid.uuid5(uuid.NAMESPACE_URL, f'viamind-t0123-large-part-{i}-{terminal}'))
        part['pads'][0]['id'] = str(uuid.uuid5(uuid.NAMESPACE_URL, f'viamind-t0123-large-pad-{i}-{terminal}'))
        part['pads'][0]['net'] = f'N{i:04}'
        part['position'] = {'x': (i % 100) * 3_000_000, 'y': (i // 100) * 3_000_000 + terminal * 1_500_000}
        part['rotation_mdeg'] = 0
        large['project']['board']['parts'][part['id']] = part
large_path = root / 'large.viamind.json'
with large_path.open('x') as f:
    json.dump(large, f)
    f.write('\n')
large_original = large_path.read_bytes()
with StdioClient([str(a.binary), '--headless', '--agent-read-only', '--open', str(large_path)], a.repo) as c:
    c.initialize()
    state = source(c)
    result = c.call('query_unrouted_nets', {})
    check('one actual production call answers all1201 unrouted nets beyond old page cap', len(result['nets']) == 1201 and result['unrouted_count'] == 1201 and result['complete'] and result['unknown_count'] == 0 and all(n['unrouted'] is True and n['terminal_islands'] == 2 for n in result['nets']))
    check('large one-call result preserves source and full session/native', result['source_cursor'] == state[0]['cursor'] and source(c) == state and large_path.read_bytes() == large_original)
    publish('large-result.json', result)
    c.finish()

http = None
if a.url:
    assert a.pid and a.expected_native
    exe = Path(f'/proc/{a.pid}/exe').resolve(strict=True)
    check('actual desktop executable is exact supplied binary', exe == a.binary.resolve(strict=True) and digest(exe) == digest(a.binary))
    endpoint = urlparse(a.url)
    assert endpoint.scheme == 'http' and endpoint.hostname == '127.0.0.1' and endpoint.port and endpoint.path == '/mcp'
    sockets = set()
    for fd in Path(f'/proc/{a.pid}/fd').iterdir():
        try:
            target = fd.readlink().as_posix()
        except FileNotFoundError:
            continue
        if target.startswith('socket:['):
            sockets.add(target[8:-1])
    listener = []
    for table in ['tcp', 'tcp6']:
        for row in Path(f'/proc/{a.pid}/net/{table}').read_text().splitlines()[1:]:
            fields = row.split()
            if fields[3] == '0A' and int(fields[1].split(':')[1],16) == endpoint.port and fields[9] in sockets:
                listener.append(fields[9])
    check('actual desktop owns requested MCP listener', bool(listener))
    spec = importlib.util.spec_from_file_location('smoke', a.repo / 'scripts/mcp-smoke.py')
    smoke = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(smoke)
    with ExitStack() as stack:
        x, y = [smoke.McpClient(a.url, 'same unrouted observer') for _ in range(2)]
        for c in [x,y]:
            stack.callback(c.close)
        state = source(x)
        active = Path(state[1]['path'])
        check('desktop expected native identity is preserved', active.resolve(strict=True) == a.expected_native.resolve(strict=True))
        active_original = active.read_bytes()
        result = x.call('query_unrouted_nets', {})
        verify_result(result, state)
        check('two same-name desktop clients observe identical complete result and CLI rows', y.call('query_unrouted_nets', {}) == result and result['nets'] == baseline_rows)
        for c in [x,y]:
            c.call('query_unrouted_nets', {'limit':1}, expect_error=True)
            c.call('query_unrouted_nets', {'path':str(root/'forbidden')}, expect_error=True)
        check('actual desktop full session/status/autoDRC/native immutable', source(x) == source(y) == state and active.read_bytes() == active_original and not (root/'forbidden').exists())
        publish('desktop-result.json', result)
        http = {'pid':a.pid,'url':a.url,'listener_inodes':listener,'live_executable':str(exe),'native_path':str(active),'native_sha256':hashlib.sha256(active_original).hexdigest()}

publish('verification.json', {'passed':True,'checks':checks,'binary_sha256':digest(a.binary),'fixture_sha256':hashlib.sha256(original).hexdigest(),'http':http,'scope':'Actual three startup-mode stdio processes and >1000-net production one-call case; optional actual same-name two-client native HTTP observation. No model edits or OS input.'})
print(json.dumps({'passed':True,'checks':len(checks),'http':http is not None}))
