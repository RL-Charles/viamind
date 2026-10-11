from client import Client
from pathlib import Path
import hashlib,json,datetime
root=Path(__file__).parent
a,b=Client(7941),Client(7941)
s=a.call('get_session_state',{});status=a.call('get_document_status',{})
for kind,collection in [('track','tracks'),('via','vias')]:
 id=next(iter(s['project']['board'][collection]));q=b.call('query_copper_properties',{'target':{'kind':kind,'id':id}})
 assert q['source_cursor']==s['cursor']and q['project_id']==s['project']['id']
 assert q['object'][kind]==s['project']['board'][collection][id]
assert a.call('get_session_state',{})==b.call('get_session_state',{})==s
assert a.call('get_document_status',{})==status
report={'passed':True,'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'pid':2281903,'live_exe_sha256':hashlib.sha256(Path('/proc/2281903/exe').read_bytes()).hexdigest(),'checks':['two initialized observers agree on full session','immutable track query matches exact original/full cursor','immutable via query matches exact original/full cursor','full session and document status unchanged'],'source':'2512b2ace979043e4c04dda077bcedb613ead89d','state':s,'document_status':status,'historical_pid':2165952,'historical_pid_absent_at_packaging':True,'cause':'undetermined; no matching coredump or kernel/journal error found; no crash inferred'}
(root/'production-restored-observers.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:report[k]for k in ['passed','pid','live_exe_sha256','checks']}))
