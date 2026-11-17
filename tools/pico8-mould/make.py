#!/usr/bin/env python3
"""slot.'s own PICO-8 cart mould, drawn from code (there is no real PICO-8
cartridge to capture): the neutral front shell the game's colour is blended
onto, the back shell, the board with and without its battery, and the chip.

    python3 tools/pico8-mould/make.py

writes assets/home/p8_*. The geometry is the mould's row in
crates/slot-ui/src/home_art.rs; change both together.
"""
import random
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "assets" / "home"
FONT = ROOT / "crates" / "slot-ui" / "assets" / "label.ttf"

W, H = 312, 400          # logical size (HOME_CART_W, the mould's h)
K = 3                    # FRONT_K and BOARD_K
CUT = 28                 # the top-right corner cut
R = 14                   # the other corners' radius
LABEL = (36, 66, 240, 296)          # the label well: art 240x240, title 56
WINDOW = (26, 56, 260, 316)         # the recess around it
FOOTPRINTS = [(40.0, 196.0, 104.0, 52.0), (168.0, 196.0, 104.0, 52.0)]
TITLE = (100.0, 120.0, 112.0, 31.0)
NEUTRAL = 128


def outline(k):
    """The cart's silhouette at scale k, antialiased."""
    big = 4
    s = k * big
    m = Image.new("L", (W * s, H * s), 0)
    d = ImageDraw.Draw(m)
    r, cut = R * s, CUT * s
    d.rounded_rectangle([0, 0, W * s - 1, H * s - 1], radius=r, fill=255)
    # the cut corner: a straight chamfer instead of the round one
    d.polygon([(W * s - cut, 0), (W * s, 0), (W * s, cut)], fill=0)
    return m.resize((W * k, H * k), Image.LANCZOS)


def grain(size, amount, seed):
    rnd = random.Random(seed)
    g = Image.new("L", size)
    g.putdata([128 + rnd.randint(-amount, amount) for _ in range(size[0] * size[1])])
    return g.filter(ImageFilter.GaussianBlur(0.7))


def neutral():
    k = K
    w, h = W * k, H * k
    mask = outline(k)
    # Plastic: a soft top-to-bottom fall around the neutral grey.
    body = Image.new("L", (w, h))
    bd = ImageDraw.Draw(body)
    for y in range(h):
        t = y / h
        bd.line([(0, y), (w, y)], fill=int(NEUTRAL + 14 - 26 * t))
    body = ImageChops.add(body, grain((w, h), 5, 8), 1.0, -128)
    d = ImageDraw.Draw(body)
    # Rim light on the top and left edges, shade on the bottom and right.
    edge = ImageChops.subtract(mask, mask.filter(ImageFilter.MinFilter(2 * k + 1)))
    lit = Image.linear_gradient("L").rotate(180).resize((w, h))
    body.paste(170, (0, 0), ImageChops.multiply(edge, lit))
    body.paste(80, (0, 0), ImageChops.multiply(edge, lit.rotate(180)))
    # Grip ridges.
    for i in range(5):
        y = (14 + i * 7) * k
        d.line([(26 * k, y), (226 * k, y)], fill=88, width=2 * k)
        d.line([(26 * k, y + 2 * k), (226 * k, y + 2 * k)], fill=150, width=k)
    # The recessed window the label sits in.
    x, y, ww, wh = WINDOW
    d.rounded_rectangle([x * k, y * k, (x + ww) * k, (y + wh) * k], radius=7 * k, fill=72)
    d.rounded_rectangle([x * k, y * k, (x + ww) * k, (y + wh) * k], radius=7 * k,
                        outline=54, width=2 * k)
    d.line([((x + 6) * k, (y + wh) * k), ((x + ww - 6) * k, (y + wh) * k)], fill=150, width=k)
    lx, ly, lw, lh = LABEL
    d.rectangle([lx * k, ly * k, (lx + lw) * k - 1, (ly + lh) * k - 1], fill=NEUTRAL)
    # "PICO-8" moulded under the window.
    f = ImageFont.truetype(str(FONT), 13 * k)
    cx, cy = W * k / 2, 387 * k
    d.text((cx, cy + k), "PICO-8", font=f, fill=150, anchor="mm")
    d.text((cx, cy), "PICO-8", font=f, fill=96, anchor="mm")
    rgba = Image.merge("RGBA", [body, body, body, mask])
    rgba.save(OUT / "p8_neutral.png", optimize=True)


