use std::path::Path;

use slot_gfx::{ScreenEffect, SRC_H, SRC_W, WHOLE_TEXTURE};
use slot_store::{ini, Platform};

use slot_ui::own_overlay::{self, Place};
use slot_ui::watomsk::{self, Shape};

use crate::settings::{Bezel, Shader};

pub const VIDEO_MODE_FILE: &str = "Config/video_mode.ini";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum VideoMode {
    #[default]
    Actual,
    /// Non-integer: the full panel height at the platform's own aspect ratio.
    Aspect,
    Stretch,
}

impl VideoMode {
    pub fn as_str(self) -> &'static str {
        match self {
            VideoMode::Actual => "actual",
            VideoMode::Aspect => "aspect",
            VideoMode::Stretch => "stretch",
        }
    }

    pub fn parse(s: &str) -> Option<VideoMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "actual" => Some(VideoMode::Actual),
            "aspect" => Some(VideoMode::Aspect),
            "stretch" => Some(VideoMode::Stretch),
            _ => None,
        }
    }

    /// L1: one size up, Actual -> Aspect -> Stretch.
    pub fn bigger(self) -> VideoMode {
        match self {
            VideoMode::Actual => VideoMode::Aspect,
            VideoMode::Aspect | VideoMode::Stretch => VideoMode::Stretch,
        }
    }

    /// R1: one size down, Stretch -> Aspect -> Actual.
    pub fn smaller(self) -> VideoMode {
        match self {
            VideoMode::Stretch => VideoMode::Aspect,
            VideoMode::Aspect | VideoMode::Actual => VideoMode::Actual,
        }
    }
    /// One size up on this panel: Original Ratio is left out where it is
    /// the Integer picture itself (Siham 2026-11-22: Integer is kept).
    pub fn bigger_on(self, platform: Platform, panel: (u32, u32)) -> VideoMode {
        let next = self.on(platform, panel).bigger();
        if next == VideoMode::Aspect && integer_is_ratio(platform, panel) { VideoMode::Stretch } else { next }
    }
    /// One size down on this panel, as bigger_on.
    pub fn smaller_on(self, platform: Platform, panel: (u32, u32)) -> VideoMode {
        let next = self.on(platform, panel).smaller();
        if next == VideoMode::Aspect && integer_is_ratio(platform, panel) { VideoMode::Actual } else { next }
    }
    /// This size on this panel: Original Ratio is Integer where the two
    /// are the same picture.
    pub fn on(self, platform: Platform, panel: (u32, u32)) -> VideoMode {
        if self == VideoMode::Aspect && integer_is_ratio(platform, panel) { VideoMode::Actual } else { self }
    }
}

/// Whether Integer and Original Ratio show the very same picture on this
/// panel (the Lynx's 160x102 six times on the KPA's 960x640).
pub fn integer_is_ratio(platform: Platform, panel: (u32, u32)) -> bool {
    // Home consoles can change resolution and aspect between games (or
    // between gameplay and a title screen). Never collapse a user's mode
    // based on the nominal square-pixel system picture.
    if home_console(platform) { return false; }
    let a = picture_rect(platform, VideoMode::Actual, panel);
    let b = picture_rect(platform, VideoMode::Aspect, panel);
    (a[2] - b[2]).abs() < 0.5 && (a[3] - b[3]).abs() < 0.5
}

/// The official Slot bezel's window is cut for integer scale only, the
/// Watomsk overlays' for the full-height picture (integer fits inside it);
/// nothing can frame a stretched picture.
pub fn bezel_fits(bezel: Bezel, mode: VideoMode) -> bool {
    match bezel {
        Bezel::None => true,
        Bezel::Default => mode == VideoMode::Actual,
        _ => mode != VideoMode::Stretch,
    }
}

/// Why `bezel` is hidden in `mode`, for the short Android message.
pub fn bezel_hidden_message(bezel: Bezel, mode: VideoMode) -> Option<&'static str> {
    if bezel_fits(bezel, mode) {
        return None;
    }
    Some(if mode == VideoMode::Stretch {
        "No bezel in Stretch."
    } else {
        "The slot. bezel needs Integer scale."
    })
}

/// Original Ratio is not a whole multiple of the Game Boy picture: plain
/// pixels would come out 4 or 5 screen pixels wide, so a sprite or a scroll
/// moving one pixel per frame would advance unevenly. With no shader picked,
/// draw it with RetroArch's sharp-bilinear instead, which keeps every pixel
/// the same size and the motion regular.
pub fn screen_effect(shader: Shader, platform: Platform, mode: VideoMode) -> ScreenEffect {
    // A home console's picture is never a whole multiple of the panel in
    // both directions once its aspect is applied: always sharp-bilinear.
    if shader == Shader::Off && !platform.handheld() {
        return ScreenEffect::SharpBilinear;
    }
    if shader == Shader::Off && platform != Platform::Gba && mode == VideoMode::Aspect {
        ScreenEffect::SharpBilinear
    } else {
        shader.effect()
    }
}

/// A game's size: its own, else its system's setting, else the automatic
/// default for the actual launch geometry. No automatic choice is persisted.
pub fn video_mode_for(root: &Path, stem: &str, platform: Platform, panel: (u32, u32), square: bool,
    frame: (u32, u32)) -> VideoMode
{
    let read = |key: &str| ini::value(root, VIDEO_MODE_FILE, key).as_deref().and_then(VideoMode::parse);
    read(stem)
        .or_else(|| read(&system_key(platform)))
        .unwrap_or_else(|| game_default_mode(platform, panel, square, frame))
        .on(platform, panel)
}

/// Settings > Systems' Screen Size: the size a system's games start at
/// until one is given its own with L1/R1 or the pause menu. Kept in the
/// same file as the games', under a key no ROM name starts with.
fn system_key(platform: Platform) -> String {
    format!("*{}", platform.dir_name().to_ascii_lowercase())
}

pub fn system_mode(root: &Path, platform: Platform, panel: (u32, u32)) -> VideoMode {
    ini::value(root, VIDEO_MODE_FILE, &system_key(platform))
        .as_deref()
        .and_then(VideoMode::parse)
        .unwrap_or_else(|| default_mode(platform, panel))
        .on(platform, panel)
}

pub fn write_system_mode(root: &Path, platform: Platform, mode: VideoMode) -> std::io::Result<()> {
    ini::write(root, VIDEO_MODE_FILE, &system_key(platform), mode.as_str())
}

/// Whether the panel already has this system's shape (within 1%): the
/// picture then fills it and neither Screen Size nor a bezel is offered,
/// as GBA on the KONKR's 3:2 screen.
pub fn screen_fits(platform: Platform, panel: (u32, u32)) -> bool {
    // The handhelds' LCDs: the picture's own shape.
    let shape = if platform.handheld() || matches!(platform,
        Platform::Gg | Platform::Ngp | Platform::Ws | Platform::Lynx | Platform::Pokemini) {
        let (w, h) = platform.picture();
        w as f32 / h as f32
    } else {
        4.0 / 3.0
    };
    let panel = panel.0.max(1) as f32 / panel.1.max(1) as f32;
    (panel / shape - 1.0).abs() < 0.01
}

