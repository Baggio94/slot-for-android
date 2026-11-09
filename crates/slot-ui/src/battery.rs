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
    let bolt = if b.charge == Charge::Charging { BOLT_GAP + BOLT_W } else { 0.0 };
    GAUGE_W + NUB_W + bolt + GAP + percent.w as f32
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
        rect(x + 2.0 * WALL, y + 2.0 * WALL, fill, GAUGE_H - 4.0 * WALL, out);
    }
    let mut px = x + GAUGE_W + NUB_W + GAP;
    if b.charge == Charge::Charging {
        let bx = x + GAUGE_W + NUB_W + BOLT_GAP;
        if let Some(tex) = bolt {
            out.push(Draw::Tex { x: bx, y: y + (GAUGE_H - BOLT_H) / 2.0, w: BOLT_W, h: BOLT_H, tex, alpha: 1.0 });
        }
        px = bx + BOLT_W + GAP;
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

    fn printed(w: u32) -> Printed { Printed { face: None, w } }

    #[test]
    fn status_gauge_ends_at_right_and_bolt_only_when_charging() {
        for (charge, bolt_room) in [(Charge::Charging, BOLT_GAP + BOLT_W), (Charge::Discharging, 0.0)] {
            let b = Some(Battery { percent: 78, charge });
            let w = status_gauge_width(b, printed(30));
            assert!((w - (GAUGE_W + NUB_W + bolt_room + GAP + 30.0)).abs() < 0.01);
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
