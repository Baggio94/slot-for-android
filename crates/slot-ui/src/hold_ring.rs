//! The ring that fills while L2 and R2 are held together on the carousel to
//! delete a game (Siham 2026-11-10): red over grey, on a dark disc, about a
//! third smaller than the first preview (some 60 screen pixels on the KPA).
use crate::density::{self, Canvas};
use crate::plate::UndoFace;

/// The ring's side, in slot.'s logical pixels.
pub const HOLD_RING: u32 = 44;
const DISC: [u8; 4] = [0, 0, 0, 150];
const TRACK: [u8; 3] = [90, 90, 90];
const FILL: [u8; 3] = [226, 62, 62];

/// The ring `progress` (0 to 1) of the way round, clockwise from the top.
pub fn hold_ring_face(progress: f32) -> UndoFace {
    let mut face = Canvas::new(HOLD_RING, HOLD_RING);
    let (tw, th) = (face.tw, face.th);
    let s = density::scale();
    let half = HOLD_RING as f32 * s / 2.0;
    // Outer edge of the disc, then the ring's band, in texels.
    let disc = half - 0.5;
    let (outer, inner) = (half * 0.84, half * 0.66);
    let done = progress.clamp(0.0, 1.0) * std::f32::consts::TAU;
    for y in 0..th {
        for x in 0..tw {
            let (dx, dy) = (x as f32 + 0.5 - half, y as f32 + 0.5 - half);
            let r = (dx * dx + dy * dy).sqrt();
            let disc_a = (disc - r + 0.5).clamp(0.0, 1.0);
            if disc_a <= 0.0 { continue; }
            // Soft edges: a texel's share inside the band.
            let band = ((outer - r + 0.5).clamp(0.0, 1.0)).min((r - inner + 0.5).clamp(0.0, 1.0));
            // The angle from the top, clockwise.
            let angle = dx.atan2(-dy).rem_euclid(std::f32::consts::TAU);
            let colour = if angle <= done { FILL } else { TRACK };
            let base_a = DISC[3] as f32 / 255.0 * disc_a;
            let a = band + base_a * (1.0 - band);
            let mix = |c: u8| if a <= 0.0 { 0 } else {
                ((c as f32 * band + DISC[0] as f32 * base_a * (1.0 - band)) / a).round() as u8
            };
            let o = ((y * tw + x) * 4) as usize;
            face.rgba[o..o + 4].copy_from_slice(&[mix(colour[0]), mix(colour[1]), mix(colour[2]),
                (a * 255.0).round() as u8]);
        }
    }
    face.into_undo()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(face: &UndoFace, x: u32, y: u32) -> [u8; 4] {
        let o = ((y * face.w + x) * 4) as usize;
        face.rgba[o..o + 4].try_into().unwrap()
    }

    #[test]
    fn the_ring_fills_clockwise_from_the_top() {
        density::with_scale(1.0, || {
            let half = hold_ring_face(0.5);
            let mid = HOLD_RING / 2;
            let band = (HOLD_RING as f32 / 2.0 * 0.75) as u32;
            // Right of the centre: done (red); left: still grey.
            let right = pixel(&half, mid + band, mid);
            let left = pixel(&half, mid - band, mid);
            assert!(right[0] > 200 && right[1] < 100, "{right:?}");
            assert!(left[0] < 120 && left[0] == left[1], "{left:?}");
            // The centre is the dark disc, the corners empty.
            assert_eq!(pixel(&half, mid, mid)[0], 0);
            assert!(pixel(&half, mid, mid)[3] > 100);
            assert_eq!(pixel(&half, 0, 0)[3], 0);
            let full = hold_ring_face(1.0);
            assert!(pixel(&full, mid - band, mid)[0] > 200);
        });
    }
}
