//! L2 and R2 on the carousel (Siham 2026-11-10).
//!
//! Held together for 1.5 s, they delete the game in hand: "HOLD TO DELETE"
//! and a small red ring show only while both are down, letting go cancels.
//! The cart then jumps up and shrinks away (the eject animation the player
//! chose), the carts on its right close the gap, and Android deletes the
//! ROM file; saves and save states stay.
//!
//! Tapped alone on the Game Boy Advance shelf, either one shows the Game Boy
//! Advance Video shelf behind it, and back (as slot. original 1.15).
use super::*;

/// The GBA Video shelf: after the systems, Recents and Favorites. It has
/// no place in the row of shelves; L2 / R2 reach it from the GBA shelf.
pub(super) const VIDEO_RACK: usize = FAVORITES_RACK + 1;
/// How long L2 and R2 are held together to delete.
const HOLD_S: f32 = 1.5;
/// The cart's jump and shrink.
const EJECT_S: f32 = 0.7;
/// The ring is drawn again every 1/48 of the way round.
const RING_STEPS: f32 = 48.0;

/// Requests to Android: the URI of a game to delete.
pub(crate) static DELETE_REQUEST: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// A deleted cart on its way out.
pub(super) struct Ejecting {
    rack: usize,
    uri: PathBuf,
    stem: String,
    face: Option<slot_gfx::TexId>,
    rect: (f32, f32, f32, f32),
    started: Instant,
}

/// The L2 / R2 state on the carousel.
#[derive(Default)]
pub(super) struct Triggers {
    down: [bool; 2],
    /// Both went down together since the first one: no shelf switch.
    both: bool,
    /// Since when both are held.
    pub(super) hold: Option<Instant>,
    /// The ring's face and the step it shows; "HOLD TO DELETE".
    ring: Option<(slot_gfx::TexId, i32)>,
    text: Option<(slot_gfx::TexId, u32, u32)>,
    pub(super) ejecting: Option<Ejecting>,
}

/// A GBA Video cart: its game code starts with M.
pub(crate) fn is_gba_video(cart: &slot_store::Cart) -> bool {
    cart.platform == Platform::Gba && cart.code.len() == 4 && cart.code.starts_with('M')
}

/// Where the eject animation draws the cart at `t` (0 to 1): bottom-anchored,
/// jumping up then shrinking and fading (the preview chosen, at 720 wide).
fn eject_at(rect: (f32, f32, f32, f32), t: f32) -> (f32, f32, f32, f32, f32) {
    let (x, y, w, h) = rect;
    let t = t.clamp(0.0, 1.0);
    let up = if t < 0.5 { (t * 2.0 * std::f32::consts::FRAC_PI_2).sin() * 45.0 } else { 45.0 + (t - 0.5) * 30.0 };
    let s = if t < 0.3 { 1.0 } else { (1.0 - (t - 0.3) / 0.7).max(0.02) };
    let alpha = 1.0 - ((t - 0.4) / 0.6).max(0.0);
    let (cx, foot) = (x + w / 2.0, y + h);
    (cx - w * s / 2.0, foot - h * s - up, w * s, h * s, alpha)
}

impl Engine {
    /// L2 (104) or R2 (105) on the carousel.
    pub(super) fn carousel_trigger(&mut self, code: i32, pressed: bool) {
        let side = usize::from(code == 105);
        let t = &mut self.triggers;
        t.down[side] = pressed;
        if pressed {
            if t.down[1 - side] {
                t.both = true;
                let has_cart = self.shelves.get(self.active).is_some_and(|s| !s.carts.is_empty());
                if has_cart && t.ejecting.is_none() { t.hold = Some(Instant::now()); }
            }
            return;
        }
        // A release: any hold ends; a lone tap switches GBA and GBA Video.
        let lone = !t.both;
        t.hold = None;
        if !t.down[0] && !t.down[1] { t.both = false; }
        if lone { self.flip_video_shelf(); }
    }

    /// Between the GBA shelf and the GBA Video shelf, when both have carts.
    fn flip_video_shelf(&mut self) {
        let to = match self.active {
            0 => VIDEO_RACK,
            VIDEO_RACK => 0,
            _ => return,
        };
        if self.shelves.get(to).is_none_or(|s| s.carts.is_empty()) { return; }
        self.shelves[self.active].release_hold();
        self.active = to;
        self.gba_video = to == VIDEO_RACK;
        self.progress = 0.0;
        self.letter_shown_at = None;
        self.platform_shown_at = Some(Instant::now());
        self.last_cart_navigation = Instant::now();
    }

