#!/usr/bin/env python3
"""Exercise command hook settings and real veto/recovery on an owned engine."""
import argparse,contextlib,hashlib,importlib.util,json,pathlib,sys,tempfile,time,subprocess
sys.dont_write_bytecode=True
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',type=pathlib.Path,required=True);p.add_argument('--repo',type=pathlib.Path,required=True);p.add_argument('--report',type=pathlib.Path,required=True);p.add_argument('--url');p.add_argument('--pid',type=int);p.add_argument('--native',type=pathlib.Path);p.add_argument('--mode',choices=['live','approval','read_only'],default='live');a=p.parse_args()
sys.path.insert(0,str(a.repo/'scripts'));from mcp_reference import StdioClient,digest
binary=a.binary.resolve(strict=True);checks=[];timings=[]
def check(name,ok):
 if not ok:raise AssertionError(name)
 checks.append(name)
with tempfile.TemporaryDirectory(prefix='viamind-hook-proof-') as tmp,contextlib.ExitStack() as stack:
 if a.url:
  assert a.pid and a.native and a.mode=='live','HTTP requires explicit owned live PID/native'
  assert pathlib.Path(f'/proc/{a.pid}/exe').resolve()==binary
  port=a.url.split(':')[2].split('/')[0];sockets=subprocess.check_output(['ss','-ltnp',f'( sport = :{port} )'],text=True);assert f'pid={a.pid},' in sockets
  spec=importlib.util.spec_from_file_location('smoke',a.repo/'scripts/mcp-smoke.py');smoke=importlib.util.module_from_spec(spec);spec.loader.exec_module(smoke)
  clients=[smoke.McpClient(a.url,'hook-proof-a'),smoke.McpClient(a.url,'hook-proof-b')]
  for c in clients:stack.callback(c.close)
 else:
  flags={'live':[],'approval':['--agent-approval'],'read_only':['--agent-read-only']}[a.mode]
  clients=[stack.enter_context(StdioClient([str(binary),'--headless',*flags],pathlib.Path(tmp)))];clients[0].initialize()
 def call(name,args=None,error=False,peer=False):
  start=time.monotonic()
  try:r=clients[int(bool(a.url)and peer)].rpc('tools/call',{'name':name,'arguments':{} if args is None else args})
  except ValueError as exc:
   assert error and name=='configure_command_hooks' and a.mode!='live' and str(exc)=='MCP protocol error during tools/call',str(exc)
   return {'protocol_refusal':str(exc)}
  timings.append({'tool':name,'ms':1000*(time.monotonic()-start)})
  assert bool(r.get('isError',False))==error,(name,r)
  if error:return r
  if 'structuredContent' in r:return r['structuredContent']
  return json.loads(next(c['text']for c in r['content']if c['type']=='text'))
 tools=clients[0].rpc('tools/list',{})['tools'];names={t['name']for t in tools}
 check('explicit hook observation discovered','get_command_hooks'in names)
 check('configuration only advertised to Live writers',('configure_command_hooks'in names)==(a.mode=='live'))
 state=call('get_session_state');status=call('get_document_status');auto=call('get_auto_drc_status');hooks=call('get_command_hooks')
 if a.url:
  hooks=call('configure_command_hooks',{})
  check('owned HTTP settings normalized disabled without callbacks',not hooks['config']['enabled'] and hooks['pre_callbacks']==[] and hooks['post_callbacks']==[])
 else:check('default disabled and no registered callbacks',not hooks['config']['enabled'] and hooks['pre_callbacks']==[] and hooks['post_callbacks']==[] and hooks['generation']==0)
 if a.mode!='live':
  call('configure_command_hooks',{'enabled':True},error=True)
  check('restricted configuration preserves full session and file status',call('get_session_state')==state and call('get_document_status')==status and call('get_auto_drc_status')==auto and call('get_command_hooks')==hooks)
 else:
  native=a.native.resolve() if a.native else pathlib.Path(tmp)/'protected.viamind.json'
  if not a.url:call('save_project',{'path':str(native)});status=call('get_document_status')
  else:check('owned active native path verified',pathlib.Path(status['path']).resolve()==native)
  original_bytes=native.read_bytes();state=call('get_session_state');auto=call('get_auto_drc_status')
  baseline=call('configure_command_hooks',{})
  check('exact no-op config retains generation',baseline['generation']==hooks['generation'])
  enabled=call('configure_command_hooks',{'enabled':True})
  check('enabled builtin first with checked generation',enabled['generation']==hooks['generation']+1 and enabled['pre_callbacks']==['builtin_drc'])
  check('peer observes identical settings',call('get_command_hooks',peer=True)==enabled)
  check('configuration preserves session/native status/AutoDRC',call('get_session_state')==state and call('get_document_status')==status and call('get_auto_drc_status')==auto)
  legacy={'designator':'REJECT','value':'legacy','footprint':'F','x_mm':0,'y_mm':0}
  call('add_part',legacy,error=True,peer=True)
  check('legacy candidate veto preserves full session/document/AutoDRC',call('get_session_state')==state and call('get_document_status')==status and call('get_auto_drc_status')==auto)
  rejected=call('get_command_hooks');d=rejected['diagnostics'][-1]
  check('bounded diagnostic identifies actual source and callback',d['callback_id']=='builtin_drc' and d['source_cursor']==state['cursor'] and len(d['message'].encode())<=1024)
  check('veto publishes no design or selection event',call('watch_events',{'after':state['cursor']})['events']==[])
  for bad in [{'max_objects':0},{'max_objects':10001},{'enabled':'yes'},{'extra':True},{'rules':{'copper_clearance_mm':101}},{'rules':{'max_violations':0}},{'rules':{'extra':True}}]:
   before=call('get_command_hooks');call('configure_command_hooks',bad,error=True);check('invalid config refuses atomically '+json.dumps(bad,sort_keys=True),call('get_command_hooks')==before and call('get_session_state')==state)
  call('get_command_hooks',{'extra':True},error=True)
  call('configure_command_hooks',{'enabled':False});added=call('add_part',legacy,peer=True)
  check('veto consumed no transaction ID',added['tx']==1)
  check('recovery publishes exactly one design event',call('get_session_state')['cursor']['sequence']==state['cursor']['sequence']+1)
  call('configure_command_hooks',{'enabled':True});call('undo',peer=True);undone=call('get_session_state');undone_status=call('get_document_status');undone_auto=call('get_auto_drc_status')
  check('undo escapes validator and restores exact project',undone['project']==state['project'])
  call('redo',error=True,peer=True)
  check('redo candidate veto preserves full state',call('get_session_state')==undone and call('get_document_status')==undone_status and call('get_auto_drc_status')==undone_auto)
  via={'x_mm':20,'y_mm':20,'diameter_mm':1,'drill_mm':0.4,'net':'A'}
  passed=call('add_via',via,peer=True)
  check('actual scoped DRC valid candidate commits without ID gap',passed['tx']==2)
  call('configure_command_hooks',{'enabled':True,'max_objects':1});limited=call('get_session_state');limited_auto=call('get_auto_drc_status')
  call('add_via',{**via,'x_mm':30},error=True,peer=True)
  check('borrowed object admission veto preserves full state',call('get_session_state')==limited and call('get_auto_drc_status')==limited_auto)
  call('configure_command_hooks',{'enabled':True});valid=call('add_via',{**via,'x_mm':30},peer=True)
  check('budget refusal consumes no ID and recovers',valid['tx']==3)
  config_before=call('get_command_hooks');current=call('get_session_state')
  call('new_project',{'name':'Owned hook lifecycle replacement','discard_dirty':True,'expected_cursor':current['cursor']})
  check('hook configuration and diagnostics survive New',call('get_command_hooks')==config_before)
  new=call('get_session_state');call('open_project',{'path':str(native),'expected_cursor':new['cursor']})
  check('hook configuration survives same-native reopen with new stream',call('get_command_hooks')==config_before and call('get_session_state')['cursor']['stream_id']!=state['cursor']['stream_id'])
  check('original owned native bytes unchanged through all operations',native.read_bytes()==original_bytes)
 report={'passed':True,'mode':a.mode,'checks':checks,'check_count':len(checks),'binary':str(binary),'sha256':digest(binary),'http_url':a.url,'pid':a.pid,'os_input_claim':False,'timings':timings}
 assert not a.report.exists();a.report.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'passed':True,'checks':len(checks),'report':str(a.report)}))
