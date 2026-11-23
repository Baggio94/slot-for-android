//! slot.'s own overlays for the 3.0 systems, in Watomsk's style (Siham
//! 2026-11-21): black, the console's logo read top to bottom in the bar
//! on the right (centred in the band under a picture at the top of the
//! screen), a red power light on the left, Watomsk's inner shadow along
//! the window. Only the Sega ones keep his faint grid behind the logo.
//! Drawn for the real panel, so the window is always the picture's rect.

/// One of the overlays.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Design {
    MasterSystem, GameGear,
    NeoGeoPocket, NeoGeoPocketColor,
    WonderSwan, WonderSwanColor,
    Lynx, LynxGold,
    PokemonMini,
}

pub const ALL: [Design; 9] = [
    Design::MasterSystem, Design::GameGear,
    Design::NeoGeoPocket, Design::NeoGeoPocketColor,
    Design::WonderSwan, Design::WonderSwanColor,
    Design::Lynx, Design::LynxGold,
    Design::PokemonMini,
];

/// Where the logo goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Place {
    /// Bars left and right of the picture.
    Side,
    /// The picture at the top of the screen, a band under it.
    Band,
}

fn logo_png(design: Design) -> &'static [u8] {
    use Design::*;
    match design {
        MasterSystem => include_bytes!("../../../assets/overlays-3.0/master-system.png"),
        GameGear => include_bytes!("../../../assets/overlays-3.0/game-gear.png"),
        NeoGeoPocket => include_bytes!("../../../assets/overlays-3.0/neo-geo-pocket.png"),
        NeoGeoPocketColor => include_bytes!("../../../assets/overlays-3.0/neo-geo-pocket-color.png"),
        WonderSwan => include_bytes!("../../../assets/overlays-3.0/wonderswan.png"),
        WonderSwanColor => include_bytes!("../../../assets/overlays-3.0/wonderswan-color.png"),
        Lynx => include_bytes!("../../../assets/overlays-3.0/lynx.png"),
        LynxGold => include_bytes!("../../../assets/overlays-3.0/lynx-gold.png"),
        PokemonMini => include_bytes!("../../../assets/overlays-3.0/pokemon-mini.png"),
    }
}

const POWER_PNG: &[u8] = include_bytes!("../../../assets/overlays-3.0/power.png");

/// Sega's overlays keep the grid behind the logo (Siham 2026-11-21).
fn grid(design: Design) -> bool {
    matches!(design, Design::MasterSystem | Design::GameGear)
}

/// A side bar narrower than this share of the screen's height would only
/// cover the picture's edge (8 %: 86 px on 1080, the Master System's bars
/// on a 3:2 screen are 90, as the NES's).
pub const MIN_BAR: f32 = 0.08;

/// An RGBA picture and its size.
struct Image {
    rgba: Vec<u8>,
    w: u32,
    h: u32,
}

impl Image {
    fn decode(bytes: &[u8]) -> Option<Image> {
        let (rgba, w, h) = crate::art::decode_png_bytes(bytes)?;
        Some(Image { rgba, w, h })
    }

    /// Premultiplied texel, clamped to the edge.
    fn at(&self, x: i32, y: i32) -> [f32; 4] {
        let x = x.clamp(0, self.w as i32 - 1) as u32;
        let y = y.clamp(0, self.h as i32 - 1) as u32;
        let p = &self.rgba[((y * self.w + x) * 4) as usize..][..4];
        let a = p[3] as f32 / 255.0;
        [p[0] as f32 * a, p[1] as f32 * a, p[2] as f32 * a, p[3] as f32]
    }

    /// Bilinear, premultiplied, at source pixel coordinates.
    fn bilinear(&self, x: f32, y: f32) -> [f32; 4] {
        let (fx, fy) = (x - 0.5, y - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as i32, y0 as i32);
        let (a, b, c, d) = (self.at(x0, y0), self.at(x0 + 1, y0),
            self.at(x0, y0 + 1), self.at(x0 + 1, y0 + 1));
        let mut out = [0.0; 4];
        for i in 0..4 {
            out[i] = (a[i] * (1.0 - tx) + b[i] * tx) * (1.0 - ty)
                + (c[i] * (1.0 - tx) + d[i] * tx) * ty;
        }
        out
    }

