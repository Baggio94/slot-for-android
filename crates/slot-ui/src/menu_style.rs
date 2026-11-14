//! The menus' look: a ground a shade under the housing, a rounded bar under
//! the selected row, and a small path line over each page's title.
use crate::density::Canvas;
use crate::plate::UndoFace;
use crate::power_menu::MENU_INK;
use crate::slot_chrome::theme;
use crate::text;
use std::sync::atomic::{AtomicBool, Ordering};

// OLED changes page grounds, never cartridge shells or console hardware.
// The theme is set on the thread that draws and read there first, then by
// any other (Android's pages): two engines drawing at once in the tests
// keep their own.
static OLED: AtomicBool = AtomicBool::new(false);
thread_local! {
    static LOCAL_OLED: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    static LOCAL_PALETTE: std::cell::Cell<Option<Option<MenuPalette>>> = const { std::cell::Cell::new(None) };
}
pub fn set_oled(enabled: bool) {
    OLED.store(enabled, Ordering::Relaxed);
    LOCAL_OLED.with(|local| local.set(Some(enabled)));
}
pub fn oled() -> bool {
    LOCAL_OLED.with(|local| local.get()).unwrap_or_else(|| OLED.load(Ordering::Relaxed))
}

fn ground_rgb(housing: [u8; 3], oled: bool) -> [u8; 3] {
    if oled { [0; 3] } else { housing.map(|c| (c as f32 * 0.88).round() as u8) }
}

/// The Light and Original menus' colours; None is Dark's (and OLED's), from
/// the theme. `ink` is set where the text turns dark on a light ground.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct MenuPalette {
    pub ground: [u8; 3],
    pub pill: [u8; 3],
    pub ink: Option<[u8; 3]>,
    /// The shelf's ground under the Theme wallpaper: a colour that goes
    /// with the bar and keeps the carts clear of it (Siham 2026-11-14).
    pub shelf: [u8; 3],
}

const DARK_INK: [u8; 3] = [28, 28, 32];

impl MenuPalette {
    /// Light: light grey pages, a white bar, dark text.
    pub const LIGHT: Self = Self { ground: [234, 233, 236], pill: [255, 255, 255], ink: Some(DARK_INK),
        shelf: [238, 233, 222] };

    /// Original: the plastic of the console whose bar is on screen
    /// (`family` as bitmaps::home_slot): black text on the light plastics,
    /// white on the GBA's indigo and the Mega Drive's black, as the clock on
    /// their bars (bitmaps::console_slot_takes_dark_ink).
    pub fn original(family: usize) -> Self {
        // The shelf, light so every cart stands out (Siham 2026-11-14): a
        // pale rose for the Famicom and the NES, a light blue grey for the
        // Super Nintendo, a light grey the black Mega Drive carts stand out
        // on, the Game Boy's screen green, a pale cream under the Color's
        // yellow, lavender over the GBA's indigo.
        let (ground, pill, shelf) = match family {
            0 => ([226, 216, 200], [244, 237, 224], [239, 220, 217]), // Famicom
            1 => ([188, 188, 188], [214, 214, 214], [228, 205, 203]),  // NES
            2 => ([196, 190, 198], [222, 216, 226], [205, 211, 222]), // Super Nintendo
            3 => ([34, 34, 36], [68, 68, 72], [185, 187, 192]),        // Mega Drive
            4 => ([178, 178, 170], [206, 206, 198], [202, 211, 168]),  // Game Boy
            5 => ([236, 188, 20], [252, 224, 128], [243, 227, 160]), // Game Boy Color
            _ => ([74, 80, 152], [108, 114, 184], [196, 198, 228]), // Game Boy Advance
        };
        let ink = crate::bitmaps::console_slot_takes_dark_ink(family, 0).then_some(DARK_INK);
        Self { ground, pill, ink, shelf }
    }
}

static PALETTE: std::sync::RwLock<Option<MenuPalette>> = std::sync::RwLock::new(None);
pub fn set_palette(palette: Option<MenuPalette>) {
    *PALETTE.write().unwrap_or_else(|e| e.into_inner()) = palette;
    LOCAL_PALETTE.with(|local| local.set(Some(palette)));
}
pub fn palette() -> Option<MenuPalette> {
    LOCAL_PALETTE.with(|local| local.get())
        .unwrap_or_else(|| *PALETTE.read().unwrap_or_else(|e| e.into_inner()))
}

