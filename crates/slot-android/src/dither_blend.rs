//! Mega Drive Dither Blending: Sp00kyFox's GDAPT (Genesis Dithering and
//! Pseudo Transparency v1.3, libretro/glsl-shaders), its two passes on the
//! picture before it is uploaded, so every shader draws the blended one.
//! Mega Drive games drew water, shadows and glass as one-pixel vertical
//! stripes that a TV's blur turned into a see-through colour: here the
//! stripes are blended back into that colour. Defaults: colour analysis
//! (MODE 0), metric exponent 2 (PWR), one step (STEPS 1).

#[derive(Default)]
pub struct DitherBlend {
    rgb: Vec<[f32; 3]>,
    tag: Vec<f32>,
    out: Vec<u8>,
}

/// How alike two colours are (compuphase's weighted metric), 1 the same.
fn alike(a: [f32; 3], b: [f32; 3]) -> f32 {
    let ravg = (a[0] + b[0]) * 0.5;
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let dist = (d[0] * d[0] * (2.0 + ravg) + d[1] * d[1] * 4.0 + d[2] * d[2] * (3.0 - ravg)).sqrt();
    // smoothstep(3, 0, dist), squared.
    let t = ((3.0 - dist) / 3.0).clamp(0.0, 1.0);
    let s = t * t * (3.0 - 2.0 * t);
    s * s
}

fn unit(v: [f32; 3]) -> [f32; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n > 0.0 { [v[0] / n, v[1] / n, v[2] / n] } else { [0.0; 3] }
}

impl DitherBlend {
    /// The picture to upload for `frame` (XRGB8888 bytes, `size` pixels).
    pub fn apply<'a>(&'a mut self, frame: &'a [u8], (w, h): (u32, u32)) -> &'a [u8] {
        let (w, h) = (w as usize, h as usize);
        if w < 3 || frame.len() < w * h * 4 {
            return frame;
        }
        let n = w * h;
        self.rgb.clear();
        self.rgb.extend(frame[..n * 4].chunks_exact(4)
            .map(|p| [p[2] as f32 / 255.0, p[1] as f32 / 255.0, p[0] as f32 / 255.0]));
        // Pass 0: a pixel unlike both its neighbours, which are alike, is
        // part of a stripe pattern.
        self.tag.resize(n, 0.0);
        for y in 0..h {
            let row = &self.rgb[y * w..(y + 1) * w];
            for x in 0..w {
                let c = row[x];
                let l = row[x.saturating_sub(1)];
                let r = row[(x + 1).min(w - 1)];
                let a = unit([c[0] - l[0], c[1] - l[1], c[2] - l[2]]);
                let b = unit([c[0] - r[0], c[1] - r[1], c[2] - r[2]]);
                let dot = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(0.0, 1.0);
                self.tag[y * w + x] = dot * alike(l, r);
            }
        }
        // Pass 1: blend a tagged pixel with its tagged neighbours.
        self.out.resize(n * 4, 0);
        self.out.copy_from_slice(&frame[..n * 4]);
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let (li, ri) = (y * w + x.saturating_sub(1), y * w + (x + 1).min(w - 1));
                let (lw, rw) = (self.tag[li], self.tag[ri]);
                let strength = lw.max(rw).min(self.tag[i]);
                if strength <= 0.0 { continue; }
                let sum = lw + rw;
                let most = lw.max(rw);
                let weight = if most == 0.0 { 1.0 } else { sum / most };
                let (c, l, r) = (self.rgb[i], self.rgb[li], self.rgb[ri]);
                for k in 0..3 {
                    let mixed = (weight * c[k] + lw * l[k] + rw * r[k]) / (weight + sum);
                    let v = c[k] + (mixed - c[k]) * strength;
                    // XRGB8888 is B, G, R, X in memory.
                    self.out[i * 4 + 2 - k] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        &self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(w: usize, h: usize, at: impl Fn(usize, usize) -> [u8; 3]) -> Vec<u8> {
        (0..w * h).flat_map(|i| { let [r, g, b] = at(i % w, i / w); [b, g, r, 0] }).collect()
    }

    #[test]
    fn vertical_stripes_become_their_mixed_colour() {
        let frame = picture(16, 4, |x, _| if x % 2 == 0 { [0, 0, 255] } else { [255, 255, 255] });
        let mut d = DitherBlend::default();
        let out = d.apply(&frame, (16, 4)).to_vec();
        // Away from the edges every pixel is close to the blue/white mix.
        for x in 3..13 {
            let i = (16 + x) * 4;
            let (b, g, r) = (out[i], out[i + 1], out[i + 2]);
            assert!(b > 240 && (100..=160).contains(&r) && (100..=160).contains(&g),
                "x {x}: {r} {g} {b}");
        }
    }

    #[test]
    fn flat_colours_and_hard_edges_stay_as_they_are() {
        let frame = picture(16, 4, |x, _| if x < 8 { [200, 40, 40] } else { [10, 10, 90] });
        let mut d = DitherBlend::default();
        assert_eq!(d.apply(&frame, (16, 4)), &frame[..]);
    }
}
