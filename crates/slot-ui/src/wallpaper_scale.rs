//! Wallpaper-only sampling. Reductions integrate the exact pixel coverage;
//! enlargements use Mitchell, with premultiplied alpha for imported PNGs.
//! Work is bounded to a few source rows rather than a full float RGBA image.

use std::collections::VecDeque;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Crop {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Crop {
    pub(crate) fn cover(sw: u32, sh: u32, w: u32, h: u32) -> Option<Self> {
        if sw == 0 || sh == 0 || w == 0 || h == 0 { return None; }
        let scale = (w as f64 / sw as f64).max(h as f64 / sh as f64);
        let (cw, ch) = ((w as f64 / scale).min(sw as f64), (h as f64 / scale).min(sh as f64));
        Some(Self { x: (sw as f64 - cw) * 0.5, y: (sh as f64 - ch) * 0.5, w: cw, h: ch })
    }
}

#[derive(Clone, Copy)]
struct Tap { at: usize, weight: f32 }

fn mitchell(x: f64) -> f64 {
    let x = x.abs();
    if x < 1.0 {
        (7.0 * x * x * x - 12.0 * x * x + 16.0 / 3.0) / 6.0
    } else if x < 2.0 {
        (-7.0 / 3.0 * x * x * x + 12.0 * x * x - 20.0 * x + 32.0 / 3.0) / 6.0
    } else { 0.0 }
}

fn taps(input: u32, output: u32, start: f64, extent: f64) -> Vec<Vec<Tap>> {
    let scale = extent / output as f64;
    (0..output).map(|i| {
        let mut row = Vec::new();
        if scale >= 1.0 {
            let a = start + i as f64 * scale;
            let b = start + (i + 1) as f64 * scale;
            for p in a.floor() as i64..b.ceil() as i64 {
                let weight = (b.min(p as f64 + 1.0) - a.max(p as f64)).max(0.0);
                if weight > 0.0 {
                    row.push(Tap { at: p.clamp(0, input as i64 - 1) as usize, weight: weight as f32 });
                }
            }
        } else {
            let centre = start + (i as f64 + 0.5) * scale - 0.5;
            for p in (centre - 2.0).ceil() as i64..=(centre + 2.0).floor() as i64 {
                let weight = mitchell(centre - p as f64);
                if weight != 0.0 {
                    row.push(Tap { at: p.clamp(0, input as i64 - 1) as usize, weight: weight as f32 });
                }
            }
        }
        let sum: f32 = row.iter().map(|t| t.weight).sum();
        for t in &mut row { t.weight /= sum; }
        row
    }).collect()
}

/// One channel for a mask, four for an RGBA photograph. `crop` may have
/// fractional edges: do not round it into a temporary, differently shaped PNG.
pub(crate) fn sample<const C: usize>(
    src: &[u8], sw: u32, sh: u32, crop: Crop, w: u32, h: u32,
) -> Option<Vec<u8>> {
    if !matches!(C, 1 | 4) || sw == 0 || sh == 0 || w == 0 || h == 0 { return None; }
    let count = (sw as usize).checked_mul(sh as usize)?.checked_mul(C)?;
    let output = (w as usize).checked_mul(h as usize)?.checked_mul(C)?;
    if src.len() != count || ![crop.x, crop.y, crop.w, crop.h].iter().all(|v| v.is_finite())
        || crop.w <= 0.0 || crop.h <= 0.0 || crop.x < 0.0 || crop.y < 0.0
        || crop.x + crop.w > sw as f64 + 1e-6 || crop.y + crop.h > sh as f64 + 1e-6 {
        return None;
    }
    let (xs, ys) = (taps(sw, w, crop.x, crop.w), taps(sh, h, crop.y, crop.h));
    let stride = w as usize * C;
    let mut rows: VecDeque<(usize, Vec<f32>)> = VecDeque::new();
    let mut out = vec![0; output];
    for (y, yt) in ys.iter().enumerate() {
        let first = yt.first()?.at;
        // Source rows advance monotonically, including clamped edge taps.
        while rows.front().is_some_and(|(at, _)| *at < first) { rows.pop_front(); }
        for t in yt {
            if rows.back().is_some_and(|(at, _)| *at >= t.at) { continue; }
            let mut row = vec![0.0; stride];
            for (x, xt) in xs.iter().enumerate() {
                for tx in xt {
                    let p = &src[(t.at * sw as usize + tx.at) * C..][..C];
                    for c in 0..C {
                        let value = if C == 4 && c < 3 { p[c] as f32 * p[3] as f32 / 255.0 } else { p[c] as f32 };
                        row[x * C + c] += tx.weight * value;
                    }
                }
            }
            rows.push_back((t.at, row));
        }
        let mut acc = vec![0.0; stride];
        for t in yt {
            let row = &rows.iter().find(|(at, _)| *at == t.at)?.1;
            for (v, p) in acc.iter_mut().zip(row) { *v += t.weight * p; }
        }
        for (dst, p) in out[y * stride..(y + 1) * stride].chunks_exact_mut(C).zip(acc.chunks_exact(C)) {
            if C == 4 {
                dst[3] = p[3].round().clamp(0.0, 255.0) as u8;
                if dst[3] > 0 && p[3] > 0.0 {
                    for c in 0..3 { dst[c] = (p[c] * 255.0 / p[3]).round().clamp(0.0, 255.0) as u8; }
                }
            } else { dst[0] = p[0].round().clamp(0.0, 255.0) as u8; }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractional_coverage_preserves_thin_lines() {
        // A 3-to-2 reduction: the middle pixel contributes only half to
        // either footprint, not as much as the outer source pixel.
        let crop = Crop { x: 0.0, y: 0.0, w: 3.0, h: 1.0 };
        assert_eq!(sample::<1>(&[0, 240, 0], 3, 1, crop, 2, 1).unwrap(), [80, 80]);
        // The same integration holds along the other axis.
        let crop = Crop { x: 0.0, y: 0.0, w: 1.0, h: 3.0 };
        assert_eq!(sample::<1>(&[0, 240, 0], 1, 3, crop, 1, 2).unwrap(), [80, 80]);
    }

    #[test]
    fn fractional_crop_is_not_rounded_or_stretched() {
        let crop = Crop::cover(4, 3, 2, 2).unwrap();
        assert_eq!((crop.x, crop.y, crop.w, crop.h), (0.5, 0.0, 3.0, 3.0));
        let src = [0, 120, 240, 60].repeat(3);
        assert_eq!(sample::<1>(&src, 4, 3, crop, 2, 2).unwrap(), [80, 180, 80, 180]);
    }

    #[test]
    fn opaque_and_transparent_pixels_do_not_make_a_dark_fringe() {
        let src = [255, 0, 0, 255, 0, 0, 0, 0];
        let crop = Crop { x: 0.0, y: 0.0, w: 2.0, h: 1.0 };
        assert_eq!(sample::<4>(&src, 2, 1, crop, 1, 1).unwrap(), [255, 0, 0, 128]);
        for p in sample::<4>(&src, 2, 1, crop, 8, 4).unwrap().chunks_exact(4) {
            if p[3] > 0 { assert_eq!(&p[..3], &[255, 0, 0]); }
        }
    }

    #[test]
    fn flat_backgrounds_stay_flat_at_all_panel_shapes() {
        for (w, h) in [(1, 1), (7, 7), (9, 8), (16, 9), (27, 18), (9, 16), (40, 17)] {
            let crop = Crop::cover(17, 17, w, h).unwrap();
            assert!(sample::<1>(&vec![81; 17 * 17], 17, 17, crop, w, h).unwrap().iter().all(|v| *v == 81));
            let rgba = [49, 78, 128, 201].repeat(17 * 17);
            assert!(sample::<4>(&rgba, 17, 17, crop, w, h).unwrap().chunks_exact(4).all(|p| p == [49, 78, 128, 201]));
        }
    }

    #[test]
    fn identity_and_invalid_inputs() {
        let src: Vec<u8> = (0..35).map(|i| (i * 7) as u8).collect();
        let crop = Crop::cover(7, 5, 7, 5).unwrap();
        assert_eq!(sample::<1>(&src, 7, 5, crop, 7, 5).unwrap(), src);
        assert!(Crop::cover(7, 5, 0, 5).is_none());
        assert!(sample::<1>(&[], 7, 5, crop, 7, 5).is_none());
        assert!(sample::<4>(&src, 7, 5, crop, 7, 5).is_none());
        assert!(sample::<1>(&src, 7, 5, Crop { x: f64::NAN, ..crop }, 7, 5).is_none());
        assert!(sample::<1>(&src, 7, 5, Crop { x: 1.0, ..crop }, 7, 5).is_none());
    }
}
