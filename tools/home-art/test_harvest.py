"""The stored set must answer every pick the app makes exactly as ScreenScraper's full list.

`app_pick` is a line-for-line port of HomeCartLabels.shell / labelRegions / pick (Kotlin);
the first tests pin it to HomeCartLabelsTest's own cases, so the port cannot drift unseen.

    python3 -m unittest tools/home-art/test_harvest.py
"""

import io
import unittest.mock
import random
import re
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import harvest  # noqa: E402

WINDOWS = {'nes', 'snes', 'snes_us', 'md', 'md_jp', 'md_ea'}
EUROPE = ['eu', 'fr', 'de', 'uk', 'sp', 'it', 'nl', 'se', 'au']


def name_regions(filename):
    j = u = e = False
    for m in re.finditer(r'[(\[]([^)\]]*)[)\]]', filename):
        for it in (p.strip().lower() for p in m.group(1).split(',')):
            if it in ('japan', 'j', 'jp', 'jpn', 'asia'):
                j = True
            elif it in ('usa', 'u', 'us', 'canada', 'world', 'w'):
                u = True
            elif it in ('europe', 'e', 'eu', 'pal', 'france', 'germany', 'spain', 'italy', 'uk',
                        'australia', 'sweden', 'netherlands', 'f', 'g', 'scandinavia', 'brazil'):
                e = True
    return (j, u, e) if (j or u or e) else None


def shell(platform, filename, region='', maker=''):
    regions = name_regions(filename)
    japan_only = (regions[0] and not regions[1] and not regions[2]) if regions else region == 'J'
    usa = regions[1] if regions else region == 'U'
    maker = maker.upper()
    if platform == 'NES':
        return 'fc' if japan_only else 'nes'
    if platform == 'SNES':
        return 'snes_us' if usa else 'snes'
    if platform == 'MD':
        if japan_only:
            return 'md_jp'
        if usa and ('T-50' in maker or 'ELECTRONIC ARTS' in maker):
            return 'md_ea'
        return 'md'
    return ''


def label_regions(platform, filename, region='', maker=''):
    regions = name_regions(filename)
    s = shell(platform, filename, region, maker)
    if s in ('fc', 'md_jp'):
        return ['jp']
    if s in ('snes_us', 'md_ea'):
        return ['us']
    if s == 'snes':
        if regions and regions[0] and not regions[2]:
            return ['jp'] + EUROPE + ['wor']
        return EUROPE + ['jp', 'wor']
    if regions and regions[1] and not regions[2]:
        return ['us', 'wor'] + EUROPE + ['ss']
    return EUROPE + ['us', 'wor', 'ss']


def app_pick(medias, platform, filename, region='', maker=''):
    """HomeCartLabels.medias + pick: only https urls count."""
    medias = [m for m in medias if str(m.get('url', '')).startswith('https://')]
    regions = label_regions(platform, filename, region, maker)

    def best(kind):
        for r in regions:
            for m in medias:
                if m.get('type') == kind and m.get('region') == r:
                    return m
        return None
    hit = best('support-texture')
    if hit:
        return hit
    if shell(platform, filename, region, maker) in WINDOWS:
        return best('support-2D')
    return None


