#!/usr/bin/env python3
"""Independently inspect synthetic selected-sheet SVG/PNG (Pillow + librsvg).

Run BINARY fixtures/views/schematic.viamind.json NEW_DIRECTORY. No proprietary assets.
"""
import argparse
import json
from pathlib import Path
import subprocess
import xml.etree.ElementTree as ET
from PIL import Image

SHEET = '00000000-0000-0000-0000-000000000066'
NS = '{http://www.w3.org/2000/svg}'

def verify(binary, native, directory):
    checks = []
    def check(name, condition):
        if not condition:
            raise AssertionError(name)
        checks.append(name)
    def export(suffix, name, *options, source=native):
        path = directory / (name + '.' + suffix)
        subprocess.run([str(binary), '--export-schematic-' + suffix, '--sheet', SHEET,
                        '--open', str(source), '--output', str(path), *options], check=True)
        return path
    common = ['--view-bounds', '0,0,30,20', '--view-size', '900,600']
    svg = export('svg', 'schematic', *common)
    png = export('png', 'schematic', *common)
    root = ET.parse(svg).getroot()
    metadata = json.loads(root.find(NS + 'metadata').text)
    ids = {node.attrib.get('id', '') for node in root.iter()}
    check('outlined text has stable object IDs and no runtime fonts',
          not list(root.iter(NS+'text')) and any(i.startswith('pin-text-') for i in ids)
          and not any('href' in key for node in root.iter() for key in node.attrib))
    check('semantic Unicode and XML escaping preserved',
          any(t['text'] == 'U<&>1' for t in metadata['visible_text'])
          and any('10kΩ µA é Я' in t['text'] for t in metadata['visible_text']))
    check('unchanged font license accompanies outlines', 'BITSTREAM VERA LICENSE' in metadata['font_notice'])
    check('absolute region and pixel mapping declared', root.attrib['viewBox']=='0 0 900 600' and metadata['view']['region_nm'] ==
          {'min_x':0,'min_y':0,'max_x':30000000,'max_y':20000000})
    reference = directory / 'reference.png'
    subprocess.run(['rsvg-convert','--output',str(reference),str(svg)],check=True)
    with Image.open(png) as decoded:
        check('independent static RGBA8 decoding',decoded.mode == 'RGBA' and decoded.size == (900,600)
              and not getattr(decoded,'is_animated',False))
        product=decoded.copy()
    independent=Image.open(reference).convert('RGBA')
    interiors=0
    for y in range(2,598,3):
        for x in range(2,898,3):
            expected=independent.getpixel((x,y))
            if all(independent.getpixel((x+dx,y+dy))==expected for dx,dy in [(-1,-1),(1,-1),(-1,1),(1,1)]):
                assert all(abs(a-b)<=2 for a,b in zip(product.getpixel((x,y)),expected)),(x,y)
                interiors+=1
    check('uniform interiors match independent librsvg',interiors>50000)
    at=lambda x,y:(int(x*30),int(y*30))
    color=lambda x,y:product.getpixel(at(x,y))
    check('mirrored quarter-turn asymmetric pin stem one',color(11,7.5)[3]>200)
    check('mirrored quarter-turn asymmetric pin stem two',color(8,12.25)[3]>200)
    check('rectangular body corner orientation',color(7,9)[3]>150 and color(10,8)[3]>150)
    check('junction is explicit filled dot',color(16,7)[3]==255)
    check('wire center and round cap',color(18,7)[3]==255 and color(20.04,7)[3]>100 and color(20.3,7)[3]==0)
    check('unpainted sheet is transparent',color(2,2)[3]==0)
    def ink(image, box):
        x0,y0,x1,y1=map(lambda v:int(v*30),box)
        return sum(image.getpixel((x,y))[3]>100 for y in range(y0,y1) for x in range(x0,x1))
    # The symbol presentation is deliberately separate from electrical geometry.
    # Inspect every declared shared envelope rather than old centered-text slots.
    symbol_boxes=[tuple(label['envelope'][key]/1_000_000 for key in
                        ['min_x','min_y','max_x','max_y'])
                  for label in metadata['view']['symbol_labels']]
    check('shared label envelopes are explicit and inside the fixture crop',
          len(symbol_boxes)==4 and all(0<=x0<x1<=30 and 0<=y0<y1<=20
                                      for x0,y0,x1,y1 in symbol_boxes))
    text_boxes=symbol_boxes+[(20.2,4.9,29.5,5.8)]
    check('reference value pin and scoped label visible',all(ink(product,b)>30 for b in text_boxes))
    # Remove outlined text by preserved source IDs. Geometry stays identical.
    no_text=ET.parse(svg)
    for parent in no_text.getroot().iter():
        for child in list(parent):
            if child.attrib.get('id','').startswith(('reference-','value-','pin-text-','label-000')):
                parent.remove(child)
    no_text_path=directory/'missing-text.svg';no_text.write(no_text_path,encoding='unicode')
    no_text_png=directory/'missing-text.png';subprocess.run(['rsvg-convert','--output',str(no_text_png),str(no_text_path)],check=True)
    absent=Image.open(no_text_png).convert('RGBA')
    check('missing-text negative control rejected',all(ink(absent,b)<ink(product,b) for b in text_boxes))
    wrong=json.loads(native.read_text());symbol=next(iter(wrong['project']['schematic']['sheets'][SHEET]['content']['symbols'].values()));symbol['rotation_quarters']=3
    wrong_native=directory/'wrong-transform.viamind.json';wrong_native.write_text(json.dumps(wrong))
    wrong_image=Image.open(export('png','wrong-transform',*common,source=wrong_native)).convert('RGBA')
    check('wrong-quarter-turn negative control rejected',wrong_image.getpixel(at(11,7.5))[3]==0)
    for layer, sample in [('wires',(18,7)),('symbols',(7,9)),('pins',(11,7.5)),('junctions',(16,7)),('labels',(21,5.5))]:
        layer_image=Image.open(export('png',layer,*common,'--schematic-layers',layer)).convert('RGBA')
        check(layer+' isolation',ink(layer_image,(0,0,30,20))>0 and
              (layer_image.getpixel(at(*sample))[3]>100 if layer!='labels' else ink(layer_image,text_boxes[-1])>30))
    crop=Image.open(export('png','crop','--view-bounds','10.5,7,11.5,8','--view-size','400,400','--schematic-layers','pins')).convert('RGBA')
    check('explicit pin stem crop',crop.getpixel((200,200))[3]>200 and crop.getpixel((20,200))[3]==0)
    margin=Image.open(export('png','margin','--view-bounds','0,0,30,20','--view-size','900,800')).convert('RGBA')
    check('aspect ratio margins transparent',margin.getpixel((330,30))[3]==0 and margin.getpixel((330,780))[3]==0 and margin.getpixel((330,325))[3]>200)
    # Independent alpha envelopes prove auto-fit leaves breathing room around visible text.
    auto=Image.open(export('png','auto-fit','--view-size','900,600')).convert('RGBA')
    box=auto.getchannel('A').getbbox()
    check('automatic fit contains all visible ink with transparent border',box is not None and box[0]>2 and box[1]>2 and box[2]<898 and box[3]<598)
    long_text=json.loads(native.read_text());long_symbol=next(iter(long_text['project']['schematic']['sheets'][SHEET]['content']['symbols'].values()))
    long_symbol['value']='éΩµЯM'*200
    long_native=directory/'long-text.viamind.json';long_native.write_text(json.dumps(long_text))
    long_view=Image.open(export('png','long-text','--view-size','4096,1000','--view-padding-mm','10',source=long_native)).convert('RGBA')
    box=long_view.getchannel('A').getbbox()
    check('long accented engineering text auto-fit envelope',box is not None and box[0]>10 and box[1]>10 and box[2]<4086 and box[3]<990)
    white=Image.new('RGBA',product.size,'white');white.alpha_composite(product);white.convert('RGB').save(directory/'inspection.png')
    return {'passed':True,'checks':checks,'uniform_interior_samples':interiors,'png':str(png),
            'independent_renderer':subprocess.check_output(['rsvg-convert','--version'],text=True).strip()}

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary',type=Path);parser.add_argument('native',type=Path);parser.add_argument('new_directory',type=Path)
    args=parser.parse_args();args.new_directory.mkdir()
    print(json.dumps(verify(args.binary,args.native,args.new_directory),indent=2))
