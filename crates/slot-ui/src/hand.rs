//! A cart with no label art gets its title handwritten on the plastic, in
//! black or white marker (slot. original 1.16, Siham 2026-11-16). The hand
//! is "No Smear" (Brandon T. Kowalski, SIL Open Font License 1.1): its
//! `rand` alternates and a seed from the game's name make each cart's
//! writing its own. Written still, without the original's pen animation.
use std::collections::HashMap;
use std::sync::OnceLock;

use fontdue::{Font, FontSettings};
use rustybuzz::{
    ttf_parser::{GlyphId, Tag},
    Face, Feature, UnicodeBuffer,
};

const HAND_TTF: &[u8] = include_bytes!("../assets/NoSmear.ttf");

const CAP: f32 = 0.70;
const LEADING: f32 = 1.02;
const CAP_ACCENT_ROOM: f32 = 0.3;
const ACCENT_ROOM: f32 = 0.15;
const DESCENT: f32 = 0.20;

pub struct Hand {
    face: Face<'static>,
    raster: Font,
    alternates: HashMap<u16, Vec<u16>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub glyph: u16,
    pub x: f32,
    pub y: f32,
}

/// The writing: one coverage byte per texel of the well, and its ink.
pub struct Written {
    pub w: u32,
    pub h: u32,
    pub ink: Vec<u8>,
    pub colour: [u8; 3],
}

fn load_alternates(face: &Face) -> HashMap<u16, Vec<u16>> {
    use rustybuzz::ttf_parser::gsub::SubstitutionSubtable;
    let mut out = HashMap::new();
    let Some(gsub) = face.tables().gsub else {
        return out;
    };
    let rand = Tag::from_bytes(b"rand");
    let features = (0..gsub.features.len()).filter_map(|i| gsub.features.get(i));
    for feature in features.filter(|f| f.tag == rand) {
        for index in feature.lookup_indices {
            let Some(lookup) = gsub.lookups.get(index) else {
                continue;
            };
            for sub in 0..lookup.subtables.len() {
                let Some(SubstitutionSubtable::Alternate(alt)) =
                    lookup.subtables.get::<SubstitutionSubtable>(sub)
                else {
                    continue;
                };
                for g in 0..face.number_of_glyphs() {
                    let Some(set) = alt
                        .coverage
                        .get(GlyphId(g))
                        .and_then(|i| alt.alternate_sets.get(i))
                    else {
                        continue;
                    };
                    let mut all = vec![g];
                    all.extend(set.alternates.into_iter().map(|a| a.0));
                    out.insert(g, all);
                }
            }
        }
    }
    out
}

fn picks(text: &str, seed: u64) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (n, word) in text.split_whitespace().enumerate() {
        let at = word.as_ptr() as usize - text.as_ptr() as usize;
        let key = seed ^ fnv(word) ^ (n as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let mut last: Option<(char, usize)> = None;
        for (j, (i, ch)) in word.char_indices().enumerate() {
            let roll = (noise(key, j as u32) * 10_000.0) as usize;
            // Two of the same letter in a row never take the same alternate.
            let pick = match last {
                Some((prev, p)) if prev == ch && roll % 5 == p % 5 => roll + 1,
                _ => roll,
            };
            last = Some((ch, pick));
            out.push((at + i, pick));
        }
    }
    out
}

/// The hand, loaded once; None if the font can't be read.
pub fn hand_font() -> Option<&'static Hand> {
    static FONT: OnceLock<Option<Hand>> = OnceLock::new();
    FONT.get_or_init(|| {
        let face = Face::from_slice(HAND_TTF, 0)?;
        Some(Hand {
            alternates: load_alternates(&face),
            raster: Font::from_bytes(HAND_TTF, FontSettings::default()).ok()?,
            face,
        })
    })
    .as_ref()
}

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn leading(line: &str) -> f32 {
    let accented = line.chars().filter(|c| !c.is_ascii() && c.is_alphabetic());
    let room = accented.fold(0.0, |room: f32, c| {
        room.max(if c.is_uppercase() { CAP_ACCENT_ROOM } else { ACCENT_ROOM })
    });
    LEADING + room
}

fn gaps(lines: &[String]) -> f32 {
    lines.iter().skip(1).map(|l| leading(l)).sum()
}

