#!/usr/bin/env python3
"""Owned explicit bus unfold production stdio/HTTP proof; no OS input."""
import argparse, copy, hashlib, importlib.util, json, sys
from contextlib import ExitStack
from pathlib import Path
from urllib.parse import urlparse
sys.dont_write_bytecode=True
p=argparse.ArgumentParser()
for arg in ['repo','binary','directory']: p.add_argument('--'+arg,type=Path,required=True)
p.add_argument('--url');p.add_argument('--pid',type=int);p.add_argument('--expected-native',type=Path)
a=p.parse_args();sys.path.insert(0,str(a.repo/'scripts'))
from mcp_reference import StdioClient,digest
root=a.directory.resolve();root.mkdir();checks=[]
def publish(name,value):
    with (root/name).open('x') as f: json.dump(value,f,indent=2,ensure_ascii=False);f.write('\n')
def check(label,value):
    assert value,label
    checks.append(label)
def state(c): return (c.call('get_session_state',{}),c.call('get_document_status',{}),c.call('get_auto_drc_event_state',{}))
def ids(n): return f'00000000-0000-0000-0000-{n:012x}'
def request(s):return {'source':{'expected_project':s[0]['project']['id'],'source_cursor':s[0]['cursor'],'sheet_id':ids(0x1271)},'bus_id':ids(0x1272),'member':'DATA0','bus_point':{'x':0,'y':0},'wire_point':{'x':2000000,'y':2000000},'wire_end':{'x':2000000,'y':8000000}}

def inventory(c):
    found={};params={}
    for _ in range(64):
        page=c.rpc('tools/list',params)
        for t in page['tools']: assert t['name'] not in found;found[t['name']]=t
        if 'nextCursor' not in page:return found
        params={'cursor':page['nextCursor']}
    raise AssertionError('unterminated catalog')
def verify(r,s,label):
    check(label+' exact source project/full cursor',r['expected_project']==s[0]['project']['id'] and r['source_cursor']==s[0]['cursor'])
    check(label+' explicit exact member and three signed integer coordinates',r['member']=='DATA0' and r['bus_point']=={'x':0,'y':0} and r['wire_point']=={'x':2000000,'y':2000000} and r['wire_end']=={'x':2000000,'y':8000000})
    check(label+' independently declared two allowed original scalar components',r['allowed_original_component_count']==2 and r['original_graph_members']==3)
    check(label+' complete actual UUID partition proof and honest scope',r['unrelated_partitions_preserved'] is True and 0<r['exact_candidate_visits']<=1000000 and 'graphics do not conduct' in r['scope'])
def query(c,s,label):
    r=c.call('query_schematic_buses',{})
    check(label+' one-call complete actual member inventory',r['complete'] is True and r['source_cursor']==s[0]['cursor'] and r['total_buses']==1 and r['total_entries']==0 and r['total_expanded_members']==2)
    members=r['buses'][0]['members']
    check(label+' ascending expansion and exact actual Local-label identity',[(x['name'],x['missing']) for x in members]==[('DATA0',False),('DATA1',True)] and members[0]['labels'][0]['label_id']==ids(0x1274) and members[0]['labels'][0]['scope']=='local')
    return r
def partition(graph,original=None):
    assert graph.get('next_offset') is None, 'independent fixture graph must be complete'
    assert sum(len(net['members']) for net in graph['nets'])==graph['coverage']['members'], 'all graph members must be reported'
    groups=[];anchors=set()
    for net in graph['nets']:
        members=sorted((m['sheet_id'],m['id'],m['kind']) for m in net['members'] if original is None or m['id'] in original)
        anchors.update(m['id'] for m in net['members'])
        names=sorted(json.dumps(n,sort_keys=True) for n in net['names'])
        groups.append((members,names))
    return sorted(groups),anchors
