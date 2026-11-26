#!/usr/bin/env python3
"""slot.'s PICO-8 slot bar, drawn from code like the cart (make.py): the
back (the plastic and the hole's inside) and the front (the plastic with
the hole cut out), in the three bar themes.

    python3 tools/pico8-mould/bar.py [variant] [out_dir]

The opening follows the rule of every slot bar: the seated cart's width
(240, cart.rs SEATED_W) plus 16, centred (232..488 of 720), like the Lynx,
Game Gear and Game Boy bars. Its corners are the cart's: rounded on the
left, cut on the right (Siham's choice, 2026-11-27; "cuts" cuts both).
Everything that is not plain plastic stays inside the bar's middle
(refit_band keeps 200..520 as drawn and repeats the sides).
"""
import random
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parents[2]

W, H, K = 720, 58, 3
SS = 4                                  # supersampling for the outlines
HOLE = (232, 488)                       # seated 240 + 16
DEPTH = 44                              # how far down the hole goes
ROUND = 11                              # the cart's corner radius, seated (14 x 240/312)
CUT = 17                                # the cart's cut corner, seated (22 x 240/312)

THEMES = {
    # the default cart's own plastic (home_art.rs, p8: [52, 52, 56])
    "": dict(plastic=(56, 56, 61), floor=(26, 26, 29), ink_hi=92, ink_lo=30),
    "_dark": dict(plastic=(46, 48, 53), floor=(24, 25, 28), ink_hi=78, ink_lo=26),
    "_light": dict(plastic=(229, 228, 222), floor=(72, 74, 80), ink_hi=250, ink_lo=170),
}


def hole_path(variant, s):
    """The opening's outline at scale s, as a polygon."""
    x0, x1, d = HOLE[0] * s, HOLE[1] * s, DEPTH * s
    pts = [(x0, -s), (x1, -s)]
    # right corner: the cart's cut
    c = CUT * s
    pts += [(x1, d - c), (x1 - c, d)]
    if variant == "cuts":
        pts += [(x0 + c, d), (x0, d - c)]
    else:
        r = ROUND * s
        import math
        for i in range(17):
            a = math.pi / 2 + i * (math.pi / 2) / 16
            pts.append((x0 + r + r * math.cos(a), d - r + r * math.sin(a)))
    return pts


def mask(variant, grow=0):
    s = K * SS
    m = Image.new("L", (W * s, H * s), 0)
    ImageDraw.Draw(m).polygon(hole_path(variant, s), fill=255)
    m = m.resize((W * K, H * K), Image.LANCZOS)
    if grow > 0:
        m = m.filter(ImageFilter.MaxFilter(2 * grow + 1))
    elif grow < 0:
        m = m.filter(ImageFilter.MinFilter(-2 * grow + 1))
    return m


def grain(size, amount, seed):
    rnd = random.Random(seed)
    g = Image.new("L", size)
    g.putdata([128 + rnd.randint(-amount, amount) for _ in range(size[0] * size[1])])
    return g.filter(ImageFilter.GaussianBlur(0.7))


def shade(img, amount):
    """Brightens (amount > 0) or darkens an RGB image by a 0..255 L mask."""
    def f(m):
        if amount > 0:
            return ImageChops.add(img, Image.merge("RGB", [m] * 3))
        return ImageChops.subtract(img, Image.merge("RGB", [m] * 3))
    return f


