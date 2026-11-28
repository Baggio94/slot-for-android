import io
import json
import tempfile
import unittest
from pathlib import Path

from PIL import Image

import fix_labels
import import_hacks

HERE = Path(__file__).parent


class HackLabelTests(unittest.TestCase):
    def setUp(self):
        manifest = json.loads((HERE / 'hack-labels' / 'manifest.json').read_text(encoding='utf-8'))
        self.titles = {h['key']: h['title'] for h in manifest['hacks']}

    def test_keys_are_their_own_words(self):
        for key in self.titles:
            self.assertEqual(key.split(), import_hacks.words(key), key)

    def test_file_names_find_their_hack(self):
        cases = {
            'Pokemon - Radical Red (v4.1).gba': 'radical red',
            'Pokémon Sors 2.gba': 'sors 2',
            'Pokemon Sors.gba': 'sors',
            'Too Many Types 2 v1.0 (Pokemon).gba': 'too many types 2',
            'Pokemon Fire Red Rocket Edition.gba': 'firered rocket edition',
            'Pokemon FireRed & LeafGreen+.gba': 'firered and leafgreen plus',
            'Pokemon R.O.W.E. v1.2.gba': 'rowe',
            "Pokemon Let's Go Pikachu.gba": 'lets go pikachu',
            'Super Mariomon.gba': 'mariomon',
            'Pokemon - FireRed Version (USA).gba': None,
            'Peach Adventure.gba': None,  # a Pokémon hack's key needs the word Pokémon
        }
        for name, key in cases.items():
            self.assertEqual(key, import_hacks.key_of(name, self.titles), name)

    def test_labels_are_trimmed_and_scaled(self):
        im = Image.new('RGBA', (1400, 800), (0, 0, 0, 0))
        im.paste((200, 30, 30, 255), (100, 50, 1300, 650))
        buf = io.BytesIO()
        im.save(buf, 'PNG')
        with Image.open(io.BytesIO(import_hacks.prepare(buf.getvalue()))) as out:
            self.assertEqual((960, 480), out.size)
            self.assertEqual('RGB', out.mode)


class LabelFixTests(unittest.TestCase):
    def test_fix_replaces_every_entry_naming_the_old_file(self):
        fixes = json.loads((HERE / 'label-fixes' / 'manifest.json').read_text(encoding='utf-8'))
        old = fixes[0]['replaces']
        with tempfile.TemporaryDirectory() as out:
            out = Path(out)
            (out / 'GBA.json').write_text(json.dumps({'31554C22': [['support-texture', 'us', old],
                                                                   ['wheel', 'wor', 'files/x.webp']]}))
            (out / 'GBA.names.json').write_text(json.dumps({'31554C22': old}))
            self.assertEqual(1, fix_labels.apply(out))
            index = json.loads((out / 'GBA.json').read_text())
            new = index['31554C22'][0][2]
            self.assertNotEqual(old, new)
            self.assertTrue((out / new).exists())
            self.assertEqual(old, json.loads((out / 'GBA.names.json').read_text())['31554C22'])


if __name__ == '__main__':
    unittest.main()
