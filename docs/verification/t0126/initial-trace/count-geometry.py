import math,json
from pathlib import Path
mask=(1<<64)-1;state=0x05EED0FB0A2D
def u64():
 global state
 state=(state+0x9E3779B97F4A7C15)&mask;z=state;z=((z^(z>>30))*0xBF58476D1CE4E5B9)&mask;z=((z^(z>>27))*0x94D049BB133111EB)&mask;return z^(z>>31)
def f():return(u64()>>11)/float(1<<53)
def r(a,b):return a+(b-a)*f()
w=math.sqrt(5000);h=round(w*.7);items=[]
def add(box,layer):items.append((box,layer))
corners=[(0,0),(w,0),(w,h),(0,h)]
for i,a in enumerate(corners):
 b=corners[(i+1)%4];add((min(a[0],b[0])-.1,min(a[1],b[1])-.1,max(a[0],b[0])+.1,max(a[1],b[1])+.1),'edge')
while len(items)<5000:
 x,y=r(1,w-1),r(1,h-1);front=f()<.6;kind=f()
 if kind<.30:
  hw,hh=r(.15,1),r(.15,1);add((x-hw,y-hh,x+hw,y+hh),'front'if front else'back')
  if f()<.5 and len(items)<5000:add((x-hw-.05,y-hh-.05,x+hw+.05,y+hh+.05),'mask')
 elif kind<.45:
  radius=r(.2,.45);add((x-radius,y-radius,x+radius,y+radius),'via')
 else:
  silk=kind>=.85;angle=(u64()%8)*math.pi/4;length=r(.5,3 if silk else 8);bx=max(1,min(w-1,x+length*math.cos(angle)));by=max(1,min(h-1,y+length*math.sin(angle)))
  if silk:layer,width='silk',.15
  else:layer=['front','front','in1','in2','back'][u64()%5];width=r(.1,.5)
  add((min(x,bx)-width/2,min(y,by)-width/2,max(x,bx)+width/2,max(y,by)+width/2),layer)
add((142,92,158,108),'via');add((157,100,166,108),'front')
t=json.loads(Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0126/initial-pairs/trace.json').read_text());s=t['images'][9]['arrival_state'];cx,cy=s['camera_center'];zoom=s['camera_zoom'];v=s['viewport']
def count(cx,cy,z):
 ex=(v[2]-v[0])/z/2+2/z;ey=(v[3]-v[1])/z/2+2/z
 return sum(layer!='in1'and box[0]<=cx+ex and cx-ex<=box[2]and box[1]<=cy+ey and cy-ey<=box[3]for box,layer in items)
result={'scope':'Independent deterministic synthetic bounds/culling analysis only; not original camera provenance','current_count':count(cx,cy,zoom),'candidate_variations':[]}
for factor in[1/1.04,1,1.04,1.04**2]:
 z=zoom*factor
 for mode in['same_center','pan83at_newzoom']:
  x,y=(cx,cy)if mode=='same_center'else(w/2-83/z,h/2+47/z)
  result['candidate_variations'].append({'zoom_factor':factor,'mode':mode,'camera':[x,y,z],'count':count(x,y,z)})
print(json.dumps(result,indent=2))
Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0126/geometry-count-analysis.json').write_text(json.dumps(result,indent=2)+'\n')
