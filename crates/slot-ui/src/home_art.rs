//! The NES / Famicom, Super Nintendo and Mega Drive moulds: their art, where
//! things sit on them, and which one (and which colour) a game's cart came in.
//!
//! Nothing here depends on slot-store, so Cart Studio compiles this very file
//! (by `#[path]`) and draws the same shells, wells and automatic choices the
//! device does. It needs only `crate::cart::label_tags`, which both have.

use std::collections::HashMap;
use std::sync::OnceLock;

use unicode_normalization::UnicodeNormalization;

/// The three home consoles, and the 3.0 systems on the test shelf.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum HomeSystem {
    Nes,
    Snes,
    Md,
    NeoGeoPocket,
    WonderSwan,
    GameGear,
    MasterSystem,
    Lynx,
    PokemonMini,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum HomeMould {
    Nes,
    Famicom,
    FamicomNamco,
    FamicomKonami,
    Snes,
    SnesUs,
    MegaDrive,
    MegaDriveJp,
    MegaDriveEa,
    MegaDriveAccolade,
    MegaDriveCodemasters,
    /// Virtua Racing's big cart.
    MegaDriveVr,
    /// Sonic & Knuckles' lock-on cart.
    MegaDriveSk,
    // The 3.0 systems (3.0.0-cartes1: on the test shelf only).
    NeoGeoPocket,
    WonderSwan,
    WonderSwanColor,
    GameGear,
    MasterSystem,
    Lynx,
    PokemonMini,
}

/// A mould's art and where things sit on it, in its logical pixels.
pub(crate) struct MouldArt {
    pub h: u32,
    /// The label well: x, y, w, h.
    pub label: (u32, u32, u32, u32),
    /// The two big-chip footprints (left, top, w, h): the cores' sockets.
    pub footprints: [(f32, f32, f32, f32); 2],
    /// The small chip the game's title is printed on.
    pub title: (f32, f32, f32, f32),
    /// The plain shell's colour when nothing names another.
    pub colour: [u8; 3],
    pub neutral: &'static [u8],
    pub decal: Option<&'static [u8]>,
    pub shell_svg: &'static str,
    pub board: &'static [u8],
    pub board_battery: &'static [u8],
    pub chip_svg: &'static str,
    /// The outline when the shell leaves part of it open (the WonderSwan's
    /// and Lynx's exposed contacts, drawn by the decal): else the neutral
    /// shell's own alpha.
    pub silhouette: Option<&'static [u8]>,
    /// How opaque a clear shell is, texel by texel (its red channel), where
    /// it is not the same 80% everywhere (the Pokémon mini's frosted cap).
    pub opacity: Option<&'static [u8]>,
    /// One core only: the board has no empty footprints and never opens.
    pub single_core: bool,
    /// The front that lifts off when the cart opens, where it differs from
    /// the outline: the Lynx's contacts stay on the board (alpha only).
    pub lid: Option<&'static [u8]>,
}

pub const HOME_CART_W: u32 = 312;
/// Boards are drawn at three times the logical size, for 3x panels.
pub const BOARD_K: u32 = 3;
/// So are the front shells and decals (Siham 2026-11-03).
pub const FRONT_K: u32 = 3;

