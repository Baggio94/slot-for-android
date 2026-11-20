# slot. for Android — v2.5.1

Take your slot. with you: the new **slot. folder** carries your saves, states, labels, favorites and settings from one handheld to another, and **RomM** can now keep your saves in sync too.

### Fixed in 2.5.1

- Big ROM folders: slot. now reads up to 10,000 games. When a folder is even bigger, slot. adds the games it found instead of leaving the shelf empty, and says some games were skipped ([#122](https://github.com/Baggio94/slot-for-android/issues/122)).
- The Apps page, slot.'s windows (RomM, slot. folder…) and the RetroAchievements account now follow your A and B buttons from **Settings → Controls**, like the shelf. B no longer opens an app on the Apps page ([#123](https://github.com/Baggio94/slot-for-android/issues/123)).

### New in 2.5

- **slot. folder** — Keep slot.'s data in a folder of your choice, on the internal storage or an SD card: saves, save states, labels, Cart Studio changes, favorites, recents, cheats and settings. Put the card in another handheld, pick the same folder there, and everything comes back. Each handheld keeps its own screen sizes, overlays, Frame Pacing, Run-Ahead, Touchscreen and Handheld settings. **START → Library → Choose slot. Folder**. See the [slot. Folder](https://github.com/Baggio94/slot-for-android/wiki/slot.-Folder) guide.
- **Sync Saves with RomM** — slot. sends your in-game saves to your RomM server and brings back the newer one when you start a game. If both changed, slot. asks which one to play. **START → Library → Saves & States**.
- **Saves & States page** — Save Folder, Save State Folder, Share Core Saves and Sync Saves with RomM now have their own page in **Library**.
- **Your games keep their settings across handhelds** — Favorites, recents, labels, cores and game settings follow a game by its system and file name, wherever it sits.
- **Two saves, one choice** — When a handheld and your slot. folder each have a different save for a game, slot. asks which one to use. The other one is kept.
- **Deleted saves stay deleted** — A save you delete in RetroArch's folders, in your slot. folder or on another handheld is no longer put back: slot. asks **PUT IT BACK** or **DELETE IT** when you start the game.
- **Your own cheats** — Put RetroArch `.cht` files in your slot. folder's **CustomCheats** folder: they show first in the game's Cheats, as **My Cheats**.
- **BIOS from RomM** — Game Boy, Game Boy Color and Game Boy Advance BIOS files come from your RomM server with your games, when slot. has none.
- **Default wallpapers** — A new mosaic for each shelf, drawn for slot., in the colours of your theme. The wood wallpaper is now called **slot.**.
- **Wallpaper page** — **Blur**, **Halo** and **Tint** in percent, for every shelf.
- **Touch** — Push the middle cart up to choose its core, then tap a chip. Hold a cart to open it in Cart Studio: the cart flies to the page and back.
- **Progress pill** — Long tasks (copying to your slot. folder, RomM downloads…) show a small pill next to the battery.
- **Low battery** — Under 15 %, the battery turns red on the shelf. Under 10 %, a small red battery blinks over the game ([#87](https://github.com/Baggio94/slot-for-android/issues/87)).
- **Logs** — **About → X** saves slot.'s log to a file of your choice, to attach to an issue ([#107](https://github.com/Baggio94/slot-for-android/issues/107)).
- **Grey carts** — When the SD card with your games is out, their carts turn grey and slot. tells you the card is missing.
- **Cores updated** — Genesis Plus GX, Gambatte and Snes9x at libretro's latest.

### Fixed in 2.5

- RetroAchievements: Game Boy Color DX hacks play the Game Boy set ([#117](https://github.com/Baggio94/slot-for-android/issues/117)).
- The selected game's title fits narrow screens instead of running past the edges ([#119](https://github.com/Baggio94/slot-for-android/issues/119)).
- Large RomM libraries no longer crash slot. while loading ([#121](https://github.com/Baggio94/slot-for-android/issues/121)).
- RG Rotate: the cover shows sooner when you close the screen.
- A game never waits more than 2 seconds for an unreachable RomM server.
- A setting from a newer version of slot. resets alone instead of resetting all your settings.
- Simpletex looks right on screens where the Game Boy isn't at a whole scale.

### Updating

Install over your current version, as usual. Your favorites, labels, saves and settings are kept. To keep your saves safe from now on, choose a slot. folder in **START → Library → Choose slot. Folder**.
