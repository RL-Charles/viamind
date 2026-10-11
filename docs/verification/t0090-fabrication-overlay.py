#!/usr/bin/env python3
"""Exact fabrication overlay MCP proof: --binary B --fixture F --report R [--port P]."""
import argparse, copy, hashlib, json, os, subprocess, tempfile, urllib.request
from pathlib import Path
class Client:
    def __init__(self, binary, fixture, port=None):
        self.port, self.sid, self.seq = port, None, 0
        self.stderr = tempfile.TemporaryFile() if port is None else None
        self.process = subprocess.Popen([binary, '--headless', '--open', fixture], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr, text=True) if port is None else None
        self.rpc('initialize', {'protocolVersion':'2025-03-26','capabilities':{},'clientInfo':{'name':'fabrication-overlay-verifier','version':'1'}})
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

p=argparse.ArgumentParser()
p.add_argument('--binary',required=True)
p.add_argument('--fixture',required=True)
p.add_argument('--report',required=True)
p.add_argument('--port',type=int)
args=p.parse_args()
directory=Path(tempfile.mkdtemp(prefix='viamind-t0090-'))
a=Client(args.binary,args.fixture,args.port)
b=Client(args.binary,args.fixture,args.port) if args.port else a
report={'passed':False,'mode':'actual production desktop HTTP / two sessions' if args.port else 'production stdio / one session','binary_sha256':hashlib.sha256(Path(args.binary).read_bytes()).hexdigest(),'checks':[],'directory':str(directory)}
def check(label,condition):
    assert condition,label
    report['checks'].append(label)
def state():return a.call('get_session_state',{})
def request(paths,context=None):
    context=context or state()
    return {'paths':[str(path) for path in paths],'expected_project':context['project']['id'],'expected_cursor':context['cursor']}
def reject(command,label):
    before=state()
    a.reject('inspect_fabrication_files',command)
    check(label,state()==before)
try:
    tools={t['name']:t for t in a.rpc('tools/list',{})['tools']}
    check('shared strict read-only inspection tool discovered','inspect_fabrication_files' in tools)
    before=state()
    package=a.call('export_fabrication',{'path':str(directory/'fabrication'),'mask_expansion_mm':0.050001,'expected_cursor':before['cursor']})
    check('new supported native export preserves complete session',state()==before)
    paths=sorted(path for path in (directory/'fabrication').iterdir() if path.suffix in ('.gbr','.xnc'))
    raw={path:path.read_bytes() for path in paths}
    command=request(paths,before)
    result=a.call('inspect_fabrication_files',command)
    check('all six fabrication geometry files inspected from exact captured full context',len(result['files'])==6 and result['expected_project']==before['project']['id'] and result['source_cursor']==before['cursor'])
    check('inspection preserves complete design history selection and dirty status',state()==before)
    check('independent peer sees identical exact source report',b.call('inspect_fabrication_files',command)==result)
    check('source hashes identify actual bytes and sources remain unchanged',all(file['source_sha256']==hashlib.sha256(raw[Path(file['path'])]).hexdigest() and Path(file['path']).read_bytes()==raw[Path(file['path'])] for file in result['files']))
    layers={file['geometry']['layer']:file['geometry'] for file in result['files']}
    check('all sides share top view and exact source project attribute',all(geometry['same_coordinates']==before['project']['id'] for layer,geometry in layers.items() if layer!='drill'))
    check('negative mask files explicitly describe openings',all(layers[layer]['negative_file_polarity'] for layer in ('top_mask_openings','bottom_mask_openings')))
    check('drill common-origin alignment is explicitly unverified',layers['drill']['unverified_common_origin'] and layers['drill']['same_coordinates'] is None)
    check('profile outer ring and cutout have eight exact centerlines',len(layers['profile']['primitives'])==8 and all(p['kind']=='segment' and p['width']==0 for p in layers['profile']['primitives']))
    check('drill hits have exact native downward-Y coordinates',any(p['center']=={'x':18000000,'y':12000000} and p['diameter']==300000 for p in layers['drill']['primitives']))
    check('rotated rectangular apertures preserve exact angle',any(p['kind']=='rectangle' and p['rotation_mdeg']==30000 for p in layers['bottom_copper']['primitives']))
    check('linear bottom track uses exact native endpoints',any(p['kind']=='segment' and p['start']=={'x':30000000,'y':17000000} and p['end']=={'x':40000000,'y':9000000} for p in layers['bottom_copper']['primitives']))
    shifted=directory/'shifted.gbr'
    original=raw[directory/'fabrication'/'bottom-copper.gbr']
    changed=original.replace(b'X30000000Y-17000000',b'X30000009Y-17000000')
    check('independent source shift preserves byte length and changes actual content',len(changed)==len(original) and changed!=original)
    shifted.write_bytes(changed)
    shifted_result=a.call('inspect_fabrication_files',request([shifted]))['files'][0]
    check('reader observes shifted file geometry independently of current board',any(p['kind']=='segment' and p['start']=={'x':30000009,'y':17000000} for p in shifted_result['geometry']['primitives']))
    check('shifted report hash identifies changed bytes',shifted_result['source_sha256']==hashlib.sha256(changed).hexdigest())
    for name,data in [('unsupported',original.replace(b'M02*',b'G36*M02*')),('wrong-project',original.replace(before['project']['id'].encode(),b'10000000-0000-4000-8000-000000000099')),('trailing',original+b'G01*'),('sub-nm',original.replace(b'0.400000',b'0.0000001'))]:
        path=directory/(name+'.gbr');path.write_bytes(data)
        reject(request([paths[0],path]),name+' rejects entire inspection without mutation')
    extra=copy.deepcopy(command);extra['unknown']=True
    reject(extra,'unknown request field rejects strictly')
    reject(request([]),'empty file list rejects')
    reject(request(paths+[paths[0]]),'seven files reject')
    reject(request([paths[0],paths[0]]),'duplicate file path rejects')
    reject(request([directory]),'directory descriptor rejects')
    reject(request([directory/('x'*4097)]),'oversized path rejects before filesystem work')
    fifo=directory/'pipe.gbr';os.mkfifo(fifo)
    reject(request([fifo]),'FIFO rejects without waiting for writer')
    check('all original export file bytes remain unchanged',all(path.read_bytes()==data for path,data in raw.items()))
    stale=command
    b.call('add_track',{'start_x_mm':45,'start_y_mm':25,'end_x_mm':46,'end_y_mm':25,'width_mm':0.25,'layer':'top','net':'PEER'})
    peer=state()
    reject(stale,'real peer edit invalidates old inspection full cursor')
    check('peer-created geometry survives rejected inspection',state()==peer)
    native=directory/'verified.viamind.json'
    a.call('save_project',{'path':str(native)})
    saved=state();old=request(paths,saved)
    a.call('open_project',{'path':str(native),'discard_dirty':False,'expected_cursor':saved['cursor']})
    reopened=state()
    check('same-project reopen changes stream while preserving project identity',reopened['project']['id']==saved['project']['id'] and reopened['cursor']['stream_id']!=saved['cursor']['stream_id'])
    reject(old,'same-project reopened stream rejects old inspection guard')
    report['final_native_project']=str(native)
    report['passed']=True
finally:
    Path(args.report).write_text(json.dumps(report,indent=2)+'\n')
    a.close()
    if b is not a:b.close()
print(json.dumps({'passed':report['passed'],'checks':len(report['checks']),'report':args.report}))
