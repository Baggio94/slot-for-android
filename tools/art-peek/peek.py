"""Copy a few published labels (by dat name regex) out of the bucket listing for a look."""
import json, re, sys, shutil
from pathlib import Path
art, out, pattern = Path(sys.argv[1]), Path(sys.argv[2]), re.compile(sys.argv[3], re.I)
platform = sys.argv[4]
index = json.loads((art / f'{platform}.json').read_text())
names = json.loads((art / f'{platform}.names.json').read_text())
want = {crc: names[crc] for crc in names if pattern.search(names[crc])}
print(json.dumps({crc: [names[crc], index.get(crc)] for crc in want}, indent=1))
(out / 'files').mkdir(parents=True, exist_ok=True)
(out / 'peek.json').write_text(json.dumps({crc: [names[crc], index.get(crc)] for crc in want}, indent=1))
for crc in want:
    for _, _, f in index.get(crc) or []:
        print(f)
