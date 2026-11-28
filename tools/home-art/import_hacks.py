#!/usr/bin/env python3
"""Add the ROM hack labels of hack-labels/manifest.json to a slot. art set.

A ROM hack has no fixed CRC32 (each release of its patch gives another one), so these labels
are found by name instead: <out>/HACKS.json maps a hack's key (a few lower-case words, such as
"radical red") to label entries in the usual [type, region, file] form, the first one the label
to show, and HACKS.names.json maps it to the hack's title. The app looks a cart up there when
its CRC32 is in no index: `key_of(file name)` below is the rule, the longest key whose words
all stand together in the file name's words.

The images stay in the repository they come from, at the commit the manifest names (each
checked by SHA-1), with the permission and credits the manifest records. Each is trimmed to its
opaque part, scaled to at most 960 pixels wide and stored as PNG, named by its SHA-1.
"""
import argparse
import hashlib
import io
import json
import re
import unicodedata
import urllib.parse
import urllib.request
from pathlib import Path

from PIL import Image

MANIFEST = Path(__file__).parent / 'hack-labels' / 'manifest.json'
RAW = 'https://raw.githubusercontent.com/{repo}/{commit}/{file}'
MAX_WIDTH = 960

# Words a hack's file name carries that never tell one hack from another.
NOISE = {'pokemon', 'version', 'hack', 'rom', 'by', 'the', 'gba', 'gbc', 'gb', 'usa',
         'europe', 'japan', 'world', 'en', 'final', 'beta', 'patched', 'patch'}


def words(text):
    """'Pokémon - Radical Red (v4.1) [Hack].gba' -> ['radical', 'red']"""
    text = unicodedata.normalize('NFKD', text).encode('ascii', 'ignore').decode().lower()
    text = re.sub(r'\.(gba|gbc|gb|zip|7z|bps|ips|ups)$', '', text)
    text = text.replace("'", '').replace('&', ' and ').replace('+', ' plus ')
    text = re.sub(r'\bv?\d+(\.\d+)+[a-z]?\b|\bv\d+[a-z]?\b', ' ', text)  # version numbers
    text = re.sub(r'\b([a-z])\.(?=[a-z]\b)', r'\1', text)  # R.O.W.E. -> rowe
    text = re.sub(r'\bfire\s*red\b', 'firered', text)
    text = re.sub(r'\bleaf\s*green\b', 'leafgreen', text)
    return [w for w in re.findall(r'[a-z0-9]+', text) if w not in NOISE]


def key_of(file_name, titles):
    """The hack a file is of: the longest key whose words stand together in its name. A hack
    whose title starts with Pokémon also needs the word in the file name, so that a game merely
    called Peach or Vega never wears a hack's label. `titles`: key -> title (HACKS.names.json)."""
    have = words(file_name)
    pokemon = 'pokemon' in re.findall(r'[a-z]+', unicodedata.normalize('NFKD', file_name)
                                      .encode('ascii', 'ignore').decode().lower())
    best = None
    for key, title in titles.items():
        if title.startswith('Pokémon') and not pokemon:
            continue
        k = key.split()
        if any(have[i:i + len(k)] == k for i in range(len(have) - len(k) + 1)):
            if best is None or len(k) > len(best.split()):
                best = key
    return best


def fetch(url):
    req = urllib.request.Request(url, headers={'User-Agent': 'slot. art harvest'})
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read()


def prepare(body):
    """The label as stored: its opaque part, at most MAX_WIDTH wide, as PNG."""
    with Image.open(io.BytesIO(body)) as im:
        im = im.convert('RGBA')
    alpha = im.getchannel('A')
    if alpha.getextrema()[0] < 255:
        box = alpha.point(lambda v: 255 if v > 8 else 0).getbbox()
        if box:
            im = im.crop(box)
    if im.getchannel('A').getextrema() == (255, 255):
        im = im.convert('RGB')
    if im.width > MAX_WIDTH:
        im = im.resize((MAX_WIDTH, round(im.height * MAX_WIDTH / im.width)), Image.LANCZOS)
    buf = io.BytesIO()
    im.save(buf, 'PNG', optimize=True)
    return buf.getvalue()


def build(out, manifest=MANIFEST, source_dir=None):
    out = Path(out)
    data = json.loads(Path(manifest).read_text(encoding='utf-8'))
    repo = urllib.parse.urlsplit(data['source']).path.strip('/')
    index, names, keys = {}, {}, set()
    for hack in data['hacks']:
        key = hack['key']
        if key in keys or words(key) != key.split():
            raise ValueError(f'bad or duplicate key: {key!r}')
        keys.add(key)
        entries = []
        for f in hack['files']:
            if source_dir:
                body = (Path(source_dir) / f['file']).read_bytes()
            else:
                body = fetch(RAW.format(repo=repo, commit=data['commit'],
                                        file=urllib.parse.quote(f['file'])))
            if hashlib.sha1(body).hexdigest() != f['sha1']:
                raise ValueError(f'{f["file"]} is not the file the manifest names')
            png = prepare(body)
            sha = hashlib.sha1(png).hexdigest()
            rel = f'files/{sha[:2]}/{sha}.png'
            path = out / rel
            if not path.exists():
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(png)
            entries.append(['support-texture', 'wor', rel])
        index[key] = entries
        names[key] = hack['title']
    for name, value in (('HACKS.json', index), ('HACKS.names.json', names)):
        (out / name).write_text(json.dumps(value, ensure_ascii=False, sort_keys=True,
                                           separators=(',', ':')), encoding='utf-8')
    about = {k: data[k] for k in ('source', 'commit', 'permission', 'credits')}
    about['artists'] = {h['key']: h['artist'] for h in data['hacks'] if h.get('artist')}
    (out / 'HACKS.about.json').write_text(json.dumps(about, ensure_ascii=False, indent=1),
                                          encoding='utf-8')
    print(f'{len(index)} hacks, {sum(map(len, index.values()))} labels')
    return index


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    ap.add_argument('--out', required=True)
    ap.add_argument('--source-dir', help='a local clone of the source repository')
    args = ap.parse_args()
    build(args.out, source_dir=args.source_dir)


if __name__ == '__main__':
    main()
