"""Export the Watomsk overlay masters slot. ships (assets/watomsk/).

Usage: python3 export.py <Watomsk 1080p dir> <assets/watomsk dir>

<1080p dir> holds his console folders (GameBoy, GameBoyAdvance, NES, SNES,
Genesis, NEOGEO, MasterSystem, Turbografx16) from WatomsksOverlays-1080p
and his Turbografx16-1080p. Only his black variants ship (the
dark grey -G ones were dropped, Siham 2026-11-04). Two kinds of files:

* <name>-<3x2|4x3|16x9>.png: side frames, all 1080 high (his own
  resolution, so 3x panels stay sharp), the window cut for the full
  height at the system's shape (GB 10:9, GBA 3:2, home 4:3), centred. The
  app refits them to the real panel (fit_overlay).
* <name>-band.png: the logo (opaque, on the frame's colour) for screens
  narrower than the picture: the game sits at the top and the logo is
  centred in the band below (fit_band). Drawn at his 1080p scale.

Every design has a band (Siham 2026-11-27): Game Boy, NES, SNES, Neo Geo,
PC Engine and TurboGrafx take the logo of his right panel, turned to read
left to right, alone (logo_only: his lines above it dropped); the Master
System keeps the Sega grid behind its logo, as the Mega Drive.

The Game Boy 3:2 frames are make.py's (validated on the KONKR); GBA has no
LED anywhere and the Mega Drive band drops the POWER strip (Siham,
2026-11-03).
"""
import os
import sys

import numpy as np
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import compose as C  # noqa: E402
import make  # noqa: E402

VARIANTS = {
    "GameBoy": [("GB", "gb"), ("GBP", "gbp"), ("GBC", "gbc")],
    "GameBoyAdvance": [("GBA-Classic", "gba-classic"), ("GBA", "gba"),
                       ("GBASP", "gbasp"), ("GBA-Alt", "gba-alt")],
    "NES": [("NES", "nes"), ("NES-GrayLogo", "nes-graylogo")],
    "SNES": [("SNES", "snes"), ("SNES-Alt", "snes-alt"),
             ("SNES-GrayLogo", "snes-graylogo"), ("SNES-GrayLogo-Alt", "snes-graylogo-alt")],
    "Genesis": [("Genesis-Classic", "genesis-classic"), ("Genesis", "genesis"),
                ("Megadrive", "megadrive")],
    "NEOGEO": [("NeoGeo", "neogeo")],
    "MasterSystem": [("MasterSystem", "mastersystem")],
    "Turbografx16": [("PCEngine", "pce"), ("Turbografx", "tg16")],
}
SHAPE = {"GameBoy": 10 / 9, "GameBoyAdvance": 1.5, "NES": 4 / 3, "SNES": 4 / 3, "Genesis": 4 / 3,
         "NEOGEO": 4 / 3, "MasterSystem": 4 / 3, "Turbografx16": 4 / 3}


