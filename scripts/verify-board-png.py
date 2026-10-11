#!/usr/bin/env python3
"""Verify product PNG with Pillow and independent librsvg (synthetic fixture only).

Requires rsvg-convert and Pillow. All outputs go to a new directory.
"""
import argparse
import json
import math
from pathlib import Path
import subprocess
from PIL import Image

def verify(binary, native, directory):
    checks = []
    def check(name, condition):
        if not condition:
            raise AssertionError(name)
        checks.append(name)
    def export(suffix, name, *options):
        path = directory / (name + '.' + suffix)
        subprocess.run([str(binary), '--export-' + suffix, '--open', str(native), '--output', str(path), *options], check=True)
        return path
    common = ['--view-bounds', '0,0,60,40', '--view-size', '1200,800']
    svg = export('svg', 'board', *common)
    png = export('png', 'board', *common)
    reference = directory / 'reference.png'
    subprocess.run(['rsvg-convert', '--output', str(reference), str(svg)], check=True)
    with Image.open(png) as decoded:
        check('independent PNG decoder sees static RGBA8 dimensions', decoded.mode == 'RGBA' and decoded.size == (1200, 800) and not getattr(decoded, 'is_animated', False))
        product = decoded.copy()
    independent = Image.open(reference).convert('RGBA')
    # Compare uniform interior neighborhoods; edges may have different anti-aliasing.
    interiors = 0
    for y in range(2, 798, 4):
        for x in range(2, 1198, 4):
            expected = independent.getpixel((x,y))
            if all(independent.getpixel((x+dx,y+dy)) == expected for dx,dy in [(-1,-1),(1,-1),(-1,1),(1,1)]):
                observed = product.getpixel((x,y))
                assert all(abs(a-b) <= 2 for a,b in zip(observed,expected)), (x,y,observed,expected)
                interiors += 1
    check('product interiors match independent librsvg raster', interiors > 50000)
    project = json.loads(native.read_text())['project']
    for via in project['board']['vias'].values():
        x,y = via['position']['x']/1e6, via['position']['y']/1e6
        check('physical drill transparency ' + via['id'], product.getpixel((int(x*20),int(y*20)))[3] == 0)
        offset = (via['diameter']+via['drill'])/4e6
        check('annular copper survives ' + via['id'], product.getpixel((int((x+offset)*20),int(y*20)))[3] > 200)
    check('cutout and exterior transparent', product.getpixel((940,480))[3] == 0 and product.getpixel((1180,780))[3] == 0)
    check('substrate opaque', product.getpixel((200,400)) == (37,48,43,255))
    # Asymmetric pad interior samples distinguish the wrong rotation sign.
    part = next(p for p in project['board']['parts'].values() if p['side'] == 'bottom')
    pad = part['pads'][0]
    angle = math.radians(part['rotation_mdeg']/1000)
    c,s = math.cos(angle), math.sin(angle)
    lx,ly = -pad['position']['x']/1e6,pad['position']['y']/1e6
    cx = part['position']['x']/1e6 + lx*c + ly*s
    cy = part['position']['y']/1e6 - lx*s + ly*c
    points = []
    for u,v in [(-.75,-.75),(-.75,.75),(.75,-.75),(.75,.75)]:
        dx,dy = u*pad['width']/2e6,v*pad['height']/2e6
        points.append((int((cx+dx*c+dy*s)*20),int((cy-dx*s+dy*c)*20)))
    check('oriented mirrored bottom pad interiors', all(product.getpixel(p) == (71,115,220,255) for p in points))
    negative = directory / 'wrong-rotation.svg'
    negative.write_text(svg.read_text().replace('rotate(-30.000)', 'rotate(30.000)'))
    negative_png = directory / 'wrong-rotation.png'
    subprocess.run(['rsvg-convert', '--output', str(negative_png), str(negative)], check=True)
    wrong = Image.open(negative_png).convert('RGBA')
    check('negative rotation control rejected by oriented samples', any(wrong.getpixel(p) != product.getpixel(p) for p in points))
    opaque = directory / 'opaque-holes.svg'
    opaque.write_text(svg.read_text().replace('fill="black"', 'fill="white"'))
    opaque_png = directory / 'opaque-holes.png'
    subprocess.run(['rsvg-convert', '--output', str(opaque_png), str(opaque)], check=True)
    opaque_image = Image.open(opaque_png).convert('RGBA')
    via = next(iter(project['board']['vias'].values()))
    center = (int(via['position']['x']/50000),int(via['position']['y']/50000))
    check('opaque-hole negative control rejected', product.getpixel(center)[3] == 0 and opaque_image.getpixel(center)[3] == 255)
    for layer in ['top_copper', 'bottom_copper']:
        image = Image.open(export('png', layer, *common, '--view-layers', layer)).convert('RGBA')
        top,bottom = image.getpixel((200,191))[3],image.getpixel((700,260))[3]
        check(layer + ' isolation', (top > 200 and bottom == 0) if layer == 'top_copper' else (top == 0 and bottom > 200))
        if layer == 'top_copper':
            check('round track cap radius', image.getpixel((98,160))[3] > 200 and image.getpixel((96,160))[3] == 0)
    crop = Image.open(export('png', 'crop', '--view-layers', 'top_copper', '--view-bounds', '17,11,19,13', '--view-size', '400,400')).convert('RGBA')
    check('cropped physical hole and annulus coordinates', crop.getpixel((200,200))[3] == 0 and crop.getpixel((250,200))[3] > 200 and crop.getpixel((0,0))[3] == 0)
    margin = Image.open(export('png', 'margins', '--view-bounds', '0,0,60,40', '--view-size', '1200,1000')).convert('RGBA')
    check('aspect margins transparent and substrate centered', margin.getpixel((200,50))[3] == 0 and margin.getpixel((200,950))[3] == 0 and margin.getpixel((200,500)) == (37,48,43,255))
    return {'passed': True, 'checks': checks, 'uniform_interior_samples': interiors, 'png': str(png), 'independent_renderer': subprocess.check_output(['rsvg-convert','--version'],text=True).strip()}

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('native', type=Path)
    parser.add_argument('new_directory', type=Path)
    args = parser.parse_args()
    args.new_directory.mkdir()
    print(json.dumps(verify(args.binary, args.native, args.new_directory), indent=2))