fixture=a.repo/'fixtures/schematic/buses.viamind.json'
native=root/'protected.viamind.json';native.write_bytes(fixture.read_bytes());original=native.read_bytes()
for mode,flags in [('live',[]),('approval',['--agent-approval']),('read_only',['--agent-read-only'])]:
    with StdioClient([str(a.binary),'--headless',*flags,'--open',str(native)],a.repo) as c:
        c.initialize();tools=inventory(c)
        check(mode+' initialized discovery and default writer admission','preview_schematic_bus_unfold' in tools and ('unfold_schematic_bus_member' in tools)==(mode=='live'))
        schema=tools['preview_schematic_bus_unfold']['inputSchema']
        check(mode+' strict required full-guard typed schema',schema.get('additionalProperties') is False and set(schema['required'])=={'source','bus_id','member','bus_point','wire_point','wire_end'})
        before=state(c);query(c,before,mode);args=request(before);result=c.call('preview_schematic_bus_unfold',args);verify(result,before,mode)
        check(mode+' repeated preview deterministic and complete session/status/autoDRC immutable',c.call('preview_schematic_bus_unfold',args)==result and state(c)==before and native.read_bytes()==original)
        for invalid in [dict(args,pass_flag=True),dict(args,wire_point={'x':1.5,'y':0}),dict(args,source=dict(args['source'],source_cursor={'stream_id':ids(999),'sequence':before[0]['cursor']['sequence']}))]:c.call('preview_schematic_bus_unfold',invalid,expect_error=True)
        check(mode+' invalid fields fractional coordinate and wrong stream refuse unchanged',state(c)==before and native.read_bytes()==original)
        if mode=='live':
            graph=c.call('query_schematic_connectivity',{'offset':0,'limit':100});oldgroups,anchors=partition(graph)
            c.call('unfold_schematic_bus_member',args);after=state(c)
            check('Live actual atomic one-event unfolding preserves entire PCB',after[0]['cursor']['sequence']==before[0]['cursor']['sequence']+1 and after[0]['project']['board']==before[0]['project']['board'])
            graph_after=c.call('query_schematic_connectivity',{'offset':0,'limit':100})
            original_groups=[sorted(m['id'] for m in n['members'] if m['id'] in anchors) for n in graph_after['nets']]
            check('Live independently declared original UUID union and unrelated component preservation',sorted(g for g in original_groups if g)==[[ids(0x1273),ids(0x1274)],[ids(0x1275)]])
            content=after[0]['project']['schematic']['sheets'][ids(0x1271)]['content'];old=before[0]['project']['schematic']['sheets'][ids(0x1271)]['content']
            check('Live exact original objects preserved plus one entry/wire/Local label',all(content.get(k,{})==old.get(k,{}) for k in ['symbols','junctions','packages','buses']) and all(content['labels'][k]==v for k,v in old['labels'].items()) and all(content['wires'][k]==v for k,v in old['wires'].items()) and len(content['bus_entries'])==1 and len(content['wires'])==2 and len(content['labels'])==3)
            entry=next(iter(content['bus_entries'].values()))
            check('Live complete explicit scalar ownership identities',entry['wire_id'] in content['wires'] and entry['label_id'] in content['labels'] and entry['bus_id']==ids(0x1272) and content['labels'][entry['label_id']]['scope']=='local' and content['labels'][entry['label_id']]['name']=='DATA0')
            c.call('unfold_schematic_bus_member',args,expect_error=True);check('Live old cursor refuses after successful apply',state(c)==after)
            c.call('undo',{});check('Live origin-owned undo restores exact entire project',state(c)[0]['project']==before[0]['project'])
            c.call('redo',{});check('Live origin-owned redo restores exact entire candidate',state(c)[0]['project']==after[0]['project'])
            c.call('undo',{})
        publish(mode+'-preview.json',result);c.finish()
