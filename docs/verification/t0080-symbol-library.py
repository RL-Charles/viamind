#!/usr/bin/env python3
"""Exact production MCP proof: --binary B --fixture F --report R [--port P]."""
import argparse, copy, hashlib, json, subprocess, tempfile, urllib.request
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
p = argparse.ArgumentParser(); p.add_argument('--binary', required=True); p.add_argument('--fixture', required=True); p.add_argument('--report', required=True); p.add_argument('--port', type=int)
args = p.parse_args(); directory = Path(tempfile.mkdtemp(prefix='viamind-t0080-')); a = Client(args.binary, args.fixture, args.port); b = Client(args.binary, args.fixture, args.port) if args.port else a
report = {'passed':False,'mode':'actual desktop HTTP / two independent MCP sessions' if args.port else 'production headless stdio / one session', 'binary_sha256':hashlib.sha256(Path(args.binary).read_bytes()).hexdigest(), 'fixture':str(Path(args.fixture).resolve()), 'directory':str(directory), 'checks':[]}
def check(label, condition):
    assert condition, label
    report['checks'].append(label)
def state(): return a.call('get_session_state', {})
def unchanged_reject(tool, command, label):
    before = state(); a.reject(tool, command); check(label, state() == before)
def guard(): return a.call('snapshot_symbol_sheet', {'sheet_id':sheet_id})
def command(reference='U2'):
    g=guard()
    return {'expected_project':g['expected_project'],'source_cursor':g['source_cursor'],'expected':g['expected'],'template':template,'designator':reference,'position':{'x':40000001,'y':50000003},'rotation_quarters':1,'mirrored':True}
