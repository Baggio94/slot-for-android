#!/usr/bin/env python3
"""Build slot.'s label set for every system it plays carts of, ahead of time.

The same idea as slot.'s own art set for the Game Boy family (cart-studio-web's
tools/harvest.py, which this follows): ScreenScraper is asked once, here, for every dump
No-Intro lists, and the images land in a folder served as static files. The app then looks a
cart up by its CRC32 in an index instead of querying ScreenScraper itself, one request at a
time, on a developer quota every slot. user shares.

The app keeps deciding which label a cart wears (HomeCartLabels.pick, from the shell it is
drawn in) and keeps processing it (the whole scan, or the label cut out of the front render).
So this stores ScreenScraper's images pixel for pixel (a PNG as lossless WebP only when decoding
it back gives every pixel unchanged), and for every dump enough of them that
pick() over the stored set always answers what it would over ScreenScraper's full list:
per region family (us, wor, Europe, jp, ss) the first scan in the app's own order, else the
first front render. `select` holds that rule and test_harvest.py proves the equivalence
against a port of the Kotlin pick.

Files are named by their SHA-1, so the revisions and alternate dumps of one game share them.
Each platform gets an index, <out>/<PLATFORM>.json: CRC32 -> [[type, region, file], ...],
an empty list for a dump ScreenScraper has no label for, and <PLATFORM>.names.json: CRC32 -> dat
name for every dump with art, which Cart Studio searches (Another game…).

ScreenScraper's quota is per account, per minute and per day. This reads the account's own
limits, paces to them and stops when the day's allowance is spent; a later run skips every
dump already in the index, so a full set is built across as many days as it needs.

    SS_DEVID=... SS_DEVPASSWORD=... [SS_SSID=... SS_SSPASSWORD=...] \\
        tools/home-art/harvest.py --platform SNES --out ../slot-art --limit 40

Images are ScreenScraper's (CC BY-NC-SA 4.0); names and CRCs come from libretro-database
(CC BY-SA 4.0). A published set has to carry both attributions.
"""

import argparse
import hashlib
import json
import os
import re
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

API = 'https://api.screenscraper.fr/api2'
LIBRETRO = 'https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/no-intro/'

LIBRETRO_HACKS = 'https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/hacks/'

# slot.'s platform names, libretro's dat for each, and ScreenScraper's system id
# (HomeCartLabels.systemId for the home consoles). The ids are checked against ScreenScraper's
# live list at start.
PLATFORMS = {
    'GB': 'Nintendo - Game Boy',
    'GBC': 'Nintendo - Game Boy Color',
    'GBA': 'Nintendo - Game Boy Advance',
    'NES': 'Nintendo - Nintendo Entertainment System',
    'SNES': 'Nintendo - Super Nintendo Entertainment System',
    'MD': 'Sega - Mega Drive - Genesis',
}
SS_SYSTEM = {'GB': 9, 'GBC': 10, 'GBA': 12, 'NES': 3, 'SNES': 4, 'MD': 1}
SS_EXPECT = {
    'GB': ('game boy',),
    'GBC': ('game boy color',),
    'GBA': ('game boy advance',),
    'NES': ('nes', 'nintendo entertainment system', 'famicom'),
    'SNES': ('super nintendo', 'snes', 'super famicom'),
    'MD': ('mega drive', 'megadrive', 'genesis'),
}
EXTENSION = {'GB': '.gb', 'GBC': '.gbc', 'GBA': '.gba', 'NES': '.nes', 'SNES': '.sfc', 'MD': '.md'}
HOME = {'NES', 'SNES', 'MD'}

# HomeCartLabels.labelRegions: the regions the app ever asks for, Europe always as this block.
EUROPE = ('eu', 'fr', 'de', 'uk', 'sp', 'it', 'nl', 'se', 'au')
FAMILIES = (('us',), ('wor',), EUROPE, ('jp',), ('ss',))
SCAN, RENDER, LOGO = 'support-texture', 'support-2D', 'wheel'

# A dump's own region first, then the regions most dumps in circulation come from
# (cart-studio-web's harvest.regions_for, which slot.'s Game Boy art set was built with).
REGIONS = ('us', 'wor', 'eu', 'jp')
COUNTRY = {
    'USA': 'us', 'World': 'wor', 'Europe': 'eu', 'Japan': 'jp', 'Germany': 'de', 'France': 'fr',
    'Spain': 'sp', 'Italy': 'it', 'UK': 'uk', 'Australia': 'au', 'Netherlands': 'nl',
    'Sweden': 'se', 'Korea': 'kr', 'Brazil': 'br', 'China': 'cn',
}