/// PokeMini's own LCD scale (1x..6x of 96x64) for this panel: the largest
/// whose picture the panel then shows at a whole multiple, so its dot
/// matrix stays even; 4x, PokeMini's default, when no scale does.
pub fn pokemini_scale(panel: (u32, u32)) -> u32 {
    let (w, h) = panel;
    let fit = (w / 96).min(h / 64);
    // Whole only when the limiting side is filled exactly.
    let exact = fit > 0 && ((w / 96 <= h / 64 && w % 96 == 0) || (h / 64 <= w / 96 && h % 64 == 0));
    if !exact { return 4; }
    (1..=6).rev().find(|s| fit % s == 0).unwrap_or(4)
}

fn home_console(platform: Platform) -> bool {
    matches!(platform, Platform::Nes | Platform::Snes | Platform::Md | Platform::Sms)
}

/// Representative gameplay geometry for the system settings, before a core
/// is loaded. At launch use the core's dimensions instead.
fn default_frame(platform: Platform, square: bool) -> (u32, u32) {
    match platform {
        Platform::Nes if square => (256, 224),
        Platform::Md if square => (256, 224),
        _ => platform.picture(),
    }
}

pub fn default_mode(platform: Platform, panel: (u32, u32)) -> VideoMode {
    let square = square_pixels(platform, panel);
    game_default_mode(platform, panel, square, default_frame(platform, square))
}

/// Home consoles prefer Integer only if the existing crop policy admits
/// the next whole scale (or the picture fits exactly). A smaller Integer
/// picture is not a good automatic default: use Original Ratio instead.
/// Portable systems keep their existing Original Ratio default.
pub fn game_default_mode(platform: Platform, panel: (u32, u32), square: bool,
    frame: (u32, u32)) -> VideoMode
{
    if !home_console(platform) { return VideoMode::Aspect; }
    let mut frame = if frame.0 == 0 || frame.1 == 0 {
        default_frame(platform, square)
    } else { frame };
    // Listed Mega Drive games can boot in a 320-wide title/menu before
    // entering H32 gameplay. Choose for the intended 256-wide gameplay;
    // home_fit still draws those wide screens at 4:3 without crop.
    if square && matches!(platform, Platform::Md | Platform::Sms) && !narrow_frame(frame) {
        frame.0 = 256;
    }
    let slot_gfx::Fit::SmartInteger(aspect, logical, crop) =
        home_fit(platform, VideoMode::Actual, 4.0 / 3.0, frame, square)
    else { return VideoMode::Aspect; };
    let (ow, oh) = (panel.0.max(1) as f32, panel.1.max(1) as f32);
    let h = logical.1.max(1) as f32;
    let fit = (oh / h).min(ow / (h * aspect));
    if fit < 1.0 { return VideoMode::Aspect; }
    let (_, integer_h) = slot_gfx::smart_integer_size(logical, aspect, (ow, oh), crop);
    if integer_h + 1e-4 >= h * fit { VideoMode::Actual } else { VideoMode::Aspect }
}

fn narrow(panel: (u32, u32)) -> bool {
    panel_aspect(panel) < 4.0 / 3.0 * 0.99
}

/// NES and SNES on screens narrower than 4:3 (the Retroid Pocket Classic,
/// the 1:1 screens) show square pixels, 8:7 for a 256x224 picture: Integer
/// is overscale (the next whole multiple up, a few pixels cut at the
/// edges), then Original Ratio and Stretch (Siham, 2026-11-03).
pub fn square_pixels(platform: Platform, panel: (u32, u32)) -> bool {
    matches!(platform, Platform::Nes | Platform::Snes) && narrow(panel)
}

/// A frame 256 pixels wide (248 with the Master System's left bar hidden):
/// the Mega Drive's H32 mode, a Master System game's.
pub fn narrow_frame(frame: (u32, u32)) -> bool {
    matches!(frame.0, 248 | 256)
}

/// Square pixels for the game played: NES and SNES as above, and the
/// Master System and Mega Drive games played 256 pixels wide (`listed`:
/// square_games), on screens narrower than 4:3 (Siham 2026-11-23). Their
/// 256-wide screens show 8:7 with Integer overscale, their 320-wide ones
/// (logos, titles, menus) 4:3. Every other game keeps 4:3.
pub fn square_game(platform: Platform, panel: (u32, u32), listed: bool) -> bool {
    square_pixels(platform, panel)
        || (matches!(platform, Platform::Sms | Platform::Md) && listed && narrow(panel))
}

/// A square-pixel game's shape, for its overlay: a listed Master System or
/// Mega Drive game is an 8:7 game, its 320-wide screens too.
fn square_shape(platform: Platform, frame: (u32, u32)) -> f32 {
    if matches!(platform, Platform::Sms | Platform::Md) && !narrow_frame(frame) {
        8.0 / 7.0
    } else {
        square_aspect(frame)
    }
}

/// NES on screens narrower than 4:3: the cores hide 8 lines at the top and
/// 8 at the bottom, as a television did, so the square-pixel picture is
/// 256x224, 8:7 like the SNES (Siham, 2026-11-12). Wider screens keep all
/// 240 lines, shown at 4:3.
pub fn nes_overscan_lines(panel: (u32, u32)) -> Option<&'static str> {
    square_pixels(Platform::Nes, panel).then_some("8")
}

/// A frame's shape in its console's own pixels: SNES hi-res and interlaced
/// frames (512 wide, 448 tall) count their pixels double.
pub fn square_aspect(frame: (u32, u32)) -> f32 {
    let w = if frame.0 >= 400 { frame.0 / 2 } else { frame.0 };
    let h = if frame.1 >= 400 { frame.1 / 2 } else { frame.1 };
    w.max(1) as f32 / h.max(1) as f32
}

/// The shape a home console's picture is shown at: 4:3 for NES, SNES and
/// Mega Drive, whatever aspect the core reports (FCEUmm and Nestopia report
/// 8:7 PAR, bsnes and Genesis Plus GX their own), except NES and SNES on
/// screens narrower than 4:3, which keep square pixels (Siham, 2026-11-12).
pub fn home_aspect(platform: Platform, core: f32, frame: (u32, u32), square: bool) -> f32 {
    match platform {
        // The NES's side bars and the Master System's left bar hidden: the
        // columns left keep their width, the picture narrows.
        Platform::Nes | Platform::Sms if !square && (240..256).contains(&frame.0) =>
            4.0 / 3.0 * frame.0 as f32 / 256.0,
        Platform::Nes | Platform::Snes | Platform::Md | Platform::Sms if !square => 4.0 / 3.0,
        Platform::Md | Platform::Sms => square_shape(platform, frame),
        // The Game Gear's LCD: its 160x144 pixels are square.
        Platform::Gg => 10.0 / 9.0,
        // NeoPop and RACE report 160x152's square pixels, Beetle Cygne
        // 14:9 (9:14 for a game played upright): kept as they come.
        _ => core,
    }
}

/// Integer's cut, in the frame's own pixels per edge (Siham 2026-11-25):
/// up to 8 on home consoles, the overscan a TV hid (the RG Rotate shows
/// 240 of the SNES's 256 columns at 3x, as before 3.0.1); up to 4 on
/// handhelds, whose screens showed every pixel (issue #132: an upright
/// WonderSwan game at 5x on the Retroid Pocket Classic, 4 lines cut).
pub fn crop_limit(platform: Platform) -> f32 {
    match platform {
        Platform::Nes | Platform::Snes | Platform::Md | Platform::Sms => 8.0,
        _ => 4.0,
    }
}

