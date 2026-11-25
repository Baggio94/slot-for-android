#!/usr/bin/env python3
"""Merge reviewed manual Game Boy labels into a downloaded slot. art set."""
import argparse
import hashlib
import io
import json
import re
from pathlib import Path
from PIL import Image

MANIFEST = Path(__file__).parent / 'manual-labels' / 'manifest.json'
PLATFORMS = ('GB', 'GBC')


def read_map(path, required=False):
    if not path.exists() and not required:
        return {}
    data = json.loads(path.read_text(encoding='utf-8'))
    if not isinstance(data, dict):
        raise ValueError(f'Not an object: {path}')
    return data


def write_map(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_suffix(path.suffix + '.part')
    temp.write_text(json.dumps(value, sort_keys=True, separators=(',', ':')), encoding='utf-8')
    temp.replace(path)


def merge(out, manifest=MANIFEST, dry_run=False, allow_empty=False):
    out, manifest = Path(out), Path(manifest)
    rows = json.loads(manifest.read_text(encoding='utf-8'))
    seen = set()
    validated = []
    for row in rows:
        platform, crc, name = row['platform'], row['crc32'], row['name']
        region = row['region']
        if (platform not in PLATFORMS or not re.fullmatch(r'[0-9A-F]{8}', crc)
                or crc in seen or not name.strip() or region not in ('us', 'wor', 'eu', 'jp')):
            raise ValueError(f'Invalid or duplicate manifest entry: {row}')
        seen.add(crc)
        path = manifest.parent / row['image']
        if row['image'] != f'{platform}/{crc}.png':
            raise ValueError(f'Invalid path: {path}')
        body = path.read_bytes()
        sha = hashlib.sha1(body).hexdigest()
        if sha != row['sha1'] or len(body) > 6 * 1024 * 1024:
            raise ValueError(f'Bad digest or size: {path}')
        with Image.open(io.BytesIO(body)) as img:
            if img.format != 'PNG' or not all(32 <= v <= 4096 for v in img.size):
                raise ValueError(f'Invalid PNG: {path}')
            img.load()
        rel = f'files/{sha[:2]}/{sha}.png'
        validated.append((row, body, rel))
    maps = {}
    for p in PLATFORMS:
        paths = [out / f'{p}.json', out / f'{p}.names.json',
                 out / '.state' / f'{p}.derived.json', out / '.state' / f'{p}.asked.json']
        maps[p] = [(path, read_map(path, required=(i < 2 and not allow_empty)))
                   for i, path in enumerate(paths)]
    result = {'added': 0, 'unchanged': 0, 'conflicts': 0}
    writes = {}
    for row, body, rel in validated:
        p, crc, name = row['platform'], row['crc32'], row['name']
        (_, index), (_, names), (_, derived), (_, asked) = maps[p]
        other = 'GB' if p == 'GBC' else 'GBC'
        other_index, other_names = maps[other][0][1], maps[other][1][1]
        if crc not in index and (crc in other_index or crc in other_names):
            print(f'CONFLICT {p} {crc}: present in {other}; review platform before import')
            result['conflicts'] += 1
            continue
        entries = index.get(crc, [])
        if not isinstance(entries, list) or any(not isinstance(e, list) or len(e) < 3 for e in entries):
            raise ValueError(f'Malformed index entry {p} {crc}')
        scans = [e for e in entries if e[0] == 'support-texture']
        own = ['support-texture', row['region'], rel]
        already = any(e[2] == rel for e in scans)
        if scans and not already and crc not in derived:
            print(f'CONFLICT {p} {crc}: existing independent scan preserved')
            result['conflicts'] += 1
            continue
        if crc in derived:
            entries = []
            already = False
        if not already:
            index[crc] = [own] + [e for e in entries if e[0] != 'support-texture']
            result['added'] += 1
        else:
            result['unchanged'] += 1
        names.setdefault(crc, name)
        derived.pop(crc, None)
        asked.pop(crc, None)
        writes[rel] = body
        print(f'{"KEEP" if already else "ADD"} {p} {crc} {name} -> {rel}')
    if not dry_run and not result['conflicts']:
        for rel, body in writes.items():
            path = out / rel
            if path.exists():
                if path.read_bytes() != body:
                    raise ValueError(f'Content-addressed path mismatch: {path}')
            else:
                path.parent.mkdir(parents=True, exist_ok=True)
                tmp = path.with_suffix('.png.part')
                tmp.write_bytes(body)
                tmp.replace(path)
        for paths in maps.values():
            for path, value in paths:
                write_map(path, value)
    print(result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--manifest', type=Path, default=MANIFEST)
    parser.add_argument('--dry-run', action='store_true')
    parser.add_argument('--allow-empty', action='store_true', help='Local tests only; not for production')
    args = parser.parse_args()
    result = merge(args.out, args.manifest, args.dry_run, args.allow_empty)
    raise SystemExit(2 if result['conflicts'] else 0)


if __name__ == '__main__':
    main()
