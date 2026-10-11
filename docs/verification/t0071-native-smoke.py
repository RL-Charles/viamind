import importlib.util,json,base64,hashlib,io,argparse
from pathlib import Path
from PIL import Image
parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--directory',type=Path,required=True);parser.add_argument('--endpoint',default='http://127.0.0.1:7896/mcp');config=parser.parse_args();directory=config.directory.resolve();binary=config.binary.resolve()
spec=importlib.util.spec_from_file_location('mcp_smoke',Path(__file__).resolve().parents[2]/'scripts/mcp-smoke.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
a=m.McpClient(config.endpoint,'desktop-export-reader');b=m.McpClient(config.endpoint,'desktop-export-editor')
report={'passed':False,'checks':[],'runtime':'fixed-size wrapper using production ViamindApp and shared engine/MCP; form opened through public UI entry point'}
sheet='00000000-0000-0000-0000-000000000066'
def check(name,condition):
 assert condition,name
 report['checks'].append(name)
try:
 names={t['name'] for t in a.rpc('tools/list')['tools']};check('native HTTP exposes four desktop-matched render tools',{'render_board_view','render_board_png','render_schematic_view','render_schematic_png'}.issubset(names))
 before=a.call('get_session_state',{});status=a.call('get_document_status',{});native=Path(status['path']);native_bytes=native.read_bytes()
 for view in ['board','schematic']:
  for fmt in ['svg','png']:
   tool=f'render_{view}_{"view" if fmt=="svg" else "png"}';output=directory/f'native-{view}.{fmt}';assert not output.exists()
   args={'path':str(output),'expected_cursor':before['cursor']}
   if view=='schematic':args['sheet']=sheet
   result=a.rpc('tools/call',{'name':tool,'arguments':args});check(f'{view} {fmt} native tool succeeds',not result.get('isError'))
   meta=result['structuredContent'];check(f'{view} {fmt} exact source snapshot',meta['source_cursor']==before['cursor'])
   if fmt=='png':
    blocks=[c for c in result['content'] if c['type']=='image'];check(f'{view} PNG is actual compact image block',len(blocks)==1 and blocks[0]['mimeType']=='image/png' and 'png_base64' not in meta)
    content=base64.b64decode(blocks[0]['data'],validate=True);image=Image.open(io.BytesIO(content));check(f'{view} PNG independently decodes RGBA',image.mode=='RGBA' and image.size==(1600,1000))
   else:content=meta['svg'].encode()
   check(f'{view} {fmt} HTTP file production CLI bytes identical',content==output.read_bytes()==(directory/f'cli-{view}.{fmt}').read_bytes())
 check('native view calls preserve full session and status',a.call('get_session_state',{})==before and a.call('get_document_status',{})==status)
 a.call('render_board_view',{'path':str(native)},expect_error=True);a.call('render_board_png',{'path':str(directory/'native-board.png')},expect_error=True)
 check('active native and existing outputs preserved',native.read_bytes()==native_bytes)
 b.call('add_part',{'designator':'R_NATIVE','value':'1k','footprint':'R_0603','x_mm':1,'y_mm':1})
 stale=directory/'native-stale.svg';assert not stale.exists();a.call('render_board_view',{'path':str(stale),'expected_cursor':before['cursor']},expect_error=True);check('other native agent invalidates stale export with no file',not stale.exists())
 b.call('undo',{});check('native editor undo restores project',a.call('get_design',{})==before['project']);check('distinct native transport sessions',a.session!=b.session)
 report.update(passed=True,endpoint=config.endpoint,binary=str(binary),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),source_cursor=before['cursor'])
finally:
 a.close();b.close();(directory/'native-report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
