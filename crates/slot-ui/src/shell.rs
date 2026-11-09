use std::path::Path;

use slot_store::gb::Class;
use slot_store::{Cart, Platform, ShellFinish};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Finish {
    Solid,
    Translucent,
    /// The system's own glitter plastic: clear on a Game Boy or a Game Boy
    /// Advance cart, opaque on a home console's.
    Glitter,
    SolidGlitter,
    ClearGlitter,
}

impl Finish {
    /// The finish as a shell choice (`cart_shell.txt`, Studio files) saves it.
    pub fn stored(self) -> ShellFinish {
        match self {
            Finish::Solid => ShellFinish::Solid,
            Finish::Translucent => ShellFinish::Clear,
            Finish::Glitter => ShellFinish::Glitter,
            Finish::SolidGlitter => ShellFinish::SolidGlitter,
            Finish::ClearGlitter => ShellFinish::ClearGlitter,
        }
    }

    pub fn has_flecks(self) -> bool {
        matches!(self, Finish::Glitter | Finish::SolidGlitter | Finish::ClearGlitter)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Shell {
    pub colour: [u8; 3],
    pub finish: Finish,
}

pub const DEFAULT_SHELL: Shell = Shell {
    colour: [0x35, 0x35, 0x3a],
    finish: Finish::Solid,
};

const fn shell(colour: [u8; 3], finish: Finish) -> Shell {
    Shell { colour, finish }
}

const EXACT: &[(&str, Shell)] = &[
    ("AXV", shell([0xc2, 0x33, 0x2e], Finish::Translucent)),
    ("AXP", shell([0x2f, 0x5c, 0xc0], Finish::Translucent)),
    ("BPE", shell([0x24, 0x9c, 0x60], Finish::Translucent)),
    ("BPR", shell([0xd8, 0x52, 0x24], Finish::Solid)),
    ("BPG", shell([0x63, 0xb0, 0x44], Finish::Solid)),
    ("U3I", CLEAR),
    ("U32", CLEAR),
    ("U33", CLEAR),
    ("V49", shell([0xa9, 0x51, 0x3b], Finish::Solid)),
    ("KYGE", YOSHI),
    ("KYGP", YOSHI),
    ("RZW", shell([0x5f, 0x62, 0x64], Finish::Translucent)),
    ("RZWJ", shell([0xec, 0xee, 0xe8], Finish::Solid)),
];

const CLEAR: Shell = shell([0xd9, 0xdb, 0xd8], Finish::Translucent);
const YOSHI: Shell = shell([0x2f, 0x8f, 0x4e], Finish::Solid);

const FAMILY: &[(u8, Shell)] = &[(b'M', shell([0xc6, 0xc6, 0xc9], Finish::Solid))];

/// Classic NES Series: the NES cartridge's light grey.
const CLASSIC_NES: Shell = shell([0xb9, 0xb9, 0xb6], Finish::Solid);
/// Famicom Mini 1-20: the Famicom's red.
const FAMICOM_MINI: Shell = shell([0xa3, 0x1f, 0x2b], Finish::Solid);
/// Famicom Mini 21-30, the Disk System selection: the disks' yellow.
const DISK_SYSTEM: Shell = shell([0xf0, 0xc8, 0x1e], Finish::Solid);
const DISK_SYSTEM_CODES: &[&str] = &[
    "FM2J", "FNMJ", "FMRJ", "FPTJ", "FLBJ", "FFMJ", "FTKJ", "FTUJ", "FADJ", "FSDJ",
];

pub const DMG_SHELL: Shell = shell([0x9a, 0x97, 0x8f], Finish::Solid);

pub const DUAL_MODE_SHELL: Shell = shell([0x33, 0x30, 0x31], Finish::Solid);

pub const GB_CLEAR_SHELL: Shell = shell([0x7c, 0x7a, 0x8a], Finish::Translucent);

const GB_CODES: &[(&str, Shell)] = &[
    ("AAU", shell([0xb3, 0x8b, 0x3a], Finish::Solid)),
    ("AAUJ", DUAL_MODE_SHELL),
    ("AAX", shell([0xa9, 0xaa, 0xa7], Finish::Solid)),
    ("AAXJ", DUAL_MODE_SHELL),
    ("BYT", CRYSTAL),
    ("BXT", CRYSTAL),
    ("KTN", KIRBY),
    ("KKK", KIRBY),
    ("KCE", shell([0x1f, 0x9f, 0xb6], Finish::Translucent)),
    ("VCA", shell([0xf0, 0xa9, 0x5e], Finish::Translucent)),
    ("VPHJ", PINBALL),
    ("VPHP", PINBALL),
    ("BMG", DUAL_MODE_SHELL),
];

const GB_OVERSEAS_TITLES: &[(&str, Shell)] = &[
    ("POKEMON RED", shell([0xc0, 0x28, 0x2c], Finish::Solid)),
    ("POKEMON BLU", shell([0x2b, 0x3a, 0x88], Finish::Solid)),
    ("POKEMON YEL", shell([0xe9, 0xa8, 0x26], Finish::Solid)),
    // Donkey Kong Land 1, 2 and III all start "DONKEYKONGLAND".
    ("DONKEYKONGL", shell([0xf2, 0xc4, 0x1b], Finish::Solid)),
];

const PINBALL: Shell = shell([0xe2, 0xb4, 0x13], Finish::Solid);

const CRYSTAL: Shell = shell([0x86, 0xb9, 0xbf], Finish::Glitter);
const KIRBY: Shell = shell([0xec, 0x94, 0xb4], Finish::Translucent);

pub fn shell_for(cart: &Cart) -> Shell {
    if let Some(choice) = cart.shell {
        let finish = match choice.finish {
            ShellFinish::Solid => Finish::Solid,
            ShellFinish::Clear => Finish::Translucent,
            ShellFinish::Glitter => Finish::Glitter,
            ShellFinish::SolidGlitter => Finish::SolidGlitter,
            ShellFinish::ClearGlitter => Finish::ClearGlitter,
        };
        return shell(choice.colour, finish);
    }
    match cart.platform {
        Platform::Gba => gba_shell_for(&cart.code),
        Platform::Gb | Platform::Gbc => gb_shell_for(&cart.rom),
        Platform::Nes | Platform::Snes | Platform::Md => crate::home::home_mould_of(cart)
            .map_or(DEFAULT_SHELL, |m| shell(m.art().colour, Finish::Solid)),
    }
}

pub fn gba_shell_for(code: &str) -> Shell {
    by_code(code, EXACT)
        .or_else(|| famicom_shell(code))
        .unwrap_or_else(|| lookup(code, EXACT, FAMILY))
}

/// The NES re-releases all have an `F` code: Japan's Famicom Mini, the West's Classic NES Series.
fn famicom_shell(code: &str) -> Option<Shell> {
    if code.len() != 4 || !code.starts_with('F') {
        return None;
    }
    Some(if !code.ends_with('J') {
        CLASSIC_NES
    } else if DISK_SYSTEM_CODES.contains(&code) {
        DISK_SYSTEM
    } else {
        FAMICOM_MINI
    })
}

fn gb_shell_for(rom: &Path) -> Shell {
    let Some(header) = slot_store::gb::header(rom) else {
        return flag_shell(slot_store::gb::class(rom));
    };
    gb_shell_from_header(&header)
}

/// Canonical automatic GB/GBC cartridge material for both direct ROM files and
/// Android SAF/ZIP indexes. Uses the same Nintendo header as Cart Studio.
pub fn gb_shell_from_header(h: &slot_store::gb::Header) -> Shell {
    let by_title = || {
        GB_OVERSEAS_TITLES
            .iter()
            .find(|(t, _)| !h.japan && *t == h.title)
            .map(|(_, s)| *s)
    };
    by_code(&h.code, GB_CODES)
        .or_else(by_title)
        .unwrap_or_else(|| flag_shell(h.class()))
}

fn flag_shell(class: Class) -> Shell {
    match class {
        Class::Original => DMG_SHELL,
        Class::DualMode => DUAL_MODE_SHELL,
        Class::ColourOnly => GB_CLEAR_SHELL,
    }
}

pub fn table_keys() -> Vec<&'static str> {
    EXACT.iter().map(|(k, _)| *k).collect()
}

pub fn gb_table_shells() -> Vec<Shell> {
    GB_CODES
        .iter()
        .chain(GB_OVERSEAS_TITLES)
        .map(|(_, s)| *s)
        .collect()
}

pub fn lookup_order_is_exact_then_family_then_default() -> bool {
    const A: Shell = shell([1, 1, 1], Finish::Solid);
    const B: Shell = shell([2, 2, 2], Finish::Solid);
    let exact = [("MSK", A)];
    let family = [(b'M', B)];
    lookup("MSKE", &exact, &family) == A
        && lookup("MPOE", &exact, &family) == B
        && lookup("ZZZZ", &exact, &family) == DEFAULT_SHELL
}

fn lookup(code: &str, exact: &[(&str, Shell)], family: &[(u8, Shell)]) -> Shell {
    if let Some(s) = by_code(code, exact) {
        return s;
    }
    if let Some(first) = code.as_bytes().first() {
        if let Some((_, s)) = family.iter().find(|(k, _)| k == first) {
            return *s;
        }
    }
    DEFAULT_SHELL
}

fn by_code(code: &str, table: &[(&str, Shell)]) -> Option<Shell> {
    let whole: String = code.chars().take(4).collect();
    let prefix: String = code.chars().take(3).collect();
    [whole, prefix]
        .iter()
        .find_map(|key| table.iter().find(|(k, _)| k == key).map(|(_, s)| *s))
}

pub fn shell_presets() -> Vec<(&'static str, Shell)> {
    const GBA: &[(&str, &str)] = &[
        ("Pokémon Ruby", "AXV"),
        ("Pokémon Sapphire", "AXP"),
        ("Pokémon Emerald", "BPE"),
        ("Pokémon FireRed", "BPR"),
        ("Pokémon LeafGreen", "BPG"),
        ("Boktai clear", "U3I"),
        ("Drill Dozer red", "V49"),
        ("Yoshi green", "KYGE"),
        ("WarioWare smoke", "RZW"),
        ("Made in Wario white", "RZWJ"),
    ];
    const GB: &[(&str, &str)] = &[
        ("Pokémon Gold", "AAU"),
        ("Pokémon Silver", "AAX"),
        ("Pokémon Crystal", "BYT"),
        ("Kirby pink", "KTN"),
        ("Command Master turquoise", "KCE"),
        ("Chee-Chai Alien orange", "VCA"),
        ("Pinball yellow", "VPHJ"),
    ];
    let mut out = vec![
        ("Game Boy grey", DMG_SHELL),
        ("Game Boy black", DUAL_MODE_SHELL),
        ("Game Boy Color clear", GB_CLEAR_SHELL),
        ("Advance charcoal", DEFAULT_SHELL),
        ("Advance Video grey", FAMILY[0].1),
    ];
    out.extend(
        GBA.iter()
            .filter_map(|(n, k)| Some((*n, by_code(k, EXACT)?))),
    );
    out.extend(
        GB.iter()
            .filter_map(|(n, k)| Some((*n, by_code(k, GB_CODES)?))),
    );
    out.extend(
        [
            ("Pokémon Red", "POKEMON RED"),
            ("Pokémon Blue", "POKEMON BLU"),
            ("Pokémon Yellow", "POKEMON YEL"),
        ]
        .iter()
        .filter_map(|(n, t)| {
            let s = GB_OVERSEAS_TITLES.iter().find(|(k, _)| k == t)?.1;
            Some((*n, s))
        }),
    );
    out
}

/// One colour of Cart Studio's colour chart: its name, its plastic and the
/// finish the real cart had in it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Swatch {
    pub name: &'static str,
    pub shell: Shell,
}