def plastic(t):
    w, h = W * K, H * K
    base = Image.new("RGB", (w, h), t["plastic"])
    # a soft fall top to bottom, and the cart's fine grain
    fall = Image.linear_gradient("L").resize((w, h)).point(lambda v: v * 18 // 255)
    base = ImageChops.subtract(base, Image.merge("RGB", [fall] * 3))
    g = grain((w, h), 6, 8)
    base = ImageChops.add(base, Image.merge("RGB", [g] * 3), 1.0, -128)
    d = ImageDraw.Draw(base)
    p = t["plastic"]
    light = tuple(min(255, c + (40 if sum(p) < 400 else 18)) for c in p)
    dark = tuple(max(0, c - (26 if sum(p) < 400 else 30)) for c in p)
    # top lip: a dark seam, then the lit edge; the bottom's shade
    d.rectangle([0, 0, w, K - 1], fill=dark)
    d.rectangle([0, K, w, 2 * K - 1], fill=light)
    for i in range(2 * K):
        f = 1 - i / (2 * K)
        col = tuple(int(c - (c - dc) * f) for c, dc in zip(p, dark))
        d.line([(0, h - 1 - i), (w, h - 1 - i)], fill=col)
    return base


def ridges(img, t):
    """The cart's grip ridges, short, on each side of the opening."""
    d = ImageDraw.Draw(img)
    for x0, x1 in ((207, 225), (495, 513)):
        for i in range(5):
            y = (12 + i * 6) * K
            d.line([(x0 * K, y), (x1 * K, y)], fill=(t["ink_lo"],) * 3, width=2 * K)
            d.line([(x0 * K, y + 2 * K), (x1 * K, y + 2 * K)], fill=(t["ink_hi"],) * 3, width=K)


def inside(t, variant):
    """The hole's inside: its floor, the wall's shade, two guide posts."""
    w, h = W * K, H * K
    fl = t["floor"]
    img = Image.new("RGB", (w, h), fl)
    d = ImageDraw.Draw(img)
    # the back wall falls into shadow towards the top
    for y in range(0, 14 * K):
        f = 1 - y / (14 * K)
        d.line([(0, y), (w, y)], fill=tuple(int(c * (1 - 0.55 * f)) for c in fl))
    # a ledge the cart rests on
    ly = 34 * K
    d.rectangle([0, ly, w, ly + K], fill=tuple(min(255, c + 22) for c in fl))
    d.rectangle([0, ly + K + 1, w, ly + 3 * K], fill=tuple(int(c * 0.7) for c in fl))
    # guide posts, as on the other handheld bars
    for x in (HOLE[0] + 14, HOLE[1] - 14 - 4):
        d.rectangle([x * K, 6 * K, (x + 4) * K, 33 * K], fill=tuple(min(255, c + 30) for c in fl))
        d.rectangle([(x + 3) * K, 6 * K, (x + 4) * K, 33 * K], fill=tuple(int(c * 0.6) for c in fl))
    img = ImageChops.add(img, Image.merge("RGB", [grain((w, h), 3, 9)] * 3), 1.0, -128)
    return img


def rim(img, variant):
    """The opening's edge: shade on the plastic just inside it, light on its lip."""
    m = mask(variant)
    outer = ImageChops.subtract(mask(variant, K), m)        # a band just outside the hole
    lit = Image.new("L", outer.size, 0)
    # the lower half of the band catches light, the upper sides are in shade
    grad = Image.linear_gradient("L").resize(outer.size)
    lit = ImageChops.multiply(outer, grad.point(lambda v: 0 if v < 110 else min(255, (v - 110) * 3)))
    dark = ImageChops.multiply(outer, grad.point(lambda v: 255 - v))
    img = ImageChops.add(img, Image.merge("RGB", [lit.point(lambda v: v * 60 // 255)] * 3))
    img = ImageChops.subtract(img, Image.merge("RGB", [dark.point(lambda v: v * 40 // 255)] * 3))
    return img


def build(variant, suffix):
    t = THEMES[suffix]
    face = plastic(t)
    ridges(face, t)
    face = rim(face, variant)
    m = mask(variant)
    # inner wall shade inside the hole, so the floor reads deeper
    inner = inside(t, variant)
    wall = ImageChops.subtract(m, mask(variant, -2 * K)).filter(ImageFilter.GaussianBlur(K))
    inner = ImageChops.subtract(inner, Image.merge("RGB", [wall.point(lambda v: v * 90 // 255)] * 3))
    back = Image.composite(inner, face, m).convert("RGBA")
    front = face.convert("RGBA")
    front.putalpha(ImageChops.invert(m))
    return back, front


def main():
    variant = sys.argv[1] if len(sys.argv) > 1 else "cut"
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else ROOT / "assets" / "home"
    out.mkdir(parents=True, exist_ok=True)
    for suffix in THEMES:
        back, front = build(variant, suffix)
        back.save(out / f"slot_back_p8{suffix}.png", optimize=True)
        front.save(out / f"slot_front_p8{suffix}.png", optimize=True)
    print("wrote", out)


if __name__ == "__main__":
    main()