    /// The average over a screen pixel's footprint (`step` source pixels),
    /// so a logo shrunk a lot keeps its thin strokes smooth.
    fn area(&self, x: f32, y: f32, step: f32) -> [f32; 4] {
        let n = step.ceil().clamp(1.0, 8.0) as i32;
        let mut sum = [0.0; 4];
        for j in 0..n {
            for i in 0..n {
                let sx = x + step * ((i as f32 + 0.5) / n as f32 - 0.5);
                let sy = y + step * ((j as f32 + 0.5) / n as f32 - 0.5);
                let p = self.bilinear(sx, sy);
                for c in 0..4 { sum[c] += p[c]; }
            }
        }
        sum.map(|v| v / (n * n) as f32)
    }
}

/// The panel being drawn, opaque black to start with.
struct Canvas {
    rgba: Vec<u8>,
    w: u32,
    h: u32,
}

impl Canvas {
    fn new(w: u32, h: u32) -> Canvas {
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for p in rgba.chunks_exact_mut(4) { p[3] = 255; }
        Canvas { rgba, w, h }
    }

    /// Lay `rgb` over the pixel at `alpha` (0..1).
    fn over(&mut self, x: i32, y: i32, rgb: [f32; 3], alpha: f32) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 || alpha <= 0.0 { return; }
        let i = ((y as u32 * self.w + x as u32) * 4) as usize;
        let a = alpha.min(1.0);
        for c in 0..3 {
            let v = self.rgba[i + c] as f32 * (1.0 - a) + rgb[c] * a;
            self.rgba[i + c] = v.round().clamp(0.0, 255.0) as u8;
        }
    }

    /// Lay a premultiplied texel over the pixel.
    fn over_pre(&mut self, x: i32, y: i32, p: [f32; 4]) {
        let a = p[3] / 255.0;
        if a <= 0.0 { return; }
        self.over(x, y, [p[0] / a, p[1] / a, p[2] / a], a);
    }

    /// Draw `img` scaled into the box x, y, w, h; `turned`: a quarter turn
    /// clockwise first, so a logo reads top to bottom.
    fn image(&mut self, img: &Image, [x, y, w, h]: [f32; 4], turned: bool) {
        let (sw, sh) = if turned { (img.h as f32, img.w as f32) } else { (img.w as f32, img.h as f32) };
        let step = sw / w.max(1e-3);
        for oy in y.floor() as i32..(y + h).ceil() as i32 {
            for ox in x.floor() as i32..(x + w).ceil() as i32 {
                let u = (ox as f32 + 0.5 - x) / w * sw;
                let v = (oy as f32 + 0.5 - y) / h * sh;
                if u < 0.0 || v < 0.0 || u >= sw || v >= sh { continue; }
                // Turned: the picture's left edge is the logo's top.
                let (su, sv) = if turned { (v, img.h as f32 - u) } else { (u, v) };
                let p = if step > 1.0 { img.area(su, sv, step) } else { img.bilinear(su, sv) };
                self.over_pre(ox, oy, p);
            }
        }
    }

    /// Watomsk's faint grid, fading from its middle, in the box.
    fn grid(&mut self, [x, y, w, h]: [f32; 4]) {
        let step = (self.h as f32 / 36.0).max(10.0);
        let line = (self.h as f32 / 540.0).max(1.0);
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let gx0 = x + (w % step) / 2.0;
        let gy0 = y + (h % step) / 2.0;
        for oy in y.max(0.0) as i32..(y + h).min(self.h as f32) as i32 {
            for ox in x.max(0.0) as i32..(x + w).min(self.w as f32) as i32 {
                let (px, py) = (ox as f32 - gx0, oy as f32 - gy0);
                let on = px.rem_euclid(step) < line || py.rem_euclid(step) < line;
                if !on { continue; }
                let d = (((ox as f32 + 0.5 - cx) / (w / 2.0)).powi(2)
                    + ((oy as f32 + 0.5 - cy) / (h / 2.0)).powi(2)).sqrt();
                let f = (1.0 - d).clamp(0.0, 1.0).powf(0.7);
                self.over(ox, oy, [255.0; 3], 0.42 * f);
            }
        }
    }

    /// A disc of `rgb`, its edge smoothed over a pixel.
    fn disc(&mut self, cx: f32, cy: f32, r: f32, rgb: [f32; 3], alpha: f32) {
        for oy in (cy - r - 1.0) as i32..=(cy + r + 1.0) as i32 {
            for ox in (cx - r - 1.0) as i32..=(cx + r + 1.0) as i32 {
                let d = ((ox as f32 + 0.5 - cx).powi(2) + (oy as f32 + 0.5 - cy).powi(2)).sqrt();
                self.over(ox, oy, rgb, alpha * (r + 0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// A soft red glow around a point.
    fn glow(&mut self, cx: f32, cy: f32, r: f32, alpha: f32) {
        for oy in (cy - 2.0 * r) as i32..=(cy + 2.0 * r) as i32 {
            for ox in (cx - 2.0 * r) as i32..=(cx + 2.0 * r) as i32 {
                let d = ((ox as f32 + 0.5 - cx).powi(2) + (oy as f32 + 0.5 - cy).powi(2)).sqrt() / r;
                self.over(ox, oy, [220.0, 30.0, 30.0], alpha * (-2.5 * d * d).exp());
            }
        }
    }

    /// The handhelds' power light with POWER under it, in the bar at
    /// x0..x1.
    fn power_light(&mut self, x0: f32, x1: f32, power: &Image) {
        let h = self.h as f32;
        let r = (h / 110.0).max(5.0);
        let (cx, cy) = ((x0 + x1) / 2.0, h * 0.70);
        self.glow(cx, cy, r * 3.0, 0.45);
        self.disc(cx, cy, r, [255.0, 80.0, 70.0], 1.0);
        self.disc(cx - r * 0.25, cy - r * 0.25, r * 0.4, [255.0, 190.0, 180.0], 1.0);
        let th = ((x1 - x0) * 0.17).min(h / 45.0).max(12.0) * 0.78;
        let tw = th * power.w as f32 / power.h as f32;
        self.image(power, [cx - tw / 2.0, cy + r * 3.0, tw, th], false);
    }

    /// The Master System's: a red light bar along the screen's left edge,
    /// as on Watomsk's Genesis overlay.
    fn power_bar(&mut self, width: f32) {
        let h = self.h as f32;
        let bh = (h / 40.0).max(6.0);
        let cy = h * 0.88;
        let bw = width * 0.95;
        for oy in (cy - 3.0 * bh) as i32..=(cy + 3.0 * bh) as i32 {
            for ox in 0..(bw + 2.0 * bh) as i32 {
                let dx = (ox as f32 + 0.5 - bw).max(0.0);
                let dy = ((oy as f32 + 0.5 - cy).abs() - bh / 2.0).max(0.0);
                let d = (dx * dx + dy * dy).sqrt() / bh;
                self.over(ox, oy, [230.0, 20.0, 20.0], 0.47 * (-1.5 * d * d).exp());
            }
        }
        for oy in (cy - bh / 2.0) as i32..(cy + bh / 2.0) as i32 {
            for ox in 0..bw as i32 { self.over(ox, oy, [200.0, 10.0, 10.0], 1.0); }
        }
        self.disc(width / 2.0, cy, 3.0, [255.0, 110.0, 100.0], 1.0);
    }

    /// Cut the window over the picture, Watomsk's inner shadow along it.
    fn window(&mut self, [x, y, w, h]: [f32; 4], base: f32) {
        for oy in y.max(0.0) as i32..(y + h).min(self.h as f32).ceil() as i32 {
            for ox in x.max(0.0) as i32..(x + w).min(self.w as f32).ceil() as i32 {
                let (px, py) = (ox as f32 + 0.5, oy as f32 + 0.5);
                if px < x || py < y || px >= x + w || py >= y + h { continue; }
                let d = (px - x).min(x + w - px).min(py - y).min(y + h - py) - 0.5;
                let i = ((oy as u32 * self.w + ox as u32) * 4) as usize;
                self.rgba[i..i + 4].copy_from_slice(
                    &[0, 0, 0, crate::fit_overlay::shadow(d / base).round() as u8]);
            }
        }
    }
}

/// A box of `aspect` (w / h) as big as fits in `max_w` x `max_h`.
fn fit(aspect: f32, max_w: f32, max_h: f32) -> (f32, f32) {
    if max_w / max_h > aspect { (max_h * aspect, max_h) } else { (max_w, max_w / aspect) }
}

/// The overlay for a `out` panel whose picture sits at `pic` (x, y, w, h,
/// panel pixels). The logo grows with the room it has (Siham 2026-11-21):
/// Watomsk's Genesis bar, 115 px on a 1080 screen, carries a logo about a
/// third of the height long.
pub fn draw(design: Design, place: Place, out: (u32, u32), pic: [f32; 4]) -> Option<Vec<u8>> {
    let (pw, ph) = (out.0 as f32, out.1 as f32);
    let logo = Image::decode(logo_png(design))?;
    let mut canvas = Canvas::new(out.0, out.1);
    let la = logo.w as f32 / logo.h as f32;
    match place {
        Place::Side => {
            let left = pic[0].max(0.0);
            let right0 = (pic[0] + pic[2]).min(pw);
            let right = pw - right0;
            let g = (right / (0.106 * ph)).max(1.0).sqrt().min(1.9);
            // Turned, the logo is 1 / la wide for 1 tall.
            let (w, h) = fit(1.0 / la, (ph * 0.075 * g).min(right * 0.45), (ph * 0.36 * g).min(ph * 0.6));
            let (x, y) = (right0 + (right - w) / 2.0, (ph - h) / 2.0);
            if grid(design) {
                let (gw, gh) = (right * 0.96, (ph * 0.92).min(h * 1.6));
                canvas.grid([right0 + (right - gw) / 2.0, (ph - gh) / 2.0, gw, gh]);
            }
            canvas.image(&logo, [x, y, w, h], true);
            if design == Design::MasterSystem {
                canvas.power_bar(left);
            } else {
                let power = Image::decode(POWER_PNG)?;
                canvas.power_light(0.0, left, &power);
            }
        }
        Place::Band => {
            let top = pic[1] + pic[3];
            let band = (ph - top).max(0.0);
            let g = (band / (0.15 * ph)).max(1.0).sqrt().min(1.6);
            let (w, h) = fit(la, pw * 0.36 * g, (band * 0.45).min(ph * 0.06 * g));
            let (x, y) = ((pw - w) / 2.0, top + (band - h) / 2.0);
            if grid(design) {
                let (gw, gh) = ((pw * 0.8).min(w * 2.2), band * 0.9);
                canvas.grid([(pw - gw) / 2.0, top + (band - gh) / 2.0, gw, gh]);
            }
            canvas.image(&logo, [x, y, w, h], false);
        }
    }
    canvas.window(pic, ph / 1080.0);
    Some(canvas.rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(rgba: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    #[test]
    fn every_logo_decodes() {
        for design in ALL {
            assert!(Image::decode(logo_png(design)).is_some(), "{design:?}");
        }
        assert!(Image::decode(POWER_PNG).is_some());
    }

    #[test]
    fn the_window_shows_the_picture_and_the_bars_are_black() {
        // Neo Geo Pocket on the KONKR's 3:2 at 1620x1080.
        let pic = [241.0, 0.0, 1137.0, 1080.0];
        let rgba = draw(Design::NeoGeoPocket, Place::Side, (1620, 1080), pic).unwrap();
        assert_eq!(px(&rgba, 1620, 810, 540)[3], 0, "window see-through");
        assert_eq!(px(&rgba, 1620, 20, 20), [0, 0, 0, 255], "plain black bar");
        // The logo is in the right bar, around its middle.
        let lit = (300..780).any(|y| (1378..1620).any(|x| px(&rgba, 1620, x, y)[0] > 128));
        assert!(lit, "logo drawn");
        // The power light is on the left bar.
        assert!(px(&rgba, 1620, 120, 756)[0] > 200, "power light");
    }

    #[test]
    fn the_band_carries_the_logo_under_the_picture() {
        // WonderSwan Color on a 4:3 screen: picture at the top, 155 px band.
        let pic = [0.0, 0.0, 1440.0, 925.0];
        let rgba = draw(Design::WonderSwanColor, Place::Band, (1440, 1080), pic).unwrap();
        assert_eq!(px(&rgba, 1440, 720, 400)[3], 0);
        let lit = (930..1080).any(|y| (400..1040).any(|x| px(&rgba, 1440, x, y)[0] > 128));
        assert!(lit);
        assert_eq!(px(&rgba, 1440, 10, 1075), [0, 0, 0, 255]);
    }

    #[test]
    fn only_sega_keeps_the_grid() {
        let pic = [210.0, 0.0, 1200.0, 1080.0];
        let grey = |d| {
            let rgba = draw(d, Place::Side, (1620, 1080), pic).unwrap();
            // Grid lines near the logo, above and below it.
            (60..200).chain(880..1020).flat_map(|y| (1420..1600).map(move |x| (x, y)))
                .any(|(x, y)| { let p = px(&rgba, 1620, x, y); p[0] > 8 && p[0] == p[1] && p[1] == p[2] })
        };
        assert!(grey(Design::GameGear));
        assert!(!grey(Design::PokemonMini));
    }
}