macro_rules! art {
    ($name:literal, $h:expr, $label:expr, $fp:expr, $title:expr, $colour:expr, $decal:expr) => {
        MouldArt {
            h: $h,
            label: $label,
            footprints: $fp,
            title: $title,
            colour: $colour,
            neutral: include_bytes!(concat!("../../../assets/home/", $name, "_neutral.png")),
            decal: $decal,
            shell_svg: include_str!(concat!("../../../assets/home/", $name, "_shell.svg")),
            board: include_bytes!(concat!("../../../assets/home/", $name, "_board.png")),
            board_battery: include_bytes!(concat!("../../../assets/home/", $name, "_board_battery.png")),
            chip_svg: include_str!(concat!("../../../assets/home/", $name, "_chip.svg")),
            silhouette: None,
            opacity: None,
            single_core: false,
            lid: None,
        }
    };
    ($name:literal, $h:expr, $label:expr, $fp:expr, $title:expr, $colour:expr, $decal:expr,
     $silhouette:expr, $opacity:expr, $single:expr, $lid:expr) => {
        MouldArt {
            lid: $lid,
            ..art!($name, $h, $label, $fp, $title, $colour, $decal, $silhouette, $opacity, $single)
        }
    };
    ($name:literal, $h:expr, $label:expr, $fp:expr, $title:expr, $colour:expr, $decal:expr,
     $silhouette:expr, $opacity:expr, $single:expr) => {
        MouldArt {
            silhouette: $silhouette,
            opacity: $opacity,
            single_core: $single,
            ..art!($name, $h, $label, $fp, $title, $colour, $decal)
        }
    };
}

