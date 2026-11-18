use slot_gfx::{Draw, TexId};
use slot_power::{Battery, Charge};

use crate::footer::Printed;
use crate::hud::HUD_INK;
use crate::plate::HINT_H;

pub const BOLT_PX: f32 = 18.0;

pub const GAUGE_W: f32 = 22.0;
pub const GAUGE_H: f32 = 11.0;
const _: () = assert!(GAUGE_H < GAUGE_W);
const NUB_W: f32 = 2.5;
const NUB_H: f32 = 4.0;
pub const WALL: f32 = 1.5;
const GAP: f32 = 7.0;

const BOLT_W: f32 = 14.0;
const BOLT_H: f32 = 14.0;
const BOLT_GAP: f32 = 5.0;
/// The bolt to its percent: with it, the ink gaps gauge-bolt and
/// bolt-percent are the same (10 px each on the RG Rotate, Siham 2026-11-14).
const BOLT_TEXT_GAP: f32 = 3.0;
const _: () = assert!(BOLT_GAP > 0.0);

const INK: [f32; 4] = [
    HUD_INK[0] as f32 / 255.0,
    HUD_INK[1] as f32 / 255.0,
    HUD_INK[2] as f32 / 255.0,
    1.0,
];

pub fn draw_gauge(
    x:f32,y:f32,battery:Option<Battery>,percent:Printed,
    bolt:Option<TexId>,out:&mut Vec<Draw>
) {
    draw_gauge_with_ink(x,y,battery,percent,bolt,INK,out)
}

pub fn draw_gauge_with_ink(
    x:f32,y:f32,battery:Option<Battery>,percent:Printed,
    bolt:Option<TexId>,ink:[f32;4],out:&mut Vec<Draw>
) {
    let Some(b) = battery else {
        return;
    };

    let rect = |x: f32, y: f32, w: f32, h: f32, out: &mut Vec<Draw>| {
        out.push(Draw::Rect {
            x,
            y,
            w,
            h,
            colour: ink,
        });
    };

    let cx = x + BOLT_W + BOLT_GAP;

    rect(cx, y, GAUGE_W, WALL, out);
    rect(cx, y + GAUGE_H - WALL, GAUGE_W, WALL, out);
    rect(cx, y, WALL, GAUGE_H, out);
    rect(cx + GAUGE_W - WALL, y, WALL, GAUGE_H, out);
    rect(cx + GAUGE_W, y + (GAUGE_H - NUB_H) / 2.0, NUB_W, NUB_H, out);

    let inner = GAUGE_W - 4.0 * WALL;
    let fill = inner * f32::from(b.percent.min(100)) / 100.0;
    if fill > 0.0 {
        rect(
            cx + 2.0 * WALL,
            y + 2.0 * WALL,
            fill,
            GAUGE_H - 4.0 * WALL,
            out,
        );
    }

    if let (Charge::Charging, Some(tex)) = (b.charge, bolt) {
        out.push(Draw::Tex {
            x,
            y: y + (GAUGE_H - BOLT_H) / 2.0,
            w: BOLT_W,
            h: BOLT_H,
            tex,
            alpha: 1.0,
        });
    }

    if percent.w > 0 {
        let px = cx + GAUGE_W + NUB_W + GAP;
        let py = y + (GAUGE_H - HINT_H as f32) / 2.0;
        out.push(match percent.face {
            Some(tex) => Draw::Tex {
                x: px,
                y: py,
                w: percent.w as f32,
                h: HINT_H as f32,
                tex,
                alpha: 1.0,
            },
            None => Draw::Rect {
                x: px,
                y: py,
                w: percent.w as f32,
                h: HINT_H as f32,
                colour: [1.0, 1.0, 1.0, 0.08],
            },
        });
    }
}

/// How wide `draw_status_gauge` draws: the bolt only takes room while
/// charging, so on battery the gauge sits right next to its percent.
pub fn status_gauge_width(battery: Option<Battery>, percent: Printed) -> f32 {
    let Some(b) = battery else { return 0.0 };
    let after = if b.charge == Charge::Charging { BOLT_GAP + BOLT_W + BOLT_TEXT_GAP } else { GAP };
    GAUGE_W + NUB_W + after + percent.w as f32
}

