use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};
use slot_gfx::ScreenEffect;
use slot_store::GbPalette;

/// Matches Slot v1.7.0's canonical WallpaperPick semantics, while storing
/// the user's choice in the existing Android JSON settings file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallpaperPick {
    None,
    Random,
    /// Upstream Slot v1.13: Wallpapers/GBA.png, GB.png, GBC.png,
    /// Recents.png or Favorites.png follow the shelf on screen.
    Shelf,
    #[default]
    Default,
    Blur,
    File(String),
}
impl WallpaperPick {
    pub fn choices(root: &Path) -> Vec<Self> {
        let mut choices=vec![Self::None, Self::Random, Self::Shelf, Self::Default, Self::Blur];
        choices.extend(crate::wallpaper::list(root).into_iter().map(Self::File));
        choices
    }
    pub fn step(&self, root: &Path, right: bool) -> Self {
        let choices=Self::choices(root);
        let at=choices.iter().position(|v| v==self).unwrap_or(3);
        let n=choices.len();
        choices[if right {(at+1)%n}else{(at+n-1)%n}].clone()
    }
    pub fn label(&self) -> String {
        match self {
            Self::None=>"None".into(),
            Self::Random=>"Random".into(),
            Self::Shelf=>"Shelf".into(),
            Self::Default=>"Default".into(),
            Self::Blur=>"Default Blur".into(),
            // The imported image reads "User", like the other values.
            Self::File(name)=>match name.rsplit_once('.').map(|(a,_)|a).unwrap_or(name) {
                stem if stem.eq_ignore_ascii_case("user") => "User".into(),
                stem => stem.into(),
            }
        }
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallpaperTint { #[default] Off, Low, Medium, High }
impl WallpaperTint {
    pub const ALL: [Self; 4]=[Self::Off, Self::Low, Self::Medium, Self::High];
    pub fn step(self, right: bool) -> Self {
        let at=Self::ALL.iter().position(|v|*v==self).unwrap_or(0);
        let n=Self::ALL.len();
        Self::ALL[if right {(at+1)%n}else{(at+n-1)%n}]
    }
    pub fn alpha(self)->f32 {
        // Exact Slot v1.7.0 app.rs tint_alpha().
        match self { Self::Off=>0.0, Self::Low=>0.25, Self::Medium=>0.45, Self::High=>0.65 }
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance { #[default] Dark, Light, Original }
impl Appearance {
    /// Dark, Light, then Original: the dark interface with every console
    /// bar in its console's own colours (Siham 2026-11-03).
    pub fn toggle(self)->Self {
        match self { Self::Dark=>Self::Light, Self::Light=>Self::Original, Self::Original=>Self::Dark }
    }
    /// Which set of console bars `slot_ui::bitmaps::home_slot` draws.
    pub fn bar_theme(self)->usize {
        match self { Self::Original=>0, Self::Dark=>1, Self::Light=>2 }
    }
}

/// Original PR #2 by Its-TroyM: persisted optional 12/24-hour shelf clock.
/// Frame Blending: each picture mixed with the one before, as the slow
/// LCDs of the Game Boys showed sprites a game flickers every other frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Blending {
    #[default]
    Off,
    Mix,
    /// Mix plus the fading trail of an LCD.
    Ghosting,
}

/// The upscaler of GBA Video carts, drawn before their shader: Bilinear,
/// AMD's FSR 1 or bloc97's Anime4K (Siham 2026-11-10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Upscaler {
    Off,
    Bilinear,
    #[default]
    Fsr,
    Anime4kFast,
    Anime4k,
}

impl Upscaler {
    pub const ALL: [Upscaler; 5] = [Upscaler::Off, Upscaler::Bilinear, Upscaler::Fsr,
        Upscaler::Anime4kFast, Upscaler::Anime4k];
    pub fn step(self, right: bool) -> Upscaler {
        let n = Self::ALL.len();
        let at = Self::ALL.iter().position(|u| *u == self).unwrap_or(0);
        Self::ALL[if right { (at + 1) % n } else { (at + n - 1) % n }]
    }
    pub fn gpu(self) -> slot_gfx::Upscaler {
        match self {
            Upscaler::Off => slot_gfx::Upscaler::Off,
            Upscaler::Bilinear => slot_gfx::Upscaler::Bilinear,
            Upscaler::Fsr => slot_gfx::Upscaler::Fsr,
            Upscaler::Anime4kFast => slot_gfx::Upscaler::Anime4kFast,
            Upscaler::Anime4k => slot_gfx::Upscaler::Anime4k,
        }
    }
}

/// LCD Effect of the Game Boy family: Matt Akins' Pixel Transparency or
/// Sunlight Shimmer over the shader (Siham 2026-11-09).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LcdEffect {
    #[default]
    Off,
    Transparency,
    Sunlight,
}

impl LcdEffect {
    pub fn step(self, right: bool) -> LcdEffect {
        const ALL: [LcdEffect; 3] = [LcdEffect::Off, LcdEffect::Transparency, LcdEffect::Sunlight];
        let at = ALL.iter().position(|b| *b == self).unwrap_or(0);
        ALL[if right { (at + 1) % 3 } else { (at + 2) % 3 }]
    }
    pub fn gpu(self) -> slot_gfx::LcdEffect {
        match self {
            LcdEffect::Off => slot_gfx::LcdEffect::Off,
            LcdEffect::Transparency => slot_gfx::LcdEffect::PixelTransparency,
            LcdEffect::Sunlight => slot_gfx::LcdEffect::SunlightShimmer,
        }
    }
}

impl Blending {
    pub fn step(self, right: bool) -> Blending {
        const ALL: [Blending; 3] = [Blending::Off, Blending::Mix, Blending::Ghosting];
        let at = ALL.iter().position(|b| *b == self).unwrap_or(0);
        ALL[if right { (at + 1) % 3 } else { (at + 2) % 3 }]
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeFormat {
    #[default]
    #[serde(rename = "24h")]
    TwentyFourHour,
    #[serde(rename = "12h")]
    TwelveHour,
}
impl TimeFormat {
    pub fn toggle(self) -> Self {
        match self {
            Self::TwentyFourHour => Self::TwelveHour,
            Self::TwelveHour => Self::TwentyFourHour,
        }
    }

    /// Android supplies local time as ASCII HH:mm, independent of the choice.
    pub fn format_clock(self, clock: &str) -> String {
        if self == Self::TwentyFourHour { return clock.to_owned(); }
        let time = clock.split_once(':').and_then(|(hour, minute)| {
            Some((hour.parse::<u8>().ok()?, minute.parse::<u8>().ok()?))
        });
        match time {
            Some((hour, minute)) if hour < 24 && minute < 60 => {
                let period = if hour < 12 { "AM" } else { "PM" };
                let display_hour = if hour % 12 == 0 { 12 } else { hour % 12 };
                format!("{display_hour}:{minute:02} {period}")
            }
            _ => clock.to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Bezel {
    None,
    #[default]
    Default,
    // Watomsk's overlays (github.com/Watomsk/Overlays, used with his
    // permission), recomposed for each screen shape. His dark grey versions
    // were dropped (Siham, 2026-11-04): a saved grey reads as the black one.
    #[serde(alias="gb_grey")] Gb,
    #[serde(alias="pocket_grey")] Pocket,
    #[serde(alias="color_grey")] Color,
    #[serde(alias="gba_classic_grey")] GbaClassic,
    #[serde(alias="gba_grey")] Gba,
    #[serde(alias="gba_sp_grey")] GbaSp,
    #[serde(alias="gba_alt_grey")] GbaAlt,
    #[serde(alias="nes_grey")] Nes,
    #[serde(alias="nes_gray_logo_grey")] NesGrayLogo,
    #[serde(alias="snes_grey")] Snes,
    #[serde(alias="snes_alt_grey")] SnesAlt,
    #[serde(alias="snes_gray_logo_grey")] SnesGrayLogo,
    #[serde(alias="snes_gray_logo_alt_grey")] SnesGrayLogoAlt,
    #[serde(alias="genesis_classic_grey")] GenesisClassic,
    #[serde(alias="genesis_grey")] Genesis,
    #[serde(alias="mega_drive_grey")] MegaDrive,
}
impl Bezel {
    /// GB and GBC also keep the slot. bezel; every system's own Watomsk
    /// designs follow, the classic one first (Siham, 2026-11-03).
    pub const GB: [Bezel; 4] = [Bezel::None, Bezel::Default, Bezel::Gb, Bezel::Pocket];
    pub const GBC: [Bezel; 3] = [Bezel::None, Bezel::Default, Bezel::Color];
    pub const GBA: [Bezel; 5] = [Bezel::None, Bezel::GbaClassic, Bezel::Gba, Bezel::GbaSp,
        Bezel::GbaAlt];
    pub const NES: [Bezel; 3] = [Bezel::None, Bezel::Nes, Bezel::NesGrayLogo];
    pub const SNES: [Bezel; 5] = [Bezel::None, Bezel::Snes, Bezel::SnesAlt,
        Bezel::SnesGrayLogo, Bezel::SnesGrayLogoAlt];
    pub const MD: [Bezel; 4] = [Bezel::None, Bezel::GenesisClassic, Bezel::Genesis,
        Bezel::MegaDrive];
    pub fn list(platform: slot_store::Platform) -> &'static [Bezel] {
        use slot_store::Platform;
        match platform {
            Platform::Gb => &Self::GB,
            Platform::Gbc => &Self::GBC,
            Platform::Gba => &Self::GBA,
            Platform::Nes => &Self::NES,
            Platform::Snes => &Self::SNES,
            Platform::Md => &Self::MD,
        }
    }
    pub fn step_in(self,list:&[Bezel],right:bool)->Self {
        let n=list.len();
        let at=list.iter().position(|b| *b==self).unwrap_or(0);
        list[if right {(at+1)%n} else {(at+n-1)%n}]
    }
    /// Which Watomsk design.
    pub fn watomsk(self) -> Option<slot_ui::watomsk::Design> {
        use slot_ui::watomsk::Design as D;
        Some(match self {
            Bezel::None | Bezel::Default => return None,
            Bezel::Gb => D::Gb, Bezel::Pocket => D::Pocket, Bezel::Color => D::Color,
            Bezel::GbaClassic => D::GbaClassic, Bezel::Gba => D::Gba,
            Bezel::GbaSp => D::GbaSp, Bezel::GbaAlt => D::GbaAlt,
            Bezel::Nes => D::Nes, Bezel::NesGrayLogo => D::NesGrayLogo,
            Bezel::Snes => D::Snes, Bezel::SnesAlt => D::SnesAlt,
            Bezel::SnesGrayLogo => D::SnesGrayLogo, Bezel::SnesGrayLogoAlt => D::SnesGrayLogoAlt,
            Bezel::GenesisClassic => D::GenesisClassic, Bezel::Genesis => D::Genesis,
            Bezel::MegaDrive => D::MegaDrive,
        })
    }
}

fn bezel_nes_default() -> Bezel { Bezel::Nes }
fn bezel_snes_default() -> Bezel { Bezel::Snes }
fn bezel_md_default() -> Bezel { Bezel::GenesisClassic }

pub const REWIND_SPEEDS: [u8; 4] = [1, 2, 3, 5];

/// Port: 1x means ten stored snapshots/sec, 2x (upstream feel) means twenty,
/// and 5x means fifty. Upstream captures 30/s, not 10/s.
/// Hold L2 continuously; the memory meter indicates occupied history, not progress.
pub fn rewind_step_seconds(speed: u8) -> f32 {
    0.1 / f32::from(if REWIND_SPEEDS.contains(&speed) { speed } else { 2 })
}

fn cycle_rewind_setting(enabled: &mut bool, speed: &mut u8, right: bool) {
    // Five values on the existing six-row Gameplay screen: OFF, 1x, 2x, 3x, 5x.
    // No new row means original Slot's menu geometry and keycaps are preserved.
    let at = if !*enabled { 0 } else {
        REWIND_SPEEDS.iter().position(|v| *v == *speed).map_or(4, |i| i + 1)
    };
    let next = if right { (at + 1) % 5 } else { (at + 4) % 5 };
    *enabled = next != 0;
    if *enabled { *speed = REWIND_SPEEDS[next-1]; }
}

/// Index of "GBC Green" in slot_store's palette list (the default before
/// TWB64 Game.com).
const GB_PALETTE_GBC_GREEN: u8 = 13;
/// Index of "TWB64 028 Game.com Ver.", the Game Boy default since 1.3.5.
const GB_PALETTE_TWB64_GAME_COM: u8 = 48;

/// Original Slot 1.8.1 (#123): 0 means Fast Forward Off.
/// Fast Forward MAX: as fast as the handheld goes (Siham 2026-11-11).
pub const FF_MAX: u8 = u8::MAX;
pub const SPEEDS: [u8; 6] = [0, 2, 3, 4, 6, FF_MAX];
const NAME: &str = "slot-konkr-settings.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shader {
    Off, Lcd3x, Grid, Dot, Simpletex,
    LcdGridV2, LcdGridV2Gba, ZfastLcd, Lcd1x, SimpletexGba,
    Agb001, Ags001, SharpBilinear,
    // NES, Super Nintendo and Mega Drive (Siham 2026-11-04).
    ZfastCrt, CrtGdvTrinitron, CrtGeomDeluxe, CrtPi,
    // Light ones for the weaker GPUs (Siham 2026-11-05).
    Fakelottes, CrtAperture,
    // Game Boy Dot Matrix, the player's palette (Siham 2026-11-09).
    DotMatrix,
    // Five more for NES, Super Nintendo and Mega Drive (Siham 2026-11-09).
    CrtEasymode,
    CrtHyllian,
    Sabr,
    SuperEagle,
    XbrLv2,
    // Game Boy Color only: Dot Matrix in the game's colours (Siham
    // 2026-11-10).
    DotMatrixColor,
    // crt-royale (fast), flat, for NES, Super Nintendo and Mega Drive
    // (Siham 2026-11-11).
    CrtRoyale,
}
impl Shader {
    // Siham's order (2026-11-11): Off, then the lightest and the most used
    // first, the heaviest last. LCD3x stays the first-install default.
    // Choices are saved by name, so a new order changes nobody's.
    pub const GBA: [Self; 11] = [
        Self::Off, Self::SharpBilinear, Self::Lcd1x, Self::Lcd3x, Self::ZfastLcd,
        Self::SimpletexGba, Self::Dot, Self::LcdGridV2, Self::LcdGridV2Gba,
        Self::Agb001, Self::Ags001,
    ];
    // GB/GBC: their own Grid among the GBA presets without GBA colours.
    pub const GB: [Self; 9] = [
        Self::Off, Self::SharpBilinear, Self::Grid, Self::Lcd1x, Self::Lcd3x,
        Self::ZfastLcd, Self::Simpletex, Self::Dot, Self::LcdGridV2,
    ];
    /// Game Boy Color: the GB/GBC list plus Dot Matrix Color, the Dot
    /// Matrix in the game's own colours (Siham 2026-11-10).
    pub const GBC: [Self; 10] = [
        Self::Off, Self::SharpBilinear, Self::Grid, Self::Lcd1x, Self::Lcd3x,
        Self::ZfastLcd, Self::Simpletex, Self::Dot, Self::LcdGridV2,
        Self::DotMatrixColor,
    ];
    /// Game Boy (DMG) only: the GB/GBC list plus Dot Matrix, a shader made
    /// for the original Game Boy's greens (Siham 2026-11-09).
    pub const DMG: [Self; 10] = [
        Self::Off, Self::SharpBilinear, Self::Grid, Self::Lcd1x, Self::Lcd3x,
        Self::ZfastLcd, Self::Simpletex, Self::Dot, Self::LcdGridV2,
        Self::DotMatrix,
    ];
    /// NES / Super Nintendo / Mega Drive: Off, the CRTs from the lightest
    /// to the heaviest (zfast-crt, the default, first), then the pixel art
    /// smoothers (Siham 2026-11-11).
    pub const HOME: [Self; 13] = [
        Self::Off, Self::ZfastCrt, Self::Fakelottes, Self::CrtPi,
        Self::CrtEasymode, Self::CrtHyllian, Self::CrtAperture,
        Self::CrtGdvTrinitron, Self::CrtGeomDeluxe, Self::CrtRoyale,
        Self::SuperEagle, Self::XbrLv2, Self::Sabr,
    ];
    pub fn effect(self) -> ScreenEffect {
        match self {
            Self::Off => ScreenEffect::None,
            Self::Lcd3x => ScreenEffect::Lcd3x,
            Self::Grid => ScreenEffect::Grid,
            Self::Dot => ScreenEffect::Dot,
            Self::Simpletex => ScreenEffect::Simpletex,
            Self::LcdGridV2 => ScreenEffect::LcdGridV2,
            Self::LcdGridV2Gba => ScreenEffect::LcdGridV2Gba,
            Self::ZfastLcd => ScreenEffect::ZfastLcd,
            Self::Lcd1x => ScreenEffect::Lcd1x,
            Self::SimpletexGba => ScreenEffect::SimpletexGba,
            Self::Agb001 => ScreenEffect::Agb001,
            Self::Ags001 => ScreenEffect::Ags001,
            Self::SharpBilinear => ScreenEffect::SharpBilinear,
            Self::ZfastCrt => ScreenEffect::ZfastCrt,
            Self::CrtGdvTrinitron => ScreenEffect::CrtGdvMiniUltra,
            Self::CrtGeomDeluxe => ScreenEffect::CrtGeomDeluxe,
            Self::CrtPi => ScreenEffect::CrtPi,
            Self::Fakelottes => ScreenEffect::Fakelottes,
            Self::CrtAperture => ScreenEffect::CrtAperture,
            Self::DotMatrix => ScreenEffect::DotMatrix,
            Self::CrtEasymode => ScreenEffect::CrtEasymode,
            Self::CrtHyllian => ScreenEffect::CrtHyllian,
            Self::Sabr => ScreenEffect::Sabr,
            Self::SuperEagle => ScreenEffect::SuperEagle,
            Self::XbrLv2 => ScreenEffect::XbrLv2,
            Self::DotMatrixColor => ScreenEffect::DotMatrixColour,
            Self::CrtRoyale => ScreenEffect::CrtRoyale,
        }
    }
    pub fn includes_gba_color(self) -> bool {
        // Simpletex on the GBA lost its own GBA Color in 2.2.1: Color
        // Correction is an option there too (Siham 2026-11-10).
        matches!(self, Self::LcdGridV2Gba)
    }
    pub fn core_colour(self, requested: bool) -> bool {
        // GBA Color is already in the shader; avoid applying it twice
        // through mGBA/gpSP core options when user enables correction.
        requested && !self.includes_gba_color()
    }
    fn step(self, values: &[Self], right: bool) -> Self {
        let at = values.iter().position(|v| *v == self).unwrap_or(0);
        values[if right { (at + 1) % values.len() }
               else { (at + values.len() - 1) % values.len() }]
    }
}

/// What a fresh install's defaults depend on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Device {
    /// The KONKR Pocket Advance's 960x640 panel.
    pub konkr: bool,
    pub retroid_pocket_classic: bool,
    /// A 3:2 panel (the KONKR's and others): NES, SNES and Mega Drive start
    /// at Original Ratio there, with no overlay (Siham, 2026-11-04).
    pub three_two: bool,
}

impl Device {
    pub const KONKR: Device = Device { konkr: true, retroid_pocket_classic: false, three_two: true };
    pub const RPC: Device = Device { konkr: false, retroid_pocket_classic: true, three_two: false };
    /// Whether a panel is 3:2, within 1%.
    pub fn is_three_two(panel: (u32, u32)) -> bool {
        let a = panel.0.max(1) as f32 / panel.1.max(1) as f32;
        (a / 1.5 - 1.0).abs() < 0.01
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub ff_speed: u8, pub ff_sound: bool, pub colour_correction: bool,
    pub rumble: bool, pub shader_gba: Shader, pub shader_gb: Shader,
    pub shader_gbc: Shader,
    /// The home consoles' CRT shaders (new in 1.3.5: zfast-crt by default).
    pub shader_nes: Shader, pub shader_snes: Shader, pub shader_md: Shader,
    // Retain legacy field for old RC1 Settings.json and old desktop Link code;
    // the three platform fields below are authoritative after migration.
    pub colour_correction_gba: bool,
    pub colour_correction_gb: bool,
    pub colour_correction_gbc: bool,
    pub bezel_gba: Bezel, pub bezel_gbc: Bezel, pub bezel_gb: Bezel,
    #[serde(default="bezel_nes_default")]
    pub bezel_nes: Bezel,
    #[serde(default="bezel_snes_default")]
    pub bezel_snes: Bezel,
    #[serde(default="bezel_md_default")]
    pub bezel_md: Bezel,
    pub gb_palettes: bool, pub gb_palette: u8, pub rewind: bool,
    // Number of historical rewind snapshots restored per 100 ms of game time.
    // Prior Android builds captured 10/s and replayed 10/s (1x). Original
    // Slot captures ~30/s and replays ~60/s (2x). Optional 5x is faster.
    pub rewind_speed: u8,
    // X/Y Buttons: true = Slot's original Turbo, false = Y=A and X=B.
    // Existing persisted turbo=false now selects the alternative A/B mapping.
    pub turbo: bool, pub eject_save: bool,
    // Optional one-line FPS display.
    pub show_fps: bool,
    // Frame Pacing is always RetroArch's (display-clocked core, resampled
    // audio with rate control). false = NATIVE (swap-paced loop), true =
    // RESPONSIVE (Choreographer VSync loop, a little less lag, a little less
    // smooth).
    pub responsive_loop: bool,
    pub run_ahead_frames: u8,
    // Opt-in only. Old builds enabled these by default; the migration
    // below must not interpret those historical booleans as explicit consent.
    pub show_wifi: bool, pub show_bluetooth: bool,
    pub show_integrations: bool,
    /// Missing legacy field retains original 24h display.
    pub time_format: TimeFormat,
    /// Clock & Battery at the top of the shelf instead of on the bar.
    #[serde(default)]
    pub clock_on_top: bool,
    /// Wi-Fi, Bluetooth and app icons on the bar instead of at the top.
    #[serde(default)]
    pub icons_on_bottom: bool,
    /// Apps: only the running ones, instead of the stopped ones dimmed.
    #[serde(default)]
    pub apps_running_only: bool,
    /// Cart Studio's size step (SELECT), the same for every system; 0, two
    /// big carts, is the default (Siham 2026-11-10).
    #[serde(default)]
    pub cart_studio_size: u8,
    #[serde(default)]
    pub status_opt_in_v2: bool,
    // Slot v1.7 visual settings; independent of saved emulation profiles.
    pub wallpaper: WallpaperPick,
    pub wallpaper_tint: WallpaperTint,
    pub appearance: Appearance,
    /// Slot v1.7 GENERAL: Recents is off by default; persist its list regardless.
    pub show_recents: bool,
    pub show_titles: bool,
    /// Set once the home consoles' 3:2 default (no overlay) was applied.
    #[serde(default)]
    pub home_bezel_three_two: bool,
    /// One-time move of the old GBC Green default to TWB64 Game.com.
    #[serde(default)]
    pub gb_palette_twb64: bool,
    /// One-time move of GB/GBC overlays left OFF to Watomsk's frames.
    #[serde(default)]
    pub gb_overlays_watomsk: bool,
    /// One-time Color Correction ON for a GBA Simpletex chosen before 2.2.1,
    /// when the shader still had its own GBA Color.
    #[serde(default)]
    pub simpletex_gba_colour: bool,
    /// Frame Blending of each system (Systems > [system] > Frame Blending).
    #[serde(default)]
    pub blending_gba: Blending,
    #[serde(default)]
    pub blending_gbc: Blending,
    #[serde(default)]
    pub blending_gb: Blending,
    #[serde(default)]
    pub blending_nes: Blending,
    #[serde(default)]
    pub blending_snes: Blending,
    #[serde(default)]
    pub blending_md: Blending,
    /// LCD Effect of each Game Boy (Systems > [system] > LCD Effect).
    #[serde(default)]
    pub lcd_effect_gba: LcdEffect,
    #[serde(default)]
    pub lcd_effect_gbc: LcdEffect,
    #[serde(default)]
    pub lcd_effect_gb: LcdEffect,
    /// Mega Drive: Sp00kyFox's GDAPT blends the dithering games used for
    /// transparency (Systems > Mega Drive > Dither Blending).
    #[serde(default)]
    pub dither_blend_md: bool,
    /// GBA Video carts (game code M...) have their own Screen: Shader,
    /// Upscaler, Color Correction, and no LCD Effect or Frame Blending
    /// (Siham 2026-11-10).
    #[serde(default = "shader_gbav_default")]
    pub shader_gbav: Shader,
    #[serde(default)]
    pub upscaler_gbav: Upscaler,
    #[serde(default)]
    pub colour_correction_gbav: bool,
    /// One-time move of 2.3.0-beta7's Anime4K Fast default to FSR.
    #[serde(default)]
    pub gbav_fsr: bool,
}
fn shader_gbav_default() -> Shader { Shader::Lcd3x }
impl Default for Settings {
    fn default() -> Self {
        // Historical defaults for serde's partial legacy JSON. Do not change:
        // old installs may omit fields and still need their prior values.
        Self { ff_speed: 2, ff_sound: true, colour_correction: false,
            rumble: true, shader_gba: Shader::Off, shader_gb: Shader::Off,
            shader_gbc: Shader::Off,
            shader_nes: Shader::ZfastCrt, shader_snes: Shader::ZfastCrt,
            shader_md: Shader::ZfastCrt,
            colour_correction_gba: false, colour_correction_gb: false,
            colour_correction_gbc: false,
            bezel_gba: Bezel::None, bezel_gbc: Bezel::Default, bezel_gb: Bezel::Default,
            bezel_nes: Bezel::Nes, bezel_snes: Bezel::Snes, bezel_md: Bezel::GenesisClassic,
            gb_palettes: false, gb_palette: 1, rewind: true,
            rewind_speed: 2,
            turbo: true, eject_save: true,
            show_fps: false, responsive_loop: false,
            run_ahead_frames: 0,
            show_wifi: false, show_bluetooth: false, show_integrations: false,
            time_format: TimeFormat::TwentyFourHour,
            clock_on_top: false, icons_on_bottom: false, apps_running_only: false,
            cart_studio_size: 0,
            status_opt_in_v2: true, wallpaper: WallpaperPick::Default,
            wallpaper_tint: WallpaperTint::Off, appearance: Appearance::Dark,
            show_recents: false, show_titles: false, home_bezel_three_two: false,
            gb_palette_twb64: false, gb_overlays_watomsk: false,
            simpletex_gba_colour: false,
            blending_gba: Blending::Off, blending_gbc: Blending::Off, blending_gb: Blending::Off,
            blending_nes: Blending::Off, blending_snes: Blending::Off, blending_md: Blending::Off,
            lcd_effect_gba: LcdEffect::Off, lcd_effect_gbc: LcdEffect::Off, lcd_effect_gb: LcdEffect::Off,
            dither_blend_md: false,
            shader_gbav: Shader::Lcd3x, upscaler_gbav: Upscaler::Fsr,
            colour_correction_gbav: false, gbav_fsr: false }
    }
}
impl Settings {
    /// Defaults for a truly fresh KONKR install only. The serde Default above
    /// is intentionally the legacy baseline for existing, sometimes partial,
    /// settings JSON. Changing it would silently reset old users' choices.
    pub fn first_install_defaults() -> Self {
        Self {
            wallpaper: WallpaperPick::None,
            // The clock and battery at the top with the icons, like a phone
            // (Siham 2026-11-09); upgrades keep theirs on the bar.
            clock_on_top: true,
            shader_gba: Shader::Lcd3x,
            shader_gbc: Shader::Lcd3x,
            shader_gb: Shader::Lcd3x,
            // Watomsk's frames, wherever the screen leaves room beside the
            // picture (3:2, 4:3, 16:9; Siham 2026-11-04).
            bezel_gbc: Bezel::Color,
            bezel_gb: Bezel::Gb,
            // Shown only where the GBA picture leaves room (not the KONKR).
            bezel_gba: Bezel::GbaClassic,
            gb_palettes: true,
            gb_palette: GB_PALETTE_TWB64_GAME_COM,
            ff_speed: 3,
            rewind_speed: 3,
            turbo: false,
            // Frame Pacing NATIVE: RetroArch's loop, smoother than RetroArch
            // itself on the KONKR (1.2.3 beta22 captures).
            responsive_loop: false,
            run_ahead_frames: 0,
            home_bezel_three_two: true,
            gb_palette_twb64: true,
            gb_overlays_watomsk: true,
            simpletex_gba_colour: true,
            gbav_fsr: true,
            ..Self::default()
        }
    }
    /// Device-local first-install defaults: LCD3x on every screen (Siham
    /// 2026-11-04); Frame Pacing RESPONSIVE everywhere but on the KONKR,
    /// which keeps NATIVE (Siham 2026-11-05): with RESPONSIVE every shader
    /// keeps up on the weaker GPUs (Retroid Pocket Classic and others).
    pub fn first_install_defaults_for_device(device: Device) -> Self {
        let mut defaults = Self::first_install_defaults();
        defaults.responsive_loop = !device.konkr;
        if device.three_two { defaults.home_bezels_off(); }
        defaults
    }
    /// NES, SNES and Mega Drive without an overlay.
    fn home_bezels_off(&mut self) {
        self.bezel_nes = Bezel::None;
        self.bezel_snes = Bezel::None;
        self.bezel_md = Bezel::None;
    }
    /// Existing Settings.json always wins, including an explicit NATIVE choice.
    /// Only a missing or unreadable first-install configuration gets a
    /// model-specific default; never rewrite the file just to apply a default.
    pub fn load_for_device(dir: &Path, device: Device) -> Self {
        fs::read(dir.join(NAME)).ok()
            .and_then(|b| {
                let old_json=serde_json::from_slice::<serde_json::Value>(&b).ok();
                let had_wallpaper_choice=old_json.as_ref()
                    .is_some_and(|v|v.get("wallpaper").is_some());
                serde_json::from_slice::<Self>(&b).ok().map(|mut s| {
                    // Critical: prior U1/RC1 stored a shared GB/GBC shader and
                    // shared color correction. Copy those values *once* into
                    // independent fields, never reset the user's preferences.
                    if let Some(json)=old_json.as_ref() {
                        if json.get("shader_gbc").is_none() {s.shader_gbc=s.shader_gb;}
                        for (name,platform) in [
                            ("colour_correction_gba", slot_store::Platform::Gba),
                            ("colour_correction_gb", slot_store::Platform::Gb),
                            ("colour_correction_gbc", slot_store::Platform::Gbc),
                        ] {
                            if json.get(name).is_none() {
                                s.set_colour_for(platform,s.colour_correction);
                            }
                        }
                        if legacy_pacing_is_native(json) { s.responsive_loop=false; }
                    }
                    // Beta RC1 imported user.png but lacked an option to turn
                    // it off. Preserve that picture on update to U1 instead
                    // of silently replacing it with Slot's bundled default.
                    if !had_wallpaper_choice &&
                        dir.join("Wallpapers/user.png").is_file() {
                        s.wallpaper=WallpaperPick::File("user.png".into());
                    }
                    s
                })
            })
            .map(|mut s| {
                if !SPEEDS.contains(&s.ff_speed) { s.ff_speed = 2; }
                if s.shader_gba == Shader::Grid { s.shader_gba = Shader::LcdGridV2; }
                if !Shader::GBA.contains(&s.shader_gba) { s.shader_gba = Shader::Off; }
                if !Shader::GBA.contains(&s.shader_gbav) { s.shader_gbav = Shader::Lcd3x; }
                if !Shader::DMG.contains(&s.shader_gb) { s.shader_gb = Shader::Off; }
                if !Shader::GBC.contains(&s.shader_gbc) { s.shader_gbc = Shader::Off; }
                for shader in [&mut s.shader_nes, &mut s.shader_snes, &mut s.shader_md] {
                    if !Shader::HOME.contains(shader) { *shader = Shader::ZfastCrt; }
                }
                if s.gb_palette as usize >= GbPalette::COUNT { s.gb_palette = 1; }
                if !REWIND_SPEEDS.contains(&s.rewind_speed) { s.rewind_speed = 2; }
                if s.run_ahead_frames > 2 { s.run_ahead_frames = 0; }
                // Before this version all three toggles defaulted ON.
                // A previously written settings file therefore isn't proof
                // that the user chose to permit status probing. One-time
                // safe migration, without touching any other preference.
                if !s.status_opt_in_v2 {
                    s.show_wifi = false;
                    s.show_bluetooth = false;
                    s.show_integrations = false;
                    s.status_opt_in_v2 = true;
                }
                // Once: on 3:2 the home consoles' overlays were on by
                // default in the betas; they now start without one.
                if !s.home_bezel_three_two {
                    if device.three_two { s.home_bezels_off(); }
                    s.home_bezel_three_two = true;
                }
                // Once: Game Boy carts that still wore the old default
                // (GBC Green) move to the new one, TWB64 Game.com.
                if !s.gb_palette_twb64 {
                    if s.gb_palette == GB_PALETTE_GBC_GREEN {
                        s.gb_palette = GB_PALETTE_TWB64_GAME_COM;
                    }
                    s.gb_palette_twb64 = true;
                }
                // Once: GBA Video's Upscaler default is FSR, not 2.3.0-beta7's
                // Anime4K Fast (Siham 2026-11-10); a game's own choice stays.
                if !s.gbav_fsr {
                    s.upscaler_gbav = Upscaler::Fsr;
                    s.gbav_fsr = true;
                }
                // Once: GB/GBC overlays left OFF take Watomsk's frames, the
                // new default; a later OFF is kept.
                if !s.gb_overlays_watomsk {
                    if s.bezel_gb == Bezel::None { s.bezel_gb = Bezel::Gb; }
                    if s.bezel_gbc == Bezel::None { s.bezel_gbc = Bezel::Color; }
                    s.gb_overlays_watomsk = true;
                }
                // Once: Simpletex on the GBA no longer has its own GBA
                // Color; a player who used it keeps colours close to before.
                if !s.simpletex_gba_colour {
                    if s.shader_gba == Shader::SimpletexGba {
                        s.colour_correction_gba = true;
                    }
                    s.simpletex_gba_colour = true;
                }
                s
            }).unwrap_or_else(|| Self::first_install_defaults_for_device(device))
    }
    pub fn load(dir: &Path) -> Self {
        Self::load_for_device(dir, Device::KONKR)
    }
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let bytes = serde_json::to_vec_pretty(&self).map_err(io::Error::other)?;
        // Durable like battery saves: a power cut must never leave an empty
        // Settings.json that silently resets every preference.
        slot_store::atomic_write(&dir.join(NAME), &bytes)
    }
    pub fn shader_for(&self, platform: slot_store::Platform)->Shader {
        match platform {
            slot_store::Platform::Gba=>self.shader_gba,
            slot_store::Platform::Gbc=>self.shader_gbc,
            slot_store::Platform::Gb=>self.shader_gb,
            slot_store::Platform::Nes=>self.shader_nes,
            slot_store::Platform::Snes=>self.shader_snes,
            slot_store::Platform::Md=>self.shader_md,
        }
    }
    pub fn set_shader_for(&mut self,platform:slot_store::Platform,shader:Shader) {
        match platform {
            slot_store::Platform::Gba=>self.shader_gba=shader,
            slot_store::Platform::Gbc=>self.shader_gbc=shader,
            slot_store::Platform::Gb=>self.shader_gb=shader,
            slot_store::Platform::Nes=>self.shader_nes=shader,
            slot_store::Platform::Snes=>self.shader_snes=shader,
            slot_store::Platform::Md=>self.shader_md=shader,
        }
    }
    /// The Shader row's key of a home console (47 NES, 48 SNES, 49 MD).
    pub fn home_shader_key(platform:slot_store::Platform)->Option<usize> {
        match platform {
            slot_store::Platform::Nes=>Some(47),
            slot_store::Platform::Snes=>Some(48),
            slot_store::Platform::Md=>Some(49),
            _=>None,
        }
    }
    pub fn colour_for(&self,platform:slot_store::Platform)->bool {
        match platform {
            slot_store::Platform::Gba=>self.colour_correction_gba,
            slot_store::Platform::Gbc=>self.colour_correction_gbc,
            slot_store::Platform::Gb=>self.colour_correction_gb,
            _=>false,
        }
    }
    /// The Settings key of a system's Frame Blending row (60..=65).
    pub fn blending_key(platform: slot_store::Platform) -> usize {
        use slot_store::Platform::*;
        match platform { Gba => 60, Gbc => 61, Gb => 62, Nes => 63, Snes => 64, Md => 65 }
    }

    pub(crate) fn blending_mut(&mut self, platform: slot_store::Platform) -> &mut Blending {
        use slot_store::Platform::*;
        match platform {
            Gba => &mut self.blending_gba, Gbc => &mut self.blending_gbc, Gb => &mut self.blending_gb,
            Nes => &mut self.blending_nes, Snes => &mut self.blending_snes, Md => &mut self.blending_md,
        }
    }

    /// The Settings key of a Game Boy's LCD Effect row (66..=68), or the
    /// Mega Drive's Dither Blending row (69): the row after the shader.
    pub fn look_key(platform: slot_store::Platform) -> Option<usize> {
        use slot_store::Platform::*;
        match platform { Gba => Some(66), Gbc => Some(67), Gb => Some(68), Md => Some(69), _ => None }
    }

    pub fn lcd_effect_for(&self, platform: slot_store::Platform) -> LcdEffect {
        use slot_store::Platform::*;
        match platform {
            Gba => self.lcd_effect_gba, Gbc => self.lcd_effect_gbc, Gb => self.lcd_effect_gb,
            _ => LcdEffect::Off,
        }
    }

    pub fn set_lcd_effect_for(&mut self, platform: slot_store::Platform, value: LcdEffect) {
        use slot_store::Platform::*;
        match platform {
            Gba => self.lcd_effect_gba = value, Gbc => self.lcd_effect_gbc = value,
            Gb => self.lcd_effect_gb = value, _ => {}
        }
    }

    pub fn blending_for(&self, platform: slot_store::Platform) -> Blending {
        use slot_store::Platform::*;
        match platform {
            Gba => self.blending_gba, Gbc => self.blending_gbc, Gb => self.blending_gb,
            Nes => self.blending_nes, Snes => self.blending_snes, Md => self.blending_md,
        }
    }

    /// What a GBA Video cart plays with: its own Shader and Color
    /// Correction in the GBA fields, no LCD Effect, no Frame Blending.
    pub fn as_gba_video(&self) -> Settings {
        let mut video = self.clone();
        video.shader_gba = self.shader_gbav;
        video.set_colour_for(slot_store::Platform::Gba, self.colour_correction_gbav);
        video.lcd_effect_gba = LcdEffect::Off;
        video.blending_gba = Blending::Off;
        video
    }
    pub fn set_colour_for(&mut self,platform:slot_store::Platform,value:bool) {
        match platform {
            slot_store::Platform::Gba=>{
                self.colour_correction_gba=value;
                self.colour_correction=value; // compatibility with old JSON
            },
            slot_store::Platform::Gbc=>self.colour_correction_gbc=value,
            slot_store::Platform::Gb=>self.colour_correction_gb=value,
            _=>{}
        }
    }
    pub fn bezel_for(&self,platform:slot_store::Platform)->Bezel {
        match platform {
            // GBA's old ON is its classic Watomsk frame.
            slot_store::Platform::Gba=>match self.bezel_gba {
                Bezel::Default=>Bezel::GbaClassic, bezel=>bezel,
            },
            slot_store::Platform::Gbc=>self.bezel_gbc,
            slot_store::Platform::Gb=>self.bezel_gb,
            slot_store::Platform::Nes=>self.bezel_nes,
            slot_store::Platform::Snes=>self.bezel_snes,
            slot_store::Platform::Md=>self.bezel_md,
        }
    }
    pub fn set_bezel_for(&mut self,platform:slot_store::Platform,value:Bezel) {
        match platform {
            slot_store::Platform::Gba=>self.bezel_gba=value,
            slot_store::Platform::Gbc=>self.bezel_gbc=value,
            slot_store::Platform::Gb=>self.bezel_gb=value,
            slot_store::Platform::Nes=>self.bezel_nes=value,
            slot_store::Platform::Snes=>self.bezel_snes=value,
            slot_store::Platform::Md=>self.bezel_md=value,
        }
    }
    /// The Bezel row's settings key for each system.
    pub fn bezel_key(platform:slot_store::Platform)->usize {
        match platform {
            slot_store::Platform::Gba=>25, slot_store::Platform::Gbc=>26,
            slot_store::Platform::Gb=>27, slot_store::Platform::Nes=>44,
            slot_store::Platform::Snes=>45, slot_store::Platform::Md=>46,
        }
    }
    fn step_bezel(&mut self,platform:slot_store::Platform,right:bool) {
        let next=self.bezel_for(platform).step_in(Bezel::list(platform),right);
        self.set_bezel_for(platform,next);
    }
    // Screen: 0 GBA shader, 1 GB shader, 2 legacy GBA correction, 3 palettes.
    // 21 GBC shader, 22/23/24 independent corrections, 25/26/27 bezels.
    // Gameplay: keys 4..9.
    // Faithful to Slot quick_menu.rs except Date & Time (owned by Android).
    pub fn change(&mut self, key: usize, right: bool) -> bool {
        let before = self.clone();
        match key {
            0 => self.shader_gba = self.shader_gba.step(&Shader::GBA, right),
            1 => self.shader_gb = self.shader_gb.step(&Shader::DMG, right),
            2 | 22 => self.set_colour_for(slot_store::Platform::Gba,
                !self.colour_correction_gba),
            23 => self.set_colour_for(slot_store::Platform::Gbc,
                !self.colour_correction_gbc),
            24 => self.set_colour_for(slot_store::Platform::Gb,
                !self.colour_correction_gb),
            21 => self.shader_gbc=self.shader_gbc.step(&Shader::GBC,right),
            60..=65 => {
                use slot_store::Platform::*;
                let platform = [Gba, Gbc, Gb, Nes, Snes, Md][key - 60];
                let blending = self.blending_mut(platform);
                *blending = blending.step(right);
            }
            66 => self.lcd_effect_gba = self.lcd_effect_gba.step(right),
            70 => self.upscaler_gbav = self.upscaler_gbav.step(right),
            67 => self.lcd_effect_gbc = self.lcd_effect_gbc.step(right),
            68 => self.lcd_effect_gb = self.lcd_effect_gb.step(right),
            69 => self.dither_blend_md = !self.dither_blend_md,
            25 => self.step_bezel(slot_store::Platform::Gba,right),
            26 => self.step_bezel(slot_store::Platform::Gbc,right),
            27 => self.step_bezel(slot_store::Platform::Gb,right),
            44 => self.step_bezel(slot_store::Platform::Nes,right),
            45 => self.step_bezel(slot_store::Platform::Snes,right),
            46 => self.step_bezel(slot_store::Platform::Md,right),
            47 => self.shader_nes=self.shader_nes.step(&Shader::HOME,right),
            48 => self.shader_snes=self.shader_snes.step(&Shader::HOME,right),
            49 => self.shader_md=self.shader_md.step(&Shader::HOME,right),
            3 => step_palette(&mut self.gb_palettes, &mut self.gb_palette, right),
            4 => {
                let at = SPEEDS.iter().position(|x| *x == self.ff_speed).unwrap_or(0);
                let n = SPEEDS.len();
                self.ff_speed = SPEEDS[if right { (at + 1) % n } else { (at + n - 1) % n }];
            }
            5 => self.ff_sound = !self.ff_sound,
            6 => cycle_rewind_setting(&mut self.rewind, &mut self.rewind_speed, right),
            7 => self.turbo = !self.turbo,
            8 => self.rumble = !self.rumble,
            9 => self.eject_save = !self.eject_save,
            10 => self.show_fps = !self.show_fps,
            11 => self.responsive_loop = !self.responsive_loop,
            12 => self.run_ahead_frames = cycle_u8(self.run_ahead_frames, 2, right),
            // Wi-Fi & Bluetooth: BOTH, WI-FI, BLUETOOTH, OFF.
            13 => {
                const RADIOS: [(bool, bool); 4] = [(true, true), (true, false), (false, true), (false, false)];
                let at = RADIOS.iter().position(|r| *r == (self.show_wifi, self.show_bluetooth)).unwrap_or(3);
                (self.show_wifi, self.show_bluetooth) = RADIOS[if right { (at + 1) % 4 } else { (at + 3) % 4 }];
            }
            14 => self.show_bluetooth = !self.show_bluetooth,
            // Apps: ALL, RUNNING, OFF.
            15 => {
                const APPS: [(bool, bool); 3] = [(true, false), (true, true), (false, false)];
                let now = if self.show_integrations { (true, self.apps_running_only) } else { (false, false) };
                let at = APPS.iter().position(|a| *a == now).unwrap_or(2);
                (self.show_integrations, self.apps_running_only) =
                    APPS[if right { (at + 1) % 3 } else { (at + 2) % 3 }];
            }
            32 => self.clock_on_top = !self.clock_on_top,
            33 => self.icons_on_bottom = !self.icons_on_bottom,
            28 => self.time_format = self.time_format.toggle(),
            _ => return false,
        }
        *self != before
    }
    #[cfg(test)]
    pub fn palette_label(&self) -> &'static str {
        GbPalette::all().nth(self.gb_palette as usize)
            .unwrap_or(GbPalette::DEFAULT).label()
    }
    pub fn palette_name(&self) -> &'static str {
        GbPalette::all().nth(self.gb_palette as usize)
            .unwrap_or(GbPalette::DEFAULT).core_name()
    }
}
/// One live Palette row, as slot. 1.8.1: OFF, then every palette in order,
/// then OFF again. Left walks the same ring backwards.
fn step_palette(on: &mut bool, index: &mut u8, right: bool) {
    let last = (GbPalette::COUNT - 1) as u8;
    match (*on, right) {
        (false, true) => { *on = true; *index = 0; }
        (false, false) => { *on = true; *index = last; }
        (true, true) if *index >= last => *on = false,
        (true, false) if *index == 0 => *on = false,
        (true, true) => *index += 1,
        (true, false) => *index -= 1,
    }
}
/// Frame Pacing has two options, both RetroArch pacing (display-clocked
/// core, resampled audio with rate control): NATIVE runs RetroArch's
/// swap-paced loop, RESPONSIVE the Choreographer VSync loop. Up to 1.2.3 the
/// JSON also stored older modes (STANDARD, DISPLAY SYNC, SYNC LAB, RA CLOCK
/// LAB) as `retroarch_sync_pacing: false`; every one of them loads as NATIVE.
pub fn legacy_pacing_is_native(json: &serde_json::Value) -> bool {
    json.get("retroarch_sync_pacing").and_then(serde_json::Value::as_bool) == Some(false)
}

/// RA CLOCK: accumulate fractional GBA time from Choreographer VSync
/// deltas, not jitter between GL callbacks. Used by RetroArch pacing when
/// the panel cannot clock the core. Still one real frame per callback.
pub fn frame_debt_ra_clock(prev: f64, elapsed: f64, speed: f64, period: f64) -> f64 {
    (prev + elapsed * speed).clamp(0.0, period * 4.0)
}

pub fn cycle_u8(value: u8, max: u8, right: bool) -> u8 {
    if right { if value >= max { 0 } else { value + 1 } }
    else if value == 0 || value > max { max } else { value - 1 }
}

/// Wall-clock frame debt for fast-forward and linked sessions, the only
/// paths not clocked by the display.
pub fn frame_debt(prev: f64, elapsed: f64, speed: f64, fast: bool) -> f64 {
    let max = if fast { 0.40 } else { 0.10 };
    (prev + elapsed * speed).min(max)
}

/// A11: linked mGBA Cable can legitimately stall while awaiting a real
/// remote input frame. Its lost emulation time must not be discarded, or
/// the PCM producer permanently falls behind AudioTrack.
/// The 2.5-frame cap avoids runaway catch-up after a long AVD/Wi-Fi hitch.
/// The runtime may emulate at most TWO frames in one GL callback and each
/// still requires confirmed local AND peer inputs: never predict link traffic.
pub fn frame_debt_mgba_cable(prev: f64, elapsed: f64, period: f64) -> f64 {
    (prev + elapsed.max(0.0)).min(period * 2.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_formats_midnight_noon_and_day_boundaries() {
        for (clock, expected) in [
            ("00:00", "12:00 AM"),
            ("00:07", "12:07 AM"),
            ("01:05", "1:05 AM"),
            ("11:59", "11:59 AM"),
            ("12:00", "12:00 PM"),
            ("12:07", "12:07 PM"),
            ("13:05", "1:05 PM"),
            ("23:59", "11:59 PM"),
        ] {
            assert_eq!(TimeFormat::TwelveHour.format_clock(clock), expected);
            assert_eq!(TimeFormat::TwentyFourHour.format_clock(clock), clock);
        }
        for clock in ["", "invalid", "24:00", "12:60"] {
            assert_eq!(TimeFormat::TwelveHour.format_clock(clock), clock);
        }
    }

    #[test]
    fn time_format_defaults_preserve_existing_and_fresh_install_clocks() {
        let legacy: Settings = serde_json::from_str(
            r#"{"ff_speed":4,"show_titles":true}"#).unwrap();
        assert_eq!(legacy.time_format, TimeFormat::TwentyFourHour);
        assert_eq!(Settings::first_install_defaults().time_format,
            TimeFormat::TwentyFourHour);
        assert_eq!(legacy.ff_speed, 4);
        assert!(legacy.show_titles);
    }

    #[test]
    fn time_format_changes_in_both_directions_and_survives_restart() {
        let root = std::env::temp_dir().join(format!(
            "slot-time-format-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .unwrap().as_nanos()));
        let mut settings = Settings::first_install_defaults();
        settings.ff_speed = 4;
        settings.show_titles = true;
        let before = settings.clone();
        for right in [true, false] {
            assert!(settings.change(28, right));
            assert_eq!(settings.time_format, TimeFormat::TwelveHour);
            settings.save(&root).unwrap();
            assert_eq!(Settings::load(&root), settings);
            assert!(settings.change(28, right));
            assert_eq!(settings, before);
            settings.save(&root).unwrap();
            assert_eq!(Settings::load(&root), before);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn old_shared_gb_gbc_shader_and_colour_migrate_identically_then_split() {
        let root=std::env::temp_dir().join(format!("konkr-u1-screen-migrate-{}",std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(NAME),
            r#"{"shader_gb":"grid","colour_correction":true,"ff_speed":3}"#).unwrap();
        let mut settings=Settings::load(&root);
        assert_eq!(settings.shader_for(slot_store::Platform::Gb),Shader::Grid);
        assert_eq!(settings.shader_for(slot_store::Platform::Gbc),Shader::Grid);
        for p in [slot_store::Platform::Gba,slot_store::Platform::Gbc,slot_store::Platform::Gb] {
            assert!(settings.colour_for(p));
        }
        settings.change(21,true); // GBC shader only
        settings.change(23,true); // GBC correction only
        assert_eq!(settings.shader_gb,Shader::Grid);
        assert_ne!(settings.shader_gbc,Shader::Grid);
        assert!(!settings.colour_correction_gbc);
        assert!(settings.colour_correction_gb);
        assert_eq!(settings.ff_speed,3);
        settings.save(&root).unwrap();
        assert_eq!(Settings::load(&root),settings);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn general_recents_and_titles_are_opt_in_and_persist_without_losing_settings() {
        let previous=r#"{"ff_speed":4,"wallpaper":"none","run_ahead_frames":1}"#;
        let mut s:Settings=serde_json::from_str(previous).unwrap();
        assert!(!s.show_recents && !s.show_titles);
        s.show_recents=true;s.show_titles=true;
        let copy:Settings=serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert!(copy.show_recents && copy.show_titles);
        assert_eq!(copy.ff_speed,4);
        assert_eq!(copy.run_ahead_frames,1);
        assert_eq!(copy.wallpaper,WallpaperPick::None);
    }

    #[test]
    fn upstream_v17_wallpaper_picker_follows_original_order() {
        use WallpaperPick::*;
        let root=std::env::temp_dir().join(format!(
            "slot-wallpaper-choice-test-{}", std::process::id()));
        let dir=root.join("Wallpapers");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("user.png"), b"placeholder").unwrap();
        std::fs::write(dir.join("another.png"), b"placeholder").unwrap();
        std::fs::write(dir.join("GBA.png"), b"placeholder").unwrap();
        let choices=WallpaperPick::choices(&root);
        assert_eq!(choices, vec![None,Random,Shelf,Default,Blur,
            File("another.png".into()),File("user.png".into())]);
        assert_eq!(None.step(&root,true), Random);
        assert_eq!(Random.step(&root,true), Shelf);
        assert_eq!(Shelf.step(&root,true), Default);
        assert_eq!(None.step(&root,false), File("user.png".into()));
        assert_eq!(Default.step(&root,true), Blur);
        assert_eq!(Blur.step(&root,true), File("another.png".into()));
        assert_eq!(File("missing.png".into()).step(&root,true), Blur);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wallpaper_none_persists_with_other_dev13_preferences_unchanged() {
        let mut settings: Settings=serde_json::from_str(
            r#"{"run_ahead_frames":2,"show_wifi":true,"status_opt_in_v2":true}"#).unwrap();
        assert_eq!(settings.wallpaper, WallpaperPick::Default);
        assert_eq!(settings.appearance, Appearance::Dark);
        assert_eq!(settings.wallpaper_tint, WallpaperTint::Off);
        settings.wallpaper=WallpaperPick::None;
        settings.wallpaper_tint=WallpaperTint::High;
        settings.appearance=Appearance::Light;
        let saved=serde_json::to_string(&settings).unwrap();
        let restored:Settings=serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.wallpaper,WallpaperPick::None);
        assert_eq!(restored.wallpaper_tint.alpha(),0.65);
        assert_eq!(restored.appearance,Appearance::Light);
        assert_eq!(restored.run_ahead_frames,2);
        assert!(restored.show_wifi && restored.status_opt_in_v2);
        settings.wallpaper=WallpaperPick::Shelf;
        let saved=serde_json::to_string(&settings).unwrap();
        assert!(saved.contains(r#""wallpaper":"shelf""#), "{saved}");
        let restored:Settings=serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.wallpaper,WallpaperPick::Shelf);
    }

    #[test]
    fn old_user_png_migrates_to_file_without_overwriting_existing_settings() {
        let root=std::env::temp_dir().join(format!(
            "slot-wallpaper-migration-test-{}",std::process::id()));
        std::fs::create_dir_all(root.join("Wallpapers")).unwrap();
        std::fs::write(root.join("Wallpapers/user.png"),b"png").unwrap();
        std::fs::write(root.join("slot-konkr-settings.json"),
            br#"{"eject_save":false,"status_opt_in_v2":true}"#).unwrap();
        let s=Settings::load(&root);
        assert_eq!(s.wallpaper,WallpaperPick::File("user.png".into()));
        assert!(!s.eject_save);
        let mut settings=s.clone();
        settings.wallpaper=WallpaperPick::None;
        settings.save(&root).unwrap();
        assert_eq!(Settings::load(&root).wallpaper,WallpaperPick::None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn official_wallpaper_tint_alpha_and_theme_toggle_are_exact() {
        assert_eq!(WallpaperTint::ALL.map(WallpaperTint::alpha),
            [0.0,0.25,0.45,0.65]);
        assert_eq!(Appearance::Dark.toggle(),Appearance::Light);
        assert_eq!(Appearance::Light.toggle(),Appearance::Original);
        assert_eq!(Appearance::Original.toggle(),Appearance::Dark);
    }

    #[test]
    fn mgba_cable_preserves_bounded_debt_while_peer_frame_is_missing() {
        let frame = 1.0 / 59.7275;
        let mut debt = frame_debt_mgba_cable(0.0, frame, frame);
        assert!((debt - frame).abs() < 1e-9);
        // A missing peer input blocks the real frame: do not consume debt.
        debt = frame_debt_mgba_cable(debt, frame, frame);
        assert!((debt - 2.0 * frame).abs() < 1e-9);
        debt = frame_debt_mgba_cable(debt, 0.20, frame);
        assert!((debt - 2.5 * frame).abs() < 1e-9);
        // Represent TWO successful lockstep frames, only after peer data.
        for _ in 0..2 { debt -= frame; }
        assert!((debt - 0.5 * frame).abs() < 1e-9);
        assert_eq!(frame_debt_mgba_cable(0.0, -1.0, frame), 0.0);
    }

    #[test]
    fn mgba_cable_keeps_more_debt_than_wall_clock_pacing() {
        let frame = 1.0 / 59.7275;
        let linked = frame_debt_mgba_cable(0.0, 0.1, frame);
        assert!((linked - 2.5 * frame).abs() < 1e-9);
        assert!((frame_debt(0.0, 0.05, 1.0, false) - 0.05).abs() < 1e-9);
        assert!((frame_debt(0.0, 0.5, 1.0, false) - 0.10).abs() < 1e-9);
        assert!((frame_debt(0.0, 0.35, 2.0, true) - 0.40).abs() < 1e-9);
    }

    #[test]
    fn ra_clock_keeps_a_bounded_four_frame_backlog() {
        let p = 1.0 / 59.7275;
        assert!((frame_debt_ra_clock(0.0, 3.0 * p, 1.0, p) - 3.0 * p).abs() < 1e-9);
        assert_eq!(frame_debt_ra_clock(0.0, 10.0, 1.0, p), 4.0 * p);
        assert_eq!(frame_debt_ra_clock(0.0, -1.0, 1.0, p), 0.0);
    }

    #[test]
    fn frame_pacing_is_native_or_responsive_and_old_modes_become_native() {
        let mut s = Settings::default();
        assert!(!s.responsive_loop);
        assert!(s.change(11, true));
        assert!(s.responsive_loop);
        assert!(s.change(11, false));
        assert!(!s.responsive_loop);
        assert!(s.change(11, false) && s.responsive_loop);
        let dir = std::env::temp_dir().join(format!("slot-konkr-pacing-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for old in [r#"{"responsive_pacing":false}"#, r#"{"responsive_pacing":true}"#,
            r#"{"responsive_pacing":true,"display_sync_pacing":true}"#,
            r#"{"hybrid_pacing":true}"#, r#"{"retroarch_clock_pacing":true}"#,
            r#"{"retroarch_sync_pacing":false,"responsive_loop":true}"#] {
            fs::write(dir.join(NAME), old).unwrap();
            assert!(!Settings::load(&dir).responsive_loop, "{old}");
        }
        fs::write(dir.join(NAME), r#"{"retroarch_sync_pacing":true,"responsive_loop":true}"#)
            .unwrap();
        assert!(Settings::load(&dir).responsive_loop);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fresh_install_configuration_matches_all_seven_settings_screenshots() {
        let s=Settings::first_install_defaults();
        use slot_store::Platform::{Gba,Gbc,Gb};
        // GENERAL / WALLPAPER / INTERFACE
        assert_eq!(s.wallpaper,WallpaperPick::None);
        assert_eq!(s.appearance,Appearance::Dark);
        assert_eq!(s.wallpaper_tint,WallpaperTint::Off);
        assert!(!s.show_recents && !s.show_titles);
        assert!(s.clock_on_top);
        assert!(!s.show_wifi && !s.show_bluetooth && !s.show_integrations);
        // SCREEN: three independent LCD3x selections, no grade, proper bezels
        for platform in [Gba,Gbc,Gb] {
            assert_eq!(s.shader_for(platform),Shader::Lcd3x);
            assert!(!s.colour_for(platform));
        }
        // GBA's classic Watomsk frame is shown only off the KONKR.
        assert_eq!(s.bezel_gba,Bezel::GbaClassic);
        // GB/GBC: Watomsk's frames where the screen has room (2026-11-04).
        assert_eq!(s.bezel_gbc,Bezel::Color);
        assert_eq!(s.bezel_gb,Bezel::Gb);
        assert_eq!((s.bezel_nes,s.bezel_snes,s.bezel_md),
            (Bezel::Nes,Bezel::Snes,Bezel::GenesisClassic));
        assert!(s.gb_palettes);
        assert_eq!(s.palette_label(),"TWB64 Game.com");
        // GAMEPLAY: Fast Forward 3x, Rewind 3x, X/Y Buttons A/B.
        assert_eq!(s.ff_speed,3);
        assert!(s.rewind);
        assert_eq!(s.rewind_speed,3);
        assert!(!s.turbo);
        assert!(s.ff_sound && s.rumble && s.eject_save);
        // TIME: 24 HR clock.
        assert_eq!(s.time_format,TimeFormat::TwentyFourHour);
        // EMULATION: Frame Pacing NATIVE.
        assert!(!s.show_fps);
        assert!(!s.responsive_loop);
        assert_eq!(s.run_ahead_frames,0);
    }
    #[test]
    fn gb_and_gbc_overlays_left_off_move_to_watomsk_once() {
        let root=std::env::temp_dir().join(format!("slot-gb-watomsk-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let old=Settings { bezel_gb: Bezel::None, bezel_gbc: Bezel::None, bezel_gba: Bezel::None,
            ..Settings::default() };
        old.save(&root).unwrap();
        let mut s=Settings::load_for_device(&root, Device::KONKR);
        assert_eq!((s.bezel_gb, s.bezel_gbc), (Bezel::Gb, Bezel::Color));
        assert_eq!(s.bezel_gba, Bezel::None, "GBA keeps its choice");
        // Turned OFF again afterwards: kept.
        s.bezel_gb=Bezel::None;
        s.save(&root).unwrap();
        assert_eq!(Settings::load_for_device(&root, Device::KONKR).bezel_gb, Bezel::None);
        // The slot. bezel someone picked is kept too.
        let picked=Settings { bezel_gb: Bezel::Default, ..Settings::default() };
        picked.save(&root).unwrap();
        assert_eq!(Settings::load_for_device(&root, Device::KONKR).bezel_gb, Bezel::Default);
        let _=std::fs::remove_dir_all(&root);
    }
    #[test]
    fn every_screen_starts_with_lcd3x_on_a_fresh_install() {
        let konkr=Settings::first_install_defaults_for_device(Device::KONKR);
        let mut plain=Settings::first_install_defaults();
        plain.home_bezels_off();
        assert_eq!(konkr, plain);
        let other=Settings::first_install_defaults_for_device(
            Device { konkr: false, retroid_pocket_classic: false, three_two: false });
        for shader in [other.shader_gba, other.shader_gb, other.shader_gbc] {
            assert_eq!(shader, Shader::Lcd3x);
        }
        assert!(other.responsive_loop, "RESPONSIVE everywhere but on the KONKR");
        assert!(Settings::first_install_defaults_for_device(Device::RPC).responsive_loop);
        assert!(!konkr.responsive_loop, "the KONKR keeps NATIVE");
    }
    #[test]
    fn responsive_by_default_but_native_on_the_konkr_preserving_saved_pacing() {
        let root=std::env::temp_dir().join(format!(
            "slot-rpc-responsive-default-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .unwrap().as_nanos()));
        fs::create_dir_all(&root).unwrap();
        assert!(Settings::load_for_device(&root, Device::RPC).responsive_loop);
        assert!(!Settings::load_for_device(&root, Device::KONKR).responsive_loop);
        assert!(!Settings::first_install_defaults_for_device(Device::KONKR).responsive_loop);
        assert!(Settings::first_install_defaults_for_device(Device::RPC).responsive_loop);
        assert!(!root.join(NAME).exists(), "defaults must never auto-save");
        let mut native=Settings::first_install_defaults_for_device(Device::RPC);
        native.responsive_loop=false;
        native.save(&root).unwrap();
        assert!(!Settings::load_for_device(&root, Device::RPC).responsive_loop,
            "a user's explicit persisted NATIVE choice must survive");
        native.responsive_loop=true;
        native.save(&root).unwrap();
        assert!(Settings::load_for_device(&root, Device::RPC).responsive_loop);
        assert!(Settings::load_for_device(&root, Device::KONKR).responsive_loop,
            "a saved RESPONSIVE choice must also survive other models");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn missing_settings_file_gets_new_defaults_but_existing_json_is_untouched() {
        let root=std::env::temp_dir().join(format!(
            "slot-konkr-first-install-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&root).unwrap();
        let fresh=Settings::load(&root);
        assert_eq!(fresh,Settings::first_install_defaults_for_device(Device::KONKR));
        assert!(!root.join(NAME).exists(), "loading must not write settings");
        // Saved existing settings MUST survive upgrading from 0.2.0.
        let mut customised=fresh.clone();
        customised.shader_gba=Shader::Dot;
        customised.shader_gbc=Shader::Grid;
        customised.shader_gb=Shader::Simpletex;
        customised.wallpaper=WallpaperPick::Default;
        customised.responsive_loop=true;
        customised.run_ahead_frames=0;
        customised.show_recents=true;
        customised.show_wifi=true;
        customised.save(&root).unwrap();
        assert_eq!(Settings::load(&root),customised);
        // An older *partial* JSON must use old missing-field values, not
        // freshly introduced LCD3x/Display Sync/Run-Ahead preferences.
        std::fs::write(root.join(NAME),r#"{"ff_speed":4,"show_titles":true}"#).unwrap();
        let previous=Settings::load(&root);
        assert_eq!(previous.ff_speed,4);
        assert!(previous.show_titles);
        assert_eq!(previous.shader_gba,Shader::Off);
        assert_eq!(previous.shader_gb,Shader::Off);
        assert_eq!(previous.shader_gbc,Shader::Off);
        assert_eq!(previous.wallpaper,WallpaperPick::Default);
        assert!(!previous.responsive_loop);
        assert_eq!(previous.run_ahead_frames,0);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn defaults_preserve_dev13() {
        let s = Settings::default();
        assert_eq!(s.ff_speed, 2);
        assert!(s.ff_sound && s.rumble && s.rewind && s.turbo && s.eject_save);
        assert!(!s.gb_palettes);
        assert_eq!(s.shader_gba, Shader::Off);
    }
    #[test]
    fn gba_has_ten_unique_serde_roundtripping_presets_plus_off() {
        assert_eq!(Shader::GBA.len(),11);
        assert_eq!(Shader::GB.len(),9);
        for shader in Shader::GBA {
            assert_eq!(Shader::GBA.iter().filter(|s| **s == shader).count(),1);
            let json=serde_json::to_string(&shader).unwrap();
            assert_eq!(serde_json::from_str::<Shader>(&json).unwrap(),shader);
            assert_eq!(shader.core_colour(true), !shader.includes_gba_color());
        }
        // Simpletex on the GBA has no GBA Color of its own since 2.2.1.
        assert!(Shader::SimpletexGba.core_colour(true));
        assert!(!Shader::LcdGridV2Gba.core_colour(true));
        assert!(Shader::Dot.core_colour(true));
    }
    #[test]
    fn gba_simpletex_players_get_color_correction_once() {
        let dir=std::env::temp_dir().join(format!(
            "slot-simpletex-gba-colour-{}",std::process::id()));
        let _=fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(NAME),
            r#"{"shader_gba":"simpletexgba","colour_correction_gba":false}"#).unwrap();
        let mut s=Settings::load(&dir);
        assert_eq!(s.shader_gba,Shader::SimpletexGba);
        assert!(s.colour_correction_gba && s.simpletex_gba_colour);
        // A later OFF is kept.
        s.colour_correction_gba=false;
        s.save(&dir).unwrap();
        assert!(!Settings::load(&dir).colour_correction_gba);
        // Another shader keeps the player's choice.
        fs::write(dir.join(NAME),
            r#"{"shader_gba":"lcd3x","colour_correction_gba":false}"#).unwrap();
        assert!(!Settings::load(&dir).colour_correction_gba);
        fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn new_shaders_lcd_effects_and_dither_blending_roundtrip() {
        assert_eq!(Shader::HOME.len(), 13);
        assert_eq!(Shader::HOME[..2], [Shader::Off, Shader::ZfastCrt]);
        assert_eq!(Shader::HOME.last(), Some(&Shader::Sabr));
        assert!(Shader::HOME.contains(&Shader::CrtRoyale));
        assert!(!Shader::GBA.contains(&Shader::CrtRoyale) && !Shader::GB.contains(&Shader::CrtRoyale));
        assert_eq!(serde_json::to_string(&Shader::CrtRoyale).unwrap(), "\"crtroyale\"");
        assert!(Shader::DMG.contains(&Shader::DotMatrix) && !Shader::GB.contains(&Shader::DotMatrix));
        assert!(Shader::GBC.contains(&Shader::DotMatrixColor) && !Shader::DMG.contains(&Shader::DotMatrixColor)
            && !Shader::GBA.contains(&Shader::DotMatrixColor));
        let mut s = Settings::default();
        assert_eq!(s.lcd_effect_for(slot_store::Platform::Gb), LcdEffect::Off);
        assert!(s.change(68, true) && s.lcd_effect_gb == LcdEffect::Transparency);
        assert!(s.change(66, false) && s.lcd_effect_gba == LcdEffect::Sunlight);
        assert!(s.change(69, true) && s.dither_blend_md);
        s.shader_md = Shader::XbrLv2;
        s.shader_gb = Shader::DotMatrix;
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"xbrlv2\"") && json.contains("\"dotmatrix\"") && json.contains("\"sunlight\""));
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
        // Older settings files have none of these.
        let old: Settings = serde_json::from_str(r#"{"shader_md":"zfastcrt"}"#).unwrap();
        assert_eq!((old.lcd_effect_gbc, old.dither_blend_md), (LcdEffect::Off, false));
        assert_eq!(Settings::look_key(slot_store::Platform::Nes), None);
    }
    #[test]
    fn gba_grid_v2_migration_keeps_original_gb_gbc() {
        let root=std::env::temp_dir().join(format!(
            "slot-gba-shader-migration-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(NAME),
            r#"{"shader_gba":"grid","shader_gb":"grid","shader_gbc":"lcd3x"}"#).unwrap();
        let s=Settings::load(&root);
        assert_eq!(s.shader_gba,Shader::LcdGridV2);
        assert_eq!(s.shader_gb,Shader::Grid);
        assert_eq!(s.shader_gbc,Shader::Lcd3x);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rewind_speed_steps_and_legacy_upgrade_do_not_change_unrelated_options() {
        let old: Settings = serde_json::from_str(
            r#"{"rewind":true,"ff_speed":4,"shader_gba":"dot","colour_correction_gba":true}"#
        ).unwrap();
        assert_eq!(old.rewind_speed, 2, "match upstream pacing for old profiles");
        assert_eq!(rewind_step_seconds(1), 0.1);
        assert!((rewind_step_seconds(5) - 0.02).abs() < 1e-6);
        let mut s = old.clone();
        s.rewind = false;
        for (speed, rate) in [(1, 0.1_f32), (2, 0.05), (3, 1.0/30.0), (5, 0.02)] {
            s.change(6,true);
            assert!(s.rewind);
            assert_eq!(s.rewind_speed,speed);
            assert!((rewind_step_seconds(speed)-rate).abs()<1e-6);
        }
        assert_eq!(s.ff_speed,old.ff_speed);
        assert_eq!(s.shader_gba,old.shader_gba);
        assert_eq!(s.colour_correction_gba,old.colour_correction_gba);
        let json=serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(),s);
        let invalid: Settings=serde_json::from_str(r#"{"rewind_speed":255}"#).unwrap();
        assert_eq!(rewind_step_seconds(invalid.rewind_speed),0.05);
    }

    #[test]
    fn shader_color_pipeline_never_applies_mgba_correction_twice() {
        for shader in Shader::GBA {
            for wanted in [false,true] {
                let core=shader.core_colour(wanted);
                assert!(!(core && shader.includes_gba_color()));
                if !shader.includes_gba_color() { assert_eq!(core,wanted); }
            }
        }
        for shader in Shader::GB {
            assert!(!shader.includes_gba_color());
            assert!(shader.core_colour(true));
        }
    }

    #[test]
    fn screen_gameplay_original_choices_cycle() {
        let mut s = Settings::default();
        s.change(0, false); assert_eq!(s.shader_gba, Shader::Ags001);
        s.change(1, false); assert_eq!(s.shader_gb, Shader::DotMatrix);
        // 2x wraps left to Off (original Slot 1.8.1), then back to 6x.
        s.change(4, false); assert_eq!(s.ff_speed, 0);
        s.change(4, false); assert_eq!(s.ff_speed, FF_MAX);
        s.change(4, false); assert_eq!(s.ff_speed, 6);
        s.change(4, true); assert_eq!(s.ff_speed, FF_MAX);
        s.change(4, true); assert_eq!(s.ff_speed, 0);
        s.change(4, true); assert_eq!(s.ff_speed, 2);
        for i in [2, 3, 5, 7, 8, 9] { assert!(s.change(i, true)); }
        for _ in 0..3 { assert!(s.change(6,true)); } // 2x -> 3x -> 5x -> OFF
        assert!(!s.change(16, true));
        assert!(!s.rewind && !s.turbo && !s.eject_save);
        assert!(s.change(10, true) && s.show_fps);
        assert!(s.change(11, true) && s.responsive_loop);
        assert!(s.change(12, true) && s.run_ahead_frames == 1);
    }
    #[test]
    fn fast_forward_off_survives_a_round_trip() {
        let mut s = Settings::default();
        s.ff_speed = 0;
        let json = serde_json::to_vec(&s).unwrap();
        let back: Settings = serde_json::from_slice(&json).unwrap();
        assert_eq!(back.ff_speed, 0);
    }
    #[test]
    fn dev13_json_migrates_without_wiping_preferences() {
        let s: Settings = serde_json::from_str(
            r#"{"ff_speed":4,"ff_sound":false,"colour_correction":true,"rumble":false}"#
        ).unwrap();
        assert_eq!(s.ff_speed, 4);
        assert!(!s.ff_sound && !s.rumble);
        assert!(s.rewind && s.turbo && s.eject_save);
        assert!(!s.show_fps && !s.responsive_loop);
        assert_eq!(s.run_ahead_frames, 0);
    }
    #[test]
    fn beta8_obsolete_experiment_fields_are_ignored_when_migrating() {
        let old = r#"{
            "responsive_pacing": true,
            "display_sync_pacing": true,
            "run_ahead_frames": 1,
            "frame_delay_ms": 2,
            "gpu_sync": true,
            "audio_latency_ms": 96
        }"#;
        let parsed: Settings = serde_json::from_str(old).unwrap();
        assert!(!parsed.responsive_loop);
        assert_eq!(parsed.run_ahead_frames, 1);
        let serialized = serde_json::to_string(&parsed).unwrap();
        for removed in ["frame_delay_ms", "gpu_sync", "audio_latency_ms",
            "responsive_pacing", "display_sync_pacing"] {
            assert!(!serialized.contains(removed),
                "obsolete Beta 8 field must not survive next save: {removed}");
        }
    }
    #[test]
    fn interface_status_indicators_are_opt_in_only() {
        let mut s = Settings::default();
        assert!(!s.show_wifi && !s.show_bluetooth && !s.show_integrations);
        assert!(s.status_opt_in_v2);
        assert!(s.change(13, true) && s.show_wifi && s.show_bluetooth);
        assert!(s.change(15, true) && s.show_integrations && !s.apps_running_only);
        let json = serde_json::to_string(&s).unwrap();
        let restored: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, s);
    }
    #[test]
    fn status_bar_rows_cycle_their_choices() {
        let mut s = Settings::default();
        // Wi-Fi & Bluetooth: OFF > BOTH > WI-FI > BLUETOOTH > OFF.
        let mut radios = Vec::new();
        for _ in 0..4 {
            s.change(13, true);
            radios.push((s.show_wifi, s.show_bluetooth));
        }
        assert_eq!(radios, [(true, true), (true, false), (false, true), (false, false)]);
        s.change(13, false);
        assert_eq!((s.show_wifi, s.show_bluetooth), (false, true));
        // Apps: OFF > ALL > RUNNING > OFF.
        s.change(15, true);
        assert!(s.show_integrations && !s.apps_running_only);
        s.change(15, true);
        assert!(s.show_integrations && s.apps_running_only);
        s.change(15, true);
        assert!(!s.show_integrations && !s.apps_running_only);
        // Clock & Battery and Icons swap places.
        assert!(!s.clock_on_top && !s.icons_on_bottom);
        assert!(s.change(32, true) && s.clock_on_top);
        assert!(s.change(33, true) && s.icons_on_bottom);
        let restored: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(restored, s);
    }
    #[test]
    fn old_status_defaults_are_migrated_without_resurrecting_permission_prompts() {
        let legacy = r#"{"ff_speed":3,"show_wifi":true,"show_bluetooth":true,"show_integrations":true}"#;
        let dir = std::env::temp_dir().join(format!("kpa-status-opt-in-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(NAME), legacy).unwrap();
        let mut migrated = Settings::load(&dir);
        assert_eq!(migrated.ff_speed, 3);
        assert!(!migrated.show_wifi && !migrated.show_bluetooth && !migrated.show_integrations);
        assert!(migrated.status_opt_in_v2);
        migrated.change(14, true);
        migrated.save(&dir).unwrap();
        let restored = Settings::load(&dir);
        assert!(!restored.show_wifi && restored.show_bluetooth && !restored.show_integrations);
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn roundtrip_and_invalid_values() {
        let dir = std::env::temp_dir().join(format!("slot-konkr-dev14-{}", std::process::id()));
        // Already past the one-time 3:2 home overlay change.
        let mut s = Settings { home_bezel_three_two: true, gb_palette_twb64: true,
            gb_overlays_watomsk: true, simpletex_gba_colour: true, gbav_fsr: true,
            ..Settings::default() };
        s.change(0, true); s.change(6, true); s.gb_palette = 47;
        s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir), s);
        fs::write(dir.join(NAME), r#"{"ff_speed":99,"shader_gba":"simpletex","gb_palette":89}"#).unwrap();
        let s = Settings::load(&dir);
        assert_eq!(s.ff_speed, 2);
        assert_eq!(s.shader_gba, Shader::Off);
        assert_eq!(s.gb_palette, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_old_gbc_green_default_moves_to_twb64_game_com_once() {
        let dir = std::env::temp_dir().join(format!("slot-konkr-twb64-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(NAME), r#"{"gb_palettes":true,"gb_palette":13}"#).unwrap();
        let mut s = Settings::load(&dir);
        assert_eq!(s.palette_name(), "TWB64 028 Game.com Ver.");
        assert!(s.gb_palette_twb64);
        // A palette picked afterwards, GBC Green included, is kept.
        s.gb_palette = 13;
        s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir).gb_palette, 13);
        fs::write(dir.join(NAME), r#"{"gb_palettes":true,"gb_palette":2}"#).unwrap();
        assert_eq!(Settings::load(&dir).palette_name(), "GB Pocket");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn palette_row_rings_off_then_every_palette_then_off() {
        let mut s = Settings::default();
        assert!(!s.gb_palettes);
        assert!(s.change(3, true));
        assert!(s.gb_palettes && s.gb_palette == 0);
        assert!(s.change(3, false));
        assert!(!s.gb_palettes, "left from the first palette is OFF");
        assert!(s.change(3, false));
        assert_eq!((s.gb_palettes, s.gb_palette as usize), (true, GbPalette::COUNT - 1));
        assert_eq!(s.palette_label(), "TWB64 Game.com");
        assert!(s.change(3, true));
        assert!(!s.gb_palettes, "right from the last palette is OFF");
        s.change(3, true);
        s.change(3, true);
        assert_eq!(s.palette_name(), "DMG Green");
    }

    #[test]
    fn each_bezel_row_offers_only_its_own_watomsk_overlays() {
        use slot_store::Platform;
        let mut s=Settings::default();
        for want in [Bezel::Gb,Bezel::Pocket,Bezel::None,Bezel::Default] {
            s.change(27,true);
            assert_eq!(s.bezel_gb,want);
        }
        for want in [Bezel::Color,Bezel::None,Bezel::Default] {
            s.change(26,true);
            assert_eq!(s.bezel_gbc,want);
        }
        assert_eq!(serde_json::to_string(&Bezel::Pocket).unwrap(),"\"pocket\"");
        // GBA: OFF, then the classic frame first (an old ON reads as it).
        s.change(25,true);
        assert_eq!(s.bezel_gba,Bezel::GbaClassic);
        s.change(25,false);
        assert_eq!(s.bezel_gba,Bezel::None);
        s.bezel_gba=Bezel::Default;
        assert_eq!(s.bezel_for(Platform::Gba),Bezel::GbaClassic);
        s.change(25,true);
        assert_eq!(s.bezel_gba,Bezel::Gba);
        // The home consoles start on their classic frame.
        assert_eq!(s.bezel_for(Platform::Md),Bezel::GenesisClassic);
        s.change(46,true);
        assert_eq!(s.bezel_md,Bezel::Genesis);
        s.change(44,false);
        assert_eq!(s.bezel_nes,Bezel::None);
        s.change(45,false);
        assert_eq!(s.bezel_snes,Bezel::None);
        for bezel in Bezel::list(Platform::Snes).iter().skip(1) {
            assert!(bezel.watomsk().is_some());
        }
        // Old settings files without the new fields keep the defaults.
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.bezel_nes,Bezel::Nes);
        // A grey overlay saved by an earlier beta reads as the black one.
        let grey: Settings = serde_json::from_str(
            r#"{"bezel_gb":"pocket_grey","bezel_gba":"gba_sp_grey","bezel_md":"mega_drive_grey","bezel_snes":"snes_gray_logo_alt_grey"}"#).unwrap();
        assert_eq!((grey.bezel_gb,grey.bezel_gba,grey.bezel_md,grey.bezel_snes),
            (Bezel::Pocket,Bezel::GbaSp,Bezel::MegaDrive,Bezel::SnesGrayLogoAlt));
    }

    #[test]
    fn home_consoles_start_without_an_overlay_on_three_two_screens() {
        assert!(Device::is_three_two((960, 640)) && Device::is_three_two((1620, 1080)));
        assert!(!Device::is_three_two((1240, 1080)) && !Device::is_three_two((1920, 1080)));
        let konkr=Settings::first_install_defaults_for_device(Device::KONKR);
        assert_eq!((konkr.bezel_nes,konkr.bezel_snes,konkr.bezel_md),
            (Bezel::None,Bezel::None,Bezel::None));
        let rpc=Settings::first_install_defaults_for_device(Device::RPC);
        assert_eq!((rpc.bezel_nes,rpc.bezel_snes,rpc.bezel_md),
            (Bezel::Nes,Bezel::Snes,Bezel::GenesisClassic));
        // A beta's saved overlays go off once on 3:2; a later choice stays.
        let root=std::env::temp_dir().join(format!("slot-home-bezel-{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(NAME), r#"{"bezel_nes":"nes","bezel_md":"genesis"}"#).unwrap();
        let mut s=Settings::load_for_device(&root, Device::KONKR);
        assert_eq!((s.bezel_nes,s.bezel_md),(Bezel::None,Bezel::None));
        assert_eq!(Settings::load_for_device(&root, Device::RPC).bezel_md,Bezel::Genesis);
        s.bezel_md=Bezel::MegaDrive;
        s.save(&root).unwrap();
        assert_eq!(Settings::load_for_device(&root, Device::KONKR).bezel_md,Bezel::MegaDrive);
        let _=fs::remove_dir_all(&root);
    }

    #[test]
    fn a_gba_video_cart_has_its_own_screen() {
        let mut s = Settings::default();
        assert_eq!((s.shader_gbav, s.upscaler_gbav, s.colour_correction_gbav),
            (Shader::Lcd3x, Upscaler::Fsr, false));
        s.shader_gba = Shader::Dot;
        s.set_colour_for(slot_store::Platform::Gba, true);
        s.lcd_effect_gba = LcdEffect::Sunlight;
        s.blending_gba = Blending::Mix;
        let video = s.as_gba_video();
        assert_eq!(video.shader_gba, Shader::Lcd3x);
        assert!(!video.colour_for(slot_store::Platform::Gba));
        assert_eq!((video.lcd_effect_gba, video.blending_gba), (LcdEffect::Off, Blending::Off));
        let mut seen = vec![s.upscaler_gbav];
        for _ in 0..5 { assert!(s.change(70, true)); seen.push(s.upscaler_gbav); }
        assert_eq!(seen.first(), seen.last());
        assert!(s.change(70, false) && s.upscaler_gbav == Upscaler::Bilinear);
    }

    #[test]
    fn old_settings_get_the_gba_video_defaults() {
        let dir = std::env::temp_dir().join(format!("slot-gbav-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(NAME), r#"{"shader_gba":"dot"}"#).unwrap();
        let s = Settings::load(&dir);
        assert_eq!((s.shader_gbav, s.upscaler_gbav), (Shader::Lcd3x, Upscaler::Fsr));
        // beta7's saved Anime4K Fast moves to FSR once; a later choice stays.
        fs::write(dir.join(NAME), r#"{"upscaler_gbav":"anime4kfast"}"#).unwrap();
        let mut s = Settings::load(&dir);
        assert_eq!(s.upscaler_gbav, Upscaler::Fsr);
        s.upscaler_gbav = Upscaler::Anime4k;
        s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir).upscaler_gbav, Upscaler::Anime4k);
        let _ = fs::remove_dir_all(&dir);
    }
}