// Printed by nes-megadrive/export_app.py.
static ARTS: [MouldArt; 20] = [
    art!("nes", 352, (126, 0, 145, 239),
        [(40.6, 234.6, 101.0, 46.5), (170.4, 234.6, 101.0, 46.5)],
        (166.3, 103.6, 121.6, 39.5), [126, 126, 128], None),
    art!("fc", 207, (22, 22, 269, 133),
        [(32.1, 96.7, 109.0, 53.0), (170.9, 96.7, 109.0, 53.0)],
        (72.2, 50.6, 135.4, 31.6), [196, 125, 8], None),
    art!("fc_namco", 204, (14, 33, 280, 138),
        [(32.1, 93.0, 109.0, 53.0), (170.9, 93.0, 109.0, 53.0)],
        (72.2, 46.9, 135.4, 31.6), [18, 18, 18], None),
    art!("fc_konami", 294, (27, 25, 214, 159),
        [(32.6, 183.4, 108.8, 52.9), (171.2, 183.4, 108.8, 52.9)],
        (72.7, 137.5, 135.1, 31.5), [12, 21, 34], None),
    art!("snes", 215, (22, 11, 269, 92),
        [(141.4, 82.7, 58.4, 53.1), (216.5, 82.7, 58.4, 53.1)],
        (45.4, 76.3, 76.0, 29.6), [162, 157, 163], None),
    art!("snes_us", 200, (63, 0, 185, 84),
        [(141.7, 81.8, 57.3, 52.1), (215.4, 81.8, 57.3, 52.1)],
        (47.5, 75.5, 74.5, 29.0), [163, 156, 163], None),
    art!("md", 210, (48, 0, 216, 179),
        [(52.1, 82.9, 103.6, 51.9), (164.5, 82.9, 103.6, 51.9)],
        (153.2, 26.6, 108.6, 30.9), [18, 18, 19], None),
    art!("md_jp", 209, (39, 0, 232, 175),
        [(52.0, 82.6, 103.7, 51.8), (164.5, 82.6, 103.7, 51.8)],
        (153.2, 26.5, 108.7, 30.8), [12, 12, 12], None),
    art!("md_ea", 290, (62, 0, 218, 240),
        [(52.1, 149.8, 103.6, 58.3), (164.5, 149.8, 103.6, 58.3)],
        (153.2, 86.6, 108.6, 34.7), [14, 14, 15],
        Some(include_bytes!("../../../assets/home/md_ea_decal.png"))),
    // Printed by nes-megadrive/export_new.py (2026-11-10).
    art!("md_accolade", 285, (42, 0, 229, 221),
        [(54.4, 144.9, 101.0, 56.9), (164.1, 144.9, 101.0, 56.9)],
        (153.0, 83.2, 106.0, 33.9), [14, 14, 15], None),
    art!("md_codemasters", 246, (52, 0, 214, 211),
        [(74.7, 131.4, 81.3, 45.8), (163.0, 131.4, 81.3, 45.8)],
        (154.0, 81.8, 85.3, 27.3), [14, 14, 15], None),
    art!("md_vr", 306, (34, 48, 248, 118),
        [(62.6, 175.1, 93.1, 52.5), (163.7, 175.1, 93.1, 52.5)],
        (153.4, 118.3, 97.6, 31.2), [18, 19, 19], None),
    art!("md_sk", 216, (17, 52, 276, 29),
        [(66.3, 88.7, 88.6, 49.9), (162.5, 88.7, 88.6, 49.9)],
        (152.8, 34.6, 93.0, 29.7), [14, 14, 15], None),
    // The 3.0 systems, from the 3.0 art package and slot.'s own boards
    // (3.0.0-cartes1).
    art!("ngp", 280, (31, 66, 252, 138),
        [(46.0, 132.0, 98.0, 64.0), (168.0, 132.0, 98.0, 64.0)],
        (150.0, 58.0, 100.0, 38.0), [18, 18, 18],
        None,
        None,
        None, false),
    art!("ws", 188, (10, 26, 293, 108),
        [(30.0, 30.0, 110.0, 54.0), (30.0, 30.0, 110.0, 54.0)],
        (30.0, 30.0, 110.0, 54.0), [30, 30, 30],
        Some(include_bytes!("../../../assets/home/ws_decal.png")),
        Some(include_bytes!("../../../assets/home/ws_silhouette.png")),
        None, true),
    art!("wsc", 188, (10, 26, 293, 108),
        [(30.0, 30.0, 110.0, 54.0), (30.0, 30.0, 110.0, 54.0)],
        (30.0, 30.0, 110.0, 54.0), [145, 148, 148],
        Some(include_bytes!("../../../assets/home/wsc_decal.png")),
        Some(include_bytes!("../../../assets/home/wsc_silhouette.png")),
        Some(include_bytes!("../../../assets/home/wsc_opacity.png")), true),
    art!("gg", 316, (28, 58, 256, 185),
        [(52.0, 150.0, 98.0, 64.0), (162.0, 150.0, 98.0, 64.0)],
        (150.0, 64.0, 102.0, 40.0), [20, 20, 20],
        None,
        None,
        None, false),
    art!("sms", 200, (8, 4, 296, 42),
        [(28.0, 66.0, 92.0, 58.0), (136.0, 66.0, 92.0, 58.0)],
        (180.0, 20.0, 102.0, 32.0), [18, 18, 18],
        None,
        None,
        None, false),
    art!("lynx", 326, (5, 69, 302, 211),
        [(36.0, 150.0, 110.0, 66.0), (166.0, 150.0, 110.0, 66.0)],
        (150.0, 64.0, 122.0, 40.0), [25, 25, 25],
        Some(include_bytes!("../../../assets/home/lynx_decal.png")),
        Some(include_bytes!("../../../assets/home/lynx_silhouette.png")),
        None, false,
        Some(include_bytes!("../../../assets/home/lynx_lid.png"))),
    art!("pokemini", 300, (19, 107, 274, 149),
        [(82.0, 28.0, 150.0, 64.0), (82.0, 28.0, 150.0, 64.0)],
        (0.0, 0.0, 0.0, 0.0), [128, 150, 178],
        Some(include_bytes!("../../../assets/home/pokemini_decal.png")),
        None,
        Some(include_bytes!("../../../assets/home/pokemini_opacity.png")), true),
];


impl HomeMould {
    pub const ALL: [HomeMould; 20] = [
        HomeMould::Nes,
        HomeMould::Famicom,
        HomeMould::FamicomNamco,
        HomeMould::FamicomKonami,
        HomeMould::Snes,
        HomeMould::SnesUs,
        HomeMould::MegaDrive,
        HomeMould::MegaDriveJp,
        HomeMould::MegaDriveEa,
        HomeMould::MegaDriveAccolade,
        HomeMould::MegaDriveCodemasters,
        HomeMould::MegaDriveVr,
        HomeMould::MegaDriveSk,
        HomeMould::NeoGeoPocket,
        HomeMould::WonderSwan,
        HomeMould::WonderSwanColor,
        HomeMould::GameGear,
        HomeMould::MasterSystem,
        HomeMould::Lynx,
        HomeMould::PokemonMini,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub(crate) fn art(self) -> &'static MouldArt {
        &ARTS[self.index()]
    }

