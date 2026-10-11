import argparse,hashlib,json,pathlib
from PIL import Image
p=argparse.ArgumentParser();p.add_argument('directory',type=pathlib.Path);a=p.parse_args();root=a.directory.resolve();rows=[]
sha=lambda b:hashlib.sha256(b).hexdigest()
for mode in ['wide','veto','recovery']:
 name='hooks-'+mode;raw=(root/(name+'.p7')).read_bytes();head,rgba=raw.split(b'ENDHDR\n',1);fields=dict(line.split(b' ',1)for line in head.splitlines()[1:]);w=int(fields[b'WIDTH']);h=int(fields[b'HEIGHT']);assert fields[b'DEPTH']==b'4'and fields[b'TUPLTYPE']==b'RGB_ALPHA'and len(rgba)==w*h*4
 rgb=bytearray(w*h*3);rgb[0::3]=rgba[0::4];rgb[1::3]=rgba[1::4];rgb[2::3]=rgba[2::4];p6=(root/(name+'.p6')).read_bytes();assert Image.open(root/(name+'.p6')).convert('RGB').tobytes()==rgb
 png=root/(name+'.png');assert not png.exists();Image.frombytes('RGBA',(w,h),rgba).save(png);assert Image.open(png).mode=='RGBA'and Image.open(png).tobytes()==rgba
 prov=json.loads((root/(name+'-provenance.json')).read_text());native=prov['native'];assert native['pixels']==[w,h]and native['pid']==prov['launch']['pid']and not pathlib.Path('/proc/'+str(native['pid'])).exists()
 state=json.loads((root/(name+'-state.json')).read_bytes());assert state['veto_rollback']and state['actual_local_input']and state['native_unchanged']and state['hooks']['config']['enabled']==(mode!='recovery')
 for r,c in native['controls_and_effective_clips']:assert c[0]<=r[0]<=r[2]<=c[2]and c[1]<=r[1]<=r[3]<=c[3]
 rows.append({'name':name,'pixels':[w,h],'pid':native['pid'],'self_closed':True,'p7_path':str(root/(name+'.p7')),'p7_sha256':sha(raw),'p6_path':str(root/(name+'.p6')),'p6_sha256':sha(p6),'rgba_sha256':sha(rgba),'rgb_sha256':sha(rgb),'png_path':str(png),'png_sha256':sha(png.read_bytes()),'png_mode':'RGBA','every_original_color32_rgba_and_rgb_sample_equal':True,'alpha_range':[min(rgba[3::4]),max(rgba[3::4])],'nonopaque_alpha_pixels':sum(v!=255 for v in rgba[3::4]),'scope':'Original egui Color32 RGBA channel samples exactly retained in P7 and PNG; P6 exact RGB. No opacity/compositor/cadence/OS-input claim.'})
with(root/'captures.json').open('x')as f:json.dump(rows,f,indent=2)
print(json.dumps({'passed':True,'captures':len(rows)}))
