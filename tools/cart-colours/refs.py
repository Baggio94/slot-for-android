"""Prints the cached support-2D renders of the carts listed in refs.txt
(platform CRC per line), as is and upscaled x4 (Real-ESRGAN x4plus), as
base64 PNG lines in the log: the reference captures for new moulds."""
import base64, json, sys
from pathlib import Path

cache = Path("cache")
wanted = [l.split() for l in Path("tools/cart-colours/refs.txt").read_text().splitlines() if l.strip()]
rows = {}
for line in (cache / "seen-home.jsonl").read_text().splitlines():
    r = json.loads(line)
    rows[(r["platform"], r["crc"])] = r

def emit(tag, data):
    b = base64.b64encode(data).decode()
    for i in range(0, len(b), 30000):
        print(f"REF {tag} {i // 30000} {b[i:i + 30000]}")
    print(f"REFEND {tag} {len(data)}")

upscale = None
if len(sys.argv) > 1:
    import numpy as np, torch
    from PIL import Image
    from scipy import ndimage
    from spandrel import ModelLoader
    model = ModelLoader().load_from_file(sys.argv[1]).eval()
    def up(rgb):
        t = torch.from_numpy(rgb).permute(2, 0, 1)[None].float() / 255
        with torch.no_grad():
            o = model(t)[0].clamp(0, 1).permute(1, 2, 0).numpy()
        return (o * 255 + .5).astype(np.uint8)
    def upscale(path):
        import io
        a = np.array(Image.open(path).convert("RGBA"))
        m = a[:, :, 3] > 8
        idx = ndimage.distance_transform_edt(~m, return_distances=False, return_indices=True)
        rgb = a[:, :, :3][idx[0], idx[1]].copy()
        o = up(rgb)
        al = up(np.repeat(a[:, :, 3:4], 3, 2))[:, :, 0]
        buf = io.BytesIO()
        # The moulds only use the shell's light and its outline: grey + alpha.
        grey = (o.astype(np.float32) @ np.array([.299, .587, .114], np.float32) + .5).astype(np.uint8)
        Image.fromarray(np.dstack([grey, al]), "LA").save(buf, "PNG", optimize=True)
        return buf.getvalue()

for platform, crc in wanted:
    r = rows.get((platform, crc))
    img = r and r.get("image") and cache / "img" / r["image"]
    if not img or not img.exists():
        print("MISSING", platform, crc)
        continue
    print("NAME", crc, r["name"])
    if not upscale:
        emit(f"{crc}-x1", img.read_bytes())
    else:
        emit(f"{crc}-x4", upscale(img))
