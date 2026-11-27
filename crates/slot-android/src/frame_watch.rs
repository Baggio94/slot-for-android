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

/// Parts of a shelf frame timed on their own (beta38), to say what a slow
/// frame spent its time on: the rest is building the frame's commands.
pub(crate) const PART_WALLPAPER: usize = 0;
pub(crate) const PART_UPLOAD: usize = 1;
pub(crate) const PART_GPU: usize = 2;
const PART_NAMES: [&str; 4] = ["wallpaper", "upload", "gpu", "build"];
static PARTS: Mutex<[u32; 3]> = Mutex::new([0; 3]);

/// Pieces of the build part timed on their own (beta41), for the line a
/// very slow frame writes: the menus' preparing, the selected title, the
/// last selection written to disk, the shelf's own tick, the overlay's
/// commands.
pub(crate) const SPAN_MENUS: usize = 0;
pub(crate) const SPAN_TITLE: usize = 1;
pub(crate) const SPAN_FOCUS: usize = 2;
pub(crate) const SPAN_TICK: usize = 3;
pub(crate) const SPAN_OVERLAY: usize = 4;
pub(crate) const SPAN_STATE: usize = 5;
pub(crate) const SPAN_SHELF: usize = 6;
pub(crate) const SPAN_CHROME: usize = 7;
pub(crate) const SPAN_BEFORE: usize = 8;
/// The menus' span, by page (beta43: a first opening still took 51 ms).
pub(crate) const SPAN_MENU_CONTROLS: usize = 9;
pub(crate) const SPAN_MENU_LOOK: usize = 10;
pub(crate) const SPAN_MENU_STUDIO: usize = 11;
pub(crate) const SPAN_MENU_LIBRARY: usize = 12;
pub(crate) const SPAN_MENU_ROMM: usize = 13;
const SPANS_N: usize = 14;
const SPAN_NAMES: [&str; SPANS_N] = ["menus", "title", "focus", "tick", "overlay", "state", "shelf", "chrome",
    "before", "controls", "look", "studio", "library", "romm"];
static SPANS: Mutex<[u32; SPANS_N]> = Mutex::new([0; SPANS_N]);

/// A frame slower than this writes its own line, with every part (bits of
/// an f32, so the bench can lower it).
static HITCH_MS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x4248_0000);

#[cfg(test)]
pub(crate) fn set_hitch_ms(ms: f32) { HITCH_MS.store(ms.to_bits(), Ordering::Relaxed); }

/// [span] of the build part took [took] this frame.
pub(crate) fn span(span: usize, took: Duration) {
    let mut spans = SPANS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(slot) = spans.get_mut(span) {
        *slot = slot.saturating_add(took.as_micros().min(u128::from(u32::MAX)) as u32);
    }
}

/// The line of one very slow frame: every part and span, in ms.
fn hitch_line(ms: f32, parts: &[u32; 3], spans: &[u32; SPANS_N], moment: Moment, work: u8) -> String {
    let at = |us: u32| format!("{:.1}", us as f32 / 1000.0);
    let total_us = (ms * 1000.0) as u32;
    let build = total_us.saturating_sub(parts.iter().sum::<u32>());
    let spans = SPAN_NAMES.iter().zip(spans)
        .filter(|(_, us)| **us >= 500)
        .map(|(name, us)| format!("{name} {}", at(*us)))
        .collect::<Vec<_>>().join(", ");
    let mut during = Vec::new();
    if moment.scrolling { during.push("scroll"); }
    if moment.menu { during.push("menu"); }
    if moment.inserting { during.push("insert"); }
    if moment.ejecting { during.push("eject"); }
    if work & WORK_LABEL != 0 { during.push("label"); }
    if work & WORK_WALLPAPER != 0 { during.push("wallpaper"); }
    if work & WORK_LIBRARY != 0 { during.push("library"); }
    if work & WORK_CORE != 0 { during.push("core"); }
    format!("Slow frame {ms:.0}ms: wallpaper {}, upload {}, gpu {}, build {}{}; during: {}",
        at(parts[0]), at(parts[1]), at(parts[2]), at(build),
        if spans.is_empty() { String::new() } else { format!(" ({spans})") },
        if during.is_empty() { "-".into() } else { during.join(" ") })
}

