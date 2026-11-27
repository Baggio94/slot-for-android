#!/usr/bin/env python3
"""Per-game cart shells for PC Engine / SuperGrafx and Neo Geo: every game's
ScreenScraper support-2D (front render of the cart or HuCard), its outline's
aspect, the shell's colour round its edge and a small thumbnail, so the
exceptions to each system's default shell can be read off a contact sheet.

usage: scrape_ng.py OUT_DIR [KEY ...]   (ScreenScraper credentials as for home-art)"""
import io, json, re, sys, time, urllib.parse, urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'home-art'))
import harvest as H                     # creds, call, fetch, parse_dat, quota
from PIL import Image

NOINTRO = H.LIBRETRO
SYSTEMS = {
    # key: (ScreenScraper id, words its name must hold, game list)
    'PCE': (31, ('pc engine', 'turbografx'), ('nointro', 'NEC - PC Engine - TurboGrafx 16', '.pce')),
    'SGX': (105, ('supergrafx',), ('nointro', 'NEC - PC Engine SuperGrafx', '.sgx')),
    'NEOGEO': (142, ('neo-geo', 'neo geo', 'neogeo'), ('fbneo', None, '.zip')),
}
FBNEO_NEOGEO = ('https://raw.githubusercontent.com/libretro/FBNeo/master/dats/'
                + urllib.parse.quote('FinalBurn Neo (ClrMame Pro XML, Neogeo only).dat'))


def get(url):
    with urllib.request.urlopen(urllib.request.Request(url, headers={'User-Agent': 'slot-android'}), timeout=120) as r:
        return r.read().decode('utf-8', 'replace')


def games(key):
    kind, dat, ext = SYSTEMS[key][2]
    if kind == 'nointro':
        out = []
        for crc, (name, size) in H.parse_dat(get(NOINTRO + urllib.parse.quote(dat) + '.dat')).items():
            out.append(dict(crc=crc, romnom=name + ext, romtaille=str(size) if size else None, name=name))
        return out
    text = get(FBNEO_NEOGEO)
    out = []
    for m in re.finditer(r'<game name="([^"]+)"([^>]*)>(.*?)</game>', text, re.S):
        set_name, attrs, body = m.groups()
        if 'isbios="yes"' in attrs or set_name == 'neogeo':
            continue
        desc = re.search(r'<description>(.*?)</description>', body, re.S)
        clone = re.search(r'cloneof="([^"]+)"', attrs)
        out.append(dict(crc=None, romnom=set_name + ext, romtaille=None, name=(desc.group(1) if desc else set_name),
                        parent=clone.group(1) if clone else None))
    return out


def shell_stats(im):
    """Outline bbox aspect (h/w) and the mean colour of a thin band just inside the outline."""
    im = im.convert('RGBA')
    a = im.getchannel('A')
    box = a.point(lambda v: 255 if v > 127 else 0).getbbox() or (0, 0, *im.size)
    x0, y0, x1, y1 = box
    w, h = x1 - x0, y1 - y0
    px = im.load()
    band = max(2, round(min(w, h) * .03))
    acc, n = [0, 0, 0], 0
    for y in range(y0, y1, max(1, h // 120)):
        row = [x for x in range(x0, x1) if px[x, y][3] > 200]
        if not row:
            continue
        for x in (row[0] + band, row[-1] - band):
            r, g, b, al = px[x, y]
            if al > 200:
                acc[0] += r; acc[1] += g; acc[2] += b; n += 1
    rgb = [round(v / n) for v in acc] if n else None
    alpha_frac = a.point(lambda v: 255 if v > 127 else 0).histogram()[255] / (im.size[0] * im.size[1])
    return dict(w=im.size[0], h=im.size[1], aspect=round(h / w, 3) if w else None,
                edge=rgb, fill=round(alpha_frac, 3))


def main(out, keys):
    out = Path(out); (out / 'thumbs').mkdir(parents=True, exist_ok=True)
    c = H.creds()
    listed = (H.call('systemesListe.php', c) or {}).get('response', {}).get('systemes', [])
    names = {int(s['id']): ' | '.join(str(v).lower() for v in (s.get('noms') or {}).values()) for s in listed if str(s.get('id', '')).isdigit()}
    seen_game, rows = set(), []
    for key in keys:
        sid, words, _ = SYSTEMS[key]
        got = names.get(sid, '')
        print(f'{key}: ScreenScraper {sid} = {got}', flush=True)
        if not any(w in got for w in words):
            sys.exit(f'{key}: system id {sid} is not it')
        try:
            todo = games(key)
        except Exception as e:
            print(f'{key}: no game list ({e})', flush=True); continue
        print(f'{key}: {len(todo)} dumps', flush=True)
        for i, g in enumerate(todo):
            params = dict(c, systemeid=sid, romtype='rom', romnom=g['romnom'])
            if g['crc']:
                params['crc'] = g['crc']
            if g['romtaille']:
                params['romtaille'] = g['romtaille']
            try:
                data = H.call('jeuInfos.php', params)
            except (H.QuotaSpent, H.Refused) as e:
                print(f'stopped: {e}', flush=True); break
            except Exception as e:
                print(f'  {g["name"]}: {type(e).__name__}', flush=True); continue
            jeu = (data or {}).get('response', {}).get('jeu') or {}
            gid = jeu.get('id')
            row = dict(sys=key, name=g['name'], romnom=g['romnom'], parent=g.get('parent'), gameid=gid, renders=[])
            if gid and (key, gid) in seen_game:
                row['dup'] = True
                rows.append(row); continue
            seen_game.add((key, gid))
            for m in jeu.get('medias') or []:
                if m.get('type') not in ('support-2D', 'support-texture') or not H.https(m):
                    continue
                try:
                    body = H.fetch(m['url'])
                    im = Image.open(io.BytesIO(body)) if body else None
                except Exception:
                    im = None
                if im is None:
                    continue
                st = shell_stats(im) if m['type'] == 'support-2D' else dict(w=im.size[0], h=im.size[1])
                thumb = f"{key}-{gid}-{m['type']}-{m.get('region', 'xx')}.webp"
                t = im.convert('RGBA'); t.thumbnail((200, 200)); t.save(out / 'thumbs' / thumb, quality=80)
                row['renders'].append(dict(type=m['type'], region=m.get('region'), thumb=thumb, **st))
            rows.append(row)
            if i % 50 == 0:
                print(f'  {key} {i}/{len(todo)}', flush=True)
                (out / 'results.jsonl').write_text('\n'.join(json.dumps(r, ensure_ascii=False) for r in rows) + '\n')
    (out / 'results.jsonl').write_text('\n'.join(json.dumps(r, ensure_ascii=False) for r in rows) + '\n')
    print('rows', len(rows), flush=True)


if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2:] or list(SYSTEMS))