const fn swatch(name: &'static str, colour: [u8; 3], finish: Finish) -> Swatch {
    Swatch { name, shell: Shell { colour, finish } }
}

const GB_SWATCHES: &[Swatch] = &[
    swatch("Game Boy grey", DMG_SHELL.colour, Finish::Solid),
    swatch("Black", DUAL_MODE_SHELL.colour, Finish::Solid),
    swatch("Clear", GB_CLEAR_SHELL.colour, Finish::Translucent),
    swatch("Gold", [0xb3, 0x8b, 0x3a], Finish::Solid),
    swatch("Silver", [0xa9, 0xaa, 0xa7], Finish::Solid),
    swatch("Crystal", CRYSTAL.colour, Finish::Glitter),
    swatch("Kirby pink", KIRBY.colour, Finish::Translucent),
    swatch("Turquoise", [0x1f, 0x9f, 0xb6], Finish::Translucent),
    swatch("Orange", [0xf0, 0xa9, 0x5e], Finish::Translucent),
    swatch("Pinball yellow", PINBALL.colour, Finish::Solid),
    swatch("Banana yellow", [0xf2, 0xc4, 0x1b], Finish::Solid),
];

const GBA_SWATCHES: &[Swatch] = &[
    swatch("Charcoal", DEFAULT_SHELL.colour, Finish::Solid),
    swatch("Video grey", [0xc6, 0xc6, 0xc9], Finish::Solid),
    swatch("Ruby", [0xc2, 0x33, 0x2e], Finish::Translucent),
    swatch("Sapphire", [0x2f, 0x5c, 0xc0], Finish::Translucent),
    swatch("Emerald", [0x24, 0x9c, 0x60], Finish::Translucent),
    swatch("FireRed", [0xd8, 0x52, 0x24], Finish::Solid),
    swatch("LeafGreen", [0x63, 0xb0, 0x44], Finish::Solid),
    swatch("Clear", CLEAR.colour, Finish::Translucent),
    swatch("Brick red", [0xa9, 0x51, 0x3b], Finish::Solid),
    swatch("Yoshi green", YOSHI.colour, Finish::Solid),
    swatch("Smoke", [0x5f, 0x62, 0x64], Finish::Translucent),
    swatch("White", [0xec, 0xee, 0xe8], Finish::Solid),
    swatch("NES grey", CLASSIC_NES.colour, Finish::Solid),
    swatch("Famicom red", FAMICOM_MINI.colour, Finish::Solid),
    swatch("Disk yellow", DISK_SYSTEM.colour, Finish::Solid),
];