/// The cold renderer's steps, timed once at start (beta41): one line.
pub(crate) struct Steps { last: Instant, done: Vec<(&'static str, u128)> }

impl Steps {
    pub(crate) fn new() -> Self { Self { last: Instant::now(), done: Vec::new() } }
    /// The step called [name] ends here.
    pub(crate) fn mark(&mut self, name: &'static str) {
        let now = Instant::now();
        self.done.push((name, now.duration_since(self.last).as_millis()));
        self.last = now;
    }
    pub(crate) fn line(&self) -> String {
        let steps = self.done.iter().map(|(name, ms)| format!("{name} {ms}")).collect::<Vec<_>>();
        format!("Cold renderer steps (ms): {}", steps.join(", "))
    }
}

/// [part] took [took] this frame (microseconds kept).
pub(crate) fn part(part: usize, took: Duration) {
    let mut parts = PARTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(slot) = parts.get_mut(part) {
        *slot = slot.saturating_add(took.as_micros().min(u128::from(u32::MAX)) as u32);
    }
}

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
    /// Slow frames by the part that took most of them.
    parts: [u32; 4],
}

impl Window {
    fn line(&self, why: &str) -> String {
        let causes = CAUSES.iter().zip(self.causes)
            .filter(|(_, n)| *n > 0)
            .map(|(name, n)| format!("{name}={n}"))
            .collect::<Vec<_>>().join(" ");
        let parts = PART_NAMES.iter().zip(self.parts)
            .filter(|(_, n)| *n > 0)
            .map(|(name, n)| format!("{name}={n}"))
            .collect::<Vec<_>>().join(" ");
        format!("Shelf frames ({why}): {} drawn, {} over {SLOW_MS:.0}ms, {} over {VERY_SLOW_MS:.0}ms, \
            longest {:.1}ms; during: {}{}",
            self.frames, self.slow, self.very_slow, self.max_ms,
            if causes.is_empty() { "-".into() } else { causes },
            if parts.is_empty() { String::new() } else { format!("; slowest part: {parts}") })
    }
}

static SHELF: Mutex<Window> = Mutex::new(Window {
    started: None, frames: 0, slow: 0, very_slow: 0, max_ms: 0.0, causes: [0; 9], parts: [0; 4],
});

/// One cart's way in or out.
struct Cart {
    eject: bool,
    started: Instant,
    sound_ms: Option<u128>,
    /// Android took the launch request (3.2 perf).
    handed_ms: Option<u128>,
    /// The core started opening (the cart seated and the ROM ready).
    core_at_ms: Option<u128>,
    /// What the ejection's save did, part by part (3.2 perf).
    detail: String,
    rom_ms: Option<u128>,
    core_ms: Option<u128>,
    save_ms: Option<u128>,
    longest_ms: f32,
    /// Time the player spent in a window before the game (DELETED, TWO
    /// SAVES, ROMM SAVE...): not slot.'s, left out of the line (beta39).
    asked_ms: u128,
}

static CART: Mutex<Option<Cart>> = Mutex::new(None);

fn cart() -> std::sync::MutexGuard<'static, Option<Cart>> {
    CART.lock().unwrap_or_else(|e| e.into_inner())
}

