#!/usr/bin/env python3
"""What an art set weighs, and what it would weigh stored smaller.

    tools/home-art/stats.py --out ../slot-art
"""

import argparse
import io
from pathlib import Path

from PIL import Image

LABEL_MAX = 960  # HomeCartLabels.LABEL_MAX: the app never keeps a label larger than this


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', required=True)
    args = ap.parse_args()
    files = sorted(p for p in (Path(args.out) / 'files').rglob('*.*') if not p.name.endswith('.part'))
    total = small_png = small_webp = 0
    for f in files:
        body = f.read_bytes()
        with Image.open(io.BytesIO(body)) as im:
            im.load()
            w, h = im.size
            k = min(1.0, LABEL_MAX / max(w, h))
            small = im.resize((max(1, int(w * k)), max(1, int(h * k))), Image.BILINEAR) if k < 1 else im
            png, webp = io.BytesIO(), io.BytesIO()
            small.save(png, 'PNG', optimize=True)
            small.save(webp, 'WEBP', lossless=True, method=6)
        total += len(body)
        small_png += len(png.getvalue())
        small_webp += len(webp.getvalue())
        print(f'{f.name[:10]}  {im.format:4} {w:4}x{h:<4} {len(body) // 1024:5} KiB'
              f'  -> {LABEL_MAX}px png {len(png.getvalue()) // 1024:4} KiB, webp {len(webp.getvalue()) // 1024:4} KiB')
    mib = 1048576
    print(f'\n{len(files)} files: {total / mib:.1f} MiB as stored, '
          f'{small_png / mib:.1f} MiB at {LABEL_MAX}px PNG, {small_webp / mib:.1f} MiB at {LABEL_MAX}px lossless WebP')


if __name__ == '__main__':
    main()
