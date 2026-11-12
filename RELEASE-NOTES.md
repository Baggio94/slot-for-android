# slot. for Android — v2.3.0

This update connects slot. for Android to your own RomM server, gives Game Boy Advance Video cartridges their own shelf, and makes motion games work on handhelds without a gyroscope.

### What's New

- **RomM** — If you run your own [RomM](https://github.com/rommapp/romm) server, a self-hosted library for your games, slot. can now download games from it. Open **START → Library → RomM**, enter your server's address and sign in. Choose the games you want: they download into your ROM folder and appear on their shelf with their label.
- **Game Boy Advance Video** — GBA Video cartridges have their own shelf: on the GBA shelf, tap **L2** or **R2** to switch to it and back. They play on mGBA, with an **Upscaler** in the Screen menu (FSR by default, Anime4K Fast, Anime4K).
- **Delete a game from the carousel** — Hold **L2 + R2** for 1.5 seconds to delete the selected game. Its saves and save states are kept.
- **Motion sensitivity** — Games you play by tilting or turning the handheld (WarioWare: Twisted!, Yoshi Topsy-Turvy, Koro Koro Puzzle and Kirby Tilt 'n' Tumble) now have a **Sensitivity** setting from 1 to 10, saved for each game. Change it in the in-game menu, **Settings → Emulation**, or while playing with **R3 / L3** or **SELECT + R2 / L2**.
- **crt-royale** — A new CRT shader for the NES, Super Nintendo and Mega Drive.
- **Fast Forward MAX** — **Settings → Gameplay → Fast Forward** has a new MAX setting: uncapped speed.
- **Lighter shaders** — LCD Effect, Dot Matrix and the CRT shaders use less of the GPU, with the same look.
- **Mega Drive cartridges** — Accolade, Codemasters, Virtua Racing and Sonic & Knuckles shells, and EA's cartridge for every EA game.
- **Shader order** — Each system lists its shaders in a clearer order.
- **Status Bar** — Icons line up with the clock and battery, and turn black on light wallpapers.

### Fixed

- WarioWare: Twisted! now works on handhelds without a gyroscope, such as the KONKR Pocket Advance: turn the handheld and slot. detects the rotation with the accelerometer. Handhelds with a gyroscope still use it. ([#106](https://github.com/Baggio94/slot-for-android/issues/106))
- RetroAchievements: the status says what slot. is doing (Caching games, Caching badges, Retrying…), and a badge missing on the server no longer leaves it on "Needs attention". ([#105](https://github.com/Baggio94/slot-for-android/issues/105))
- On near-square screens, the NES shows in 8:7 like the Super Nintendo, instead of a taller picture.

### Installation

Install the APK below over your existing version: your games, saves and settings are kept. On 2.0 and later, slot. also offers this update from **START → About**. On 1.3, install this APK by hand once.

### Credits

An unofficial Android port of [slot.](https://slot-cfw.fyi/) by [Brandon Kowalski](https://github.com/BrandonKowalski/slot) and contributors. Overlays by [Watomsk](https://github.com/Watomsk/Overlays). Pixel Transparency and Sunlight Shimmer by [Matt Akins](https://github.com/mattakins/Pixel_Transparency) and GDAPT by Sp00kyFox, used with their authors' permission. crt-royale by TroggleMonkey. Anime4K by [bloc97](https://github.com/bloc97/Anime4K), FSR by AMD. Cheats from [libretro-database](https://github.com/libretro/libretro-database) (CC BY-SA 4.0). Emulation by mGBA, gpSP, Gambatte, FCEUmm, Nestopia, Snes9x, bsnes, Genesis Plus GX and PicoDrive. Labels from ScreenScraper. Works with [RomM](https://github.com/rommapp/romm).

Licensed under GPL-3.0.
