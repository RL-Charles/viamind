#!/usr/bin/env python3
"""Independently parse the synthetic fixture's output using PyGerber 2.4.3 and Gerbonara 1.6.3.

Install pygerber[svg]==2.4.3 and gerbonara==1.6.3 in a disposable venv; run after:
viamind --export-gerber --open fixtures/fabrication/asymmetric.viamind.json \
  --output NEW_DIRECTORY --mask-expansion-mm 0.05
No reader implementation is copied into Viamind.
"""
import argparse
import json
import math
from pathlib import Path
import warnings
from importlib.metadata import version
from gerbonara import ExcellonFile
from pygerber.gerberx3.api.v2 import GerberFile
from pygerber.gerberx3.parser2.commands2.flash2 import Flash2 as Flash
from pygerber.gerberx3.parser2.commands2.line2 import Line2 as Line


def near(actual, expected, tolerance=0.000002):
    assert abs(actual-expected) <= tolerance, (actual, expected)


def mm(offset):
    return float(offset.as_millimeters())


def xy(vector):
    return mm(vector.x), mm(vector.y)


def objects(parsed):
    # Pinned reader-version adapter for access to its parsed command geometry.
    return parsed._command_buffer.commands


def bbox(command):
    box = command.get_bounding_box()
    return ((mm(box.min_x), mm(box.min_y)), (mm(box.max_x), mm(box.max_y)))


def verify(directory, fixture):
    project = json.loads(fixture.read_text())["project"]
    board = project["board"]
    manifest = json.loads((directory / "manifest.json").read_text())
    expansion = manifest["mask_expansion_nm"] / 1e6
    checks = []
    with warnings.catch_warnings(record=True) as seen:
        warnings.simplefilter("always")
        # PyGerber 2.4.3 calls this deprecated pyparsing spelling; unrelated to file validity.
        warnings.filterwarnings("ignore", message=".*oneOf.*deprecated.*one_of.*", category=DeprecationWarning)
        files = {name: GerberFile.from_file(directory / name).parse() for name in manifest["files"] if name.endswith(".gbr")}
        drill = ExcellonFile.open(directory / "plated-through.xnc")
    assert not seen, [str(w.message) for w in seen]
    checks.append("PyGerber parses all five transformed Gerber files and Gerbonara parses XNC without warnings")
    for part in board["parts"].values():
        side = part["side"]
        angle = math.radians(part["rotation_mdeg"] / 1000)
        cosine, sine = math.cos(angle), math.sin(angle)
        def transform(x, y):
            # Independent corner transform in top-view physical coordinates.
            if side == "bottom":
                x = -x
            y = -y
            return (part["position"]["x"] / 1e6 + x*cosine-y*sine,
                    -part["position"]["y"] / 1e6 + x*sine+y*cosine)
        for pad in part["pads"]:
            x, y = pad["position"]["x"]/1e6, pad["position"]["y"]/1e6
            center = transform(x, y)
            for kind, margin in (("copper", 0), ("mask", expansion)):
                flashes = [o for o in objects(files[f"{side}-{kind}.gbr"]) if isinstance(o, Flash)]
                match = min(flashes, key=lambda o: math.dist(xy(o.flash_point), center))
                near(xy(match.flash_point)[0], center[0]); near(xy(match.flash_point)[1], center[1])
                half_w, half_h = pad["width"]/2e6+margin, pad["height"]/2e6+margin
                corners = [transform(x+dx*half_w, y+dy*half_h) for dx, dy in ((-1,-1),(-1,1),(1,-1),(1,1))]
                expected = ((min(p[0] for p in corners), min(p[1] for p in corners)),
                            (max(p[0] for p in corners), max(p[1] for p in corners)))
                actual = bbox(match)
                for a, e in zip(actual[0]+actual[1], expected[0]+expected[1]):
                    near(a,e)
                # Compare oriented corners, not only a bounding box: ±angle share a box.
                actual_angle = math.radians(float(match.aperture.rotation))
                ac, ass = math.cos(actual_angle), math.sin(actual_angle)
                cx, cy = xy(match.flash_point)
                aw, ah = mm(match.aperture.x_size)/2, mm(match.aperture.y_size)/2
                actual_corners = [(cx+dx*aw*ac-dy*ah*ass, cy+dx*aw*ass+dy*ah*ac)
                                  for dx,dy in ((-1,-1),(-1,1),(1,-1),(1,1))]
                for actual_corner in actual_corners:
                    assert min(math.dist(actual_corner, corner) for corner in corners) <= 0.000002, (actual_corner,corners)
    checks.append("asymmetric top and mirrored bottom pads match independent oriented corners and mask expansion")
    for track in board["tracks"].values():
        lines = [o for o in objects(files[f'{track["layer"]}-copper.gbr']) if isinstance(o, Line)]
        start, end = track["start"], track["end"]
        match = min(lines, key=lambda o: math.dist(xy(o.start_point), (start["x"]/1e6, -start["y"]/1e6)))
        for a, e in zip(xy(match.start_point)+xy(match.end_point), (start["x"]/1e6,-start["y"]/1e6,end["x"]/1e6,-end["y"]/1e6)):
            near(a,e)
        near(mm(match.aperture.diameter), track["width"]/1e6)
    checks.append("top and bottom track endpoints and round-aperture widths match")
    assert len(drill.objects) == len(board["vias"])
    for via in board["vias"].values():
        x, y = via["position"]["x"]/1e6, -via["position"]["y"]/1e6
        hole = min(drill.objects, key=lambda o: math.hypot(o.x-x,o.y-y))
        near(hole.x,x); near(hole.y,y); near(hole.aperture.diameter,via["drill"]/1e6)
        for side in ("top","bottom"):
            for kind, margin in (("copper",0),("mask",expansion)):
                flashes = [o for o in objects(files[f"{side}-{kind}.gbr"]) if isinstance(o,Flash)]
                match = min(flashes, key=lambda o: math.dist(xy(o.flash_point), (x,y)))
                near(xy(match.flash_point)[0],x); near(xy(match.flash_point)[1],y)
                near(bbox(match)[1][0]-bbox(match)[0][0],via["diameter"]/1e6+2*margin)
    checks.append("both via copper/mask discs align with exact finished-hole XNC positions and diameters")
    segments = []
    for ring in [board["outline"]["outer"]]+board["outline"]["cutouts"]:
        for p, q in zip(ring,ring[1:]+ring[:1]):
            segments.append((p["x"]/1e6,-p["y"]/1e6,q["x"]/1e6,-q["y"]/1e6))
    assert len(objects(files["profile.gbr"]))==len(segments)
    for line, expected in zip(objects(files["profile.gbr"]),segments):
        for a,e in zip(xy(line.start_point)+xy(line.end_point),expected): near(a,e)
    checks.append("outer and cutout profile rings close exactly in the same coordinate system")
    for name, parsed in files.items():
        parsed.render_svg(directory / f"reader-{name}.svg")
        if name != "profile.gbr":
            parsed.render_raster(directory / f"reader-{name}.png", dpmm=40)
    return {"readers":{"pygerber":version("pygerber"), "gerbonara":version("gerbonara")},"passed":True,"checks":checks,"generated_views":[f"reader-{name}.svg" for name in files]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory",type=Path)
    parser.add_argument("--fixture",type=Path,default=Path(__file__).resolve().parents[1]/"fixtures/fabrication/asymmetric.viamind.json")
    args=parser.parse_args()
    result=verify(args.directory,args.fixture)
    print(json.dumps(result,indent=2))
