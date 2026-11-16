//! Touch on the carousel (Siham 2026-11-16).
//!
//! A finger drags the carts exactly under it; let go quickly and they coast
//! on, slow down and settle on a cart with the D-pad's own spring. A tap on
//! the cart in the middle inserts it, as A does, and so does pulling it
//! down; a tap on a cart beside it brings that one to the middle. Touch only works on the carousel itself:
//! never in a game, a menu, or with the RG Rotate closed.
use super::*;

/// How far, in canvas pixels (the canvas is 480 high), a finger may move
/// and still tap.
const TAP_SLOP: f32 = 10.0;
/// A finger pulled down this far (canvas pixels) from the middle cart
/// pushes it into the console (ljn35XX's idea, Siham 2026-11-16).
const PUSH_DOWN: f32 = 40.0;
/// A touch held longer than this is not a tap (kept for a long press).
const TAP_MS: i64 = 600;
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
    /// The finger stopped a moving shelf: letting go does not insert.
    caught: bool,
    /// The last positions: (time in ms, x), oldest first.
    samples: VecDeque<(i64, f32)>,
}

impl Touch {
    fn new(x: f32, y: f32, ms: i64, caught: bool) -> Self {
        let mut samples = VecDeque::with_capacity(SAMPLES);
        samples.push_back((ms, x));
        Touch { start: (x, y), start_ms: ms, last_x: x, dragging: false, pulling: false, caught, samples }
    }

    fn sample(&mut self, ms: i64, x: f32) {
        if self.samples.len() == SAMPLES { self.samples.pop_front(); }
        self.samples.push_back((ms, x));
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

impl Engine {
    /// The carousel alone is on screen, waiting for a cart.
    fn touch_ready(&self) -> bool {
        self.game.is_none()
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
        if action == DOWN {
            self.touch = None;
            if !self.touch_ready() { return; }
            let shelf = &mut self.shelves[self.active];
            let caught = shelf.is_scrolling();
            shelf.grab();
            self.touch = Some(Touch::new(x, y, ms, caught));
            return;
        }
        let Some(mut touch) = self.touch.take() else { return };
        if !self.touch_ready() {
            // Something else took the screen: the shelf settles where it is.
            self.shelves[self.active].let_go(0.0);
            return;
        }
        match action {
            MOVE => {
                touch.sample(ms, x);
                let (dx, dy) = (x - touch.start.0, y - touch.start.1);
                if !touch.dragging && !touch.pulling && dx.hypot(dy) > TAP_SLOP {
                    if dy > dx.abs() {
                        // Down, not sideways: maybe a push into the console.
                        touch.pulling = true;
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
