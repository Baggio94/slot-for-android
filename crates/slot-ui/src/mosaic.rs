//! slot.'s Default wallpapers (2.5): a mosaic for each shelf, drawn by Siham
//! for the four themes. A theme only changes two colours, the ground and
//! the line, so each shelf ships one 2048x2048 4-bit mask (how much of the
//! line a pixel is) and the colours below; the panel's own size is cut from
//! its centre, like any wallpaper. Default Blur is the same image softened
//! about as much as the bundled wood's blur.

use std::sync::Mutex;

use crate::art;

/// The shelves' keys, in wallpaper::SHELF_KEYS's order.
pub const MOSAIC_KEYS: [&str; 9] =
    ["GBA", "GB", "GBC", "NES", "SNES", "MD", "Recents", "Favorites", "GBAV"];

const MASKS: [&[u8]; 9] = [
    include_bytes!("../../../assets/wallpapers/GBA.png"),
    include_bytes!("../../../assets/wallpapers/GB.png"),
    include_bytes!("../../../assets/wallpapers/GBC.png"),
    include_bytes!("../../../assets/wallpapers/NES.png"),
    include_bytes!("../../../assets/wallpapers/SNES.png"),
    include_bytes!("../../../assets/wallpapers/MD.png"),
    include_bytes!("../../../assets/wallpapers/Recents.png"),
    include_bytes!("../../../assets/wallpapers/Favorites.png"),
    include_bytes!("../../../assets/wallpapers/GBAV.png"),
];

/// The four themes a mosaic is coloured for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MosaicTheme { Original, Light, Dark, Oled }

/// Ground, then line, for Original, Light, Dark and OLED, read off Siham's
/// PNGs (each is exactly ground + mask x (line - ground)).
const COLOURS: [[[[u8; 3]; 2]; 4]; 9] = [
    // GBA
    [[[196, 198, 228], [145, 147, 182]], [[238, 233, 222], [181, 172, 158]], [[22, 22, 24], [67, 67, 73]], [[0, 0, 0], [50, 50, 56]]],
    // GB
    [[[202, 211, 168], [149, 163, 112]], [[238, 233, 222], [180, 171, 157]], [[22, 22, 24], [67, 67, 73]], [[0, 0, 0], [50, 50, 57]]],
    // GBC
    [[[243, 227, 160], [191, 167, 99]], [[238, 233, 222], [181, 172, 158]], [[22, 22, 24], [66, 66, 73]], [[0, 0, 0], [50, 50, 56]]],
    // NES
    [[[228, 205, 203], [177, 152, 150]], [[238, 233, 222], [181, 172, 158]], [[22, 22, 24], [66, 66, 73]], [[0, 0, 0], [50, 50, 56]]],
    // SNES
    [[[205, 211, 222], [152, 161, 178]], [[238, 233, 222], [181, 172, 158]], [[22, 22, 24], [66, 66, 73]], [[0, 0, 0], [50, 50, 56]]],
    // MD
    [[[88, 90, 96], [134, 140, 152]], [[238, 233, 222], [181, 172, 158]], [[22, 22, 24], [66, 66, 73]], [[0, 0, 0], [50, 50, 56]]],
    // Recents
    [[[211, 216, 223], [155, 164, 177]], [[238, 233, 222], [183, 174, 160]], [[22, 22, 24], [65, 65, 71]], [[0, 0, 0], [48, 48, 54]]],
    // Favorites
    [[[229, 215, 186], [180, 161, 125]], [[238, 233, 222], [183, 174, 160]], [[22, 22, 24], [65, 65, 71]], [[0, 0, 0], [48, 48, 54]]],
    // GBAV
    [[[216, 206, 231], [163, 151, 181]], [[238, 233, 222], [183, 174, 160]], [[22, 22, 24], [65, 65, 71]], [[0, 0, 0], [48, 48, 54]]],
];

/// How soft Default Blur is: the blur's spread as a share of the width.
/// Softer than the wood's, so the drawings still read (Siham 2026-11-18).
const BLUR_SPREAD: f32 = 0.0025;

/// Masks already cut to a size: a theme change only recolours them.
static CUT: Mutex<Vec<(usize, u32, u32, Vec<u8>)>> = Mutex::new(Vec::new());

