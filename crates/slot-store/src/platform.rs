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
}

impl Platform {
    pub const ALL: [Platform; 8] = [
        Platform::Gba, Platform::Gb, Platform::Gbc,
        Platform::Nes, Platform::Snes, Platform::Md,
        Platform::Sms, Platform::Gg,
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
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| {
                self.extensions()
                    .iter()
                    .any(|k| ext.eq_ignore_ascii_case(k))
            })
    }
}