/// Logical pixels for the crop limit: SNES high-res/interlaced samples
/// and PokeMini's internal LCD multiplier must not count as native dots.
pub fn logical_frame(platform: Platform, frame: (u32, u32)) -> (u32, u32) {
    match platform {
        Platform::Pokemini => platform.picture(),
        Platform::Snes => (if frame.0 >= 400 { frame.0 / 2 } else { frame.0 },
            if frame.1 >= 400 { frame.1 / 2 } else { frame.1 }),
        _ => frame,
    }
}

/// How a home console frame fills the screen in `mode` (`square`: see
/// `square_game`).
pub fn home_fit(platform: Platform, mode: VideoMode, aspect: f32, frame: (u32, u32),
    square: bool) -> slot_gfx::Fit
{
    // A listed game's 320-wide screen: 4:3, as large as it fits.
    if square && matches!(platform, Platform::Sms | Platform::Md) && !narrow_frame(frame) {
        return if mode == VideoMode::Stretch { slot_gfx::Fit::Stretch } else { slot_gfx::Fit::Aspect(4.0 / 3.0) };
    }
    if square {
        let a = square_shape(platform, frame);
        return match mode {
            VideoMode::Actual => slot_gfx::Fit::SmartInteger(a, logical_frame(platform, frame), crop_limit(platform)),
            VideoMode::Aspect => slot_gfx::Fit::Aspect(a),
            VideoMode::Stretch => slot_gfx::Fit::Stretch,
        };
    }
    let aspect = home_aspect(platform, aspect, frame, false);
    match mode {
        VideoMode::Actual => slot_gfx::Fit::SmartInteger(aspect, logical_frame(platform, frame), crop_limit(platform)),
        VideoMode::Aspect => slot_gfx::Fit::Aspect(aspect),
        VideoMode::Stretch => slot_gfx::Fit::Stretch,
    }
}

pub fn write_video_mode(root: &Path, stem: &str, mode: VideoMode) -> std::io::Result<()> {
    ini::write(root, VIDEO_MODE_FILE, stem, mode.as_str())
}

/// Where the picture lands on the real panel, in panel pixels: Integer is
/// the largest whole multiple that fits, Original Ratio the largest size at
/// the system's own shape, Stretch the whole panel. On the KONKR's 960x640
/// this is exactly the 3:2 frame's old geometry.
pub fn picture_rect(platform: Platform, mode: VideoMode, panel: (u32, u32)) -> [f32; 4] {
    let (pw, ph) = (panel.0.max(1) as f32, panel.1.max(1) as f32);
    let (w, h) = platform.picture();
    let (w, h) = (w as f32, h as f32);
    let fit = (pw / w).min(ph / h);
    let (dw, dh) = match mode {
        VideoMode::Actual => slot_gfx::smart_integer_size((w as u32, h as u32), w / h, (pw, ph), crop_limit(platform)),
        VideoMode::Aspect => (w * fit, h * fit),
        VideoMode::Stretch => (pw, ph),
    };
    [((pw - dw) / 2.0).floor(), ((ph - dh) / 2.0).floor(), dw, dh]
}

/// The picture a bezel's window is cut for: the slot. bezel frames the
/// Integer picture, Watomsk's the full Original Ratio one.
pub fn bezel_window(bezel: Bezel, platform: Platform, panel: (u32, u32)) -> [f32; 4] {
    let mode = if bezel == Bezel::Default { VideoMode::Actual } else { VideoMode::Aspect };
    picture_rect(platform, mode, panel)
}

/// A bezel needs room around its window: under 8% of the panel's width on
/// each side (the Retroid Pocket Classic's near-square screen) it would
/// only cover the picture's edge, so it stays hidden.
pub fn bezel_has_room(bezel: Bezel, platform: Platform, panel: (u32, u32)) -> bool {
    let [x, ..] = bezel_window(bezel, platform, panel);
    x >= 0.08 * panel.0 as f32
}

/// How a Watomsk overlay sits on this panel.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum WatomskFit {
    /// His side frame drawn for `Shape`, its window over this rect.
    Side(Shape, [f32; 4]),
    /// The picture at the top of the screen (this rect), his logo centred
    /// in the band under it (GBA and Mega Drive on narrower screens).
    Band([f32; 4]),
    /// slot.'s own overlay for a 3.0 system, drawn around the picture at
    /// this rect (at the top of the screen for a band).
    Own(Place, [f32; 4]),
}

/// Under 4% of the panel on each side (or under it) a frame would only
/// cover the picture's edge: no overlay there.
const ROOM: f32 = 0.04;

fn panel_aspect(panel: (u32, u32)) -> f32 {
    panel.0.max(1) as f32 / panel.1.max(1) as f32
}

/// A home console picture on `panel`, as slot_gfx::fit_window draws it
/// (`top`: at the top of the screen instead of centred; `crop`: Integer's,
/// crop_limit).
pub fn home_rect(mode: VideoMode, aspect: f32, frame: (u32, u32), panel: (u32, u32), top: bool,
    crop: f32) -> [f32; 4]
{
    let (ow, oh) = (panel.0.max(1) as f32, panel.1.max(1) as f32);
    let (tw, th) = (frame.0.max(1) as f32, frame.1.max(1) as f32);
    let a = if aspect > 0.0 { aspect } else { tw / th };
    let (pw, ph) = match mode {
        VideoMode::Aspect => if oh * a <= ow { (oh * a, oh) } else { (ow, ow / a) },
        VideoMode::Actual => slot_gfx::smart_integer_size(frame, a, (ow, oh), crop),
        VideoMode::Stretch => (ow, oh),
    };
    let y = if top && ph <= oh { 0.0 } else { (oh - ph) / 2.0 };
    [(ow - pw) / 2.0, y, pw, ph]
}

/// Where a 3.0 system's own overlay goes on `panel` (Siham 2026-11-21):
/// in the bars beside the Original Ratio picture when they are at least 8 %
/// of the screen's height, else in a band of at least as much under the
/// picture moved to the top, else nowhere. Never in Stretch. The window is
/// the picture of `mode` itself.
pub fn own_place(aspect: f32, frame: (u32, u32), mode: VideoMode, panel: (u32, u32), crop: f32)
    -> Option<(Place, [f32; 4])>
{
    if mode == VideoMode::Stretch { return None; }
    let (pw, ph) = (panel.0.max(1) as f32, panel.1.max(1) as f32);
    let full = home_rect(VideoMode::Aspect, aspect, frame, panel, false, crop);
    let min = own_overlay::MIN_BAR * ph;
    if full[3] >= ph - 0.5 && full[0] >= min {
        let pic = home_rect(mode, aspect, frame, panel, false, crop);
        (pic[0] >= min).then_some((Place::Side, pic))
    } else if full[2] >= pw - 0.5 && ph - full[3] >= min {
        let pic = home_rect(mode, aspect, frame, panel, true, crop);
        (ph - pic[3] >= min).then_some((Place::Band, pic))
    } else {
        None
    }
}

/// The shapes a 3.0 system's picture takes: the WonderSwan's games are
/// also played upright.
fn own_aspects(platform: Platform) -> &'static [f32] {
    match platform {
        Platform::Sms => &[4.0 / 3.0],
        Platform::Gg => &[160.0 / 144.0],
        Platform::Ngp => &[160.0 / 152.0],
        Platform::Ws => &[224.0 / 144.0, 144.0 / 224.0],
        Platform::Lynx => &[160.0 / 102.0],
        Platform::Pokemini => &[96.0 / 64.0],
        _ => &[],
    }
}