/// The mask of shelf `shelf`, cut from its centre to w x h, one byte a pixel.
fn mask(shelf: usize, w: u32, h: u32) -> Option<Vec<u8>> {
    if let Some((_, _, _, cut)) = CUT.lock().unwrap_or_else(|e| e.into_inner()).iter()
        .find(|(s, cw, ch, _)| *s == shelf && *cw == w && *ch == h) {
        return Some(cut.clone());
    }
    let (rgba, sw, sh) = art::decode_png_bytes(MASKS.get(shelf)?)?;
    let cut: Vec<u8> = art::cover_rgba(&rgba, sw, sh, w, h)?.chunks_exact(4).map(|p| p[0]).collect();
    let mut cache = CUT.lock().unwrap_or_else(|e| e.into_inner());
    // One size at a time (the RG Rotate turns): a new size drops the others.
    cache.retain(|(_, cw, ch, _)| *cw == w && *ch == h);
    cache.push((shelf, w, h, cut.clone()));
    Some(cut)
}

/// Shelf `shelf`'s ground colour in `theme`: the words over it turn dark
/// when it is light.
pub fn mosaic_ground(shelf: usize, theme: MosaicTheme) -> Option<[u8; 3]> {
    COLOURS.get(shelf).map(|c| c[theme as usize][0])
}

/// How a mosaic is drawn: Default, Default Blur or Default Halo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MosaicLook { Sharp, Blur, Halo }

/// Default Halo: the lines kept sharp but faint, in a wide soft glow
/// (Siham 2026-11-18). The glow's spread as a share of the width, and how
/// much of the line and of the glow is drawn.
const HALO_SPREAD: f32 = 0.0052;
const HALO_LINE: f32 = 0.45;
const HALO_GLOW: f32 = 0.9;

/// Shelf `shelf`'s mosaic in `theme` at w x h, RGBA, as `look` draws it.
pub fn mosaic(shelf: usize, theme: MosaicTheme, look: MosaicLook, w: u32, h: u32) -> Option<Vec<u8>> {
    let mut mask = mask(shelf, w, h)?;
    let [ground, line] = COLOURS.get(shelf)?[theme as usize];
    if look == MosaicLook::Halo {
        let mut glow: Vec<u8> = mask.iter().flat_map(|m| [*m, *m, *m, 255]).collect();
        soften(&mut glow, w as usize, h as usize, (w as f32 * HALO_SPREAD).round().max(1.0) as usize);
        for (m, g) in mask.iter_mut().zip(glow.chunks_exact(4)) {
            let v = HALO_LINE * *m as f32 + HALO_GLOW * g[0] as f32;
            *m = v.round().min(255.0) as u8;
        }
    }
    let mut rgba = vec![255u8; mask.len() * 4];
    for (px, m) in rgba.chunks_exact_mut(4).zip(&mask) {
        let m = *m as f32 / 255.0;
        for c in 0..3 {
            px[c] = (ground[c] as f32 + m * (line[c] as f32 - ground[c] as f32)).round() as u8;
        }
    }
    if look == MosaicLook::Blur {
        soften(&mut rgba, w as usize, h as usize, (w as f32 * BLUR_SPREAD).round().max(1.0) as usize);
    }
    Some(rgba)
}

/// At the panel's own size, for the carousel.
pub fn mosaic_native(shelf: usize, theme: MosaicTheme, look: MosaicLook) -> Option<Vec<u8>> {
    mosaic(shelf, theme, look, crate::native_w(), crate::native_h())
}

/// Three box blurs of radius `r` each way: close to a Gaussian.
fn soften(rgba: &mut [u8], w: usize, h: usize, r: usize) {
    if w == 0 || h == 0 { return; }
    let mut line = Vec::new();
    for _ in 0..3 {
        for y in 0..h {
            box_pass(rgba, y * w * 4, 4, w, r, &mut line);
        }
        for x in 0..w {
            box_pass(rgba, x * 4, w * 4, h, r, &mut line);
        }
    }
}

