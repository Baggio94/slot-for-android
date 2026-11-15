# slot. for Android — v2.4.0

The Anbernic RG Rotate is the star of this update: slot. for Android now fits its square screen, its buttons and its rotating screen, with covers to choose when you close it. This update also gives slot. a new look, with four themes and a wallpaper for each shelf.

### What's New

- **Anbernic RG Rotate** — slot. now fits the RG Rotate's square screen and its buttons:
  - The **middle button** works like the slot. cap: tap to insert or open the game menu, double-tap for the Save State Switcher, hold to save and eject.
  - **Closing the screen** shows a cover of your choice, set in **Settings → Handheld**:
    - while playing: **Inserted Cart**, **Full Cart** or **Last Frame**;
    - with no game: **Drifting Shelf**, **Last Played**, **Random Cart**, **Black**, **Digital Clock** or **Analog Clock**.
  - **Game Info** adds **Now Playing**, the game's title and the time played to the cover while playing.
  - After a while, slot. dims the screen, then puts the RG Rotate to sleep (**Dim After**, **Sleep After**, **Sleep When Closed**). Opening it takes you straight back to your game.
  - See the setup below for the best experience.
- **A new look** — Every menu has been redrawn: a softer anthracite background, a rounded selection bar, the page's path above its title, and long pages that scroll smoothly. The same style now covers Cart Studio, the Library, RomM, Apps and the RetroAchievements pages.
- **Four themes** — **Settings → Interface → Theme**: **Dark** (default), **OLED** (true black), **Light** and **Original** (each console's slot in its own colours). The theme now dresses every page, and the shelves take matching colours.
- **A wallpaper for each shelf** — **Settings → Interface → Wallpaper**: pick a shelf, then its wallpaper. A shelf without its own shows the General one. With **None**, the shelf shows the theme's colour.
- **C and Z buttons** — On six-button handhelds such as the Retroid Pocket Classic 6-button, the pad's C and Z buttons can play the Mega Drive's C and Z. On other systems, a **C / Z** row in **Systems → Controls** gives them a shortcut such as Save / Load or Turbo. Thanks to [yunake](https://github.com/yunake) ([#108](https://github.com/Baggio94/slot-for-android/pull/108)).
- **Faster menus** — **L1 / R1** jump a screen up or down in long menus (**L2 / R2** in Cheats).
- **Long names** — A setting value or cheat name too long for its row now scrolls when you select it, instead of being shrunk or cut.
- **Screen menu in game** — It now shows three rows at a time, so more of the game stays in view while you change the picture.
- **Games keep the screen on** — While a game is running, Android no longer turns the screen off.

### Fixed

- A second launch of slot. (from another launcher, for example) no longer opens a second copy, which made every game fail with "Couldn't start". Thanks to [yunake](https://github.com/yunake) for tracking it down ([#109](https://github.com/Baggio94/slot-for-android/pull/109)).
- On the RG Rotate, the screen no longer starts upside down, and the Apps page no longer turns a quarter.
- The About page fits narrow and square screens: the label is printed at full size instead of shrunk.
- Light theme: the Mega Drive slot is drawn in light greys like the other consoles.
- RomM and RetroAchievements sign-in pages: the button hints fit narrow screens and the text no longer gets cut.
- Apps: holding a direction no longer keeps scrolling on its own.

### RG Rotate setup

For the best experience with slot., set these on your RG Rotate:

1. **Settings → Security → Screen lock → Swipe**
2. **Settings → Handheld Game Console Settings → Rotate Setting → Slide out Setting → Wake Up and Unlock**
3. **Settings → Handheld Game Console Settings → Rotate Setting → Slide into Setting → No Operation**
4. **Quick Settings** (swipe down twice from the top) **→ Direction following system → On**
5. **Settings → Handheld Game Console Settings → Quick Launcher → slot.**

Then in slot.: **START → Settings → Handheld → Sleep When Closed → ON**, and allow the permission Android asks for.

More in the [RG Rotate](https://github.com/Baggio94/slot-for-android/wiki/RG-Rotate) guide.

### Installation

Install the APK below over your existing version: your games, saves and settings are kept. On 2.0 and later, slot. also offers this update from **START → About**. On 1.3, install this APK by hand once.

### Credits

An unofficial Android port of [slot.](https://slot-cfw.fyi/) by [Brandon Kowalski](https://github.com/BrandonKowalski/slot) and contributors. Thanks to [yunake](https://github.com/yunake) for the contributions in this release. Overlays by [Watomsk](https://github.com/Watomsk/Overlays). Pixel Transparency and Sunlight Shimmer by [Matt Akins](https://github.com/mattakins/Pixel_Transparency) and GDAPT by Sp00kyFox, used with their authors' permission. crt-royale by TroggleMonkey. Anime4K by [bloc97](https://github.com/bloc97/Anime4K), FSR by AMD. Cheats from [libretro-database](https://github.com/libretro/libretro-database) (CC BY-SA 4.0). Emulation by mGBA, gpSP, Gambatte, FCEUmm, Nestopia, Snes9x, bsnes, Genesis Plus GX and PicoDrive. Labels from ScreenScraper. Works with [RomM](https://github.com/rommapp/romm).

Licensed under GPL-3.0.
