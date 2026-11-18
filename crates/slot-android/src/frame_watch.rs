//! What takes time on the shelf, measured on the GL thread (2.5.0-beta37):
//! slow frames with what was going on, and each cart's insertion and
//! ejection, step by step. Nothing is written per frame: a few counters,
//! and a line in Android's log only when something was slow, or once per
//! insertion and ejection. Games themselves are timed elsewhere (Kotlin's
//! GL callback pacing).

use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

/// Work a frame did, for the slow frame's line.
pub(crate) const WORK_LABEL: u8 = 1;
pub(crate) const WORK_WALLPAPER: u8 = 2;
pub(crate) const WORK_CORE: u8 = 4;
pub(crate) const WORK_LIBRARY: u8 = 8;

static WORK: AtomicU8 = AtomicU8::new(0);

/// This frame did [work] (WORK_*).
pub(crate) fn did(work: u8) {
    WORK.fetch_or(work, Ordering::Relaxed);
}

/// What the shelf was doing during a frame.
#[derive(Clone, Copy, Default)]
pub(crate) struct Moment {
    pub playing: bool,
    pub scrolling: bool,
    pub menu: bool,
    pub inserting: bool,
    pub ejecting: bool,
}

/// A frame slower than this, at 60 Hz, is a frame shown twice.
const SLOW_MS: f32 = 17.0;
const VERY_SLOW_MS: f32 = 33.0;
const WINDOW: Duration = Duration::from_secs(5);
const CAUSES: [&str; 9] = ["scroll", "label", "wallpaper", "menu", "insert", "eject",
    "library", "core", "other"];

#[derive(Default)]
struct Window {
    started: Option<Instant>,
    frames: u32,
    slow: u32,
    very_slow: u32,
    max_ms: f32,
    causes: [u32; 9],
}

impl Window {
    fn line(&self, why: &str) -> String {
        let causes = CAUSES.iter().zip(self.causes)
            .filter(|(_, n)| *n > 0)
            .map(|(name, n)| format!("{name}={n}"))
            .collect::<Vec<_>>().join(" ");
        format!("Shelf frames ({why}): {} drawn, {} over {SLOW_MS:.0}ms, {} over {VERY_SLOW_MS:.0}ms, \
            longest {:.1}ms; during: {}",
            self.frames, self.slow, self.very_slow, self.max_ms,
            if causes.is_empty() { "-".into() } else { causes })
    }
}

static SHELF: Mutex<Window> = Mutex::new(Window {
    started: None, frames: 0, slow: 0, very_slow: 0, max_ms: 0.0, causes: [0; 9],
});

/// One cart's way in or out.
struct Cart {
    eject: bool,
    started: Instant,
    sound_ms: Option<u128>,
    rom_ms: Option<u128>,
    core_ms: Option<u128>,
    save_ms: Option<u128>,
    longest_ms: f32,
}

static CART: Mutex<Option<Cart>> = Mutex::new(None);

fn cart() -> std::sync::MutexGuard<'static, Option<Cart>> {
    CART.lock().unwrap_or_else(|e| e.into_inner())
}

fn new_cart(eject: bool, save_ms: Option<u128>) -> Cart {
    Cart { eject, started: Instant::now(), sound_ms: None, rom_ms: None, core_ms: None,
        save_ms, longest_ms: 0.0 }
}

/// A pressed: the cart starts going in.
pub(crate) fn insert_started() {
    *cart() = Some(new_cart(false, None));
}

/// The game was saved ([save] long) and the cart starts coming out.
pub(crate) fn eject_started(save: Option<Duration>) {
    *cart() = Some(new_cart(true, save.map(|d| d.as_millis())));
}

/// The cart's click was handed to Android.
pub(crate) fn sound() {
    if let Some(c) = cart().as_mut() { c.sound_ms.get_or_insert(c.started.elapsed().as_millis()); }
}

/// Android prepared the ROM and its saves (MainActivity.loadGameFromSaf).
pub(crate) fn rom_ready() {
    if let Some(c) = cart().as_mut() { c.rom_ms.get_or_insert(c.started.elapsed().as_millis()); }
}