/// The room a 3.0 system's own overlay has on `panel`, as a share of the
/// screen's height: its side bar or band at Original Ratio, the widest of
/// its shapes; 0 where it has none.
pub fn own_room(platform: Platform, panel: (u32, u32)) -> f32 {
    let (pw, ph) = (panel.0.max(1) as f32, panel.1.max(1) as f32);
    own_aspects(platform).iter().filter_map(|&aspect| {
        let (place, pic) = own_place(aspect, platform.picture(), VideoMode::Aspect, panel,
            crop_limit(platform))?;
        Some(match place {
            Place::Side => pic[0] / ph,
            Place::Band => (ph - pic[3]) / ph,
        })
    }).fold(0.0, f32::max).min(pw / ph)
}

/// Under this share of the height the bars are thin: the overlay is there
/// but starts OFF, as NES, SNES and Mega Drive on 3:2 screens (Siham
/// 2026-11-21).
pub const OWN_ROOM_ON: f32 = 0.10;

/// Where `bezel`'s Watomsk overlay goes (Siham's plan, 2026-11-03):
/// GB/GBC beside the Original Ratio picture (not on near-square screens);
/// GBA nowhere on 3:2, beside it on wider screens in Original Ratio
/// (Perfect_GBA's layout), at the bottom otherwise; NES/SNES/Mega Drive
/// beside the 4:3 picture on wider screens, nothing on 4:3, and on
/// narrower screens only the Mega Drive, at the bottom. Never in Stretch.
/// `home` is the console's display aspect and frame size.
pub fn watomsk_fit(bezel: Bezel, platform: Platform, mode: VideoMode, panel: (u32, u32),
    home: (f32, (u32, u32))) -> Option<WatomskFit>
{
    if !bezel_fits(bezel, mode) || !Bezel::list(platform).contains(&bezel) { return None; }
    if bezel.own(false).is_some() {
        let (place, pic) = own_place(home.0, home.1, mode, panel, crop_limit(platform))?;
        return Some(WatomskFit::Own(place, pic));
    }
    let design = bezel.watomsk()?;
    let a = panel_aspect(panel);
    let (pw, ph) = (panel.0 as f32, panel.1 as f32);
    let side = |window: [f32; 4]| (window[0] >= ROOM * pw)
        .then(|| WatomskFit::Side(watomsk::nearest_shape(design, panel), window));
    let band = |mut pic: [f32; 4]| {
        pic[1] = 0.0;
        (ph - pic[3] >= ROOM * ph).then_some(WatomskFit::Band(pic))
    };
    match platform {
        Platform::Gb | Platform::Gbc => side(picture_rect(platform, VideoMode::Aspect, panel)),
        Platform::Gba => {
            if screen_fits(platform, panel) { return None; }
            if a > 1.5 && mode == VideoMode::Aspect {
                side(picture_rect(platform, VideoMode::Aspect, panel))
            } else {
                band(picture_rect(platform, mode, panel))
            }
        }
        Platform::Nes | Platform::Snes | Platform::Md => {
            if a > 4.0 / 3.0 * 1.01 {
                side(home_rect(VideoMode::Aspect, 4.0 / 3.0, home.1, panel, false, crop_limit(platform)))
            } else if a < 4.0 / 3.0 * 0.99 && watomsk::has_band(design)
                && (home.0 - 4.0 / 3.0).abs() < 0.01 {
                // Not over a 256x224 game's 8:7 picture, which fills the screen.
                band(home_rect(mode, home.0, home.1, panel, true, crop_limit(platform)))
            } else {
                None
            }
        }
        // Their own overlays, above.
        Platform::Sms | Platform::Gg | Platform::Ngp | Platform::Ws
        | Platform::Lynx | Platform::Pokemini => None,
    }
}

/// Whether a system's Bezel row means anything on this panel.
pub fn bezel_offered(platform: Platform, panel: (u32, u32)) -> bool {
    let a = panel_aspect(panel);
    match platform {
        Platform::Gb | Platform::Gbc =>
            picture_rect(platform, VideoMode::Aspect, panel)[0] >= ROOM * panel.0 as f32,
        Platform::Gba | Platform::Md => !screen_fits(platform, panel),
        Platform::Nes | Platform::Snes => a > 4.0 / 3.0 * 1.01,
        Platform::Sms | Platform::Gg | Platform::Ngp | Platform::Ws
        | Platform::Lynx | Platform::Pokemini => own_aspects(platform).iter().any(|&aspect|
            own_place(aspect, platform.picture(), VideoMode::Aspect, panel, crop_limit(platform)).is_some()),
    }
}

/// The game texture's own picture: GB/GBC sit centred in the 240x160 frame.
pub fn picture_source(platform: Platform) -> [f32; 4] {
    let (w, h) = platform.picture();
    let x = SRC_W.saturating_sub(w) / 2;
    let y = SRC_H.saturating_sub(h) / 2;
    [
        x as f32 / SRC_W as f32,
        y as f32 / SRC_H as f32,
        w as f32 / SRC_W as f32,
        h as f32 / SRC_H as f32,
    ]
}

/// Whether the picture is a whole multiple of the system's pixels.
pub fn whole_scale(platform: Platform, rect: [f32; 4]) -> bool {
    let (w, h) = platform.picture();
    let (kx, ky) = (rect[2] / w as f32, rect[3] / h as f32);
    kx >= 1.0 && (kx - kx.round()).abs() < 0.01 && (ky - ky.round()).abs() < 0.01
}

