//! PokeDaisy's field HUD, drawn by slot. over the picture (its
//! PD_ENV_SET_HUD): at the screen's corners in EXPAND and every ZOOM step,
//! at its own pixels, and never through the game's shader. Its parts stay
//! where PokeDaisy's touch looks for them (the same box).
//!
//! Based on PokeDaisy by Lidor Itzhari - https://github.com/lidor30/pokedaisy
use super::*;
use slot_gfx::TexId;

/// The HUD as PokeDaisy last sent it, as textures: the whole picture, or
/// its four quarters for the smaller sizes (each at its own corner).
#[derive(Default)]
pub(super) struct PokedaisyHud {
    whole: Option<TexId>,
    quarters: [Option<TexId>; 4],
    shown: Option<slot_retro::PdHud>,
}

impl PokedaisyHud {
    /// A new HUD from the core: its textures made again from its pixels.
    pub(super) fn update(&mut self, gpu: &mut Compositor, hud: slot_retro::PdHud) {
        if !hud.active || hud.size.0 == 0 || hud.size.1 == 0 {
            self.shown = None;
            return;
        }
        let (w, h) = hud.size;
        if hud.pct >= 100 {
            put(gpu, &mut self.whole, w, h, &hud.rgba);
        } else {
            let (qw, qh) = (w / 2, h / 2);
            for (i, slot) in self.quarters.iter_mut().enumerate() {
                let (qx, qy) = ((i as u32 % 2) * qw, (i as u32 / 2) * qh);
                let mut part = Vec::with_capacity((qw * qh * 4) as usize);
                for y in qy..qy + qh {
                    let at = ((y * w + qx) * 4) as usize;
                    part.extend_from_slice(&hud.rgba[at..at + (qw * 4) as usize]);
                }
                put(gpu, slot, qw, qh, &part);
            }
        }
        self.shown = Some(hud);
    }

    pub(super) fn clear(&mut self) {
        self.shown = None;
    }

    /// The HUD over the picture at `rect` (the canvas units the picture is
    /// drawn in), for a core canvas of `frame` pixels.
    pub(super) fn draw(&self, rect: Option<[f32; 4]>, frame: (u32, u32), out: &mut Vec<Draw>) {
        let Some(hud) = self.shown.as_ref() else { return };
        let places = places(hud, rect, frame);
        let texs: Vec<Option<TexId>> = if hud.pct >= 100 { vec![self.whole] } else { self.quarters.to_vec() };
        for ([x, y, w, h], tex) in places.into_iter().zip(texs) {
            if let Some(tex) = tex { out.push(Draw::Tex { x, y, w, h, tex, alpha: 1.0 }); }
        }
    }
}

/// Where the HUD goes over the picture at `rect` for a core canvas of
/// `frame` pixels: its box, or each quarter at its own corner of the box
/// (top left, top right, bottom left, bottom right), as PokeDaisy's own
/// host draws it.
fn places(hud: &slot_retro::PdHud, rect: Option<[f32; 4]>, frame: (u32, u32)) -> Vec<[f32; 4]> {
    let Some([px, py, pw, ph]) = rect else { return Vec::new() };
    if frame.0 == 0 || frame.1 == 0 { return Vec::new(); }
    let (sx, sy) = (pw / frame.0 as f32, ph / frame.1 as f32);
    let [x, y, w, h] = hud.area;
    let (bx, by, bw, bh) = (px + x as f32 * sx, py + y as f32 * sy, w as f32 * sx, h as f32 * sy);
    if hud.pct >= 100 { return vec![[bx, by, bw, bh]]; }
    let (qw, qh) = (bw * hud.pct as f32 / 200.0, bh * hud.pct as f32 / 200.0);
    if qw < 1.0 || qh < 1.0 { return Vec::new(); }
    (0..4).map(|i| [if i % 2 == 0 { bx } else { bx + bw - qw },
        if i / 2 == 0 { by } else { by + bh - qh }, qw, qh]).collect()
}

fn put(gpu: &mut Compositor, slot: &mut Option<TexId>, w: u32, h: u32, rgba: &[u8]) {
    match *slot {
        Some(tex) => gpu.update_texture(tex, w, h, rgba),
        None => *slot = Some(gpu.create_texture_nearest(w, h, rgba)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_smaller_hud_sits_at_the_four_corners_of_the_picture() {
        let hud = slot_retro::PdHud { rgba: vec![0; 4 * 4 * 4], size: (4, 4),
            area: [0, 0, 240, 160], pct: 50, active: true };
        assert_eq!(places(&hud, Some([0.0, 0.0, 960.0, 640.0]), (240, 160)),
            [[0.0, 0.0, 240.0, 160.0], [720.0, 0.0, 240.0, 160.0],
             [0.0, 480.0, 240.0, 160.0], [720.0, 480.0, 240.0, 160.0]]);
        let large = slot_retro::PdHud { pct: 100, area: [0, 0, 210, 160], ..hud };
        assert_eq!(places(&large, Some([60.0, 0.0, 840.0, 640.0]), (210, 160)),
            [[60.0, 0.0, 840.0, 640.0]]);
        assert!(places(&large, None, (210, 160)).is_empty());
    }
}
