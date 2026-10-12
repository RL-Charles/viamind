import hashlib,json,os
from pathlib import Path
p=Path(__file__).parent;d=p/'hardened-pairs';t=json.loads((d/'trace.json').read_bytes());r=json.loads((p/'hardened-report.json').read_bytes())
assert t['paired_provenance_valid'] and t['failure'] is None and t['requests']==10 and len(t['images'])==10
assert not Path('/proc/4006395').exists()
assert hashlib.sha256((p/'trace-c0e2ccd3/gpu_parity').read_bytes()).hexdigest()=='660e5de117a60afd86c8cf9ceb1e58a27890230edf275a6b899dbcac59cde399'
images=[];hashes=[]
for i,v in enumerate(t['images']):
 raw=(d/f'image-{i:02}.p7').read_bytes();h,data=raw.split(b'ENDHDR\n',1)
 assert b'WIDTH 1400\nHEIGHT 1000\nDEPTH 4\n' in h and len(data)==5600000
 assert v['dimensions']==v['request']['state']['root_pixels']==[1400,1000]
 assert v['token']==v['request']['token']=={'step':i,'sequence':i}
 for k in ['valid_token','viewport_matches','state_matches_request','draw_states_match_request','pair_state_matches_reference','pair_dimensions_match_reference','request_dimensions_match']:assert v[k] is True
 assert v['request']['completed_stable_frames']>=6 and v['request']['frame']['frame']<v['arrival']['frame']
 assert v['request']['state']==v['arrival_state']
 for draw in t['ui_pass_receipts'][v['request']['draw_start']:v['draw_end']]:assert draw['state']==v['request']['state']
 images.append(data);hashes.append(hashlib.sha256(raw).hexdigest())
cases=[]
for i in range(5):
 a,b=images[2*i:2*i+2];s=t['images'][2*i]['arrival_state'];assert s==t['images'][2*i+1]['arrival_state']
 x0,y0,x1,y1=(round(v*s['pixels_per_point']) for v in s['viewport']);size=(x1-x0)*(y1-y0)
 ar=b''.join(a[(y*1400+x0)*4:(y*1400+x1)*4]for y in range(y0,y1));br=b''.join(b[(y*1400+x0)*4:(y*1400+x1)*4]for y in range(y0,y1))
 delta=[bytes(abs(x-y)for x,y in zip(ar[c::4],br[c::4]))for c in range(4)]
 actual={'case':i,'pixels':size,'max_channel_difference':max(map(max,delta)),'mean_channel_difference':sum(map(sum,delta))/(size*4),'pixels_over_2_channels':sum(max(c)>2 for c in zip(*delta)),'drawn':t['ui_pass_receipts'][t['images'][2*i]['request']['draw_start']]['drawn']}
 assert actual==r['cases'][i],(actual,r['cases'][i]);cases.append(actual)
assert r['passed'] and all(c['pixels_over_2_channels']==0 for c in cases)
v={'passed':True,'source':'c0e2ccd3db319a5b8dd1b1900c3c2891a0097e05','native_pid':4006395,'process_absent':True,'raw_p7_sha256':hashes,'all10_request_root_dimensions_token_and_intervening_states_exact':True,'all_requests_completed_stable_frames_at_least6':True,'cases':cases,'verification':'Standard-library exact raw RGBA four-channel comparison; original harness mean includes alpha. Initial optional Pillow unavailable; initial RGB-only mean expectation corrected after independently reading original four-channel denominator. No product change.','scope':'Current bounded provenance and image parity only; historical581 mismatch/stall/cadence unresolved'}
with(p/'root-hardened-independent.json').open('x')as f:json.dump(v,f,indent=2);f.flush();os.fsync(f.fileno())
print('PASS10 raw captures, exact request/token/state and all5 independently recomputed RGBA metrics')