pub fn source_rect(platform: Platform, mode: VideoMode) -> [f32; 4] {
    let (w, h) = platform.picture();
    if mode == VideoMode::Actual || (w, h) == (SRC_W, SRC_H) {
        return WHOLE_TEXTURE;
    }
    let y = SRC_H.saturating_sub(h) / 2;
    if mode == VideoMode::Aspect {
        // Widen the crop to the panel's shape so the picture keeps its ratio.
        let shown = SRC_W as f32 * h as f32 / SRC_H as f32;
        let x = (SRC_W as f32 - shown) / 2.0;
        return [
            x / SRC_W as f32,
            y as f32 / SRC_H as f32,
            shown / SRC_W as f32,
            h as f32 / SRC_H as f32,
        ];
    }
    let x = SRC_W.saturating_sub(w) / 2;
    [
        x as f32 / SRC_W as f32,
        y as f32 / SRC_H as f32,
        w as f32 / SRC_W as f32,
        h as f32 / SRC_H as f32,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_konkr_keeps_its_three_sizes() {
        let kpa = (960, 640);
        assert_eq!(picture_rect(Platform::Gb, VideoMode::Actual, kpa), [160.0, 32.0, 640.0, 576.0]);
        let [x, y, w, h] = picture_rect(Platform::Gb, VideoMode::Aspect, kpa);
        assert_eq!((y, h), (0.0, 640.0));
        assert!((w - 711.111).abs() < 0.01 && x == 124.0);
        assert_eq!(picture_rect(Platform::Gb, VideoMode::Stretch, kpa), [0.0, 0.0, 960.0, 640.0]);
        for mode in [VideoMode::Actual, VideoMode::Aspect, VideoMode::Stretch] {
            assert_eq!(picture_rect(Platform::Gba, mode, kpa), [0.0, 0.0, 960.0, 640.0]);
        }
        for bezel in [Bezel::Default, Bezel::Gb, Bezel::Color] {
            assert!(bezel_has_room(bezel, Platform::Gb, kpa));
        }
    }

    #[test]
    fn other_panels_use_their_whole_screen() {
        let rpc = (1240, 1080);
        assert_eq!(picture_rect(Platform::Gb, VideoMode::Actual, rpc), [60.0, 36.0, 1120.0, 1008.0]);
        assert_eq!(picture_rect(Platform::Gba, VideoMode::Actual, rpc), [20.0, 140.0, 1200.0, 800.0]);
        assert!(!bezel_has_room(Bezel::Default, Platform::Gb, rpc));
        assert!(!bezel_has_room(Bezel::Gb, Platform::Gbc, rpc));
        let wide = (1920, 1080);
        assert_eq!(picture_rect(Platform::Gba, VideoMode::Actual, wide), [120.0, -20.0, 1680.0, 1120.0]);
        assert!(bezel_has_room(Bezel::Gb, Platform::Gb, wide));
        let four_three = (1440, 1080);
        assert!(bezel_has_room(Bezel::Pocket, Platform::Gb, four_three));
        assert!(whole_scale(Platform::Gb, picture_rect(Platform::Gb, VideoMode::Actual, four_three)));
        assert!(!whole_scale(Platform::Gb, picture_rect(Platform::Gb, VideoMode::Aspect, four_three)));
    }

    #[test]
    fn watomsk_overlays_follow_the_plan_on_every_screen() {
        use WatomskFit::*;
        use VideoMode::*;
        let nes = (4.0 / 3.0, (256, 240));
        let gb = |b, p, m, panel| watomsk_fit(b, p, m, panel, nes);
        // GB/GBC: beside the picture on 3:2, 4:3, 16:9; not on squares.
        assert!(matches!(gb(Bezel::Gb, Platform::Gb, Aspect, (960, 640)), Some(Side(Shape::ThreeTwo, _))));
        assert!(matches!(gb(Bezel::Pocket, Platform::Gb, Actual, (1024, 768)), Some(Side(Shape::FourThree, _))));
        assert!(matches!(gb(Bezel::Color, Platform::Gbc, Aspect, (1920, 1080)), Some(Side(Shape::SixteenNine, _))));
        assert_eq!(gb(Bezel::Gb, Platform::Gb, Aspect, (720, 720)), None);
        assert_eq!(gb(Bezel::Gb, Platform::Gb, Aspect, (1240, 1080)), None);
        assert_eq!(gb(Bezel::Gb, Platform::Gb, Stretch, (960, 640)), None);
        assert_eq!(gb(Bezel::Color, Platform::Gb, Aspect, (960, 640)), None, "not a GB frame");
        // GBA: nothing on 3:2, Perfect_GBA's side frame on 16:9 Original,
        // the band under the picture otherwise.
        let gba = Bezel::GbaClassic;
        assert_eq!(gb(gba, Platform::Gba, Actual, (960, 640)), None);
        assert!(matches!(gb(gba, Platform::Gba, Aspect, (1920, 1080)), Some(Side(Shape::SixteenNine, _))));
        // Smart 7x clips fewer than four native lines; the old band no longer fits.
        assert_eq!(gb(gba, Platform::Gba, Actual, (1920, 1080)), None);
        assert_eq!(gb(gba, Platform::Gba, Aspect, (1024, 768)), Some(Band([0.0, 0.0, 1024.0, 682.6667])));
        assert_eq!(gb(gba, Platform::Gba, Actual, (1240, 1080)), Some(Band([20.0, 0.0, 1200.0, 800.0])));
        assert_eq!(gb(gba, Platform::Gba, Actual, (720, 720)), Some(Band([0.0, 0.0, 720.0, 480.0])));
        // NES/SNES: beside on 3:2 and 16:9, nothing on 4:3 or squares.
        assert!(matches!(gb(Bezel::Nes, Platform::Nes, Aspect, (960, 640)),
            Some(Side(Shape::ThreeTwo, [x, 0.0, _, 640.0])) if (x - 53.33).abs() < 0.1));
        assert!(matches!(gb(Bezel::SnesAlt, Platform::Snes, Actual, (1920, 1080)), Some(Side(Shape::SixteenNine, _))));
        assert_eq!(gb(Bezel::Nes, Platform::Nes, Aspect, (1024, 768)), None);
        assert_eq!(gb(Bezel::Snes, Platform::Snes, Aspect, (1240, 1080)), None);
        // Mega Drive: and at the bottom of squares.
        let md = (4.0 / 3.0, (320, 224));
        assert_eq!(watomsk_fit(Bezel::GenesisClassic, Platform::Md, Aspect, (720, 720), md),
            Some(Band([0.0, 0.0, 720.0, 540.0])));
        assert_eq!(watomsk_fit(Bezel::MegaDrive, Platform::Md, Aspect, (1024, 768), md), None);
        assert!(bezel_offered(Platform::Md, (720, 720)) && !bezel_offered(Platform::Snes, (720, 720)));
        assert!(!bezel_offered(Platform::Gba, (960, 640)) && bezel_offered(Platform::Nes, (960, 640)));
        assert!(!bezel_offered(Platform::Gb, (1240, 1080)));
    }

    #[test]
    fn integer_cuts_up_to_eight_columns_on_home_consoles_four_on_handhelds() {
        let rotate = (720, 720);
        for platform in [Platform::Nes, Platform::Snes, Platform::Md, Platform::Sms] {
            assert_eq!(crop_limit(platform), 8.0);
        }
        // The RG Rotate: NES and SNES at 3x again, 8 columns cut each side.
        for platform in [Platform::Nes, Platform::Snes] {
            assert!(square_pixels(platform, rotate));
            let fit = home_fit(platform, VideoMode::Actual, 4.0 / 3.0, (256, 224), true);
            assert_eq!(fit, slot_gfx::Fit::SmartInteger(8.0 / 7.0, (256, 224), 8.0));
            let pic = home_rect(VideoMode::Actual, 8.0 / 7.0, (256, 224), rotate, false, crop_limit(platform));
            assert!((pic[2] - 768.0).abs() < 0.01 && pic[3] == 672.0, "{platform:?}: {pic:?}");
        }
        for platform in [Platform::Gb, Platform::Gba, Platform::Ws, Platform::Gg, Platform::Pokemini] {
            assert_eq!(crop_limit(platform), 4.0);
        }
    }

    #[test]
    fn smart_integer_uses_the_live_frame_and_overlay_geometry() {
        let panel = (1240, 1080);
        let frame = (144, 224);
        let aspect = 9.0 / 14.0;
        let fit = home_fit(Platform::Ws, VideoMode::Actual, aspect, frame, false);
        assert_eq!(fit, slot_gfx::Fit::SmartInteger(aspect, frame, 4.0));
        let (_, picture) = own_place(aspect, frame, VideoMode::Actual, panel, 4.0).unwrap();
        assert!((picture[0] - 260.0).abs() < 0.001);
        assert!((picture[1] + 20.0).abs() < 0.001);
        assert!((picture[3] - 1120.0).abs() < 0.001);
        assert_eq!(logical_frame(Platform::Snes, (512, 448)), (256, 224));
        assert_eq!(logical_frame(Platform::Pokemini, (480, 320)), (96, 64));
    }

    #[test]
    fn game_boys_start_at_original_ratio_on_every_screen() {
        for panel in [(1240, 1080), (960, 640), (1440, 1080), (1920, 1080), (720, 720)] {
            for platform in [Platform::Gb, Platform::Gbc, Platform::Gba] {
                assert_eq!(default_mode(platform, panel), VideoMode::Aspect, "{platform:?} {panel:?}");
            }
        }
    }

    #[test]
    fn home_defaults_follow_the_real_panel_and_existing_crop_policy() {
        use VideoMode::{Actual as I, Aspect as A};
        let systems = [Platform::Nes, Platform::Snes, Platform::Md, Platform::Sms];
        for (panel, expected) in [
            ((640, 480), [I, A, A, A]),
            ((720, 720), [I, I, A, I]),
            ((960, 640), [A, I, I, A]),
            ((1024, 768), [A, A, A, I]),
            ((1152, 768), [A, A, A, I]),
            ((1280, 720), [I, A, A, I]),
            ((1280, 800), [A, A, A, A]),
            ((1280, 960), [I, A, A, I]),
            ((1240, 1080), [I, I, A, I]),
            ((1440, 1080), [A, I, I, I]),
            ((1620, 1080), [A, I, I, I]),
            ((1920, 1080), [A, I, I, I]),
            ((1920, 1200), [I, A, A, A]),
            ((2560, 1440), [I, A, A, I]),
            ((1080, 1080), [A, A, A, A]),
        ] {
            assert_eq!(systems.map(|p| default_mode(p, panel)), expected, "{panel:?}");
            for p in Platform::ALL {
                if !home_console(p) {
                    assert_eq!(default_mode(p, panel), A, "{p:?} {panel:?}");
                }
            }
        }
        // PAL 240-line games on KPA should not inherit the 224-line default.
        for p in [Platform::Snes, Platform::Md] {
            assert_eq!(game_default_mode(p, (960, 640), false, (256, 240)), A);
        }
        // Hi-res SNES samples are logical 256x224, and listed MD title
        // screens do not override the intended H32 gameplay default.
        assert_eq!(game_default_mode(Platform::Snes, (960, 640), false, (512, 448)), I);
        for panel in [(720, 720), (1240, 1080)] {
            assert_eq!(game_default_mode(Platform::Md, panel, true, (320, 224)), I);
            assert_eq!(game_default_mode(Platform::Md, panel, true, (256, 224)), I);
            assert_eq!(game_default_mode(Platform::Md, panel, false, (320, 224)), A);
            assert_eq!(home_fit(Platform::Md, I, 4.0 / 3.0, (320, 224), true),
                slot_gfx::Fit::Aspect(4.0 / 3.0));
            assert_eq!(game_default_mode(Platform::Sms, panel, true, (248, 192)), I);
        }
        assert_eq!(game_default_mode(Platform::Nes, (720, 720), true, (256, 224)), I);
        assert_eq!(game_default_mode(Platform::Snes, (1080, 1080), true, (256, 224)), A);
        assert_eq!(game_default_mode(Platform::Snes, (100, 100), true, (256, 224)), A);
    }

    #[test]
    fn saved_screen_modes_take_precedence_over_smart_defaults() {
        use VideoMode::{Actual as I, Aspect as A, Stretch as S};
        let root = tempfile::tempdir().unwrap();
        let p = Platform::Snes;
        let panel = (960, 640);
        let mode = |name| video_mode_for(root.path(), name, p, panel, false, (256, 224));
        assert_eq!(mode("game"), I);
        write_system_mode(root.path(), p, A).unwrap();
        assert_eq!(mode("game"), A);
        assert_eq!(system_mode(root.path(), p, panel), A);
        write_video_mode(root.path(), "game", S).unwrap();
        assert_eq!(mode("game"), S);
        assert_eq!(mode("other"), A);
        // Original Ratio must remain selectable even when the nominal
        // system geometry is exact: an individual game may differ.
        assert_eq!(VideoMode::Actual.bigger_on(Platform::Nes, (640, 480)), A);
    }

    #[test]
    fn nes_and_snes_use_square_pixels_on_narrow_screens() {
        use slot_gfx::Fit;
        let rpc = (1240, 1080);
        assert_eq!(default_mode(Platform::Snes, rpc), VideoMode::Actual);
        assert_eq!(default_mode(Platform::Nes, (720, 720)), VideoMode::Actual);
        assert_eq!(default_mode(Platform::Md, rpc), VideoMode::Aspect);
        assert_eq!(default_mode(Platform::Snes, (960, 640)), VideoMode::Actual);
        assert_eq!(home_fit(Platform::Snes, VideoMode::Actual, 4.0 / 3.0, (512, 448), square_pixels(Platform::Snes, rpc)),
            Fit::SmartInteger(256.0 / 224.0, (256, 224), 8.0));
        assert_eq!(home_fit(Platform::Nes, VideoMode::Aspect, 4.0 / 3.0, (256, 240), square_pixels(Platform::Nes, rpc)),
            Fit::Aspect(256.0 / 240.0));
        assert_eq!(home_fit(Platform::Nes, VideoMode::Actual, 4.0 / 3.0, (256, 240), square_pixels(Platform::Nes, (960, 640))),
            Fit::SmartInteger(4.0 / 3.0, (256, 240), 8.0));
    }

    #[test]
    fn home_consoles_show_4_3_whatever_the_core_reports() {
        use slot_gfx::Fit;
        let par = 256.0 * 8.0 / 7.0 / 240.0; // FCEUmm and Nestopia "8:7 PAR"
        for panel in [(960, 640), (1024, 768), (1920, 1080)] {
            for platform in [Platform::Nes, Platform::Snes, Platform::Md] {
                assert_eq!(home_fit(platform, VideoMode::Aspect, par, (256, 240), square_pixels(platform, panel)),
                    Fit::Aspect(4.0 / 3.0), "{platform:?} {panel:?}");
                assert_eq!(home_fit(platform, VideoMode::Actual, 1.5238, (320, 224), square_pixels(platform, panel)),
                    Fit::SmartInteger(4.0 / 3.0, (320, 224), 8.0), "{platform:?} {panel:?}");
            }
        }
        // Narrow screens: NES and SNES keep square pixels, the Mega Drive 4:3.
        let rpc = (1240, 1080);
        assert_eq!(home_fit(Platform::Nes, VideoMode::Aspect, par, (256, 240), square_pixels(Platform::Nes, rpc)),
            Fit::Aspect(256.0 / 240.0));
        assert_eq!(home_fit(Platform::Md, VideoMode::Aspect, 1.5238, (320, 224), square_pixels(Platform::Md, rpc)),
            Fit::Aspect(4.0 / 3.0));
        assert_eq!(home_aspect(Platform::Gba, 1.5, (240, 160), false), 1.5);
    }

    #[test]
    fn listed_master_system_and_mega_drive_games_take_square_pixels_on_narrow_screens() {
        use slot_gfx::Fit;
        let rpc = (1240, 1080);
        for platform in [Platform::Sms, Platform::Md] {
            assert!(square_game(platform, rpc, true), "{platform:?}");
            assert!(!square_game(platform, rpc, false), "{platform:?}: other games keep 4:3");
            assert!(!square_game(platform, (960, 640), true), "{platform:?}: wider screens keep 4:3");
            assert_eq!(home_fit(platform, VideoMode::Actual, 4.0 / 3.0, (256, 224), true),
                Fit::SmartInteger(8.0 / 7.0, (256, 224), 8.0));
        }
        // The cropped left bar: the Integer picture is the 248 columns shown.
        assert_eq!(home_fit(Platform::Sms, VideoMode::Actual, 4.0 / 3.0, (248, 224), true),
            Fit::SmartInteger(248.0 / 224.0, (248, 224), 8.0));
        // A listed game's 320-wide screens (logos, menus): 4:3, the whole picture.
        assert_eq!(home_fit(Platform::Md, VideoMode::Actual, 1.5238, (320, 224), true),
            Fit::Aspect(4.0 / 3.0));
        assert_eq!(home_fit(Platform::Md, VideoMode::Stretch, 1.5238, (320, 224), true), Fit::Stretch);
        // A European game's 240 lines: square too.
        assert_eq!(home_fit(Platform::Md, VideoMode::Actual, 1.0, (256, 240), true),
            Fit::SmartInteger(256.0 / 240.0, (256, 240), 8.0));
        assert_eq!(home_aspect(Platform::Md, 1.5238, (320, 224), true), 8.0 / 7.0);
        assert_eq!(home_aspect(Platform::Md, 1.5238, (320, 224), false), 4.0 / 3.0);
        // Hidden bars on a wider screen: the columns keep their width.
        assert_eq!(home_aspect(Platform::Nes, 1.0, (240, 240), false), 4.0 / 3.0 * 240.0 / 256.0);
        assert_eq!(home_aspect(Platform::Sms, 1.0, (248, 192), false), 4.0 / 3.0 * 248.0 / 256.0);
        // A 320-wide frame isn't a 256-wide game's.
        assert!(!narrow_frame((320, 224)) && narrow_frame((248, 192)));
        // No room beside an 8:7 picture on the Retroid Pocket Classic: no overlay.
        assert!(own_place(8.0 / 7.0, (256, 224), VideoMode::Aspect, rpc, 8.0).is_none());
    }

    #[test]
    fn nes_hides_its_overscan_only_on_narrow_screens() {
        use slot_gfx::Fit;
        for narrow in [(1240, 1080), (720, 720), (1280, 1024)] {
            assert_eq!(nes_overscan_lines(narrow), Some("8"), "{narrow:?}");
        }
        for wide in [(960, 640), (1024, 768), (1920, 1080), (2400, 1080)] {
            assert_eq!(nes_overscan_lines(wide), None, "{wide:?}");
        }
        // The cropped 256x224 frame is 8:7, the same picture as the SNES.
        let rpc = (1240, 1080);
        assert_eq!(home_fit(Platform::Nes, VideoMode::Aspect, 1.0, (256, 224), square_pixels(Platform::Nes, rpc)),
            Fit::Aspect(256.0 / 224.0));
        assert_eq!(home_fit(Platform::Nes, VideoMode::Actual, 1.0, (256, 224), square_pixels(Platform::Nes, rpc)),
            home_fit(Platform::Snes, VideoMode::Actual, 1.0, (256, 224), square_pixels(Platform::Snes, rpc)));
    }

    #[test]
    fn shoulders_step_through_the_three_sizes() {
        assert_eq!(VideoMode::Actual.bigger(), VideoMode::Aspect);
        assert_eq!(VideoMode::Aspect.bigger(), VideoMode::Stretch);
        assert_eq!(VideoMode::Stretch.bigger(), VideoMode::Stretch);
        assert_eq!(VideoMode::Stretch.smaller(), VideoMode::Aspect);
        assert_eq!(VideoMode::Aspect.smaller(), VideoMode::Actual);
        assert_eq!(VideoMode::Actual.smaller(), VideoMode::Actual);
        assert_eq!(VideoMode::parse("aspect"), Some(VideoMode::Aspect));
    }

    #[test]
    fn aspect_keeps_the_game_boy_ratio_on_a_3_2_panel() {
        let [x, y, w, h] = source_rect(Platform::Gb, VideoMode::Aspect);
        let (tw, th) = (w * SRC_W as f32, h * SRC_H as f32);
        assert!((th - 144.0).abs() < 1e-3);
        assert!((tw / th - SRC_W as f32 / SRC_H as f32).abs() < 1e-4);
        assert!((x + w / 2.0 - 0.5).abs() < 1e-6 && (y + h / 2.0 - 0.5).abs() < 1e-6);
        // Shown on the whole panel, the 160 game columns keep 10:9.
        let game_w = 160.0 / tw * 3.0;
        assert!((game_w / 2.0 - 10.0 / 9.0).abs() < 1e-3);
    }

    #[test]
    fn original_ratio_without_shader_keeps_pixels_even() {
        assert_eq!(screen_effect(Shader::Off, Platform::Gb, VideoMode::Aspect),
            ScreenEffect::SharpBilinear);
        assert_eq!(screen_effect(Shader::Off, Platform::Gbc, VideoMode::Aspect),
            ScreenEffect::SharpBilinear);
        assert_eq!(screen_effect(Shader::Off, Platform::Gb, VideoMode::Actual),
            ScreenEffect::None);
        assert_eq!(screen_effect(Shader::Grid, Platform::Gb, VideoMode::Aspect),
            ScreenEffect::Grid);
        assert_eq!(screen_effect(Shader::Off, Platform::Gba, VideoMode::Aspect),
            ScreenEffect::None);
    }

    #[test]
    fn each_bezel_fits_only_its_sizes() {
        use VideoMode::*;
        for mode in [Actual, Aspect, Stretch] {
            assert!(bezel_fits(Bezel::None, mode));
        }
        assert!(bezel_fits(Bezel::Default, Actual));
        assert!(!bezel_fits(Bezel::Default, Aspect));
        for watomsk in [Bezel::Gb, Bezel::Pocket, Bezel::Color] {
            assert!(bezel_fits(watomsk, Actual));
            assert!(bezel_fits(watomsk, Aspect));
            assert!(!bezel_fits(watomsk, Stretch));
            assert_eq!(bezel_hidden_message(watomsk, Stretch), Some("No bezel in Stretch."));
        }
        assert_eq!(bezel_hidden_message(Bezel::Default, Stretch), Some("No bezel in Stretch."));
        assert!(bezel_hidden_message(Bezel::Default, Aspect).unwrap().contains("Integer"));
        assert_eq!(bezel_hidden_message(Bezel::Gb, Aspect), None);
    }

    /// The 3.0 overlays' places, as in Siham's validated previews (v3,
    /// 2026-11-21): bars of at least 8 % of the height, else a band.
    #[test]
    fn the_new_systems_overlays_go_where_there_is_room() {
        use VideoMode::*;
        let panels = [(1620, 1080), (1800, 1080), (1920, 1080), (1440, 1080), (1240, 1080), (1080, 1080)];
        let place = |aspect: f32, frame, panel| own_place(aspect, frame, Aspect, panel, 4.0).map(|(p, _)| p);
        let row = |aspect: f32, frame: (u32, u32)| panels.map(|panel| place(aspect, frame, panel));
        let (s, b, n) = (Some(Place::Side), Some(Place::Band), None);
        assert_eq!(row(4.0 / 3.0, (256, 192)), [s, s, s, n, b, b], "Master System");
        assert_eq!(row(160.0 / 144.0, (160, 144)), [s, s, s, s, n, b], "Game Gear");
        assert_eq!(row(160.0 / 152.0, (160, 152)), [s, s, s, s, n, n], "Neo Geo Pocket");
        assert_eq!(row(224.0 / 144.0, (224, 144)), [n, n, s, b, b, b], "WonderSwan");
        assert_eq!(row(144.0 / 224.0, (144, 224)), [s, s, s, s, s, s], "WonderSwan upright");
        assert_eq!(row(160.0 / 102.0, (160, 102)), [n, n, s, b, b, b], "Lynx");
        assert_eq!(row(1.5, (96, 64)), [n, s, s, b, b, b], "Pokemon mini");
        // Never in Stretch; the window is the picture of the size shown.
        assert_eq!(own_place(4.0 / 3.0, (256, 192), Stretch, (1620, 1080), 8.0), None);
        let (_, pic) = own_place(160.0 / 152.0, (160, 152), Actual, (1620, 1080), 4.0).unwrap();
        assert_eq!(pic[3], 152.0 * 7.0);
        let (_, pic) = own_place(224.0 / 144.0, (224, 144), Aspect, (1440, 1080), 4.0).unwrap();
        assert_eq!(pic[1], 0.0, "picture at the top over a band");
    }

    #[test]
    fn integer_is_kept_where_it_is_the_original_ratio_picture() {
        let kpa = (960, 640);
        // The Lynx: 160x102 six times is both on the KPA.
        assert!(integer_is_ratio(Platform::Lynx, kpa));
        assert_eq!(VideoMode::Aspect.on(Platform::Lynx, kpa), VideoMode::Actual);
        assert_eq!(VideoMode::Actual.bigger_on(Platform::Lynx, kpa), VideoMode::Stretch);
        assert_eq!(VideoMode::Stretch.smaller_on(Platform::Lynx, kpa), VideoMode::Actual);
        // The WonderSwan keeps its three sizes there.
        assert!(!integer_is_ratio(Platform::Ws, kpa));
        assert_eq!(VideoMode::Actual.bigger_on(Platform::Ws, kpa), VideoMode::Aspect);
        // On a 1080p 16:9 screen the Lynx has three sizes again.
        assert!(!integer_is_ratio(Platform::Lynx, (1920, 1080)));
    }

    #[test]
    fn the_new_systems_offer_their_bezel_where_it_shows() {
        let kpa = (960, 640);
        for platform in [Platform::Sms, Platform::Gg, Platform::Ngp, Platform::Ws] {
            assert!(bezel_offered(platform, kpa), "{platform:?}");
        }
        assert!(!bezel_offered(Platform::Lynx, kpa));
        assert!(!bezel_offered(Platform::Pokemini, kpa));
        // KPA 960x640: 5x (480x320), then exactly x2.
        assert_eq!(pokemini_scale(kpa), 5);
        assert_eq!(pokemini_scale((1440, 960)), 5);
        assert_eq!(pokemini_scale((1152, 768)), 6);
        assert_eq!(pokemini_scale((1920, 1080)), 4);
        assert_eq!(pokemini_scale((640, 480)), 4);
        assert_eq!(pokemini_scale((720, 720)), 4);
        assert!(!bezel_offered(Platform::Ngp, (720, 720)));
        // A Color game gets the Color overlay with AUTO; both stay choosable.
        let home = (160.0 / 152.0, (160, 152));
        let fit = watomsk_fit(Bezel::NgpAuto, Platform::Ngp, VideoMode::Aspect, kpa, home);
        assert!(matches!(fit, Some(WatomskFit::Own(Place::Side, _))));
        use slot_ui::own_overlay::Design;
        assert_eq!(Bezel::NgpAuto.own(true), Some(Design::NeoGeoPocketColor));
        assert_eq!(Bezel::NgpAuto.own(false), Some(Design::NeoGeoPocket));
        assert_eq!(Bezel::WsAuto.own(true), Some(Design::WonderSwanColor));
        assert_eq!(Bezel::WsColor.own(false), Some(Design::WonderSwanColor));
        assert_eq!(watomsk_fit(Bezel::None, Platform::Ngp, VideoMode::Aspect, kpa, home), None);
    }

    #[test]
    fn thin_bars_start_without_the_overlay() {
        let thin = |platform, panel| own_room(platform, panel) < OWN_ROOM_ON;
        // 90 px bars on 1080: the Master System on 3:2, the Pokemon mini on 5:3.
        assert!(thin(Platform::Sms, (1620, 1080)));
        assert!(thin(Platform::Pokemini, (1800, 1080)));
        // Everything else keeps it: the WonderSwan's upright games, the Lynx on 16:9.
        assert!(!thin(Platform::Ws, (1620, 1080)));
        assert!(!thin(Platform::Lynx, (1920, 1080)));
        assert!(!thin(Platform::Ngp, (1620, 1080)));
        assert!(!thin(Platform::Sms, (1920, 1080)));
        assert!(!thin(Platform::Pokemini, (1440, 1080)));
    }
}

#[cfg(test)]
mod watomsk_preview {
    use super::*;

    /// SLOT_OVERLAY_PREVIEW=<dir> cargo test -p slot-android watomsk_preview -- --ignored
    #[test]
    #[ignore]
    fn write_watomsk_previews() {
        use slot_ui::fit_overlay::{fit_band, fit_overlay};
        let Ok(dir) = std::env::var("SLOT_OVERLAY_PREVIEW") else { return };
        let panels = [(960, 640), (1024, 768), (1920, 1080), (720, 720), (1240, 1080), (1280, 800)];
        let cases = [(Platform::Gb, Bezel::Gb), (Platform::Gbc, Bezel::Color),
            (Platform::Gba, Bezel::GbaClassic), (Platform::Nes, Bezel::Nes),
            (Platform::Snes, Bezel::SnesAlt), (Platform::Md, Bezel::GenesisClassic),
            (Platform::Md, Bezel::MegaDrive)];
        for panel in panels {
            for (platform, bezel) in cases {
                for mode in [VideoMode::Actual, VideoMode::Aspect] {
                    let home = (4.0 / 3.0, if platform == Platform::Md { (320, 224) } else { (256, 224) });
                    let Some(fit) = watomsk_fit(bezel, platform, mode, panel, home) else { continue };
                    let design = bezel.watomsk().unwrap();
                    let (rgba, pic) = match fit {
                        WatomskFit::Side(shape, window) => {
                            let side = slot_ui::watomsk::side(design, shape).unwrap();
                            (fit_overlay(&side.art(), panel, window), window)
                        }
                        WatomskFit::Band(pic) => {
                            let band = slot_ui::watomsk::band(design).unwrap();
                            (fit_band(&band.art(), panel, pic), pic)
                        }
                        WatomskFit::Own(..) => continue,
                    };
                    let name = format!("{dir}/{:?}-{:?}-{}-{}x{}-pic{}_{}_{}_{}.rgba", platform, bezel,
                        mode.as_str(), panel.0, panel.1,
                        pic[0].round(), pic[1].round(), pic[2].round(), pic[3].round());
                    std::fs::write(name, rgba).unwrap();
                }
            }
        }
    }
}
