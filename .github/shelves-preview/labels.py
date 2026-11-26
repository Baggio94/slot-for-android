"""Real labels from slot.'s art set for the shelves page preview: three
well-known games per system, as <SYSTEM>__<title>.png."""
import io, json, re, sys, urllib.request
from pathlib import Path
from PIL import Image

BASE = 'https://art.slot-android.app/'
out = Path(sys.argv[1]); out.mkdir(parents=True, exist_ok=True)
WANT = {
    'GBA': (['GBA'], ['Pokemon - Emerald', 'Metroid - Zero Mission', 'Mario Kart - Super Circuit', 'Golden Sun']),
    'GB': (['GB'], ['Tetris', 'Super Mario Land', 'Pokemon - Red', 'Kirby']),
    'GBC': (['GBC'], ['Pokemon - Crystal', 'Legend of Zelda, The - Link', 'Wario Land 3', 'Pokemon - Gold']),
    'NES': (['NES'], ['Super Mario Bros. 3', 'Legend of Zelda, The', 'Mega Man 2', 'Metroid']),
    'SNES': (['SNES'], ['Super Metroid', 'Super Mario World', 'Chrono Trigger', 'Donkey Kong Country']),
    'MD': (['MD'], ['Sonic The Hedgehog', 'Streets of Rage 2', 'Gunstar Heroes', 'Sonic The Hedgehog 2']),
    'SMS': (['SMS'], ['Alex Kidd in Miracle World', 'Sonic The Hedgehog', 'Wonder Boy III', 'Phantasy Star']),
    'GG': (['GG'], ['Sonic The Hedgehog', 'Columns', 'Shinobi', 'Sonic Chaos']),
    'NGP': (['NGPC', 'NGP'], ['Metal Slug', 'SNK vs. Capcom', 'Sonic the Hedgehog', 'King of Fighters']),
    'WS': (['WSC', 'WS'], ['Final Fantasy', 'Gunpey', 'Klonoa', 'Makaimura']),
    'LYNX': (['LYNX'], ['California Games', "Chip's Challenge", 'Blue Lightning', 'Gates of Zendocon']),
    'POKEMINI': (['POKEMINI'], ['Pokemon Pinball Mini', 'Pokemon Party Mini', 'Pokemon Puzzle Collection', 'Pokemon Tetris']),
}

def get(path):
    req = urllib.request.Request(BASE + path, headers={'User-Agent': 'slot. for Android'})
    with urllib.request.urlopen(req, timeout=30) as r:
        return r.read()

for system, (keys, titles) in WANT.items():
    got = 0
    for key in keys:
        try:
            names = json.loads(get(f'{key}.names.json'))
            index = json.loads(get(f'{key}.json'))
        except Exception as e:
            print(f'{key}: {e}'); continue
        by_title = sorted(names.items(), key=lambda kv: (not re.search(r'\((USA|World|Europe)', kv[1]), kv[1]))
        for want in titles:
            if got >= 3: break
            for crc, name in by_title:
                if not name.lower().startswith(want.lower()) or 'Beta' in name or 'Proto' in name:
                    continue
                entries = index.get(crc) or []
                pick = next((e for e in entries if e[0] == 'support-texture'), None) or \
                       next((e for e in entries if e[0] == 'support-2D'), None)
                if not pick or pick[0] != 'support-texture':
                    continue
                title = re.sub(r'\s*\(.*$', '', name).strip()
                try:
                    im = Image.open(io.BytesIO(get(pick[2]))).convert('RGBA')
                    im.thumbnail((960, 960))
                    im.save(out / f'{system}__{title}.png')
                    got += 1
                    print(system, title, im.size)
                except Exception as e:
                    print(f'{system} {name}: {e}')
                break
    print(f'{system}: {got} labels')
