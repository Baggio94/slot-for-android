"""Probes a few named games per system: which support media ScreenScraper has for them, and
what each image's plastic measures. Tells real photos (the shell changes from game to game)
from templates (every game measures the same) before any full pass is spent on a system."""
import json
import re
import sys
import time
import urllib.parse
from pathlib import Path

import scrape

SYSTEMS = {"NES": 3, "SNES": 4, "MD": 1, **{p: s for p, (_, s) in scrape.SYSTEMS.items()}}
DATS = {
    "NES": "Nintendo - Nintendo Entertainment System.dat",
    "SNES": "Nintendo - Super Nintendo Entertainment System.dat",
    "MD": "Sega - Mega Drive - Genesis.dat",
    **{p: d for p, (d, _) in scrape.SYSTEMS.items()},
}
KINDS = ("support-2D", "support-scan")


def main():
    plan = json.loads(Path(sys.argv[1]).read_text())
    cache = Path("cache/probe")
    cache.mkdir(parents=True, exist_ok=True)
    for platform, pattern in plan.items():
        status, body = scrape.get(scrape.DAT_BASE + urllib.parse.quote(DATS[platform]))
        if status != 200:
            print(f"{platform}: DAT HTTP {status}")
            continue
        roms = [r for r in scrape.parse_dat(body.decode("utf-8", "replace"))
                if re.search(pattern, r["name"]) and not scrape.SKIP.search(r["name"])]
        print(f"{platform}: {len(roms)} ROMs match")
        for rom in roms:
            status, body = scrape.jeu_infos(SYSTEMS[platform], rom)
            time.sleep(1.2)
            if status in scrape.STOP:
                print("stopped:", status)
                return
            try:
                medias = json.loads(body)["response"]["jeu"].get("medias") or []
            except (ValueError, KeyError, TypeError):
                print("PROBE", json.dumps({"p": platform, "n": rom["name"], "e": status}))
                continue
            kinds = sorted({m.get("type") for m in medias if str(m.get("type", "")).startswith("support")})
            for kind in KINDS:
                media = scrape.pick_support(medias, scrape.regions_of(rom["name"]), kind)
                if not media:
                    continue
                key = media.get("md5") or media.get("crc") or f"{rom['crc']}-{kind}"
                img = cache / f"{key}.{media.get('format', 'png')}"
                if not img.exists():
                    st, data = scrape.get(media["url"], timeout=120)
                    time.sleep(1.2)
                    if st == 200 and data[:4] != b"NOME":
                        img.write_bytes(data)
                m = scrape.measure(img) if img.exists() else None
                print("PROBE", json.dumps({
                    "p": platform, "n": rom["name"], "kind": kind, "r": media.get("region"),
                    "kinds": kinds, "m": m,
                }, ensure_ascii=False, separators=(",", ":")))


if __name__ == "__main__":
    main()