def side(src, W, H, aspect, logo=None):
    w = round(H * aspect)
    rect = ((W - w) // 2, 0, (W - w) // 2 + w, H)
    s = H / 1080
    cv = C.Canvas(W, H, rect, src, s)
    sl, sr = C.side_panels(src)
    tl, tr = (0, 0, rect[0], H), (rect[2], 0, W, H)
    if src.get("sega"):
        C.place_strip_side(cv, src, src["sega"], tl, s)
    elif logo is None:
        C.place_panel_group(cv, src, sl, tl, s, s)
    if logo is not None:
        lg = C.resize(logo, s)
        tw = tr[2] - tr[0]
        if lg.width > tw * 0.8:
            lg = C.resize(lg, tw * 0.8 / lg.width)
        cv.paste(lg, (tr[0] + tr[2] - lg.width) / 2, (H - lg.height) / 2, clip=tr)
    else:
        C.place_panel_group(cv, src, sr, tr, s, s)
    return cv.finish()


def centred_crop(src, box, centre):
    """Crop `box` grown so `centre` (source px) is the crop's centre."""
    x0, y0, x1, y1 = box
    cx, cy = centre
    hw = max(cx - x0, x1 - cx)
    hh = max(cy - y0, y1 - cy)
    bx0, by0 = round(cx - hw), round(cy - hh)
    bx1, by1 = round(cx + hw), round(cy + hh)
    out = Image.new("RGB", (bx1 - bx0, by1 - by0), src["bg"])
    H, W = src["flat"].shape[:2]
    inner = (max(bx0, 0), max(by0, 0), min(bx1, W), min(by1, H))
    out.paste(C.crop_rgb(src, inner), (inner[0] - bx0, inner[1] - by0))
    return out


def gba_logo(src):
    x0, y0, x1, y1 = src["rect"]
    lb = C.strong_bbox(src["flat"], src["bg"], (0, y1, 1920, 1080))
    pad = 8
    box = (lb[0] - pad, lb[1] - pad, lb[2] + pad, min(1080, lb[3] + pad))
    return centred_crop(src, box, ((lb[0] + lb[2]) / 2, (lb[1] + lb[3]) / 2))


def sega_logo(src):
    """The vertical logo with its grid, turned to read left to right."""
    _, sr = C.side_panels(src)
    sb = C.strong_bbox(src["flat"], src["bg"], sr)
    box = (sr[0] + 20, 0, 1920, 620)
    centre = ((sb[0] + sb[2]) / 2, (sb[1] + sb[3]) / 2)
    return centred_crop(src, box, centre).rotate(90, expand=True)


def logo_only(src, grid=False):
    """His right panel's logo alone, turned to read left to right: the
    lines running down to it from the top are left out, the logo centred
    with plain ground around it (and its halo, PC Engine / TurboGrafx).
    `grid`: the Master System's Sega grid stays behind it."""
    _, sr = C.side_panels(src)
    x0, y0, x1, y1 = sr
    flat = src["flat"].astype(np.int32)
    bg = np.array(src["bg"])
    strong = np.abs(flat[y0:y1, x0:x1] - bg).max(axis=2) > 40
    rows = np.nonzero(strong.any(axis=1))[0]
    top = rows.min()
    # A line runs straight down from the top: the same columns row after
    # row, until the logo (or a gap).
    ref = np.nonzero(strong[top])[0]
    y = top
    while y + 1 < strong.shape[0]:
        c = np.nonzero(strong[y + 1])[0]
        if (len(c) == 0 or abs(len(c) - len(ref)) > 1 or abs(c.min() - ref.min()) > 1
                or abs(c.max() - ref.max()) > 1):
            break
        y += 1
    line = y - top >= 40
    ly0 = y + 1 if line else top
    if line:
        # The line's own end (a cap, a fade), still joined to it.
        while ly0 < strong.shape[0]:
            c = np.nonzero(strong[ly0])[0]
            if len(c) == 0 or c.min() < ref.min() - 4 or c.max() > ref.max() + 4:
                break
            ly0 += 1
    stub_end = ly0
    while ly0 < strong.shape[0] and not strong[ly0].any():
        ly0 += 1
    ly1 = rows.max() + 1
    cols = np.nonzero(strong[ly0:ly1].any(axis=0))[0]
    box = [x0 + cols.min(), y0 + ly0, x0 + cols.max() + 1, y0 + ly1]
    if grid:
        return centred_crop(src, (sr[0] + 20, 0, 1920, 620),
                            ((box[0] + box[2]) / 2, (box[1] + box[3]) / 2)).rotate(90, expand=True)
    # Halfway through the gap: the logo's soft edge stays, the line goes.
    floor = y0 + (stub_end + ly0) // 2 if line else 2
    faint = np.abs(flat - bg).max(axis=2) > 3
    b = [box[0] - 14, max(box[1] - 14, floor), box[2] + 14, box[3] + 14]
    for _ in range(120):
        grown = False
        if faint[b[1]:b[3], b[0]].any() and b[0] > x0 + 2:
            b[0] -= 1; grown = True
        if faint[b[1]:b[3], b[2] - 1].any() and b[2] < 1918:
            b[2] += 1; grown = True
        if faint[b[1], b[0]:b[2]].any() and b[1] > floor:
            b[1] -= 1; grown = True
        if faint[b[3] - 1, b[0]:b[2]].any() and b[3] < 1078:
            b[3] += 1; grown = True
        if not grown:
            break
    crop = C.crop_rgb(src, tuple(b))
    ml, mr, mt, mb = box[0] - b[0], b[2] - box[2], box[1] - b[1], b[3] - box[3]
    mx, my = max(ml, mr), max(mt, mb)
    out = Image.new("RGB", (box[2] - box[0] + 2 * mx, box[3] - box[1] + 2 * my), src["bg"])
    out.paste(crop, (mx - ml, my - mt))
    return out.rotate(90, expand=True)


def save(img, path):
    img.save(path, optimize=True)


def main(src_dir, out):
    C.SRC = src_dir
    os.makedirs(out, exist_ok=True)
    for console, vs in VARIANTS.items():
        for name, key in vs:
            src = C.load(f"{console}/{name}.png")
            if console == "Genesis":
                src["sega"] = C.strip_info(src)
            tag = key
            shape = SHAPE[console]
            if console == "GameBoy":
                save(make.adapt(f"{src_dir}/{console}/{name}.png", (1620, 1080)),
                     f"{out}/{tag}-3x2.png")
                save(side(src, 1440, 1080, shape), f"{out}/{tag}-4x3.png")
                save(src["im"], f"{out}/{tag}-16x9.png")
                save(logo_only(src), f"{out}/{tag}-band.png")
            elif console == "GameBoyAdvance":
                logo = gba_logo(src)
                save(side(src, 1920, 1080, shape, logo.rotate(-90, expand=True)),
                     f"{out}/{tag}-16x9.png")
                save(logo, f"{out}/{tag}-band.png")
            else:
                save(side(src, 1620, 1080, shape), f"{out}/{tag}-3x2.png")
                save(side(src, 1920, 1080, shape), f"{out}/{tag}-16x9.png")
                if console == "Genesis":
                    save(sega_logo(src), f"{out}/{tag}-band.png")
                else:
                    save(logo_only(src, grid=console == "MasterSystem"), f"{out}/{tag}-band.png")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
