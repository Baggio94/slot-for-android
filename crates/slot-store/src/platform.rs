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
    /// Master System and Mark III.
    Sms,
    /// Game Gear.
    Gg,
    /// Neo Geo Pocket and Neo Geo Pocket Color: one shelf.
    Ngp,
    /// WonderSwan and WonderSwan Color: one shelf.
    Ws,
    /// Atari Lynx.
    Lynx,
    /// Pokémon mini.
    Pokemini,
    /// PICO-8 carts (.p8 and .p8.png).
    Pico8,
}

impl Platform {
    pub const ALL: [Platform; 13] = [
        Platform::Gba, Platform::Gb, Platform::Gbc,
        Platform::Nes, Platform::Snes, Platform::Md,
        Platform::Sms, Platform::Gg, Platform::Ngp, Platform::Ws,
        Platform::Lynx, Platform::Pokemini, Platform::Pico8,
    ];

    pub fn name(self) -> &'static str {
        self.system().name
    }

    pub fn dir_name(self) -> &'static str {
        self.system().dir
    }

    pub fn parse(name: &str) -> Option<Platform> {
        Platform::ALL.into_iter().find(|p| name.eq_ignore_ascii_case(p.dir_name()))
    }

    pub fn extensions(self) -> &'static [&'static str] {
        self.system().extensions
    }

    /// The Game Boy family draws into the fixed 240x160 canvas; the home
    /// consoles hand over each frame at the size their core produced.
    pub fn handheld(self) -> bool {
        self.system().handheld
    }

    /// The usual picture: for the home consoles, what the core shows most
    /// of the time (a frame may differ, e.g. SNES hi-res or MD H32).
    pub fn picture(self) -> (u32, u32) {
        self.system().picture
    }

    pub fn accepts(self, path: &Path) -> bool {
        // A .png is a PICO-8 cart only as NAME.p8.png.
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

/// A cart's name without its extension: "Celeste.p8.png" is "Celeste", as
/// "Pokemon.gba" is "Pokemon".
pub fn cart_stem(file_name: &str) -> &str {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".p8.png") {
        return &file_name[..file_name.len() - ".p8.png".len()];
    }
    file_name.rsplit_once('.').map_or(file_name, |(stem, _)| stem)
}
