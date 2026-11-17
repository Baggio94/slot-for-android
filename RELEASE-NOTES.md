# slot. for Android — v2.4.2

The Anbernic RG Rotate is the star of this update: slot. for Android now fits its square screen, its buttons and its rotating screen, with now playing and stand by screens when you close it.

### Fixed in 2.4.2

- **Saves you copy into your save folder now load.** If you already played that game in slot., it asks whether to play the copied save or continue where you left off ([#116](https://github.com/Baggio94/slot-for-android/issues/116)).
- **Save states you copy into your states folder load too**, even when the file has an old date.
- **Your newest save is kept after a Link trade**, even if it couldn't be copied to your save folder right away.

### New in 2.4.1

- **Handwritten carts** — A cart with no label art now gets its title written by hand on the plastic. **slot. label** in Cart Studio writes it the same way. From slot. original 1.16.
- **Touch on the shelf** — Swipe to scroll the carts, tap a side cart to bring it to the middle, tap or pull down the middle cart to insert it. **Settings → Controls → Touchscreen**.
- **GBA: X / Y as L / R** — A new **L / R** value for the X / Y row in **Systems → Controls**.
- **Update progress** — **About** shows the download percentage.
- **RomM labels** — The labels shown on the RomM page now go on the carts of the games you download.

### Fixed in 2.4.1

- RetroArch save states compressed with zstd now load in slot. Thanks to Daniel Byon for testing.
- A save folder Android can't read no longer stops the game: it starts with slot.'s own save.
- **Keep Folder Saves**: a game starts from the save in your saves folder, not from an older slot. save state.
- Labels found by the label search show on the carts right away, without restarting slot.
- A colour or finish changed in Cart Studio shows on the shelf right away.
- Choosing a ROM or BIOS folder takes you back to the shelf.
- A deleted ROM folder empties the shelf with **ROM FOLDER NOT FOUND**. Your saves, states, labels and favorites are kept.
- **Update available** sits at the right of the About row again.
- Light and Original themes: the selected row of the in-game **Screen** page is readable.
- **Settings → Controls**: the selection bar sits on its row again.
- RomM: choosing your ROM folder from the RomM page keeps the page open, and the games reach the shelf in groups as they download, with **DOWNLOADING n/m** while they come in.

### Anbernic RG Rotate (since 2.4)

- The **middle button** works like the slot. cap: tap to insert or open the game menu, double-tap for the Save State Switcher, hold to save and eject.
- **Closing the screen** shows a cover of your choice, set in **Settings → Handheld**:
  - while playing: **Inserted Cart**, **Full Cart** or **Last Frame**;
  - with no game: **Drifting Shelf**, **Last Played**, **Random Cart**, **Black**, **Digital Clock** or **Analog Clock**.
- **Game Info** adds **Now Playing**, the game's title and the time played to the cover while playing.
- After a while, slot. dims the screen, then puts the RG Rotate to sleep (**Dim After**, **Sleep After**, **Sleep When Closed**). Opening it takes you straight back to the shelf or your game.

### RG Rotate setup

For the best experience with slot., set these on your RG Rotate:

1. **Settings → Security → Screen lock → Swipe**
2. **Settings → Handheld Game Console Settings → Rotate Setting → Slide out Setting → Wake Up and Unlock**
3. **Settings → Handheld Game Console Settings → Rotate Setting → Slide into Setting → No Operation**
4. **Quick Settings** (swipe down twice from the top) **→ Direction following system → On**
5. **Settings → Handheld Game Console Settings → Quick Launcher → slot.**

Then in slot.: **START → Settings → Handheld → Sleep When Closed → ON**, and allow the permission Android asks for.

More in the [RG Rotate](https://github.com/Baggio94/slot-for-android/wiki/RG-Rotate) guide.
