# Cart art

slot. for Android's own cart label set, for every system it plays: Game Boy, Game Boy Color,
Game Boy Advance, NES, Super Nintendo and Mega Drive. It is built ahead of time from
ScreenScraper, the way slot.'s art set (art.slot-cfw.fyi) is, and published to an R2 bucket.

- `harvest.py` asks ScreenScraper about every dump in libretro's No-Intro and rom hack dats,
  keeps the label scans and front renders the app can pick (pixel for pixel) and one clear logo,
  and writes one index per platform (`GB.json` ... `MD.json`), mapping CRC32 to
  `[type, region, file]`. Files live under
  `files/`, named by SHA-1, so revisions of one game share them. It paces itself to the account's
  quota and resumes where it stopped.
  A dump ScreenScraper has no label for wears the label of the game it comes from: a revision,
  beta, proto, demo or alternate dump that of the same game in the same region, a rom hack that
  of the dump its dat names (`.state/<PLATFORM>.derived.json` lists them; they are asked again
  like any dump without a label).
- `test_harvest.py` proves that the app's `HomeCartLabels.pick` answers the same over the stored
  set as over ScreenScraper's full list, so the shelf shows exactly the labels it shows today.
- `preview.py` writes a contact sheet, `preview.html`, of a set.

```sh
SS_DEVID=... SS_DEVPASSWORD=... SS_SSID=... SS_SSPASSWORD=... \
    tools/home-art/harvest.py --platform SNES --out ../slot-art --limit 40
python3 -m unittest tools/home-art/test_harvest.py
```

The **Cart art** workflow runs it once a week with the repository's ScreenScraper secrets,
starting from the published indexes, asks again about a dump without a label a month after the
last answer (`--retry-after 30`), and publishes new files and indexes to the R2 bucket
(`R2_ACCOUNT_ID`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`, `R2_BUCKET`). A push to
feature/home-art runs a short trial kept as an artifact.

Images come from [ScreenScraper](https://www.screenscraper.fr/) (CC BY-NC-SA 4.0); game names
and CRCs from [libretro-database](https://github.com/libretro/libretro-database) (CC BY-SA 4.0).
A published set carries both attributions.
