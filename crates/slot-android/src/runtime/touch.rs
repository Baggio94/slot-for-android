//! Touch on the carousel (Siham 2026-11-16).
//!
//! A finger drags the carts exactly under it; let go quickly and they coast
//! on, slow down and settle on a cart with the D-pad's own spring. A tap on
//! the cart in the middle inserts it, as A does, and so does pulling it
//! down; a tap on a cart beside it brings that one to the middle. Touch only works on the carousel itself:
//! never in a menu or with the RG Rotate closed, and in a game only on
//! PokeDaisy, which reads it as its own app's touchscreen.
//!
//! Pushing the middle cart up opens its core picker, where a tap on a core
//! chooses it and a tap anywhere else closes it; holding a finger still on a
//! cart lifts it and opens Cart Studio (Siham 2026-11-17).
use super::*;

/// How far, in canvas pixels (the canvas is 480 high), a finger may move
/// and still tap.
const TAP_SLOP: f32 = 10.0;
/// A finger pulled down this far (canvas pixels) from the middle cart
/// pushes it into the console (ljn35XX's idea, Siham 2026-11-16).
const PUSH_DOWN: f32 = 40.0;
/// A touch held longer than this is not a tap (kept for a long press).
const TAP_MS: i64 = 600;
/// A finger held still this long on a cart opens Cart Studio.
const LONG_PRESS_S: f32 = 0.5;
/// The held cart starts rising after this (a tap never shows it).
const LIFT_FROM_S: f32 = 0.18;
/// How much the held cart grows and rises (canvas pixels).
const LIFT_GROW: f32 = 0.04;
const LIFT_RISE: f32 = 6.0;
/// After a chosen core's chip lands, a beat before it's kept (ms).
const SEAT_PAUSE_MS: u64 = 80;
/// The finger's speed is measured over its last this many milliseconds.
const SPEED_WINDOW_MS: i64 = 80;
/// A finger that rested this long before letting go has no speed left.
const STILL_MS: i64 = 60;
const SAMPLES: usize = 16;

/// Android's touch actions, as MainActivity sends them.
pub(crate) const DOWN: u8 = 0;
pub(crate) const MOVE: u8 = 1;
pub(crate) const UP: u8 = 2;
pub(crate) const CANCEL: u8 = 3;

/// One finger on the carousel.
pub(super) struct Touch {
    start: (f32, f32),
    start_ms: i64,
    last_x: f32,
    dragging: bool,
    /// The finger went mostly down, not sideways: no shelf drag.
    pulling: bool,
    /// Mostly up: maybe the middle cart's core picker.
    pushing: bool,
    /// On the open core picker, not the shelf.
    picker: bool,
    /// The long press already opened Cart Studio.
    long_fired: bool,
    at: Instant,
    /// The finger stopped a moving shelf: letting go does not insert.
    caught: bool,
    /// The last positions: (time in ms, x), oldest first.
    samples: VecDeque<(i64, f32)>,
}

impl Touch {
    fn new(x: f32, y: f32, ms: i64, caught: bool) -> Self {
        let mut samples = VecDeque::with_capacity(SAMPLES);
        samples.push_back((ms, x));
        Touch { start: (x, y), start_ms: ms, last_x: x, dragging: false, pulling: false, pushing: false,
            picker: false, long_fired: false, at: Instant::now(), caught, samples }
    }

    fn sample(&mut self, ms: i64, x: f32) {
        if self.samples.len() == SAMPLES { self.samples.pop_front(); }
        self.samples.push_back((ms, x));
    }

    /// Still a candidate for a long press: not moved, not caught.
    fn still(&self) -> bool {
        !self.dragging && !self.pulling && !self.pushing && !self.caught && !self.picker && !self.long_fired
    }

    /// Canvas pixels a second, from the last samples.
    fn speed(&self, now: i64) -> f32 {
        speed_of(&self.samples, now)
    }
}

/// The finger's speed at `now` over its last SPEED_WINDOW_MS, 0 if it
/// rested before letting go.
pub(super) fn speed_of(samples: &VecDeque<(i64, f32)>, now: i64) -> f32 {
    let Some(&(last_ms, last_x)) = samples.back() else { return 0.0 };
    if now - last_ms > STILL_MS { return 0.0; }
    let first = samples.iter()
        .find(|(ms, _)| last_ms - ms <= SPEED_WINDOW_MS)
        .copied()
        .unwrap_or((last_ms, last_x));
    let dt = (last_ms - first.0) as f32 / 1000.0;
    if dt <= 0.0 { 0.0 } else { (last_x - first.1) / dt }
}

