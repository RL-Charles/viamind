from pathlib import Path
import hashlib,json,os,subprocess,sys
root=Path(sys.argv[1]);reports=[]
for stem in ('native-preview','native-created'):
 source=root/(stem+'.ppm');ppm=source.read_bytes();pam=(root/(stem+'.pam')).read_bytes()
 magic,size,maximum,rgb=ppm.split(b'\n',3)
 assert magic==b'P6' and size==b'1600 1000' and maximum==b'255' and len(rgb)==1600*1000*3
 header,rgba=pam.split(b'ENDHDR\n',1)
 assert header==b'P7\nWIDTH 1600\nHEIGHT 1000\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\n' and len(rgba)==1600*1000*4
 assert bytes(v for i,v in enumerate(rgba) if i%4!=3)==rgb
 png=subprocess.check_output(['magick',str(root/(stem+'.pam')),'png:-'])
 assert len(png)<25*1024*1024
 path=root/(stem+'.png')
 with path.open('xb') as f:f.write(png);f.flush();os.fsync(f.fileno())
 readback=subprocess.check_output(['magick',str(path),'-depth','8','RGBA:-'])
 assert readback==rgba
 reports.append({'passed':True,'route':'ImageMagick original native P7 RGBA to PNG then independent PNG decode to raw RGBA; RGB compared separately against P6','ppm_sha256':hashlib.sha256(ppm).hexdigest(),'pam_sha256':hashlib.sha256(pam).hexdigest(),'png_sha256':hashlib.sha256(png).hexdigest(),'pixels':[1600,1000],'decoded_rgba_exactly_equal':True,'decoded_rgb_exactly_equal':True,'alpha_min':min(rgba[3::4]),'alpha_max':max(rgba[3::4]),'nonopaque_pixels':sum(a!=255 for a in rgba[3::4]),'new_png_file':str(path)})
report={'passed':True,'captures':reports,'tool_version':subprocess.check_output(['magick','-version'],text=True).splitlines()[0]}
with (root/'capture-conversion.json').open('x') as f:json.dump(report,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
print(json.dumps(report))
