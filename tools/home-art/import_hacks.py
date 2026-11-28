#!/usr/bin/env python3
"""Add the hack, translation and alternative labels of hack-labels/manifest.json to a slot. art set.

A ROM hack has no fixed CRC32 (each release of its patch gives another one), so these labels
are found by name instead: <out>/HACKS.json maps a hack's key (a few lower-case words, such as
"radical red") to label entries in the usual [type, region, file] form, the first one the label
to show, HACKS.names.json maps it to the hack's title and HACKS.platforms.json to its system
(GB, GBC or GBA). The app looks a cart up there when its CRC32 is in no index: `key_of(file
name)` below is the rule, the longest key whose words all stand together in the file name's
words.

Every label also joins its system's own index (GBA.json and GBA.names.json...) under a reserved
key of eight hex digits that no dump has, named like "Pokémon Radical Red (hack)", so that
Cart Studio's Search finds it by name; so do the alternative labels of official games, which
are never put on a cart by themselves. HACKS.extra.json lists these keys per system.

The images stay in the repository they come from, at the commit the manifest names, or in
hack-labels/own for labels made for slot.; each is checked by SHA-1, with the permission and
credits the manifest records (HACKS.about.json). Each is trimmed to its opaque part, never cut
further, scaled to at most 960 pixels wide and stored as PNG, named by its SHA-1.
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


def extra_key(platform, source, file, taken):
    """The reserved key of an extra label in <platform>.json: eight hex digits like a CRC32,
    fixed by where the label comes from, and never a key the index already holds."""
    salt = ''
    while True:
        key = hashlib.sha1(f'{platform}|{source}|{file}{salt}'.encode()).hexdigest()[:8].upper()
        if key not in taken:
            return key
        salt += '#'


def load(path):
    return json.loads(path.read_text(encoding='utf-8')) if path.exists() else {}


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':')),
                    encoding='utf-8')


def build(out, manifest=MANIFEST, source_dir=None):
    """`source_dir`: a folder holding a local clone of each source, named by the source's name."""
    out = Path(out)
    manifest = Path(manifest)
    data = json.loads(manifest.read_text(encoding='utf-8'))
    index, names, platforms, keys = {}, {}, {}, set()
    extras = []  # (platform, source, file, name, entry): what Cart Studio's Search lists
    about = {'sources': [], 'artists': {}}

    def stored(source, f):
        if source['source'].startswith('local:'):
            body = (manifest.parent / source['source'][6:].split('/', 1)[1] / f['file']).read_bytes()
        elif source_dir:
            body = (Path(source_dir) / source['name'] / f['file']).read_bytes()
        else:
            repo = urllib.parse.urlsplit(source['source']).path.strip('/')
            body = fetch(RAW.format(repo=repo, commit=source['commit'],
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
        return ['support-texture', 'wor', rel]

    for source in data['sources']:
        about['sources'].append({k: source[k] for k in
                                 ('name', 'source', 'commit', 'licence', 'permission', 'credits')
                                 if k in source})
        for hack in source.get('hacks', []):
            entries = [stored(source, f) for f in hack['files']]
            kind = hack.get('kind', 'hack')
            for key in [hack['key']] + hack.get('aliases', []):
                if key in keys or words(key) != key.split():
                    raise ValueError(f'bad or duplicate key: {key!r}')
                keys.add(key)
                index[key] = entries
                names[key] = hack['title']
                platforms[key] = hack['platform']
                if hack.get('artist'):
                    about['artists'][key] = hack['artist']
            for n, (f, entry) in enumerate(zip(hack['files'], entries), 1):
                # "Pokémon Unbound (hack, label 2)"; a translation's title already says so.
                tags = ([kind] if kind not in hack['title'] else []) + \
                    ([f'label {n}'] if len(entries) > 1 else [])
                name = hack['title'] + (f' ({", ".join(tags)})' if tags else '')
                extras.append((hack['platform'], source['name'], f['file'], name, entry))
        for alt in source.get('alternatives', []):
            for n, f in enumerate(alt['files'], 1):
                label = f'redesign by {source["name"]}' + (f', {n}' if len(alt['files']) > 1 else '')
                extras.append((alt['platform'], source['name'], f['file'], f'{alt["title"]} ({label})',
                               stored(source, f)))

    save(out / 'HACKS.json', index)
    save(out / 'HACKS.names.json', names)
    save(out / 'HACKS.platforms.json', platforms)
    (out / 'HACKS.about.json').write_text(json.dumps(about, ensure_ascii=False, indent=1),
                                          encoding='utf-8')

    # The extra labels join their platform's own index under reserved keys, so that Search
    # finds them by name. The keys of the previous run go first: the set stays what the
    # manifest says, and a label keeps its key from run to run.
    previous = load(out / 'HACKS.extra.json')
    extra_keys = {}
    for platform in sorted({e[0] for e in extras} | set(previous)):
        idx, nms = load(out / f'{platform}.json'), load(out / f'{platform}.names.json')
        for key in previous.get(platform, []):
            idx.pop(key, None)
            nms.pop(key, None)
        taken = set(idx) | set(nms)
        for p, source, file, name, entry in extras:
            if p != platform:
                continue
            key = extra_key(platform, source, file, taken)
            taken.add(key)
            idx[key] = [entry]
            nms[key] = name
            extra_keys.setdefault(platform, []).append(key)
        save(out / f'{platform}.json', idx)
        save(out / f'{platform}.names.json', nms)
    save(out / 'HACKS.extra.json', extra_keys)
    print(f'{len(index)} hack keys, {len(extras)} labels in Search '
          + ', '.join(f'{p} {len(k)}' for p, k in sorted(extra_keys.items())))
    return index


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    ap.add_argument('--out', required=True)
    ap.add_argument('--source-dir', help='local clones of the sources, one folder per source name')
    args = ap.parse_args()
    build(args.out, source_dir=args.source_dir)


if __name__ == '__main__':
    main()
