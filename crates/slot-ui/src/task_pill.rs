//! The task pill (2.5.0-beta39, Siham 2026-11-19): what slot. is busy with,
//! by the battery or at the top right of a menu. A soft pill, a ring that
//! fills as the delete ring does, the task's words in slot.'s label type,
//! "+1" softer when another task runs too. Drawn in white: the engine inks
//! it as the line it sits on (dark over a light wallpaper, the menus'
//! colours). A problem is a red pill, shown a few seconds.
use crate::density::{self, Canvas};
use crate::plate::UndoFace;
use crate::text;

/// The pill's height, in slot.'s logical pixels.
pub const TASK_PILL_H: u32 = 20;
const RING: u32 = 12;
const LEFT: u32 = 5;
const GAP: u32 = 6;
const RIGHT: u32 = 9;
const SIZE: f32 = 12.0;
const WHITE: [u8; 3] = [255, 255, 255];
const PROBLEM: [u8; 3] = [214, 58, 58];

/// `progress` 0 to 1 fills the ring; None (a task without a count) leaves
/// it a faint circle. `more`: the other tasks running ("+1").
pub fn task_pill_face(words: &str, more: usize, progress: Option<f32>, problem: bool) -> UndoFace {
    let Some(font) = text::label_font() else { return Canvas::new(1, TASK_PILL_H).into_undo() };
    let measure = |t: &str| {
        let layout = text::fit(font, t, 600.0, 1, SIZE, SIZE);
        let w = layout.lines.first()
            .map_or(0.0, |line| text::line_width(font, line, layout.px, layout.tracking)).ceil() as u32 + 2;
        (layout, w)
    };
    let (main, main_w) = measure(words);
    let extra = (more > 0).then(|| measure(format!("+{more}").as_str()));
    let extra_w = extra.as_ref().map_or(0, |(_, w)| 6 + *w);
    let ring_w = if problem { 0 } else { RING + GAP };
    let w = LEFT + ring_w + main_w + extra_w + RIGHT;
    let mut c = Canvas::new(w, TASK_PILL_H);
    let s = density::scale();
    let (tw, th) = (c.tw, c.th);
    let r = th as f32 / 2.0;
    let (ground, ground_a) = if problem { (PROBLEM, 0.92) } else { (WHITE, 0.14) };
    for y in 0..th {
        for x in 0..tw {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let cx = fx.clamp(r, tw as f32 - r);
            let d = ((fx - cx).powi(2) + (fy - r).powi(2)).sqrt();
            over(&mut c.rgba, tw, x, y, ground, (r - d + 0.5).clamp(0.0, 1.0) * ground_a);
        }
    }
    if !problem {
        let (rx, ry) = ((LEFT as f32 + RING as f32 / 2.0) * s, r);
        let outer = RING as f32 * s / 2.0;
        let inner = outer - 2.2 * s;
        let done = progress.map_or(0.0, |p| p.clamp(0.0, 1.0)) * std::f32::consts::TAU;
        for y in 0..th {
            for x in 0..tw {
                let (dx, dy) = (x as f32 + 0.5 - rx, y as f32 + 0.5 - ry);
                let d = (dx * dx + dy * dy).sqrt();
                let band = ((outer - d + 0.5).clamp(0.0, 1.0)).min((d - inner + 0.5).clamp(0.0, 1.0));
                if band <= 0.0 { continue; }
                let angle = dx.atan2(-dy).rem_euclid(std::f32::consts::TAU);
                over(&mut c.rgba, tw, x, y, WHITE, band * if angle <= done { 1.0 } else { 0.3 });
            }
        }
    }
    let at = LEFT + ring_w;
    lay(&mut c.rgba, tw, th, &main, main_w, at, 1.0);
    if let Some((layout, lw)) = &extra {
        lay(&mut c.rgba, tw, th, layout, *lw, at + main_w + 6, 0.7);
    }
    c.into_undo()
}

/// Straight-alpha "over" of `colour` at `a`.
fn over(rgba: &mut [u8], tw: u32, x: u32, y: u32, colour: [u8; 3], a: f32) {
    if a <= 0.0 { return; }
    let o = ((y * tw + x) * 4) as usize;
    let a = (a.clamp(0.0, 1.0) * 255.0).round() as u32;
    let under = rgba[o + 3] as u32 * (255 - a) / 255;
    let out = a + under;
    if out == 0 { return; }
    for i in 0..3 {
        rgba[o + i] = ((colour[i] as u32 * a + rgba[o + i] as u32 * under + out / 2) / out) as u8;
    }
    rgba[o + 3] = out as u8;
}

/// Words in white, `lw` wide, at logical `at` across the pill.
fn lay(rgba: &mut [u8], tw: u32, th: u32, layout: &text::Layout, lw: u32, at: u32, alpha: f32) {
    let mut cell = Canvas::new(lw, TASK_PILL_H);
    cell.text(layout, WHITE);
    let ox = (at as f32 * density::scale()).round() as u32;
    for y in 0..cell.th.min(th) {
        for x in 0..cell.tw {
            if ox + x >= tw { break; }
            let a = cell.rgba[((y * cell.tw + x) * 4 + 3) as usize] as f32 / 255.0;
            over(rgba, tw, ox + x, y, WHITE, a * alpha);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pill_holds_its_words_and_grows_with_them() {
        let short = task_pill_face("LABELS 4/8", 0, Some(0.5), false);
        let long = task_pill_face("DOWNLOADING 3/15", 1, Some(0.2), false);
        assert_eq!(short.h, TASK_PILL_H);
        assert!(long.w > short.w);
        // A problem has no ring: narrower than the same words with one.
        let problem = task_pill_face("LABELS 4/8", 0, None, true);
        assert!(problem.w < short.w);
        // Something drawn, nothing outside the pill's height.
        assert!(short.rgba.chunks_exact(4).any(|p| p[3] > 0));
    }
}