# The app refuses anything outside these bounds (HomeCartLabels.labelPng).
MIN_SIDE, MAX_SIDE = 32, 4096
MAX_IMAGE = 6 * 1024 * 1024
SECRET_PARAMS = {'devid', 'devpassword', 'ssid', 'sspassword'}


# ------------------------------------------------------------ selection ---

def https(m):
    return str(m.get('url', '')).startswith('https://')


def first(medias, kind, regions):
    """The first media of a kind in the given region order, or None."""
    return next((m for region in regions for m in medias
                 if m.get('type') == kind and m.get('region') == region and https(m)), None)


def own_regions(name):
    """The region order for a dump: the country its name gives first, then REGIONS."""
    found = re.search(r'\(([^)]*)\)', name or '')
    own = COUNTRY.get(found.group(1).split(',')[0].strip()) if found else None
    first_ = [own] + (['eu'] if own in EUROPE[1:] else []) if own else []
    return tuple(dict.fromkeys(first_ + list(REGIONS)))


def home_labels(medias):
    """The label media to keep for a home cart: per region family the first scan, in the
    app's order, else the first front render. Every pick HomeCartLabels can make over
    ScreenScraper's list it makes identically over this subset (test_harvest.py)."""
    kept = []
    for family in FAMILIES:
        for kind in (SCAN, RENDER):
            hit = first(medias, kind, family)
            if hit:
                kept.append(hit)
                break
    return kept


def select(platform, name, medias):
    """What to keep for one dump: its label media (every region a home cart's shell can ask
    for; the dump's own region's scan for a Game Boy cart, as slot.'s art set has it) and one
    clear logo, for Logo Only."""
    if platform in HOME:
        kept = home_labels(medias)
    else:
        scan = first(medias, SCAN, own_regions(name))
        kept = [scan] if scan else []
    logo = first(medias, LOGO, own_regions(name))
    return kept + ([logo] if logo else [])


# --------------------------------------------------------- screenscraper ---

class QuotaSpent(Exception):
    pass


class Refused(Exception):
    pass


def creds():
    missing = [k for k in ('SS_DEVID', 'SS_DEVPASSWORD') if not os.environ.get(k)]
    if missing:
        sys.exit(f'set {" and ".join(missing)} in the environment')
    c = {
        'devid': os.environ['SS_DEVID'],
        'devpassword': os.environ['SS_DEVPASSWORD'],
        'softname': os.environ.get('SS_SOFTNAME') or 'slot',
        'output': 'json',
    }
    # A user account carries the quota; without one the daily allowance is small.
    if os.environ.get('SS_SSID'):
        c['ssid'] = os.environ['SS_SSID']
        c['sspassword'] = os.environ.get('SS_SSPASSWORD', '')
    return c


def call(endpoint, params, tries=4):
    """One API call, backing off rather than hammering a service that limits by design."""
    for attempt in range(tries):
        url = f'{API}/{endpoint}?' + urllib.parse.urlencode(params)
        req = urllib.request.Request(url, headers={'User-Agent': 'slot. art harvest'})
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                return json.loads(r.read().decode('utf-8', 'replace'))
        except urllib.error.HTTPError as e:
            if e.code == 404:  # not a game ScreenScraper knows: an answer, not a failure
                return None
            if e.code in (430, 431):
                raise QuotaSpent(f'ScreenScraper says the quota is spent (HTTP {e.code})')
            if e.code in (400, 401, 403, 423, 426):
                raise Refused(f'ScreenScraper refused the request (HTTP {e.code})')
            if e.code == 429 and attempt < tries - 1:
                wait = 20 * (attempt + 1)
                print(f'    rate limited, waiting {wait}s', flush=True)
                time.sleep(wait)
                continue
            raise
        except (urllib.error.URLError, json.JSONDecodeError, TimeoutError):
            if attempt == tries - 1:
                raise
            time.sleep(5 * (attempt + 1))
    return None


