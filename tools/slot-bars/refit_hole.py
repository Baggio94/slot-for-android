#!/usr/bin/env python3
"""Brings a slot bar's opening to the rule of every slot bar: the seated
cart's width (240, cart.rs SEATED_W) plus 16, centred (232..488 of 720),
as tools/pico8-mould/bar.py draws it.

    python3 tools/slot-bars/refit_hole.py <bar> <hole width now> <cut1> <cut2>

<bar> names assets/home/slot_{back,front}_<bar>{,_dark,_light}.png. The
opening narrows (or widens) by the same number of columns taken out of
(or repeated at) two plain strips inside it, at x = cut1 and cut2 (720
units), so its corners, posts and screws keep their shape; the sides
follow and the bar's outer columns are refilled from its own plastic
(native_assets::refit_band repeats them on wider screens anyway).
Neo Geo: 298 -> 256, PC Engine: 244 -> 256 (Siham 2026-11-28).
"""
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
RULE = 256
K = 3


def refit(im, now, cut1, cut2):
    w, h = im.size
    d = round((now - RULE) / 2 * K)        # columns per side, + narrows
    a, b = round(cut1 * K), round(cut2 * K)
    if d > 0:   # take d columns out at each cut, pad d at each outer edge
        left, mid, right = im.crop((0, 0, a, h)), im.crop((a + d, 0, b, h)), im.crop((b + d, 0, w, h))
        out = Image.new(im.mode, (w, h))
        out.paste(im.crop((0, 0, d, h)), (0, 0))
        out.paste(left, (d, 0))
        out.paste(mid, (d + a, 0))
        out.paste(right, (d + b - d, 0))
        out.paste(im.crop((w - d, 0, w, h)), (w - d, 0))
        return out
    d = -d      # repeat d columns at each cut, drop d at each outer edge
    left, mid, right = im.crop((d, 0, a, h)), im.crop((a - d, 0, b + d, h)), im.crop((b, 0, w - d, h))
    out = Image.new(im.mode, (w, h))
    out.paste(left, (0, 0))
    out.paste(mid, (a - d, 0))
    out.paste(right, (b + d, 0))
    return out


def main():
    bar, now, cut1, cut2 = sys.argv[1], float(sys.argv[2]), float(sys.argv[3]), float(sys.argv[4])
    for part in ("back", "front"):
        for theme in ("", "_dark", "_light"):
            path = ROOT / "assets/home" / f"slot_{part}_{bar}{theme}.png"
            im = Image.open(path)
            im.load()
            refit(im.convert("RGBA"), now, cut1, cut2).save(path, optimize=True)
            print("refitted", path.name)


if __name__ == "__main__":
    main()
