import datetime,gzip,hashlib,json,shutil
from pathlib import Path
repo=Path('/home/charlesmowbray/Development/viamind-t-0126');home=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0126');dest=repo/'docs/verification/t0126/hardened-trace';dest.mkdir()
for folder in ['pairs','analysis']:(dest/folder).mkdir()
for name in ['root-verify-hardened.py','root-hardened-independent.json','trace-gate9.sh','trace-gate9.log','trace-gate10.sh','trace-gate10.log','trace-gate11.sh','trace-gate11.log','audit-hardened.py','hardened-admission-audit.json','run-hardened-trace.py','analyze-progress.py','hardened-capture-ready.json','hardened-quiet-ready.json','hardened-system.json','hardened-launch.json','hardened-completed.json','hardened-report.json','hardened-runtime.log','hardened-orchestrator.log']:
 shutil.copyfile(home/name,dest/name)
shutil.copyfile(home/'trace-c0e2ccd3/provenance.json',dest/'binary-provenance.json')
raws=[]
for file in sorted((home/'hardened-pairs').iterdir()):
 if file.suffix=='.p7':
  data=file.read_bytes();target=dest/'pairs'/(file.name+'.gz');target.write_bytes(gzip.compress(data,mtime=0));raws.append({'path':str(target.relative_to(dest)),'original_bytes':len(data),'original_sha256':hashlib.sha256(data).hexdigest()})
 else:shutil.copyfile(file,dest/'pairs'/file.name)
for file in sorted((home/'hardened-analysis').iterdir()):
 if file.suffix=='.p6':
  data=file.read_bytes();target=dest/'pairs'/(file.name+'.gz');target.write_bytes(gzip.compress(data,mtime=0));raws.append({'path':str(target.relative_to(dest)),'original_bytes':len(data),'original_sha256':hashlib.sha256(data).hexdigest()})
 else:shutil.copyfile(file,dest/'analysis'/file.name)
verify=(repo/'docs/verification/t0126/initial-trace/verify.py').read_text().replace('initial diagnostic','hardened diagnostic')
(dest/'verify.py').write_text(verify)
initial=json.loads((home/'initial-pairs/trace.json').read_text());trace=json.loads((home/'hardened-pairs/trace.json').read_text());report=json.loads((home/'hardened-report.json').read_text());progress=json.loads((home/'hardened-pairs/progress.json').read_text());complete=json.loads((home/'hardened-completed.json').read_text())
comparison={'scope':'Current initial and hardened diagnostic states/metrics only; original failure has no exact camera receipts. Different full screenshots can include changing UI statistics outside the compared canvas. No cause closure.','same_ten_arrival_states_except_hardened_root_pixels':all(a['arrival_state']=={k:v for k,v in b['arrival_state'].items() if k!='root_pixels'}for a,b in zip(initial['images'],trace['images'])),'same_five_metrics':json.loads((home/'initial-report.json').read_text())==report,'initial_elapsed_seconds':initial['elapsed_seconds'],'hardened_elapsed_seconds':complete['elapsed_seconds'],'deferred_prior_elapsed_seconds':'>210, incomplete; no stage receipt','original_drawn_case4':581,'initial_progress_and_hardened_drawn_case4':606,'root_cause':'UNRESOLVED','stalled_stage_question':'Not answered: stall did not recur; terminal receipt coherent, all requests returned.','hardened_admission_audit':json.loads((home/'hardened-admission-audit.json').read_text())}
(dest/'comparison.json').write_text(json.dumps(comparison,indent=2)+'\n')
artifacts=[]
for file in sorted(dest.rglob('*')):
 if file.is_file():
  data=file.read_bytes();artifacts.append({'path':str(file.relative_to(dest)),'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
master={'schema':1,'ticket':'T-0126','source':'c0e2ccd3db319a5b8dd1b1900c3c2891a0097e05','approved_spec':'c912fc9ac04910ed061aef7926356f865831332e','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'ONE optional hardened manual-camera/completed-frame/root-pixel/token/input validation; no renderer change/historical cause/cadence claim; original five cases/criteria preserved.','binary':json.loads((dest/'binary-provenance.json').read_text()),'gate':'Gate11 source c0e2: fmt/release UI all-target strict Clippy/22 example tests/release example build PASS; four unchanged Canvas API/focus tests passed at ff180e3 gate9. Gate10 afb76d6 passed20 examples. No full workspace gate claim.','selfclosed_pid':4006395,'elapsed_seconds':complete['elapsed_seconds'],'images':10,'raw_bytes':trace['raw_bytes'],'metadata_charged':trace['metadata_charged'],'progress':progress,'paired_provenance_valid':trace['paired_provenance_valid'],'five_case_pass':report['passed'],'comparison':comparison,'samples':'Original P7 RGBA and P6 RGB retained as lossless gzip; independent ImageMagick decode verifies every RGBA/RGB PNG sample. RGB review omits alpha explicitly; original alpha retained.','raw_files':raws,'artifacts':artifacts,'remaining':'Historical mismatch and deferred stall causes unresolved; no renderer/cadence/strict60fps acceptance; no further native run authorized.'}
(dest/'master.json').write_text(json.dumps(master,indent=2)+'\n')
print(json.dumps({'artifacts':len(artifacts),'raw_files':len(raws),'path':str(dest/'master.json')},indent=2))