def quota(c):
    # ssuserInfos needs a user account; without one (as the app asks), go slowly on one thread
    # and let ScreenScraper's own 430/431 say when the day is over.
    fallback = {'per_min': 20, 'per_day': None, 'today': 0, 'threads': 1}
    if 'ssid' not in c:
        return fallback
    try:
        data = call('ssuserInfos.php', c) or {}
    except Refused as e:
        print(f'no account limits ({e}); going slowly', flush=True)
        return fallback
    u = data.get('response', {}).get('ssuser', {}) or {}

    def num(*keys):
        for k in keys:
            try:
                return int(str(u.get(k)).strip())
            except (TypeError, ValueError):
                pass
        return None
    return {
        'per_min': num('maxrequestspermin'),
        'per_day': num('maxrequestsperday'),
        'today': num('requeststoday') or 0,
        'threads': num('maxthreads') or 1,
    }


def check_systems(c, chosen):
    data = call('systemesListe.php', c)
    listed = (data or {}).get('response', {}).get('systemes', [])
    if not listed:
        sys.exit('ScreenScraper would not list its systems: check the credentials')
    names = {}
    for s in listed:
        try:
            names[int(s['id'])] = ' | '.join(str(v).lower() for v in (s.get('noms') or {}).values())
        except (TypeError, ValueError, KeyError):
            continue
    for platform in chosen:
        sid = SS_SYSTEM[platform]
        got = names.get(sid, '')
        if not any(word in got for word in SS_EXPECT[platform]):
            sys.exit(f'ScreenScraper system {sid} is "{got}", not {platform}: ids have moved')


def public(url):
    """A media url without the credentials ScreenScraper puts in it."""
    parts = urllib.parse.urlsplit(url)
    query = [(k, v) for k, v in urllib.parse.parse_qsl(parts.query) if k not in SECRET_PARAMS]
    return urllib.parse.urlunsplit(parts._replace(query=urllib.parse.urlencode(query)))


def fetch(url, tries=3):
    for attempt in range(tries):
        try:
            req = urllib.request.Request(url, headers={'User-Agent': 'slot. art harvest'})
            with urllib.request.urlopen(req, timeout=120) as r:
                body = r.read(MAX_IMAGE + 1)
            return body if 256 <= len(body) <= MAX_IMAGE else None
        except urllib.error.HTTPError as e:
            if e.code == 404:
                return None
            if e.code in (430, 431):
                raise QuotaSpent(f'ScreenScraper says the quota is spent (HTTP {e.code})')
            if attempt == tries - 1:
                raise
        except (urllib.error.URLError, TimeoutError):
            if attempt == tries - 1:
                raise
        time.sleep(5 * (attempt + 1))
    return None


# ----------------------------------------------------------------- files ---

def kind_of(body):
    if body.startswith(b'\x89PNG\r\n\x1a\n'):
        return '.png'
    if body.startswith(b'\xff\xd8\xff'):
        return '.jpg'
    if body[:4] == b'RIFF' and body[8:12] == b'WEBP':
        return '.webp'
    return None


def usable(body):
    """An image the app will accept: a known format, decodable, within its size bounds."""
    ext = kind_of(body) if body else None
    if not ext:
        return None
    try:
        import io
        from PIL import Image
        with Image.open(io.BytesIO(body)) as im:
            w, h = im.size
            im.verify()
        if not (MIN_SIDE <= w <= MAX_SIDE and MIN_SIDE <= h <= MAX_SIDE):
            return None
    except ImportError:
        pass  # without Pillow only the signature is checked
    except Exception:
        return None
    return ext


def as_webp(body):
    """The same pixels as lossless WebP, about a third lighter, or None to keep the original:
    without Pillow, when it would not be smaller, when the image carries a colour profile (that
    Android could honour differently) or when decoding it back gives any other pixel."""
    try:
        import io
        from PIL import Image
        with Image.open(io.BytesIO(body)) as im:
            if im.info.get('icc_profile') or im.info.get('gamma') or getattr(im, 'n_frames', 1) > 1:
                return None
            if im.mode not in ('RGB', 'RGBA', 'L', 'LA', 'P'):
                return None
            pixels = im.convert('RGBA')
        out = io.BytesIO()
        pixels.save(out, 'WEBP', lossless=True, method=6, exact=True)
        webp = out.getvalue()
        if len(webp) >= len(body):
            return None
        with Image.open(io.BytesIO(webp)) as back:
            if back.convert('RGBA').tobytes() != pixels.tobytes():
                return None
        return webp
    except Exception:
        return None


