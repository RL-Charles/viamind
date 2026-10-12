#!/usr/bin/env python3
"""Exercise NPTH through the actual application MCP and CLI; owned fixtures only."""
import argparse, copy, hashlib, importlib.util, json, os, shutil, subprocess, sys, uuid
from pathlib import Path
sys.dont_write_bytecode = True
p = argparse.ArgumentParser(description=__doc__)
for field in ('binary', 'repo', 'directory'): p.add_argument('--'+field, type=Path, required=True)
p.add_argument('--endpoint'); p.add_argument('--source', required=True)
a=p.parse_args(); repo=a.repo.resolve(); binary=a.binary.resolve(); out=a.directory.resolve(); out.mkdir()
sys.path.insert(0,str(repo/'scripts')); from mcp_reference import StdioClient
spec=importlib.util.spec_from_file_location('smoke',repo/'scripts/mcp-smoke.py'); smoke=importlib.util.module_from_spec(spec); spec.loader.exec_module(smoke)
report={'passed':False,'source':a.source,'binary':str(binary),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'endpoint':a.endpoint,'checks':[]}
clients=[]; headless=None
try:
 if a.endpoint: clients=[smoke.McpClient(a.endpoint,'T121 creator'),smoke.McpClient(a.endpoint,'T121 peer')]
 else:
  headless=StdioClient([str(binary),'--headless'],out,timeout=60); headless.initialize(); clients=[headless]
 def call(name,args=None,peer=False,error=False): return clients[min(int(peer),len(clients)-1)].call(name,args or {},expect_error=error)
 def state(): return call('get_session_state')
 def status(): return call('get_document_status')
 def check(label,condition=True): assert condition,label; report['checks'].append(label)
 def reject(name,args):
  before,st=state(),status(); result=call(name,args,error=True); assert result and state()==before and status()==st; return result
 def discover(client):
  tools=[]; cursor=None
  for _ in range(64):
   page=client.rpc('tools/list',{} if cursor is None else {'cursor':cursor}); tools.extend(page['tools']); cursor=page.get('nextCursor')
   if cursor is None:return {t['name']:t for t in tools}
  raise AssertionError('catalog pagination bound')
 tools=discover(clients[0]); check('catalog exposes shared hole writer and explicit fabrication opt-in','set_part_holes' in tools and 'include_npth' in tools['export_fabrication']['inputSchema']['properties'])
 if a.endpoint:
  check('two initialized clients discover same application catalog',set(discover(clients[1]))==set(tools))
  check('owned native fixture identity before HTTP mutation',state()['project']['name']=='T121 explicit holes owned inspection')
 call('open_project',{'path':str(repo/'fixtures/fabrication/asymmetric.viamind.json')}); initial=state(); part_id=next(iter(initial['project']['board']['parts']))
 def guard(holes):
  s=state(); return {'expected_project':s['project']['id'],'expected_cursor':s['cursor'],'expected':s['project']['board']['parts'][part_id],'holes':holes}
 legacy=out/'legacy.footprint.json'; s=state(); call('export_footprint',{'path':str(legacy),'expected_project':s['project']['id'],'expected_cursor':s['cursor'],'expected':s['project']['board']['parts'][part_id]})
 before,st=state(),status(); call('export_fabrication',{'path':str(out/'legacy-fab'),'expected_cursor':before['cursor']}); check('Legacy fabrication preserves full session/status',state()==before and status()==st)
 hole={'id':str(uuid.uuid4()),'key':'mount.A','center':{'x':3,'y':5},'diameter':1000001}; request=guard([hole]); result=call('set_part_holes',request); after=state(); expected=copy.deepcopy(before['project']); expected['board']['parts'][part_id]['holes']=[hole]
 check('hole edit changes only exact Part holes with one event',after['project']==expected and len(result['deltas'])==1 and after['cursor']['sequence']==before['cursor']['sequence']+1)
 check('peer observes same stored mechanical identities',call('get_design',peer=True)==expected); reject('set_part_holes',request); check('stale exact Part/full cursor refuses unchanged')
 call('undo'); check('undo restores original whole project',state()['project']==before['project']); call('redo'); check('redo restores exact keyed hole',state()['project']==expected)
 noop=state(); call('set_part_holes',guard([hole])); check('equal hole policy emits no event',state()==noop)
 for label,holes in [('zero diameter',[dict(hole,diameter=0)]),('duplicate key',[hole,dict(hole,id=str(uuid.uuid4()))]),('nil UUID',[dict(hole,id=str(uuid.UUID(int=0)))]),('foreign identity',[dict(hole,id=before['project']['id'])]),('missing center coordinate',[dict(hole,center={'x':3})]),('poison row max plus one',[None]*257)]:
  reject('set_part_holes',guard(holes)); check(label+' refuses atomically')
 missing=guard(None); del missing['holes']; reject('set_part_holes',missing); check('required nullable holes member enforced')
 stale=guard(None); call('add_part',{'designator':'PEER121','value':'keep','footprint':'legacy','x_mm':2,'y_mm':2},peer=True); call('undo',peer=True); reject('set_part_holes',stale); check('peer edit and undo invalidates complete initial cursor')
 s,st=state(),status(); reject('export_fabrication',{'path':str(out/'refused-fab'),'expected_cursor':s['cursor']}); check('missing NPTH opt-in refuses before publication',not(out/'refused-fab').exists())
 exported=call('export_fabrication',{'path':str(out/'npth-fab'),'expected_cursor':s['cursor'],'include_npth':True}); summary=exported['manifest']['nonplated_holes']
 check('separate NPTH file and summary preserve existing plated and copper bytes',summary['physical_drills']==1 and summary['tool_diameters']==1 and all((out/'npth-fab'/n).read_bytes()==(out/'legacy-fab'/n).read_bytes() for n in exported['manifest']['files'] if n not in ['manifest.json','nonplated-through.xnc']))
 text=(out/'npth-fab/nonplated-through.xnc').read_text(); check('NPTH contains exact diameter and attributed common origin','T01C1.000001' in text and 'TF.FileFunction,NonPlated,1,2,NPTH' in text and str(s['project']['id']) in text)
 view=call('render_board_view'); check('static metadata exposes mechanical role and stored identity',view['metadata']['nonplated_holes']['holes'][0]['hole_id']==hole['id'])
 lib=out/'holes.footprint.json'; call('export_footprint',{'path':str(lib),'expected_project':s['project']['id'],'expected_cursor':s['cursor'],'expected':s['project']['board']['parts'][part_id]}); definition=json.loads(lib.read_bytes()); check('detached library4 contains mechanical keys without global IDs',definition['version']==4 and definition['footprint']['holes'][0]['key']=='mount.A' and 'id' not in definition['footprint']['holes'][0])
 check('detached load remains read-only',call('load_footprint_definition',{'path':str(lib)},peer=True)==definition['footprint'] and state()==s and status()==st)
 edited=copy.deepcopy(definition['footprint']); edited['holes'][0]['diameter']+=2; edited['holes'].append({'key':'mount.B','center':{'x':1000003,'y':5},'diameter':500003}); newlib=out/'edited.footprint.json'; call('save_footprint_definition',{'path':str(newlib),'footprint':edited}); check('detached NEW save never applies',state()==s and status()==st)
 reject('import_footprint',{'path':str(newlib),'expected_project':s['project']['id'],'expected':s['project']['board']['parts'][part_id]}); check('hole library reuse requires complete cursor')
 call('import_footprint',{'path':str(newlib),'expected_project':s['project']['id'],'expected_cursor':s['cursor'],'expected':s['project']['board']['parts'][part_id]}); applied=state()['project']['board']['parts'][part_id]
 check('keyed reuse keeps existing UUID/pads/nets and gives new key a distinct UUID',applied['holes'][0]['id']==hole['id'] and applied['holes'][0]['diameter']==1000003 and applied['holes'][1]['id']!=hole['id'] and applied['pads']==s['project']['board']['parts'][part_id]['pads'])
 call('undo'); check('one reuse undo restores complete project',state()['project']==s['project'])
 s=state(); call('import_footprint',{'path':str(legacy),'expected_project':s['project']['id'],'expected_cursor':s['cursor'],'expected':s['project']['board']['parts'][part_id]}); check('old library authoritatively restores Legacy absence','holes' not in state()['project']['board']['parts'][part_id]); call('undo')
 call('set_part_holes',guard([])); check('explicit empty survives as present array',state()['project']['board']['parts'][part_id]['holes']==[]); reject('export_fabrication',{'path':str(out/'refused-empty'),'expected_cursor':state()['cursor']}); check('explicit empty still requires opt-in',not(out/'refused-empty').exists()); call('set_part_holes',guard([hole]))
 native=out/'authored.viamind.json'; call('save_project',{'path':str(native)}); saved=json.loads(native.read_bytes()); check('native15 retains exact authored project',saved['version']==15 and saved['project']==state()['project'])
 stale=guard(None); call('open_project',{'path':str(native),'expected_cursor':state()['cursor']}); reject('set_part_holes',stale); check('reopen changes stream and invalidates stale writer')
 before,st=state(),status(); job={'format':'viamind-output-job','version':6,'name':'Explicit drill proof','profile':'generic_outer_2layer','mask_expansion_mm':0,'include_npth':True}; jobpath=out/'npth.job.json'; call('save_output_job',{'path':str(jobpath),'job':job}); check('saved job6 canonical load exact',call('load_output_job',{'path':str(jobpath)})['job']==job)
 call('run_output_job',{'job_path':str(jobpath),'path':str(out/'release'),'expected_cursor':before['cursor']}); facts=call('inspect_output_release',{'path':str(out/'release')})['observed_nonplated_holes']; check('saved release independently observes actual NPTH and plated counts',facts=={'physical_drills':1,'tool_diameters':1,'same_coordinates':before['project']['id'],'plated_via_drills':2})
 corrupt=out/'corrupt-release'; shutil.copytree(out/'release',corrupt); file=corrupt/'fabrication/nonplated-through.xnc'; data=file.read_bytes(); file.write_bytes(data.replace(b'NonPlated,1,2,NPTH',b'NonPlated,1,3,NPTH')); reject('inspect_output_release',{'path':str(corrupt)}); check('same-length wrong NPTH span rejected independently',len(file.read_bytes())==len(data))
 check('all jobs and inspections preserve full session/status',state()==before and status()==st)
 with StdioClient([str(binary),'--headless','--agent-read-only','--open',str(native)],out,timeout=60) as readonly:
  readonly.initialize(); check('restricted process hides writer but reads exact holes','set_part_holes' not in discover(readonly) and readonly.call('get_design')==before['project']); rs,ds=readonly.call('get_session_state'),readonly.call('get_document_status')
  try: readonly.call('set_part_holes',guard(None))
  except ValueError as e: assert str(e)=='MCP protocol error during tools/call'
  else: raise AssertionError('hidden writer must receive protocol denial')
  check('forged restricted call preserves whole session/status',readonly.call('get_session_state')==rs and readonly.call('get_document_status')==ds); readonly.finish()
 env=dict(os.environ); env.pop('DISPLAY',None); env.pop('WAYLAND_DISPLAY',None); cli=out/'cli-npth'; proc=subprocess.run([str(binary),'--open',str(native),'--export-gerber','--output',str(cli),'--include-npth'],env=env,capture_output=True,text=True,timeout=60); check('no-display CLI produces identical NPTH and plated files',proc.returncode==0 and all((cli/n).read_bytes()==(out/'npth-fab'/n).read_bytes()for n in ['nonplated-through.xnc','plated-through.xnc']))
 report.update(passed=True,check_count=len(report['checks']),tool_count=len(tools),native=str(native),final_state=state(),final_status=status(),cli={'returncode':proc.returncode,'stdout':proc.stdout,'stderr':proc.stderr})
finally:
 if headless: headless.finish()
 else:
  for client in clients: client.close()
 with(out/'production.json').open('x')as f: json.dump(report,f,indent=2); f.flush(); os.fsync(f.fileno())
print(json.dumps({'passed':report['passed'],'checks':len(report['checks']),'report':str(out/'production.json')}))
