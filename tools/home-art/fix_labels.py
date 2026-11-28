#!/usr/bin/env python3
"""Swap published label files for cleaned copies (label-fixes/manifest.json).

ScreenScraper's scan of a label sometimes keeps the paper around it (white corners). A fix
names the published file it replaces and a cleaned image kept here (made with
label-fixes/clear_border.py); every index entry naming the old file then names the new one.
Runs after each harvest, so a dump asked again keeps the cleaned label.
"""
import argparse
import hashlib
import json
from pathlib import Path

FIXES = Path(__file__).parent / 'label-fixes'


def apply(out, fixes=FIXES):
    out, fixes = Path(out), Path(fixes)
    swap = {}
    for fix in json.loads((fixes / 'manifest.json').read_text(encoding='utf-8')):
        body = (fixes / fix['image']).read_bytes()
        sha = hashlib.sha1(body).hexdigest()
        if sha != fix['sha1']:
            raise ValueError(f'{fix["image"]} is not the image the manifest names')
        rel = f'files/{sha[:2]}/{sha}{Path(fix["image"]).suffix}'
        path = out / rel
        if not path.exists():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(body)
        swap[fix['replaces']] = rel
    changed = 0
    for index_path in sorted(out.glob('*.json')):
        if index_path.name.endswith(('.names.json', '.about.json')):
            continue
        index = json.loads(index_path.read_text(encoding='utf-8'))
        if not isinstance(index, dict):
            continue
        hit = 0
        for entries in index.values():
            for e in entries if isinstance(entries, list) else ():
                if isinstance(e, list) and len(e) >= 3 and e[2] in swap:
                    e[2] = swap[e[2]]
                    hit += 1
        if hit:
            index_path.write_text(json.dumps(index, sort_keys=True, separators=(',', ':')),
                                  encoding='utf-8')
            print(f'{index_path.name}: {hit} entries now name a cleaned label')
            changed += hit
    return changed


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    ap.add_argument('--out', required=True)
    apply(ap.parse_args().out)


if __name__ == '__main__':
    main()