/// The core started, its save and state restored, in [took].
pub(crate) fn core_prepared(took: Duration) {
    if let Some(c) = cart().as_mut() { c.core_ms = Some(took.as_millis()); }
}

/// The game's screen goes on: the insertion's line.
pub(crate) fn insert_done() {
    let Some(c) = cart().take() else { return };
    if c.eject { return; }
    let ms = |v: Option<u128>| v.map_or("-".to_string(), |v| format!("+{v}ms"));
    crate::game::log_launch_timing(&format!(
        "Insert timeline: click {}, ROM and saves ready {}, core {}, screen on +{}ms \
         (original Slot: 730ms), longest frame {:.0}ms",
        ms(c.sound_ms), ms(c.rom_ms), c.core_ms.map_or("-".to_string(), |v| format!("{v}ms")),
        c.started.elapsed().as_millis(), c.longest_ms));
}

/// The cart is all the way out: the ejection's line. An insertion cancelled
/// with B has none.
pub(crate) fn cart_out() {
    let Some(c) = cart().take() else { return };
    if !c.eject { return; }
    crate::game::log_launch_timing(&format!(
        "Eject timeline: save {}, click {}, cart out +{}ms, longest frame {:.0}ms",
        c.save_ms.map_or("-".to_string(), |v| format!("{v}ms")),
        c.sound_ms.map_or("-".to_string(), |v| format!("+{v}ms")),
        c.started.elapsed().as_millis(), c.longest_ms));
}

/// B before the game started, or a launch that failed.
pub(crate) fn cancel() {
    let mut c = cart();
    if c.as_ref().is_some_and(|c| !c.eject) { *c = None; }
}

/// After each nativeDrawFrame: [took] it, while the shelf did [moment].
pub(crate) fn frame(took: Duration, moment: Moment) {
    let work = WORK.swap(0, Ordering::Relaxed);
    let ms = took.as_secs_f32() * 1000.0;
    if let Some(c) = cart().as_mut() { c.longest_ms = c.longest_ms.max(ms); }
    if moment.playing && !moment.ejecting { return; }
    let mut window = SHELF.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    let started = *window.started.get_or_insert(now);
    window.frames += 1;
    window.max_ms = window.max_ms.max(ms);
    if ms > SLOW_MS {
        window.slow += 1;
        if ms > VERY_SLOW_MS { window.very_slow += 1; }
        let mut any = false;
        let mut count = |i: usize, yes: bool| if yes { window.causes[i] += 1; any = true; };
        count(0, moment.scrolling);
        count(1, work & WORK_LABEL != 0);
        count(2, work & WORK_WALLPAPER != 0);
        count(3, moment.menu);
        count(4, moment.inserting);
        count(5, moment.ejecting);
        count(6, work & WORK_LIBRARY != 0);
        count(7, work & WORK_CORE != 0);
        if !any { window.causes[8] += 1; }
    }
    if now.duration_since(started) >= WINDOW {
        if window.slow > 0 { crate::game::log_launch_timing(&window.line("5 s")); }
        *window = Window { started: Some(now), ..Window::default() };
    }
}

/// The bench's end: the frames since the last line, slow or not.
pub(crate) fn flush(why: &str) -> String {
    let mut window = SHELF.lock().unwrap_or_else(|e| e.into_inner());
    let line = window.line(why);
    crate::game::log_launch_timing(&line);
    *window = Window::default();
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slow_frame_says_what_was_going_on() {
        let line = {
            let mut w = Window::default();
            w.frames = 300; w.slow = 2; w.very_slow = 1; w.max_ms = 41.5;
            w.causes[0] = 1; w.causes[1] = 2;
            w.line("bench")
        };
        assert_eq!(line, "Shelf frames (bench): 300 drawn, 2 over 17ms, 1 over 33ms, longest 41.5ms; \
            during: scroll=1 label=2");
    }
}
