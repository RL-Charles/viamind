#!/usr/bin/env python3
"""Original explicit aperture workflow in an owned process or two actual HTTP clients."""
import argparse, copy, hashlib, importlib.util, json, os, shutil, subprocess, sys, time
from pathlib import Path
sys.dont_write_bytecode=True
p=argparse.ArgumentParser(description=__doc__)
for field in ('binary','repo','directory'):p.add_argument('--'+field,type=Path,required=True)
p.add_argument('--endpoint');p.add_argument('--source',required=True)
a=p.parse_args();repo=a.repo.resolve();binary=a.binary.resolve();out=a.directory.resolve();out.mkdir()
sys.path.insert(0,str(repo/'scripts'));from mcp_reference import StdioClient
spec=importlib.util.spec_from_file_location('smoke',repo/'scripts/mcp-smoke.py');smoke=importlib.util.module_from_spec(spec);spec.loader.exec_module(smoke)
report={'passed':False,'source':a.source,'binary':str(binary),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'endpoint':a.endpoint,'checks':[],'latency_ms':{}}
clients=[];headless=None
try:
 if a.endpoint:clients=[smoke.McpClient(a.endpoint,'T120 creator'),smoke.McpClient(a.endpoint,'T120 peer')]
 else:
  headless=StdioClient([str(binary),'--headless'],out,timeout=60);headless.initialize();clients=[headless]
 def call(name,args=None,peer=False,error=False):return clients[min(int(peer),len(clients)-1)].call(name,args or {},expect_error=error)
 def state():return call('get_session_state')
 def status():return call('get_document_status')
 def check(label,condition=True):assert condition,label;report['checks'].append(label)
 def reject(name,args):
  before,st=state(),status();r=call(name,args,error=True);assert r and state()==before and status()==st;return r
 def timed(label,name,args):
  t=time.perf_counter();r=call(name,args);report['latency_ms'][label]=(time.perf_counter()-t)*1000;return r
 def discover(c):
  tools=[];cursor=None
  for _ in range(64):
   page=c.rpc('tools/list',{} if cursor is None else {'cursor':cursor});tools.extend(page['tools']);cursor=page.get('nextCursor')
   if cursor is None:return {t['name']:t for t in tools}
  raise AssertionError('catalog pagination bound')
 tools=discover(clients[0]);check('actual catalog advertises shared aperture action and paste request', 'set_pad_apertures'in tools and 'include_paste'in tools['export_fabrication']['inputSchema']['properties'])
 if a.endpoint:check('two initialized HTTP clients discover identical tools',set(discover(clients[1]))==set(tools))
 if a.endpoint:check('owned fixture identity verified before HTTP mutation',state()['project']['name']in['T120 exact apertures owned inspection','Independent fabrication reader fixture'])
 call('open_project',{'path':str(repo/'fixtures/fabrication/asymmetric.viamind.json')});initial=state();check('original explicit-pad fixture opened by shared lifecycle',len(initial['project']['board']['parts'])==2)
 before,st=state(),status();check('peer sees exact shared fixture',call('get_design',peer=True)==before['project']);oldsvg=call('render_board_view')['svg'];legacy=call('export_fabrication',{'path':str(out/'legacy'),'expected_cursor':before['cursor'],'mask_expansion_mm':0.050001})
 check('Legacy export remains exact seven retained pre-feature files',len(legacy['manifest']['files'])==7 and all((out/'legacy'/n).read_bytes()==(repo/'fixtures/fabrication/legacy-aperture-baseline'/n).read_bytes() for n in legacy['manifest']['files']))
 check('read and Legacy export preserve full session/status',state()==before and status()==st)
 top=next(i for i,x in before['project']['board']['parts'].items()if x['side']=='top');bottom=next(i for i,x in before['project']['board']['parts'].items()if x['side']=='bottom')
 def guard(part_id,pad_id,policy):
  s=state();return {'expected_project':s['project']['id'],'expected_cursor':s['cursor'],'expected':s['project']['board']['parts'][part_id],'pad_id':pad_id,'apertures':policy}
 explicit={'mask':{'width':4200003,'height':1200007},'paste':{'width':3800001,'height':800003}}
 pad=before['project']['board']['parts'][bottom]['pads'][0];request=guard(bottom,pad['id'],explicit);result=timed('exact_bottom_policy','set_pad_apertures',request);changed=state();expected=copy.deepcopy(before['project']);expected['board']['parts'][bottom]['pads'][0]['apertures']=explicit
 check('single guarded policy yields exact one Part delta/event and unchanged copper/nets/identity',changed['project']==expected and len(result['deltas'])==1 and changed['cursor']['sequence']==before['cursor']['sequence']+1)
 check('peer reads exact explicit dimensions',call('get_design',peer=True)==expected);reject('set_pad_apertures',request);check('stale full cursor refuses unchanged')
 call('undo');check('one undo restores complete original document',state()['project']==before['project']);call('redo');check('redo restores exact explicit document',state()['project']==expected)
 noop=state();call('set_pad_apertures',guard(bottom,pad['id'],explicit));check('equal policy no-op produces no event',state()==noop)
 for label,policy in [('zero rectangle',{'mask':{'width':0,'height':1},'paste':None}),('negative rectangle',{'mask':{'width':-1,'height':1},'paste':None}),('overflow dimensions',{'mask':{'width':9223372036854775807,'height':1},'paste':None}),('missing required shape',{'mask':None}),('unknown subtree',{'mask':None,'paste':None,'zone':{}})]:
  reject('set_pad_apertures',guard(bottom,pad['id'],policy));check(label+' rejects atomically')
 invalid=guard(bottom,pad['id'],explicit);invalid['expected']['pads']=[None]*1025;reject('set_pad_apertures',invalid);check('oversized borrowed poison rows refuse before typed decoding')
 stale=guard(bottom,pad['id'],None);call('add_part',{'designator':'PEER120','value':'keep','footprint':'legacy','x_mm':2,'y_mm':2},peer=True);call('undo',peer=True);reject('set_pad_apertures',stale);check('peer edit plus undo still invalidates full cursor')
 partial=timed('partial_explicit_export','export_fabrication',{'path':str(out/'partial'),'include_paste':True,'expected_cursor':state()['cursor']})
 summary=partial['manifest']['pad_apertures'];check('partial direct export discloses Legacy coverage and actual bottom sides',summary['legacy_pads']==2 and not summary['explicit_coverage_complete'] and summary['bottom_mask_rectangles']==1 and summary['bottom_paste_rectangles']==1)
 call('set_pad_apertures',guard(bottom,before['project']['board']['parts'][bottom]['pads'][1]['id'],{'mask':None,'paste':None}))
 top_policy={'mask':{'width':1500003,'height':800007},'paste':{'width':1100001,'height':500003}}
 call('set_pad_apertures',guard(top,before['project']['board']['parts'][top]['pads'][0]['id'],top_policy));authored=state()
 check('authoring all pads preserves entire original geometry except explicit policy',all({k:v for k,v in x.items()if k!='apertures'}=={k:v for k,v in y.items()if k!='apertures'}for i,part in authored['project']['board']['parts'].items()for x,y in zip(part['pads'],initial['project']['board']['parts'][i]['pads'])))
 check('default SVG remains exact old bytes',call('render_board_view')['svg']==oldsvg)
 svg=call('render_board_view',{'options':{'layers':['top_mask','bottom_mask','top_paste','bottom_paste']}});check('selected SVG identifies explicit geometry and deliberate omission', 'pad-'+pad['id']+'-mask'in svg['svg'] and len(svg['metadata']['pad_apertures']['pads'])==3)
 reject('export_fabrication',{'path':str(out/'refused-paste'),'expected_cursor':authored['cursor']});check('unrequested non-null paste refuses before publication',not(out/'refused-paste').exists())
 export=timed('complete_paste_export','export_fabrication',{'path':str(out/'paste'),'include_paste':True,'include_silkscreen':True,'expected_cursor':authored['cursor'],'mask_expansion_mm':0.5});summary=export['manifest']['pad_apertures']
 check('complete summary retains exact side counts and omission intent',summary['explicit_coverage_complete'] and summary['legacy_pads']==0 and summary['top_mask_rectangles']==1 and summary['bottom_mask_rectangles']==1 and summary['top_paste_rectangles']==1 and summary['bottom_paste_rectangles']==1 and summary['mask_omissions']==1 and summary['paste_omissions']==1)
 check('paired paste files have positive side-specific file attributes',all('FilePolarity,Positive'in(out/'paste'/n).read_text() and 'FileFunction,Paste,'+side in(out/'paste'/n).read_text() for n,side in [('top-paste.gbr','Top'),('bottom-paste.gbr','Bot')]))
 check('mask is negative and exact explicit dimensions ignore Legacy expansion', 'FilePolarity,Negative'in(out/'paste/bottom-mask.gbr').read_text() and '4.200003X1.200007'in(out/'paste/bottom-mask.gbr').read_text())
 lib=out/'exact.footprint.json';s=state();call('export_footprint',{'path':str(lib),'expected_project':s['project']['id'],'expected_cursor':s['cursor'],'expected':s['project']['board']['parts'][bottom]});definition=json.loads(lib.read_bytes());check('detached library3 stores exact policy and no pad IDs/nets',definition['version']==3 and definition['footprint']['pads'][0]['apertures']==explicit and all('id'not in x and 'net'not in x for x in definition['footprint']['pads']))
 s,st=state(),status();check('detached load is read-only and preserves full session/status',call('load_footprint_definition',{'path':str(lib)},peer=True)==definition['footprint'] and state()==s and status()==st)
 library_new=out/'edited.footprint.json';reuse_definition=copy.deepcopy(definition['footprint']);reuse_definition['pads'][0]['width']+=1
 call('save_footprint_definition',{'path':str(library_new),'footprint':reuse_definition},peer=True);check('detached NEW library3 save never applies to design',state()==s and status()==st)
 reject('save_footprint_definition',{'path':str(library_new),'footprint':reuse_definition});check('NEW file collision preserves exact library bytes',json.loads(library_new.read_bytes())['footprint']==reuse_definition)
 reuse={'expected_project':s['project']['id'],'expected':s['project']['board']['parts'][bottom],'path':str(library_new)};reject('import_footprint',reuse);check('explicit definition reuse requires complete source cursor')
 reuse['expected_cursor']=state()['cursor'];call('import_footprint',reuse);applied=state()['project']['board']['parts'][bottom]
 check('separate guarded library reuse retains pad UUID/net/absolute policy',applied['pads'][0]['width']==reuse_definition['pads'][0]['width'] and applied['pads'][0]['id']==pad['id'] and applied['pads'][0]['net']==pad['net'] and applied['pads'][0]['apertures']==explicit)
 call('undo');check('one library reuse undo restores exact complete project',state()['project']==s['project']);call('redo');check('one redo restores exact reused part',state()['project']['board']['parts'][bottom]==applied);call('undo')
 native=out/'authored.viamind.json';call('save_project',{'path':str(native)});saved=json.loads(native.read_bytes());check('native14 stores exact complete authored document',saved['version']==14 and saved['project']==state()['project'])
 stale=guard(bottom,pad['id'],None);call('open_project',{'path':str(native),'expected_cursor':state()['cursor']});reject('set_pad_apertures',stale);check('same UUID reopen old stream refuses unchanged')
 before,st=state(),status();job={'format':'viamind-output-job','version':5,'name':'Exact aperture proof','profile':'generic_outer_2layer','mask_expansion_mm':0,'include_paste':True,'include_silkscreen':True};jobpath=out/'paste.job.json';call('save_output_job',{'path':str(jobpath),'job':job});check('saved job5 exact canonical load',call('load_output_job',{'path':str(jobpath)})['job']==job)
 release=timed('saved_job5_release','run_output_job',{'job_path':str(jobpath),'path':str(out/'release'),'expected_cursor':before['cursor']});inspection=call('inspect_output_release',{'path':str(out/'release')});observed=inspection['observed_pad_openings']
 check('independent saved reader observes actual side rectangle and via counts',observed=={'top_mask_rectangles':1,'bottom_mask_rectangles':1,'top_mask_via_circles':2,'bottom_mask_via_circles':2,'top_paste_rectangles':1,'bottom_paste_rectangles':1} and inspection['file_count']==15 and release['file_count']==15)
 corrupt=out/'corrupt-release';shutil.copytree(out/'release',corrupt);file=corrupt/'fabrication/top-paste.gbr';old=file.read_bytes();file.write_bytes(old.replace(b'Paste,Top',b'Paste,Bot'));reject('inspect_output_release',{'path':str(corrupt)});check('same-length saved paste side corruption refuses independently',len(file.read_bytes())==len(old))
 check('all exports/jobs/inspection preserve full session/status',state()==before and status()==st)
 with StdioClient([str(binary),'--headless','--agent-read-only','--open',str(native)],out,timeout=60) as readonly:
  readonly.initialize();names=discover(readonly);check('read-only admission hides setter but permits exact stored observation','set_pad_apertures'not in names and readonly.call('get_design')==before['project'])
  restricted_state=readonly.call('get_session_state');restricted_status=readonly.call('get_document_status')
  try:readonly.call('set_pad_apertures',guard(bottom,pad['id'],None))
  except ValueError as refusal:assert str(refusal)=='MCP protocol error during tools/call',str(refusal)
  else:raise AssertionError('forged hidden setter must receive protocol denial')
  check('forged setter receives protocol denial and preserves full restricted session/status',readonly.call('get_session_state')==restricted_state and readonly.call('get_document_status')==restricted_status);readonly.finish()
 env=dict(os.environ);env.pop('DISPLAY',None);env.pop('WAYLAND_DISPLAY',None);cli=out/'cli-paste';proc=subprocess.run([str(binary),'--open',str(native),'--export-gerber','--output',str(cli),'--include-paste','--include-silkscreen'],env=env,capture_output=True,text=True,timeout=60);check('actual no-display CLI shares exact stored paste files',proc.returncode==0 and all((cli/n).read_bytes()==(out/'paste'/n).read_bytes()for n in ['top-paste.gbr','bottom-paste.gbr']))
 report.update(passed=True,check_count=len(report['checks']),tool_count=len(tools),native=str(native),final_state=state(),final_status=status(),cli={'returncode':proc.returncode,'stdout':proc.stdout,'stderr':proc.stderr})
finally:
 if headless:headless.finish()
 else:
  for client in clients:client.close()
 with(out/'production.json').open('x')as f:json.dump(report,f,indent=2);f.flush();os.fsync(f.fileno())
print(json.dumps({'passed':report['passed'],'checks':len(report['checks']),'report':str(out/'production.json')}))
