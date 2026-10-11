#!/usr/bin/env python3
"""Owned two-client production artwork/library/Legend workflow; no desktop input injection."""
import argparse, copy, hashlib, importlib.util, json, os, subprocess, sys, time
from pathlib import Path
sys.dont_write_bytecode=True
p=argparse.ArgumentParser(description=__doc__)
for name in ('binary','repo','directory'):p.add_argument('--'+name,type=Path,required=True)
p.add_argument('--endpoint',required=True);p.add_argument('--source',required=True)
a=p.parse_args();out=a.directory.resolve();out.mkdir();repo=a.repo.resolve();binary=a.binary.resolve()
sys.path.insert(0,str(repo/'scripts'));from mcp_reference import StdioClient
spec=importlib.util.spec_from_file_location('smoke',repo/'scripts/mcp-smoke.py');smoke=importlib.util.module_from_spec(spec);spec.loader.exec_module(smoke)
report={'passed':False,'source':a.source,'binary':str(binary),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'endpoint':a.endpoint,'checks':[],'latency_ms':{}}
clients=[]
def check(label,ok=True):assert ok,label;report['checks'].append(label)
try:
 clients=[smoke.McpClient(a.endpoint,'T119 creator'),smoke.McpClient(a.endpoint,'T119 peer')]
 def call(name,args=None,peer=False,error=False):return clients[int(peer)].call(name,args or {},expect_error=error)
 def state():return call('get_session_state')
 def status():return call('get_document_status')
 def reject(name,args):
  before,st=state(),status();result=call(name,args,error=True);assert result and state()==before and status()==st;return result
 def timed(label,name,args):
  start=time.perf_counter();result=call(name,args);report['latency_ms'][label]=(time.perf_counter()-start)*1000;return result
 tools={t['name']:t for t in clients[0].rpc('tools/list',{})['tools']}
 check('two initialized clients discover shared artwork and native/Legend workflows',all(n in tools for n in ['set_footprint_artwork','import_footprint','export_footprint','save_footprint_definition','export_fabrication','run_output_job','inspect_output_release']) and {t['name']for t in clients[1].rpc('tools/list',{})['tools']}==set(tools))
 initial=state();check('owned scratch identity verified before edits',initial['project']['name']=='T119 explicit artwork owned proof')
 ids=sorted(initial['project']['board']['parts']);top=next(i for i in ids if initial['project']['board']['parts'][i]['side']=='top');bottom=next(i for i in ids if initial['project']['board']['parts'][i]['side']=='bottom')
 check('independent observation preserves full session and status',call('get_session_state',peer=True)==initial and call('get_design',peer=True)==initial['project'])
 oldsvg=call('render_board_view',{})['svg'];baseline=call('export_fabrication',{'path':str(out/'legacy-before'),'expected_cursor':initial['cursor']})
 art={'silkscreen':[{'start':{'x':-3000003,'y':-2000005},'end':{'x':3000007,'y':-2000005},'width':150003},{'start':{'x':3000007,'y':-2000005},'end':{'x':3000007,'y':2000011},'width':150003}], 'courtyard':{'min':{'x':-4000003,'y':-3000005},'max':{'x':4000007,'y':3000011},'width':100001}}
 def guard(id,artwork):
  s=state();return {'expected_project':s['project']['id'],'expected_cursor':s['cursor'],'expected':s['project']['board']['parts'][id],'artwork':artwork}
 request=guard(top,art);before=state();result=timed('top_artwork_commit','set_footprint_artwork',request);changed=state();expected=copy.deepcopy(before['project']);expected['board']['parts'][top]['artwork']=art
 check('guarded artwork is one Part delta and one event with exact unrelated geometry',changed['project']==expected and len(result['deltas'])==1 and changed['cursor']['sequence']==before['cursor']['sequence']+1)
 check('peer sees identical stored artwork',call('get_design',peer=True)==changed['project'])
 reject('set_footprint_artwork',request);check('duplicate stale full cursor refuses unchanged')
 call('undo');check('one creator undo restores exact no-art project',state()['project']==before['project']);call('redo');check('one redo restores exact explicit art project',state()['project']==changed['project'])
 before=state();call('set_footprint_artwork',guard(top,art));check('exact artwork no-op emits no event',state()==before)
 for label,bad in [('257 raw poison rows',{'silkscreen':['poison']*257}),('zero stroke',{'silkscreen':[dict(art['silkscreen'][0],width=0)]}),('collapsed line',{'silkscreen':[dict(art['silkscreen'][0],end=art['silkscreen'][0]['start'])]}),('reversed courtyard',{'silkscreen':[],'courtyard':dict(art['courtyard'],min=art['courtyard']['max'])}),('unknown point',{'silkscreen':[dict(art['silkscreen'][0],start={'x':0,'y':0,'poison':1})]})]:
  reject('set_footprint_artwork',guard(top,bad));check(label+' refuses full state/status unchanged')
 stale=guard(bottom,art);call('add_part',{'designator':'PEER119','value':'keep','footprint':'legacy','x_mm':2,'y_mm':2},peer=True);call('undo',peer=True);reject('set_footprint_artwork',stale);check('peer edit plus undo still invalidates initial cursor')
 timed('bottom_artwork_commit','set_footprint_artwork',guard(bottom,art));authored=state();check('both sides contain explicit art without modifying IDs/pads/nets',all(authored['project']['board']['parts'][i]['pads']==initial['project']['board']['parts'][i]['pads'] for i in ids))
 check('nonempty artwork preserves default SVG bytes',call('render_board_view',{})['svg']==oldsvg)
 rendered=call('render_board_view',{'options':{'layers':['top_silkscreen','bottom_silkscreen','top_courtyard','bottom_courtyard']}});check('explicit SVG layers render only requested stored art',rendered['metadata']['artwork']['silkscreen_segments']==4 and rendered['metadata']['artwork']['courtyards']==2 and '<line ' in rendered['svg'])
 library=out/'explicit.footprint.json';g=guard(top,art);libargs={k:v for k,v in g.items()if k!='artwork'};libargs['path']=str(library);call('export_footprint',libargs);lib=json.loads(library.read_bytes());check('native library2 export retains exact non-null art and omits pad IDs/nets',lib['version']==2 and lib['footprint']['artwork']==art and all('id'not in x and 'net'not in x for x in lib['footprint']['pads']))
 before=state();call('set_footprint_artwork',guard(top,{'silkscreen':[]}));check('explicit empty remains stored and feature13',state()['project']['board']['parts'][top]['artwork']=={'silkscreen':[]})
 imp={k:v for k,v in guard(top,None).items()if k!='artwork'};imp['path']=str(library);missing=copy.deepcopy(imp);missing.pop('expected_cursor');reject('import_footprint',missing);check('library2 import requires full source cursor')
 call('import_footprint',imp);check('library2 guarded import restores art and preserves pads IDs/nets',state()['project']['board']['parts'][top]['artwork']==art and state()['project']['board']['parts'][top]['pads']==authored['project']['board']['parts'][top]['pads'])
 native=out/'authored.viamind.json';call('save_project',{'path':str(native)});saved=json.loads(native.read_bytes());check('native save uses13 and exact full stored project',saved['version']==13 and saved['project']==state()['project'])
 stale=guard(top,None);call('open_project',{'path':str(native)});check('same UUID reopen replaces stream',state()['project']['id']==stale['expected_project'] and state()['cursor']['stream_id']!=stale['expected_cursor']['stream_id']);reject('set_footprint_artwork',stale);check('same UUID old-stream edit refuses unchanged')
 before,st=state(),status();legacy=call('export_fabrication',{'path':str(out/'legacy-after'),'expected_cursor':before['cursor']});legend=timed('legend_export','export_fabrication',{'path':str(out/'legend'),'include_silkscreen':True,'expected_cursor':before['cursor']})
 check('explicit Legend opt-in adds paired files with correct functions',len(list((out/'legend').iterdir()))==9 and 'Legend,Top'in(out/'legend/top-silkscreen.gbr').read_text() and 'Legend,Bot'in(out/'legend/bottom-silkscreen.gbr').read_text())
 check('default fabrication copper/mask/drill/profile bytes remain exact',all((out/'legacy-before'/name).read_bytes()==(out/'legacy-after'/name).read_bytes() for name in baseline['manifest']['files']if name!='manifest.json'))
 check('actual manifest discloses emitted art and excluded courtyard',legend['manifest']['footprint_artwork']['top_silkscreen_segments']==2 and legend['manifest']['footprint_artwork']['bottom_silkscreen_segments']==2 and legend['manifest']['footprint_artwork']['courtyard_count']==2 and state()==before and status()==st)
 job={'format':'viamind-output-job','version':4,'name':'T119 explicit Legend','profile':'generic_outer_2layer','mask_expansion_mm':0,'include_silkscreen':True};jobpath=out/'legend.job.json';call('save_output_job',{'path':str(jobpath),'job':job});check('detached job4 canonical load retains explicit opt-in',call('load_output_job',{'path':str(jobpath)})['job']==job)
 release=timed('job4_release','run_output_job',{'job_path':str(jobpath),'path':str(out/'release'),'expected_cursor':before['cursor']});inspection=call('inspect_output_release',{'path':str(out/'release')});check('actual job4 release and independent reader agree13files/exactcursor',release['file_count']==13 and inspection['file_count']==13 and inspection['exported_cursor']==before['cursor'])
 check('exports/jobs/inspection preserve complete session/status',state()==before and status()==st)
 with StdioClient([str(binary),'--headless','--agent-read-only','--open',str(native)],out,timeout=60) as client:
  client.initialize();names={t['name']for t in client.rpc('tools/list',{})['tools']};check('actual read-only stdio observes stored art and hides guarded mutation','set_footprint_artwork'not in names and client.call('get_design',{})==before['project']);client.finish()
 cli=out/'cli-legend';env=dict(os.environ);env.pop('DISPLAY',None);env.pop('WAYLAND_DISPLAY',None);completed=subprocess.run([str(binary),'--open',str(native),'--export-gerber','--output',str(cli),'--include-silkscreen'],env=env,capture_output=True,text=True,timeout=60);check('actual no-display CLI Legend export matches same captured geometry',completed.returncode==0 and (cli/'top-silkscreen.gbr').read_bytes()==(out/'legend/top-silkscreen.gbr').read_bytes());report['cli']={'returncode':completed.returncode,'stdout':completed.stdout,'stderr':completed.stderr}
 check('final two clients still agree on exact saved native session',call('get_session_state',peer=True)==state() and state()['project']==saved['project'])
 report.update(passed=True,check_count=len(report['checks']),native=str(native),library=str(library),final_state=state(),final_status=status())
finally:
 for client in clients:client.close()
 with (out/'production-http.json').open('x')as f:json.dump(report,f,indent=2);f.flush();os.fsync(f.fileno())
print(json.dumps({'passed':report['passed'],'checks':len(report['checks']),'report':str(out/'production-http.json')}))