    pub fn size(self) -> (u32, u32) {
        (HOME_CART_W, self.art().h)
    }

    pub fn system(self) -> HomeSystem {
        match self {
            HomeMould::Nes | HomeMould::Famicom | HomeMould::FamicomNamco | HomeMould::FamicomKonami => {
                HomeSystem::Nes
            }
            HomeMould::Snes | HomeMould::SnesUs => HomeSystem::Snes,
            HomeMould::MegaDrive
            | HomeMould::MegaDriveJp
            | HomeMould::MegaDriveEa
            | HomeMould::MegaDriveAccolade
            | HomeMould::MegaDriveCodemasters
            | HomeMould::MegaDriveVr
            | HomeMould::MegaDriveSk => HomeSystem::Md,
            HomeMould::NeoGeoPocket => HomeSystem::NeoGeoPocket,
            HomeMould::WonderSwan | HomeMould::WonderSwanColor => HomeSystem::WonderSwan,
            HomeMould::GameGear => HomeSystem::GameGear,
            HomeMould::MasterSystem => HomeSystem::MasterSystem,
            HomeMould::Lynx => HomeSystem::Lynx,
            HomeMould::PokemonMini => HomeSystem::PokemonMini,
        }
    }

    /// One of the 3.0 systems, which slot. shows but does not play yet.
    pub fn is_preview(self) -> bool {
        !matches!(self.system(), HomeSystem::Nes | HomeSystem::Snes | HomeSystem::Md)
    }

    /// The shell a system's carts get when nothing picks another.
    pub fn default_of(system: HomeSystem) -> HomeMould {
        match system {
            HomeSystem::Nes => HomeMould::Nes,
            HomeSystem::Snes => HomeMould::Snes,
            HomeSystem::Md => HomeMould::MegaDrive,
            HomeSystem::NeoGeoPocket => HomeMould::NeoGeoPocket,
            HomeSystem::WonderSwan => HomeMould::WonderSwan,
            HomeSystem::GameGear => HomeMould::GameGear,
            HomeSystem::MasterSystem => HomeMould::MasterSystem,
            HomeSystem::Lynx => HomeMould::Lynx,
            HomeSystem::PokemonMini => HomeMould::PokemonMini,
        }
    }

    /// The outline's name in `cart_shell.txt`.
    pub fn name(self) -> &'static str {
        match self {
            HomeMould::Nes => "nes",
            HomeMould::Famicom => "famicom",
            HomeMould::FamicomNamco => "famicom-namco",
            HomeMould::FamicomKonami => "famicom-konami",
            HomeMould::Snes => "snes",
            HomeMould::SnesUs => "snes-us",
            HomeMould::MegaDrive => "megadrive",
            HomeMould::MegaDriveJp => "megadrive-jp",
            HomeMould::MegaDriveEa => "megadrive-ea",
            HomeMould::MegaDriveAccolade => "megadrive-accolade",
            HomeMould::MegaDriveCodemasters => "megadrive-codemasters",
            HomeMould::MegaDriveVr => "megadrive-vr",
            HomeMould::MegaDriveSk => "megadrive-sk",
            HomeMould::NeoGeoPocket => "ngp",
            HomeMould::WonderSwan => "wonderswan",
            HomeMould::WonderSwanColor => "wonderswan-color",
            HomeMould::GameGear => "gamegear",
            HomeMould::MasterSystem => "mastersystem",
            HomeMould::Lynx => "lynx",
            HomeMould::PokemonMini => "pokemini",
        }
    }

    pub fn of_name(name: &str) -> Option<HomeMould> {
        HomeMould::ALL.into_iter().find(|m| m.name().eq_ignore_ascii_case(name))
    }
}