fn noise(seed: u64, i: u32) -> f32 {
    let mut h = seed ^ (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    h ^= h >> 31;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 29;
    (h % 10_000) as f32 / 10_000.0
}

impl Hand {
    fn shape_from(&self, text: &str, px: f32, seed: u64) -> (Vec<Placed>, f32) {
        let picks = picks(text, seed);
        let off = |tag: &[u8; 4]| Feature::new(Tag::from_bytes(tag), 0, ..);
        let features = vec![off(b"rand"), off(b"calt")];
        let mut buf = UnicodeBuffer::new();
        buf.push_str(text);
        let out = rustybuzz::shape(&self.face, &features, buf);
        let scale = px / self.face.units_per_em() as f32;
        let mut pen = 0.0;
        let mut placed = Vec::with_capacity(out.len());
        for (info, pos) in out.glyph_infos().iter().zip(out.glyph_positions()) {
            let shaped = info.glyph_id as u16;
            let pick = picks
                .binary_search_by_key(&(info.cluster as usize), |(i, _)| *i)
                .ok()
                .and_then(|at| {
                    let all = self.alternates.get(&shaped)?;
                    Some(all[picks[at].1 % all.len()])
                });
            let (glyph, advance) = match pick {
                Some(g) if g != shaped => (
                    g,
                    self.face.glyph_hor_advance(GlyphId(g)).map_or(pos.x_advance, |a| a as i32),
                ),
                _ => (shaped, pos.x_advance),
            };
            placed.push(Placed {
                glyph,
                x: pen + pos.x_offset as f32 * scale,
                y: pos.y_offset as f32 * scale,
            });
            pen += advance as f32 * scale;
        }
        (placed, pen)
    }

    /// The title on one to three lines, as big as the box takes.
    fn fit(&self, title: &str, seed: u64, bw: f32, bh: f32) -> (Vec<String>, f32) {
        let words: Vec<&str> = title.split_whitespace().collect();
        if words.is_empty() {
            return (Vec::new(), 0.0);
        }
        let unit = |s: &str| self.shape_from(s, 100.0, seed).1 / 100.0;
        let mut best: (Vec<String>, f32) = (vec![words.join(" ")], 0.0);
        let n = words.len();
        let mut splits: Vec<Vec<usize>> = vec![vec![]];
        for a in 1..n {
            splits.push(vec![a]);
            for b in a + 1..n {
                splits.push(vec![a, b]);
            }
        }
        for cut in splits {
            let mut lines = Vec::new();
            let mut start = 0;
            for &c in cut.iter().chain(std::iter::once(&n)) {
                lines.push(words[start..c].join(" "));
                start = c;
            }
            let widest = lines.iter().map(|l| unit(l)).fold(0.0, f32::max);
            let block = CAP + gaps(&lines) + DESCENT;
            let px = (bw / widest.max(0.01)).min(bh / block);
            // A split must be clearly bigger to win: fewer lines otherwise.
            if px > best.1 * 1.04 {
                best = (lines, px);
            }
        }
        best
    }

    /// `title` written by hand across a `w` x `h` well whose plastic is
    /// `recess`, slightly tilted; `key` (the game) seeds its letters.
    pub fn write(&self, title: &str, key: &str, w: u32, h: u32, recess: [u8; 3]) -> Written {
        let seed = fnv(key);
        let (fw, fh) = (w as f32, h as f32);
        let tilt = (noise(seed, 1) - 0.5) * 0.07;
        let (bx, by, bw, bh, slope) = (fw * 0.07, fh * 0.12, fw * 0.86, fh * 0.76, tilt * 0.6);
        let colour = ink_for(recess);
        let (lines, px) = self.fit(title, seed, bw, bh);
        let mut ink = vec![0u8; (w * h) as usize];
        let block = CAP * px + gaps(&lines) * px;
        let mut baseline = by + (bh - block - DESCENT * px) / 2.0 + CAP * px;
        let mid = fw / 2.0;
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                baseline += leading(line) * px;
            }
            let (placed, width) = self.shape_from(line, px, seed);
            let left = bx + (bw - width) / 2.0;
            for g in placed {
                let gx = left + g.x;
                let lift = (gx - mid) * slope;
                let base = baseline - g.y + lift;
                let (m, cov) = self.raster.rasterize_indexed(g.glyph, px);
                stamp(
                    &mut ink,
                    w,
                    h,
                    (gx + m.xmin as f32).round() as i32,
                    (base - (m.height as f32 + m.ymin as f32)).round() as i32,
                    &cov,
                    m.width as u32,
                    m.height as u32,
                );
            }
        }
        Written { w, h, ink, colour }
    }
}

const INKS: [[u8; 3]; 2] = [[18, 18, 22], [250, 250, 246]];

/// Black or white marker, whichever stands out more on the plastic.
pub fn ink_for(shell: [u8; 3]) -> [u8; 3] {
    let lum = |c: [u8; 3]| {
        let lin = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
    };
    let contrast = |a: [u8; 3]| {
        let (x, y) = (lum(a), lum(shell));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    };
    INKS.into_iter().max_by(|a, b| contrast(*a).total_cmp(&contrast(*b))).unwrap_or(INKS[0])
}

#[allow(clippy::too_many_arguments)]
fn stamp(dst: &mut [u8], dst_w: u32, dst_h: u32, x: i32, y: i32, cov: &[u8], gw: u32, gh: u32) {
    for gy in 0..gh {
        let dy = y + gy as i32;
        if dy < 0 || dy >= dst_h as i32 {
            continue;
        }
        for gx in 0..gw {
            let dx = x + gx as i32;
            if dx < 0 || dx >= dst_w as i32 {
                continue;
            }
            let i = (dy as u32 * dst_w + dx as u32) as usize;
            dst[i] = dst[i].max(cov[(gy * gw + gx) as usize]);
        }
    }
}
