# slot. for Android — v2.1.1

This update introduces customizable controller mappings and hotkeys, along with online multiplayer. The repository has also been renamed to slot-for-android.

### Fixed in 2.1.1

- **Game Boy and Game Boy Color labels** — slot.'s art set now serves its labels as WebP. slot. fetches them in that format, so they download again and Cart Studio's Real Label shows them.
- **A, Up and Down always keep a key** — Settings → Controls no longer lets you clear them, since slot.'s menus need them. If you cleared one in 2.1.0, updating gives it back its key.
- **Saves, states and Polaroids follow your folder** — Choosing a save or save state folder now copies them there in RetroArch's layout, Polaroids included (up to 10 per game, RetroArch's manual slots). If the folder already has different ones, you choose which copy it keeps; the other is kept beside it.
- **Custom logo with a picture** — In Cart Studio, a PNG without transparency fills the whole label instead of sitting small with wide borders.

### What's New

- **Multiplayer over the Internet (beta)** — Next to LAN, play over the Internet (WAN) with a four-digit room code: Host shows the code, Y shares it, your friend types it with the D-pad. No router setup. Every Link mode works except Gambatte's Game Boy link, which stays on LAN for now.
- **Settings → Controls** — Choose the controller of Port 1 and Port 2 and remap its buttons. With a second controller, two players share one handheld on NES, Super Nintendo and Mega Drive.
- **Hotkeys** — Change every shortcut, and choose SELECT, L3 or R3 as the Hotkey Button
- **Controls per system and per game** 
- **Screen menu** — SELECT + Y opens it straight from the game. The game stays in view while you change its shader, color correction, palette and overlay, and L1 / R1 change the screen size.
- **Frame Blending** — Off, Mix or Ghosting for each system, for games that flicker their sprites to draw shadows or transparency.

### Fixed

- Grid draws its lines as the original slot., even at any scale.
- LCD Grid V2 blends its subpixels as in RetroArch.
- AGB-001 and AGS-001 keep even subpixel columns at any scale.
- Settings → Systems pages show their system's name.
- No more "LABELS" flash after ejecting a cartridge.
- Multiplayer keycaps fit 4:3 and narrower screens.

### Installation

Install the APK below over your existing version: your games, saves and settings are kept. On 2.0 and 2.1, slot. also offers this update from **START → About**. On 1.3, install this APK by hand once; updates from **START → About** work again from there.

### Credits

An unofficial Android port of [slot.](https://slot-cfw.fyi/) by [Brandon Kowalski](https://github.com/BrandonKowalski/slot) and contributors. Overlays by [Watomsk](https://github.com/Watomsk/Overlays). Emulation by mGBA, gpSP, Gambatte, FCEUmm, Nestopia, Snes9x, bsnes, Genesis Plus GX and PicoDrive. Labels from ScreenScraper.

Licensed under GPL-3.0.