/// Under this, and not charging, the shelf's battery turns red (#87).
pub const LOW_PERCENT: u8 = 15;
/// Under this, and not charging, a small red battery blinks over the game.
pub const VERY_LOW_PERCENT: u8 = 10;
/// The low battery's red: reads on the dark and the light themes.
pub const LOW_RED: [u8; 3] = [255, 92, 80];

/// Whether `battery` is under `below` percent and running on itself.
pub fn battery_low(battery: Option<Battery>, below: u8) -> bool {
    battery.is_some_and(|b| b.percent < below && matches!(b.charge, Charge::Discharging | Charge::Unknown))
}

/// The game's low battery sign (#87): a red battery, nearly empty, in a dark
/// pill, BADGE_W x BADGE_H, drawn at the screen's density.
pub const BADGE_W: f32 = 46.0;
pub const BADGE_H: f32 = 24.0;
pub fn low_battery_badge() -> crate::UndoFace {
    let (w, h) = (BADGE_W as u32, BADGE_H as u32);
    let (tw, th) = (crate::density::px(w), crate::density::px(h));
    let s = tw as f32 / BADGE_W;
    // Logical shapes: the pill, then the gauge's walls, nub and fill.
    let (gw, gh, wall) = (26.0f32, 13.0f32, 2.0f32);
    let (gx, gy) = (8.0f32, (BADGE_H - gh) / 2.0);
    let inside = |x: f32, y: f32, l: f32, t: f32, r: f32, b: f32| x >= l && x < r && y >= t && y < b;
    let pill = |x: f32, y: f32| {
        let r = BADGE_H / 2.0;
        let cx = x.clamp(r, BADGE_W - r);
        (x - cx).powi(2) + (y - r).powi(2) <= r * r
    };
    let red = |x: f32, y: f32| {
        let walls = inside(x, y, gx, gy, gx + gw, gy + gh)
            && !inside(x, y, gx + wall, gy + wall, gx + gw - wall, gy + gh - wall);
        let nub = inside(x, y, gx + gw, gy + gh / 2.0 - 2.5, gx + gw + 3.0, gy + gh / 2.0 + 2.5);
        let fill = inside(x, y, gx + 2.0 * wall, gy + 2.0 * wall,
            gx + 2.0 * wall + (gw - 4.0 * wall) * 0.15, gy + gh - 2.0 * wall);
        walls || nub || fill
    };
    let mut rgba = vec![0u8; (tw * th * 4) as usize];
    const N: u32 = 4;
    for ty in 0..th {
        for tx in 0..tw {
            let (mut p, mut g) = (0u32, 0u32);
            for sy in 0..N {
                for sx in 0..N {
                    let x = (tx as f32 + (sx as f32 + 0.5) / N as f32) / s;
                    let y = (ty as f32 + (sy as f32 + 0.5) / N as f32) / s;
                    if pill(x, y) { p += 1; }
                    if red(x, y) { g += 1; }
                }
            }
            let (p, g) = (p as f32 / (N * N) as f32, g as f32 / (N * N) as f32);
            // Red over the pill's black at 0.6.
            let a = g + p * 0.6 * (1.0 - g);
            let i = ((ty * tw + tx) * 4) as usize;
            if a > 0.0 {
                for c in 0..3 { rgba[i + c] = (LOW_RED[c] as f32 * g / a).round() as u8; }
                rgba[i + 3] = (a * 255.0).round() as u8;
            }
        }
    }
    crate::UndoFace { rgba, w, h }
}