def board(battery):
    k = K
    w, h = W * k, H * k
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    green, dark, trace = (58, 154, 60), (34, 104, 38), (78, 176, 80)
    gold, silk = (226, 174, 40), (238, 240, 232)
    x0, y0, x1, y1 = 16 * k, 50 * k, (W - 16) * k, (H - 8) * k
    d.rounded_rectangle([x0, y0, x1, y1], radius=8 * k, fill=green, outline=dark, width=2 * k)
    rnd = random.Random(3)
    # Traces from the fingers up to the footprints.
    for i in range(24):
        x = (34 + i * 10.3) * k
        top = (252 + rnd.randint(0, 40)) * k
        d.line([(x, top), (x, 352 * k)], fill=trace, width=k + 1)
    for _ in range(40):
        vx, vy = rnd.randint(30, 280) * k, rnd.randint(70, 340) * k
        d.ellipse([vx - 2 * k, vy - 2 * k, vx + 2 * k, vy + 2 * k], fill=dark)
    # Edge connector.
    for i in range(24):
        x = (34 + i * 10.3) * k
        d.rounded_rectangle([x - 3 * k, 354 * k, x + 3 * k, 388 * k], radius=k, fill=gold)
    # Footprints: rows of pads above and below each big chip.
    for fx, fy, fw, fh in FOOTPRINTS:
        n = 14
        for i in range(n):
            px = (fx + 6 + i * (fw - 12) / (n - 1)) * k
            for py in (fy - 6, fy + fh + 1):
                d.rectangle([px - 2 * k, py * k, px + 2 * k, (py + 5) * k], fill=silk)
        d.rectangle([fx * k, fy * k, (fx + fw) * k, (fy + fh) * k], outline=silk, width=k)
    # The title chip's pads.
    tx, ty, tw, th = TITLE
    for i in range(8):
        px = (tx + 8 + i * (tw - 16) / 7) * k
        for py in (ty - 5, ty + th + 1):
            d.rectangle([px - 2 * k, py * k, px + 2 * k, (py + 4) * k], fill=silk)
    # Mounting holes and silkscreen.
    for hx in (34, W - 34):
        d.ellipse([(hx - 9) * k, (78 - 9) * k, (hx + 9) * k, (78 + 9) * k], fill=silk)
        d.ellipse([(hx - 6) * k, (78 - 6) * k, (hx + 6) * k, (78 + 6) * k], fill=(20, 20, 20, 255))
    f = ImageFont.truetype(str(FONT), 9 * k)
    d.text((26 * k, 96 * k), "P/N P8-SLOT", font=f, fill=silk)
    d.text((26 * k, 108 * k), "REV 1", font=f, fill=silk)
    d.text((28 * k, 340 * k), "1", font=f, fill=silk)
    d.text(((W - 36) * k, 340 * k), "24", font=f, fill=silk)
    if battery:
        bx, by, br = 250, 132, 22
        d.ellipse([(bx - br) * k, (by - br) * k, (bx + br) * k, (by + br) * k],
                  fill=(186, 188, 192), outline=(120, 122, 126), width=2 * k)
        d.ellipse([(bx - br + 6) * k, (by - br + 6) * k, (bx + br - 6) * k, (by + br - 6) * k],
                  outline=(214, 216, 220), width=k)
        d.text((bx * k, by * k), "+", font=f, fill=(90, 90, 96), anchor="mm")
    else:
        bx, by = 250, 132
        for dx in (-14, 14):
            d.rectangle([(bx + dx - 4) * k, (by - 6) * k, (bx + dx + 4) * k, (by + 6) * k], fill=silk)
    name = "p8_board_battery.png" if battery else "p8_board.png"
    img.save(OUT / name, optimize=True)


def shell_svg():
    """The back shell: plastic, a floor inset 10, the wall's shade and two posts."""
    def path(inset):
        i = inset
        r, cut = max(R - i / 2, 4), CUT - i / 2
        return (f"M{i + r} {i} L{W - i - cut} {i} L{W - i} {i + cut} L{W - i} {H - i - r} "
                f"Q{W - i} {H - i} {W - i - r} {H - i} L{i + r} {H - i} Q{i} {H - i} {i} {H - i - r} "
                f"L{i} {i + r} Q{i} {i} {i + r} {i} Z")
    svg = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}">
  <defs><linearGradient id="wallShade" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#000" stop-opacity=".6"/><stop offset=".06" stop-color="#000" stop-opacity="0"/></linearGradient></defs>
  <path d="{path(0)}" fill="#ff00ff"/>
  <path d="{path(8)}" fill="#800080"/>
  <path d="{path(8)}" fill="url(#wallShade)"/>
  <circle cx="34" cy="78" r="10" fill="#ff00ff" stroke="#400040" stroke-width="1.24"/>
  <circle cx="34" cy="78" r="4" fill="#400040"/>
  <circle cx="{W - 34}" cy="78" r="10" fill="#ff00ff" stroke="#400040" stroke-width="1.24"/>
  <circle cx="{W - 34}" cy="78" r="4" fill="#400040"/>
</svg>
'''
    (OUT / "p8_shell.svg").write_text(svg)


def chip_svg():
    (OUT / "p8_chip.svg").write_text((OUT / "md_chip.svg").read_text())


if __name__ == "__main__":
    neutral()
    board(False)
    board(True)
    shell_svg()
    chip_svg()
    print("wrote", ", ".join(sorted(p.name for p in OUT.glob("p8_*"))))