/// The redrawing of the menus' text for a dark ink: Draw::Ink's colours.
pub fn ink_map() -> Option<slot_gfx::InkMap> {
    let palette = palette()?;
    let ink = palette.ink?;
    let f = |c: [u8; 3]| c.map(|v| v as f32 / 255.0);
    Some(slot_gfx::InkMap { paper: f(palette.ground), ink: f(ink) })
}

/// The menus' text colour (Android's pages take it too).
pub fn ink_rgb() -> [u8; 3] {
    palette().and_then(|p| p.ink).unwrap_or(MENU_INK)
}

/// A chosen cell's or key's ground on the special pages: the theme's edge,
/// or the palette's bar in Light and Original.
pub fn highlight() -> [f32; 4] {
    match palette() {
        Some(p) => [p.pill[0] as f32 / 255.0, p.pill[1] as f32 / 255.0, p.pill[2] as f32 / 255.0, 1.0],
        None => crate::slot_chrome::edge(),
    }
}

/// A veil over a page's grid for a question or details: black, or the
/// light ground itself where the text is dark.
pub fn scrim(alpha: f32) -> [f32; 4] {
    match palette().filter(|p| p.ink.is_some()) {
        Some(p) => [p.ground[0] as f32 / 255.0, p.ground[1] as f32 / 255.0, p.ground[2] as f32 / 255.0,
                    (alpha * 1.15).min(0.94)],
        None => [0.0, 0.0, 0.0, alpha],
    }
}

/// Dark's ground whatever the theme, for pages with their own dark art
/// (Multiplayer): black in OLED.
pub fn dark_ground() -> [f32; 4] {
    let c = ground_rgb(theme().housing, oled());
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0]
}

/// Dark's shelf ground (OLED's is black).
pub const DARK_SHELF: [f32; 4] = [0.085, 0.085, 0.093, 1.0];

/// The shelf's ground under the Theme wallpaper: Dark's grey, OLED's
/// black, or the Light / Original palette's shelf colour.
pub fn shelf_ground() -> [f32; 4] {
    match palette() {
        Some(p) => [p.shelf[0] as f32 / 255.0, p.shelf[1] as f32 / 255.0, p.shelf[2] as f32 / 255.0, 1.0],
        None => page_ground(DARK_SHELF),
    }
}

/// Whether the Theme wallpaper's ground wants dark words over it (the
/// shelf's name, the game's title, the clock).
pub fn shelf_takes_dark_ink() -> bool {
    let [r, g, b, _] = shelf_ground();
    0.299 * r + 0.587 * g + 0.114 * b > 0.5
}

/// Keep a specialised page's layout while making its plain ground OLED black.
pub fn page_ground(original: [f32; 4]) -> [f32; 4] {
    if oled() { [0.0, 0.0, 0.0, original[3]] } else { original }
}

/// The selected row's bar: its height and corner radius.
pub const PILL_H: u32 = 44;
pub const PILL_R: u32 = 11;
/// The bar's left and right edges, from the screen's.
pub const PILL_LEFT: f32 = 28.0;
pub const PILL_RIGHT: f32 = 22.0;

const PATH_PX: f32 = 13.0;
const PATH_TRACKING: f32 = 2.0;
pub const PATH_H: u32 = 18;

/// The menus' ground: the housing, a shade darker.
pub fn menu_ground_rgb() -> [u8; 3] {
    palette().map_or_else(|| ground_rgb(theme().housing, oled()), |p| p.ground)
}

pub fn menu_ground() -> [f32; 4] {
    let c = menu_ground_rgb();
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0]
}

/// The bar's colour: the theme's edge, settled into the ground.
pub fn pill_rgb() -> [u8; 3] {
    if let Some(p) = palette() { return p.pill; }
    // OLED retains Dark's selection colour.
    let (ground, edge) = (ground_rgb(theme().housing, false), theme().edge);
    [0, 1, 2].map(|i| (ground[i] as f32 + (edge[i] as f32 - ground[i] as f32) * 0.62).round() as u8)
}

