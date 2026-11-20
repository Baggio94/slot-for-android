//! Every system slot. plays, in one table: what the shelf, the scanner,
//! RomM, RetroAchievements, ScreenScraper and the core boards need to know.
//! A new system is one more row here (and in the Android app's
//! `Systems.TABLE`, which `tests/systems.rs` keeps identical).

use crate::{Core, Platform};

#[derive(Debug, PartialEq, Eq)]
pub struct System {
    pub platform: Platform,
    /// The shelf and Settings page title.
    pub name: &'static str,
    /// The shelf key the Android side passes ("GBA", "MD"...), also the
    /// system's folder in slot.'s own files (CustomCheats/GBA...).
    pub dir: &'static str,
    /// Cartridge file extensions, lower case, without the .zip container.
    pub extensions: &'static [&'static str],
    /// ES-DE ROM folder names: an existing one is used, else the first is
    /// made (RomM downloads).
    pub folders: &'static [&'static str],
    /// RomM's platform slugs (IGDB's) for this system.
    pub romm: &'static [&'static str],
    /// RetroAchievements console id.
    pub retroachievements: u32,
    /// ScreenScraper system id (labels).
    pub screenscraper: u32,
    /// The board's two sockets: the default core first.
    pub cores: [Core; 2],
    /// The usual picture the core hands over.
    pub picture: (u32, u32),
    /// The Game Boy family draws into the fixed 240x160 canvas.
    pub handheld: bool,
}

pub const SYSTEMS: [System; 6] = [
    System {
        platform: Platform::Gba, name: "Game Boy Advance", dir: "GBA",
        extensions: &["gba"], folders: &["gba"], romm: &["gba"],
        retroachievements: 5, screenscraper: 12,
        cores: [Core::Mgba, Core::Gpsp], picture: (240, 160), handheld: true,
    },
    System {
        platform: Platform::Gb, name: "Game Boy", dir: "GB",
        extensions: &["gb", "gbc"], folders: &["gb"], romm: &["gb"],
        retroachievements: 4, screenscraper: 9,
        cores: [Core::Mgba, Core::Gambatte], picture: (160, 144), handheld: true,
    },
    System {
        platform: Platform::Gbc, name: "Game Boy Color", dir: "GBC",
        extensions: &["gb", "gbc"], folders: &["gbc"], romm: &["gbc"],
        retroachievements: 6, screenscraper: 10,
        cores: [Core::Mgba, Core::Gambatte], picture: (160, 144), handheld: true,
    },
    System {
        platform: Platform::Nes, name: "NES", dir: "NES",
        extensions: &["nes", "fc", "unf", "unif"], folders: &["nes", "famicom"],
        romm: &["nes", "famicom"],
        retroachievements: 7, screenscraper: 3,
        cores: [Core::Fceumm, Core::Nestopia], picture: (256, 240), handheld: false,
    },
    System {
        platform: Platform::Snes, name: "Super Nintendo", dir: "SNES",
        extensions: &["sfc", "smc"], folders: &["snes", "sfc", "snesna"],
        romm: &["snes", "sfam"],
        retroachievements: 3, screenscraper: 4,
        cores: [Core::Snes9x, Core::Bsnes], picture: (256, 224), handheld: false,
    },
    System {
        // .bin is also used by other systems: Android only files a .bin
        // here when its header says SEGA.
        platform: Platform::Md, name: "Mega Drive", dir: "MD",
        extensions: &["md", "gen", "smd", "bin"], folders: &["megadrive", "genesis", "megadrivejp"],
        romm: &["genesis", "megadrive", "genesis-slash-megadrive", "sega-mega-drive"],
        retroachievements: 1, screenscraper: 1,
        cores: [Core::GenesisPlusGx, Core::Picodrive], picture: (320, 224), handheld: false,
    },
];

impl Platform {
    /// This system's row of SYSTEMS.
    pub fn system(self) -> &'static System {
        &SYSTEMS[self as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_row_per_platform_in_order() {
        assert_eq!(SYSTEMS.len(), Platform::ALL.len());
        for (row, platform) in SYSTEMS.iter().zip(Platform::ALL) {
            assert_eq!(row.platform, platform);
        }
    }
}
