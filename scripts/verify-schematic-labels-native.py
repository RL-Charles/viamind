#!/usr/bin/env python3
"""Check immutable schematic label exports against an explicitly owned scratch engine."""
import sys
sys.dont_write_bytecode=True
import argparse,base64,copy,importlib.util,io,json,tempfile,time
from contextlib import ExitStack
from pathlib import Path
import xml.etree.ElementTree as ET
from PIL import Image

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--repo',type=Path,required=True)
parser.add_argument('--report',type=Path,required=True)
parser.add_argument('--url',help='Owned scratch desktop only; replaces its project')
args=parser.parse_args()
sys.path.insert(0,str(args.repo/'scripts'))
from mcp_reference import StdioClient,digest,read_regular
binary=args.binary.resolve(strict=True)
checks=[];latencies=[]
def check(name,condition):
    if not condition:raise AssertionError(name)
    checks.append(name)
def overlap(a,b):return a['min_x']<b['max_x'] and b['min_x']<a['max_x'] and a['min_y']<b['max_y'] and b['min_y']<a['max_y']
with tempfile.TemporaryDirectory(prefix='viamind-schematic-label-proof-') as temporary,ExitStack() as stack:
    directory=Path(temporary)
    if args.url:
        spec=importlib.util.spec_from_file_location('smoke',args.repo/'scripts/mcp-smoke.py');smoke=importlib.util.module_from_spec(spec);spec.loader.exec_module(smoke)
        clients=[smoke.McpClient(args.url,'label-proof-a'),smoke.McpClient(args.url,'label-proof-b')]
        for client in clients:stack.callback(client.close)
    else:
        clients=[stack.enter_context(StdioClient([str(binary),'--headless'],directory))];clients[0].initialize()
    def call(name,arguments=None,error=False,peer=False):
        started=time.monotonic();raw=clients[int(peer and bool(args.url))].rpc('tools/call',{'name':name,'arguments':arguments or {}})
        if raw.get('isError',False)!=error:raise AssertionError((name,raw))
        result=raw if error else raw.get('structuredContent',json.loads(next((c['text'] for c in raw.get('content',[]) if c['type']=='text'),'null')))
        latencies.append({'tool':name,'milliseconds':(time.monotonic()-started)*1000});return result
    tools=clients[0].rpc('tools/list',{})['tools']
    check('actual SVG/PNG and immutable session tools discovered',{'render_schematic_view','render_schematic_png','get_session_state','query_schematic_connectivity'}<={t['name']for t in tools})
    fixture=args.repo/'fixtures/schematic/labels-wide.viamind.json';source=read_regular(fixture);native=json.loads(source);sid=next(iter(native['project']['schematic']['sheets']))
    cursor=call('get_session_state')['cursor'];call('open_project',{'path':str(fixture.resolve()),'expected_cursor':cursor,'discard_dirty':True})
    added=call('add_part',{'designator':'KEEP1','value':'dirty board remains intact','footprint':'Original','x_mm':1.000003,'y_mm':-2.000007},peer=True)
    call('set_selection',{'ids':added['created']})
    before=call('get_session_state');status=call('get_document_status');graph=call('query_schematic_connectivity',{'limit':100})
    params={'sheet':sid,'expected_cursor':before['cursor'],'options':{'width_px':1200,'height_px':800}}
    svg=call('render_schematic_view',params);peer_svg=call('render_schematic_view',params,peer=True)
    check('two independent clients return byte-identical SVG/metadata/source cursor',svg==peer_svg)
    check('exact source cursor and no output path for in-memory SVG',svg['source_cursor']==before['cursor'] and svg['path'] is None)
    labels=svg['metadata']['symbol_labels'];check('five complete units and fourteen pins produce24 declared labels',len(labels)==24 and svg['metadata']['symbol_count']==5 and svg['metadata']['pin_count']==14)
    for key in ['A','B','C','D','P']:
        check('explicit unit-qualified reference '+key,any(l['role']=='reference' and l['text'].startswith('U1:'+key+' (')for l in labels))
    content=native['project']['schematic']['sheets'][sid]['content'];pins={}
    for s in content['symbols'].values():
        for p in s['pins']:
            x,y=p['position']['x'],p['position']['y']
            if s['mirrored']:x=-x
            x,y=[(x,y),(y,-x),(-x,-y),(-y,x)][s['rotation_quarters']]
            pins[p['id']]={'x':s['position']['x']+x,'y':s['position']['y']+y}
    check('every leader starts at independently transformed exact source endpoint',all(l['leader'][0]==pins[l['object_id']]for l in labels if l['role']=='pin'))
    check('all labels retain stable source ownership and unique role IDs',len({l['id']for l in labels})==24 and all(l['symbol_id']in content['symbols']for l in labels))
    check('owned symbol label envelopes are disjoint',all(not overlap(a['envelope'],b['envelope'])for i,a in enumerate(labels)for b in labels[i+1:]if a['symbol_id']==b['symbol_id']))
    root=ET.fromstring(svg['svg']);check('SVG contains outlined glyphs and no active text/font dependencies',not list(root.iter('{http://www.w3.org/2000/svg}text')) and 'pin-leader-'in svg['svg'])
    raw=clients[-1].rpc('tools/call',{'name':'render_schematic_png','arguments':params})
    check('actual PNG response has one image/png block',raw.get('isError')is not True and len([c for c in raw['content']if c['type']=='image' and c['mimeType']=='image/png'])==1)
    png_meta=raw['structuredContent'];check('PNG metadata shares exact native-world labels and source cursor',png_meta['metadata']['symbol_labels']==labels and png_meta['source_cursor']==before['cursor'])
    image_block=next(c for c in raw['content']if c['type']=='image');pixels=base64.b64decode(image_block['data'],validate=True)
    with Image.open(io.BytesIO(pixels))as image:
        check('PNG independently decodes static RGBA8 at requested size',image.mode=='RGBA' and image.size==(1200,800) and not getattr(image,'is_animated',False))
        box=image.getchannel('A').getbbox();check('auto-fit retains all ink within transparent margin',box is not None and box[0]>1 and box[1]>1 and box[2]<1199 and box[3]<799)
    for layer,role in [('symbols','reference'),('pins','pin')]:
        filtered=call('render_schematic_view',params|{'options':{'layers':[layer]}},peer=True)
        rows=filtered['metadata']['symbol_labels'];check('actual '+layer+' layer preserves isolated text roles',bool(rows) and all((l['role']=='pin')==(role=='pin')for l in rows))
        check('actual '+layer+' leader isolation',('pin-leader-'in filtered['svg'])==(layer=='pins'))
    stale=copy.deepcopy(params);stale['expected_cursor']['sequence']+=1
    check('stale full-cursor render rejects before response',bool(call('render_schematic_view',stale,error=True)))
    check('empty category selection is explicit validation error','layers requires'in str(call('render_schematic_view',params|{'options':{'layers':[]}},error=True)))
    path=directory/'NEW.svg';published=call('render_schematic_view',params|{'path':str(path)},peer=True)
    check('NEW SVG publication exactly matches in-memory bytes',read_regular(path)==svg['svg'].encode() and published['svg']==svg['svg'])
    original=read_regular(path);call('render_schematic_view',params|{'path':str(path)},error=True)
    call('render_schematic_view',params|{'path':str(fixture.resolve())},error=True,peer=True)
    check('collision/native path refusals preserve both files',read_regular(path)==original and read_regular(fixture)==source)
    check('all successful and failed renders preserve dirty full session/status/graph',call('get_session_state')==before and call('get_session_state',peer=True)==before and call('get_document_status')==status and call('query_schematic_connectivity',{'limit':100})==graph)
    poison=copy.deepcopy(native);next(iter(poison['project']['schematic']['sheets'][sid]['content']['symbols'].values()))['value']='A'*33+'漢'
    # Multi-unit shared values must remain coherent while testing presentation.
    for symbol in poison['project']['schematic']['sheets'][sid]['content']['symbols'].values():symbol['value']='A'*33+'漢'
    poison_path=directory/'unsupported-tail.viamind.json';poison_path.write_text(json.dumps(poison))
    call('open_project',{'path':str(poison_path),'expected_cursor':before['cursor'],'discard_dirty':True})
    unchanged=call('get_session_state');params={'sheet':sid,'expected_cursor':unchanged['cursor']}
    check('abbreviation does not hide unsupported selected source tail','U+6F22'in str(call('render_schematic_view',params,error=True)))
    pins_only=call('render_schematic_view',params|{'options':{'layers':['pins']}},peer=True)
    check('unsupported omitted value does not poison Pins-only export',all(l['role']=='pin'for l in pins_only['metadata']['symbol_labels']))
    check('unsupported-tail render success/refusal retains full state',call('get_session_state')==unchanged)
    ascii_source=copy.deepcopy(poison)
    for symbol in ascii_source['project']['schematic']['sheets'][sid]['content']['symbols'].values():symbol['value']=('Engineering value '*4).strip()
    ascii_path=directory/'long-ascii.viamind.json';ascii_path.write_text(json.dumps(ascii_source))
    call('open_project',{'path':str(ascii_path),'expected_cursor':unchanged['cursor'],'discard_dirty':True})
    last=call('get_session_state');long_view=call('render_schematic_view',{'sheet':sid,'expected_cursor':last['cursor']},peer=True)
    values=[l for l in long_view['metadata']['symbol_labels']if l['role']=='value']
    check('actual long labels display32scalars+explicitellipsis and retain complete source',len(values)==5 and all(l['abbreviated'] and l['display_text']==l['text'][:32]+'...' and len(l['display_text'])==35 for l in values))
    check('long display and graph metadata preserve latest full session',call('get_session_state')==last and read_regular(fixture)==source)
    report={'passed':True,'checks':checks,'binary_sha256':digest(binary),'transport':args.url or'owned headless stdio','tool_count':len(tools),'latencies':latencies,'scope':'Read-only render calls preserve dirty complete engine state and exact original geometry. Setup explicitly replaces only the owned scratch project. Native WGPU UI screenshots/bounds are separate inspection artifacts; no OS input or global page-collision claim.'}
    with args.report.open('x')as output:json.dump(report,output,indent=2);output.write('\n')
print(json.dumps({'passed':True,'checks':len(checks),'report':str(args.report)}))