/// The bar in three faces: its left end, a strip to stretch between, its
/// right end. One colour throughout, so the joins can't show.
pub fn pill_faces() -> [UndoFace; 3] {
    let colour = pill_rgb();
    let end = |right: bool| {
        let mut face = Canvas::new(PILL_R, PILL_H);
        let (tw, th) = (face.tw as f32, face.th as f32);
        let r = tw;
        for y in 0..face.th {
            for x in 0..face.tw {
                // The corner's centre is on the inner side of the end.
                let px = if right { x as f32 + 0.5 } else { tw - (x as f32 + 0.5) };
                let py = y as f32 + 0.5;
                let dy = if py < r { r - py } else if py > th - r { py - (th - r) } else { 0.0 };
                let cover = if dy > 0.0 {
                    (r - (px * px + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0)
                } else { 1.0 };
                let at = ((y * face.tw + x) * 4) as usize;
                face.rgba[at..at + 3].copy_from_slice(&colour);
                face.rgba[at + 3] = (cover * 255.0).round() as u8;
            }
        }
        face.into_undo()
    };
    let mut middle = Canvas::new(4, PILL_H);
    for p in middle.rgba.chunks_exact_mut(4) {
        p[..3].copy_from_slice(&colour);
        p[3] = 255;
    }
    [end(false), middle.into_undo(), end(true)]
}

/// A page's path line ("SETTINGS > SYSTEMS"): small, widely spaced capitals.
pub fn menu_path_face(label: &str) -> UndoFace {
    let Some(font) = text::label_font() else {
        return UndoFace { rgba: Vec::new(), w: 0, h: 0 };
    };
    let upper = label.to_uppercase();
    let ink = text::line_width(font, &upper, PATH_PX, PATH_TRACKING).ceil() as u32;
    let mut face = Canvas::new(ink + 4, PATH_H);
    let layout = text::Layout { lines: vec![upper], px: PATH_PX, tracking: PATH_TRACKING };
    face.text(&layout, MENU_INK);
    face.into_undo()
}

/// The mark on a row that opens a page.
pub fn menu_opens_face() -> UndoFace {
    let Some(font) = text::label_font() else {
        return UndoFace { rgba: Vec::new(), w: 0, h: 0 };
    };
    let px = 26.0;
    let w = text::line_width(font, "\u{203a}", px, 0.0).ceil() as u32 + 4;
    let mut face = Canvas::new(w, crate::power_menu::MENU_H);
    let layout = text::Layout { lines: vec!["\u{203a}".to_owned()], px, tracking: 0.0 };
    face.text(&layout, MENU_INK);
    face.into_undo()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::density::Face as _;

    #[test]
    fn original_menus_write_as_the_clock_on_their_console_bar() {
        for family in 0..7 {
            let dark = crate::bitmaps::console_slot_takes_dark_ink(family, 0);
            assert_eq!(MenuPalette::original(family).ink.is_some(), dark, "family {family}");
        }
        // The GBA's indigo and the Mega Drive's black keep white text.
        assert!(MenuPalette::original(6).ink.is_none());
        assert!(MenuPalette::original(3).ink.is_none());
        assert!(MenuPalette::LIGHT.ink.is_some());
    }

    #[test]
    fn oled_grounds_are_pure_black_even_with_a_custom_palette() {
        for housing in [[36, 36, 41], [255, 255, 255], [120, 40, 80]] {
            assert_eq!(ground_rgb(housing, true), [0; 3]);
            assert_ne!(ground_rgb(housing, false), [0; 3]);
        }
    }

    #[test]
    fn the_bar_is_round_at_its_ends_and_whole_between() {
        let [left, middle, right] = pill_faces();
        let (w, h) = left.texels();
        let alpha = |f: &UndoFace, x: u32, y: u32| f.rgba[((y * w + x) * 4 + 3) as usize];
        assert_eq!(alpha(&left, 0, 0), 0, "outside the corner");
        assert!(alpha(&left, w - 1, 0) >= 250, "the top edge meets the strip");
        assert_eq!(alpha(&left, 0, h / 2), 255, "the side is straight");
        assert_eq!(alpha(&right, w - 1, 0), 0);
        assert!(alpha(&right, 0, h - 1) >= 250);
        assert!(middle.rgba.chunks_exact(4).all(|p| p[3] == 255));
        assert_eq!(&left.rgba[((h / 2 * w) * 4) as usize..][..3], &middle.rgba[..3]);
    }

    #[test]
    fn the_ground_sits_under_the_housing_and_the_bar_over_it() {
        let luma = |c: [u8; 3]| c.iter().map(|&v| v as u32).sum::<u32>();
        assert!(luma(menu_ground_rgb()) < luma(theme().housing));
        assert!(luma(pill_rgb()) > luma(menu_ground_rgb()));
    }

    #[test]
    fn a_path_line_and_the_mark_have_ink() {
        for face in [menu_path_face("Settings \u{203a} Systems"), menu_opens_face()] {
            assert!(face.w > 0 && face.rgba.chunks_exact(4).any(|p| p[3] > 0));
        }
    }
}
