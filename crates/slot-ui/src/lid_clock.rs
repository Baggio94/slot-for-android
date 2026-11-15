//! The closed RG Rotate's clocks (Settings > Interface > RG Rotate > No
//! Game > Digital Clock / Analog Clock, Siham 2026-11-14).

use crate::plate::UndoFace;

/// A round dial `px` square: twelve hour marks (the quarters longer), the
/// hour and minute hands and a centre dot, in `ink` on transparent ground.
/// Drawn once a minute, antialiased from each shape's distance.
pub fn analog_clock_face(px: u32, hour: u32, minute: u32, ink: [u8; 3]) -> UndoFace {
    let px = px.max(16);
    let r = px as f32 / 2.0 - 1.5;
    let c = px as f32 / 2.0;
    // Each piece: a rounded stroke from a to b (dial units, 0 = centre,
    // 1 = rim, y down), half its width.
    let mut strokes: Vec<([f32; 2], [f32; 2], f32)> = Vec::with_capacity(15);
    let at = |turn: f32, d: f32| {
        let a = turn * std::f32::consts::TAU;
        [a.sin() * d, -a.cos() * d]
    };
    for mark in 0..12 {
        let turn = mark as f32 / 12.0;
        let (from, half) = if mark % 3 == 0 { (0.74, 0.028) } else { (0.82, 0.018) };
        strokes.push((at(turn, from), at(turn, 0.92), half));
    }
    let minutes = (minute % 60) as f32;
    let hours = (hour % 12) as f32 + minutes / 60.0;
    strokes.push((at(hours / 12.0 + 0.5, 0.10), at(hours / 12.0, 0.50), 0.034));
    strokes.push((at(minutes / 60.0 + 0.5, 0.10), at(minutes / 60.0, 0.78), 0.024));
    let ring = (0.985, 0.014);
    let dot = 0.055;
    let mut rgba = vec![0u8; (px * px * 4) as usize];
    for y in 0..px {
        for x in 0..px {
            // This pixel in dial units.
            let p = [(x as f32 + 0.5 - c) / r, (y as f32 + 0.5 - c) / r];
            let len = (p[0] * p[0] + p[1] * p[1]).sqrt();
            let mut d = ((len - ring.0).abs() - ring.1).min(len - dot);
            for &(a, b, half) in &strokes {
                d = d.min(segment_distance(p, a, b) - half);
            }
            // Distance in pixels, half a pixel of edge either way.
            let cover = (0.5 - d * r).clamp(0.0, 1.0);
            if cover <= 0.0 { continue; }
            let i = ((y * px + x) * 4) as usize;
            rgba[i..i + 3].copy_from_slice(&ink);
            rgba[i + 3] = (cover * 255.0).round() as u8;
        }
    }
    UndoFace { rgba, w: px, h: px }
}

fn segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [p[0] - a[0], p[1] - a[1]];
    let t = ((ap[0] * ab[0] + ap[1] * ab[1]) / (ab[0] * ab[0] + ab[1] * ab[1]).max(1e-6)).clamp(0.0, 1.0);
    let q = [a[0] + ab[0] * t - p[0], a[1] + ab[1] * t - p[1]];
    (q[0] * q[0] + q[1] * q[1]).sqrt()
}

/// "21:42" (or "9:42 PM") as hours and minutes, for the dial.
pub fn clock_hours_minutes(text: &str) -> Option<(u32, u32)> {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit() || *c == ':').collect();
    let (h, m) = digits.split_once(':')?;
    let (mut h, m): (u32, u32) = (h.parse().ok()?, m.get(..2).unwrap_or(m).parse().ok()?);
    let upper = text.to_ascii_uppercase();
    if upper.contains("PM") && h < 12 { h += 12; }
    if upper.contains("AM") && h == 12 { h = 0; }
    (h < 24 && m < 60).then_some((h, m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_both_clock_formats() {
        assert_eq!(clock_hours_minutes("21:42"), Some((21, 42)));
        assert_eq!(clock_hours_minutes("9:05 PM"), Some((21, 5)));
        assert_eq!(clock_hours_minutes("12:30 AM"), Some((0, 30)));
        assert_eq!(clock_hours_minutes(""), None);
    }

    #[test]
    fn the_dial_has_its_hands_and_a_clear_corner() {
        let face = analog_clock_face(200, 3, 0, [255, 255, 255]);
        let alpha = |x: u32, y: u32| face.rgba[((y * 200 + x) * 4 + 3) as usize];
        assert_eq!(alpha(1, 1), 0, "outside the rim");
        assert_eq!(alpha(100, 100), 255, "centre dot");
        // Three o'clock: the hour hand points right, the minute hand up.
        assert!(alpha(140, 100) > 200);
        assert!(alpha(100, 40) > 200);
        assert_eq!(alpha(100, 150), 0, "nothing at half past");
    }
}