try:
    tools = {t['name'] for t in a.rpc('tools/list', {})['tools']}
    check('all four shared symbol library tools discovered', {'snapshot_symbol_sheet','load_symbol_template','export_symbol_template','place_symbol_template'} <= tools)
    before = state(); project=before['project']; sheet_id, sheet=next(iter(project['schematic']['sheets'].items())); symbol=next(s for s in sheet['content']['symbols'].values() if s['designator']=='U1')
    g=guard(); check('bounded selected-sheet capture is immutable with exact full cursor', g['expected']==sheet and g['source_cursor']==before['cursor'] and state()==before)
    path=directory/'original.viamind-symbol.json'
    export={'expected_project':project['id'],'source_cursor':before['cursor'],'sheet_id':sheet_id,'symbol_id':symbol['id'],'name':'Original regulator','path':str(path)}
    result=a.call('export_symbol_template',export); raw=path.read_bytes(); envelope=json.loads(raw)
    check('detached strict envelope excludes UUIDs/reference/instance transform', set(envelope)=={'format','version','symbol'} and envelope['format']=='viamind-symbol' and envelope['version']==1 and set(envelope['symbol'])=={'name','value','footprint','width','height','pins'} and all(set(pin)=={'number','name','position','electrical'} for pin in envelope['symbol']['pins']))
    check('export records full source cursor and preserves entire session', result['exported_cursor']==before['cursor'] and state()==before)
    unchanged_reject('export_symbol_template',export,'NEW collision rejects without changing session'); check('NEW collision preserves library bytes',path.read_bytes()==raw)
    template=b.call('load_symbol_template',{'path':str(path)}); check('loaded definitions exactly equal exported local definitions', template==envelope['symbol'] and state()==before)
    bad=directory/'unknown.json'; damaged=copy.deepcopy(envelope); damaged['symbol']['pins'][0]['net']='invented'; bad.write_text(json.dumps(damaged))
    unchanged_reject('load_symbol_template',{'path':str(bad)},'unknown nested pin field rejects atomically')
    huge=directory/'huge.json'; huge.write_bytes(b' '* (1024*1024+1)); unchanged_reject('load_symbol_template',{'path':str(huge)},'oversized regular file rejects atomically')
    unchanged_reject('load_symbol_template',{'path':str(directory)},'directory instead of regular file rejects atomically')
    unchanged_reject('place_symbol_template',command('U1'),'duplicate global reference rejects atomically')
    proposal=command(); old=state(); a.call('place_symbol_template',proposal); created=state(); content=created['project']['schematic']['sheets'][sheet_id]['content']; instance=next(s for s in content['symbols'].values() if s['designator']=='U2')
    old_ids={s['id'] for s in sheet['content']['symbols'].values()}|{pin['id'] for s in sheet['content']['symbols'].values() for pin in s['pins']}; new_ids={instance['id']}|{pin['id'] for pin in instance['pins']}
    check('fresh instance and all pin identities with exact transform', len(new_ids)==4 and not old_ids&new_ids and instance['position']==proposal['position'] and instance['rotation_quarters']==1 and instance['mirrored'] and instance['pins'][0]['position']==symbol['pins'][0]['position'])
    restored=copy.deepcopy(created['project']); del restored['schematic']['sheets'][sheet_id]['content']['symbols'][instance['id']]
    check('one event changes only the new schematic symbol',restored==old['project'] and created['cursor']['sequence']==old['cursor']['sequence']+1)
    if b is not a:
        b.reject('undo',{}); check('second connection cannot undo creator placement',state()==created)
    a.call('undo',{}); check('one creator undo removes entire instance',a.call('get_design',{})==old['project']); a.call('redo',{}); check('redo preserves all fresh UUIDs',a.call('get_design',{})==created['project']); a.call('undo',{})
    stale=command(); b.call('add_part',{'designator':'R_PEER','value':'keep','footprint':'F','x_mm':1,'y_mm':2}); peer=state()
    unchanged_reject('place_symbol_template',stale,'concurrent edit invalidates full placement cursor')
    a.call('place_symbol_template',command()); placed=state(); check('fresh placement preserves peer PCB edit',placed['project']['board']==peer['project']['board']); a.call('undo',{}); check('creator undo preserves peer board edit',a.call('get_design',{})==peer['project']); a.call('redo',{}); check('creator redo restores exact new instance',a.call('get_design',{})==placed['project'])
    stale_export=dict(export,path=str(directory/'stale.symbol.json')); unchanged_reject('export_symbol_template',stale_export,'stale source export rejects without output'); check('stale export publishes nothing',not Path(stale_export['path']).exists())
    stale=command('U3'); native=directory/'saved.viamind.json'; a.call('save_project',{'path':str(native),'overwrite':False}); saved=native.read_bytes(); oldstream=state()['cursor']['stream_id']; a.call('open_project',{'path':str(native),'discard_dirty':False,'expected_cursor':state()['cursor']})
    check('same-project reopen obtains new event stream',state()['project']['id']==project['id'] and state()['cursor']['stream_id']!=oldstream)
    unchanged_reject('place_symbol_template',stale,'same-project reopen rejects old placement stream')
    current=state(); protect={'expected_project':project['id'],'source_cursor':current['cursor'],'sheet_id':sheet_id,'symbol_id':symbol['id'],'name':'Original regulator','path':str(native)}
    unchanged_reject('export_symbol_template',protect,'active native destination rejects atomically'); check('native destination retains exact saved bytes',native.read_bytes()==saved)
    edited=copy.deepcopy(envelope); edited['symbol']['value']='Detached revised regulator'; path.write_text(json.dumps(edited)+'\n'); updated=a.call('load_symbol_template',{'path':str(path)}); check('library edit/load leaves existing instances unchanged',state()==current and updated['value']=='Detached revised regulator')
    template=updated; proposal=command('U3'); proposal['position']={'x':65000001,'y':50000003}; a.call('place_symbol_template',proposal); final=state(); symbols=final['project']['schematic']['sheets'][sheet_id]['content']['symbols']; check('explicit new instance uses revised template; original instances unchanged',next(s for s in symbols.values() if s['designator']=='U3')['value']=='Detached revised regulator' and all(s['value']=='3.3 V regulator' for s in symbols.values() if s['designator'] in ('U1','U2')))
    a.call('save_project',{'path':str(directory/'final.viamind.json'),'overwrite':False}); check('native round trip persists fresh identities',json.loads((directory/'final.viamind.json').read_text())['project']==final['project'])
    report['final_project']=str(directory/'final.viamind.json'); report['passed']=True
finally:
    a.close()
    if b is not a: b.close()
    Path(args.report).write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
