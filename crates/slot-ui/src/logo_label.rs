//! Cart Studio's Logo Only label: a game's clear logo on slot.'s two-stop
//! diagonal ground, as the original Cart Studio composes it (label.rs and
//! hue.rs of slot-cart-studio): the same corners for a hue and a logo's
//! brightness, the same box for the logo, the same lean of the gradient.

/// How bright a logo has to be before the ground goes deep under it.
const BRIGHT: f32 = 110.0;
/// Yellow has no deep form: taken deep it turns toward amber.
const OLIVE_LO: f32 = 45.0;
const OLIVE_HI: f32 = 75.0;
const AMBER: f32 = 38.0;
/// The logo's box and where it sits, as fractions of the label.
const BOX_W_OF: f32 = 920.0 / 1280.0;
const BOX_H_OF: f32 = 423.0 / 640.0;
const TALL_TOP_OF: f32 = 70.0 / 640.0;
const MID_ROW_OF: f32 = 320.0 / 640.0;
/// Bounds ignore the faint fringe some rips carry around a logo.
const ALPHA_EDGE: u8 = 16;

pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r, g, b].map(|v| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

pub fn rgb_to_hsl(rgb: [u8; 3]) -> (f32, f32, f32) {
    let [r, g, b] = rgb.map(|v| v as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    (h, s, l)
}

/// The ground's two corners for a hue, given how bright the logo is.
pub fn stops(hue: f32, logo_luma: f32) -> ([u8; 3], [u8; 3]) {
    if logo_luma < BRIGHT {
        (hsl_to_rgb(hue, 0.55, 0.62), hsl_to_rgb(hue, 0.50, 0.84))
    } else {
        let h = if (OLIVE_LO..=OLIVE_HI).contains(&hue) { AMBER } else { hue };
        (hsl_to_rgb(h, 0.65, 0.24), hsl_to_rgb(h, 0.60, 0.46))
    }
}

/// The pale corner for a deep corner chosen by hand.
pub fn pale_for(deep: [u8; 3]) -> [u8; 3] {
    let (h, s, l) = rgb_to_hsl(deep);
    hsl_to_rgb(h, (s - 0.05).clamp(0.0, 1.0), (l + 0.22).clamp(0.0, 1.0))
}

/// Mean luminance of a logo's opaque pixels; 255 for none.
pub fn logo_luma(rgba: &[u8]) -> f32 {
    let (mut sum, mut n) = (0.0f32, 0usize);
    for p in rgba.chunks_exact(4) {
        if p[3] < 200 {
            continue;
        }
        sum += 0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32;
        n += 1;
    }
    if n == 0 { 255.0 } else { sum / n as f32 }
}

/// The dominant saturated hue of a picture (a cart's real label), or None
/// when it is greyscale: the ground follows the cart's own colours.
pub fn dominant_hue(rgba: &[u8]) -> Option<f32> {
    let mut bins = [0u32; 36];
    let mut counted = 0u32;
    for p in rgba.chunks_exact(4).step_by(3) {
        if p[3] < 200 {
            continue;
        }
        let (h, s, l) = rgb_to_hsl([p[0], p[1], p[2]]);
        if s < 0.35 || !(0.18..=0.85).contains(&l) {
            continue;
        }
        bins[(h / 10.0) as usize % 36] += 1;
        counted += 1;
    }
    let (best, n) = bins.iter().enumerate().max_by_key(|(_, n)| **n)?;
    (counted > 50 && *n > counted / 12).then_some(best as f32 * 10.0 + 5.0)
}

/// The visible part of a logo as (x0, y0, x1, y1), far edges exclusive.
fn trim(rgba: &[u8], w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    let mut b: Option<(u32, u32, u32, u32)> = None;
    for y in 0..h {
        for x in 0..w {
            if rgba[((y * w + x) * 4 + 3) as usize] < ALPHA_EDGE {
                continue;
            }
            b = Some(match b {
                None => (x, y, x + 1, y + 1),
                Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)),
            });
        }
    }
    b
}

