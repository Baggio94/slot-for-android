# slot. for Android — v2.2.0

This update brings a native Cart Studio built into slot. for Android, labels from slot.'s own database, cheats and new shaders.

### What's New

- **Native Cart Studio** — Cart Studio is now built directly into slot. for Android. Press **X** on a cartridge to change its label (Real Label, Logo Only, another game's label, or your own picture), shell colour, finish and shape. **START → Cart Studio** shows every cartridge of a system: **L1 / R1** change the system, **L2 / R2** jump between letters, **Y** saves all changes. It replaces the web Cart Studio.
- **Our own label database** — Labels now come from slot. for Android's own database, art.slot-android.app, for every system, rom hacks included. ScreenScraper only fills in what it doesn't have yet.
- **Cheats** — RetroArch's cheat database is built in for the Game Boy, Game Boy Color, Game Boy Advance, NES, Super Nintendo and Mega Drive. Open **Cheats** in the game menu; slot. remembers them for each game.
- **LCD Effect** — Matt Akins' Pixel Transparency and Sunlight Shimmer on the Game Boy, Game Boy Color and Game Boy Advance.
- **New shaders** — Dot Matrix on the Game Boy. crt-easymode, crt-hyllian, SABR, Super Eagle and xBR-lv2 on the NES, Super Nintendo and Mega Drive.
- **Dither Blending** — Mega Drive dithering blends as it did on a TV.
- **Status Bar** — Clock and battery at the top or bottom, Wi-Fi and Bluetooth in one row, apps All, Running or Off, and a time format. New installs show the clock and battery at the top.
- **Screenshot hotkey** — Set it in **Settings → Controls → Hotkeys**. Screenshots go to Pictures/Screenshots. ([#86](https://github.com/Baggio94/slot-for-android/issues/86))
- **Scan progress** — The first library scan shows a progress bar and a count; Refresh Library shows its progress top right. ([#102](https://github.com/Baggio94/slot-for-android/issues/102))
- **Interface** — Settings → General is now Interface. With the Shelf wallpaper, choose which shelf each wallpaper goes to.
- **Console slots** — The NES, Famicom, Super Nintendo and Mega Drive slots fit their cartridges.

### Fixed

- Pokémon: Save and Eject no longer fails after loading a save state made before the first in-game save. ([#103](https://github.com/Baggio94/slot-for-android/issues/103))
- An older auto-state no longer resumes over a newer in-game save.
- A clear cartridge's lid no longer lifts with the board drawn on it.
- Simpletex shows its paper texture again.

### Installation

Install the APK below over your existing version: your games, saves and settings are kept. On 2.0 and 2.1, slot. also offers this update from **START → About**. On 1.3, install this APK by hand once.

### Credits

An unofficial Android port of [slot.](https://slot-cfw.fyi/) by [Brandon Kowalski](https://github.com/BrandonKowalski/slot) and contributors. Overlays by [Watomsk](https://github.com/Watomsk/Overlays). Pixel Transparency and Sunlight Shimmer by [Matt Akins](https://github.com/mattakins/Pixel_Transparency) and GDAPT by Sp00kyFox, used with their authors' permission. Cheats from [libretro-database](https://github.com/libretro/libretro-database) (CC BY-SA 4.0). Emulation by mGBA, gpSP, Gambatte, FCEUmm, Nestopia, Snes9x, bsnes, Genesis Plus GX and PicoDrive. Labels from ScreenScraper.

Licensed under GPL-3.0.