check('all stdio modes preserve active native file exact bytes',native.read_bytes()==original)
http=None
if a.url:
    assert a.pid and a.expected_native
    exe=Path(f'/proc/{a.pid}/exe').resolve(strict=True)
    check('actual desktop live executable equals exact retained production binary',exe==a.binary.resolve(strict=True) and digest(exe)==digest(a.binary))
    endpoint=urlparse(a.url);assert endpoint.scheme=='http' and endpoint.hostname=='127.0.0.1' and endpoint.port and endpoint.path=='/mcp'
    sockets=set()
    for fd in Path(f'/proc/{a.pid}/fd').iterdir():
        try: target=fd.readlink().as_posix()
        except FileNotFoundError:continue
        if target.startswith('socket:['):sockets.add(target[8:-1])
    listeners=[]
    for table in ['tcp','tcp6']:
        for line in Path(f'/proc/{a.pid}/net/{table}').read_text().splitlines()[1:]:
            cols=line.split()
            if cols[3]=='0A' and int(cols[1].split(':')[1],16)==endpoint.port and cols[9] in sockets:listeners.append(cols[9])
    check('actual desktop owns requested MCP listener',bool(listeners))
    spec=importlib.util.spec_from_file_location('smoke',a.repo/'scripts/mcp-smoke.py');smoke=importlib.util.module_from_spec(spec);spec.loader.exec_module(smoke)
    with ExitStack() as stack:
        x,y=[smoke.McpClient(a.url,'same explicit bus observer') for _ in range(2)]
        for c in [x,y]:stack.callback(c.close)
        before=state(x);check('actual desktop expected native identity and original fixture',Path(before[1]['path']).resolve(strict=True)==a.expected_native.resolve(strict=True) and before[0]['project']==json.loads(original)['project'])
        protected=a.expected_native.read_bytes();query(x,before,'HTTP');args=request(before);r=x.call('preview_schematic_bus_unfold',args);verify(r,before,'HTTP')
        check('actual same-name two-client preview exact equality and full source immutability',y.call('preview_schematic_bus_unfold',args)==r and state(x)==state(y)==before)
        x.call('add_part',{'designator':'R_PEER','value':'','footprint':'F','x_mm':0,'y_mm':0});peer=state(y)
        y.call('unfold_schematic_bus_member',args,expect_error=True);check('actual peer edit stale full-cursor apply refuses exact state',state(x)==state(y)==peer)
        x.call('undo',{});check('actual peer origin undo restores entire source project',state(x)[0]['project']==before[0]['project'])
        fresh=state(y);command=request(fresh);y.call('preview_schematic_bus_unfold',command);y.call('unfold_schematic_bus_member',command);after=state(x)
        check('actual desktop guarded Apply is exactly one event and unchanged PCB',after[0]['cursor']['sequence']==fresh[0]['cursor']['sequence']+1 and after[0]['project']['board']==before[0]['project']['board'])
        y.call('undo',{});final=state(x)
        check('actual desktop own undo restores entire project/native identity and bytes',final[0]['project']==before[0]['project'] and final[1]['path']==before[1]['path'] and a.expected_native.read_bytes()==protected)
        check('actual desktop source cursor advancement honestly retained',final[0]['cursor']['stream_id']==before[0]['cursor']['stream_id'] and final[0]['cursor']['sequence']==before[0]['cursor']['sequence']+4)
        publish('http-preview.json',r)
        http={'pid':a.pid,'url':a.url,'executable':str(exe),'listener_inodes':listeners,'before_cursor':before[0]['cursor'],'final_cursor':final[0]['cursor'],'native':str(a.expected_native),'native_sha256':digest(a.expected_native)}
publish('verification.json',{'passed':True,'checks':checks,'binary_sha256':digest(a.binary),'fixture_sha256':digest(fixture),'http':http,'scope':'Actual three initialized startup-mode stdio processes and optional two same-name real desktop HTTP clients. Owned fixture mutations and origin undo restore exact project/native bytes; cursor advances honestly. No OS input or electrical certification.'})
print(json.dumps({'passed':True,'checks':len(checks),'http':http is not None}))
