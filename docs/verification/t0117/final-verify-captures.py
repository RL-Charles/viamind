from pathlib import Path
import hashlib,json,subprocess,os
root=Path(__file__).resolve().parent
rows=[]
for mode in ['wide','compact','accepted','rejected','locked','busy']:
 p7=root/(mode+'.p7');p6=root/(mode+'.p6');png=root/(mode+'.png')
 header,rgba=p7.read_bytes().split(b'ENDHDR\n',1)
 fields=dict(line.split(b' ',1) for line in header.splitlines()[1:])
 assert fields[b'DEPTH']==b'4' and fields[b'MAXVAL']==b'255' and fields[b'TUPLTYPE']==b'RGB_ALPHA'
 width,height=int(fields[b'WIDTH']),int(fields[b'HEIGHT'])
 magic,size,maximum,rgb=p6.read_bytes().split(b'\n',3)
 assert magic==b'P6' and size==f'{width} {height}'.encode() and maximum==b'255'
 assert len(rgba)==width*height*4 and len(rgb)==width*height*3
 assert bytes(v for i,v in enumerate(rgba) if i%4!=3)==rgb
 encoded=subprocess.check_output(['magick',str(p7),'png:-'])
 with png.open('xb') as f:f.write(encoded);f.flush();os.fsync(f.fileno())
 assert subprocess.check_output(['magick',str(png),'-depth','8','RGBA:-'])==rgba
 runtime=(root/(mode+'-runtime.log')).read_text()
 capture=[json.loads(line) for line in runtime.splitlines() if line.startswith('{') and 'actual eframe native WGPU Screenshot event' in line]
 assert len(capture)==1;capture=capture[0]
 assert capture['full_session_unchanged'] and capture['document_status_unchanged']
 assert capture['pixels']==[width,height] and capture['original_color32_rgba_retained']
 launch=json.loads((root/(mode+'-launch.json')).read_text())
 assert capture['pid']==launch['pid']
 assert not Path('/proc',str(capture['pid'])).exists(),'inspection must have actually exited'
 rows.append({'mode':mode,'pid':capture['pid'],'passed':True,'pixels':[width,height],'all_rgba_rgb_bytes_equal':True,'original_alpha_min':min(rgba[3::4]),'original_alpha_max':max(rgba[3::4]),'original_nonopaque_pixels':sum(a!=255 for a in rgba[3::4]),'actual_process_exited':True,'capture':capture,'sha256':{ext:hashlib.sha256((root/(mode+ext)).read_bytes()).hexdigest() for ext in ['.p7','.p6','.png']}})
report={'passed':True,'route':'native WGPU P7 RGBA and P6 RGB; PNG conversion with independent complete RGBA readback comparison','captures':rows,'tool_version':subprocess.check_output(['magick','-version'],text=True).splitlines()[0]}
with (root/'capture-verification.json').open('x') as f:json.dump(report,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
print(json.dumps({'passed':True,'captures':len(rows)}))