class PortMatchesKotlinTests(unittest.TestCase):
    """HomeCartLabelsTest's cases (Rom filename is "$name.rom")."""

    def test_shells(self):
        self.assertEqual('fc', shell('NES', 'Super Mario Bros. 3 (Japan).rom'))
        self.assertEqual('nes', shell('NES', 'Super Mario Bros. (Japan, USA).rom'))
        self.assertEqual('snes_us', shell('SNES', 'Super Mario World (USA).rom'))
        self.assertEqual('snes', shell('SNES', 'Super Mario World (Japan).rom'))
        self.assertEqual('snes_us', shell('SNES', 'smw.rom', region='U'))
        self.assertEqual('md_jp', shell('MD', 'Sonic (Japan).rom'))
        self.assertEqual('md_ea', shell('MD', 'Road Rash (USA, Europe).rom', maker='(C)T-50 1991.SEP'))
        self.assertEqual('md', shell('MD', 'Road Rash (Europe).rom', maker='(C)T-50 1991.SEP'))

    def test_regions(self):
        self.assertEqual(['jp'], label_regions('NES', 'Dig Dug (Japan).rom'))
        self.assertEqual('us', label_regions('NES', 'Metroid (USA).rom')[0])
        self.assertEqual('eu', label_regions('NES', 'Metroid (Europe).rom')[0])
        self.assertEqual(['us'], label_regions('SNES', 'Super Metroid (USA).rom'))
        self.assertEqual('jp', label_regions('SNES', 'Super Metroid (Japan).rom')[0])

    def test_pick(self):
        medias = [
            {'type': 'support-2D', 'region': 'us', 'url': 'https://a/2d-us'},
            {'type': 'support-texture', 'region': 'jp', 'url': 'https://a/tex-jp'},
            {'type': 'support-texture', 'region': 'us', 'url': 'https://a/tex-us'},
            {'type': 'wheel', 'region': 'us', 'url': 'https://a/wheel'},
        ]
        self.assertEqual('https://a/tex-us', app_pick(medias, 'SNES', 'Game (USA).rom')['url'])
        no_scan = [m for m in medias if m['type'] != 'support-texture']
        self.assertEqual('https://a/2d-us', app_pick(no_scan, 'SNES', 'Game (USA).rom')['url'])
        self.assertIsNone(app_pick(no_scan, 'SNES', 'Game (Europe).rom'))


REGIONS = ['us', 'wor', 'ss', 'jp', 'cn', 'kr', 'br', ''] + EUROPE
KINDS = ['support-texture', 'support-2D', 'wheel', 'box-2D']
NAMES = [
    'Game (USA)', 'Game (Europe)', 'Game (Japan)', 'Game (Japan, USA)', 'Game (USA, Europe)',
    'Game (World)', 'Game (France)', 'Game (Germany)', 'Game (Japan, Europe)', 'Game (Korea)',
    'Game', 'Game (Asia)', 'Game (Brazil)', 'Game (Australia)', 'Game (Spain)',
]
SHELL_INPUTS = [('', ''), ('U', ''), ('J', ''), ('E', ''), ('U', '(C)T-50 1991.SEP'),
                ('', 'ELECTRONIC ARTS'), ('J', 'SEGA')]


class StoredSetIsEquivalentTests(unittest.TestCase):

    def test_every_pick_survives_selection(self):
        rng = random.Random(20261107)
        checked = 0
        for _ in range(4000):
            medias = []
            for i in range(rng.randint(0, 12)):
                medias.append({
                    'type': rng.choice(KINDS),
                    'region': rng.choice(REGIONS),
                    'url': ('https://ss/' if rng.random() > 0.05 else 'http://ss/') + str(i),
                })
            kept = harvest.home_labels(medias)
            for platform in ('NES', 'SNES', 'MD'):
                for name in NAMES:
                    for region, maker in SHELL_INPUTS:
                        filename = name + '.rom'
                        full = app_pick(medias, platform, filename, region, maker)
                        stored = app_pick(kept, platform, filename, region, maker)
                        self.assertIs(full, stored, (platform, filename, region, maker, medias))
                        checked += 1
        self.assertGreater(checked, 1_000_000)

    def test_selection_keeps_one_label_per_family_at_most(self):
        medias = [{'type': t, 'region': r, 'url': f'https://ss/{t}/{r}'}
                  for t in ('support-texture', 'support-2D') for r in REGIONS]
        kept = harvest.home_labels(medias)
        self.assertEqual(
            [('support-texture', 'us'), ('support-texture', 'wor'), ('support-texture', 'eu'),
             ('support-texture', 'jp'), ('support-texture', 'ss')],
            [(m['type'], m['region']) for m in kept])
        renders = [m for m in medias if m['type'] == 'support-2D' or m['region'] == 'fr']
        kept = harvest.home_labels(renders)
        self.assertIn(('support-texture', 'fr'), [(m['type'], m['region']) for m in kept])
        self.assertEqual(5, len(kept))