    /// L1 / R1 from shelf `from` (GBA Video counts as the GBA's place):
    /// the next shelf with carts, landing on GBA Video when it was the one
    /// used last, or when the GBA shelf itself is empty.
    pub(super) fn next_shelf(&self, from: usize, delta: i32) -> usize {
        let next = next_nonempty_shelf(&self.shelves, from, delta);
        self.video_landing(next)
    }

    pub(super) fn video_landing(&self, at: usize) -> usize {
        let video = self.shelves.get(VIDEO_RACK).is_some_and(|s| !s.carts.is_empty());
        let gba = self.shelves.first().is_some_and(|s| !s.carts.is_empty());
        if at == 0 && video && (self.gba_video || !gba) { VIDEO_RACK } else { at }
    }

    /// Every frame on the carousel: the hold ends when it should, the
    /// delete starts once held long enough, the cart leaves.
    pub(super) fn tick_triggers(&mut self) {
        let idle = self.game.is_none() && !self.inserted && self.progress == 0.0
            && matches!(self.overlay, ShelfOverlay::None) && self.picker.is_none();
        if !idle {
            self.triggers.hold = None;
            if self.triggers.ejecting.is_some() { self.finish_eject(); }
            return;
        }
        if let Some(since) = self.triggers.hold {
            let done = since.elapsed().as_secs_f32() / HOLD_S;
            if done >= 1.0 {
                self.triggers.hold = None;
                self.start_eject();
            } else {
                let step = (done * RING_STEPS) as i32;
                if self.triggers.ring.is_none_or(|(_, s)| s != step) {
                    let face = slot_ui::hold_ring_face(step as f32 / RING_STEPS);
                    let tex = match self.triggers.ring {
                        Some((tex, _)) => { self.gpu.update_face(tex, &face); tex }
                        None => self.gpu.create_face(&face),
                    };
                    self.triggers.ring = Some((tex, step));
                }
                if self.triggers.text.is_none() {
                    let face = slot_ui::quick_value_face("HOLD TO DELETE", true);
                    self.triggers.text = Some((self.gpu.create_face(&face), face.w, face.h));
                }
            }
        }
        if self.triggers.ejecting.as_ref().is_some_and(|e| e.started.elapsed().as_secs_f32() >= EJECT_S) {
            self.finish_eject();
        }
    }

    /// The hold is complete: the cart leaves its place, Android deletes it.
    fn start_eject(&mut self) {
        let rack = self.active;
        let Some(shelf) = self.shelves.get(rack) else { return };
        let Some(cart) = shelf.carts.get(shelf.index) else { return };
        let Some(rect) = shelf.selected_rect() else { return };
        let uri = cart.rom.clone();
        DELETE_REQUEST.lock().unwrap_or_else(|e| e.into_inner())
            .push_back(uri.to_string_lossy().into_owned());
        // Silent: no insert or eject sound for a delete (Siham 2026-11-10).
        self.triggers.ejecting = Some(Ejecting {
            rack, stem: cart.stem.clone(), face: shelf.face(shelf.index), rect, uri,
            started: Instant::now(),
        });
    }

    /// The cart is gone: taken off every shelf, the active one closing in.
    fn finish_eject(&mut self) {
        let Some(gone) = self.triggers.ejecting.take() else { return };
        self.remove_cart_everywhere(&gone.uri, Some(gone.rack));
    }