def store(body, out, webp=True):
    """Write a file under its SHA-1 and return its path relative to the set."""
    ext = usable(body)
    if not ext:
        return None
    if webp and ext == '.png':
        smaller = as_webp(body)
        if smaller:
            body, ext = smaller, '.webp'
    sha = hashlib.sha1(body).hexdigest()
    rel = f'files/{sha[:2]}/{sha}{ext}'
    dest = out / rel
    if not dest.exists():
        dest.parent.mkdir(parents=True, exist_ok=True)
        tmp = dest.with_suffix(dest.suffix + '.part')
        tmp.write_bytes(body)
        tmp.replace(dest)
    return rel


def load(path, default):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return default


def write_json(path, data):
    """Compact and sorted, so two runs diff cleanly; atomic, so a kill leaves the old file."""
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + '.part')
    tmp.write_text(json.dumps(data, separators=(',', ':'), sort_keys=True))
    tmp.replace(path)


# ------------------------------------------------------------------- dat ---

def parse_dat(text):
    """CRC32 -> (No-Intro name, size) for every rom of a clrmamepro dat."""
    out = {}
    for block in re.finditer(r'game\s*\((.*?)\n\)', text, re.S):
        body = block.group(1)
        name = re.search(r'^\s*name\s+"([^"]+)"', body, re.M)
        for rom in re.finditer(r'^\s*rom\s*\((.*)\)\s*$', body, re.M):
            # File names carry parentheses and words of their own: look past quoted strings.
            fields = re.sub(r'"[^"]*"', '""', rom.group(1))
            crc = re.search(r'\bcrc\s+([0-9A-Fa-f]{8})\b', fields)
            size = re.search(r'\bsize\s+(\d+)', fields)
            if name and crc:
                out.setdefault(crc.group(1).upper(), (name.group(1), int(size.group(1)) if size else 0))
    return out


def read_dat(platform, local=None, base=LIBRETRO):
    if local:
        return Path(local).read_text('utf-8', 'replace')
    url = base + urllib.parse.quote(PLATFORMS[platform]) + '.dat'
    try:
        with urllib.request.urlopen(url, timeout=120) as r:
            return r.read().decode('utf-8', 'replace')
    except urllib.error.HTTPError as e:
        if e.code == 404 and base != LIBRETRO:
            return ''  # not every platform has a hacks dat
        raise


def entries_for(platform, local=None, hacks=True):
    """Every dump to ask about: No-Intro's, then libretro's known rom hacks (a No-Intro name
    wins where both list a checksum)."""
    entries = parse_dat(read_dat(platform, local))
    if hacks and not local:
        for crc, entry in parse_dat(read_dat(platform, base=LIBRETRO_HACKS)).items():
            entries.setdefault(crc, entry)
    return entries


# ------------------------------------------------------------------ main ---

