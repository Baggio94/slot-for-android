"""Saves the cached support-2D renders of the carts in refs3.txt ("PLATFORM CRC" per line)
that this cache holds, as is and upscaled x4 (Real-ESRGAN x4plus, grey + alpha), into OUT:
the reference captures for new 3.0 moulds."""
import io, json, sys
from pathlib import Path
import numpy as np, torch
from PIL import Image
from scipy import ndimage
from spandrel import ModelLoader

cache, out = Path("cache"), Path(sys.argv[2])
out.mkdir(parents=True, exist_ok=True)
wanted = [l.split() for l in Path("tools/cart-colours/refs3.txt").read_text().splitlines() if l.strip()]
rows = {}
for name in ("seen-new.jsonl", "seen-home.jsonl"):
    if (cache / name).exists():
        for line in (cache / name).read_text().splitlines():
            r = json.loads(line)
            rows[(r["platform"], r["crc"])] = r
model = ModelLoader().load_from_file(sys.argv[1]).eval()

def up(rgb):
    t = torch.from_numpy(rgb).permute(2, 0, 1)[None].float() / 255
    with torch.no_grad():
        o = model(t)[0].clamp(0, 1).permute(1, 2, 0).numpy()
    return (o * 255 + .5).astype(np.uint8)

for platform, crc in wanted:
    r = rows.get((platform, crc))
    img = r and r.get("image") and cache / "img" / r["image"]
    if not img or not img.exists():
        continue
    a = np.array(Image.open(img).convert("RGBA"))
    Image.fromarray(a).save(out / f"{platform}-{crc}.png")
    m = a[:, :, 3] > 8
    idx = ndimage.distance_transform_edt(~m, return_distances=False, return_indices=True)
    rgb = a[:, :, :3][idx[0], idx[1]].copy()
    o = up(rgb)
    al = up(np.repeat(a[:, :, 3:4], 3, 2))[:, :, 0]
    Image.fromarray(np.dstack([o, al]), "RGBA").save(out / f"{platform}-{crc}-x4.png", optimize=True)
    print("saved", platform, crc, r["name"])
