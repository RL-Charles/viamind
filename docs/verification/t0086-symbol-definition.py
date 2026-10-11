#!/usr/bin/env python3
"""Exact production MCP proof: --binary B --fixture F --report R [--port P]."""
import argparse, copy, hashlib, json, os, subprocess, tempfile, urllib.request
from pathlib import Path
class Client:
    def __init__(self, binary, fixture, port=None):
        self.port, self.sid, self.seq = port, None, 0
        self.stderr = tempfile.TemporaryFile() if port is None else None
        self.process = subprocess.Popen([binary, '--headless', '--open', fixture], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr, text=True) if port is None else None
        self.rpc('initialize', {'protocolVersion':'2025-03-26','capabilities':{},'clientInfo':{'name':'symbol-library-verifier','version':'1'}})
        self.rpc('notifications/initialized', notification=True)
    def rpc(self, method, params=None, notification=False):
        self.seq += 1
        obj = {'jsonrpc':'2.0','method':method}
        if not notification: obj['id'] = self.seq
        if params is not None: obj['params'] = params
        if self.process:
            self.process.stdin.write(json.dumps(obj)+'\n'); self.process.stdin.flush()
            if notification: return None
            while True:
                line = self.process.stdout.readline()
                if not line: raise RuntimeError('stdio MCP exited before response')
                response = json.loads(line)
                if response.get('id') == self.seq: break
        else:
            headers = {'Content-Type':'application/json','Accept':'application/json, text/event-stream'}
            if self.sid: headers['Mcp-Session-Id'] = self.sid
            request = urllib.request.Request(f'http://127.0.0.1:{self.port}/mcp', data=json.dumps(obj).encode(), headers=headers)
            with urllib.request.urlopen(request, timeout=30) as r:
                self.sid = r.headers.get('Mcp-Session-Id', self.sid); raw = r.read().decode()
            if not raw.strip(): return None
            events = [line[5:].strip() for line in raw.splitlines() if line.startswith('data:')]
            response = json.loads(events[-1] if events else raw)
        if 'error' in response: raise RuntimeError(response)
        return response.get('result')
    def call(self, name, args):
        result = self.rpc('tools/call', {'name':name,'arguments':args})
        if result.get('isError'): raise RuntimeError(result)
        return result.get('structuredContent') or json.loads(result['content'][0]['text'])
    def reject(self, name, args):
        result = self.rpc('tools/call', {'name':name,'arguments':args})
        assert result.get('isError'), result
    def close(self):
        if self.process:
            self.process.stdin.close(); self.process.wait(timeout=30)
            assert self.process.returncode == 0
            self.stderr.close()

p = argparse.ArgumentParser()
p.add_argument('--binary', required=True)
p.add_argument('--fixture', required=True)
p.add_argument('--report', required=True)
p.add_argument('--port', type=int)
args = p.parse_args()
directory = Path(tempfile.mkdtemp(prefix='viamind-t0086-'))
a = Client(args.binary, args.fixture, args.port)
b = Client(args.binary, args.fixture, args.port) if args.port else a
report = {'passed':False, 'mode':'actual production desktop HTTP / two sessions' if args.port else 'production stdio / one session', 'binary_sha256':hashlib.sha256(Path(args.binary).read_bytes()).hexdigest(), 'checks':[], 'directory':str(directory)}
def check(label, condition):
    assert condition, label
    report['checks'].append(label)
def state(): return a.call('get_session_state', {})
def reject(command, label):
    previous = state()
    a.reject('save_symbol_template', command)
    check(label, state() == previous)
