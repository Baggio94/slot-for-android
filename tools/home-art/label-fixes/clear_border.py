"""Make a label scan's white rounded-corner background transparent: flood fill from each
corner over near-white pixels, only inside a corner box, with a soft edge."""
import sys
from collections import deque
from PIL import Image

def clean(src, dst, box=40, white=232, soft=190):
    im = Image.open(src).convert('RGBA')
    w, h = im.size
    px = im.load()
    def light(p): return min(p[0], p[1], p[2])
    gone = set()
    # Every near-white pixel connected to the image's border, at most `box` pixels deep.
    q = deque([(x, y) for x in range(w) for y in (0, h - 1)] + [(x, y) for y in range(h) for x in (0, w - 1)])
    while q:
        x, y = q.popleft()
        if (x, y) in gone or not (0 <= x < w and 0 <= y < h) or light(px[x, y]) < white:
            continue
        if min(x, y, w - 1 - x, h - 1 - y) >= box:
            continue
        gone.add((x, y))
        q.extend(((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)))
    for x, y in gone:
        r, g, b, _ = px[x, y]
        px[x, y] = (r, g, b, 0)
    # Soft edge: light pixels touching the cleared area keep part of their opacity.
    for x, y in list(gone):
        for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
            if 0 <= nx < w and 0 <= ny < h and (nx, ny) not in gone:
                r, g, b, a = px[nx, ny]
                l = light((r, g, b))
                if l >= soft:
                    px[nx, ny] = (r, g, b, int(255 * (white - l) / (white - soft)) if l < white else 0)
    im.save(dst, optimize=True)
    return len(gone)

if __name__ == '__main__':
    print(clean(sys.argv[1], sys.argv[2]))
