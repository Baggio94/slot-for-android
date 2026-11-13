use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Platform {
    #[default]
    Gba,
    Gb,
    Gbc,
    /// NES and Famicom: one system, the cart shell follows the region.
    Nes,
    /// Super Nintendo and Super Famicom.
    Snes,
    /// Mega Drive and Genesis.
    Md,
    /// PICO-8 carts (.p8 and .p8.png), played by fake-08.
    Pico8,
}

impl Platform {
    pub const ALL: [Platform; 7] = [
        Platform::Gba, Platform::Gb, Platform::Gbc,
        Platform::Nes, Platform::Snes, Platform::Md, Platform::Pico8,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Platform::Gba => "Game Boy Advance",
            Platform::Gb => "Game Boy",
            Platform::Gbc => "Game Boy Color",
            Platform::Nes => "NES",
            Platform::Snes => "Super Nintendo",
            Platform::Md => "Mega Drive",
            Platform::Pico8 => "PICO-8",
        }
    }

    pub fn dir_name(self) -> &'static str {
        match self {
            Platform::Gba => "GBA",
            Platform::Gb => "GB",
            Platform::Gbc => "GBC",
            Platform::Nes => "NES",
            Platform::Snes => "SNES",
            Platform::Md => "MD",
            Platform::Pico8 => "P8",
        }
    }

    pub fn parse(name: &str) -> Option<Platform> {
        Platform::ALL.into_iter().find(|p| name.eq_ignore_ascii_case(p.dir_name()))
    }

    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Platform::Gba => &["gba"],
            Platform::Gb | Platform::Gbc => &["gb", "gbc"],
            Platform::Nes => &["nes", "fc", "unf", "unif"],
            Platform::Snes => &["sfc", "smc"],
            // .bin is also used by other systems: Android only files a .bin
            // here when its header says SEGA.
            Platform::Md => &["md", "gen", "smd", "bin"],
            // A .png is a PICO-8 cart only as NAME.p8.png (Android also
            // checks its size, 160x205).
            Platform::Pico8 => &["p8", "png"],
        }
    }

    /// The Game Boy family draws into the fixed 240x160 canvas; the home
    /// consoles hand over each frame at the size their core produced.
    pub fn handheld(self) -> bool {
        matches!(self, Platform::Gba | Platform::Gb | Platform::Gbc)
    }

    /// The usual picture: for the home consoles, what the core shows most
    /// of the time (a frame may differ, e.g. SNES hi-res or MD H32).
    pub fn picture(self) -> (u32, u32) {
        match self {
            Platform::Gba => (240, 160),
            Platform::Gb | Platform::Gbc => (160, 144),
            Platform::Nes => (256, 240),
            Platform::Snes => (256, 224),
            Platform::Md => (320, 224),
            Platform::Pico8 => (128, 128),
        }
    }

    pub fn accepts(self, path: &Path) -> bool {
        if self == Platform::Pico8 && !pico8_name(path) {
            return false;
        }
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| {
                self.extensions()
                    .iter()
                    .any(|k| ext.eq_ignore_ascii_case(k))
            })
    }
}

/// PICO-8 carts: NAME.p8, or NAME.p8.png (a picture of the cart).
fn pico8_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_ascii_lowercase())
        .is_some_and(|n| n.ends_with(".p8") || n.ends_with(".p8.png"))
}

/// A cart's name without its extension: "Celeste.p8.png" is "Celeste".
pub fn cart_stem(file_name: &str) -> &str {
    let lower = file_name.to_ascii_lowercase();
    for ext in [".p8.png", ".p8"] {
        if lower.ends_with(ext) {
            return &file_name[..file_name.len() - ext.len()];
        }
    }
    file_name.rsplit_once('.').map_or(file_name, |(stem, _)| stem)
}