class HandheldAndLogoTests(unittest.TestCase):

    MEDIAS = [
        {'type': 'support-texture', 'region': 'us', 'url': 'https://ss/tex-us'},
        {'type': 'support-texture', 'region': 'eu', 'url': 'https://ss/tex-eu'},
        {'type': 'support-texture', 'region': 'jp', 'url': 'https://ss/tex-jp'},
        {'type': 'support-texture', 'region': 'fr', 'url': 'https://ss/tex-fr'},
        {'type': 'wheel', 'region': 'jp', 'url': 'https://ss/wheel-jp'},
        {'type': 'wheel', 'region': 'wor', 'url': 'https://ss/wheel-wor'},
        {'type': 'box-2D', 'region': 'us', 'url': 'https://ss/box'},
    ]

    def urls(self, kept):
        return [m['url'].rsplit('/', 1)[1] for m in kept]

    def test_a_game_boy_cart_keeps_its_own_regions_scan_and_a_logo(self):
        self.assertEqual(['tex-us', 'wheel-wor'], self.urls(harvest.select('GBA', 'Game (USA)', self.MEDIAS)))
        self.assertEqual(['tex-jp', 'wheel-jp'], self.urls(harvest.select('GB', 'Game (Japan)', self.MEDIAS)))
        self.assertEqual(['tex-fr', 'wheel-wor'], self.urls(harvest.select('GBC', 'Jeu (France)', self.MEDIAS)))
        self.assertEqual(['tex-eu', 'wheel-wor'], self.urls(harvest.select('GBC', 'Spiel (Germany)', self.MEDIAS)))
        self.assertEqual(['tex-us', 'wheel-wor'], self.urls(harvest.select('GB', 'Hack', self.MEDIAS)))

    def test_a_home_cart_keeps_every_shells_scan_and_a_logo(self):
        self.assertEqual(['tex-us', 'tex-eu', 'tex-jp', 'wheel-wor'],
                         self.urls(harvest.select('SNES', 'Game (USA)', self.MEDIAS)))

    def test_regions_follow_the_dump(self):
        self.assertEqual(('fr', 'eu', 'us', 'wor', 'jp'), harvest.own_regions('Jeu (France)'))
        self.assertEqual(('jp', 'us', 'wor', 'eu'), harvest.own_regions('Game (Japan, USA)'))
        self.assertEqual(('us', 'wor', 'eu', 'jp'), harvest.own_regions('Hack v1.2'))


