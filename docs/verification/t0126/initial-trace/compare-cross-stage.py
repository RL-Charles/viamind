from pathlib import Path
import json,math
p=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0126');t=json.loads((p/'initial-pairs/trace.json').read_text());result=[]
def read(i):return(p/f'initial-pairs/image-{i:02}.p7').read_bytes().split(b'ENDHDR\n',1)[1]
for ia,ib in[(6,9),(7,9),(6,8),(7,8)]:
 a,b=read(ia),read(ib);w,h=t['images'][ib]['dimensions'];s=t['images'][ib]['arrival_state'];v=s['viewport'];ppp=s['pixels_per_point'];total=maximum=changed=pixels=0
 for y in range(math.ceil(v[1]*ppp),min(math.floor(v[3]*ppp),h)):
  for x in range(math.ceil(v[0]*ppp),min(math.floor(v[2]*ppp),w)):
   n=(y*w+x)*4;d=[abs(a[n+c]-b[n+c])for c in range(4)];total+=sum(d);maximum=max(maximum,*d);changed+=max(d)>2;pixels+=1
 result.append({'reference_image':ia,'compared_image':ib,'pixels':pixels,'max':maximum,'mean':total/(pixels*4),'over2':changed})
with(p/'initial-cross-stage.json').open('x')as f:json.dump(result,f,indent=2)
print(json.dumps(result,indent=2))
