# slot. for Android 2.4 — drafts to validate

Four parts: 1. release notes, 2. CHANGELOG entry, 3. README changes, 4. wiki changes.

---

## 1. Release notes (RELEASE-NOTES.md and the GitHub release)

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

---

## 2. CHANGELOG.md (new entry at the top)

## v2.4.0 — <date of release> November 2026

- Interface redesign: anthracite ground, rounded selection bar with margins, dimmed unselected rows, path line and page title, thin scroll bar, long pages that slide only near the edge; the same style on Cart Studio, Library, RomM and the Android pages (Apps, RetroAchievements, dialogs).
- Themes: Dark, OLED, Light and Original on every page (ink remapped on the GPU), shelf colours per theme (Light cream, Original pale console tints). Wallpaper None shows the theme's colour, including during insert and eject; the Theme wallpaper is removed. OLED is saved so older versions still read the settings.
- Settings: Interface (Theme, Wallpaper ›, Recents, Game Titles, Status Bar ›), Systems, Gameplay, Emulation, Controls, and Handheld on the RG Rotate. Interface → Wallpaper: Shelf (General, then each shelf with games), Wallpaper, Wallpaper Tint, Import, Delete; a wallpaper per shelf (`wallpaper_shelves`), the old Shelf mode converted on load.
- Anbernic RG Rotate: square screen held in its orientation (no upside-down start); the middle button as the slot. cap (tap, double-tap, hold 0.4 s to eject), the side button left to Android; one slot. when it is the home app. Closed cover (Hall sensor): Playing (Inserted Cart, Full Cart, Last Frame), Game Info (Off, Title, Time, All: Now Playing, title, system, time played while the game runs), Standby (Drifting Shelf from the cart in view, Last Played, Random Cart, Black, Digital Clock, Analog Clock), Dim After, Sleep After, Sleep When Closed (DevicePolicyManager.lockNow under a Swipe lock screen, NEEDS SWIPE LOCK without one). Opening returns straight to the game or the cart shown. The screen going off shut (power key, timeout) hands over to the lock screen, so the lid's keys never reach slot. without a window. Retrogame_Joypad is the built-in controller. Apps takes slot.'s orientation.
- C and Z for six-button pads (#108, Eugene Yunak): slot. buttons C (98) and Z (101), appended; Mega Drive defaults unchanged; a C / Z row with L1 / R1's choices on the other systems when a six-button pad is connected.
- MainActivity is singleTask (#109): a second launch brings back the one instance instead of opening a copy whose core stays locked.
- Menus: L1 / R1 page by six rows without wrapping (L2 / R2 in Cheats); labels at full size, long values and cheat names cut with a fade and scrolling while selected (Draw::Span), a few pixels too long shrunk instead; the in-game Screen page shows three rows.
- About: a rounded rectangle label with shadow and edge, printed at the screen's width (440-660) on narrow screens with slot.'s wordmark beside the name; rows right under it; Automatic Checks at the rows' size.
- RomM and RetroAchievements dialogs aligned with the RA page, legends fitted to narrow screens, back to the top on the first item, RA sentence on two lines. Apps: slot.'s own key repeat. Light theme: Mega Drive slot inside in the SNES greys.
- A running game keeps the screen on (FLAG_KEEP_SCREEN_ON) on every handheld.

---

## 3. README changes

**Highlights** — change one bullet and add one:

- **Any screen shape.** The interface adapts to square, 4:3, 3:2 and widescreen handhelds at their native resolution, including the Anbernic RG Rotate.
- **Themes.** Dark, OLED, Light and Original, with a wallpaper for each shelf. *(new, after "Cartridges first")*

**Essential controls** — the line under the heading becomes:

> The KONKR's round top button is the **slot. cap**. On a Retroid Pocket, it's the round centre key. On the Anbernic RG Rotate, it's the middle button.

**Guide** — add the RG Rotate page to the last line:

- [Special Games](https://github.com/Baggio94/slot-for-android/wiki/Special-Games) (Boktai, motion games) · [Apps & Home Launcher](https://github.com/Baggio94/slot-for-android/wiki/Apps-and-Home-Launcher) · [RG Rotate](https://github.com/Baggio94/slot-for-android/wiki/RG-Rotate)

---

## 4. Wiki changes

### New page: RG-Rotate.md

# RG Rotate

slot. supports the Anbernic RG Rotate: its square screen, its buttons and its rotating screen.

## Setup

For the best experience with slot., set these on your RG Rotate:

1. **Settings → Security → Screen lock → Swipe**. slot. puts the RG Rotate to sleep under the lock screen. Without one, it can only dim the screen. With a PIN, you have to unlock before you get back to your game.
2. **Settings → Handheld Game Console Settings → Rotate Setting → Slide out Setting → Wake Up and Unlock**, so opening the screen wakes the RG Rotate and goes straight back to slot.
3. **Settings → Handheld Game Console Settings → Rotate Setting → Slide into Setting → No Operation**, so closing the screen leaves slot.'s cover on screen.
4. **Quick Settings** (swipe down twice from the top) **→ Direction following system → On**, so the cover is the right way up when the screen is closed.
5. **Settings → Handheld Game Console Settings → Quick Launcher → slot.**

Then in slot.: **START → Settings → Handheld → Sleep When Closed → ON**, and allow the permission Android asks for.

**Known issue** — Opening the RG Rotate can briefly show Android's lock screen clock. Android shows its lock screen first when the RG Rotate wakes, and slot. can't come back any sooner without Android stopping it. A black lock screen wallpaper hides it well. If you know a fix, please [open an issue](https://github.com/Baggio94/slot-for-android/issues).

## Buttons

| Button | Action |
|---|---|
| Middle button | Works like the slot. cap: tap to insert a cartridge or open the Game Menu, double-tap for the Save State Switcher, hold to save and eject |
| Round side button | Left to Android |

## Closing the screen

When you close the screen, slot. shows a cover. Set it in **START → Settings → Handheld**:

| Setting | Values | Default | What it does |
|---|---|---|---|
| **Playing** | Inserted Cart, Full Cart, Last Frame | Full Cart | The cover while a game is running. The game pauses until you open the screen again. |
| **Game Info** | Off, Title, Time, All | Off | Adds **Now Playing** to the Playing cover, with the game's title and system (**Title**), the time played (**Time**), or both (**All**). The time counts only while the game runs. |
| **Standby** | Drifting Shelf, Last Played, Random Cart, Black, Digital Clock, Analog Clock | Drifting Shelf | The cover with no game. **Drifting Shelf** scrolls slowly from the cart in view. **Random Cart** shows a new cart each minute. |
| **Dim After** | 5 S, 10 S, 15 S, 30 S | 15 S | Dims the screen after this time closed. |
| **Sleep After** | 30 S, 1 MIN, 2 MIN, 5 MIN, NEVER | 1 MIN | This long after dimming, puts the RG Rotate to sleep (with Sleep When Closed on), or turns the screen fully dark. |
| **Sleep When Closed** | ON, OFF | OFF | Lets slot. put the RG Rotate to sleep. Android asks for permission once. Shows **NEEDS SWIPE LOCK** when no lock screen is set. |

When you open the screen, you're back in your game, or on the cart the cover showed.

### Settings.md

- "**START → Settings** has six pages: [Interface](#interface), [Systems](#systems), [Gameplay](#gameplay), [Emulation](#emulation), [Controls](#controls) and, on the RG Rotate, [Handheld](RG-Rotate#closing-the-screen)." The **Status Bar** section moves under Interface.
- Interface table becomes:

| Setting | Values | Default | What it does |
|---|---|---|---|
| **Theme** | Dark, OLED, Light, Original | Dark | The colours of the menus and shelves. **OLED** is true black. **Original** shows each console's slot in that console's own colours. |
| **Wallpaper** | | | Opens the Wallpaper page (below). |
| **Recents** | ON, OFF | OFF | Adds the Recents shelf, with the games you played last. |
| **Game Titles** | ON, OFF | OFF | Shows the game's name under the cartridge. |
| **Status Bar** | | | Opens the Status Bar page (below). |

- New **Wallpaper** table:

| Setting | Values | Default | What it does |
|---|---|---|---|
| **Shelf** | General, then each shelf with games | General | The shelf whose wallpaper you set. **General** is used by every shelf without its own. |
| **Wallpaper** | None, Random (General only), Default, Default Blur, and your imported images | None | The picture behind the shelf. **None** shows the theme's colour. On a shelf, **General** uses the General wallpaper. |
| **Wallpaper Tint** | Off, Low, Medium, High | Off | Darkens your own wallpaper so the cartridges stand out. |
| **Import Wallpaper** | | | Pick an image on your handheld for the shelf shown. |
| **Delete Wallpaper** | | | Removes the imported image shown. |

### Controls-and-Hotkeys.md

- The slot. cap section: "On a Retroid Pocket, the round centre key works the same way. On the Anbernic RG Rotate, it's the middle button."
- New line in the menus section: "In long menus, **L1 / R1** jump a screen up or down (**L2 / R2** in Cheats)."
- Controls per system: "With a six-button pad (Retroid Pocket Classic 6-button), the pad's C and Z can play the Mega Drive's C and Z. On the other systems, a **C / Z** row gives them a shortcut, like the L1 / R1 row."

### _Sidebar.md

Under **Features**, after Apps & Home Launcher: `- [RG Rotate](RG-Rotate)`