class FilesTests(unittest.TestCase):

    def png(self, w, h):
        from PIL import Image
        out = io.BytesIO()
        Image.new('RGB', (w, h), (200, 30, 40)).save(out, 'PNG')
        return out.getvalue()

    def test_store_dedupes_and_keeps_bytes(self):
        body = self.png(300, 200)
        with tempfile.TemporaryDirectory() as d:
            a = harvest.store(body, Path(d), webp=False)
            b = harvest.store(body, Path(d), webp=False)
            self.assertEqual(a, b)
            self.assertTrue(a.endswith('.png') and a.startswith('files/'))
            self.assertEqual(body, (Path(d) / a).read_bytes())

    def test_store_the_same_image_from_many_threads(self):
        import threading
        body = self.png(300, 200)
        with tempfile.TemporaryDirectory() as d:
            got, errors = [], []
            def one():
                try:
                    got.append(harvest.store(body, Path(d), webp=False))
                except Exception as e:  # the race this guards against
                    errors.append(e)
            threads = [threading.Thread(target=one) for _ in range(8)]
            for t in threads: t.start()
            for t in threads: t.join()
            self.assertEqual(errors, [])
            self.assertEqual(len(set(got)), 1)
            self.assertEqual(list(Path(d).rglob('*.part')), [])

    def test_webp_keeps_every_pixel(self):
        from PIL import Image
        rng = random.Random(7)
        im = Image.new('RGBA', (120, 80))
        im.putdata([(rng.randrange(256), rng.randrange(256), rng.randrange(256),
                     rng.choice((0, 128, 255))) for _ in range(120 * 80)])
        flat = Image.new('RGB', (300, 120), (10, 200, 90))
        for source in (im, flat, flat.convert('P'), flat.convert('L')):
            out = io.BytesIO()
            source.save(out, 'PNG')
            with tempfile.TemporaryDirectory() as d:
                rel = harvest.store(out.getvalue(), Path(d))
                with Image.open(Path(d) / rel) as back:
                    self.assertEqual(source.convert('RGBA').tobytes(), back.convert('RGBA').tobytes())
                if source is flat:
                    self.assertTrue(rel.endswith('.webp'))

    def test_a_colour_profile_keeps_its_png(self):
        from PIL import Image
        out = io.BytesIO()
        Image.new('RGB', (300, 120), (10, 200, 90)).save(out, 'PNG', icc_profile=b'\0' * 200)
        with tempfile.TemporaryDirectory() as d:
            self.assertTrue(harvest.store(out.getvalue(), Path(d)).endswith('.png'))

    def test_store_refuses_what_the_app_refuses(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertIsNone(harvest.store(self.png(20, 300), Path(d)))
            self.assertIsNone(harvest.store(b'<html>quota</html>' * 40, Path(d)))
            self.assertIsNone(harvest.store(b'\x89PNG\r\n\x1a\n' + b'\0' * 400, Path(d)))

    def test_public_url_drops_credentials(self):
        url = ('https://neoclone.screenscraper.fr/api2/mediaJeu.php?devid=me&devpassword=pw'
               '&softname=slot&ssid=u&sspassword=p&systemeid=4&jeuid=1&media=support-texture(us)')
        clean = harvest.public(url)
        for secret in ('devid=', 'devpassword=', 'ssid=', 'sspassword='):
            self.assertNotIn(secret, clean)
        self.assertIn('media=support-texture%28us%29', clean)

    def test_dat_parsing(self):
        dat = '''clrmamepro (
\tname "Nintendo - Super Nintendo Entertainment System"
)

game (
\tname "Super Mario World (USA)"
\tdescription "Super Mario World (USA)"
\trom ( name "Super Mario World (USA).sfc" size 524288 crc b19ed489 md5 x sha1 y )
)

game (
\tname "Tetris (Japan)"
\trom ( name "a.sfc" size 1024 crc 0000abcd )
\trom ( name "b.sfc" size 2048 crc 0000ABCE )
)
'''
        got = harvest.parse_dat(dat)
        self.assertEqual(('Super Mario World (USA)', 524288), got['B19ED489'])
        self.assertEqual(('Tetris (Japan)', 1024), got['0000ABCD'])
        self.assertEqual(('Tetris (Japan)', 2048), got['0000ABCE'])

    def test_the_game_a_dump_comes_from(self):
        self.assertEqual(('alpha', ('USA',)), harvest.game_of('Alpha (USA) (Rev 1)'))
        self.assertEqual(harvest.game_of('Alpha (USA)'), harvest.game_of('Alpha (USA) (Beta 2) (1993-04-01)'))
        self.assertEqual(('alpha', ('Japan, USA', 'En,Ja')), harvest.game_of('Alpha (Japan, USA) (En,Ja) [b]'))
        self.assertNotEqual(harvest.game_of('Tetris (USA)'), harvest.game_of('Tetris (USA) (Unl)'))
        self.assertEqual(('alpha redux', ()), harvest.game_of('Alpha Redux [Hack by X]'))
        dat = 'game (\n\tname "Alpha Redux [Hack by X]"\n\trom ( name "Alpha (USA).sfc" size 1 crc 0000000A )\n)\n'
        self.assertEqual({'0000000A': 'Alpha (USA)'}, harvest.rom_stems(dat))

    def test_derive(self):
        entries = {'1': ('Alpha (USA)', 1), '2': ('Alpha (USA) (Rev 1)', 1),
                   '3': ('Alpha (USA) (Beta)', 1), '4': ('Alpha (Japan) (Proto)', 1),
                   '5': ('Alpha Redux [Hack by X]', 1), '6': ('Gamma (Europe)', 1),
                   '7': ('Delta (USA) (Demo)', 1)}
        index = {'1': [], '2': [['support-texture', 'us', 'f/2']], '3': [], '4': [], '5': [], '6': [],
                 '7': [['support-texture', 'us', 'f/7']]}
        stems = {'5': 'Alpha (USA)'}
        # Alpha (USA) has none of its own: its revision lends it one; the beta too. The hack
        # names Alpha (USA), which has none, so it is matched by its own name: nothing.
        self.assertEqual({'1': '2', '3': '2'}, harvest.derive(index, entries, stems))
        index['1'] = [['support-texture', 'us', 'f/1']]
        self.assertEqual({'3': '1', '5': '1'}, harvest.derive(index, entries, stems))


class RunTests(unittest.TestCase):
    """A whole run against a stand-in ScreenScraper: index, shared files, quota, resume."""

    DAT = '''game (
\tname "Alpha (USA)"
\trom ( name "Alpha (USA).sfc" size 1024 crc 00000001 )
)
game (
\tname "Alpha (USA) (Rev 1)"
\trom ( name "Alpha (USA) (Rev 1).sfc" size 1024 crc 00000002 )
)
game (
\tname "Beta (Japan)"
\trom ( name "Beta (Japan).sfc" size 1024 crc 00000003 )
)
game (
\tname "Gamma (Europe)"
\trom ( name "Gamma (Europe).sfc" size 1024 crc 00000004 )
)
'''

    def setUp(self):
        self.png = FilesTests.png(self, 400, 300)
        self.lookups = []
        self.downloads = []
        self.quota_after = None
        self.missing = {'00000004'}
        game = {'medias': [
            {'type': 'support-texture', 'region': 'us', 'url': 'https://ss/m?devid=x&media=tex-us'},
            {'type': 'support-texture', 'region': 'jp', 'url': 'https://ss/m?devid=x&media=tex-jp'},
            {'type': 'wheel', 'region': 'us', 'url': 'https://ss/m?devid=x&media=wheel'},
        ]}

        def call(endpoint, params, tries=4):
            if endpoint == 'systemesListe.php':
                return {'response': {'systemes': [{'id': 4, 'noms': {'nom_eu': 'Super Nintendo'}}]}}
            if endpoint == 'ssuserInfos.php':
                return {'response': {'ssuser': {'maxrequestspermin': '6000', 'maxrequestsperday': '100',
                                                'requeststoday': '0', 'maxthreads': '1'}}}
            self.lookups.append(params['crc'])
            if self.quota_after is not None and len(self.lookups) > self.quota_after:
                raise harvest.QuotaSpent('spent')
            return None if params['crc'] in self.missing else {'response': {'jeu': game}}

        def fetch(url, tries=3):
            self.downloads.append(url)
            return self.png

        self.patches = [(harvest, 'call', call), (harvest, 'fetch', fetch)]
        self.saved = [(o, a, getattr(o, a)) for o, a, _ in self.patches]
        for o, a, v in self.patches:
            setattr(o, a, v)
        self.env = {k: harvest.os.environ.get(k) for k in ('SS_DEVID', 'SS_DEVPASSWORD')}
        harvest.os.environ.update(SS_DEVID='id', SS_DEVPASSWORD='pw')

    def tearDown(self):
        for o, a, v in self.saved:
            setattr(o, a, v)
        for k, v in self.env.items():
            if v is None:
                harvest.os.environ.pop(k, None)
            else:
                harvest.os.environ[k] = v

    def run_harvest(self, out, dat, *extra):
        argv = sys.argv
        sys.argv = ['harvest.py', '--platform', 'SNES', '--out', str(out), '--dat', str(dat), *extra]
        try:
            with unittest.mock.patch('sys.stdout', new=io.StringIO()):
                harvest.main()
        finally:
            sys.argv = argv

    def test_run_resume_and_shared_files(self):
        import json
        with tempfile.TemporaryDirectory() as d:
            out, dat = Path(d) / 'set', Path(d) / 'snes.dat'
            dat.write_text(self.DAT)
            self.quota_after = 2
            self.run_harvest(out, dat)
            index = json.loads((out / 'SNES.json').read_text())
            self.assertEqual({'00000001', '00000002'}, set(index))
            # Two scans and a logo per Alpha dump, downloaded once for both dumps.
            self.assertEqual(3, len(self.downloads))
            self.assertEqual(index['00000001'], index['00000002'])
            self.assertEqual([['support-texture', 'us'], ['support-texture', 'jp'], ['wheel', 'us']],
                             [e[:2] for e in index['00000001']])
            files = {e[2] for e in index['00000001']}
            self.assertEqual(1, len(files))  # identical bytes, one file
            self.assertTrue((out / files.pop()).is_file())
            self.assertNotIn('devid', (out / '.state' / 'urls.json').read_text())

            self.quota_after = None
            self.lookups.clear()
            self.run_harvest(out, dat)
            self.assertEqual(['00000003', '00000004'], self.lookups)  # resumes, skips the done
            index = json.loads((out / 'SNES.json').read_text())
            self.assertEqual([], index['00000004'])
            self.assertEqual(3, len(self.downloads))  # Beta's media were already stored

            names = json.loads((out / 'SNES.names.json').read_text())
            self.assertEqual({'00000001': 'Alpha (USA)', '00000002': 'Alpha (USA) (Rev 1)',
                              '00000003': 'Beta (Japan)'}, names, 'named: every dump with art')

            self.lookups.clear()
            self.run_harvest(out, dat)
            self.assertEqual([], self.lookups)
            self.run_harvest(out, dat, '--retry-missing')
            self.assertEqual(['00000004'], self.lookups)

    def test_a_dump_without_a_label_wears_its_games(self):
        import json
        dat_text = self.DAT + '''game (
\tname "Alpha (USA) (Beta)"
\trom ( name "Alpha (USA) (Beta).sfc" size 1024 crc 00000005 )
)
game (
\tname "Alpha (Japan) (Proto)"
\trom ( name "Alpha (Japan) (Proto).sfc" size 1024 crc 00000006 )
)
game (
\tname "Alpha Redux [Hack by X]"
\trom ( name "Alpha (USA).sfc" size 1024 crc 00000007 )
)
'''
        self.missing = {'00000004', '00000005', '00000006', '00000007'}
        with tempfile.TemporaryDirectory() as d:
            out, dat = Path(d) / 'set', Path(d) / 'snes.dat'
            dat.write_text(dat_text)
            self.run_harvest(out, dat)
            index = json.loads((out / 'SNES.json').read_text())
            self.assertEqual(index['00000001'], index['00000005'])
            self.assertEqual(index['00000001'], index['00000007'])
            self.assertEqual([], index['00000006'])  # no Japanese Alpha to borrow from
            self.assertEqual([], index['00000004'])
            self.assertEqual({'00000005': '00000001', '00000007': '00000001'},
                             json.loads((out / '.state' / 'SNES.derived.json').read_text()))
            names = json.loads((out / 'SNES.names.json').read_text())
            self.assertIn('00000005', names)

            # Borrowed labels don't count as answers: asked again, and ScreenScraper's own wins.
            self.lookups.clear()
            self.missing = {'00000004', '00000006', '00000007'}
            self.run_harvest(out, dat, '--retry-missing')
            self.assertEqual(['00000006', '00000005', '00000007', '00000004'], self.lookups)
            self.assertEqual({'00000007': '00000001'},
                             json.loads((out / '.state' / 'SNES.derived.json').read_text()))

            # --retry-after: only the dumps last asked about long enough ago.
            asked_path = out / '.state' / 'SNES.asked.json'
            asked = json.loads(asked_path.read_text())
            self.assertEqual({'00000004', '00000006', '00000007'}, set(asked))
            asked['00000004'] = '2000-01-01'
            asked_path.write_text(json.dumps(asked))
            self.lookups.clear()
            self.run_harvest(out, dat, '--retry-after', '30')
            self.assertEqual(['00000004'], self.lookups)
            self.lookups.clear()
            self.run_harvest(out, dat, '--retry-after', '30')
            self.assertEqual([], self.lookups)


class VideoTests(unittest.TestCase):
    """Game Boy Advance Video carts, which no dat libretro reads for No-Intro names."""

    SERIAL = '''game (
\tcomment "Game Boy Advance Video - Shrek (USA)"
\tserial "AGB-MSKE-USA"
\trom (
\t\tcrc 0000000c
\t)
)

game (
\tcomment "Famicom Mini 01 - Super Mario Bros. (Japan)"
\tserial "AGB-FSMJ-JPN"
\trom (
\t\tcrc 0000000D
\t)
)
'''

    def test_the_serial_dat_names_the_video_carts_only(self):
        self.assertEqual({'0000000C': ('Game Boy Advance Video - Shrek (USA)', 0)},
                         harvest.serial_videos(self.SERIAL))

    def test_a_search_hit_is_named_after_its_dump(self):
        jeu = {'id': '5', 'noms': [{'region': 'us', 'text': 'GBA Video : Pokemon Vol. 1'}],
               'roms': [{'romcrc': 'abcdef01', 'romsize': '33554432',
                         'romfilename': 'Game Boy Advance Video - Pokemon - Volume 1 (USA).gba'},
                        {'romcrc': 'ABCDEF02', 'romsize': '', 'romfilename': 'pokevid.gba'},
                        {'romcrc': 'nothex!!', 'romfilename': 'x.gba'}]}
        self.assertEqual({'ABCDEF01': ('Game Boy Advance Video - Pokemon - Volume 1 (USA)', 33554432),
                          'ABCDEF02': ('Game Boy Advance Video - GBA Video : Pokemon Vol. 1', 0)},
                         harvest.video_roms(jeu))
        self.assertEqual({}, harvest.video_roms({'noms': [{'text': 'Pokemon Emerald'}],
                                                 'roms': [{'romcrc': '00000001', 'romfilename': 'p.gba'}]}))

    def test_homebrew_dats_are_read_with_the_hacks(self):
        asked = []

        def read_dat(platform, local=None, base=harvest.LIBRETRO):
            asked.append(base)
            return ''
        with unittest.mock.patch.object(harvest, 'read_dat', read_dat):
            harvest.dat_texts('GBA')
            self.assertEqual([harvest.LIBRETRO, harvest.LIBRETRO_HACKS, harvest.LIBRETRO_HOMEBREW], asked)
            asked.clear()
            harvest.dat_texts('GBA', hacks=False)
            self.assertEqual([harvest.LIBRETRO], asked)

    def test_a_run_finds_the_video_carts_once_a_month(self):
        import json
        searches, infos, lookups = [], [], []
        game = {'medias': [{'type': 'support-texture', 'region': 'us', 'url': 'https://ss/m?media=tex-us'}]}

        def call(endpoint, params, tries=4):
            if endpoint == 'systemesListe.php':
                return {'response': {'systemes': [{'id': 12, 'noms': {'nom_eu': 'Game Boy Advance'}}]}}
            if endpoint == 'jeuRecherche.php':
                searches.append(params['recherche'])
                if params['recherche'] != 'Cartoon Network':
                    return {'response': {'jeux': [{}]}}
                return {'response': {'jeux': [
                    {'id': '7', 'noms': [{'region': 'us',
                                          'text': 'Game Boy Advance Video - Cartoon Network Collection Vol. 1'}]},
                    {'id': '8', 'noms': [{'region': 'us', 'text': 'Cartoon Network Speedway'}]}]}}
            if endpoint == 'jeuInfos.php' and 'gameid' in params:
                infos.append(params['gameid'])
                return {'response': {'jeu': {'id': '7', 'roms': [
                    {'romcrc': '0000000A', 'romsize': '16777216',
                     'romfilename': 'Game Boy Advance Video - Cartoon Network Collection - Volume 1 (USA).gba'}]}}}
            lookups.append(params['crc'])
            return {'response': {'jeu': game}}

        png = FilesTests.png(self, 400, 300)
        with tempfile.TemporaryDirectory() as d, \
                unittest.mock.patch.object(harvest, 'call', call), \
                unittest.mock.patch.object(harvest, 'fetch', lambda url, tries=3: png), \
                unittest.mock.patch.dict(harvest.os.environ, SS_DEVID='id', SS_DEVPASSWORD='pw'):
            out, dat, serial = Path(d) / 'set', Path(d) / 'gba.dat', Path(d) / 'serial.dat'
            dat.write_text('game (\n\tname "Alpha (USA)"\n\trom ( name "Alpha (USA).gba" size 1024 crc 00000001 )\n)\n')
            serial.write_text(self.SERIAL)

            def run():
                argv = sys.argv
                sys.argv = ['harvest.py', '--platform', 'GBA', '--out', str(out), '--dat', str(dat),
                            '--video-dat', str(serial)]
                try:
                    with unittest.mock.patch('sys.stdout', new=io.StringIO()):
                        harvest.main()
                finally:
                    sys.argv = argv

            run()
            self.assertEqual(list(harvest.VIDEO_SEARCH), searches)
            self.assertEqual(['7'], infos)  # the speedway game isn't a video cart
            self.assertEqual(['00000001', '0000000A', '0000000C'], sorted(lookups))
            names = json.loads((out / 'GBA.names.json').read_text())
            self.assertEqual('Game Boy Advance Video - Cartoon Network Collection - Volume 1 (USA)',
                             names['0000000A'])
            self.assertEqual('Game Boy Advance Video - Shrek (USA)', names['0000000C'])

            searches.clear()
            run()
            self.assertEqual([], searches)  # kept for a month
            state = out / '.state' / 'GBA.video.json'
            kept = json.loads(state.read_text())
            kept['when'] = '2000-01-01'
            state.write_text(json.dumps(kept))
            run()
            self.assertEqual(list(harvest.VIDEO_SEARCH), searches)


if __name__ == '__main__':
    unittest.main()
