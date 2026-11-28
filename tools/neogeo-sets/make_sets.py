#!/usr/bin/env python3
"""Neo Geo sets slot. knows, from FinalBurn Neo's own Neo Geo dat at the
pinned FBNeo commit (scripts/build-home-core-android.sh):

    git show <pin>:"dats/FinalBurn Neo (ClrMame Pro XML, Neogeo only).dat" > ng.dat
    python3 tools/neogeo-sets/make_sets.py ng.dat > android/app/src/main/assets/neogeo-sets.tsv

One line per game: set name, tab, parent set (empty for a parent), tab,
the dat's title. The BIOS set is left out. The app reads it to know a
Neo Geo set in an arcade folder and to show its title."""
import sys
import xml.etree.ElementTree as ET

root = ET.parse(sys.argv[1]).getroot()
for game in root.iter('game'):
    if game.get('isbios') == 'yes':
        continue
    name = game.get('name', '').strip().lower()
    title = ' '.join((game.findtext('description') or '').split())
    if not name or not title or '\t' in title:
        continue
    print(f"{name}\t{(game.get('cloneof') or '').lower()}\t{title}")
