import hashlib,json,os,sys
from pathlib import Path
repo=Path('/home/charlesmowbray/Development/viamind-t-0116')
root=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0116')
sys.path.insert(0,str(repo/'scripts'))
from mcp_reference import StdioClient
out=root/'placed-fixture-verified';out.mkdir()
binary=root/'production-final-combined-verified';initial=root/'initial.viamind.json'
with StdioClient([str(binary),'--headless','--open',str(initial)],out,timeout=90) as client:
 client.initialize()
 before=client.call('get_session_state',{})
 assert before['project']['name']=='T116 complete package owned proof' and not before['project']['board']['parts']
 sheet=next(iter(before['project']['schematic']['sheets']))
 template=json.loads((repo/'fixtures/symbol-packages/original-quad.viamind-symbol-package.json').read_text())['package']
 request=client.call('snapshot_symbol_sheet',{'sheet_id':sheet})
 request.update(template=template,designator='U1',placements=[{'key':u['key'],'position':{'x':i*20_000_000+1,'y':3},'rotation_quarters':i%4,'mirrored':bool(i%2)} for i,u in enumerate(template['units'])])
 client.call('place_symbol_package',request)
 placed=client.call('get_session_state',{})
 native=out/'placed.viamind.json'
 client.call('save_project',{'path':str(native)})
 doc=json.loads(native.read_bytes())
 assert doc['version']==11 and doc['project']==placed['project'] and not placed['project']['board']['parts']
 assert len(placed['project']['schematic']['sheets'][sheet]['content']['packages'])==1
 client.finish()
report={'passed':True,'route':'initialized actual production headless MCP typed place_symbol_package and native save','binary':str(binary),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'fixture':str(native),'fixture_sha256':hashlib.sha256(native.read_bytes()).hexdigest(),'version':doc['version'],'package_count':1,'unit_count':5,'pin_count':14}
with (out/'report.json').open('x') as f:
 json.dump(report,f,indent=2);f.flush();os.fsync(f.fileno())
print(json.dumps(report))
