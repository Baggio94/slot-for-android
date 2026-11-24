import importlib.util
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location('manual', Path(__file__).with_name('import_manual.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class TestManual(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.out = Path(self.tmp.name)
        for p in m.PLATFORMS:
            m.write_map(self.out / f'{p}.json', {'00000001': [['wheel','wor','files/aa/other.png']]})
            m.write_map(self.out / f'{p}.names.json', {'00000001': 'Existing'})

    def test_add_idempotence_and_restore_names(self):
        count = len(m.read_map(self.out / 'GBC.json'))
        self.assertEqual(count, 1)
        self.assertEqual(m.merge(self.out)['added'], 10)
        self.assertEqual(m.merge(self.out)['unchanged'], 10)
        self.assertEqual(len(list((self.out/'files').rglob('*.png'))), 10)
        p = self.out/'GBC.names.json'
        data = m.read_map(p); del data['A1378C6B']; m.write_map(p, data)
        m.merge(self.out)
        self.assertEqual(m.read_map(p)['A1378C6B'], 'ModRetro Tetris')
        self.assertEqual(m.read_map(p)['00000001'], 'Existing')

    def test_conflict_preserved(self):
        p = self.out/'GBC.json'; data = m.read_map(p)
        old = [['support-texture','us','files/aa/other.png']]
        data['A1378C6B'] = old; m.write_map(p,data)
        self.assertEqual(m.merge(self.out)['conflicts'], 1)
        self.assertEqual(m.read_map(p)['A1378C6B'], old)
        self.assertFalse((self.out/'files').exists())

    def test_derived_replaced(self):
        p = self.out/'GBC.json'; data = m.read_map(p)
        data['A1378C6B'] = [['support-texture','us','files/aa/borrowed.png']]; m.write_map(p,data)
        state = self.out/'.state/GBC.derived.json'
        m.write_map(state, {'A1378C6B':'00000001'})
        self.assertEqual(m.merge(self.out)['added'], 10)
        self.assertNotIn('A1378C6B',m.read_map(state))

    def test_dry_run(self):
        self.assertEqual(m.merge(self.out,dry_run=True)['added'],10)
        self.assertFalse((self.out/'files').exists())

    def test_missing_indexes_refused(self):
        (self.out/'GB.json').unlink()
        with self.assertRaises(FileNotFoundError): m.merge(self.out)

    def test_other_platform_conflict(self):
        p=self.out/'GB.json'; data=m.read_map(p); data['A1378C6B']=[]; m.write_map(p,data)
        self.assertEqual(m.merge(self.out)['conflicts'],1)
        self.assertNotIn('A1378C6B',m.read_map(self.out/'GBC.json'))

if __name__ == '__main__': unittest.main()