    /// Takes the cart at `uri` off every shelf (and the library), the shelf
    /// `closing` with its gap closing in.
    pub(super) fn remove_cart_everywhere(&mut self, uri: &Path, closing: Option<usize>) {
        for rack in 0..self.shelves.len() {
            let Some(i) = self.shelves[rack].carts.iter().position(|c| c.rom == uri) else { continue };
            self.cart_render.invalidate_shelf(rack);
            let moves = if closing == Some(rack) {
                self.shelves[rack].remove_closing(i)
            } else {
                // The other carts keep their stars: no reading of the
                // favourites file while the shelf moves (Siham's small lag).
                let stars: Vec<std::path::PathBuf> = self.shelves[rack].carts.iter().enumerate()
                    .filter(|(k, _)| self.shelves[rack].starred(*k)).map(|(_, c)| c.rom.clone()).collect();
                let kept: Vec<slot_store::Cart> = self.shelves[rack].carts.iter()
                    .filter(|c| c.rom != uri).cloned().collect();
                let moves = self.shelves[rack].replace_carts_preserving_visuals(kept);
                self.shelves[rack].mark_starred(|cart| stars.contains(&cart.rom));
                moves
            };
            self.rekey_rack_visuals(rack, &moves);
        }
        {
            let text = uri.to_string_lossy();
            let mut state = crate::library::LIBRARY.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(entries) = state.entries.as_mut() { entries.retain(|e| e.uri != text); }
        }
        if self.shelves.get(self.active).is_none_or(|s| s.carts.is_empty()) {
            let from = if self.active == VIDEO_RACK { 0 } else { self.active };
            self.active = self.next_shelf(from, 1);
            if self.shelves.get(self.active).is_none_or(|s| s.carts.is_empty()) && self.active == 0 {
                let video = self.shelves.get(VIDEO_RACK).is_some_and(|s| !s.carts.is_empty());
                if video { self.active = VIDEO_RACK; }
            }
            self.platform_shown_at = Some(Instant::now());
        }
        self.focus_dirty_since = Some(Instant::now());
    }

    /// Android's answer: deleted (its toast says so), or not: Android then
    /// reads the ROM folder again and the cart comes back if it's there.
    pub(super) fn game_deleted(&mut self, _uri: String, _ok: bool) {}

    /// Over the carousel: the leaving cart, then the ring while held.
    pub(super) fn draw_triggers(&self, out: &mut Vec<Draw>) {
        if let Some(gone) = &self.triggers.ejecting {
            let t = gone.started.elapsed().as_secs_f32() / EJECT_S;
            let (x, y, w, h, alpha) = eject_at(gone.rect, t);
            if let Some(tex) = gone.face {
                out.push(Draw::Tex { x, y, w, h, tex, alpha });
            }
            return;
        }
        let (Some(_), Some((ring, _))) = (self.triggers.hold, self.triggers.ring) else { return };
        let Some((x, y, w, h)) = self.shelves.get(self.active).and_then(|s| s.selected_rect()) else { return };
        let side = slot_ui::HOLD_RING as f32;
        let (cx, cy) = (x + w / 2.0, y + h * 0.55);
        out.push(Draw::Tex { x: cx - side / 2.0, y: cy - side / 2.0, w: side, h: side, tex: ring, alpha: 1.0 });
        if let Some((tex, tw, th)) = self.triggers.text {
            let s = 0.62;
            let (dw, dh) = (tw as f32 * s, th as f32 * s);
            let (tx, ty) = (cx - dw / 2.0, y + h + 6.0);
            out.push(Draw::Rect { x: tx - 8.0, y: ty - 3.0, w: dw + 16.0, h: dh + 6.0, colour: [0.0, 0.0, 0.0, 0.72] });
            out.push(Draw::Tex { x: tx, y: ty, w: dw, h: dh, tex, alpha: 1.0 });
        }
    }

    /// The stem hidden from the row while its cart leaves.
    pub(super) fn ejecting_stem(&self) -> Option<&str> {
        self.triggers.ejecting.as_ref().filter(|e| e.rack == self.active).map(|e| e.stem.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cart(code: &str) -> slot_store::Cart {
        slot_store::Cart {
            platform: Platform::Gba, stem: "x".into(), title: "x".into(), code: code.into(),
            rom: PathBuf::from("content://x"), label: None, shell: None, battery: false,
        }
    }

    #[test]
    fn gba_video_carts_are_known_by_their_code() {
        assert!(is_gba_video(&cart("MSKE")));
        assert!(!is_gba_video(&cart("AXVE")));
        assert!(!is_gba_video(&cart("")));
        let mut gb = cart("MSKE");
        gb.platform = Platform::Gb;
        assert!(!is_gba_video(&gb));
    }

    #[test]
    fn the_cart_jumps_then_shrinks_away() {
        let rect = (100.0, 200.0, 120.0, 80.0);
        let (x, y, w, h, a) = eject_at(rect, 0.0);
        assert_eq!((x, y, w, h, a), (100.0, 200.0, 120.0, 80.0, 1.0));
        let (_, y, w, _, a) = eject_at(rect, 0.25);
        assert!(y < 200.0 - 20.0 && w == 120.0 && a == 1.0);
        let (x, _, w, _, a) = eject_at(rect, 1.0);
        assert!(w < 5.0 && a <= 0.0);
        assert!((x + w / 2.0 - 160.0).abs() < 0.01, "it stays centred");
    }
}
