import json,os
from pathlib import Path
root=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0126')
t=json.loads((root/'hardened-pairs/trace.json').read_text());r=json.loads((root/'hardened-report.json').read_text());previous=json.loads((root/'progress-pairs/trace.json').read_text())
assert t['mode']=='hardened_manual_camera_completed_frame_trace' and t['failure'] is None
assert t['requests']==len(t['images'])==10 and t['paired_provenance_valid']
assert t['metadata_charged']<=1048576 and t['raw_bytes']==56000000
assert 'rejected_navigation' not in t
requests=[]
for index,im in enumerate(t['images']):
 p=im['request'];state=p['state'];draws=t['ui_pass_receipts'][p['draw_start']:im['draw_end']]
 assert p['token']==im['token']=={'step':index,'sequence':index}
 assert im['dimensions']==state['root_pixels']==im['arrival_state']['root_pixels']==[1400,1000]
 assert p['completed_stable_frames']>=6
 assert state==im['arrival_state'] and all(d['state']==state for d in draws)
 assert all(im[k] for k in ['valid_token','viewport_matches','state_matches_request','draw_states_match_request','pair_state_matches_reference','pair_dimensions_match_reference','request_dimensions_match'])
 assert p['frame']['frame']<im['arrival']['frame']
 legacy={k:v for k,v in im['arrival_state'].items() if k!='root_pixels'}
 assert legacy==previous['images'][index]['arrival_state']
 for d in draws:
  n=d['navigation_input'];assert n['wheel']==n['zoom']==n['home']==0 and n['smooth_scroll']==[0.0,0.0] and n['zoom_delta']==1.0
  assert not n['home_held'] and not n['middle_held'] and not(n['space_held'] and n['pointer_delta']!=[0.0,0.0])
 requests.append({'step':index,'request_frame':p['frame'],'arrival_frame':im['arrival'],'completed_stable_frames':p['completed_stable_frames'],'root_pixels':state['root_pixels'],'matching_intervening_draws':len(draws)})
assert r==json.loads((root/'progress-report.json').read_text())
out={'scope':'Independent retained request/draw/arrival and prior diagnostic state audit; source guard enforces no known-discard submission. No historical cause/renderer/performance claim.','passed':True,'requests':requests,'ui_pass_receipts':len(t['ui_pass_receipts']),'distinct_observed_integration_frame_numbers':len(set(d['frame']['frame'] for d in t['ui_pass_receipts'])),'all_states_equal_previous_progress_except_additive_root_pixels':True,'same_all_five_metrics_as_previous_progress':True,'all_root_dimension_admissions':True,'all_requests_have_six_completed_stable_frames':True,'no_retained_external_navigation_contamination':True}
with (root/'hardened-admission-audit.json').open('x') as f:json.dump(out,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
print(json.dumps(out,indent=2))