const NES_SWATCHES: &[Swatch] = &[
    swatch("NES grey", [126, 126, 128], Finish::Solid),
    swatch("Zelda gold", [158, 133, 54], Finish::Glitter),
    swatch("Black", [16, 16, 16], Finish::Solid),
    swatch("White", [0xd7, 0xd7, 0xd7], Finish::Solid),
    swatch("Mario orange", [196, 125, 8], Finish::Solid),
    swatch("Navy", [0x13, 0x32, 0x66], Finish::Solid),
    swatch("Yellow", [0xde, 0xd5, 0x01], Finish::Solid),
    swatch("Dark grey", [0x5a, 0x5a, 0x5a], Finish::Solid),
    swatch("Wine", [0x8d, 0x28, 0x42], Finish::Solid),
    swatch("Mint", [0x57, 0xb3, 0x9f], Finish::Solid),
    swatch("Blue", [0x34, 0x73, 0xa1], Finish::Solid),
    swatch("Green", [0x01, 0x9f, 0x56], Finish::Solid),
    swatch("Sky", [0x9e, 0xf0, 0xf0], Finish::Solid),
    swatch("Rose", [0xbb, 0x71, 0x81], Finish::Solid),
    swatch("Red", [0x89, 0x22, 0x22], Finish::Solid),
];

