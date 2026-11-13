<p align="center"><img src=".github/resources/slot-for-android-hero.png" alt="slot. for Android on a KONKR Pocket Block, a KONKR Pocket Advance and a Retroid Pocket Classic" width="100%"></p>

# slot. for Android

An unofficial Android port of [slot.](https://slot-cfw.fyi/#hero). Your games appear as cartridges on a shelf: pick one, insert it and play.

**[Download the latest version](https://github.com/Baggio94/slot-for-android/releases/latest)** · **[Read the guide](https://github.com/Baggio94/slot-for-android/wiki)**

## Why I made this port

When I watched [Russ from Retro Game Corps showcase slot.](https://www.youtube.com/watch?v=vndbnpH0QoE&t=1167s), I thought it would be perfect for the KONKR Pocket Advance, a handheld I mostly use for Game Boy Advance games.

The round button on top of the KONKR fits slot. naturally: press it to insert a cartridge, hold it while playing to eject it. It feels close to handling a real cartridge.

The port has since grown to other Android handhelds and home consoles. The goal stays the same as the original: less time in menus, more time playing.

## Highlights

- **Cartridges first.** Your library is a shelf of cartridges with their real labels, insertion and ejection animations and sounds.
- **Six systems.** Game Boy, Game Boy Color, Game Boy Advance, NES, Super Nintendo and Mega Drive, each with two emulation cores to choose from.
- **Any screen shape.** The interface adapts to square, 4:3, 3:2 and widescreen handhelds at their native resolution.
- **Cart Studio.** Change a cartridge's label, shell colour, finish and shape, right on the handheld.
- **Screen options.** LCD and CRT shaders, overlays, color correction, Frame Blending and LCD effects, per system or per game.
- **Saves.** Save states, automatic saving on eject, rewind and fast-forward. Compatible with RetroArch saves and Syncthing.
- **RetroAchievements.** Online or offline, with unlocks synced when you reconnect.
- **Multiplayer.** Link Cable, Wireless Adapter and two-player netplay, on the same Wi-Fi or over the Internet.
- **RomM.** Download games from your own RomM server.
- **Cheats.** RetroArch's cheat database is included for every system.

## Getting started

1. Download the APK from [Releases](https://github.com/Baggio94/slot-for-android/releases/latest) and install it. It needs an ARM64 handheld with Android 12 or later.
2. Open slot., press **START → Library → Choose ROM Folder** and pick your parent ROM folder (not `gba`, `gb`...).
3. Pick a cartridge and press **A**.

Supported files: `.gba`, `.gb`, `.gbc`, `.nes`, `.fc`, `.unf`, `.unif`, `.sfc`, `.smc`, `.md`, `.gen`, `.smd`, `.bin`, and `.zip` archives with one game each. Games can sit directly in that folder or in one subfolder per system (ES-DE style).

**Updating:** slot. checks for updates in **START → About**. You can also install a newer APK over the current one. Don't uninstall slot. or clear its data first, or you will lose your settings and the saves stored inside the app.

More in [Getting Started](https://github.com/Baggio94/slot-for-android/wiki/Getting-Started).

## Essential controls

The KONKR's round top button is the **slot. cap**. On a Retroid Pocket, it's the round centre key.

| Button | Action |
|---|---|
| slot. cap or **A** | Insert a cartridge |
| Tap the slot. cap or **SELECT + X** | Open the game menu |
| Hold the slot. cap or **SELECT + START** | Save and eject |
| **SELECT + Y** | Screen menu (shader, overlay, screen size) |
| **START** (on the shelf) | Main menu |
| **SELECT** (on the shelf) | Choose the core for a game |

Every shortcut can be changed in **START → Settings → Controls → Hotkeys**. The full list is in [Controls & Hotkeys](https://github.com/Baggio94/slot-for-android/wiki/Controls-and-Hotkeys).

## Guide

The [wiki](https://github.com/Baggio94/slot-for-android/wiki) explains every screen and setting:

- [Getting Started](https://github.com/Baggio94/slot-for-android/wiki/Getting-Started) · [Controls & Hotkeys](https://github.com/Baggio94/slot-for-android/wiki/Controls-and-Hotkeys) · [Shelves](https://github.com/Baggio94/slot-for-android/wiki/Shelves) · [Game Menu](https://github.com/Baggio94/slot-for-android/wiki/Game-Menu)
- [Settings](https://github.com/Baggio94/slot-for-android/wiki/Settings) · [Screen, Shaders & Effects](https://github.com/Baggio94/slot-for-android/wiki/Screen-Shaders-and-Effects) · [Cores & Saves](https://github.com/Baggio94/slot-for-android/wiki/Cores-and-Saves)
- [Cart Studio](https://github.com/Baggio94/slot-for-android/wiki/Cart-Studio) · [RomM](https://github.com/Baggio94/slot-for-android/wiki/RomM) · [RetroAchievements](https://github.com/Baggio94/slot-for-android/wiki/RetroAchievements) · [Multiplayer](https://github.com/Baggio94/slot-for-android/wiki/Multiplayer) · [Cheats](https://github.com/Baggio94/slot-for-android/wiki/Cheats)
- [Special Games](https://github.com/Baggio94/slot-for-android/wiki/Special-Games) (Boktai, motion games) · [Apps & Home Launcher](https://github.com/Baggio94/slot-for-android/wiki/Apps-and-Home-Launcher)

## Credits & License

Thanks to **[Brandon Kowalski](https://github.com/BrandonKowalski/slot)**, creator of the original [slot.](https://slot-cfw.fyi/), **[Markus](https://github.com/MarkusMaribu)** for its artwork, and to the authors of the emulation cores, shaders, overlays and tools slot. for Android uses. The full list is on the [Credits](https://github.com/Baggio94/slot-for-android/wiki/Credits) page.

This is an unofficial community port, not affiliated with or endorsed by the original Slot developers or any handheld manufacturer. The Slot-derived source code is distributed under the [GNU GPL-3.0](LICENSE). Third-party components keep their own licenses, listed in [licenses](licenses).
