//! Material calibration for the 3.0 moulds, before the shared colour overlay.
//! Keep geometry/alpha and the original PCB assets intact. The older Nintendo,
//! NES/SNES and Mega Drive fronts retain their existing rendering byte for byte.
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::density::{self, px};
use crate::home::HomeMould;

pub(crate) fn refined(m: HomeMould) -> bool {
    use HomeMould::*;
    matches!(m, NeoGeoPocket | WonderSwan | WonderSwanColor | GameGear | MasterSystem
        | Lynx | PokemonMini | MasterSystemJp | MasterSystemCard | GameGearCodemasters
        | NeoGeo | Pce)
}

/// The moulds whose master is a photo of black or white plastic, recalibrated
/// to the neutral grey every colour is laid on. The standard 3.0 moulds keep
/// their rc2 masters as they are: sharper reliefs (Siham 2026-11-21).
pub(crate) fn calibrated(m: HomeMould) -> bool {
    use HomeMould::*;
    matches!(m, MasterSystemJp | MasterSystemCard | GameGearCodemasters)
}

/// Scan black/white is the source material colour, not a permanent shadow.
/// Values measured on the flat plastic outside the label wells of the masters.
fn neutral(m: HomeMould, value: u8, in_well: bool) -> u8 {
    use HomeMould::*;
    let v = value as f32;
    let delta = if in_well {
        (v - 128.0) * 0.72
    } else {
        match m {
            // These two masters are photographs of black plastic rather than
            // neutral grey. Compress their highlights after lifting the base.
            MasterSystemJp | GameGearCodemasters => {
                let middle = if m == MasterSystemJp { 1.0f32 } else { 11.0 };
                16.0 * ((1.0 + v / 12.0).ln() - (1.0 + middle / 12.0).ln()) - 4.0
            }
            MasterSystemCard => (v - 255.0) * 0.65,
            _ => {
                let middle = match m {
                    NeoGeoPocket => 135.0,
                    WonderSwan | WonderSwanColor => 137.0,
                    GameGear => 132.0, MasterSystem | Pce => 130.0,
                    Lynx => 134.0, PokemonMini => 121.0,
                    _ => 128.0,
                };
                (v - middle) * 0.72
            }
        }
    };
    (128.0 + delta).round().clamp(24.0, 222.0) as u8
}

/// Calibrated once per mould and panel density, shared by every colour/finish.
/// No filter, random work or new texture is added to the animation frame loop.
pub(crate) fn front(m: HomeMould) -> Arc<Vec<u8>> {
    type Cache = Mutex<HashMap<(HomeMould, u32), Arc<Vec<u8>>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let key = (m, density::scale().to_bits());
    let cache = CACHE.get_or_init(Mutex::default);
    if let Some(hit) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return hit.clone();
    }
    let (w, h) = m.size();
    let source = crate::bitmaps::dense_png(m.art().neutral, w, h);
    let mut rgba = source.to_vec();
    let (lx, ly, lw, lh) = m.art().label;
    let (x0, y0, x1, y1) = (px(lx), px(ly), px(lx) + px(lw), px(ly) + px(lh));
    let width = px(w);
    let outline = clean_outline(m, width, px(h));
    for (n, p) in rgba.chunks_exact_mut(4).enumerate() {
        if let Some(mask) = &outline { p[3] = mask[n]; }
        if p[3] == 0 { p[..3].fill(0); continue; }
        let (x, y) = (n as u32 % width, n as u32 / width);
        let in_well = (x0..x1).contains(&x) && (y0..y1).contains(&y);
        for c in &mut p[..3] { *c = neutral(m, *c, in_well); }
        // The Japanese master's lower body was a separate, darker patch of
        // the photo with ragged edges; on the real cart it is the same flat
        // plastic as the frame around the label (Siham 2026-11-21).
        if m == HomeMould::MasterSystemJp && (px(8)..px(304)).contains(&x)
            && (px(213)..px(354)).contains(&y) {
            for c in &mut p[..3] { *c = (135 + (*c as i32 - 123).clamp(-3, 3)) as u8; }
        }
        // This scan retained a narrow strip of the donor game's sticker
        // above the rectangular well. It is plain plastic on a blank cart.
        if m == HomeMould::GameGearCodemasters && (px(41)..px(272)).contains(&x)
            && (px(7)..px(17)).contains(&y) {
            p[..3].fill(128);
        }
    }
    let rgba = Arc::new(rgba);
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(key, rgba.clone());
    rgba
}

/// The Codemasters scan has transparent cracks within the front. Its existing
/// back-shell SVG traces the same mould without those scan defects; use its
/// alpha for the front too, so opening and closing keep one coherent outline.
pub(crate) fn clean_outline(m: HomeMould, w: u32, h: u32) -> Option<Vec<u8>> {
    if m != HomeMould::GameGearCodemasters { return None; }
    let tree = usvg::Tree::from_str(m.art().shell_svg, &usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    let size = tree.size();
    let transform = resvg::tiny_skia::Transform::from_scale(w as f32 / size.width(), h as f32 / size.height());
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(pixmap.data().iter().skip(3).step_by(4).copied().collect())
}

/// Keep the original map's cutouts (mini cap), with denser plastic at reliefs.
pub(crate) fn clear_alpha(m: HomeMould, mapped: u8, shade: u8) -> u8 {
    // Standard moulds and the Pokemon mini keep rc2's frosted clear plastic
    // (Siham 2026-11-22); the recalibrated special moulds get the more
    // translucent one.
    if !calibrated(m) { return mapped; }
    if mapped == 0 { return 0; }
    let (base, original) = (162u32, 204u32);
    let thickness = u32::from(shade.abs_diff(128)) / 2;
    ((u32::from(mapped) * base + original / 2) / original + thickness).min(220) as u8
}

fn mini_screw(x: f32, y: f32) -> bool {
    [(27.0, 52.0), (286.0, 52.0)].into_iter()
        .any(|(cx, cy)| (x - cx).powi(2) + (y - cy).powi(2) < 4.5f32.powi(2))
}

/// Explicit material regions in logical coordinates. Connector metal, PCB,
/// My Card's fixed contact panel and the mini's screw holes never get flecks.
pub(crate) fn plastic_at(m: HomeMould, x: f32, y: f32) -> bool {
    match m {
        HomeMould::WonderSwan | HomeMould::WonderSwanColor => y < 147.0 || x < 21.5 || x > 290.5,
        HomeMould::Lynx => y < 286.0,
        HomeMould::MasterSystemCard => y < 330.0,
        // The HuCard's black panel and its contacts are no plastic.
        HomeMould::Pce => !(8.0..304.0).contains(&x) || !(337.0..483.0).contains(&y),
        HomeMould::PokemonMini => !mini_screw(x, y),
        _ => true,
    }
}

pub(crate) fn decal_plastic_at(m: HomeMould, x: f32, y: f32) -> bool {
    match m {
        HomeMould::WonderSwan | HomeMould::WonderSwanColor => x < 21.5 || x > 290.5,
        HomeMould::Lynx => y < 286.0,
        HomeMould::PokemonMini => !mini_screw(x, y),
        _ => false,
    }
}