/// The logo on the ground at `tw` x `th`, opaque RGBA.
pub fn compose(logo: &[u8], w: u32, h: u32, tw: u32, th: u32, deep: [u8; 3], pale: [u8; 3]) -> Vec<u8> {
    let mut out = Vec::with_capacity((tw * th * 4) as usize);
    for y in 0..th {
        for x in 0..tw {
            let t = 0.88 * x as f32 / tw as f32 + 0.12 * y as f32 / th as f32;
            for c in 0..3 {
                out.push((deep[c] as f32 + (pale[c] as f32 - deep[c] as f32) * t).round() as u8);
            }
            out.push(255);
        }
    }
    if logo.len() < (w * h * 4) as usize {
        return out;
    }
    let Some((x0, y0, x1, y1)) = trim(logo, w, h) else { return out };
    let (cw, ch) = (x1 - x0, y1 - y0);
    let mut part = Vec::with_capacity((cw * ch * 4) as usize);
    for y in y0..y1 {
        let row = ((y * w + x0) * 4) as usize;
        part.extend_from_slice(&logo[row..row + (cw * 4) as usize]);
    }
    let by_w = BOX_W_OF * tw as f32 / cw as f32;
    let by_h = BOX_H_OF * th as f32 / ch as f32;
    let scale = by_w.min(by_h);
    let (sw, sh) = (((cw as f32 * scale).round() as u32).max(1), ((ch as f32 * scale).round() as u32).max(1));
    let scaled = crate::native_assets::resample(&part, cw, ch, sw, sh);
    let left = ((tw as f32 - sw as f32) / 2.0).round() as i64;
    let top = if by_h <= by_w {
        (TALL_TOP_OF * th as f32).round() as i64
    } else {
        (MID_ROW_OF * th as f32 - sh as f32 / 2.0).round() as i64
    };
    for y in 0..sh as i64 {
        let ty = top + y;
        if ty < 0 || ty >= th as i64 {
            continue;
        }
        for x in 0..sw as i64 {
            let tx = left + x;
            if tx < 0 || tx >= tw as i64 {
                continue;
            }
            let s = ((y as u32 * sw + x as u32) * 4) as usize;
            let a = scaled[s + 3] as u32;
            if a == 0 {
                continue;
            }
            let d = ((ty as u32 * tw + tx as u32) * 4) as usize;
            for c in 0..3 {
                out[d + c] = ((scaled[s + c] as u32 * a + out[d + c] as u32 * (255 - a) + 127) / 255) as u8;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bright_logo_gets_a_deep_ground_and_a_dark_one_a_pale_ground() {
        let (deep, _) = stops(210.0, 200.0);
        let (pale, _) = stops(210.0, 40.0);
        assert!(rgb_to_hsl(deep).2 < 0.3 && rgb_to_hsl(pale).2 > 0.55);
        let (amber, _) = stops(60.0, 200.0);
        assert!((rgb_to_hsl(amber).0 - AMBER).abs() < 3.0, "yellow goes amber");
    }

    #[test]
    fn the_logo_sits_in_its_box_over_the_ground() {
        let (w, h) = (40u32, 10u32);
        let mut logo = vec![0u8; (w * h * 4) as usize];
        for y in 2..8 {
            for x in 5..35 {
                let i = ((y * w + x) * 4) as usize;
                logo[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        let out = compose(&logo, w, h, 320, 165, [10, 20, 30], [200, 210, 220]);
        assert_eq!(out.len(), 320 * 165 * 4);
        assert_eq!(&out[0..3], &[10, 20, 30], "the deep corner");
        let mid = ((82 * 320 + 160) * 4) as usize;
        assert_eq!(&out[mid..mid + 3], &[255, 255, 255], "the logo in the middle");
        assert!(out.chunks_exact(4).all(|p| p[3] == 255));
    }

    #[test]
    fn the_dominant_hue_of_a_red_label_is_red() {
        let px: Vec<u8> = (0..4000).flat_map(|_| [200u8, 30, 30, 255]).collect();
        let hue = dominant_hue(&px).unwrap();
        assert!(hue < 15.0 || hue > 345.0, "{hue}");
        let grey: Vec<u8> = (0..4000).flat_map(|_| [120u8, 120, 120, 255]).collect();
        assert_eq!(dominant_hue(&grey), None);
    }
}
