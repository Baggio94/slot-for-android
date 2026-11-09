#!/usr/bin/env python3
"""Pack libretro-database's cheat files into slot.'s cheat packs.

    python3 tools/cheats/pack.py <libretro-database checkout> <out dir>

The checkout needs the folders cht/ and metadat/no-intro/ (pinned commit:
see COMMIT below). For each system it writes <out>/<system>.cht.z, a zlib
stream of UTF-8 lines:

    slot-cheats 1
    R <crc32> <file>[,<file>...]      ROM checksum (No-Intro) -> cheat files
    F <name>                          a cheat file (No-Intro style name)
    C <description>\t<code>           one of its cheats

Files ending in "(Rumbles)" and RetroArch memory cheats (handler 1, no
code) are left out: they are not codes a core can apply.
The cheats are libretro-database's, CC BY-SA 4.0.
"""
import os, re, sys, zlib

COMMIT = "c4e17d41ec1bb744fb064f7cefeb61b878fc5d64"
SYSTEMS = {
    "gb": "Nintendo - Game Boy",
    "gbc": "Nintendo - Game Boy Color",
    "gba": "Nintendo - Game Boy Advance",
    "nes": "Nintendo - Nintendo Entertainment System",
    "snes": "Nintendo - Super Nintendo Entertainment System",
    "md": "Sega - Mega Drive - Genesis",
}
# Tags naming the cheat device, after the game's own No-Intro name.
DEVICES = ("Action Replay", "Pro Action Replay", "Game Genie", "GameShark",
           "Code Breaker", "Xploder", "Game Genie 2", "diff")

LINE = re.compile(r'^\s*(cheat(\d+)_(\w+))\s*=\s*(.*?)\s*$')


def unquote(value):
    value = value.strip()
    if len(value) >= 2 and value[0] == '"' and value[-1] == '"':
        value = value[1:-1]
    return value


def read_cht(path):
    entries = {}
    with open(path, encoding="utf-8", errors="replace") as f:
        for raw in f:
            m = LINE.match(raw)
            if not m:
                continue
            entries.setdefault(int(m.group(2)), {})[m.group(3)] = unquote(m.group(4))
    cheats = []
    for index in sorted(entries):
        e = entries[index]
        code = clean(e.get("code", ""))
        desc = clean(e.get("desc", "")) or f"Cheat {index + 1}"
        if not code or e.get("handler", "0") not in ("", "0"):
            continue
        cheats.append((desc, code))
    return cheats


def clean(text):
    return " ".join(text.replace("\t", " ").split())


FORBIDDEN = re.compile(r'[&*/:`<>?\\|"]')


def title(name):
    """The bare title, for a game whose region or revision has no file."""
    name = FORBIDDEN.sub("_", name)
    name = re.sub(r"\s*[\(\[][^\)\]]*[\)\]]", "", name)
    return re.sub(r"[^0-9a-z]+", " ", name.lower()).strip()


def strip_devices(stem):
    """The game's own name: the cheat device tags taken off the end."""
    while True:
        for tag in DEVICES:
            suffix = f" ({tag})"
            if stem.endswith(suffix):
                stem = stem[: -len(suffix)]
                break
        else:
            return stem


def read_dat(path):
    """(game name, [crc]) from a clrmamepro No-Intro DAT."""
    games, name, crcs = [], None, []
    game_re = re.compile(r'^\s*name "(.*)"\s*$')
    rom_re = re.compile(r'\bcrc ([0-9A-Fa-f]{8})\b')
    with open(path, encoding="utf-8", errors="replace") as f:
        for raw in f:
            if raw.startswith("game ("):
                name, crcs = None, []
            elif name is None and (m := game_re.match(raw)):
                name = m.group(1)
            elif raw.lstrip().startswith("rom (") and (m := rom_re.search(raw)):
                crcs.append(m.group(1).lower())
            elif raw.startswith(")") and name:
                games.append((name, crcs))
                name = None
    return games


def pack(db, out, system, folder):
    cht_dir = os.path.join(db, "cht", folder)
    files = []
    for entry in sorted(os.listdir(cht_dir)):
        if not entry.endswith(".cht") or entry.endswith("(Rumbles).cht"):
            continue
        cheats = read_cht(os.path.join(cht_dir, entry))
        if cheats:
            files.append((entry[:-4], cheats))
    by_game, by_title = {}, {}
    for i, (stem, _) in enumerate(files):
        by_game.setdefault(strip_devices(stem), []).append(i)
        by_title.setdefault(title(stem), []).append(i)
    lines = ["slot-cheats 1"]
    matched = set()
    for name, crcs in read_dat(os.path.join(db, "metadat", "no-intro", folder + ".dat")):
        # RetroArch's file names: the No-Intro name with & and the
        # characters a file name can't hold as "_".
        hits = by_game.get(FORBIDDEN.sub("_", name)) or by_title.get(title(name))
        if not hits:
            continue
        matched.update(hits)
        for crc in sorted(set(crcs)):
            lines.append(f"R {crc} {','.join(map(str, hits))}")
    for stem, cheats in files:
        lines.append(f"F {stem}")
        lines.extend(f"C {desc}\t{code}" for desc, code in cheats)
    data = zlib.compress(("\n".join(lines) + "\n").encode("utf-8"), 9)
    with open(os.path.join(out, f"{system}.cht.z"), "wb") as f:
        f.write(data)
    count = sum(len(c) for _, c in files)
    print(f"{system}: {len(files)} files, {count} cheats, {len(matched)} files by checksum, "
          f"{len(data) // 1024} KB")


def main():
    db, out = sys.argv[1], sys.argv[2]
    os.makedirs(out, exist_ok=True)
    for system, folder in SYSTEMS.items():
        pack(db, out, system, folder)


if __name__ == "__main__":
    main()
