#!/usr/bin/env python3
"""Independently parse the synthetic fixture's output using Gerbonara 1.6.3.

Install gerbonara==1.6.3 in a disposable venv; run after:
viamind --export-gerber --open fixtures/fabrication/asymmetric.viamind.json \
  --output NEW_DIRECTORY --mask-expansion-mm 0.05
No Gerbonara implementation is copied into Viamind.
"""
import argparse
import json
import math
from pathlib import Path
import warnings
from importlib.metadata import version
from gerbonara import GerberFile, ExcellonFile
from gerbonara.graphic_objects import Flash, Line
from gerbonara.utils import MM


def near(actual, expected, tolerance=0.000002):
    assert abs(actual-expected) <= tolerance, (actual, expected)


def verify(directory, fixture):
    project = json.loads(fixture.read_text())["project"]
    board = project["board"]
    manifest = json.loads((directory / "manifest.json").read_text())
    expansion = manifest["mask_expansion_nm"] / 1e6
    checks = []
    with warnings.catch_warnings(record=True) as seen:
        warnings.simplefilter("always")
        files = {name: GerberFile.open(directory / name) for name in manifest["files"] if name.endswith(".gbr")}
        drill = ExcellonFile.open(directory / "plated-through.xnc")
    assert not seen, [str(w.message) for w in seen]
    checks.append("all five Gerber and XNC files parse without warnings")
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
                flashes = [o for o in files[f"{side}-{kind}.gbr"].objects if isinstance(o, Flash)]
                match = min(flashes, key=lambda o: math.hypot(o.x-center[0], o.y-center[1]))
                near(match.x, center[0]); near(match.y, center[1])
                half_w, half_h = pad["width"]/2e6+margin, pad["height"]/2e6+margin
                corners = [transform(x+dx*half_w, y+dy*half_h) for dx, dy in ((-1,-1),(-1,1),(1,-1),(1,1))]
                expected = ((min(p[0] for p in corners), min(p[1] for p in corners)),
                            (max(p[0] for p in corners), max(p[1] for p in corners)))
                actual = match.bounding_box(MM)
                for a, e in zip(actual[0]+actual[1], expected[0]+expected[1]):
                    near(a,e)
    checks.append("asymmetric top and mirrored bottom pads match independent rotated corners and mask expansion")
    for track in board["tracks"].values():
        lines = [o for o in files[f'{track["layer"]}-copper.gbr'].objects if isinstance(o, Line)]
        start, end = track["start"], track["end"]
        match = min(lines, key=lambda o: math.hypot(o.x1-start["x"]/1e6, o.y1+start["y"]/1e6))
        for a, e in zip((match.x1,match.y1,match.x2,match.y2), (start["x"]/1e6,-start["y"]/1e6,end["x"]/1e6,-end["y"]/1e6)):
            near(a,e)
        near(match.aperture.diameter, track["width"]/1e6)
    checks.append("top and bottom track endpoints and round-aperture widths match")
    assert len(drill.objects) == len(board["vias"])
    for via in board["vias"].values():
        x, y = via["position"]["x"]/1e6, -via["position"]["y"]/1e6
        hole = min(drill.objects, key=lambda o: math.hypot(o.x-x,o.y-y))
        near(hole.x,x); near(hole.y,y); near(hole.aperture.diameter,via["drill"]/1e6)
        for side in ("top","bottom"):
            for kind, margin in (("copper",0),("mask",expansion)):
                flashes = [o for o in files[f"{side}-{kind}.gbr"].objects if isinstance(o,Flash)]
                match = min(flashes, key=lambda o: math.hypot(o.x-x,o.y-y))
                near(match.x,x); near(match.y,y)
                near(match.bounding_box(MM)[1][0]-match.bounding_box(MM)[0][0],via["diameter"]/1e6+2*margin)
    checks.append("both via copper/mask discs align with exact finished-hole XNC positions and diameters")
    segments = []
    for ring in [board["outline"]["outer"]]+board["outline"]["cutouts"]:
        for p, q in zip(ring,ring[1:]+ring[:1]):
            segments.append((p["x"]/1e6,-p["y"]/1e6,q["x"]/1e6,-q["y"]/1e6))
    assert len(files["profile.gbr"].objects)==len(segments)
    for line, expected in zip(files["profile.gbr"].objects,segments):
        for a,e in zip((line.x1,line.y1,line.x2,line.y2),expected): near(a,e)
    checks.append("outer and cutout profile rings close exactly in the same coordinate system")
    for name, parsed in files.items():
        (directory / f"reader-{name}.svg").write_text(str(parsed.to_svg()))
    return {"reader":"gerbonara", "version":version("gerbonara"),"passed":True,"checks":checks,"generated_views":[f"reader-{name}.svg" for name in files]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory",type=Path)
    parser.add_argument("--fixture",type=Path,default=Path(__file__).resolve().parents[1]/"fixtures/fabrication/asymmetric.viamind.json")
    args=parser.parse_args()
    result=verify(args.directory,args.fixture)
    print(json.dumps(result,indent=2))
