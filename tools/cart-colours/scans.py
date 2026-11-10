"""Prints ScreenScraper's label scans (support-texture) and fronts (support-2D) of the
carts in scans.txt ("MD CRC name" per line), every region, as base64 JPEG lines in the
log, so their shape can be checked against slot.'s label wells."""
import base64, io, sys, urllib.parse, json
from pathlib import Path
from PIL import Image
sys.path.insert(0, str(Path(__file__).parent))
from scrape import jeu_infos, get, SYSTEMS

def emit(tag, data):
    b = base64.b64encode(data).decode()
    for i in range(0, len(b), 30000):
        print(f"REF {tag} {i // 30000} {b[i:i + 30000]}")
    print(f"REFEND {tag} {len(data)}")

for line in Path("tools/cart-colours/scans.txt").read_text().splitlines():
    if not line.strip():
        continue
    system, crc, name = line.split(" ", 2)
    code, body = jeu_infos(SYSTEMS[system][1], {"crc": crc, "size": "", "name": name})
    if code != 200:
        print("FAIL", crc, code)
        continue
    medias = json.loads(body)["response"]["jeu"].get("medias", [])
    for m in medias:
        if m.get("type") not in ("support-texture", "support-2D"):
            continue
        c, img = get(m["url"])
        if c != 200:
            print("FAIL", crc, m["type"], m.get("region"), c)
            continue
        im = Image.open(io.BytesIO(img))
        print("MEDIA", crc, m["type"], m.get("region"), im.size, name)
        im = im.convert("RGB")
        im.thumbnail((640, 640))
        buf = io.BytesIO()
        im.save(buf, "JPEG", quality=82)
        emit(f"{crc}-{m['type']}-{m.get('region')}", buf.getvalue())