fn new_cart(eject: bool, save_ms: Option<u128>) -> Cart {
    Cart { eject, started: Instant::now(), sound_ms: None, handed_ms: None, core_at_ms: None, detail: String::new(),
        rom_ms: None, core_ms: None,
        save_ms, longest_ms: 0.0, asked_ms: 0 }
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

/// The player answered a window before the game, after [ms].
pub(crate) fn asked(ms: u128) {
    if let Some(c) = cart().as_mut() { if !c.eject { c.asked_ms += ms; } }
}

/// Android took the launch request (MainActivity.nativePollLaunchUri).
pub(crate) fn handed() {
    if let Some(c) = cart().as_mut() { if !c.eject { c.handed_ms.get_or_insert(c.started.elapsed().as_millis()); } }
}

/// The ejection's save and the core's closing, part by part.
pub(crate) fn eject_detail(text: String) {
    if let Some(c) = cart().as_mut() { if c.eject { c.detail = text; } }
}

/// The core starts opening.
pub(crate) fn core_started() {
    if let Some(c) = cart().as_mut() { if !c.eject { c.core_at_ms.get_or_insert(c.started.elapsed().as_millis()); } }
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
    let asked = c.asked_ms;
    let window = if asked > 0 { format!(", a window answered in {asked}ms (left out)") } else { String::new() };
    crate::game::log_launch_timing(&format!(
        "Insert timeline: Android took it {}, click {}, ROM and saves ready {}, core from {} for {}, \
         screen on +{}ms (original Slot: 730ms), longest frame {:.0}ms{window}",
        ms(c.handed_ms), ms(c.sound_ms), ms(c.rom_ms.map(|v| v.saturating_sub(asked))),
        ms(c.core_at_ms.map(|v| v.saturating_sub(asked))),
        c.core_ms.map_or("-".to_string(), |v| format!("{v}ms")),
        c.started.elapsed().as_millis().saturating_sub(asked), c.longest_ms));
}

/// The cart is all the way out: the ejection's line. An insertion cancelled
/// with B has none.
pub(crate) fn cart_out() {
    let Some(c) = cart().take() else { return };
    if !c.eject { return; }
    crate::game::log_launch_timing(&format!(
        "Eject timeline: save {}{}, click {}, cart out +{}ms, longest frame {:.0}ms",
        c.save_ms.map_or("-".to_string(), |v| format!("{v}ms")),
        if c.detail.is_empty() { String::new() } else { format!(" ({})", c.detail) },
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
    let parts = std::mem::take(&mut *PARTS.lock().unwrap_or_else(|e| e.into_inner()));
    let spans = std::mem::take(&mut *SPANS.lock().unwrap_or_else(|e| e.into_inner()));
    let ms = took.as_secs_f32() * 1000.0;
    if ms > f32::from_bits(HITCH_MS.load(Ordering::Relaxed)) && !(moment.playing && !moment.ejecting) {
        crate::game::log_launch_timing(&hitch_line(ms, &parts, &spans, moment, work));
    }
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
        // The part that took most of it, or the commands' building.
        let total_us = (ms * 1000.0) as u32;
        let (worst, worst_us) = parts.iter().copied().enumerate().max_by_key(|(_, us)| *us).unwrap_or((3, 0));
        let rest = total_us.saturating_sub(parts.iter().sum::<u32>());
        window.parts[if worst_us >= rest { worst } else { 3 }] += 1;
    }
    if now.duration_since(started) >= WINDOW {
        if window.slow > 0 { crate::game::log_launch_timing(&window.line("5 s")); }
        *window = Window { started: Some(now), ..Window::default() };
    }
}

/// Labels on the shelf (3.2 perf): where each face came from and what it
/// cost, the uploads, and the carts on screen still without their face.
/// One line per 5 s with any label work; nothing per frame.
#[derive(Default)]
struct Labels {
    started: Option<Instant>,
    disk: u32,
    disk_us: u64,
    drawn: u32,
    drawn_us: u64,
    uploads: u32,
    blank: u32,
    blank_frames: u32,
}

static LABELS: Mutex<Labels> = Mutex::new(Labels {
    started: None, disk: 0, disk_us: 0, drawn: 0, drawn_us: 0, uploads: 0, blank: 0, blank_frames: 0,
});

fn labels() -> std::sync::MutexGuard<'static, Labels> {
    LABELS.lock().unwrap_or_else(|e| e.into_inner())
}

/// A face read back from the disk cache in [took] (a raster worker).
pub(crate) fn label_from_disk(took: Duration) {
    let mut l = labels();
    l.disk += 1;
    l.disk_us += took.as_micros() as u64;
}

/// A face drawn from its label in [took] (a raster worker).
pub(crate) fn label_drawn(took: Duration) {
    let mut l = labels();
    l.drawn += 1;
    l.drawn_us += took.as_micros() as u64;
}

/// One frame of the shelf: [uploads] faces sent to the GPU, [blank] carts
/// on screen without their face yet.
pub(crate) fn labels_frame(uploads: u32, blank: u32) {
    let mut l = labels();
    l.uploads += uploads;
    l.blank += blank;
    if blank > 0 { l.blank_frames += 1; }
    let now = Instant::now();
    let started = *l.started.get_or_insert(now);
    if now.duration_since(started) < WINDOW { return; }
    if l.disk + l.drawn + l.uploads + l.blank > 0 {
        let avg = |us: u64, n: u32| if n == 0 { "-".to_string() } else { format!("{:.1}ms", us as f64 / n as f64 / 1000.0) };
        crate::game::log_launch_timing(&format!(
            "Shelf labels (5 s): {} from the disk cache (each {}), {} drawn (each {}), {} uploads, \
             {} frames with carts on screen still blank ({} cart-frames)",
            l.disk, avg(l.disk_us, l.disk), l.drawn, avg(l.drawn_us, l.drawn), l.uploads,
            l.blank_frames, l.blank));
    }
    *l = Labels { started: Some(now), ..Labels::default() };
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
        assert_eq!(hitch_line(120.0, &[1000, 2000, 7000], &[0, 4000, 90000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            Moment { scrolling: false, ..Moment::default() }, WORK_LABEL),
            "Slow frame 120ms: wallpaper 1.0, upload 2.0, gpu 7.0, build 110.0 \
            (title 4.0, focus 90.0); during: label");
        assert_eq!(line, "Shelf frames (bench): 300 drawn, 2 over 17ms, 1 over 33ms, longest 41.5ms; \
            during: scroll=1 label=2");
    }
}
