"""Actual owned production HTTP/stdio attention proof; no OS input/cadence claim."""
import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import uuid

p = argparse.ArgumentParser(description=__doc__)
for name in ['binary', 'project', 'repo', 'output']:
    p.add_argument('--'+name, type=Path, required=True)
p.add_argument('--endpoint', required=True)
a = p.parse_args()
a.output.mkdir(parents=True, exist_ok=False)
sys.path.insert(0, str(a.repo/'scripts'))
from mcp_reference import StdioClient, digest
spec = importlib.util.spec_from_file_location('smoke', a.repo/'scripts/mcp-smoke.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
native = a.project.read_bytes()
r = {'format':'viamind-agent-presence-production-proof','version':1,'passed':False,'checks':[],
     'binary_sha256':digest(a.binary),'native_sha256':digest(a.project),'endpoint':a.endpoint,
     'scope':'Owned production desktop/two same-named HTTP sessions,32-lease capacity and fresh actual live/read-only stdio. No OS input or frame-cadence claim.'}
clients = []
def check(name, yes):
    assert yes, name
    r['checks'].append(name)
def content(snapshot):
    value = copy.deepcopy(snapshot)
    for agent in value['agents']:
        del agent['remaining_lease_ms']
    return value
def guard(state):
    return {'expected_project':state['project']['id'], 'expected_stream':state['cursor']['stream_id']}
def args(state):
    return dict(guard(state), cursor={'x':1,'y':-3}, selection=list(state['project']['board']['parts'])[:1], working_on='Checking route')
def query(c):
    return c.call('query_agent_presence', {})
try:
    x = m.McpClient(a.endpoint,'same #1 human-admin')
    y = m.McpClient(a.endpoint,'same #1 human-admin')
    clients += [x,y]
    initial = x.call('get_session_state', {})
    status = x.call('get_document_status', {})
    check('independent HTTP sessions share full current session', x.session != y.session and y.call('get_session_state', {}) == initial)
    check('initial attention is empty', not query(x)['agents'])
    first = x.call('set_agent_presence', args(initial))
    pair = y.call('set_agent_presence', args(initial))
    ownerx = first['agents'][0]['connection']
    ownery = next(v['connection'] for v in pair['agents'] if v['connection'] != ownerx)
    check('same untrusted names have distinct trusted numeric owners', len(pair['agents']) == 2 and pair['agents'][0]['display_name'] == pair['agents'][1]['display_name'])
    check('presence query reports actual current project and full event cursor', pair['project_id'] == initial['project']['id'] and pair['source_cursor'] == initial['cursor'])
    for label, patch in [
        ('65 malformed selected IDs count first', {'selection':[None]*65}),
        ('257 bytes before invalid cursor', {'working_on':'x'*257,'cursor':None}),
        ('81 characters', {'working_on':'a'*81}),
        ('UTF8 260 bytes', {'working_on':'🦀'*65}),
        ('control character', {'working_on':'a\nb'}),
        ('untrimmed label', {'working_on':' space'}),
        ('coordinate positive max+1', {'cursor':{'x':10_000_000_001,'y':0}}),
        ('coordinate negative max-1', {'cursor':{'x':-10_000_000_001,'y':0}}),
        ('fractional Nm', {'cursor':{'x':1.5,'y':0}}),
        ('nested unknown field', {'cursor':{'x':0,'y':0,'owner':ownerx}}),
        ('duplicate ID', {'selection':args(initial)['selection']*2}),
        ('unknown ID', {'selection':[str(uuid.uuid4())]}),
        ('forged owner', {'connection':ownerx}),
        ('stale stream', {'expected_stream':str(uuid.uuid4())}),
        ('stale project', {'expected_project':str(uuid.uuid4())}),
    ]:
        before = query(y)
        error = y.call('set_agent_presence', dict(args(initial), **patch), expect_error=True)
        check('rejects '+label, bool(error) and content(query(y)) == content(before))
    for field in ['connection','agent','clientInfo']:
        x.call('clear_agent_presence', dict(guard(initial), **{field:ownery}), expect_error=True)
        check('clear cannot forge '+field, len(query(x)['agents']) == 2)
    x.call('clear_agent_presence', guard(initial))
    check('own clear preserves peer', [v['connection'] for v in query(x)['agents']] == [ownery])
    x.call('clear_agent_presence', guard(initial))
    check('absent own clear is idempotent', len(query(y)['agents']) == 1)
    bounded = dict(args(initial), cursor={'x':10_000_000_000,'y':-10_000_000_000}, working_on='🦀'*64)
    y.call('set_agent_presence', bounded)
    check('exact coordinate and256-byte bounds accepted', query(y)['agents'][0]['cursor'] == bounded['cursor'])
    y.call('set_agent_presence', args(initial))
    old = query(y)['agents'][0]['remaining_lease_ms']
    check('observation never increases lease', query(x)['agents'][0]['remaining_lease_ms'] <= old)
    for n in range(31):
        c = m.McpClient(a.endpoint, 'capacity '+str(n))
        clients.append(c)
        c.call('set_agent_presence', args(initial))
    full = query(x)
    check('exact32 independent owned leases accepted', len(full['agents']) == 32)
    error = x.call('set_agent_presence', args(initial), expect_error=True)
    check('33rd lease refuses without eviction', '32 active' in error and content(query(y)) == content(full))
    y.call('set_agent_presence', dict(args(initial), working_on='Renewed at capacity'))
    check('existing participant can renew at capacity', len(query(y)['agents']) == 32 and any(v['working_on']=='Renewed at capacity' for v in query(y)['agents']))
    for c in clients:
        c.call('clear_agent_presence', guard(initial))
    check('all callers clear only owned leases', not query(x)['agents'])
    check('all attention operations preserve full session/status/native bytes', x.call('get_session_state', {}) == initial and x.call('get_document_status', {}) == status and a.project.read_bytes() == native)
    with StdioClient([str(a.binary.resolve()),'--headless','--open',str(a.project.resolve())], a.output, timeout=60) as c:
        c.initialize()
        st = c.call('get_session_state', {})
        c.call('set_agent_presence', args(st))
        check('actual live stdio owns one marker', len(query(c)['agents']) == 1)
        check('stdio presence preserves complete design state', c.call('get_session_state', {}) == st)
        c.call('open_project', {'path':str(a.project.resolve()), 'expected_cursor':st['cursor']})
        changed = c.call('get_session_state', {})
        check('same UUID reopen changes stream and invalidates attention', changed['project']['id'] == st['project']['id'] and changed['cursor']['stream_id'] != st['cursor']['stream_id'] and not query(c)['agents'])
        c.call('set_agent_presence', args(st), expect_error=True)
        check('reopened stream rejects stale attention guard', not query(c)['agents'])
        c.call('set_agent_presence', args(changed))
        c.call('new_project', {'expected_cursor':changed['cursor']})
        check('new project invalidates previous presence', not query(c)['agents'])
        c.finish()
    with StdioClient([str(a.binary.resolve()),'--headless','--agent-read-only','--open',str(a.project.resolve())], a.output, timeout=60) as c:
        c.initialize()
        tools = {t['name'] for t in c.rpc('tools/list',{})['tools']}
        check('read-only stdio admits observation and hides both writers', 'query_agent_presence' in tools and not {'set_agent_presence','clear_agent_presence'} & tools and not query(c)['agents'])
        for name in ['set_agent_presence','clear_agent_presence']:
            try:
                c.rpc('tools/call', {'name':name,'arguments':dict(args(initial),readOnlyHint=True,clientInfo={'name':'human-admin'})})
            except ValueError as e:
                check('read-only direct forged '+name+' refused at protocol boundary', 'MCP protocol error' in str(e))
            else:
                raise AssertionError(name+' admitted')
        c.finish()
    check('retained executable and native bytes unchanged', digest(a.binary)==r['binary_sha256'] and a.project.read_bytes()==native)
    r['passed'] = True
finally:
    for c in clients:
        try:
            if 'initial' in globals():
                c.call('clear_agent_presence', guard(initial))
        finally:
            c.close()
    (a.output/'verification.json').write_text(json.dumps(r,indent=2)+'\n')
print('Passed',len(r['checks']),'actual production presence checks')
