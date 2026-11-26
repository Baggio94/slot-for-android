# slot. for Android v3.1.1

**A new way to browse your shelves, smarter scaling, faster setup, and more!**

### New in 3.1.0

- **Shelves** — With all the new systems, jumping from shelf to shelf with L1/R1 isn't always easy when you have many of them. Press **B** from the cartridge carousel to open a new visual shelf selector, featuring your cartridges, game counts, Favorites and Recents. Navigate with your controller or touchscreen, and choose between Scroll and Grid layouts in **Settings → Interface → Shelves**. I'd love to hear your feedback on this one!
- **Smart Integer Scaling** — slot. automatically chooses between integer overscale and underscale, allowing up to 8 native pixels of cropping per edge for home consoles and 4 for handhelds. Get a larger picture without losing too much of the original image (#132). I'm less sure about the cropping on handheld systems, so let me know what you think too.
- **Smarter default screen sizes** — Home consoles now start in Integer whenever it provides a good fit for your screen, while preserving the existing aspect ratio rules and your custom settings.
- **More LCD Effects** — Pixel Transparency and Sunlight Shimmer are now available for Game Gear, Neo Geo Pocket/Color, WonderSwan/Color and Atari Lynx, with per-system and per-game settings (#128).
- **Rumble strength** — Choose between OFF, LOW, MEDIUM, HIGH and MAX. Short vibration pulses are also handled more reliably (#131).
- **Swap Menu A/B** — Swap A and B throughout slot.'s interface without changing your in-game controls. Available in **Settings → Controls** (#127).
- **Faster and lighter** — Faster library scans and label scraping, slot.'s own artwork first, artwork restored from your slot. folder before searching online, game launches that only wait for their own files, more efficient slot. folder syncing, lower memory use, and a SYNCING indicator that counts incoming files.

### Fixed in 3.1.1

- **slot. folder** — Save states no longer fail to copy with "COPY FAILED" when two folders' names differ only by letter case, as with Pokémon mini states on internal storage or SD cards. The extra "POKEMINI (1)", "POKEMINI (2)"… folders created by this issue are left untouched, and you can delete them.
- **Shelves** — Names and the selected tile are now easy to read on light backgrounds, in the Light theme and on the Original theme's light shelves.

### Fixed in 3.1.0

- **Cart Studio** — Custom colors, transparency and shell edits are no longer reset after updates, library refreshes or moving your slot. folder to another handheld.
- **Beetle WonderSwan** — Games no longer fail to launch because of the protected save state check. Other affected cores are fixed too (#133).
- **Refresh Library** — Now checks for newly available labels even when your ROM library hasn't changed. Your custom labels are kept.
- **RetroAchievements** — Smart Cache's library preparation no longer holds back in-game achievements, and the current game's badges come first.
- **Sega BIOS** — Boot issues fixed by keeping the cores' default boot behavior.
- **RomM** — BIOS downloads fixed for supported systems.
- **Storage** — Better handling of unavailable, read-only or inaccessible storage.
- More refinements, fixes and improvements under the hood.

As always, thanks for playing slot.!