// ---------------------------------------------------------------- shells ---

/// The Legend of Zelda (and Zelda II) came in gold outside Japan.
pub(crate) const ZELDA_GOLD: [u8; 3] = [158, 133, 54];
const ZELDA_GOLD_KEYS: &[&str] = &["legendofzeldathe", "thelegendofzelda", "zeldaiitheadventureoflink"];
pub(crate) const KILLER_INSTINCT: [u8; 3] = [17, 16, 17];
/// Unknown Famicom carts: the most common shell, black.
const FAMICOM_BLACK: [u8; 3] = [16, 16, 16];
/// Super Mario Bros.' own Famicom shell, measured from its support-2D.
const FAMICOM_SMB: [u8; 3] = [196, 125, 8];

const FAMICOM_TSV: &str = include_str!("../../../assets/home/famicom.tsv");
/// Carts outside Japan whose shell is not the console's grey (Tengen, Color
/// Dreams, Camerica, Wisdom Tree, red Doom, EA's Mega Drive mould...), read from ScreenScraper's
/// support-2D: system, mould, title key, region (us, eu or any), colour, finish.
const SHELLS_TSV: &str = include_str!("../assets/home_shells.tsv");

/// A game's title as the shell tables key it: the No-Intro name without its
/// tags, lowercased, letters and digits only.
pub fn title_key(name: &str) -> String {
    let mut bare = String::with_capacity(name.len());
    let mut depth = 0u32;
    for ch in name.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => bare.push(ch),
            _ => {}
        }
    }
    bare.nfkd()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn famicom_table() -> &'static HashMap<&'static str, ([u8; 3], HomeMould)> {
    static TABLE: OnceLock<HashMap<&'static str, ([u8; 3], HomeMould)>> = OnceLock::new();
    TABLE.get_or_init(|| {
        FAMICOM_TSV
            .lines()
            .filter_map(|line| {
                let mut cols = line.split('\t');
                let key = cols.next()?;
                let hex = cols.next()?;
                let mould = match cols.next()? {
                    "namco" => HomeMould::FamicomNamco,
                    "konami" => HomeMould::FamicomKonami,
                    _ => HomeMould::Famicom,
                };
                if hex.len() != 6 {
                    return None;
                }
                let colour = [0, 2, 4].map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0));
                Some((key, (colour, mould)))
            })
            .collect()
    })
}

/// A game's own shell on a mould, from `home_shells.tsv`: colour and glitter.
/// Some titles differ by region (Camerica's US carts, Codemasters' in Europe).
fn own_shell(mould: HomeMould, key: &str, usa: bool) -> Option<([u8; 3], bool)> {
    type Table = HashMap<(&'static str, &'static str, &'static str), ([u8; 3], bool)>;
    static TABLE: OnceLock<Table> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        SHELLS_TSV
            .lines()
            .filter_map(|line| {
                let mut cols = line.split('\t');
                let _system = cols.next()?;
                let mould = cols.next()?;
                let key = cols.next()?;
                let region = cols.next()?;
                let hex = cols.next()?;
                let glitter = cols.next()? == "glitter";
                if hex.len() != 6 {
                    return None;
                }
                let colour = [0, 2, 4].map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0));
                Some(((mould, key, region), (colour, glitter)))
            })
            .collect()
    });
    let region = if usa { "us" } else { "eu" };
    table
        .get(&(mould.name(), key, region))
        .or_else(|| table.get(&(mould.name(), key, "any")))
        .copied()
}