try:
    tools = {t['name']:t for t in a.rpc('tools/list', {})['tools']}
    check('shared detached save and guarded placement tools discovered', {'save_symbol_template','load_symbol_template','snapshot_symbol_sheet','place_symbol_template'} <= tools.keys())
    before = state()
    template = {'name':'Original amplifier', 'value':'Original gain stage', 'footprint':'Original-2Pad', 'width':10000001, 'height':4000003, 'pins':[{'number':'1','name':'IN','position':{'x':-5000001,'y':2000003},'electrical':'input'}]}
    path = directory/'original-amplifier.viamind-symbol.json'
    command = {'template':template, 'path':str(path)}
    result = a.call('save_symbol_template', command)
    raw = path.read_bytes()
    check('NEW save returns canonical path/name/count and preserves entire session', result == {'path':str(path.resolve()),'name':template['name'],'pin_count':1} and state()==before)
    check('strict detached envelope has exact odd Nm geometry and no instance identities', json.loads(raw)=={'format':'viamind-symbol','version':1,'symbol':template})
    loaded = b.call('load_symbol_template', {'path':str(path)})
    check('second client loads exact saved definition without design mutation', loaded==template and state()==before)
    reject(command, 'existing definition collision rejects without session changes')
    check('existing definition bytes preserved', path.read_bytes()==raw)
    invalids = []
    bad = copy.deepcopy(command); bad['unknown']=True; invalids.append(('unknown request field',bad))
    bad = copy.deepcopy(command); bad['template']['net']='invented'; invalids.append(('unknown detached field',bad))
    bad = copy.deepcopy(command); bad['template']['width']=0; invalids.append(('nonpositive body',bad))
    bad = copy.deepcopy(command); bad['template']['pins']*=2; invalids.append(('duplicate pin numbers',bad))
    bad = copy.deepcopy(command); bad['template']['pins'][0]['position']['x']=10000000001; invalids.append(('coordinate overflow',bad))
    bad = copy.deepcopy(command); bad['template']['pins']=[dict(template['pins'][0],number=str(i)) for i in range(1025)]; invalids.append(('pin count overflow',bad))
    bad = copy.deepcopy(command); bad['template']['value']='x'*4097; invalids.append(('text field overflow',bad))
    for i,(label,bad) in enumerate(invalids):
        bad['path']=str(directory/f'invalid-{i}.json')
        reject(bad, label+' rejects atomically')
        check(label+' publishes nothing', not Path(bad['path']).exists())
    link = directory/'protected-link'; link.symlink_to(path)
    reject(dict(command,path=str(link)), 'symlink collision rejects atomically')
    check('symlink and target preserved', link.is_symlink() and path.read_bytes()==raw)
    reject(dict(command,path=str(directory)), 'existing directory rejects atomically')
    sheet_id = next(iter(before['project']['schematic']['sheets']))
    g = a.call('snapshot_symbol_sheet', {'sheet_id':sheet_id})
    proposal = {'expected_project':g['expected_project'],'source_cursor':g['source_cursor'],'expected':g['expected'],'template':loaded,'designator':'U2','position':{'x':40000001,'y':50000003},'rotation_quarters':3,'mirrored':True}
    a.call('place_symbol_template', proposal)
    placed = state()
    symbols = placed['project']['schematic']['sheets'][sheet_id]['content']['symbols']
    created = next(s for s in symbols.values() if s['designator']=='U2')
    previous_ids = {s['id'] for s in before['project']['schematic']['sheets'][sheet_id]['content']['symbols'].values()} | {pin['id'] for s in before['project']['schematic']['sheets'][sheet_id]['content']['symbols'].values() for pin in s['pins']}
    check('fresh instance and pin identities preserve detached exact local geometry', created['id'] not in previous_ids and created['pins'][0]['id'] not in previous_ids and created['pins'][0]['position']==template['pins'][0]['position'] and created['rotation_quarters']==3 and created['mirrored'])
    restored = copy.deepcopy(placed['project']); del restored['schematic']['sheets'][sheet_id]['content']['symbols'][created['id']]
    check('one guarded placement changes only the new instance', restored==before['project'] and placed['cursor']['sequence']==before['cursor']['sequence']+1)
    if b is not a:
        b.reject('undo', {})
        check('independent peer cannot undo creator placement', state()==placed)
    a.call('undo', {}); check('one undo restores entire original project', state()['project']==before['project'])
    a.call('redo', {}); check('redo restores exact new instance identities', state()['project']==placed['project'])
    revised = copy.deepcopy(template); revised['value']='Detached revised gain'
    b.call('save_symbol_template', {'template':revised,'path':str(directory/'revised.json')})
    check('saving revised definition leaves every existing instance unchanged', state()['project']==placed['project'] and path.read_bytes()==raw)
    stale = copy.deepcopy(proposal); stale['designator']='U3'
    b.call('add_part', {'designator':'R_PEER','value':'keep','footprint':'F','x_mm':1,'y_mm':2})
    peer = state(); a.reject('place_symbol_template', stale)
    check('peer edit invalidates stale placement without losing peer changes', state()==peer)
    native = directory/'saved.viamind.json'
    a.call('save_project', {'path':str(native),'overwrite':False})
    native_bytes = native.read_bytes()
    reject(dict(command,path=str(native)), 'active native destination rejects atomically')
    check('native project retains exact saved bytes', native.read_bytes()==native_bytes)
    check('successful and failed saves leave no staging files', not list(directory.glob('.viamind-symbol-*')))
    report['final_native_project']=str(native)
    report['passed']=True
finally:
    a.close()
    if b is not a: b.close()
    Path(args.report).write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
