#!/usr/bin/env python3
"""Independently inspect a synthetic board SVG and rasterize with librsvg.

Requires rsvg-convert (tested librsvg 2.62.3) and Pillow (verification only).
Run against fixtures/views/asymmetric.viamind.json; no product PNG export implied.
"""
import argparse
import json
import math
from pathlib import Path
import re
import subprocess
import xml.etree.ElementTree as ET
from PIL import Image

NS = {'s': 'http://www.w3.org/2000/svg'}


def inspect(native, svg):
    project = json.loads(native.read_text())['project']
    root = ET.parse(svg).getroot()
    checks = []

    def check(name, condition):
        if not condition:
            raise AssertionError(name)
        checks.append(name)

    check('declared model region and pixel viewport',
          list(map(float, root.attrib['viewBox'].split())) == [0, 0, 60, 40]
          and (root.attrib['width'], root.attrib['height']) == ('1200', '800'))
    ids = {e.attrib['id']: e for e in root.iter() if 'id' in e.attrib}
    check('stable layer order', [e.attrib['id'] for e in root.iter()
          if e.attrib.get('id', '').startswith('layer-')] ==
          ['layer-Outline', 'layer-BottomCopper', 'layer-TopCopper'])
    for part in project['board']['parts'].values():
        a = math.radians(part['rotation_mdeg'] / 1000)
        c, s = math.cos(a), math.sin(a)
        for pad in part['pads']:
            element = ids['pad-' + pad['id']]
            rect = element.find('s:rect', NS)
            observed = list(map(float, re.findall(r'-?\d+(?:\.\d+)?', element.attrib['transform'])))
            x, y = pad['position']['x'] / 1e6, pad['position']['y'] / 1e6
            if part['side'] == 'bottom':
                x = -x
            cx = part['position']['x'] / 1e6 + x*c + y*s
            cy = part['position']['y'] / 1e6 - x*s + y*c
            oa = math.radians(observed[2])
            oc, os = math.cos(oa), math.sin(oa)
            corners = []
            expected = []
            for u, v in [(-1, -1), (-1, 1), (1, -1), (1, 1)]:
                dx, dy = u*pad['width']/2e6, v*pad['height']/2e6
                corners.append((observed[0]+dx*oc-dy*os, observed[1]+dx*os+dy*oc))
                expected.append((cx+dx*c+dy*s, cy-dx*s+dy*c))
            check('oriented pad corners ' + pad['id'],
                  all(math.dist(p, q) <= 0.000001 for p, q in zip(corners, expected)))
            check('exact pad dimensions ' + pad['id'],
                  float(rect.attrib['width']) == pad['width']/1e6
                  and float(rect.attrib['height']) == pad['height']/1e6)
    for track in project['board']['tracks'].values():
        element = ids['track-' + track['id']]
        coordinates = list(map(float, re.findall(r'-?\d+(?:\.\d+)?', element.attrib['d'])))
        check('track endpoints width and round caps ' + track['id'],
              coordinates == [track['start']['x']/1e6, track['start']['y']/1e6,
                              track['end']['x']/1e6, track['end']['y']/1e6]
              and float(element.attrib['stroke-width']) == track['width']/1e6
              and element.attrib['stroke-linecap'] == 'round')
    check('closed even-odd outline and cutout',
          ids['board-outline'].attrib['d'].count('Z') == 2
          and ids['board-outline'].attrib['fill-rule'] == 'evenodd'
          and ids['board-outline'].attrib['stroke-linejoin'] == 'round')
    check('self-contained safe XML', all(
          e.tag.removeprefix('{'+NS['s']+'}') in
          {'svg', 'title', 'desc', 'defs', 'clipPath', 'rect', 'mask', 'circle', 'g', 'path'}
          and all(not k.startswith('on') and 'href' not in k for k in e.attrib)
          for e in root.iter()))
    png = svg.with_suffix('.reader.png')
    subprocess.run(['rsvg-convert', '--output', str(png), str(svg)], check=True)
    image = Image.open(png).convert('RGBA')
    check('independent raster viewport', image.size == (1200, 800))

    def alpha(x, y):
        return image.getpixel((int(x*20), int(y*20)))[3]

    for via in project['board']['vias'].values():
        x, y = via['position']['x']/1e6, via['position']['y']/1e6
        check('drill transparency over underlying copper and substrate ' + via['id'], alpha(x, y) == 0)
        offset = (via['diameter']+via['drill'])/4e6
        check('via annulus survives ' + via['id'], alpha(x+offset, y) > 200)
    check('cutout interior transparent', alpha(47, 24) == 0)
    check('board substrate visible', alpha(10, 20) == 255)
    check('outside asymmetric board transparent', alpha(59, 39) == 0)
    return {'passed': True, 'checks': checks, 'png': str(png),
            'renderer': subprocess.check_output(['rsvg-convert', '--version'], text=True).strip()}


def inspect_variants(binary, native, directory):
    checks = []
    for layer, count in [('top_copper', 1), ('bottom_copper', 2)]:
        path = directory / (layer + '.svg')
        subprocess.run([str(binary), '--export-svg', '--open', str(native),
                        '--output', str(path), '--view-layers', layer,
                        '--view-bounds', '0,0,60,40', '--view-size', '1200,800'], check=True)
        root = ET.parse(path).getroot()
        assert sum(e.attrib.get('id', '').startswith('pad-') for e in root.iter()) == count
        assert sum(e.attrib.get('id', '').startswith('track-') for e in root.iter()) == 1
        png = path.with_suffix('.png')
        subprocess.run(['rsvg-convert', '--output', str(png), str(path)], check=True)
        image = Image.open(png).convert('RGBA')
        # Interior points on the distinct straight top/bottom tracks.
        top = image.getpixel((200, 191))[3]
        bottom = image.getpixel((700, 260))[3]
        assert (top > 200 and bottom == 0) if layer == 'top_copper' else (top == 0 and bottom > 200)
        if layer == 'top_copper':
            assert image.getpixel((98, 160))[3] > 200  # round cap before start
            assert image.getpixel((96, 160))[3] == 0   # beyond radius
        checks.append(layer + ' independently rendered isolation and track geometry')
    path = directory / 'crop.svg'
    subprocess.run([str(binary), '--export-svg', '--open', str(native),
                    '--output', str(path), '--view-layers', 'top_copper',
                    '--view-bounds', '17,11,19,13', '--view-size', '400,400'], check=True)
    root = ET.parse(path).getroot()
    assert list(map(float, root.attrib['viewBox'].split())) == [17, 11, 2, 2]
    png = path.with_suffix('.png')
    subprocess.run(['rsvg-convert', '--output', str(png), str(path)], check=True)
    image = Image.open(png).convert('RGBA')
    assert image.size == (400, 400)
    assert image.getpixel((200, 200))[3] == 0  # physical hole retains cropped position
    assert image.getpixel((250, 200))[3] > 200  # annulus
    assert image.getpixel((0, 0))[3] == 0
    checks.append('explicit cropped region maps actual via geometry without auto padding')
    return checks


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('native', type=Path)
    parser.add_argument('svg', type=Path)
    parser.add_argument('--binary', type=Path, help='also export and render filtered/cropped variants in a fresh temporary directory')
    args = parser.parse_args()
    report = inspect(args.native, args.svg)
    if args.binary:
        import tempfile
        with tempfile.TemporaryDirectory(prefix='viamind-svg-reader-') as directory:
            report['checks'].extend(inspect_variants(args.binary, args.native, Path(directory)))
    print(json.dumps(report, indent=2))