/// One box blur along `n` pixels from `start`, `step` bytes apart, edges held.
fn box_pass(rgba: &mut [u8], start: usize, step: usize, n: usize, r: usize, line: &mut Vec<[u32; 3]>) {
    line.clear();
    line.extend((0..n).map(|i| {
        let p = &rgba[start + i * step..][..3];
        [p[0] as u32, p[1] as u32, p[2] as u32]
    }));
    let at = |i: isize| line[i.clamp(0, n as isize - 1) as usize];
    let width = (2 * r + 1) as u32;
    let mut sum = [0u32; 3];
    for i in -(r as isize)..=(r as isize) {
        let p = at(i);
        for c in 0..3 { sum[c] += p[c]; }
    }
    for i in 0..n {
        let p = &mut rgba[start + i * step..][..3];
        for c in 0..3 { p[c] = ((sum[c] + width / 2) / width) as u8; }
        let (add, gone) = (at(i as isize + r as isize + 1), at(i as isize - r as isize));
        for c in 0..3 { sum[c] = sum[c] + add[c] - gone[c]; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shelf_has_its_mosaic_in_every_theme() {
        for shelf in 0..MOSAIC_KEYS.len() {
            for theme in [MosaicTheme::Original, MosaicTheme::Light, MosaicTheme::Dark, MosaicTheme::Oled] {
                let image = mosaic(shelf, theme, MosaicLook::Sharp, 480, 320).expect("mask decodes");
                assert_eq!(image.len(), 480 * 320 * 4);
                let [ground, line] = COLOURS[shelf][theme as usize];
                // Mostly ground, with some line drawn over it.
                let near = |p: &[u8], c: [u8; 3]| p.iter().zip(c).all(|(a, b)| a.abs_diff(b) <= 2);
                let grounds = image.chunks_exact(4).filter(|p| near(p, ground)).count();
                assert!(grounds > 480 * 320 / 2, "{} {theme:?}", MOSAIC_KEYS[shelf]);
                assert!(image.chunks_exact(4).any(|p| !near(p, ground)), "{}", MOSAIC_KEYS[shelf]);
                assert_ne!(ground, line);
            }
        }
    }

    #[test]
    fn the_blur_softens_and_keeps_the_colours() {
        let sharp = mosaic(3, MosaicTheme::Light, MosaicLook::Sharp, 480, 320).unwrap();
        let soft = mosaic(3, MosaicTheme::Light, MosaicLook::Blur, 480, 320).unwrap();
        let edges = |img: &[u8]| img.chunks_exact(4).collect::<Vec<_>>().windows(2)
            .map(|p| p[0][1].abs_diff(p[1][1]) as u64).sum::<u64>();
        assert!(edges(&soft) * 3 < edges(&sharp) * 2);
        let mean = |img: &[u8]| img.chunks_exact(4).map(|p| p[1] as u64).sum::<u64>() / (480 * 320);
        assert!(mean(&soft).abs_diff(mean(&sharp)) <= 2);
    }

    #[test]
    fn the_halo_keeps_the_lines_and_glows_around_them() {
        let sharp = mosaic(3, MosaicTheme::Dark, MosaicLook::Sharp, 480, 320).unwrap();
        let halo = mosaic(3, MosaicTheme::Dark, MosaicLook::Halo, 480, 320).unwrap();
        let ground = COLOURS[3][MosaicTheme::Dark as usize][0];
        let lit = |img: &[u8]| img.chunks_exact(4).filter(|p| p[1].abs_diff(ground[1]) > 2).count();
        // The glow reaches past the lines...
        assert!(lit(&halo) > lit(&sharp) * 3 / 2);
        // ...and the lines are still there: brightest where the sharp one is.
        let peak = |img: &[u8]| img.chunks_exact(4).map(|p| p[1]).max().unwrap();
        assert!(peak(&halo) >= ground[1] + (peak(&sharp) - ground[1]) / 2);
    }

    #[test]
    fn any_screen_shape_is_filled() {
        for (w, h) in [(720, 720), (960, 640), (1024, 768), (1920, 1080), (1080, 1920), (2560, 1600)] {
            let image = mosaic(0, MosaicTheme::Dark, MosaicLook::Sharp, w, h).unwrap();
            assert_eq!(image.len(), (w * h * 4) as usize);
        }
    }
}
