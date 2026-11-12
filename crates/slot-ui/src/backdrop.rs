use std::path::Path;
use std::sync::OnceLock;

use slot_gfx::{Draw, TexId, OUT_H, out_w};

use crate::art;

const SCRIM: f32 = 0.62;

// Directly use the bundled v1.7.0 wallpaper, not a recreated colour gradient.
// A user-selected PNG still takes priority via wallpaper_face().
const DEFAULT_PNG: &[u8] = include_bytes!("../../../assets/background_default.png");
const BLUR_PNG: &[u8] = include_bytes!("../../../assets/background_default_blur.png");

pub fn default_background() -> Option<&'static [u8]> {
    static CACHED: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    CACHED.get_or_init(|| art::cover_bytes(DEFAULT_PNG, out_w(), OUT_H)).as_deref()
}

pub fn blur_background() -> Option<&'static [u8]> {
    static CACHED: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    CACHED.get_or_init(|| art::cover_bytes(BLUR_PNG, out_w(), OUT_H)).as_deref()
}

/// Preserve native pixels from custom HD wallpapers: decode directly to
/// panel size (960x640 on the KONKR) instead of downsampling to 720x480 and scaling a second time.
pub fn wallpaper_face_native(path: &Path) -> Option<Vec<u8>> {
    art::cover(path, crate::native_w(), crate::native_h())
}
/// The upstream default is embedded verbatim and resampled once for GLES.
/// Its master is 3x, covered straight onto the panel's own pixels.
pub fn default_background_native() -> Option<&'static [u8]> {
    static CACHE: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    CACHE.get_or_init(|| art::cover_bytes(DEFAULT_PNG, crate::native_w(), crate::native_h()))
        .as_deref()
}
pub fn blur_background_native() -> Option<&'static [u8]> {
    static CACHE: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    CACHE.get_or_init(|| art::cover_bytes(BLUR_PNG, crate::native_w(), crate::native_h()))
        .as_deref()
}

/// How light a wallpaper is behind the status bar's top line (0 black to 1
/// white): the band the icons, clock and battery sit in (12 to 38 of the
/// 480 canvas rows), over the left and right 40% where they stand. The
/// line is inked black on a light wallpaper, white on a dark one.
pub fn top_line_luma(rgba: &[u8], w: u32) -> f32 {
    let w = w as usize;
    if w == 0 { return 0.0; }
    let h = rgba.len() / 4 / w;
    let (top, bottom) = (h * 12 / 480, (h * 38 / 480).max(h * 12 / 480 + 1).min(h));
    let (left, right) = (w * 2 / 5, w - w * 2 / 5);
    let (mut sum, mut n) = (0.0f64, 0u64);
    for y in top..bottom {
        for x in (0..left).chain(right..w).step_by(2) {
            let p = &rgba[(y * w + x) * 4..][..3];
            sum += 0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64;
            n += 1;
        }
    }
    if n == 0 { 0.0 } else { (sum / n as f64 / 255.0) as f32 }
}

/// Black ink above this lightness (after Wallpaper Tint), white below.
pub const LIGHT_WALLPAPER: f32 = 0.6;

pub fn wallpaper_face(path: &Path) -> Option<Vec<u8>> {
    art::cover(path, out_w(), OUT_H)
}

pub fn bezel_face(path: &Path, w: u32, h: u32) -> Option<Vec<u8>> {
    art::cover(path, w, h)
}

/// Faithful Slot v1.7 shelf ground: a plain wallpaper has no fixed
/// scrim. Wallpaper Tint only darkens a selected personal image, using the
/// exact alpha values from the official app's tint_alpha().
pub fn draw_ground_tinted(face: Option<TexId>, tint: f32, out: &mut Vec<Draw>) {
    let Some(tex) = face else { return };
    out.push(Draw::Tex {
        x: 0.0, y: 0.0,
        w: out_w() as f32, h: OUT_H as f32,
        tex, alpha: 1.0,
    });
    if tint > 0.0 {
        out.push(Draw::Rect {
            x: 0.0, y: 0.0,
            w: out_w() as f32, h: OUT_H as f32,
            colour: [0.0, 0.0, 0.0, tint.clamp(0.0, 1.0)],
        });
    }
}

pub fn draw_backdrop(face: Option<TexId>, out: &mut Vec<Draw>) {
    let Some(tex) = face else {
        return;
    };
    out.push(Draw::Tex {
        x: 0.0,
        y: 0.0,
        w: out_w() as f32,
        h: OUT_H as f32,
        tex,
        alpha: 1.0,
    });
    out.push(Draw::Rect {
        x: 0.0,
        y: 0.0,
        w: out_w() as f32,
        h: OUT_H as f32,
        colour: [0.0, 0.0, 0.0, SCRIM],
    });
}

#[cfg(test)]
mod top_line_tests {
    use super::*;

    #[test]
    fn the_top_line_reads_the_wallpaper_where_the_icons_stand() {
        let (w, h) = (96u32, 64u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        assert!(top_line_luma(&rgba, w) < 0.01);
        // White only in the middle fifth, where nothing stands: still dark.
        for y in 0..h as usize { for x in 40..56 { rgba[(y * w as usize + x) * 4..][..3].fill(255); } }
        assert!(top_line_luma(&rgba, w) < 0.01);
        // White everywhere at the top: light.
        for px in rgba.chunks_exact_mut(4).take((w * 8) as usize) { px[..3].fill(255); }
        assert!(top_line_luma(&rgba, w) > 0.99);
        assert!(top_line_luma(&rgba, w) > LIGHT_WALLPAPER);
    }
}