def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    ap.add_argument('--platform', choices=sorted(PLATFORMS) + ['all'], default='all')
    ap.add_argument('--out', required=True, help='the art set folder, created if needed')
    ap.add_argument('--limit', type=int, help='stop after this many games, for a trial run')
    ap.add_argument('--match', help='only dumps whose No-Intro name matches this regex')
    ap.add_argument('--dat', help='a local dat instead of libretro\'s (one platform only)')
    ap.add_argument('--no-hacks', action='store_true', help='No-Intro dumps only, not libretro\'s rom hacks')
    ap.add_argument('--max-minutes', type=float,
                    help='stop asking after this long, leaving a complete index (CI time limits)')
    ap.add_argument('--keep-png', action='store_true',
                    help='store ScreenScraper\'s PNGs as they are instead of as lossless WebP')
    ap.add_argument('--retry-missing', action='store_true',
                    help='ask again for dumps recorded with no label')
    args = ap.parse_args()

    chosen = sorted(PLATFORMS) if args.platform == 'all' else [args.platform]
    if args.dat and len(chosen) != 1:
        sys.exit('--dat needs a single --platform')
    out = Path(args.out)
    match = re.compile(args.match, re.I) if args.match else None

    c = creds()
    check_systems(c, chosen)
    q = quota(c)
    left = None if q['per_day'] is None else max(0, q['per_day'] - q['today'])
    print(f"quota: {q['today']} used today of {q['per_day'] or '?'}, "
          f"{q['per_min'] or '?'} a minute, {q['threads']} thread(s)", flush=True)
    if left == 0:
        sys.exit('the day\'s quota is already spent; run again tomorrow and it will resume')
    threads = max(1, min(q['threads'] or 1, 6))
    pause = threads * (60.0 / q['per_min'] if q['per_min'] else 1.2)

    deadline = time.monotonic() + args.max_minutes * 60 if args.max_minutes else None
    urls_path = out / '.state' / 'urls.json'
    urls = load(urls_path, {})  # public media url -> stored file, shared by every dump
    lock = threading.Lock()
    stop = threading.Event()
    n = {'asked': 0, 'labelled': 0, 'none': 0, 'failed': 0, 'skipped': 0, 'downloads': 0}

    def media_file(m):
        key = public(m['url'])
        with lock:
            # Already stored: here, or in the published set this run started from.
            if key in urls:
                return urls[key]
        body = fetch(m['url'])
        rel = store(body, out, webp=not args.keep_png) if body else None
        with lock:
            n['downloads'] += 1
            if rel:
                urls[key] = rel
        return rel

    def work(platform, index, crc, name, size):
        with lock:
            if stop.is_set():
                return
            if ((left is not None and n['asked'] >= left) or (args.limit and n['asked'] >= args.limit)
                    or (deadline and time.monotonic() > deadline)):
                stop.set()
                return
            n['asked'] += 1
        params = dict(c, crc=crc, systemeid=SS_SYSTEM[platform], romtype='rom',
                      romnom=name + EXTENSION[platform])
        if size:
            params['romtaille'] = str(size)
        try:
            data = call('jeuInfos.php', params)
            medias = ((data or {}).get('response', {}).get('jeu') or {}).get('medias') or []
            entry = []
            for m in select(platform, name, medias):
                rel = media_file(m)
                if rel:
                    entry.append([m['type'], m['region'], rel])
        except (QuotaSpent, Refused) as e:
            print(f'\nstopped: {e}', flush=True)
            stop.set()
            return
        except Exception as e:  # one bad game must not take the run down with it
            with lock:
                n['failed'] += 1
            print(f'  {name}: {type(e).__name__}: {e}', flush=True)
            return
        with lock:
            index[crc] = entry
            n['labelled' if entry else 'none'] += 1
            done = n['labelled'] + n['none']
            if done % 50 == 0:
                print(f"  ... {done} done, {n['labelled']} with a label", flush=True)
                write_json(out / f'{platform}.json', index)
                write_json(urls_path, urls)
        time.sleep(pause)

    try:
        for platform in chosen:
            if stop.is_set():
                break
            entries = entries_for(platform, args.dat, hacks=not args.no_hacks)
            index_path = out / f'{platform}.json'
            index = load(index_path, {})
            jobs = []
            for crc, (name, size) in sorted(entries.items(), key=lambda kv: kv[1][0]):
                if match and not match.search(name):
                    continue
                if crc in index and (index[crc] or not args.retry_missing):
                    n['skipped'] += 1
                    continue
                jobs.append((crc, name, size))
            print(f'{platform}: {len(entries)} dumps in the dat, {len(jobs)} to ask', flush=True)
            try:
                with ThreadPoolExecutor(max_workers=threads) as pool:
                    for job in jobs:
                        if stop.is_set():
                            break
                        pool.submit(work, platform, index, *job)
            finally:
                write_json(index_path, index)
                # The names Cart Studio searches: every dump the set has art for.
                write_json(out / f'{platform}.names.json',
                           {crc: entries[crc][0] for crc in index if index[crc] and crc in entries})
    except KeyboardInterrupt:
        stop.set()
        print('\nstopped by hand')
    finally:
        write_json(urls_path, urls)
        files = list((out / 'files').rglob('*.*')) if (out / 'files').exists() else []
        files = [f for f in files if not f.name.endswith('.part')]
        total = sum(f.stat().st_size for f in files)
        print(f"\n{n['labelled']} dumps labelled, {n['none']} without a label, {n['failed']} failed, "
              f"{n['skipped']} already done; {n['asked']} lookups, {n['downloads']} downloads\n"
              f"set: {len(files)} files, {total / 1048576:.1f} MiB in {out}", flush=True)


if __name__ == '__main__':
    main()
