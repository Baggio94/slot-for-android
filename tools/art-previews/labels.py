"""Fetches real labels for the 3.0 cartridge previews: slot.'s own art set for the six
current systems, ScreenScraper for the new ones. One request at a time. Writes the images
and an index (labels.json) to --out; nothing is published to the art set."""
import argparse, io, json, re, sys, time, urllib.parse
from pathlib import Path
from PIL import Image
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "cart-colours"))
from scrape import get, env, parse_dat, DAT_BASE, SKIP

DATS = {
    "GB": "Nintendo - Game Boy.dat", "GBC": "Nintendo - Game Boy Color.dat",
    "GBA": "Nintendo - Game Boy Advance.dat",
    "NES": "Nintendo - Nintendo Entertainment System.dat",
    "SNES": "Nintendo - Super Nintendo Entertainment System.dat",
    "MD": "Sega - Mega Drive - Genesis.dat",
    "NGP": "SNK - Neo Geo Pocket.dat", "NGPC": "SNK - Neo Geo Pocket Color.dat",
    "WS": "Bandai - WonderSwan.dat", "WSC": "Bandai - WonderSwan Color.dat",
    "GG": "Sega - Game Gear.dat", "SMS": "Sega - Master System - Mark III.dat",
    "LYNX": "Atari - Lynx.dat", "PMINI": "Nintendo - Pokemon Mini.dat",
}
SS_ID = {"NGP": 25, "NGPC": 82, "WS": 45, "WSC": 46, "GG": 21, "SMS": 2, "LYNX": 28, "PMINI": 211}
COUNTRY = {"USA": "us", "World": "wor", "Europe": "eu", "Japan": "jp"}
ART = "https://art.slot-android.app/"
KINDS = ("support-texture", "support-2D")


def regions(name):
    m = re.search(r"\(([^)]*)\)", name)
    own = [COUNTRY[p.strip()] for p in (m.group(1).split(",") if m else []) if p.strip() in COUNTRY]
    return list(dict.fromkeys(own + ["us", "wor", "eu", "jp", "ss"]))


def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": "slot-android"})
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return r.status, r.read()
    except urllib.error.HTTPError as e:
        return e.code, b""
    except OSError as e:
        return 0, str(e).encode()


import urllib.request, urllib.error


def save(out, key, body):
    im = Image.open(io.BytesIO(body))
    im = im.convert("RGBA")
    im.thumbnail((900, 900))
    path = out / f"{key}.png"
    im.save(path, optimize=True)
    return path.name, im.size


def ss_medias(sid, rom):
    q = {"devid": env("SS_DEVID"), "devpassword": env("SS_DEVPASSWORD"),
         "softname": env("SS_SOFTNAME"), "output": "json", "systemeid": sid,
         "romtype": "rom", "crc": rom["crc"], "romtaille": rom["size"], "romnom": rom["name"]}
    if env("SS_USER"):
        q["ssid"], q["sspassword"] = env("SS_USER"), env("SS_PASSWORD")
    code, body = get("https://api.screenscraper.fr/api2/jeuInfos.php?" + urllib.parse.urlencode(q))
    time.sleep(1.2)
    if code != 200:
        print("SS FAIL", rom["name"], code, body[:120])
        return []
    jeu = json.loads(body)["response"]["jeu"]
    print("SS", rom["name"], "->", (jeu.get("noms") or [{}])[0].get("text"), "| system", jeu.get("systeme", {}).get("text"))
    return [(m["type"], m.get("region", ""), m["url"]) for m in jeu.get("medias", []) if m.get("url")]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="out")
    a = ap.parse_args()
    out = Path(a.out); out.mkdir(parents=True, exist_ok=True)
    dats, indexes, result = {}, {}, []
    for line in Path(__file__).with_name("labels.txt").read_text().splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        platform, pattern, source = line.split("\t")
        if platform not in dats:
            code, body = fetch(DAT_BASE + urllib.parse.quote(DATS[platform]))
            dats[platform] = list(parse_dat(body.decode("utf-8", "replace"))) if code == 200 else []
            print("DAT", platform, code, len(dats[platform]))
        rx = re.compile(pattern, re.I)
        rom = next((r for r in dats[platform] if rx.search(r["name"]) and not SKIP.search(r["name"])), None)
        if not rom:
            print("NO DUMP", platform, pattern)
            continue
        if source == "own":
            if platform not in indexes:
                code, body = fetch(ART + platform + ".json")
                indexes[platform] = json.loads(body) if code == 200 else {}
                print("INDEX", platform, code, len(indexes[platform]))
            medias = [(t, r, ART + f) for t, r, f in indexes[platform].get(rom["crc"], [])]
        else:
            medias = ss_medias(SS_ID[platform], rom)
        order = regions(rom["name"])
        row = {"platform": platform, "name": rom["name"], "crc": rom["crc"], "source": source, "files": {}}
        for kind in KINDS:
            hit = next((m for reg in order for m in medias if m[0] == kind and m[1] == reg), None)
            if not hit:
                continue
            code, body = fetch(hit[2])
            if code != 200:
                print("GET FAIL", kind, hit[1], code)
                continue
            key = f"{platform}-{rom['crc']}-{kind}-{hit[1]}"
            name, size = save(out, key, body)
            row["files"][kind] = {"file": name, "region": hit[1], "size": size}
        print("OK" if row["files"] else "NONE", platform, rom["name"], {k: v["region"] for k, v in row["files"].items()})
        result.append(row)
    (out / "labels.json").write_text(json.dumps(result, indent=1, ensure_ascii=False))


main()