/// The Status Bar's battery like a phone's (Siham 2026-11-09): gauge, then
/// the bolt while charging, then the percent, ending at `right`. The bolt
/// keeps its size, a little taller than the gauge.
pub fn draw_status_gauge(
    right: f32, y: f32, battery: Option<Battery>, percent: Printed,
    bolt: Option<TexId>, ink: [f32; 4], out: &mut Vec<Draw>,
) {
    let Some(b) = battery else { return };
    let x = right - status_gauge_width(battery, percent);
    let rect = |x: f32, y: f32, w: f32, h: f32, out: &mut Vec<Draw>| {
        out.push(Draw::Rect { x, y, w, h, colour: ink });
    };
    rect(x, y, GAUGE_W, WALL, out);
    rect(x, y + GAUGE_H - WALL, GAUGE_W, WALL, out);
    rect(x, y, WALL, GAUGE_H, out);
    rect(x + GAUGE_W - WALL, y, WALL, GAUGE_H, out);
    rect(x + GAUGE_W, y + (GAUGE_H - NUB_H) / 2.0, NUB_W, NUB_H, out);
    let fill = (GAUGE_W - 4.0 * WALL) * f32::from(b.percent.min(100)) / 100.0;
    if fill > 0.0 {
        // Low (#87): the charge left in red; the percent is made red too.
        let colour = if battery_low(battery, LOW_PERCENT) {
            [LOW_RED[0] as f32 / 255.0, LOW_RED[1] as f32 / 255.0, LOW_RED[2] as f32 / 255.0, ink[3]]
        } else { ink };
        out.push(Draw::Rect { x: x + 2.0 * WALL, y: y + 2.0 * WALL, w: fill, h: GAUGE_H - 4.0 * WALL, colour });
    }
    let mut px = x + GAUGE_W + NUB_W + GAP;
    if b.charge == Charge::Charging {
        let bx = x + GAUGE_W + NUB_W + BOLT_GAP;
        if let Some(tex) = bolt {
            out.push(Draw::Tex { x: bx, y: y + (GAUGE_H - BOLT_H) / 2.0, w: BOLT_W, h: BOLT_H, tex, alpha: 1.0 });
        }
        px = bx + BOLT_W + BOLT_TEXT_GAP;
    }
    if percent.w > 0 {
        let py = y + (GAUGE_H - HINT_H as f32) / 2.0;
        out.push(match percent.face {
            Some(tex) => Draw::Tex { x: px, y: py, w: percent.w as f32, h: HINT_H as f32, tex, alpha: 1.0 },
            None => Draw::Rect { x: px, y: py, w: percent.w as f32, h: HINT_H as f32, colour: [1.0, 1.0, 1.0, 0.08] },
        });
    }
}

#[cfg(test)]
mod status_tests {
    use super::*;

    #[test]
    fn low_means_under_the_line_and_not_charging() {
        let b = |percent, charge| Some(Battery { percent, charge });
        assert!(battery_low(b(14, Charge::Discharging), LOW_PERCENT));
        assert!(!battery_low(b(15, Charge::Discharging), LOW_PERCENT));
        assert!(!battery_low(b(5, Charge::Charging), LOW_PERCENT));
        assert!(!battery_low(b(5, Charge::Full), LOW_PERCENT));
        assert!(!battery_low(None, LOW_PERCENT));
        assert!(battery_low(b(9, Charge::Discharging), VERY_LOW_PERCENT));
        assert!(!battery_low(b(12, Charge::Discharging), VERY_LOW_PERCENT));
        let mut out = Vec::new();
        draw_status_gauge(600.0, 10.0, b(12, Charge::Discharging), printed(30), None, INK, &mut out);
        let red = [LOW_RED[0] as f32 / 255.0, LOW_RED[1] as f32 / 255.0, LOW_RED[2] as f32 / 255.0, 1.0];
        assert_eq!(out.iter().filter(|d| matches!(d, Draw::Rect { colour, .. } if *colour == red)).count(), 1);
        let badge = low_battery_badge();
        assert_eq!((badge.w, badge.h), (BADGE_W as u32, BADGE_H as u32));
        assert!(badge.rgba.chunks_exact(4).any(|p| p[0] == LOW_RED[0] && p[3] == 255));
    }

    fn printed(w: u32) -> Printed { Printed { face: None, w } }

    #[test]
    fn status_gauge_ends_at_right_and_bolt_only_when_charging() {
        for (charge, after) in [(Charge::Charging, BOLT_GAP + BOLT_W + BOLT_TEXT_GAP), (Charge::Discharging, GAP)] {
            let b = Some(Battery { percent: 78, charge });
            let w = status_gauge_width(b, printed(30));
            assert!((w - (GAUGE_W + NUB_W + after + 30.0)).abs() < 0.01);
            let mut out = Vec::new();
            draw_status_gauge(600.0, 10.0, b, printed(30), Some(TexId::from_raw(7)), INK, &mut out);
            let ends = out.iter().map(|d| match d {
                Draw::Rect { x, w, .. } | Draw::Tex { x, w, .. } => x + w,
                _ => 0.0,
            }).fold(0.0f32, f32::max);
            assert!((ends - 600.0).abs() < 0.01);
            let bolts = out.iter().filter(|d| matches!(d, Draw::Tex { tex, .. } if *tex == TexId::from_raw(7))).count();
            assert_eq!(bolts, usize::from(charge == Charge::Charging));
        }
    }
}
