<p align="center"><img src=".github/resources/slot-for-android-hero.png" alt="slot. for Android on an Anbernic RG Rotate, a KONKR Pocket Advance, a Retroid Pocket Classic and a KONKR Pocket Block" width="100%"></p>

# slot. for Android

**A cartridge-first playshell for Android handhelds.**

An unofficial Android port of [slot.](https://slot-cfw.fyi/#hero). Your games appear as cartridges on a shelf: pick one, insert it and play.

**[Download the latest version](https://github.com/Baggio94/slot-for-android/releases/latest)** · **[Read the Wiki](https://github.com/Baggio94/slot-for-android/wiki)**

## Why I made this port

When I watched [Russ from Retro Game Corps showcase slot.](https://www.youtube.com/watch?v=vndbnpH0QoE&t=1167s), I thought it would be perfect for the KONKR Pocket Advance, a handheld I mostly use for Game Boy Advance games.

The round button on top of the KONKR fits slot. naturally: press it to insert a cartridge, hold it while playing to eject it. It feels close to handling a real cartridge.

The port has since grown to other Android handhelds and more systems. The goal stays the same as the original: less time in menus, more time playing.

## Highlights

- **Cartridges first.** Your library is a shelf of cartridges with their real labels, insertion and ejection animations and sounds.
- **13 systems.** Game Boy, Game Boy Color, Game Boy Advance, NES, Super Nintendo, Mega Drive, Master System, Game Gear, Neo Geo Pocket/Color, WonderSwan/Color, Atari Lynx, Pokémon mini and PICO-8, with multiple emulation cores where available.
- **Any screen shape.** The interface adapts to square, 4:3, 3:2 and widescreen handhelds at their native resolution.
- **Cart Studio.** Change a cartridge's label, shell colour, finish and shape, right on the handheld.
- **Themes.** Dark, OLED, Light and Original, with a wallpaper for each shelf.
- **Screen options.** LCD and CRT shaders, overlays, colour correction, Frame Blending and LCD effects, per system or per game.
- **Your slot. on every handheld.** The slot. folder carries your saves, states, labels, favorites and settings from one handheld to another, on an SD card or in a synced folder.
- **RetroArch & Syncthing compatibility.** Use your existing RetroArch save and save state folders to keep your progress in sync with slot.
- **RetroAchievements.** Online or offline, with achievements synced when you reconnect.
- **Multiplayer.** Link Cable, Wireless Adapter and two-player netplay, on the same Wi-Fi or over the Internet.
- **RomM.** Download games and supported BIOS files, and sync saves and save states with your own RomM server.
- **Cheats.** RetroArch's cheat database is built in, with support for your own custom cheats.

## Getting Started

1. Download the APK from [Releases](https://github.com/Baggio94/slot-for-android/releases/latest) and install it on your Android handheld (Android 12 or later).
2. Open slot., press **START → Library → Choose ROM Folder**, and select your games folder.
3. Pick a cartridge, insert it, and play!

For supported formats, BIOS files, controls, updates, and optional features, see the [Getting Started guide](https://github.com/Baggio94/slot-for-android/wiki/Getting-Started).

## Essential Controls

The KONKR's round top button is the **slot. cap**. On a Retroid Pocket, it's the round centre key. On the Anbernic RG Rotate, it's the middle button.

| Button | Action |
|---|---|
| slot. cap or **A** | Insert a cartridge |
| Hold the slot. cap or **A** (on the shelf) | Start the game fresh, without its save state |
| Tap the slot. cap or **SELECT + X** | Open the game menu |
| Hold the slot. cap or **SELECT + START** | Save and eject |
| **SELECT + Y** | Screen menu |
| **START** (on the shelf) | Main menu |
| **SELECT** (on the shelf) | Choose the core for a game |
| **B** (on the shelf) | Open Shelves, to jump to another system |

On a touchscreen, swipe through your cartridges, tap or pull down the middle cart to insert it, push it up to select a core, or hold a cartridge to open Cart Studio.

All shortcuts can be customized in **START → Settings → Controls → Hotkeys**.

## Documentation

The [slot. for Android Wiki](https://github.com/Baggio94/slot-for-android/wiki) covers everything from installation to advanced emulation settings.

**Getting started:** [Installation & Setup](https://github.com/Baggio94/slot-for-android/wiki/Getting-Started) · [Controls & Hotkeys](https://github.com/Baggio94/slot-for-android/wiki/Controls-and-Hotkeys) · [Cores & Saves](https://github.com/Baggio94/slot-for-android/wiki/Cores-and-Saves)

**Customization:** [Settings](https://github.com/Baggio94/slot-for-android/wiki/Settings) · [Screen, Shaders & Effects](https://github.com/Baggio94/slot-for-android/wiki/Screen-Shaders-and-Effects) · [Cart Studio](https://github.com/Baggio94/slot-for-android/wiki/Cart-Studio)

**Features:** [RomM](https://github.com/Baggio94/slot-for-android/wiki/RomM) · [slot. Folder](https://github.com/Baggio94/slot-for-android/wiki/slot.-Folder) · [RetroAchievements](https://github.com/Baggio94/slot-for-android/wiki/RetroAchievements) · [Multiplayer](https://github.com/Baggio94/slot-for-android/wiki/Multiplayer)

**Handhelds:** [KONKR Pocket Advance](https://github.com/Baggio94/slot-for-android/wiki/KONKR-Pocket-Advance) · [Retroid Pocket Classic](https://github.com/Baggio94/slot-for-android/wiki/Retroid-Pocket-Classic) · [RG Rotate](https://github.com/Baggio94/slot-for-android/wiki/RG-Rotate)

## Credits & License

Thanks to **[Brandon Kowalski](https://github.com/BrandonKowalski/slot)**, creator of the original [slot.](https://slot-cfw.fyi/), **[Markus](https://github.com/MarkusMaribu)** for its artwork, and to the authors of the emulation cores, shaders, overlays and tools slot. for Android uses. The full list is on the [Credits](https://github.com/Baggio94/slot-for-android/wiki/Credits) page.

PICO-8 games are played by fake-08 by jtothebell - https://github.com/jtothebell/fake-08 (MIT, see [its license](licenses/fake-08-MIT.txt)). PICO-8 is a product of Lexaloffle Games; slot. includes none of its files.

The PokéDaisy core is based on PokeDaisy by Lidor Itzhari - https://github.com/lidor30/pokedaisy (GNU GPL-3.0, see [its notice](licenses/pokedaisy-GPL-3.0-NOTICE.txt)).

This is an unofficial community port, not affiliated with or endorsed by the original Slot developers or any handheld manufacturer. The Slot-derived source code is distributed under the [GNU GPL-3.0](LICENSE). Third-party components keep their own licenses, listed in [licenses](licenses).
