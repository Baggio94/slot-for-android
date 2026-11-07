#!/usr/bin/env python3
"""A contact sheet of an art set: every dump's stored labels beside its No-Intro name.

    tools/home-art/preview.py --out ../slot-art [--platform SNES]   # writes <out>/preview.html
"""

import argparse
import html
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import harvest  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', required=True)
    ap.add_argument('--platform', choices=sorted(harvest.PLATFORMS) + ['all'], default='all')
    ap.add_argument('--dat', help='a local dat (one platform only)')
    args = ap.parse_args()
    out = Path(args.out)
    chosen = sorted(harvest.PLATFORMS) if args.platform == 'all' else [args.platform]
    rows = []
    for platform in chosen:
        index = harvest.load(out / f'{platform}.json', {})
        if not index:
            continue
        names = harvest.parse_dat(harvest.read_dat(platform, args.dat))
        for crc, entry in sorted(index.items(), key=lambda kv: names.get(kv[0], (kv[0],))[0]):
            name = names.get(crc, (crc,))[0]
            print(f'{platform} {crc} {name}: ' +
                  (', '.join(f'{t}·{r} {Path(f).name[:8]}' for t, r, f in entry) or 'no label'))
            cells = ''.join(
                f'<figure><img src="{html.escape(f)}" loading="lazy">'
                f'<figcaption>{html.escape(t)} · {html.escape(r)}</figcaption></figure>'
                for t, r, f in entry) or '<em>no label</em>'
            rows.append(f'<tr><td>{platform}</td><td><code>{crc}</code></td>'
                        f'<td>{html.escape(name)}</td><td class="art">{cells}</td></tr>')
    page = f'''<!DOCTYPE html><meta charset="utf-8"><title>slot. home art</title>
<style>body{{font:14px system-ui;background:#1b1b21;color:#eee}}td{{padding:6px;vertical-align:top;border-bottom:1px solid #333}}
.art{{display:flex;gap:10px;flex-wrap:wrap}}figure{{margin:0}}img{{max-height:160px;max-width:320px;display:block;background:#333}}
figcaption{{font-size:11px;color:#aaa}}</style>
<table>{''.join(rows)}</table>'''
    (out / 'preview.html').write_text(page)
    print(f'{len(rows)} dumps in {out / "preview.html"}')


if __name__ == '__main__':
    main()
