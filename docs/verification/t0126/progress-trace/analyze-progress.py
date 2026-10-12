import collections,hashlib,json,math,os,struct,subprocess,zlib
from pathlib import Path
root=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0126')
source=root/'progress-pairs';out=root/'progress-analysis';out.mkdir()
trace=json.loads((source/'trace.json').read_text());report=json.loads((root/'progress-report.json').read_text())
def new(path,data):
 with path.open('xb')as f:f.write(data);f.flush();os.fsync(f.fileno())
def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
def png(path,w,h,raw,channels):
 filtered=b''.join(b'\0'+raw[y*w*channels:(y+1)*w*channels]for y in range(h))
 data=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,6 if channels==4 else 2,0,0,0))+chunk(b'IDAT',zlib.compress(filtered))+chunk(b'IEND',b'')
 new(path,data)
 decoded=subprocess.check_output(['magick',str(path),'-depth','8','rgba:-'if channels==4 else'rgb:-'])
 assert decoded==raw,('PNG sample roundtrip mismatch',path)
def read(index):
 path=source/f'image-{index:02}.p7';data=path.read_bytes();header,raw=data.split(b'ENDHDR\n',1)
 fields=dict(line.split(b' ',1)for line in header.splitlines()[1:]if b' 'in line)
 w,h=int(fields[b'WIDTH']),int(fields[b'HEIGHT']);assert len(raw)==w*h*4
 assert [w,h]==trace['images'][index]['dimensions'];return w,h,raw
artifacts=[]
for i in range(10):
 w,h,raw=read(i);rgb=bytearray(w*h*3);rgb[0::3]=raw[0::4];rgb[1::3]=raw[1::4];rgb[2::3]=raw[2::4]
 png(out/f'image-{i:02}-rgba.png',w,h,raw,4);png(out/f'image-{i:02}-rgb-review.png',w,h,bytes(rgb),3)
 new(out/f'image-{i:02}.p6',f'P6\n{w} {h}\n255\n'.encode()+rgb)
 alpha=raw[3::4];artifacts.append({'image':i,'width':w,'height':h,'original_rgba_sha256':hashlib.sha256(raw).hexdigest(),'alpha_min':min(alpha),'alpha_max':max(alpha),'nonopaque_pixels':sum(a!=255 for a in alpha),'rgba_and_rgb_png_samples_exact':True})
cases=[]
for case in range(5):
 w,h,a=read(case*2);w2,h2,b=read(case*2+1);assert(w,h)==(w2,h2)
 receipt=trace['images'][case*2+1];state=receipt['arrival_state'];ppp=state['pixels_per_point'];v=state['viewport'];x0,y0=math.ceil(v[0]*ppp),math.ceil(v[1]*ppp);x1,y1=min(math.floor(v[2]*ppp),w),min(math.floor(v[3]*ppp),h)
 total=maximum=changed=pixels=0;box=[w,h,-1,-1];colors=collections.Counter();rows=collections.Counter();columns=collections.Counter()
 diff=bytearray(w*h*3)
 for y in range(y0,y1):
  for x in range(x0,x1):
   n=(y*w+x)*4;d=tuple(abs(a[n+c]-b[n+c])for c in range(4));pixels+=1;total+=sum(d);maximum=max(maximum,*d)
   if max(d)>2:
    changed+=1;box=[min(box[0],x),min(box[1],y),max(box[2],x),max(box[3],y)];colors[(tuple(a[n:n+4]),tuple(b[n:n+4]))]+=1;rows[y]+=1;columns[x]+=1
   j=(y*w+x)*3;diff[j:j+3]=bytes(min(255,c*8)for c in d[:3])
 stats={'case':case,'crop':[x0,y0,x1,y1],'pixels':pixels,'max_channel_difference':maximum,'mean_channel_difference':total/(pixels*4),'pixels_over_2_channels':changed,'changed_bbox':box if changed else None,'most_common_changed_rgba_pairs':[(str(k),n)for k,n in colors.most_common(15)],'most_changed_rows':rows.most_common(10),'most_changed_columns':columns.most_common(10),'regions':receipt['regions'],'paired_provenance':{k:receipt[k]for k in['valid_token','state_matches_request','draw_states_match_request','pair_state_matches_reference']}}
 expected=report['cases'][case]
 for k in['pixels','max_channel_difference','pixels_over_2_channels']:assert stats[k]==expected[k],(case,k,stats[k],expected[k])
 assert abs(stats['mean_channel_difference']-expected['mean_channel_difference'])<1e-12
 png(out/f'case-{case}-difference-times8.png',w,h,bytes(diff),3)
 cases.append(stats)
result={'scope':'Independent original RGBA sample analysis; RGB review omits alpha, each sample verified; derived diff amplified8 only for review, original whole-canvas thresholds unchanged','trace_paired_provenance_valid':trace['paired_provenance_valid'],'artifacts':artifacts,'cases':cases}
new(out/'analysis.json',(json.dumps(result,indent=2)+'\n').encode());print(json.dumps({'paired_provenance_valid':trace['paired_provenance_valid'],'cases':[{k:v for k,v in c.items()if k in['case','changed_bbox','mean_channel_difference','pixels_over_2_channels','paired_provenance','regions']}for c in cases]},indent=2))
