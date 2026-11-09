/// mGBA's 48 palettes in its order, then the TWB64 one slot. adds to mGBA
/// (scripts/patch-mgba-twb64.py) and Gambatte ships in its TWB64 Pack 1.
pub const TWB64_GAME_COM: &str = "TWB64 028 Game.com Ver.";

const NAMES: [&str; 49] = [
    "Grayscale",
    "DMG Green",
    "GB Pocket",
    "GB Light",
    "GBC Brown ↑",
    "GBC Red ↑A",
    "GBC Dark Brown ↑B",
    "GBC Pale Yellow ↓",
    "GBC Orange ↓A",
    "GBC Yellow ↓B",
    "GBC Blue ←",
    "GBC Dark Blue ←A",
    "GBC Gray ←B",
    "GBC Green →",
    "GBC Dark Green →A",
    "GBC Reverse →B",
    "SGB 1-A",
    "SGB 1-B",
    "SGB 1-C",
    "SGB 1-D",
    "SGB 1-E",
    "SGB 1-F",
    "SGB 1-G",
    "SGB 1-H",
    "SGB 2-A",
    "SGB 2-B",
    "SGB 2-C",
    "SGB 2-D",
    "SGB 2-E",
    "SGB 2-F",
    "SGB 2-G",
    "SGB 2-H",
    "SGB 3-A",
    "SGB 3-B",
    "SGB 3-C",
    "SGB 3-D",
    "SGB 3-E",
    "SGB 3-F",
    "SGB 3-G",
    "SGB 3-H",
    "SGB 4-A",
    "SGB 4-B",
    "SGB 4-C",
    "SGB 4-D",
    "SGB 4-E",
    "SGB 4-F",
    "SGB 4-G",
    "SGB 4-H",
    TWB64_GAME_COM,
];

/// Each palette's background ramp as Dot Matrix needs it, 0xRRGGBB: the
/// lightest shade, the second darkest (the dots), the darkest. From mGBA's
/// presets (src/gb/overrides.c), ordered by brightness.
const DOT_MATRIX: [[u32; 3]; 49] = [
    [0xFFFFFF, 0x525252, 0x000000], // Grayscale
    [0x8BA44A, 0x294120, 0x182908], // DMG Green
    [0xA4B4A4, 0x4A6252, 0x182920], // GB Pocket
    [0x7BF6FF, 0x18B4AC, 0x007B73], // GB Light
    [0xFFFFFF, 0x833100, 0x000000], // GBC Brown ↑
    [0xFFFFFF, 0x943939, 0x000000], // GBC Red ↑A
    [0xFFE6C5, 0x836A29, 0x5A3108], // GBC Dark Brown ↑B
    [0xFFFFA4, 0x9494FF, 0x000000], // GBC Pale Yellow ↓
    [0xFFFFFF, 0xFF0000, 0x000000], // GBC Orange ↓A
    [0xFFFFFF, 0x7B4A00, 0x000000], // GBC Yellow ↓B
    [0xFFFFFF, 0x0000FF, 0x000000], // GBC Blue ←
    [0xFFFFFF, 0x52528B, 0x000000], // GBC Dark Blue ←A
    [0xFFFFFF, 0x525252, 0x000000], // GBC Gray ←B
    [0xFFFFFF, 0xFF4100, 0x000000], // GBC Green →
    [0xFFFFFF, 0x0062C5, 0x000000], // GBC Dark Green →A
    [0xFFFFFF, 0x008383, 0x000000], // GBC Reverse →B
    [0xFFEECD, 0xAC2920, 0x311852], // SGB 1-A
    [0xDEDEC5, 0xB45210, 0x000000], // SGB 1-B
    [0xFFC5FF, 0x9C3962, 0x39399C], // SGB 1-C
    [0xFFFFAC, 0xFF0000, 0x521800], // SGB 1-D
    [0xFFDEB4, 0x6A8B41, 0x5A3920], // SGB 1-E
    [0xDEEEFF, 0xAC0000, 0x004110], // SGB 1-F
    [0xFFFF5A, 0x7B7B00, 0x000052], // SGB 1-G
    [0xFFEEE6, 0x834100, 0x311800], // SGB 1-H
    [0xF6CDA4, 0x297B00, 0x000000], // SGB 2-A
    [0xFFFFFF, 0xFF3100, 0x52005A], // SGB 2-B
    [0xFFC5FF, 0x7B31EE, 0x29299C], // SGB 2-C
    [0xFFFFA4, 0xFF3100, 0x000052], // SGB 2-D
    [0xFFCD83, 0x291062, 0x100810], // SGB 2-E
    [0xD5FFFF, 0xA40000, 0x180000], // SGB 2-F
    [0xE6BD83, 0xE65241, 0x001800], // SGB 2-G
    [0xFFFFFF, 0x737373, 0x000000], // SGB 2-H
    [0xFFD59C, 0xFF6229, 0x314A62], // SGB 3-A
    [0xDEDEC5, 0x005200, 0x001010], // SGB 3-B
    [0xFFFF7B, 0x00BDFF, 0x20205A], // SGB 3-C
    [0xF6FFBD, 0x08CD00, 0x000000], // SGB 3-D
    [0xFFFFC5, 0xB47B20, 0x524A73], // SGB 3-E
    [0xFFD500, 0x7B7BCD, 0x414141], // SGB 3-F
    [0xFFFFFF, 0xCD3139, 0x390000], // SGB 3-G
    [0xE6FFA4, 0x4A8B18, 0x081800], // SGB 3-H
    [0xF6AC6A, 0xD500D5, 0x00007B], // SGB 4-A
    [0xF6EEF6, 0x417B39, 0x180808], // SGB 4-B
    [0xFFE6E6, 0x9CA4E6, 0x080000], // SGB 4-C
    [0xFFFFBD, 0x4A6A7B, 0x08204A], // SGB 4-D
    [0xFFDEAC, 0x7B5A8B, 0x002031], // SGB 4-E
    [0xBDD5D5, 0x8300A4, 0x390000], // SGB 4-F
    [0xB4E618, 0xBD205A, 0x291000], // SGB 4-G
    [0xFFFFCD, 0x838B41, 0x415229], // SGB 4-H
    [0xA7BF6B, 0x0F4F2F, 0x000000], // TWB64 028 Game.com Ver.
];

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct GbPalette(u8);

impl GbPalette {
    pub const COUNT: usize = NAMES.len();
    pub const DEFAULT: GbPalette = GbPalette(1);

    pub fn all() -> impl Iterator<Item = GbPalette> {
        (0..Self::COUNT as u8).map(GbPalette)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// Lightest shade, the dots' shade and darkest shade, 0xRRGGBB.
    pub fn dot_matrix(self) -> [u32; 3] {
        DOT_MATRIX[self.index()]
    }

    pub fn core_name(self) -> &'static str {
        NAMES[self.index()]
    }

    pub fn label(self) -> &'static str {
        let name = self.core_name();
        if name == TWB64_GAME_COM { return "TWB64 Game.com"; }
        match name.find(['↑', '↓', '←', '→']) {
            Some(at) => name[..at].trim_end(),
            None => name,
        }
    }

    pub fn next(self) -> GbPalette {
        GbPalette(((self.index() + 1) % Self::COUNT) as u8)
    }

    pub fn prev(self) -> GbPalette {
        GbPalette(((self.index() + Self::COUNT - 1) % Self::COUNT) as u8)
    }

    pub fn parse(name: &str) -> Option<GbPalette> {
        Self::all().find(|p| p.core_name() == name)
    }
}