const SNES_SWATCHES: &[Swatch] = &[
    swatch("Super Nintendo grey", [162, 157, 163], Finish::Solid),
    swatch("Killer black", [17, 16, 17], Finish::Solid),
];

const MD_SWATCHES: &[Swatch] = &[
    swatch("Mega Drive black", [18, 18, 19], Finish::Solid),
];

/// Colours no real cart came in, for fun, on every system.
pub const FUN_SWATCHES: &[Swatch] = &[
    swatch("Cherry", [0xe0, 0x28, 0x3a], Finish::Solid),
    swatch("Tangerine", [0xf5, 0x7f, 0x17], Finish::Solid),
    swatch("Lemon", [0xf5, 0xd9, 0x0a], Finish::Solid),
    swatch("Lime", [0x8b, 0xc3, 0x4a], Finish::Solid),
    swatch("Teal", [0x00, 0xa5, 0xa8], Finish::Solid),
    swatch("Ocean", [0x1e, 0x63, 0xc9], Finish::Solid),
    swatch("Grape", [0x6a, 0x3e, 0xa8], Finish::Solid),
    swatch("Bubblegum", [0xf0, 0x6a, 0xa8], Finish::Solid),
    swatch("Snow", [0xf2, 0xf1, 0xec], Finish::Solid),
    swatch("Midnight", [0x1c, 0x1c, 0x24], Finish::Solid),
];

/// The colours real carts of this system came in, the commonest first.
pub fn system_swatches(platform: Platform) -> &'static [Swatch] {
    match platform {
        Platform::Gb | Platform::Gbc => GB_SWATCHES,
        Platform::Gba => GBA_SWATCHES,
        Platform::Nes => NES_SWATCHES,
        Platform::Snes => SNES_SWATCHES,
        Platform::Md => MD_SWATCHES,
    }
}