/// Which regions a No-Intro / GoodTools name lists: (Japan, USA, Europe),
/// None when it lists none.
fn name_regions(stem: &str) -> Option<(bool, bool, bool)> {
    let mut found = None::<(bool, bool, bool)>;
    for tag in crate::cart::label_tags(stem) {
        for word in tag.split(',') {
            let r = found.get_or_insert((false, false, false));
            match word.trim().to_ascii_lowercase().as_str() {
                "japan" | "j" | "jp" | "jpn" | "asia" => r.0 = true,
                "usa" | "u" | "us" | "canada" | "world" | "w" => r.1 = true,
                "europe" | "e" | "eu" | "pal" | "france" | "germany" | "spain" | "italy" | "uk"
                | "australia" | "sweden" | "netherlands" | "f" | "g" | "scandinavia" | "brazil" => {
                    r.2 = true
                }
                _ => {}
            }
        }
    }
    found.filter(|r| r.0 || r.1 || r.2)
}

/// The automatic shell of a home console cart, as the real one: the
/// region's mould (Famicom, the US Super Nintendo, the Japanese Mega Drive),
/// a publisher's own (Namco and Konami Famicom carts, Electronic Arts in the
/// US) and the game's colour where it is known. The bool is a glitter finish.
///
/// `region` is the header's (J, U, E or empty) and only counts when the
/// file name lists no region; `maker` is the Mega Drive copyright field.
pub fn home_pick(system: HomeSystem, stem: &str, region: &str, maker: &str) -> (HomeMould, [u8; 3], bool) {
    let (japan_only, usa) = match name_regions(stem) {
        Some((j, u, e)) => (j && !u && !e, u),
        None => (region == "J", region == "U"),
    };
    let key = title_key(stem);
    match system {
        HomeSystem::Nes if japan_only => {
            let (colour, mould) = famicom_table().get(key.as_str()).copied().unwrap_or(
                if key == "supermariobros" {
                    (FAMICOM_SMB, HomeMould::Famicom)
                } else {
                    (FAMICOM_BLACK, HomeMould::Famicom)
                },
            );
            (mould, colour, false)
        }
        HomeSystem::Nes if ZELDA_GOLD_KEYS.contains(&key.as_str()) => (HomeMould::Nes, ZELDA_GOLD, true),
        HomeSystem::Nes => {
            let (colour, glitter) = own_shell(HomeMould::Nes, &key, usa).unwrap_or((HomeMould::Nes.art().colour, false));
            (HomeMould::Nes, colour, glitter)
        }
        HomeSystem::Snes => {
            let mould = if usa { HomeMould::SnesUs } else { HomeMould::Snes };
            let (colour, glitter) = if key == "killerinstinct" {
                (KILLER_INSTINCT, false)
            } else {
                own_shell(mould, &key, usa).unwrap_or((mould.art().colour, false))
            };
            (mould, colour, glitter)
        }
        HomeSystem::Md => {
            let maker = maker.to_ascii_uppercase();
            let ea = maker.contains("T-50") || maker.contains("ELECTRONIC ARTS");
            let mould = if japan_only {
                HomeMould::MegaDriveJp
            } else if ea && usa {
                HomeMould::MegaDriveEa
            } else {
                // EA's tall cart (also in Europe, and other publishers' games in
                // it: Zool, James Pond...), Accolade's, Codemasters', Virtua
                // Racing's and Sonic & Knuckles', as ScreenScraper shows them.
                [
                    HomeMould::MegaDriveEa,
                    HomeMould::MegaDriveAccolade,
                    HomeMould::MegaDriveCodemasters,
                    HomeMould::MegaDriveVr,
                    HomeMould::MegaDriveSk,
                ]
                .into_iter()
                .find(|m| own_shell(*m, &key, usa).is_some())
                .unwrap_or(HomeMould::MegaDrive)
            };
            let (colour, glitter) = own_shell(mould, &key, usa).unwrap_or((mould.art().colour, false));
            (mould, colour, glitter)
        }
        _ => {
            let mould = HomeMould::default_of(system);
            (mould, mould.art().colour, false)
        }
    }
}
