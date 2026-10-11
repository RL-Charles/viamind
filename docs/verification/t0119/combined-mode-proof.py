"""Initialized final-union executable proves artwork writer admission in all three modes."""
import argparse, hashlib, json, os, sys
from pathlib import Path
p=argparse.ArgumentParser()
for key in ('binary','native','repo','directory'):p.add_argument('--'+key,type=Path,required=True)
a=p.parse_args();root=a.directory.resolve();root.mkdir();sys.path.insert(0,str(a.repo.resolve()/'scripts'));sys.dont_write_bytecode=True
from mcp_reference import StdioClient
checks=[];ident=hashlib.sha256(a.binary.read_bytes()).hexdigest()
for mode,flag in [('live',None),('approval','--agent-approval'),('read_only','--agent-read-only')]:
 args=[str(a.binary.resolve()),'--headless','--open',str(a.native.resolve())]
 if flag:args.append(flag)
 with StdioClient(args,root,timeout=60) as c:
  c.initialize();before=c.call('get_session_state',{});status=c.call('get_document_status',{});names={t['name'] for t in c.rpc('tools/list',{})['tools']}
  assert ('set_footprint_artwork' in names)==(mode=='live');checks.append(mode+' tool list has correct artwork writer admission')
  assert c.call('get_agent_mode',{})['mode']==mode;checks.append(mode+' trusted startup status exact')
  part=next(iter(before['project']['board']['parts'].values()));request={'expected_project':before['project']['id'],'expected_cursor':before['cursor'],'expected':part,'artwork':part['artwork']}
  if mode=='live':c.call('set_footprint_artwork',request)
  else:
   try:c.call('set_footprint_artwork',request)
   except ValueError as e:assert 'MCP protocol error during tools/call' in str(e)
   else:raise AssertionError('hidden writer unexpectedly admitted')
  assert c.call('get_session_state',{})==before and c.call('get_document_status',{})==status;checks.append(mode+' writer/no-op call preserves complete captured session and native status')
  c.finish();checks.append(mode+' initialized clean EOF exits successfully')
assert hashlib.sha256(a.binary.read_bytes()).hexdigest()==ident
with (root/'verification.json').open('x') as f:json.dump({'passed':True,'binary':str(a.binary.resolve()),'sha256':ident,'native':str(a.native.resolve()),'checks':checks},f,indent=2);f.flush();os.fsync(f.fileno())
print(json.dumps({'passed':True,'checks':len(checks)}))
