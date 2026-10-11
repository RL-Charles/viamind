#!/usr/bin/env python3
"""Verify resumable DRC observations against an explicitly owned scratch engine."""
import sys
sys.dont_write_bytecode = True
import argparse
from contextlib import ExitStack
import importlib.util
import json
import os
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--repo', type=Path, required=True)
parser.add_argument('--directory', type=Path, required=True)
parser.add_argument('--url', help='Owned scratch desktop only: replaces its document')
args = parser.parse_args()
sys.path.insert(0, str(args.repo/'scripts'))
from mcp_reference import StdioClient, digest, read_regular
binary = args.binary.resolve(strict=True)
root = args.directory.resolve()
root.mkdir()
checks = []
report = {'passed': False, 'checks': checks, 'binary_sha256': digest(binary), 'transport': args.url or 'actual headless stdio', 'scope':'Separate bounded automatic DRC summaries. Normal transport checks do not claim deterministic worker scheduling; blocked-worker races have separate engine test provenance.'}
def check(name, condition):
    assert condition, name
    checks.append(name)

with ExitStack() as stack:
    if args.url:
        spec = importlib.util.spec_from_file_location('mcp_smoke', args.repo/'scripts/mcp-smoke.py')
        smoke = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(smoke)
        clients = [smoke.McpClient(args.url, 'DRC observer'), smoke.McpClient(args.url, 'DRC observer')]
        for client in clients: stack.callback(client.close)
        def call(name, arguments=None, error=False, peer=False):
            return clients[int(peer)].call(name, arguments or {}, expect_error=error)
        tools = clients[0].rpc('tools/list', {})['tools']
    else:
        client = stack.enter_context(StdioClient([str(binary), '--headless'], root, timeout=120))
        client.initialize()
        tools = client.discover()['tools']
        def call(name, arguments=None, error=False, peer=False):
            return client.call(name, arguments or {}, expect_error=error)
    names = {tool['name'] for tool in tools}
    check('actual full application advertises both DRC observations', {'get_auto_drc_event_state','watch_auto_drc_events','configure_auto_drc'} <= names)
    initial = call('get_session_state')
    call('new_project', {'name':'Original DRC observer proof','expected_cursor':initial['cursor'],'discard_dirty':True})
    native = root/'owned.viamind.json'
    call('save_project', {'path':str(native),'overwrite':False})
    native_bytes = read_regular(native)
    before = call('get_session_state')
    status = call('get_document_status')
    baseline = call('get_auto_drc_event_state')
    check('atomic baseline has independent controller cursor', baseline['cursor']['stream_id'] != before['cursor']['stream_id'] and baseline['status']['phase']=='disabled')
    def terminal(after, generation):
        for _ in range(10):
            batch = call('watch_auto_drc_events', {'after':after,'timeout_ms':1000}, peer=True)
            after = batch['cursor']
            for event in batch['events']:
                if event['generation']==generation and event['phase'] in ['ready','skipped','failed']:
                    return event, after
        raise AssertionError('terminal observation timeout')
    configured = call('configure_auto_drc', {'enabled':True,'debounce_ms':0})
    generation = configured['generation']
    event, tail = terminal(baseline['cursor'], generation)
    check('ready event declares original captured source and exact scoped scalar facts', event['phase']=='ready' and event['source_cursor']==before['cursor'] and event['report']=={'violation_count':0,'truncated':False,'has_incomplete_reason':False,'excluded_part_count':0,'explicit_outline_checked':False})
    page = call('watch_auto_drc_events', {'after':baseline['cursor'],'limit':1})
    check('one-row page retains Pending and has_more', len(page['events'])==1 and page['events'][0]['phase']=='pending' and page['has_more'])
    if args.url:
        clients[0].close()
        clients[0] = smoke.McpClient(args.url, 'DRC observer')
        stack.callback(clients[0].close)
    rest = call('watch_auto_drc_events', {'after':page['cursor']})
    check('independent/reconnected observer resumes Running then Ready exactly', [row['phase'] for row in rest['events']]==['running','ready'] and rest['events'][-1]==event and rest['cursor']==tail and not rest['has_more'])
    current = call('get_auto_drc_event_state')
    check('terminal event and atomic current status/cursor agree', current['cursor']==tail and current['status']['generation']==generation and current['status']['source_cursor']==event['source_cursor'])
    check('configuration and every observation preserve full document/native/status', call('get_session_state')==before and call('get_document_status')==status and read_regular(native)==native_bytes)
    for index, bad in enumerate([{'limit':0},{'limit':129},{'timeout_ms':30001},{'extra':True},{'after':{'stream_id':before['cursor']['stream_id'],'sequence':0}},{'after':dict(tail,sequence=tail['sequence']+1)},{'after':dict(tail,extra=True)}]):
        error = call('watch_auto_drc_events',bad,error=True)
        check('strict bound/cursor refusal '+str(index), bool(error) and call('get_auto_drc_event_state')==current and call('get_session_state')==before)
    call('get_auto_drc_event_state', {'extra':True}, error=True)
    call('configure_auto_drc', {'max_objects':0}, error=True)
    check('invalid baseline/config emits no event or generation', call('get_auto_drc_event_state')==current)
    empty = call('watch_auto_drc_events', {'after':tail,'timeout_ms':1})
    check('bounded empty timeout keeps requested cursor', empty=={'events':[],'cursor':tail,'has_more':False})
    via = call('add_via', {'x_mm':10,'y_mm':10,'diameter_mm':0.6,'drill_mm':0.1,'net':'A'}, peer=True)
    changed = call('get_session_state')
    event, tail = terminal(tail, generation+1)
    check('peer edit emits current generation/source and actual violation count', event['phase']=='ready' and event['source_cursor']==changed['cursor'] and event['report']['violation_count']==1)
    current = call('get_auto_drc_event_state')
    call('set_selection', {'ids':via['created']}, peer=True)
    selected = call('get_session_state')
    check('selection changes design cursor without rerunning DRC', selected['cursor']!=changed['cursor'] and call('get_auto_drc_event_state')==current)
    call('undo', {}, peer=True)
    event, tail = terminal(tail, generation+2)
    check('peer undo returns exact original geometry and zero violations', event['phase']=='ready' and event['report']['violation_count']==0 and call('get_session_state')['project']==before['project'])
    cursor = call('get_session_state')['cursor']
    call('open_project', {'path':str(native),'discard_dirty':True,'expected_cursor':cursor}, peer=True)
    event, tail = terminal(tail, generation+3)
    reopened = call('get_session_state')
    check('same-UUID native reopen changes design stream and preserves DRC stream', reopened['project']['id']==before['project']['id'] and reopened['cursor']['stream_id']!=cursor['stream_id'] and event['source_cursor']==reopened['cursor'] and tail['stream_id']==baseline['cursor']['stream_id'])
    call('configure_auto_drc', {'enabled':False})
    disabled = call('get_auto_drc_event_state')
    for _ in range(129):call('configure_auto_drc', {'enabled':False})
    state = call('get_auto_drc_event_state')
    expired = call('watch_auto_drc_events', {'after':disabled['cursor']}, error=True)
    check('129 accepted config transitions expire old resume cursor explicitly', 'expired' in json.dumps(expired) and state['cursor']['sequence']==disabled['cursor']['sequence']+129)
    floor = dict(state['cursor'],sequence=state['cursor']['sequence']-128)
    retained = call('watch_auto_drc_events', {'after':floor,'limit':128})
    check('exact retention floor returns128 summaries and final cursor', len(retained['events'])==128 and retained['events'][0]['sequence']==floor['sequence']+1 and retained['cursor']==state['cursor'] and not retained['has_more'] and all(row['phase']=='disabled' and row['report'] is None for row in retained['events']))
    check('all observations retain reopened complete document/native/status', call('get_session_state')==reopened and call('get_document_status')==status and read_regular(native)==native_bytes)
    if not args.url:client.finish()
    with StdioClient([str(binary),'--headless','--agent-read-only','--open',str(native)],root) as readonly:
        readonly.initialize()
        tools = {tool['name'] for tool in readonly.rpc('tools/list',{})['tools']}
        check('actual read-only stdio advertises both observers and hides configure', {'get_auto_drc_event_state','watch_auto_drc_events'}<=tools and 'configure_auto_drc' not in tools)
        state = readonly.call('get_auto_drc_event_state')
        check('read-only stdio state and tail observation preserve disabled baseline', state['status']['phase']=='disabled' and readonly.call('watch_auto_drc_events',{'after':state['cursor']})=={'events':[],'cursor':state['cursor'],'has_more':False})
        rejected = False
        try:readonly.rpc('tools/call',{'name':'configure_auto_drc','arguments':{'enabled':True},'_meta':{'readOnlyHint':False}})
        except ValueError:rejected=True
        check('forged read-only writer request refused before configure', rejected and readonly.call('get_auto_drc_event_state')==state)
        readonly.finish()
check('retained production executable and native bytes unchanged', digest(binary)==report['binary_sha256'] and read_regular(native)==native_bytes)
report.update(passed=True,check_count=len(checks),native=str(native),native_sha256=digest(native))
with (root/'verification.json').open('x') as output:
    json.dump(report,output,indent=2);output.flush();os.fsync(output.fileno())
print(json.dumps({'passed':True,'checks':len(checks),'directory':str(root)}))
