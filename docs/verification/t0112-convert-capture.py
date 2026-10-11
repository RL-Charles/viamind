from pathlib import Path
import hashlib,json,os,subprocess
root=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0112')
source=root/'native-framebuffer.ppm'
ppm=source.read_bytes()
magic,size,maximum,rgb=ppm.split(b'\n',3)
assert magic==b'P6' and size==b'1600 1000' and maximum==b'255' and len(rgb)==1600*1000*3
png=subprocess.check_output(['magick',str(source),'png:-'])
assert len(png)<25*1024*1024
path=root/'native-framebuffer.png'
with path.open('xb') as f:f.write(png);f.flush();os.fsync(f.fileno())
readback=subprocess.check_output(['magick',str(path),'-depth','8','RGB:-'])
assert readback==rgb
report={'passed':True,'route':'ImageMagick P6 RGB to PNG then independent PNG decode to raw RGB','ppm_sha256':hashlib.sha256(ppm).hexdigest(),'png_sha256':hashlib.sha256(png).hexdigest(),'pixels':[1600,1000],'decoded_rgb_exactly_equal':True,'new_png_file':str(path),'tool_version':subprocess.check_output(['magick','-version'],text=True).splitlines()[0]}
with (root/'capture-conversion.json').open('x') as f:json.dump(report,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
print(json.dumps(report))
