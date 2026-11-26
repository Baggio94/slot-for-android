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
    /// Recents.png or Favorites.png follow the shelf on screen. Read only:
    /// load turns it into each shelf's own wallpaper (wallpaper_shelves).
    Shelf,
    /// 2.5: Siham's mosaic for the shelf on screen, in the theme's colours
    /// (slot_ui::mosaic), softened by its Blur and lit by its Halo.
    #[default]
    Default,
    /// Read only, from 2.5's first betas: load turns Default Blur, Default
    /// Halo and slot. Blur into Default or slot. with a Blur or a Halo
    /// (WallpaperLook, Siham 2026-11-19).
    Blur,
    Halo,
    /// The wood with stickers that was Default until 2.5.
    Slot,
    SlotBlur,
    File(String),
}
impl WallpaperPick {
    /// None, Default, slot., Random, then the images (Siham 2026-11-19).
    pub fn choices(root: &Path) -> Vec<Self> {
        let mut choices=vec![Self::None, Self::Default, Self::Slot, Self::Random];
        choices.extend(crate::wallpaper::list(root).into_iter().map(Self::File));
        choices
    }
    /// A shelf's choices (Interface > Wallpaper, Shelf on a system, Recents
    /// or Favorites): None stands for General, the shelf showing General's
    /// wallpaper; then the fixed ones, the images, and the shelf's own
    /// Wallpapers/<shelf>.png (upstream's name) when there is one. No Random:
    /// General deals it.
    pub fn shelf_choices(root: &Path, shelf: usize) -> Vec<Option<Self>> {
        let mut choices=vec![None, Some(Self::None), Some(Self::Default), Some(Self::Slot)];
        choices.extend(crate::wallpaper::list(root).into_iter().map(|f| Some(Self::File(f))));
        if let Some(own)=crate::wallpaper::for_shelf(root, shelf) {
            choices.push(Some(Self::File(own)));
        }
        choices
    }
    pub fn shelf_step(current: Option<&Self>, root: &Path, shelf: usize, right: bool) -> Option<Self> {
        let choices=Self::shelf_choices(root, shelf);
        let at=choices.iter().position(|v| v.as_ref()==current).unwrap_or(0);
        let n=choices.len();
        choices[if right {(at+1)%n}else{(at+n-1)%n}].clone()
    }
    pub fn step(&self, root: &Path, right: bool) -> Self {
        let choices=Self::choices(root);
        // An unknown or retired pick (Shelf, a deleted image) steps from Default.
        let at=choices.iter().position(|v| v==self).unwrap_or(1);
        let n=choices.len();
        choices[if right {(at+1)%n}else{(at+n-1)%n}].clone()
    }
    /// Default (and the retired Default Blur and Halo): each shelf shows its
    /// own mosaic.
    pub fn is_mosaic(&self) -> bool {
        matches!(self, Self::Default | Self::Blur | Self::Halo)
    }
    pub fn label(&self) -> String {
        match self {
            Self::None=>"None".into(),
            Self::Random=>"Random".into(),
            Self::Shelf=>"Shelf".into(),
            Self::Default=>"Default".into(),
            Self::Blur=>"Default Blur".into(),
            Self::Halo=>"Default Halo".into(),
            Self::Slot=>"slot.".into(),
            Self::SlotBlur=>"slot. Blur".into(),
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
    /// The Darken that keeps a tinted image as dark as it was.
    pub fn darken(self)->u8 {
        match self { Self::Off=>0, Self::Low=>30, Self::Medium=>50, Self::High=>70 }
    }
}

/// Interface > Wallpaper's Blur, Halo and Tint (`darken`) for General or a shelf, in
/// percent, 0 (OFF) to 100 by tens (Siham 2026-11-19). Blur softens every
/// wallpaper, Halo lights Default's lines, Tint shades an image.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WallpaperLook { pub blur: u8, pub halo: u8, pub darken: u8 }
impl WallpaperLook {
    /// Default Blur and slot. Blur as they were, and Default Halo.
    pub const OLD_DEFAULT_BLUR: u8 = 30;
    pub const OLD_SLOT_BLUR: u8 = 40;
    pub const OLD_HALO: u8 = 70;
    /// Right adds 10 %, left takes 10 % away; they stop at 100 % and OFF.
    pub fn step(value: u8, right: bool) -> u8 {
        let value = value.min(100) / 10 * 10;
        if right { (value + 10).min(100) } else { value.saturating_sub(10) }
    }
    /// How much black covers the image: 100 % leaves a trace of it.
    pub fn darken_alpha(self) -> f32 { self.darken.min(100) as f32 * 0.009 }
    pub fn mosaic(self) -> slot_ui::MosaicLook {
        slot_ui::MosaicLook { halo: self.halo, blur: self.blur }
    }
}

/// Settings.json's own key for the OLED appearance (Settings::save).
const OLED_KEY: &str = "appearance_oled";
/// 2.4.0-menus6's Theme wallpaper, now None, which shows the theme's
/// colour: read only.
const THEME_WALLPAPER_KEY: &str = "wallpaper_theme";

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance { #[default] Dark, Light, Original, Oled }
impl Appearance {
    /// Dark, OLED, Light, then Original: the dark interface with every console
    /// bar in its console's own colours (Siham 2026-11-03).
    /// Right walks Dark, OLED, Light, Original; left the same ring
    /// backwards, as every other setting (Siham 2026-11-14).
    pub fn step(self, right: bool)->Self {
        const ALL: [Appearance; 4] = [Appearance::Dark, Appearance::Oled,
            Appearance::Light, Appearance::Original];
        let at = ALL.iter().position(|a| *a == self).unwrap_or(0);
        ALL[if right { (at + 1) % 4 } else { (at + 3) % 4 }]
    }
    /// Which colours the Default wallpaper's mosaics take.
    pub fn mosaic_theme(self)->slot_ui::MosaicTheme {
        match self {
            Self::Original=>slot_ui::MosaicTheme::Original, Self::Light=>slot_ui::MosaicTheme::Light,
            Self::Dark=>slot_ui::MosaicTheme::Dark, Self::Oled=>slot_ui::MosaicTheme::Oled,
        }
    }
    /// Which set of console bars `slot_ui::bitmaps::home_slot` draws.
    pub fn bar_theme(self)->usize {
        match self { Self::Original=>0, Self::Dark | Self::Oled=>1, Self::Light=>2 }
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
    // slot.'s own overlays for the 3.0 systems, in Watomsk's style (Siham
    // 2026-11-21). Neo Geo Pocket and WonderSwan: AUTO shows the Color
    // logo for a Color game, the plain one otherwise.
    MasterSystem, GameGear,
    NgpAuto, Ngp, NgpColor,
    WsAuto, Ws, WsColor,
    Lynx, LynxGold,
    PokemonMini,
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
    pub const SMS: [Bezel; 2] = [Bezel::None, Bezel::MasterSystem];
    pub const GG: [Bezel; 2] = [Bezel::None, Bezel::GameGear];
    pub const NGP: [Bezel; 4] = [Bezel::None, Bezel::NgpAuto, Bezel::Ngp, Bezel::NgpColor];
    pub const WS: [Bezel; 4] = [Bezel::None, Bezel::WsAuto, Bezel::Ws, Bezel::WsColor];
    pub const LYNX: [Bezel; 3] = [Bezel::None, Bezel::Lynx, Bezel::LynxGold];
    pub const POKEMINI: [Bezel; 2] = [Bezel::None, Bezel::PokemonMini];
    /// PICO-8 has no overlay: its square picture keeps the screen's middle.
    pub const PICO8: [Bezel; 1] = [Bezel::None];
    pub fn list(platform: slot_store::Platform) -> &'static [Bezel] {
        use slot_store::Platform;
        match platform {
            Platform::Gb => &Self::GB,
            Platform::Gbc => &Self::GBC,
            Platform::Gba => &Self::GBA,
            Platform::Nes => &Self::NES,
            Platform::Snes => &Self::SNES,
            Platform::Md => &Self::MD,
            Platform::Sms => &Self::SMS,
            Platform::Gg => &Self::GG,
            Platform::Ngp => &Self::NGP,
            Platform::Ws => &Self::WS,
            Platform::Lynx => &Self::LYNX,
            Platform::Pokemini => &Self::POKEMINI,
            Platform::Pico8 => &Self::PICO8,
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
            Bezel::MasterSystem | Bezel::GameGear | Bezel::NgpAuto | Bezel::Ngp
            | Bezel::NgpColor | Bezel::WsAuto | Bezel::Ws | Bezel::WsColor
            | Bezel::Lynx | Bezel::LynxGold | Bezel::PokemonMini => return None,
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
    /// Which of slot.'s own 3.0 overlays; `colour`: the game is a Neo Geo
    /// Pocket Color or WonderSwan Color one (for AUTO).
    pub fn own(self, colour: bool) -> Option<slot_ui::own_overlay::Design> {
        use slot_ui::own_overlay::Design as D;
        Some(match self {
            Bezel::MasterSystem => D::MasterSystem,
            Bezel::GameGear => D::GameGear,
            Bezel::NgpAuto if colour => D::NeoGeoPocketColor,
            Bezel::NgpAuto | Bezel::Ngp => D::NeoGeoPocket,
            Bezel::NgpColor => D::NeoGeoPocketColor,
            Bezel::WsAuto if colour => D::WonderSwanColor,
            Bezel::WsAuto | Bezel::Ws => D::WonderSwan,
            Bezel::WsColor => D::WonderSwanColor,
            Bezel::Lynx => D::Lynx,
            Bezel::LynxGold => D::LynxGold,
            Bezel::PokemonMini => D::PokemonMini,
            _ => return None,
        })
    }
}

fn bezel_nes_default() -> Bezel { Bezel::Nes }
fn bezel_snes_default() -> Bezel { Bezel::Snes }
fn bezel_md_default() -> Bezel { Bezel::GenesisClassic }
// The 3.0 systems start with their overlay, where it has room.
fn bezel_sms_default() -> Bezel { Bezel::MasterSystem }
fn bezel_gg_default() -> Bezel { Bezel::GameGear }
fn bezel_ngp_default() -> Bezel { Bezel::NgpAuto }
fn bezel_ws_default() -> Bezel { Bezel::WsAuto }
fn bezel_lynx_default() -> Bezel { Bezel::Lynx }
fn bezel_pokemini_default() -> Bezel { Bezel::PokemonMini }

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
    /// The 3.0 handhelds' LCDs (Game Gear, Neo Geo Pocket, WonderSwan,
    /// Lynx): the Game Boys' LCD shaders without their own colours (Siham
    /// 2026-11-21).
    pub const LCD: [Self; 8] = [
        Self::Off, Self::SharpBilinear, Self::Lcd1x, Self::Lcd3x, Self::ZfastLcd,
        Self::Simpletex, Self::Dot, Self::LcdGridV2,
    ];
    /// Pokemon mini: PokeMini draws its own LCD (Dot Matrix), no shader.
    pub const POKEMINI: [Self; 2] = [Self::Off, Self::SharpBilinear];
    /// The 3.0 systems' choices; the Master System keeps the CRT ones.
    pub fn list_3_0(platform: slot_store::Platform) -> &'static [Self] {
        use slot_store::Platform::*;
        match platform {
            Gg | Ngp | Ws | Lynx => &Self::LCD,
            Pokemini => &Self::POKEMINI,
            // PICO-8 takes the handhelds' LCD shaders, Off first (Siham 2026-11-27).
            Pico8 => &Self::LCD,
            _ => &Self::HOME,
        }
    }
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

/// The settings that belong to one handheld: its screen (overlays), its
/// chip (Frame Pacing, Run-Ahead), its buttons and lid (Touchscreen, the RG
/// Rotate's page). Kept apart in a file the slot. folder never copies, so
/// another handheld's never replace them (Siham 2026-11-16, slot. 2.5).
/// Screen Size has its own file, Config/video_mode.ini, kept likewise.
const DEVICE_NAME: &str = "slot-konkr-device.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DeviceSettings {
    bezel_gba: Bezel, bezel_gbc: Bezel, bezel_gb: Bezel,
    bezel_nes: Bezel, bezel_snes: Bezel, bezel_md: Bezel,
    #[serde(default="bezel_sms_default")] bezel_sms: Bezel,
    #[serde(default="bezel_gg_default")] bezel_gg: Bezel,
    #[serde(default="bezel_ngp_default")] bezel_ngp: Bezel,
    #[serde(default="bezel_ws_default")] bezel_ws: Bezel,
    #[serde(default="bezel_lynx_default")] bezel_lynx: Bezel,
    #[serde(default="bezel_pokemini_default")] bezel_pokemini: Bezel,
    #[serde(default)] own_bezels_room: bool,
    home_bezel_three_two: bool,
    responsive_loop: bool, run_ahead_frames: u8,
    rotate_game_cover: u8, rotate_shelf_cover: u8, rotate_dim: u8, rotate_sleep: u8,
    rotate_animations: bool, rotate_game_info: u8,
    touchscreen: bool,
}

impl DeviceSettings {
    fn of(s: &Settings) -> Self {
        DeviceSettings {
            bezel_gba: s.bezel_gba, bezel_gbc: s.bezel_gbc, bezel_gb: s.bezel_gb,
            bezel_nes: s.bezel_nes, bezel_snes: s.bezel_snes, bezel_md: s.bezel_md,
            bezel_sms: s.bezel_sms, bezel_gg: s.bezel_gg, bezel_ngp: s.bezel_ngp,
            bezel_ws: s.bezel_ws, bezel_lynx: s.bezel_lynx, bezel_pokemini: s.bezel_pokemini,
            own_bezels_room: s.own_bezels_room,
            home_bezel_three_two: s.home_bezel_three_two,
            responsive_loop: s.responsive_loop, run_ahead_frames: s.run_ahead_frames,
            rotate_game_cover: s.rotate_game_cover, rotate_shelf_cover: s.rotate_shelf_cover,
            rotate_dim: s.rotate_dim, rotate_sleep: s.rotate_sleep,
            rotate_animations: s.rotate_animations, rotate_game_info: s.rotate_game_info,
            touchscreen: s.touchscreen,
        }
    }
    fn load(dir: &Path) -> Option<Self> {
        serde_json::from_slice(&fs::read(dir.join(DEVICE_NAME)).ok()?).ok()
    }
    /// The settings file with this handheld's own values in place.
    fn merged(dir: &Path, main: Vec<u8>) -> Vec<u8> {
        let Some(own) = fs::read(dir.join(DEVICE_NAME)).ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()) else { return main };
        let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(&main) else { return main };
        let (Some(json_map), Some(own_map)) = (json.as_object_mut(), own.as_object()) else { return main };
        for (key, value) in own_map { json_map.insert(key.clone(), value.clone()); }
        serde_json::to_vec(&json).unwrap_or(main)
    }
    fn save(&self, dir: &Path) -> io::Result<()> {
        let data = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        slot_store::atomic_write(&dir.join(DEVICE_NAME), &data)
    }
}

/// This handheld's Frame Pacing and Run-Ahead (for a game whose own were
/// set on another handheld).
pub fn device_pacing(dir: &Path) -> Option<(bool, u8)> {
    DeviceSettings::load(dir).map(|d| (d.responsive_loop, d.run_ahead_frames))
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
    pub rumble: bool,
    pub rumble_level: u8,
    pub shader_gba: Shader, pub shader_gb: Shader,
    pub shader_gbc: Shader,
    /// The home consoles' CRT shaders (new in 1.3.5: zfast-crt by default).
    pub shader_nes: Shader, pub shader_snes: Shader, pub shader_md: Shader,
    /// Master System (zfast-crt) and Game Gear (no shader: an LCD), 3.0.
    #[serde(default = "shader_sms_default")]
    pub shader_sms: Shader,
    /// The 3.0 handhelds' LCDs: LCD Grid V2 (Game Gear, Neo Geo Pocket,
    /// Lynx), LCD1x (WonderSwan), none for the Pokemon mini (Siham
    /// 2026-11-21).
    #[serde(default = "shader_gg_default")]
    pub shader_gg: Shader,
    #[serde(default = "shader_gg_default")]
    pub shader_ngp: Shader,
    #[serde(default = "shader_ws_default")]
    pub shader_ws: Shader,
    #[serde(default = "shader_gg_default")]
    pub shader_lynx: Shader,
    #[serde(default = "shader_off")]
    pub shader_pokemini: Shader,
    #[serde(default = "shader_off")]
    pub shader_pico8: Shader,
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
    #[serde(default="bezel_sms_default")]
    pub bezel_sms: Bezel,
    #[serde(default="bezel_gg_default")]
    pub bezel_gg: Bezel,
    #[serde(default="bezel_ngp_default")]
    pub bezel_ngp: Bezel,
    #[serde(default="bezel_ws_default")]
    pub bezel_ws: Bezel,
    #[serde(default="bezel_lynx_default")]
    pub bezel_lynx: Bezel,
    #[serde(default="bezel_pokemini_default")]
    pub bezel_pokemini: Bezel,
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
    /// Settings > Handheld (the RG Rotate only), what the closed
    /// Rotate shows: in a game, 0 the cart inserted, 1 the whole cart in the
    /// middle, 2 the game's last frame dimmed; with no game, 0 the drifting
    /// shelf, 1 the last cart played, 2 a cart at random each minute, 3
    /// nothing, 4 a digital clock, 5 an analog clock. By default Full Cart
    /// in a game and Drifting Shelf with none (Siham 2026-11-14).
    pub rotate_game_cover: u8,
    pub rotate_shelf_cover: u8,
    /// Dim after ROTATE_DIM_S[i], then sleep ROTATE_SLEEP_S[i] later.
    pub rotate_dim: u8,
    pub rotate_sleep: u8,
    /// No longer a setting: the lid always animates (Siham 2026-11-15).
    pub rotate_animations: bool,
    /// Playing's Game Info: 0 off (the default), 1 the title and system, 2
    /// the time played, 3 both (Siham 2026-11-15).
    #[serde(default)]
    pub rotate_game_info: u8,
    /// Cart Studio's size step (SELECT), the same for every system; 0, two
    /// big carts, is the default (Siham 2026-11-10).
    #[serde(default)]
    pub cart_studio_size: u8,
    #[serde(default)]
    pub status_opt_in_v2: bool,
    // Slot v1.7 visual settings; independent of saved emulation profiles.
    pub wallpaper: WallpaperPick,
    /// Interface > Wallpaper: a shelf's own wallpaper, by its upstream name
    /// (wallpaper::SHELF_KEYS). A shelf left out shows `wallpaper`, General.
    /// A key of its own, so a version without it keeps every other setting.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub wallpaper_shelves: std::collections::BTreeMap<String, WallpaperPick>,
    /// General's Blur, Halo and Darken (2.5).
    pub wallpaper_look: WallpaperLook,
    /// A shelf's own, by the same names as wallpaper_shelves; a shelf showing
    /// General's wallpaper shows General's look.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub wallpaper_shelf_looks: std::collections::BTreeMap<String, WallpaperLook>,
    /// Read only: Wallpaper Tint became Darken (wallpaper_look) in 2.5.
    #[serde(skip_serializing)]
    pub wallpaper_tint: WallpaperTint,
    pub appearance: Appearance,
    /// Slot v1.7 GENERAL: Recents is off by default; persist its list regardless.
    pub show_recents: bool,
    pub show_titles: bool,
    /// Interface > Shelves (3.0.1-beta12): the shelves page scrolls
    /// sideways (Scroll, the default) or down as a grid.
    #[serde(default)]
    pub shelves_grid: bool,
    /// Interface > 3.0 Test Shelf (3.0.0-cartes1): the 3.0 systems' carts
    /// on a shelf of their own, to put in and take out without a game.
    #[serde(default)]
    pub test_shelf: bool,
    /// Touch on the shelf (runtime/touch.rs): on by default (Siham 2026-11-17).
    pub touchscreen: bool,
    /// Pokemon mini's LCD Filter, PokeMini's own: Dot Matrix, Scanlines,
    /// None (Siham 2026-11-21). Taken at the game's next start.
    #[serde(default)]
    pub pokemini_lcd_filter: u8,
    /// The 3.0 systems' own core options (Siham 2026-11-22), each off or
    /// at the core's own default so games start as on the real console:
    /// Master System and Game Gear's sprite limit removed, the Neo Geo
    /// Pocket's language, the WonderSwan's palette for black-and-white
    /// games, the Pokemon mini's palette and LCD mode.
    #[serde(default)]
    pub no_sprite_limit_sms: bool,
    #[serde(default)]
    pub no_sprite_limit_gg: bool,
    #[serde(default)]
    pub ngp_language: u8,
    #[serde(default)]
    pub ws_palette: u8,
    #[serde(default)]
    pub pokemini_palette: u8,
    #[serde(default)]
    pub pokemini_lcd_mode: u8,
    /// The Master System's left column hidden, its FM sound off, the
    /// WonderSwan's language, the Pokemon mini's screen shake off.
    #[serde(default)]
    pub hide_left_bar_sms: bool,
    /// The NES's left column hidden, and as much on the right so the
    /// picture stays centred (Siham 2026-11-22).
    #[serde(default)]
    pub hide_left_bar_nes: bool,
    #[serde(default)]
    pub fm_off_sms: bool,
    #[serde(default)]
    pub ws_language: u8,
    #[serde(default)]
    pub pokemini_no_shake: bool,
    /// Optional analogue audio colouring per system. Defaults match Libretro.
    #[serde(default)]
    pub nes_audio_filter: bool,
    #[serde(default)]
    pub md_fm_nuked: bool,
    #[serde(default)]
    pub ws_audio_filter: bool,
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
    /// Set once the 3.0 handhelds' LCD shaders were given to settings the
    /// 3.0 betas saved with none.
    #[serde(default)]
    pub lcd_3_0: bool,
    /// Set once rc1's LCD Grid v2 / LCD1x handheld defaults became LCD3x.
    #[serde(default)]
    pub lcd3x_3_0: bool,
    /// Set once the NES's side bars and the Master System's left bar were
    /// hidden by default (Siham 2026-11-23, 3.0.0-rc6).
    #[serde(default)]
    pub side_bars_3_0: bool,
    /// Set once the 3.0 systems whose overlay has thin bars on this screen
    /// were started without it (Siham 2026-11-21).
    #[serde(default)]
    pub own_bezels_room: bool,
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
    #[serde(default)]
    pub blending_sms: Blending,
    #[serde(default)]
    pub blending_gg: Blending,
    #[serde(default)]
    pub blending_ngp: Blending,
    /// The WonderSwan mixes each picture with the one before, as its slow
    /// LCD showed the sprites its games flicker (Siham 2026-11-21).
    #[serde(default = "blending_ws_default")]
    pub blending_ws: Blending,
    #[serde(default)]
    pub blending_lynx: Blending,
    #[serde(default)]
    pub blending_pokemini: Blending,
    #[serde(default)]
    pub blending_pico8: Blending,
    /// LCD Effect of each supported handheld (Systems > [system] > LCD Effect).
    #[serde(default)]
    pub lcd_effect_gba: LcdEffect,
    #[serde(default)]
    pub lcd_effect_gbc: LcdEffect,
    #[serde(default)]
    pub lcd_effect_gb: LcdEffect,
    #[serde(default)]
    pub lcd_effect_gg: LcdEffect,
    #[serde(default)]
    pub lcd_effect_ngp: LcdEffect,
    #[serde(default)]
    pub lcd_effect_ws: LcdEffect,
    #[serde(default)]
    pub lcd_effect_lynx: LcdEffect,
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
fn shader_sms_default() -> Shader { Shader::ZfastCrt }
fn shader_gg_default() -> Shader { Shader::Lcd3x }
fn shader_ws_default() -> Shader { Shader::Lcd3x }
fn shader_off() -> Shader { Shader::Off }
fn blending_ws_default() -> Blending { Blending::Mix }
/// A 3.0 handheld's LCD shader on a fresh install.
fn lcd_3_0_default(platform: slot_store::Platform) -> Shader {
    match platform {
        slot_store::Platform::Pokemini => Shader::Off,
        // LCD3x on every LCD handheld (Siham 2026-11-21).
        _ => Shader::Lcd3x,
    }
}
impl Default for Settings {
    fn default() -> Self {
        // Historical defaults for serde's partial legacy JSON. Do not change:
        // old installs may omit fields and still need their prior values.
        Self { ff_speed: 2, ff_sound: true, colour_correction: false,
            rumble: true, rumble_level: 3, shader_gba: Shader::Off, shader_gb: Shader::Off,
            shader_gbc: Shader::Off,
            shader_nes: Shader::ZfastCrt, shader_snes: Shader::ZfastCrt,
            shader_md: Shader::ZfastCrt,
            shader_sms: Shader::ZfastCrt, shader_gg: Shader::Lcd3x,
            shader_ngp: Shader::Lcd3x, shader_ws: Shader::Lcd3x,
            shader_lynx: Shader::Lcd3x, shader_pokemini: Shader::Off, shader_pico8: Shader::Off,
            colour_correction_gba: false, colour_correction_gb: false,
            colour_correction_gbc: false,
            bezel_gba: Bezel::None, bezel_gbc: Bezel::Default, bezel_gb: Bezel::Default,
            bezel_nes: Bezel::Nes, bezel_snes: Bezel::Snes, bezel_md: Bezel::GenesisClassic,
            bezel_sms: Bezel::MasterSystem, bezel_gg: Bezel::GameGear, bezel_ngp: Bezel::NgpAuto,
            bezel_ws: Bezel::WsAuto, bezel_lynx: Bezel::Lynx, bezel_pokemini: Bezel::PokemonMini,
            gb_palettes: false, gb_palette: 1, rewind: true,
            rewind_speed: 2,
            turbo: true, eject_save: true,
            show_fps: false, responsive_loop: false,
            run_ahead_frames: 0,
            show_wifi: false, show_bluetooth: false, show_integrations: false,
            time_format: TimeFormat::TwentyFourHour,
            clock_on_top: false, icons_on_bottom: false, apps_running_only: false,
            rotate_game_cover: 1, rotate_shelf_cover: 0, rotate_dim: 2, rotate_sleep: 1,
            rotate_animations: true,
            rotate_game_info: 0,
            cart_studio_size: 0,
            status_opt_in_v2: true, wallpaper: WallpaperPick::Default,
            wallpaper_shelves: Default::default(),
            wallpaper_look: WallpaperLook::default(),
            wallpaper_shelf_looks: Default::default(),
            wallpaper_tint: WallpaperTint::Off, appearance: Appearance::Dark,
            show_recents: false, show_titles: false, shelves_grid: false, test_shelf: false, touchscreen: true, home_bezel_three_two: false,
            pokemini_lcd_filter: 0,
            no_sprite_limit_sms: false, no_sprite_limit_gg: false, ngp_language: 0,
            ws_palette: 0, pokemini_palette: 0, pokemini_lcd_mode: 0,
            hide_left_bar_sms: false, hide_left_bar_nes: false, fm_off_sms: false, ws_language: 0,
            pokemini_no_shake: false,
            nes_audio_filter: false, md_fm_nuked: false, ws_audio_filter: false,
            gb_palette_twb64: false, gb_overlays_watomsk: false,
            simpletex_gba_colour: false, lcd_3_0: false, lcd3x_3_0: false, side_bars_3_0: false,
            own_bezels_room: false,
            blending_gba: Blending::Off, blending_gbc: Blending::Off, blending_gb: Blending::Off,
            blending_nes: Blending::Off, blending_snes: Blending::Off, blending_md: Blending::Off,
            blending_sms: Blending::Off, blending_gg: Blending::Off,
            blending_ngp: Blending::Off, blending_ws: Blending::Mix,
            blending_lynx: Blending::Off, blending_pokemini: Blending::Off,
            blending_pico8: Blending::Off,
            lcd_effect_gba: LcdEffect::Off, lcd_effect_gbc: LcdEffect::Off, lcd_effect_gb: LcdEffect::Off,
            lcd_effect_gg: LcdEffect::Off, lcd_effect_ngp: LcdEffect::Off,
            lcd_effect_ws: LcdEffect::Off, lcd_effect_lynx: LcdEffect::Off,
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
            lcd_3_0: true,
            lcd3x_3_0: true,
            // Leave NES side bars and Master System left bar visible by default.
            // These are user choices, not automatic cropping.
            hide_left_bar_nes: false,
            hide_left_bar_sms: false,
            side_bars_3_0: true,
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
        let main = fs::read(dir.join(NAME)).ok();
        // This handheld's own values go in first, so the migrations below
        // see them as they would have before 2.5.
        let mut s = main.map(|b| DeviceSettings::merged(dir, b))
            .and_then(|b| {
                let old_json=serde_json::from_slice::<serde_json::Value>(&b).ok();
                let had_wallpaper_choice=old_json.as_ref()
                    .is_some_and(|v|v.get("wallpaper").is_some());
                Self::lenient(&b).map(|mut s| {
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
                        if s.appearance == Appearance::Dark
                            && json.get(OLED_KEY).and_then(serde_json::Value::as_bool) == Some(true) {
                            s.appearance = Appearance::Oled;
                        }
                        if s.wallpaper == WallpaperPick::Default
                            && json.get(THEME_WALLPAPER_KEY).and_then(serde_json::Value::as_bool) == Some(true) {
                            s.wallpaper = WallpaperPick::None;
                        }
                    }
                    // Beta RC1 imported user.png but lacked an option to turn
                    // it off. Preserve that picture on update to U1 instead
                    // of silently replacing it with Slot's bundled default.
                    if !had_wallpaper_choice &&
                        dir.join("Wallpapers/user.png").is_file() {
                        s.wallpaper=WallpaperPick::File("user.png".into());
                    }
                    s.adopt_shelf_wallpapers(dir);
                    s.adopt_wallpaper_looks();
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
                for shader in [&mut s.shader_nes, &mut s.shader_snes, &mut s.shader_md,
                    &mut s.shader_sms] {
                    if !Shader::HOME.contains(shader) { *shader = Shader::ZfastCrt; }
                }
                use slot_store::Platform as P;
                for platform in [P::Gg, P::Ngp, P::Ws, P::Lynx, P::Pokemini, P::Pico8] {
                    if !Shader::list_3_0(platform).contains(&s.shader_for(platform)) {
                        s.set_shader_for(platform, Shader::Off);
                    }
                }
                // Once: the 3.0 betas' handhelds left with no shader take
                // their LCD defaults (Siham 2026-11-21).
                if !s.lcd_3_0 {
                    for platform in [P::Gg, P::Ngp, P::Ws, P::Lynx] {
                        if s.shader_for(platform) == Shader::Off {
                            s.set_shader_for(platform, lcd_3_0_default(platform));
                        }
                    }
                    if s.blending_ws == Blending::Off { s.blending_ws = Blending::Mix; }
                    s.lcd_3_0 = true;
                }
                // Once: rc1's defaults (LCD Grid v2, LCD1x on the WonderSwan)
                // become LCD3x (Siham 2026-11-21); any other choice stays.
                if !s.lcd3x_3_0 {
                    for platform in [P::Gg, P::Ngp, P::Lynx] {
                        if s.shader_for(platform) == Shader::LcdGridV2 {
                            s.set_shader_for(platform, Shader::Lcd3x);
                        }
                    }
                    if s.shader_ws == Shader::Lcd1x { s.shader_ws = Shader::Lcd3x; }
                    s.lcd3x_3_0 = true;
                }
                // Mark the historical side-bar migration complete, but never
                // change what an existing user chose (including rc5 installs).
                // New installs already default to visible side bars.
                if !s.side_bars_3_0 { s.side_bars_3_0 = true; }
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
            }).unwrap_or_else(|| Self::first_install_defaults_for_device(device));
        // What belongs to this handheld (its screen, chip and buttons) is
        // its own, whatever the slot. folder brought (Siham 2026-11-16).
        match DeviceSettings::load(dir) {
            Some(_) => {}
            None => {
                // First 2.5 start on this handheld: its values so far are its own.
                let _ = DeviceSettings::of(&s).save(dir);
                crate::emulation::EmulationProfile::keep_device_fields_once(dir);
            }
        }
        s
    }
    /// The old Shelf wallpaper becomes each shelf's own: a shelf with its
    /// Wallpapers/<shelf>.png keeps it, the others show Default, as they did.
    fn adopt_shelf_wallpapers(&mut self, root: &Path) {
        if self.wallpaper != WallpaperPick::Shelf { return; }
        for (index, key) in crate::wallpaper::SHELF_KEYS.iter().enumerate() {
            if let Some(file) = crate::wallpaper::for_shelf(root, index) {
                self.wallpaper_shelves.entry((*key).to_string())
                    .or_insert(WallpaperPick::File(file));
            }
        }
        self.wallpaper = WallpaperPick::Default;
    }
    /// The settings in `bytes`, each on its own: a value this version does
    /// not know (written by a newer one on a handheld sharing the slot.
    /// folder) goes back to its default alone, every other value kept.
    fn lenient(bytes: &[u8]) -> Option<Self> {
        if let Ok(whole) = serde_json::from_slice::<Self>(bytes) { return Some(whole); }
        let serde_json::Value::Object(fields) = serde_json::from_slice::<serde_json::Value>(bytes).ok()?
            else { return None };
        let mut kept = serde_json::Map::new();
        let mut dropped = Vec::new();
        for (key, value) in fields {
            let alone = serde_json::Value::Object([(key.clone(), value.clone())].into_iter().collect());
            if serde_json::from_value::<Self>(alone).is_ok() { kept.insert(key, value); }
            else { dropped.push(key); }
        }
        crate::log_warn(&format!("Settings: unknown values back to their defaults: {}", dropped.join(", ")));
        serde_json::from_value(serde_json::Value::Object(kept)).ok()
    }

    /// Default Blur, Default Halo and slot. Blur become Default or slot. with
    /// that Blur or Halo, and Wallpaper Tint each image's Darken, for General
    /// and every shelf (2.5, Siham 2026-11-19).
    fn adopt_wallpaper_looks(&mut self) {
        fn retire(pick: &mut WallpaperPick, look: &mut WallpaperLook) {
            let (to, blur, halo) = match pick {
                WallpaperPick::Blur => (WallpaperPick::Default, WallpaperLook::OLD_DEFAULT_BLUR, 0),
                WallpaperPick::Halo => (WallpaperPick::Default, 0, WallpaperLook::OLD_HALO),
                WallpaperPick::SlotBlur => (WallpaperPick::Slot, WallpaperLook::OLD_SLOT_BLUR, 0),
                _ => return,
            };
            *pick = to;
            if look.blur == 0 { look.blur = blur; }
            if look.halo == 0 { look.halo = halo; }
        }
        let darken = self.wallpaper_tint.darken();
        self.wallpaper_tint = WallpaperTint::Off;
        retire(&mut self.wallpaper, &mut self.wallpaper_look);
        if darken > 0 && self.wallpaper_look.darken == 0 { self.wallpaper_look.darken = darken; }
        let Self { wallpaper_shelves, wallpaper_shelf_looks, .. } = self;
        for (key, pick) in wallpaper_shelves.iter_mut() {
            let look = wallpaper_shelf_looks.entry(key.clone()).or_default();
            retire(pick, look);
            if darken > 0 && look.darken == 0 && matches!(pick, WallpaperPick::File(_)) {
                look.darken = darken;
            }
        }
        wallpaper_shelf_looks.retain(|_, look| *look != WallpaperLook::default());
    }
    /// The look shelf `shelf` shows: its own with a wallpaper of its own,
    /// else General's.
    pub fn look_on(&self, shelf: usize) -> WallpaperLook {
        if self.shelf_wallpaper(shelf).is_none() { return self.wallpaper_look; }
        crate::wallpaper::SHELF_KEYS.get(shelf)
            .and_then(|key| self.wallpaper_shelf_looks.get(*key)).copied().unwrap_or_default()
    }
    /// General's look (None) or a shelf's own, to change.
    pub fn set_look(&mut self, shelf: Option<usize>, look: WallpaperLook) {
        let Some(shelf) = shelf else { self.wallpaper_look = look; return };
        let Some(key) = crate::wallpaper::SHELF_KEYS.get(shelf) else { return };
        if look == WallpaperLook::default() { self.wallpaper_shelf_looks.remove(*key); }
        else { self.wallpaper_shelf_looks.insert((*key).to_string(), look); }
    }
    /// A shelf's own wallpaper, None when it shows General's.
    pub fn shelf_wallpaper(&self, shelf: usize) -> Option<&WallpaperPick> {
        crate::wallpaper::SHELF_KEYS.get(shelf).and_then(|key| self.wallpaper_shelves.get(*key))
    }
    /// What shelf `shelf` shows: its own wallpaper, else General's.
    pub fn wallpaper_on(&self, shelf: usize) -> &WallpaperPick {
        self.shelf_wallpaper(shelf).unwrap_or(&self.wallpaper)
    }
    pub fn set_shelf_wallpaper(&mut self, shelf: usize, pick: Option<WallpaperPick>) {
        let Some(key) = crate::wallpaper::SHELF_KEYS.get(shelf) else { return };
        match pick {
            Some(pick) => { self.wallpaper_shelves.insert((*key).to_string(), pick); }
            None => { self.wallpaper_shelves.remove(*key); }
        }
    }
    pub fn load(dir: &Path) -> Self {
        Self::load_for_device(dir, Device::KONKR)
    }
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        DeviceSettings::of(self).save(dir)?;
        // A version from before OLED reads the whole file or none of it: an
        // appearance it doesn't know would reset every setting. Written as
        // Dark plus a key it ignores, it keeps them and shows Dark.
        // OLED goes in a key of its own, so a version without it reads Dark
        // rather than resetting every setting.
        let oled = self.appearance == Appearance::Oled;
        let bytes = if oled {
            let older = Self { appearance: Appearance::Dark, ..self.clone() };
            let mut json = serde_json::to_value(&older).map_err(io::Error::other)?;
            json[OLED_KEY] = serde_json::json!(true);
            serde_json::to_vec_pretty(&json)
        } else {
            serde_json::to_vec_pretty(&self)
        }.map_err(io::Error::other)?;
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
            slot_store::Platform::Sms=>self.shader_sms,
            slot_store::Platform::Gg=>self.shader_gg,
            slot_store::Platform::Ngp=>self.shader_ngp,
            slot_store::Platform::Ws=>self.shader_ws,
            slot_store::Platform::Lynx=>self.shader_lynx,
            slot_store::Platform::Pokemini=>self.shader_pokemini,
            slot_store::Platform::Pico8=>self.shader_pico8,
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
            slot_store::Platform::Sms=>self.shader_sms=shader,
            slot_store::Platform::Gg=>self.shader_gg=shader,
            slot_store::Platform::Ngp=>self.shader_ngp=shader,
            slot_store::Platform::Ws=>self.shader_ws=shader,
            slot_store::Platform::Lynx=>self.shader_lynx=shader,
            slot_store::Platform::Pokemini=>self.shader_pokemini=shader,
            slot_store::Platform::Pico8=>self.shader_pico8=shader,
        }
    }
    /// The Shader row's key of a home console (47 NES, 48 SNES, 49 MD,
    /// 110 Master System, 111 Game Gear, 115 Neo Geo Pocket, 116 WonderSwan,
    /// 120 Lynx, 121 Pokemon mini).
    pub fn home_shader_key(platform:slot_store::Platform)->Option<usize> {
        match platform {
            slot_store::Platform::Nes=>Some(47),
            slot_store::Platform::Snes=>Some(48),
            slot_store::Platform::Md=>Some(49),
            slot_store::Platform::Sms=>Some(SHADER_SMS_KEY),
            slot_store::Platform::Gg=>Some(SHADER_GG_KEY),
            slot_store::Platform::Ngp=>Some(SHADER_NGP_KEY),
            slot_store::Platform::Ws=>Some(SHADER_WS_KEY),
            slot_store::Platform::Lynx=>Some(SHADER_LYNX_KEY),
            slot_store::Platform::Pokemini=>Some(SHADER_POKEMINI_KEY),
            slot_store::Platform::Pico8=>Some(SHADER_PICO8_KEY),
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
    /// The Settings key of a system's Frame Blending row (60..=65, then
    /// 112 Master System, 113 Game Gear, 117 Neo Geo Pocket, 118 WonderSwan,
    /// 122 Lynx, 123 Pokemon mini).
    pub fn blending_key(platform: slot_store::Platform) -> usize {
        use slot_store::Platform::*;
        match platform { Gba => 60, Gbc => 61, Gb => 62, Nes => 63, Snes => 64, Md => 65,
            Sms => BLENDING_SMS_KEY, Gg => BLENDING_GG_KEY,
            Ngp => BLENDING_NGP_KEY, Ws => BLENDING_WS_KEY,
            Lynx => BLENDING_LYNX_KEY, Pokemini => BLENDING_POKEMINI_KEY,
            Pico8 => BLENDING_PICO8_KEY }
    }

    /// The system a Frame Blending key belongs to.
    pub fn blending_platform(key: usize) -> Option<slot_store::Platform> {
        slot_store::Platform::ALL.into_iter().find(|p| Self::blending_key(*p) == key)
    }

    pub(crate) fn blending_mut(&mut self, platform: slot_store::Platform) -> &mut Blending {
        use slot_store::Platform::*;
        match platform {
            Gba => &mut self.blending_gba, Gbc => &mut self.blending_gbc, Gb => &mut self.blending_gb,
            Nes => &mut self.blending_nes, Snes => &mut self.blending_snes, Md => &mut self.blending_md,
            Sms => &mut self.blending_sms, Gg => &mut self.blending_gg,
            Ngp => &mut self.blending_ngp, Ws => &mut self.blending_ws,
            Lynx => &mut self.blending_lynx, Pokemini => &mut self.blending_pokemini,
            Pico8 => &mut self.blending_pico8,
        }
    }

    /// LCD Effects apply only to the handhelds using slot.'s LCD shaders.
    /// PokeMini already simulates its own LCD in the core.
    pub fn lcd_effect_key(platform: slot_store::Platform) -> Option<usize> {
        use slot_store::Platform::*;
        match platform {
            Gba => Some(66), Gbc => Some(67), Gb => Some(68),
            Gg => Some(LCD_EFFECT_GG_KEY), Ngp => Some(LCD_EFFECT_NGP_KEY),
            Ws => Some(LCD_EFFECT_WS_KEY), Lynx => Some(LCD_EFFECT_LYNX_KEY),
            _ => None,
        }
    }

    pub fn lcd_effect_platform(key: usize) -> Option<slot_store::Platform> {
        use slot_store::Platform::*;
        match key {
            66 => Some(Gba), 67 => Some(Gbc), 68 => Some(Gb),
            LCD_EFFECT_GG_KEY => Some(Gg), LCD_EFFECT_NGP_KEY => Some(Ngp),
            LCD_EFFECT_WS_KEY => Some(Ws), LCD_EFFECT_LYNX_KEY => Some(Lynx),
            _ => None,
        }
    }

    /// LCD Effect, or Mega Drive's Dither Blending.
    pub fn look_key(platform: slot_store::Platform) -> Option<usize> {
        if platform == slot_store::Platform::Md { Some(69) }
        else { Self::lcd_effect_key(platform) }
    }

    pub fn lcd_effect_for(&self, platform: slot_store::Platform) -> LcdEffect {
        use slot_store::Platform::*;
        match platform {
            Gba => self.lcd_effect_gba, Gbc => self.lcd_effect_gbc, Gb => self.lcd_effect_gb,
            Gg => self.lcd_effect_gg, Ngp => self.lcd_effect_ngp,
            Ws => self.lcd_effect_ws, Lynx => self.lcd_effect_lynx,
            _ => LcdEffect::Off,
        }
    }

    pub fn set_lcd_effect_for(&mut self, platform: slot_store::Platform, value: LcdEffect) {
        use slot_store::Platform::*;
        match platform {
            Gba => self.lcd_effect_gba = value, Gbc => self.lcd_effect_gbc = value,
            Gb => self.lcd_effect_gb = value,
            Gg => self.lcd_effect_gg = value, Ngp => self.lcd_effect_ngp = value,
            Ws => self.lcd_effect_ws = value, Lynx => self.lcd_effect_lynx = value,
            _ => {}
        }
    }

    pub fn blending_for(&self, platform: slot_store::Platform) -> Blending {
        use slot_store::Platform::*;
        match platform {
            Gba => self.blending_gba, Gbc => self.blending_gbc, Gb => self.blending_gb,
            Nes => self.blending_nes, Snes => self.blending_snes, Md => self.blending_md,
            Sms => self.blending_sms, Gg => self.blending_gg,
            Ngp => self.blending_ngp, Ws => self.blending_ws,
            Lynx => self.blending_lynx, Pokemini => self.blending_pokemini,
            Pico8 => self.blending_pico8,
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
            slot_store::Platform::Sms=>self.bezel_sms,
            slot_store::Platform::Gg=>self.bezel_gg,
            slot_store::Platform::Ngp=>self.bezel_ngp,
            slot_store::Platform::Ws=>self.bezel_ws,
            slot_store::Platform::Lynx=>self.bezel_lynx,
            slot_store::Platform::Pokemini=>self.bezel_pokemini,
            slot_store::Platform::Pico8=>Bezel::None,
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
            slot_store::Platform::Sms=>self.bezel_sms=value,
            slot_store::Platform::Gg=>self.bezel_gg=value,
            slot_store::Platform::Ngp=>self.bezel_ngp=value,
            slot_store::Platform::Ws=>self.bezel_ws=value,
            slot_store::Platform::Lynx=>self.bezel_lynx=value,
            slot_store::Platform::Pokemini=>self.bezel_pokemini=value,
            slot_store::Platform::Pico8=>{}
        }
    }
    /// The Bezel row's settings key for each system.
    pub fn bezel_key(platform:slot_store::Platform)->usize {
        match platform {
            slot_store::Platform::Gba=>25, slot_store::Platform::Gbc=>26,
            slot_store::Platform::Gb=>27, slot_store::Platform::Nes=>44,
            slot_store::Platform::Snes=>45, slot_store::Platform::Md=>46,
            slot_store::Platform::Sms=>BEZEL_SMS_KEY, slot_store::Platform::Gg=>BEZEL_GG_KEY,
            slot_store::Platform::Ngp=>BEZEL_NGP_KEY, slot_store::Platform::Ws=>BEZEL_WS_KEY,
            slot_store::Platform::Lynx=>BEZEL_LYNX_KEY,
            slot_store::Platform::Pokemini=>BEZEL_POKEMINI_KEY,
            slot_store::Platform::Pico8=>BEZEL_PICO8_KEY,
        }
    }
    /// Once per handheld: a 3.0 system whose overlay would only have thin
    /// bars on this screen starts with it OFF (the Master System on 3:2,
    /// the Pokemon mini on 5:3), as NES, SNES and Mega Drive on 3:2.
    pub fn own_bezels_by_room(&mut self, panel: (u32, u32)) {
        if self.own_bezels_room { return; }
        use slot_store::Platform as P;
        for platform in [P::Sms, P::Gg, P::Ngp, P::Ws, P::Lynx, P::Pokemini] {
            if crate::video_mode::own_room(platform, panel) < crate::video_mode::OWN_ROOM_ON {
                self.set_bezel_for(platform, Bezel::None);
            }
        }
        self.own_bezels_room = true;
    }
    /// The system a Bezel row's key belongs to.
    pub fn bezel_platform(key:usize)->Option<slot_store::Platform> {
        slot_store::Platform::ALL.into_iter().find(|p| Self::bezel_key(*p)==key)
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
            60..=65 | BLENDING_SMS_KEY | BLENDING_GG_KEY | BLENDING_NGP_KEY | BLENDING_WS_KEY
            | BLENDING_LYNX_KEY | BLENDING_POKEMINI_KEY | BLENDING_PICO8_KEY => {
                let Some(platform) = Self::blending_platform(key) else { return false };
                let blending = self.blending_mut(platform);
                *blending = blending.step(right);
            }
            66..=68 | LCD_EFFECT_GG_KEY | LCD_EFFECT_NGP_KEY | LCD_EFFECT_WS_KEY | LCD_EFFECT_LYNX_KEY => {
                let Some(platform) = Self::lcd_effect_platform(key) else { return false };
                self.set_lcd_effect_for(platform, self.lcd_effect_for(platform).step(right));
            }
            80 => self.rotate_game_cover = step_index(self.rotate_game_cover, 3, right),
            // Drifting Shelf, Last Played, Random Cart, Black, Digital
            // Clock, Analog Clock.
            81 => self.rotate_shelf_cover = step_index(self.rotate_shelf_cover, 6, right),
            82 => self.rotate_dim = step_index(self.rotate_dim, ROTATE_DIM_S.len() as u8, right),
            83 => self.rotate_sleep = step_index(self.rotate_sleep, ROTATE_SLEEP_S.len() as u8, right),
            // Game Info: Off, Title, Time, All.
            84 => self.rotate_game_info = step_index(self.rotate_game_info, 4, right),
            70 => self.upscaler_gbav = self.upscaler_gbav.step(right),
            69 => self.dither_blend_md = !self.dither_blend_md,
            25 => self.step_bezel(slot_store::Platform::Gba,right),
            26 => self.step_bezel(slot_store::Platform::Gbc,right),
            27 => self.step_bezel(slot_store::Platform::Gb,right),
            44 => self.step_bezel(slot_store::Platform::Nes,right),
            45 => self.step_bezel(slot_store::Platform::Snes,right),
            46 => self.step_bezel(slot_store::Platform::Md,right),
            POKEMINI_FILTER_KEY => self.pokemini_lcd_filter = step_index(self.pokemini_lcd_filter, 3, right),
            SPRITE_LIMIT_SMS_KEY => self.no_sprite_limit_sms = !self.no_sprite_limit_sms,
            SPRITE_LIMIT_GG_KEY => self.no_sprite_limit_gg = !self.no_sprite_limit_gg,
            NGP_LANGUAGE_KEY => self.ngp_language = step_index(self.ngp_language, 2, right),
            WS_PALETTE_KEY => self.ws_palette = step_index(self.ws_palette, WS_PALETTES.len() as u8, right),
            POKEMINI_PALETTE_KEY => self.pokemini_palette =
                step_index(self.pokemini_palette, POKEMINI_PALETTES.len() as u8, right),
            POKEMINI_LCD_MODE_KEY => self.pokemini_lcd_mode =
                step_index(self.pokemini_lcd_mode, POKEMINI_LCD_MODES.len() as u8, right),
            LEFT_BAR_SMS_KEY => self.hide_left_bar_sms = !self.hide_left_bar_sms,
            LEFT_BAR_NES_KEY => self.hide_left_bar_nes = !self.hide_left_bar_nes,
            FM_SMS_KEY => self.fm_off_sms = !self.fm_off_sms,
            NES_AUDIO_FILTER_KEY => self.nes_audio_filter = !self.nes_audio_filter,
            MD_FM_SOUND_KEY => self.md_fm_nuked = !self.md_fm_nuked,
            WS_AUDIO_FILTER_KEY => self.ws_audio_filter = !self.ws_audio_filter,
            WS_LANGUAGE_KEY => self.ws_language = step_index(self.ws_language, 2, right),
            POKEMINI_SHAKE_KEY => self.pokemini_no_shake = !self.pokemini_no_shake,
            BEZEL_SMS_KEY..=BEZEL_POKEMINI_KEY => {
                let Some(platform) = Self::bezel_platform(key) else { return false };
                self.step_bezel(platform,right);
            }
            47 => self.shader_nes=self.shader_nes.step(&Shader::HOME,right),
            48 => self.shader_snes=self.shader_snes.step(&Shader::HOME,right),
            49 => self.shader_md=self.shader_md.step(&Shader::HOME,right),
            SHADER_SMS_KEY => self.shader_sms=self.shader_sms.step(&Shader::HOME,right),
            SHADER_GG_KEY => self.shader_gg=self.shader_gg.step(&Shader::LCD,right),
            SHADER_NGP_KEY => self.shader_ngp=self.shader_ngp.step(&Shader::LCD,right),
            SHADER_WS_KEY => self.shader_ws=self.shader_ws.step(&Shader::LCD,right),
            SHADER_LYNX_KEY => self.shader_lynx=self.shader_lynx.step(&Shader::LCD,right),
            SHADER_POKEMINI_KEY => self.shader_pokemini=self.shader_pokemini.step(&Shader::POKEMINI,right),
            SHADER_PICO8_KEY => self.shader_pico8=self.shader_pico8.step(&Shader::LCD,right),
            3 => step_palette(&mut self.gb_palettes, &mut self.gb_palette, right),
            4 => {
                let at = SPEEDS.iter().position(|x| *x == self.ff_speed).unwrap_or(0);
                let n = SPEEDS.len();
                self.ff_speed = SPEEDS[if right { (at + 1) % n } else { (at + n - 1) % n }];
            }
            5 => self.ff_sound = !self.ff_sound,
            6 => cycle_rewind_setting(&mut self.rewind, &mut self.rewind_speed, right),
            7 => self.turbo = !self.turbo,
            8 => {
                let at = if self.rumble { self.rumble_level.clamp(1, 4) } else { 0 };
                let next = if right { (at + 1) % 5 } else { (at + 4) % 5 };
                self.rumble = next != 0;
                if next != 0 { self.rumble_level = next; }
            },
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
/// Settings keys of the 3.0 systems' rows (2.5 used up to 107): Default
/// Core, Shader and Frame Blending of the Master System and the Game Gear.
pub const CORE_SMS_KEY: usize = 108;
pub const CORE_GG_KEY: usize = 109;
pub const SHADER_SMS_KEY: usize = 110;
pub const SHADER_GG_KEY: usize = 111;
pub const BLENDING_SMS_KEY: usize = 112;
pub const BLENDING_GG_KEY: usize = 113;
/// Neo Geo Pocket (Default Core, Shader, Frame Blending) and WonderSwan
/// (Shader, Frame Blending: one core only), 3.0.
pub const CORE_NGP_KEY: usize = 114;
pub const SHADER_NGP_KEY: usize = 115;
pub const SHADER_WS_KEY: usize = 116;
pub const BLENDING_NGP_KEY: usize = 117;
pub const BLENDING_WS_KEY: usize = 118;
/// Lynx (Default Core, Shader, Frame Blending) and Pokemon mini (Shader,
/// Frame Blending: one core only), 3.0.
pub const CORE_LYNX_KEY: usize = 119;
pub const SHADER_LYNX_KEY: usize = 120;
pub const SHADER_POKEMINI_KEY: usize = 121;
pub const BLENDING_LYNX_KEY: usize = 122;
pub const BLENDING_POKEMINI_KEY: usize = 123;
/// The 3.0 systems' Bezel rows (slot.'s own overlays, 3.0.0-overlays1).
pub const BEZEL_SMS_KEY: usize = 124;
pub const BEZEL_GG_KEY: usize = 125;
pub const BEZEL_NGP_KEY: usize = 126;
pub const BEZEL_WS_KEY: usize = 127;
pub const BEZEL_LYNX_KEY: usize = 128;
pub const BEZEL_POKEMINI_KEY: usize = 129;
/// Settings > Systems > Pokemon mini > LCD Filter.
pub const POKEMINI_FILTER_KEY: usize = 130;

/// The 3.0 systems' own core options (3.0.0-materials3), as Settings keys.
pub const SPRITE_LIMIT_SMS_KEY: usize = 131;
pub const SPRITE_LIMIT_GG_KEY: usize = 132;
pub const NGP_LANGUAGE_KEY: usize = 133;
pub const WS_PALETTE_KEY: usize = 134;
pub const POKEMINI_PALETTE_KEY: usize = 135;
pub const POKEMINI_LCD_MODE_KEY: usize = 136;
pub const LEFT_BAR_SMS_KEY: usize = 137;
pub const FM_SMS_KEY: usize = 138;
pub const WS_LANGUAGE_KEY: usize = 139;
pub const POKEMINI_SHAKE_KEY: usize = 140;
/// Settings > Systems > NES > Hide Left Bar.
pub const LEFT_BAR_NES_KEY: usize = 141;
/// Settings > Systems: optional audio character, OFF/MAME by default.
pub const NES_AUDIO_FILTER_KEY: usize = 142;
pub const MD_FM_SOUND_KEY: usize = 143;
pub const WS_AUDIO_FILTER_KEY: usize = 144;
pub const LCD_EFFECT_GG_KEY: usize = 145;
pub const LCD_EFFECT_NGP_KEY: usize = 146;
pub const LCD_EFFECT_WS_KEY: usize = 147;
pub const LCD_EFFECT_LYNX_KEY: usize = 148;
/// PICO-8's Shader and Frame Blending rows; its Bezel key never shows (no
/// overlay), but every system has one.
pub const SHADER_PICO8_KEY: usize = 149;
pub const BLENDING_PICO8_KEY: usize = 150;
pub const BEZEL_PICO8_KEY: usize = 151;
/// Settings > Systems > PICO-8 > Default Core: fake-08 or your own PICO-8.
pub const CORE_PICO8_KEY: usize = 152;

/// Beetle Cygne's palettes for black-and-white WonderSwan games: its own
/// grey first, then the three consoles' screens.
pub const WS_PALETTES: [&str; 4] = ["default", "wonderswan", "wondeswan_color", "swancrystal"];
/// PokeMini's palettes, its Default (the real console's) first.
pub const POKEMINI_PALETTES: [&str; 7] = ["Default", "Old", "Monochrome", "Green", "Red", "Blue", "Sepia"];
/// PokeMini's LCD modes: Analog (the real LCD's ghosting) first.
pub const POKEMINI_LCD_MODES: [&str; 3] = ["analog", "3shades", "2shades"];

/// The Pokemon mini's LCD Filter for the next game started (the settings
/// stay with the runtime; the core options are set in game.rs).
pub static POKEMINI_LCD_FILTER: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// The 3.0 systems' core options a game starts with, kept by the runtime
/// whenever Settings change (as POKEMINI_LCD_FILTER).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoreExtras {
    pub no_sprite_limit_sms: bool,
    pub no_sprite_limit_gg: bool,
    pub ngp_language: u8,
    pub ws_palette: u8,
    pub pokemini_filter: u8,
    pub pokemini_palette: u8,
    pub pokemini_lcd_mode: u8,
    pub hide_left_bar_sms: bool,
    pub fm_off_sms: bool,
    pub ws_language: u8,
    pub pokemini_no_shake: bool,
    pub hide_left_bar_nes: bool,
    pub nes_audio_filter: bool,
    pub md_fm_nuked: bool,
    pub ws_audio_filter: bool,
}
pub static CORE_EXTRAS: std::sync::Mutex<CoreExtras> = std::sync::Mutex::new(CoreExtras {
    no_sprite_limit_sms: false, no_sprite_limit_gg: false, ngp_language: 0,
    ws_palette: 0, pokemini_filter: 0, pokemini_palette: 0, pokemini_lcd_mode: 0,
    hide_left_bar_sms: false, fm_off_sms: false, ws_language: 0, pokemini_no_shake: false,
    hide_left_bar_nes: false,
    nes_audio_filter: false, md_fm_nuked: false, ws_audio_filter: false });

impl CoreExtras {
    pub fn of(s: &Settings) -> Self {
        Self { no_sprite_limit_sms: s.no_sprite_limit_sms, no_sprite_limit_gg: s.no_sprite_limit_gg,
            ngp_language: s.ngp_language, ws_palette: s.ws_palette,
            pokemini_filter: s.pokemini_lcd_filter, pokemini_palette: s.pokemini_palette,
            pokemini_lcd_mode: s.pokemini_lcd_mode, hide_left_bar_sms: s.hide_left_bar_sms,
            fm_off_sms: s.fm_off_sms, ws_language: s.ws_language, pokemini_no_shake: s.pokemini_no_shake,
            hide_left_bar_nes: s.hide_left_bar_nes,
            nes_audio_filter: s.nes_audio_filter,
            md_fm_nuked: s.md_fm_nuked,
            ws_audio_filter: s.ws_audio_filter }
    }
    pub fn store(s: &Settings) {
        POKEMINI_LCD_FILTER.store(s.pokemini_lcd_filter, std::sync::atomic::Ordering::Release);
        *CORE_EXTRAS.lock().unwrap_or_else(|e| e.into_inner()) = Self::of(s);
    }
    pub fn current() -> Self { *CORE_EXTRAS.lock().unwrap_or_else(|e| e.into_inner()) }
    /// The core options these choices set for a system's core.
    pub fn options(&self, platform: slot_store::Platform, core: slot_store::Core) -> Vec<(&'static str, &'static str)> {
        use slot_store::{Core, Platform};
        let pick = |list: &[&'static str], at: u8| list[(at as usize).min(list.len() - 1)];
        let nes_bar = if self.hide_left_bar_nes { "8" } else { "0" };
        match (platform, core) {
            (Platform::Nes, Core::Fceumm) => vec![
                ("fceumm_overscan_h_left", nes_bar), ("fceumm_overscan_h_right", nes_bar),
                ("fceumm_sndlowpass", if self.nes_audio_filter { "enabled" } else { "disabled" }),
            ],
            // Nestopia's analogue-output filter is not the RF modulator:
            // map the core's own closest equivalent instead of doing nothing.
            (Platform::Nes, Core::Nestopia) => vec![
                ("nestopia_overscan_h_left", nes_bar), ("nestopia_overscan_h_right", nes_bar),
                ("nestopia_audio_filter", if self.nes_audio_filter { "enabled" } else { "disabled" }),
            ],
            (Platform::Md, Core::GenesisPlusGx) => vec![
                ("genesis_plus_gx_ym2612",
                    if self.md_fm_nuked { "nuked (ym2612)" } else { "mame (ym2612)" }),
            ],
            (Platform::Sms, Core::GenesisPlusGx) => vec![
                ("genesis_plus_gx_no_sprite_limit", if self.no_sprite_limit_sms { "enabled" } else { "disabled" }),
                ("genesis_plus_gx_left_border", if self.hide_left_bar_sms { "left border" } else { "disabled" }),
                ("genesis_plus_gx_ym2413", if self.fm_off_sms { "disabled" } else { "auto" }),
            ],
            (Platform::Gg, Core::GenesisPlusGx) => vec![
                ("genesis_plus_gx_no_sprite_limit", if self.no_sprite_limit_gg { "enabled" } else { "disabled" }),
            ],
            (Platform::Sms, Core::Gearsystem) => vec![
                ("gearsystem_no_sprite_limit", if self.no_sprite_limit_sms { "Enabled" } else { "Disabled" }),
                ("gearsystem_hide_left_bar", if self.hide_left_bar_sms { "Always" } else { "No" }),
                ("gearsystem_ym2413", if self.fm_off_sms { "Disabled" } else { "Auto" }),
            ],
            (Platform::Gg, Core::Gearsystem) => vec![
                ("gearsystem_no_sprite_limit", if self.no_sprite_limit_gg { "Enabled" } else { "Disabled" }),
            ],
            (Platform::Ngp, Core::Neopop) =>
                vec![("ngp_language", if self.ngp_language == 1 { "japanese" } else { "english" })],
            (Platform::Ngp, Core::Race) =>
                vec![("race_language", if self.ngp_language == 1 { "japanese" } else { "english" })],
            (Platform::Ws, _) => vec![
                ("wswan_mono_palette", pick(&WS_PALETTES, self.ws_palette)),
                ("wswan_language", if self.ws_language == 1 { "japanese" } else { "english" }),
                ("wswan_sound_low_pass", if self.ws_audio_filter { "enabled" } else { "disabled" }),
            ],
            (Platform::Pokemini, _) => vec![
                ("pokemini_lcdfilter", pokemini_lcd_filter_option(self.pokemini_filter)),
                ("pokemini_palette", pick(&POKEMINI_PALETTES, self.pokemini_palette)),
                ("pokemini_lcdmode", pick(&POKEMINI_LCD_MODES, self.pokemini_lcd_mode)),
                ("pokemini_screen_shake_lv", if self.pokemini_no_shake { "0" } else { "3" }),
            ],
            _ => Vec::new(),
        }
    }
}

/// PokeMini's pokemini_lcdfilter value for a Settings choice.
pub fn pokemini_lcd_filter_option(choice: u8) -> &'static str {
    match choice { 1 => "scanline", 2 => "none", _ => "dotmatrix" }
}

/// RG Rotate closed: seconds before it dims, then before it sleeps (0 never).
pub const ROTATE_DIM_S: [u16; 4] = [5, 10, 15, 30];
pub const ROTATE_SLEEP_S: [u16; 5] = [30, 60, 120, 300, 0];

/// The next of `count` choices, round.
fn step_index(value: u8, count: u8, right: bool) -> u8 {
    let value = value.min(count - 1);
    if right { (value + 1) % count } else { (value + count - 1) % count }
}

impl Settings {
    pub fn rotate_delays(&self) -> (u16, u16) {
        (ROTATE_DIM_S[usize::from(self.rotate_dim).min(ROTATE_DIM_S.len() - 1)],
         ROTATE_SLEEP_S[usize::from(self.rotate_sleep).min(ROTATE_SLEEP_S.len() - 1)])
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
mod device_tests {
    use super::*;

    #[test]
    fn a_handhelds_screen_chip_and_button_settings_stay_its_own() {
        let root = std::env::temp_dir().join(format!("slot-settings-device-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // This handheld: Responsive, touch on.
        let mut here = Settings::load_for_device(&root, Device::RPC);
        here.touchscreen = true;
        here.responsive_loop = true;
        here.save(&root).unwrap();
        // Another handheld's settings arrive through the slot. folder.
        let mut other = here.clone();
        other.touchscreen = false;
        other.responsive_loop = false;
        other.bezel_snes = Bezel::None;
        other.show_titles = true;
        std::fs::write(root.join(NAME), serde_json::to_vec(&other).unwrap()).unwrap();
        let mixed = Settings::load_for_device(&root, Device::RPC);
        assert!(mixed.touchscreen && mixed.responsive_loop, "this handheld's own");
        assert_eq!(mixed.bezel_snes, here.bezel_snes);
        assert!(mixed.show_titles, "a taste follows");
        let _ = std::fs::remove_dir_all(&root);
    }
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
    fn side_bars_are_visible_by_default_and_existing_choices_survive() {
        for device in [Device::RPC, Device::KONKR] {
            let fresh = Settings::first_install_defaults_for_device(device);
            assert!(!fresh.hide_left_bar_nes && !fresh.hide_left_bar_sms,
                "new installations keep side bars visible");
        }
        let dir = std::env::temp_dir().join(format!("slot-side-bars-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Older settings with side bars visible must not be silently changed.
        let old = Settings { hide_left_bar_nes: false, hide_left_bar_sms: false, side_bars_3_0: false,
            ..Settings::default() };
        old.save(&dir).unwrap();
        let now = Settings::load(&dir);
        assert!(!now.hide_left_bar_nes && !now.hide_left_bar_sms && now.side_bars_3_0);
        // Older settings with side bars hidden explicitly must also be kept.
        Settings { hide_left_bar_nes: true, hide_left_bar_sms: true, ..now }.save(&dir).unwrap();
        let chosen = Settings::load(&dir);
        assert!(chosen.hide_left_bar_nes && chosen.hide_left_bar_sms);
        let _ = std::fs::remove_dir_all(&dir);
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
        // Shelf is gone from General: each shelf picks its own (2.4).
        // None, Default, slot., Random, the images (Siham 2026-11-19).
        assert_eq!(choices, vec![None,Default,Slot,Random,
            File("another.png".into()),File("user.png".into())]);
        assert_eq!(None.step(&root,true), Default);
        assert_eq!(Default.step(&root,true), Slot);
        assert_eq!(Slot.step(&root,true), Random);
        assert_eq!(Random.step(&root,true), File("another.png".into()));
        assert_eq!(Shelf.step(&root,true), Slot);
        assert_eq!(None.step(&root,false), File("user.png".into()));
        assert_eq!(File("user.png".into()).step(&root,true), None);
        assert_eq!(serde_json::to_string(&Halo).unwrap(), r#""halo""#);
        // The wood that was Default until 2.5 keeps its own pick.
        assert_eq!(serde_json::to_string(&Slot).unwrap(), r#""slot""#);
        assert_eq!(serde_json::to_string(&SlotBlur).unwrap(), r#""slot_blur""#);
        assert_eq!(Slot.label(), "slot.");
        assert_eq!(SlotBlur.label(), "slot. Blur");
        assert_eq!(File("missing.png".into()).step(&root,true), Slot);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn each_shelf_picks_its_own_wallpaper_or_shows_general() {
        use WallpaperPick::*;
        let root=std::env::temp_dir().join(format!(
            "slot-shelf-wallpapers-test-{}", std::process::id()));
        let dir=root.join("Wallpapers");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("user.png"), b"placeholder").unwrap();
        std::fs::write(dir.join("NES.png"), b"placeholder").unwrap();
        // NES (3): General first, no Random, its own NES.png last.
        assert_eq!(WallpaperPick::shelf_choices(&root, 3), vec![
            Option::None, Some(None), Some(Default), Some(Slot),
            Some(File("user.png".into())), Some(File("NES.png".into()))]);
        // GB (1) has no file of its own.
        assert_eq!(WallpaperPick::shelf_choices(&root, 1).len(), 5);
        assert_eq!(WallpaperPick::shelf_step(Option::None, &root, 3, true), Some(None));
        assert_eq!(WallpaperPick::shelf_step(Option::None, &root, 3, false),
            Some(File("NES.png".into())));
        let mut s=Settings::first_install_defaults();
        assert_eq!(s.wallpaper, None);
        assert_eq!(s.wallpaper_on(3), &None);
        s.set_shelf_wallpaper(3, Some(Blur));
        s.wallpaper=Default;
        assert_eq!(s.wallpaper_on(3), &Blur);
        assert_eq!(s.wallpaper_on(0), &Default);
        let saved=serde_json::to_string(&s).unwrap();
        assert!(saved.contains(r#""wallpaper_shelves":{"NES":"blur"}"#), "{saved}");
        let restored:Settings=serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.wallpaper_on(3), &Blur);
        s.set_shelf_wallpaper(3, Option::None);
        assert_eq!(s.wallpaper_on(3), &Default);
        assert!(!serde_json::to_string(&s).unwrap().contains("wallpaper_shelves"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_value_from_a_newer_version_resets_alone() {
        let root=std::env::temp_dir().join(format!(
            "slot-lenient-settings-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(NAME), r#"{"wallpaper":"sparkles","appearance":"light",
            "wallpaper_look":{"blur":40,"halo":0,"darken":0},"show_titles":true,
            "status_opt_in_v2":true}"#).unwrap();
        let s=Settings::load(&root);
        assert_eq!(s.wallpaper, WallpaperPick::default());
        assert_eq!(s.appearance, Appearance::Light);
        assert_eq!(s.wallpaper_look.blur, 40);
        assert!(s.show_titles);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn blur_halo_and_tint_become_each_wallpapers_look() {
        use WallpaperPick::*;
        let root=std::env::temp_dir().join(format!(
            "slot-wallpaper-looks-test-{}", std::process::id()));
        std::fs::create_dir_all(root.join("Wallpapers")).unwrap();
        std::fs::write(root.join("Wallpapers/user.png"), b"placeholder").unwrap();
        std::fs::write(root.join(NAME), r#"{"wallpaper":"blur","wallpaper_tint":"high",
            "wallpaper_shelves":{"NES":"halo","GBA":"slot_blur","GB":{"file":"user.png"},"SNES":"default"},
            "status_opt_in_v2":true}"#).unwrap();
        let s=Settings::load(&root);
        assert_eq!(s.wallpaper, Default);
        assert_eq!(s.wallpaper_look, WallpaperLook { blur: 30, halo: 0, darken: 70 });
        let shelf = |key: &str| crate::wallpaper::SHELF_KEYS.iter().position(|k| *k == key).unwrap();
        assert_eq!(s.wallpaper_on(shelf("NES")), &Default);
        assert_eq!(s.look_on(shelf("NES")), WallpaperLook { blur: 0, halo: 70, darken: 0 });
        assert_eq!(s.wallpaper_on(shelf("GBA")), &Slot);
        assert_eq!(s.look_on(shelf("GBA")).blur, WallpaperLook::OLD_SLOT_BLUR);
        // An image keeps its tint as Darken; a plain Default shelf has no look.
        assert_eq!(s.look_on(shelf("GB")).darken, 70);
        assert_eq!(s.look_on(shelf("SNES")), WallpaperLook::default());
        // A shelf showing General's shows General's look.
        assert_eq!(s.look_on(shelf("GBC")), s.wallpaper_look);
        // Saved and read again, nothing moves.
        s.save(&root).unwrap();
        assert_eq!(Settings::load(&root), s);
        assert_eq!(WallpaperLook::step(0, true), 10);
        assert_eq!(WallpaperLook::step(100, true), 100);
        assert_eq!(WallpaperLook::step(0, false), 0);
        assert_eq!(WallpaperLook::step(35, false), 20);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_2_3_settings_file_keeps_its_values_in_2_4() {
        // A file as 2.3.0 wrote it: its values stay, what 2.4 adds takes its
        // default, and nothing resets the rest.
        let root=std::env::temp_dir().join(format!(
            "slot-2-3-upgrade-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(NAME), r#"{
            "ff_speed":4,"rumble":false,"run_ahead_frames":1,"show_wifi":true,
            "appearance":"original","wallpaper":"none","wallpaper_tint":"medium",
            "show_recents":true,"show_titles":true,"status_opt_in_v2":true,
            "cart_studio_size":2
        }"#).unwrap();
        let s=Settings::load(&root);
        assert_eq!(s.ff_speed, 4);
        assert!(!s.rumble);
        assert_eq!(s.run_ahead_frames, 1);
        assert!(s.show_wifi && s.show_recents && s.show_titles && s.status_opt_in_v2);
        assert_eq!(s.appearance, Appearance::Original);
        assert_eq!(s.wallpaper, WallpaperPick::None);
        // Wallpaper Tint Medium is Darken 50 % now, as dark as it was.
        assert_eq!(s.wallpaper_tint, WallpaperTint::Off);
        assert_eq!(s.wallpaper_look.darken, 50);
        assert!((s.wallpaper_look.darken_alpha() - WallpaperTint::Medium.alpha()).abs() < 0.01);
        assert_eq!(s.cart_studio_size, 2);
        // 2.4's own: no shelf wallpapers yet, Game Info off, animations on.
        assert!(s.wallpaper_shelves.is_empty());
        assert_eq!(s.rotate_game_info, 0);
        assert!(s.rotate_animations);
        // Saved again, 2.3 can still read it: no field it would choke on in
        // the values it knows.
        s.save(&root).unwrap();
        let again=Settings::load(&root);
        assert_eq!(again, s);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_old_shelf_wallpaper_becomes_each_shelfs_own() {
        use WallpaperPick::*;
        let root=std::env::temp_dir().join(format!(
            "slot-shelf-adopt-test-{}", std::process::id()));
        std::fs::create_dir_all(root.join("Wallpapers")).unwrap();
        std::fs::write(root.join("Wallpapers/gba.PNG"), b"placeholder").unwrap();
        std::fs::write(root.join("Wallpapers/Favorites.png"), b"placeholder").unwrap();
        std::fs::write(root.join(NAME), r#"{"wallpaper":"shelf","status_opt_in_v2":true}"#).unwrap();
        let s=Settings::load(&root);
        assert_eq!(s.wallpaper, Default);
        assert_eq!(s.wallpaper_on(0), &File("gba.PNG".into()));
        assert_eq!(s.wallpaper_on(1), &Default);
        let favorites = crate::wallpaper::SHELF_KEYS.iter().position(|k| *k == "Favorites").unwrap();
        assert_eq!(s.wallpaper_on(favorites), &File("Favorites.png".into()));
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
        settings.wallpaper_look.darken=70;
        settings.appearance=Appearance::Light;
        let saved=serde_json::to_string(&settings).unwrap();
        assert!(!saved.contains("wallpaper_tint"), "{saved}");
        let restored:Settings=serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.wallpaper,WallpaperPick::None);
        assert_eq!(restored.wallpaper_look.darken,70);
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
    fn menus6_theme_wallpaper_reads_as_none() {
        let root = std::env::temp_dir().join(format!(
            "slot-theme-wallpaper-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .unwrap().as_nanos()));
        let mut settings = Settings::load(&root);
        settings.ff_speed = 4;
        settings.save(&root).unwrap();
        // 2.4.0-menus6 saved its Theme wallpaper as Default plus a key.
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join(NAME)).unwrap()).unwrap();
        json["wallpaper"] = serde_json::json!("default");
        json[THEME_WALLPAPER_KEY] = serde_json::json!(true);
        fs::write(root.join(NAME), serde_json::to_vec(&json).unwrap()).unwrap();
        let loaded = Settings::load(&root);
        assert_eq!(loaded.wallpaper, WallpaperPick::None);
        assert_eq!(loaded.ff_speed, 4);
        // Saved again, the key is gone: Default stays Default afterwards.
        loaded.save(&root).unwrap();
        assert!(!String::from_utf8(fs::read(root.join(NAME)).unwrap()).unwrap()
            .contains(THEME_WALLPAPER_KEY));
        let mut back = loaded.clone();
        back.wallpaper = WallpaperPick::Default;
        back.save(&root).unwrap();
        assert_eq!(Settings::load(&root).wallpaper, WallpaperPick::Default);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn oled_is_saved_so_a_version_without_it_keeps_every_setting() {
        let root = std::env::temp_dir().join(format!(
            "slot-oled-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .unwrap().as_nanos()));
        let mut settings = Settings::load(&root);
        settings.appearance = Appearance::Oled;
        settings.ff_speed = 4;
        settings.save(&root).unwrap();
        // What 2.3.0 reads: an appearance it knows, the rest untouched.
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join(NAME)).unwrap()).unwrap();
        assert_eq!(json["appearance"], "dark");
        assert_eq!(json["ff_speed"], 4);
        assert_eq!(Settings::load(&root), settings);
        // 2.4.0-menus2 wrote the appearance itself: still OLED.
        let mut menus2 = json.clone();
        menus2["appearance"] = serde_json::json!("oled");
        menus2.as_object_mut().unwrap().remove(OLED_KEY);
        fs::write(root.join(NAME), serde_json::to_vec(&menus2).unwrap()).unwrap();
        assert_eq!(Settings::load(&root).appearance, Appearance::Oled);
        // Back to Dark, the key goes with it.
        settings.appearance = Appearance::Dark;
        settings.save(&root).unwrap();
        assert_eq!(Settings::load(&root).appearance, Appearance::Dark);
        // Light chosen in an older version over a file that still says OLED.
        let mut light = json;
        light["appearance"] = serde_json::json!("light");
        fs::write(root.join(NAME), serde_json::to_vec(&light).unwrap()).unwrap();
        assert_eq!(Settings::load(&root).appearance, Appearance::Light);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn official_wallpaper_tint_alpha_and_theme_toggle_are_exact() {
        assert_eq!(WallpaperTint::ALL.map(WallpaperTint::alpha),
            [0.0,0.25,0.45,0.65]);
        assert_eq!(Appearance::Dark.step(true),Appearance::Oled);
        assert_eq!(Appearance::Oled.step(true),Appearance::Light);
        assert_eq!(Appearance::Oled.bar_theme(),Appearance::Dark.bar_theme());
        assert_eq!(serde_json::from_str::<Appearance>("\"oled\"").unwrap(), Appearance::Oled);
        assert_eq!(Appearance::Light.step(true),Appearance::Original);
        assert_eq!(Appearance::Original.step(true),Appearance::Dark);
        // Left walks the same ring backwards.
        assert_eq!(Appearance::Dark.step(false),Appearance::Original);
        assert_eq!(Appearance::Original.step(false),Appearance::Light);
        assert_eq!(Appearance::Light.step(false),Appearance::Oled);
        assert_eq!(Appearance::Oled.step(false),Appearance::Dark);
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
        let _ = fs::remove_file(dir.join(DEVICE_NAME));
        for old in [r#"{"responsive_pacing":false}"#, r#"{"responsive_pacing":true}"#,
            r#"{"responsive_pacing":true,"display_sync_pacing":true}"#,
            r#"{"hybrid_pacing":true}"#, r#"{"retroarch_clock_pacing":true}"#,
            r#"{"retroarch_sync_pacing":false,"responsive_loop":true}"#] {
            fs::write(dir.join(NAME), old).unwrap();
            let _ = fs::remove_file(dir.join(DEVICE_NAME));
            assert!(!Settings::load(&dir).responsive_loop, "{old}");
        }
        fs::write(dir.join(NAME), r#"{"retroarch_sync_pacing":true,"responsive_loop":true}"#)
            .unwrap();
        let _ = fs::remove_file(dir.join(DEVICE_NAME));
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
        let _ = std::fs::remove_file(root.join(DEVICE_NAME));
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
    fn rumble_levels_cycle_and_legacy_off_stays_off() {
        let mut s: Settings = serde_json::from_str(r#"{"rumble":false}"#).unwrap();
        assert!(!s.rumble);
        assert_eq!(s.rumble_level, 3);
        for level in 1..=4 {
            assert!(s.change(8, true));
            assert!(s.rumble);
            assert_eq!(s.rumble_level, level);
        }
        assert!(s.change(8, true));
        assert!(!s.rumble);
        assert!(s.change(8, false));
        assert!(s.rumble);
        assert_eq!(s.rumble_level, 4);
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
    fn handheld_lcd_effects_are_independent_and_off_for_old_settings() {
        use slot_store::Platform::*;
        let old: Settings = serde_json::from_str("{}").unwrap();
        for platform in [Gg, Ngp, Ws, Lynx] {
            let mut s = old.clone();
            let key = Settings::lcd_effect_key(platform).unwrap();
            assert_eq!(Settings::lcd_effect_platform(key), Some(platform));
            assert_eq!(s.lcd_effect_for(platform), LcdEffect::Off);
            assert!(s.change(key, true));
            assert_eq!(s.lcd_effect_for(platform), LcdEffect::Transparency);
            for other in slot_store::Platform::ALL {
                if other != platform { assert_eq!(s.lcd_effect_for(other), LcdEffect::Off); }
            }
            assert!(s.change(key, true));
            assert_eq!(s.lcd_effect_for(platform), LcdEffect::Sunlight);
            assert_eq!(serde_json::from_str::<Settings>(&serde_json::to_string(&s).unwrap()).unwrap(), s);
            assert!(s.change(key, true));
            assert_eq!(s.lcd_effect_for(platform), LcdEffect::Off);
        }
        for platform in [Nes, Snes, Md, Sms, Pokemini] {
            assert_eq!(Settings::lcd_effect_key(platform), None);
            let mut s = old.clone();
            s.set_lcd_effect_for(platform, LcdEffect::Sunlight);
            assert_eq!(s, old);
        }
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
    fn the_3_0_handhelds_take_their_lcd_shaders_once() {
        let dir = std::env::temp_dir().join(format!("slot-lcd-3-0-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A 3.0 beta's settings: no shader on the handhelds, a CRT one left
        // on the Game Gear by an older beta's list.
        fs::write(dir.join(NAME), r#"{"shader_gg":"zfastcrt","shader_ngp":"off","shader_ws":"off",
            "shader_lynx":"off","shader_pokemini":"off","blending_ws":"Off"}"#).unwrap();
        let mut s = Settings::load(&dir);
        assert_eq!((s.shader_gg, s.shader_ngp, s.shader_ws, s.shader_lynx, s.shader_pokemini),
            (Shader::Lcd3x, Shader::Lcd3x, Shader::Lcd3x, Shader::Lcd3x, Shader::Off));
        assert_eq!(s.blending_ws, Blending::Mix);
        // Off chosen afterwards stays Off.
        s.shader_ngp = Shader::Off;
        s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir).shader_ngp, Shader::Off);
        // Each list: Simpletex for the LCDs, nothing but Off and Sharp
        // Bilinear for the Pokemon mini.
        assert!(Shader::LCD.contains(&Shader::Simpletex));
        let mut s = Settings::default();
        for _ in 0..3 { s.change(SHADER_POKEMINI_KEY, true); }
        assert!(Shader::POKEMINI.contains(&s.shader_pokemini));
        s.change(POKEMINI_FILTER_KEY, true);
        assert_eq!(pokemini_lcd_filter_option(s.pokemini_lcd_filter), "scanline");
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn rc1_lcd_defaults_become_lcd3x_once() {
        let dir = std::env::temp_dir().join(format!("slot-lcd3x-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(NAME), r#"{"lcd_3_0":true,"shader_gg":"lcdgridv2","shader_ngp":"dot",
            "shader_ws":"lcd1x","shader_lynx":"lcdgridv2","shader_pokemini":"off"}"#).unwrap();
        let mut s = Settings::load(&dir);
        assert_eq!((s.shader_gg, s.shader_ngp, s.shader_ws, s.shader_lynx),
            (Shader::Lcd3x, Shader::Dot, Shader::Lcd3x, Shader::Lcd3x));
        // LCD Grid v2 chosen afterwards stays.
        s.shader_gg = Shader::LcdGridV2;
        s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir).shader_gg, Shader::LcdGridV2);
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn roundtrip_and_invalid_values() {
        let dir = std::env::temp_dir().join(format!("slot-konkr-dev14-{}", std::process::id()));
        // Already past the one-time 3:2 home overlay change.
        let mut s = Settings { home_bezel_three_two: true, gb_palette_twb64: true,
            gb_overlays_watomsk: true, simpletex_gba_colour: true, gbav_fsr: true,
            lcd_3_0: true, lcd3x_3_0: true, side_bars_3_0: true, ..Settings::default() };
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
        // Another handheld reading the same file (no own values yet).
        let _ = fs::remove_file(root.join(DEVICE_NAME));
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



/// Audio preferences belong to the portable slot. folder, not the device.
#[cfg(test)]
mod audio_parity_tests {
    use super::*;
    use slot_store::{Core, Platform};

    fn value(options: &[(&str, &str)], key: &str) -> Option<String> {
        options.iter().find(|(k, _)| *k == key).map(|(_, v)| (*v).to_string())
    }

    #[test]
    fn defaults_match_libretro_audio_and_survive_older_settings_json() {
        let s = Settings::default();
        assert!(!s.nes_audio_filter && !s.md_fm_nuked && !s.ws_audio_filter);
        let extras = CoreExtras::of(&s);
        assert_eq!(value(&extras.options(Platform::Nes, Core::Fceumm), "fceumm_sndlowpass"), Some("disabled".into()));
        assert_eq!(value(&extras.options(Platform::Nes, Core::Nestopia), "nestopia_audio_filter"), Some("disabled".into()));
        assert_eq!(value(&extras.options(Platform::Md, Core::GenesisPlusGx), "genesis_plus_gx_ym2612"), Some("mame (ym2612)".into()));
        assert_eq!(value(&extras.options(Platform::Ws, Core::BeetleCygne), "wswan_sound_low_pass"), Some("disabled".into()));

        let mut json = serde_json::to_value(s).expect("settings serialized");
        let object = json.as_object_mut().expect("object");
        for field in ["nes_audio_filter", "md_fm_nuked", "ws_audio_filter"] {
            object.remove(field);
        }
        let restored: Settings = serde_json::from_value(json).expect("older settings");
        assert!(!restored.nes_audio_filter && !restored.md_fm_nuked && !restored.ws_audio_filter);
    }

    #[test]
    fn audio_rows_toggle_and_apply_only_to_supported_cores() {
        let mut s = Settings::default();
        s.change(NES_AUDIO_FILTER_KEY, true);
        s.change(MD_FM_SOUND_KEY, true);
        s.change(WS_AUDIO_FILTER_KEY, true);
        assert!(s.nes_audio_filter && s.md_fm_nuked && s.ws_audio_filter);
        let extras = CoreExtras::of(&s);
        assert_eq!(value(&extras.options(Platform::Nes, Core::Fceumm), "fceumm_sndlowpass"), Some("enabled".into()));
        assert_eq!(value(&extras.options(Platform::Nes, Core::Nestopia), "nestopia_audio_filter"), Some("enabled".into()));
        assert_eq!(value(&extras.options(Platform::Md, Core::GenesisPlusGx), "genesis_plus_gx_ym2612"), Some("nuked (ym2612)".into()));
        assert!(value(&extras.options(Platform::Md, Core::Picodrive), "genesis_plus_gx_ym2612").is_none());
        assert_eq!(value(&extras.options(Platform::Ws, Core::BeetleCygne), "wswan_sound_low_pass"), Some("enabled".into()));

        let stored = serde_json::to_string(&s).expect("serialized");
        let loaded: Settings = serde_json::from_str(&stored).expect("reloaded");
        assert!(loaded.nes_audio_filter && loaded.md_fm_nuked && loaded.ws_audio_filter);
    }
}