/// 0 to 1 across the picture as libretro's pointer reads it: -0x7fff to
/// 0x7fff, the edge past it.
pub(super) fn pointer_axis(at: f32) -> i16 {
    ((at * 2.0 - 1.0).clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// A cart's (x, y, w, h) lifted by `lift` (0 to 1) of a long press.
fn lifted_by([x, y, w, h]: [f32; 4], lift: f32) -> [f32; 4] {
    let grow = 1.0 + LIFT_GROW * lift;
    [x - w * (grow - 1.0) / 2.0, y - LIFT_RISE * lift - h * (grow - 1.0) / 2.0, w * grow, h * grow]
}

/// As high as a long press lifts it: where X has a cart take off from.
pub(super) fn lifted_fully(rect: [f32; 4]) -> [f32; 4] {
    lifted_by(rect, 1.0)
}

impl Engine {
    /// The carousel alone is on screen, waiting for a cart.
    fn touch_ready(&self) -> bool {
        self.settings.touchscreen
            && self.game.is_none()
            && !self.inserted
            && self.progress <= 0.0
            && matches!(self.overlay, ShelfOverlay::None)
            && self.picker.is_none()
            && self.controls_listen.is_none()
            && !self.lid_covered
            && self.lid_t <= 0.0
            && self.triggers.hold.is_none()
            && self.triggers.ejecting.is_none()
            && self.shelves.get(self.active).is_some_and(|s| !s.carts.is_empty())
    }

    /// `x` and `y` are 0 to 1 across the screen; `ms` is Android's event
    /// time.
    pub(super) fn touch(&mut self, action: u8, x: f32, y: f32, ms: i64) {
        let (x, y) = (x * out_w() as f32, y * OUT_H as f32);
        if self.game.is_some() {
            self.game_touch(action, x, y);
            return;
        }
        if matches!(self.overlay, ShelfOverlay::Shelves { .. }) {
            if self.settings.touchscreen { self.shelves_touch(action, x, y, ms); }
            return;
        }
        if action == DOWN {
            self.touch = None;
            if self.picker_touchable() {
                let mut touch = Touch::new(x, y, ms, false);
                touch.picker = true;
                self.touch = Some(touch);
                return;
            }
            if !self.touch_ready() { return; }
            let shelf = &mut self.shelves[self.active];
            let caught = shelf.is_scrolling();
            shelf.grab();
            self.touch = Some(Touch::new(x, y, ms, caught));
            return;
        }
        let Some(mut touch) = self.touch.take() else { return };
        if touch.picker {
            if action == MOVE {
                self.touch = Some(touch);
            } else if action == UP {
                let (dx, dy) = (x - touch.start.0, y - touch.start.1);
                if dx.hypot(dy) <= TAP_SLOP && ms - touch.start_ms <= TAP_MS && self.picker_touchable() {
                    self.picker_tap(touch.start.0, touch.start.1);
                }
            }
            return;
        }
        if touch.long_fired {
            // Cart Studio is opening: the finger's lifting is not a tap.
            return;
        }
        if !self.touch_ready() {
            // Something else took the screen: the shelf settles where it is.
            self.shelves[self.active].let_go(0.0);
            return;
        }
        match action {
            MOVE => {
                touch.sample(ms, x);
                let (dx, dy) = (x - touch.start.0, y - touch.start.1);
                if !touch.dragging && !touch.pulling && !touch.pushing && dx.hypot(dy) > TAP_SLOP {
                    if dy > dx.abs() {
                        // Down, not sideways: maybe a push into the console.
                        touch.pulling = true;
                    } else if -dy > dx.abs() {
                        // Up: maybe the core picker.
                        touch.pushing = true;
                    } else {
                        touch.dragging = true;
                        // The carts catch up with the finger from where it
                        // set down, so they stay exactly under it.
                        touch.last_x = touch.start.0;
                    }
                }
                if touch.dragging {
                    let dx = x - touch.last_x;
                    touch.last_x = x;
                    self.drag_shelf(dx);
                }
                self.touch = Some(touch);
            }
            UP => {
                touch.sample(ms, x);
                if touch.dragging {
                    let speed = touch.speed(ms);
                    self.shelves[self.active].let_go(speed);
                    self.last_cart_navigation = Instant::now();
                } else if touch.pushing {
                    self.shelves[self.active].let_go(0.0);
                    let (dx, dy) = (x - touch.start.0, y - touch.start.1);
                    if !touch.caught && -dy >= PUSH_DOWN && dx.abs() < -dy
                        && self.shelves[self.active].cart_under(touch.start.0, touch.start.1) == Some(0) {
                        self.open_core_picker();
                    }
                } else if touch.pulling {
                    self.shelves[self.active].let_go(0.0);
                    let (dx, dy) = (x - touch.start.0, y - touch.start.1);
                    if !touch.caught && dy >= PUSH_DOWN && dx.abs() < dy
                        && self.shelves[self.active].cart_under(touch.start.0, touch.start.1) == Some(0) {
                        self.insert_by_touch();
                    }
                } else {
                    self.shelves[self.active].let_go(0.0);
                    if !touch.caught && ms - touch.start_ms <= TAP_MS {
                        self.tap_shelf(touch.start.0, touch.start.1);
                    }
                }
            }
            _ => self.shelves[self.active].let_go(0.0),
        }
    }

    /// How far the held cart has risen, 0 to 1 (0 with no long press).
    pub(super) fn press_lift(&self) -> f32 {
        let Some(touch) = self.touch.as_ref().filter(|t| t.still()) else { return 0.0 };
        if self.shelves.get(self.active).and_then(|s| s.cart_under(touch.start.0, touch.start.1)) != Some(0) {
            return 0.0;
        }
        let t = ((touch.at.elapsed().as_secs_f32() - LIFT_FROM_S) / (LONG_PRESS_S - LIFT_FROM_S)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// The selected cart's (x, y, w, h), lifted as far as the press has.
    pub(super) fn lifted(&self, rect: [f32; 4]) -> [f32; 4] {
        lifted_by(rect, self.press_lift())
    }

    /// Per frame: a finger held still long enough opens Cart Studio (a side
    /// cart comes to the middle first).
    pub(super) fn touch_tick(&mut self) {
        if self.studio_when_settled {
            if !self.touch_ready() {
                self.studio_when_settled = false;
            } else if !self.shelves[self.active].is_scrolling() {
                self.studio_when_settled = false;
                self.open_cart_studio();
            }
        }
        let Some(touch) = self.touch.as_ref() else { return };
        if !touch.still() || touch.at.elapsed().as_secs_f32() < LONG_PRESS_S || !self.touch_ready() { return; }
        let (x, y) = touch.start;
        let under = self.shelves[self.active].cart_under(x, y);
        let Some(by) = under else { return };
        self.shelves[self.active].let_go(0.0);
        if by == 0 {
            // Opened while still lifted, so the flight starts where it is.
            self.open_cart_studio();
            if let Some(touch) = self.touch.as_mut() { touch.long_fired = true; }
        } else {
            if let Some(touch) = self.touch.as_mut() { touch.long_fired = true; }
            self.shelves[self.active].go_by(by);
            self.last_cart_navigation = Instant::now();
            self.studio_when_settled = true;
        }
    }

    /// The core picker is open and settled: a tap chooses or closes it.
    fn picker_touchable(&self) -> bool {
        let now = self.born.elapsed().as_millis() as u64;
        self.settings.touchscreen && self.game.is_none() && !self.lid_covered && self.lid_t <= 0.0
            && matches!(self.overlay, ShelfOverlay::Core)
            && self.picker.is_some_and(|p| !p.closing() && !p.waiting() && p.openness(now) >= 1.0)
            && self.picker_auto_seat.is_none()
    }

    /// In a game: PokeDaisy reads the finger on its picture (its HUD, its
    /// companion's tabs and buttons), as the touchscreen of its own app.
    /// Nothing else in a game takes touch; a menu over it lets go.
    fn game_touch(&mut self, action: u8, x: f32, y: f32) {
        let playing = self.settings.touchscreen && self.pokedaisy_running()
            && matches!(self.overlay, ShelfOverlay::None)
            && !self.lid_covered && self.lid_t <= 0.0;
        let Some(session) = self.game.as_mut() else { return };
        if session.core_kind() != Core::PokeDaisy { return; }
        if !playing {
            session.set_pointer(0, 0, false);
            return;
        }
        // Where companion_fit shows the canvas: as large as it fits, centred.
        let (fw, fh) = session.frame_size();
        let (cw, ch) = (out_w() as f32, OUT_H as f32);
        let a = fw.max(1) as f32 / fh.max(1) as f32;
        let (w, h) = if ch * a <= cw { (ch * a, ch) } else { (cw, cw / a) };
        let (left, top) = ((cw - w) / 2.0, (ch - h) / 2.0);
        let (px, py) = (pointer_axis((x - left) / w), pointer_axis((y - top) / h));
        session.set_pointer(px, py, matches!(action, DOWN | MOVE));
    }

    /// A tap on the open core picker: on one of the board's two halves, that
    /// core (kept once its chip has hopped there; the one already in place
    /// closes it); anywhere else, closes it as B.
    fn picker_tap(&mut self, x: f32, y: f32) {
        let now = self.born.elapsed().as_millis() as u64;
        let Some(picker) = self.picker else { return };
        let shelf = &self.shelves[self.active];
        let Some(cart) = shelf.carts.get(shelf.index) else { return };
        let layout = slot_ui::board_layout_body(slot_ui::body_of(cart));
        let board = layout.rest;
        let on_board = (board.x..=board.x + board.w).contains(&x) && (board.y..=board.y + board.h).contains(&y);
        // The core whose chip is nearer the finger (beta41: the board's
        // halves sent a tap on the GBA's left chip to the right one), for
        // every board, from where its chips are drawn.
        // A Pokemon game's board has PokeDaisy's chip too: three places.
        let zoom = layout.zoom(board);
        let places: Vec<((f32, f32), (f32, f32))> = match self.picker_three {
            Some(three) => three.chip.iter().map(|at| (*at, three.size)).collect(),
            None => (0..2).map(|i| ((layout.chip_u[i], layout.chip_v), layout.chip)).collect(),
        };
        let distance = |((u, v), (w, h)): ((f32, f32), (f32, f32))| {
            let (cx, cy) = layout.at(board, u, v);
            (x - (cx + w * zoom / 2.0)).hypot(y - (cy + h * zoom / 2.0))
        };
        let nearest = (0..places.len())
            .min_by(|a, b| distance(places[*a]).total_cmp(&distance(places[*b])))
            .unwrap_or(0);
        let core = picker.cores().get(nearest).copied().unwrap_or(picker.left());
        let Some(picker) = self.picker.as_mut() else { return };
        if !on_board || core == picker.seat() {
            picker.press(Press::Back, now);
            return;
        }
        picker.press(Press::To(nearest), now);
        self.picker_auto_seat = Some(now + crate::core_picker::HOP_MS + SEAT_PAUSE_MS);
    }

    /// Exactly A: pressed and let go at once (no fresh start).
    fn insert_by_touch(&mut self) {
        self.handle(Input::Key(96, true));
        self.handle(Input::Key(96, false));
    }

    fn drag_shelf(&mut self, dx: f32) {
        self.shelves[self.active].drag_by(dx);
        // Keeps the carousel drawing every frame (not dozing) meanwhile.
        self.last_cart_navigation = Instant::now();
    }

    fn tap_shelf(&mut self, x: f32, y: f32) {
        match self.shelves[self.active].cart_under(x, y) {
            Some(0) => self.insert_by_touch(),
            Some(by) => {
                self.shelves[self.active].go_by(by);
                self.last_cart_navigation = Instant::now();
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(points: &[(i64, f32)]) -> VecDeque<(i64, f32)> {
        points.iter().copied().collect()
    }

    #[test]
    fn the_pointer_spans_the_picture() {
        assert_eq!(pointer_axis(0.0), -32767);
        assert_eq!(pointer_axis(0.5), 0);
        assert_eq!(pointer_axis(1.0), 32767);
        assert_eq!(pointer_axis(1.4), 32767);
    }

    #[test]
    fn a_quick_flick_has_its_speed() {
        let s = samples(&[(0, 0.0), (16, 20.0), (32, 40.0), (48, 60.0)]);
        let v = speed_of(&s, 48);
        assert!((v - 1250.0).abs() < 1.0, "{v}");
    }

    #[test]
    fn a_finger_that_rested_has_no_speed() {
        let s = samples(&[(0, 0.0), (16, 20.0), (32, 40.0)]);
        assert_eq!(speed_of(&s, 200), 0.0);
    }

    #[test]
    fn only_the_last_moments_count() {
        // Slow for a while, then fast: the fast end is the fling.
        let s = samples(&[(0, 0.0), (100, 5.0), (200, 10.0), (216, 30.0), (232, 50.0)]);
        let v = speed_of(&s, 232);
        assert!(v > 1000.0, "{v}");
    }
}
