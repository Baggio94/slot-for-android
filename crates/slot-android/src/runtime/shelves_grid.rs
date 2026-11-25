//! The shelves page (3.0.1-beta12, Siham 2026-11-25): rounded tiles over the
//! whole screen, each system's chosen cart in its own colours fanned with
//! the one before and after it, Recents and Favorites with their first
//! three and their sign, apart at the start; the game count top left. Tiles
//! grow to fill the screen when there are few; past that, Interface >
//! Shelves scrolls them sideways (Scroll, faded at the edges) or down by
//! whole rows (Grid). One native page, no ROM I/O.
use super::*;

type Face = (slot_gfx::TexId, u32, u32);
const MARGIN: f32 = 22.0;
/// The tiles take the whole screen but a margin (3.0.1-beta12, Siham
/// 2026-11-25): no heading, status bar or hints on this page.
const TOP: f32 = 18.0;
const BOTTOM: f32 = 458.0;
const ANIM_MS: f32 = 180.0;
/// A tile's width to its height.
const TILE_ASPECT: f32 = 1.0;
/// The corner pills' radius: the game count (top left) and Recents' and
/// Favorites' sign (top right).
const PILL_R: f32 = 13.0;

#[derive(Clone, Copy, PartialEq, Default, Debug)]
enum Motion { #[default] Spring, Finger, Coast }

/// A finger on the page scrolling sideways.
struct Finger {
    x0: f32,
    y0: f32,
    ms0: i64,
    scroll0: f32,
    moved: bool,
    /// It stopped a moving page: lifting it is not a tap.
    caught: bool,
    samples: std::collections::VecDeque<(i64, f32)>,
}

/// SCROLL moves as the carousel does (3.0.1-beta13, Siham 2026-11-25): a
/// spring towards the chosen column, a held direction that steps faster
/// and faster, the page under the finger, a fling that coasts and settles
/// on a column. Canvas pixels.
#[derive(Default)]
struct Glider {
    x: f32,
    v: f32,
    motion: Motion,
    placed: bool,
    tick: Option<Instant>,
    /// A held direction: its key, the next step (ms since start), steps so far.
    held: Option<(i32, u64, usize)>,
    finger: Option<Finger>,
}

#[derive(Default)]
pub(super) struct GridState {
    /// White quarter discs (top-left, top-right, bottom-left, bottom-right)
    /// for rounded panels, and a soft round shadow.
    corners: Option<[slot_gfx::TexId; 4]>,
    blob: Option<slot_gfx::TexId>,
    /// "128", by rack, and the count it says.
    counts: Vec<Option<(usize, Face)>>,
    labels: Vec<Face>,
    marks: Option<[Face; 2]>,
    opened: Option<Instant>,
    closing: Option<(usize, Instant)>,
    /// The page's scroll when the choice last moved, and when: it glides
    /// from there to the new choice's.
    glide: Option<(f32, Instant)>,
    touch: Option<(f32, f32, i64)>,
    glider: Glider,
}

impl GridState {
    pub(super) fn reset(&mut self) {
        self.opened = None;
        self.closing = None;
        self.glide = None;
        self.touch = None;
        self.glider = Glider::default();
    }
}

/// Sideways: the column the page at scroll `x` is nearest to resting on.
fn column_at(layout: &Showcase, x: f32, width: f32) -> usize {
    let room = width - 2.0 * MARGIN;
    let max = (layout.extent - room).max(0.0);
    let rest = |start: f32| (start - MARGIN - (room - layout.w) / 2.0).clamp(0.0, max);
    layout.starts.iter().enumerate()
        .min_by(|a, b| (rest(*a.1) - x).abs().total_cmp(&(rest(*b.1) - x).abs()))
        .map_or(0, |(c, _)| c)
}

/// Tile `current`'s row, in column `col` (its last tile when shorter).
fn tile_in_column(layout: &Showcase, current: usize, col: usize) -> Option<usize> {
    let line = *layout.line.get(current)?;
    let row = (0..current).filter(|&i| layout.line[i] == line).count();
    let tiles: Vec<usize> = (0..layout.line.len()).filter(|&i| layout.line[i] == col).collect();
    tiles.get(row).or(tiles.last()).copied()
}

fn shelf_order(nonempty: impl Fn(usize) -> bool, show_recents: bool) -> Vec<usize> {
    let mut order = Vec::with_capacity(Platform::ALL.len() + 3);
    if show_recents && nonempty(RECENTS_RACK) { order.push(RECENTS_RACK); }
    if nonempty(FAVORITES_RACK) { order.push(FAVORITES_RACK); }
    for (rack, platform) in Platform::ALL.iter().enumerate() {
        if nonempty(rack) { order.push(rack); }
        if *platform == Platform::Gba && nonempty(VIDEO_RACK) { order.push(VIDEO_RACK); }
    }
    order
}

/// The tile the D-pad goes to from `index`: the nearest one that way, by
/// the tiles' places (rows centred, Recents and Favorites apart); L1 and
/// R1 (102, 103) the one before and after.
fn move_tile(cells: &[[f32; 4]], index: usize, key: i32) -> usize {
    let n = cells.len();
    if n == 0 { return 0; }
    let i = index.min(n - 1);
    match key {
        102 => return i.saturating_sub(1),
        103 => return (i + 1).min(n - 1),
        _ => {}
    }
    let centre = |c: [f32; 4]| (c[0] + c[2] / 2.0, c[1] + c[3] / 2.0);
    let (x, y) = centre(cells[i]);
    let (dx, dy) = match key { 19 => (0.0, -1.0), 20 => (0.0, 1.0), 21 => (-1.0, 0.0), 22 => (1.0, 0.0), _ => return i };
    let mut best: Option<(f32, usize)> = None;
    for (j, &c) in cells.iter().enumerate() {
        if j == i { continue; }
        let (cx, cy) = centre(c);
        let along = (cx - x) * dx + (cy - y) * dy;
        if along < 1.0 { continue; }
        // Sideways stays on its row (a column's lone last tile, centred,
        // still counts).
        if dx != 0.0 && (cy - y).abs() > (cells[i][3] + GAP) / 2.0 + 1.0 { continue; }
        let across = ((cx - x) * dy).abs() + ((cy - y) * dx).abs();
        let score = along + 2.0 * across;
        if best.is_none_or(|(b, _)| score < b) { best = Some((score, j)); }
    }
    best.map_or(i, |(_, j)| j)
}

fn fit(face: Face, cx: f32, cy: f32, max_w: f32, max_h: f32, alpha: f32, out: &mut Vec<Draw>) {
    let (tex, w, h) = face;
    let scale = (max_w / w.max(1) as f32).min(max_h / h.max(1) as f32);
    let (w, h) = (w as f32 * scale, h as f32 * scale);
    out.push(Draw::Tex { x: cx - w / 2.0, y: cy - h / 2.0, w, h, tex, alpha });
}

fn rect(out: &mut Vec<Draw>, [x,y,w,h]: [f32;4], colour: [f32;4]) {
    out.push(Draw::Rect { x,y,w,h,colour });
}

impl Engine {
    fn shelf_grid_order(&self) -> Vec<usize> {
        shelf_order(|rack| self.shelves.get(rack).is_some_and(|s| !s.carts.is_empty()),
            self.settings.show_recents)
    }

    /// The page's tiles for `order`, as Interface > Shelves lays them.
    fn shelves_layout(&self, order: &[usize]) -> Showcase {
        let lead = order.iter().take_while(|&&r| r == RECENTS_RACK || r == FAVORITES_RACK).count();
        showcase(lead, order.len(), out_w() as f32, TILE_ASPECT, self.settings.shelves_grid)
    }

    /// How far the page has scrolled for tile `selected` now, gliding there.
    fn shelves_scroll(&self, layout: &Showcase, selected: usize) -> f32 {
        let target = layout.scroll_for(selected, out_w() as f32);
        if layout.scroll == Scroll::Across && self.shelves_grid.glider.placed {
            return self.shelves_grid.glider.x;
        }
        match self.shelves_grid.glide {
            Some((from, at)) => {
                let t = (at.elapsed().as_secs_f32() * 1000.0 / ANIM_MS).clamp(0.0, 1.0);
                from + (target - from) * slot_ui::ease(t)
            }
            None => target,
        }
    }

    pub(super) fn open_shelves(&mut self) {
        let order = self.shelf_grid_order();
        let Some(&rack) = order.iter().find(|&&r| r == self.active).or_else(|| order.first()) else { return; };
        self.shelves[self.active].release_hold();
        self.cancel_shelves_triggers();
        self.touch = None;
        self.studio_when_settled = false;
        self.menu_held = None;
        self.shelves_grid.reset();
        self.shelves_grid.opened = Some(Instant::now());
        self.overlay = ShelfOverlay::Shelves { rack };
    }

    fn close_shelves(&mut self, rack: usize, select: bool) {
        let order = self.shelf_grid_order();
        self.shelves_grid.touch = None;
        self.shelves_grid.closing = Some((rack, Instant::now()));
        self.menu_held = None;
        self.overlay = ShelfOverlay::None;
        if select && order.contains(&rack) && (rack == VIDEO_RACK || rack == 0) {
            self.gba_video = rack == VIDEO_RACK;
        }
        if select && order.contains(&rack) && rack != self.active {
            self.shelves[self.active].release_hold();
            self.active = rack;
            self.progress = 0.0;
            self.letter_shown_at = None;
            self.platform_shown_at = Some(Instant::now());
            self.last_cart_navigation = Instant::now();
        }
    }

    /// Choose tile `next`, the page gliding from where it is.
    fn choose_shelf_tile(&mut self, order: &[usize], current: usize, next: usize) {
        if next == current { return; }
        let layout = self.shelves_layout(order);
        let from = self.shelves_scroll(&layout, current);
        self.shelves_grid.glide = Some((from, Instant::now()));
        if let Some(&rack) = order.get(next) { self.overlay = ShelfOverlay::Shelves { rack }; }
    }

    pub(super) fn shelves_key(&mut self, key: i32) {
        let ShelfOverlay::Shelves { rack } = self.overlay else { return; };
        let order = self.shelf_grid_order();
        if order.is_empty() {
            self.shelves_grid.reset();
            self.overlay = ShelfOverlay::None;
            return;
        }
        let current = order.iter().position(|&r| r == rack).unwrap_or(0);
        match key {
            96 => self.close_shelves(order[current], true),
            97 | 108 => self.close_shelves(rack, false),
            19..=22 | 102 | 103 => {
                let layout = self.shelves_layout(&order);
                let next = move_tile(&layout.cells, current, key);
                self.choose_shelf_tile(&order, current, next);
                // Sideways, a held direction keeps going, as on the carousel.
                if layout.scroll == Scroll::Across && matches!(key, 21 | 22) {
                    let glider = &mut self.shelves_grid.glider;
                    glider.motion = Motion::Spring;
                    glider.finger = None;
                    let now = self.born.elapsed().as_millis() as u64;
                    glider.held = Some((key, now + slot_ui::shelf_feel::REPEAT_DELAY_MS, 0));
                }
            }
            _ => {}
        }
    }

    /// A direction let go: a held one stops.
    pub(super) fn shelves_release(&mut self, key: i32) {
        let glider = &mut self.shelves_grid.glider;
        if glider.held.is_some_and(|(held, _, _)| held == key) { glider.held = None; }
    }

    /// Per frame, sideways: the held direction's steps, the spring, the
    /// fling.
    pub(super) fn shelves_glide(&mut self) {
        let ShelfOverlay::Shelves { rack } = self.overlay else { return };
        let order = self.shelf_grid_order();
        let layout = self.shelves_layout(&order);
        if layout.scroll != Scroll::Across {
            self.shelves_grid.glider = Glider::default();
            return;
        }
        let width = out_w() as f32;
        let now = Instant::now();
        let ms = self.born.elapsed().as_millis() as u64;
        let mut current = order.iter().position(|&r| r == rack).unwrap_or(0);
        let dt = self.shelves_grid.glider.tick.map_or(0.0, |t| (now - t).as_secs_f32().min(0.05));
        self.shelves_grid.glider.tick = Some(now);
        if let Some((key, due, fired)) = self.shelves_grid.glider.held {
            if ms >= due {
                current = move_tile(&layout.cells, current, key);
                let gap = slot_ui::shelf_feel::REPEAT_MS[fired.min(slot_ui::shelf_feel::REPEAT_MS.len() - 1)];
                self.shelves_grid.glider.held = Some((key, ms + gap, fired + 1));
            }
        }
        let max = (layout.extent - (width - 2.0 * MARGIN)).max(0.0);
        let pitch = layout.w + GAP;
        let target = layout.scroll_for(current, width);
        let omega = slot_ui::shelf_feel::OMEGA;
        let glider = &mut self.shelves_grid.glider;
        if !glider.placed {
            glider.x = target;
            glider.v = 0.0;
            glider.placed = true;
        }
        match glider.motion {
            Motion::Finger => {}
            Motion::Coast => {
                glider.v *= (-slot_ui::shelf_feel::COAST_FRICTION * dt).exp();
                glider.x += glider.v * dt;
                let edge = glider.x <= 0.0 || glider.x >= max;
                glider.x = glider.x.clamp(0.0, max);
                if let Some(next) = tile_in_column(&layout, current, column_at(&layout, glider.x, width)) {
                    current = next;
                }
                if edge || glider.v.abs() <= slot_ui::shelf_feel::COAST_END * pitch {
                    let land = (glider.x + glider.v / omega).clamp(0.0, max);
                    if let Some(next) = tile_in_column(&layout, current, column_at(&layout, land, width)) {
                        current = next;
                    }
                    glider.motion = Motion::Spring;
                    if edge { glider.v = 0.0; }
                }
            }
            Motion::Spring => {
                let target = layout.scroll_for(current, width);
                let accel = -2.0 * omega * glider.v - omega * omega * (glider.x - target);
                glider.v += accel * dt;
                glider.x += glider.v * dt;
            }
        }
        if let Some(&next) = order.get(current) {
            if next != rack { self.overlay = ShelfOverlay::Shelves { rack: next }; }
        }
    }

    /// Sideways, the page under the finger, as the carousel.
    fn shelves_finger(&mut self, action: u8, x: f32, y: f32, ms: i64, layout: &Showcase, order: &[usize]) {
        let width = out_w() as f32;
        let max = (layout.extent - (width - 2.0 * MARGIN)).max(0.0);
        let ShelfOverlay::Shelves { rack } = self.overlay else { return };
        let current = order.iter().position(|&r| r == rack).unwrap_or(0);
        let glider = &mut self.shelves_grid.glider;
        match action {
            0 => {
                // The row under the finger becomes the chosen one's as the
                // page goes (Siham 2026-11-25).
                let under = (0..layout.cells.len()).find(|&i| {
                    let [_, top, _, h] = layout.cells[i];
                    y >= top && y <= top + h
                });
                if let Some(row) = under.map(|i| (0..i).filter(|&j| layout.line[j] == layout.line[i]).count()) {
                    let col = layout.line[current];
                    let tiles: Vec<usize> = (0..layout.line.len()).filter(|&i| layout.line[i] == col).collect();
                    if let Some(&tile) = tiles.get(row) {
                        if let Some(&r) = order.get(tile) { self.overlay = ShelfOverlay::Shelves { rack: r }; }
                    }
                }
                let glider = &mut self.shelves_grid.glider;
                let caught = glider.motion == Motion::Coast || glider.v.abs() > 60.0;
                glider.held = None;
                glider.motion = Motion::Finger;
                glider.v = 0.0;
                let mut samples = std::collections::VecDeque::with_capacity(16);
                samples.push_back((ms, x));
                glider.finger = Some(Finger { x0: x, y0: y, ms0: ms, scroll0: glider.x, moved: false, caught, samples });
            }
            1 => {
                let Some(finger) = glider.finger.as_mut() else { return };
                if finger.samples.len() == 16 { finger.samples.pop_front(); }
                finger.samples.push_back((ms, x));
                if (x - finger.x0).abs() > 10.0 { finger.moved = true; }
                if !finger.moved { return; }
                let want = finger.scroll0 - (x - finger.x0);
                // Past an end the page gives a little, as a list does.
                glider.x = if want < 0.0 { want * 0.3 } else if want > max { max + (want - max) * 0.3 } else { want };
                if let Some(next) = tile_in_column(layout, current, column_at(layout, glider.x.clamp(0.0, max), width)) {
                    if let Some(&r) = order.get(next) { self.overlay = ShelfOverlay::Shelves { rack: r }; }
                }
            }
            _ => {
                let Some(finger) = glider.finger.take() else { return };
                let tap = action == 2 && !finger.moved && !finger.caught && ms - finger.ms0 <= 600
                    && (y - finger.y0).abs() <= 18.0;
                let pitch = layout.w + GAP;
                let speed = super::touch::speed_of(&finger.samples, ms);
                let v = (-speed * slot_ui::shelf_feel::FLING_GAIN)
                    .clamp(-slot_ui::shelf_feel::COAST_MAX * pitch, slot_ui::shelf_feel::COAST_MAX * pitch);
                glider.x = glider.x.clamp(0.0, max);
                if action == 2 && finger.moved && v.abs() > slot_ui::shelf_feel::COAST_END * pitch {
                    glider.v = v;
                    glider.motion = Motion::Coast;
                } else {
                    glider.v = 0.0;
                    glider.motion = Motion::Spring;
                    if finger.moved {
                        if let Some(next) = tile_in_column(layout, current, column_at(layout, glider.x, width)) {
                            if let Some(&r) = order.get(next) { self.overlay = ShelfOverlay::Shelves { rack: r }; }
                        }
                    }
                }
                if tap {
                    let scroll = glider.x;
                    for (index, &r) in order.iter().enumerate() {
                        let [left, top, w, h] = layout.cells[index];
                        let left = left - scroll;
                        if x >= left && x <= left + w && y >= top && y <= top + h {
                            self.close_shelves(r, true);
                            return;
                        }
                    }
                }
            }
        }
    }

    pub(super) fn shelves_touch(&mut self, action: u8, x: f32, y: f32, ms: i64) {
        {
            let order = self.shelf_grid_order();
            let layout = self.shelves_layout(&order);
            if layout.scroll == Scroll::Across {
                self.shelves_finger(action, x, y, ms, &layout, &order);
                return;
            }
        }
        // Android: DOWN=0, MOVE=1, UP=2, CANCEL=3.
        if action == 0 {
            self.shelves_grid.touch = Some((x,y,ms));
            return;
        }
        if action == 1 { return; }
        let Some((sx,sy,at)) = self.shelves_grid.touch.take() else { return; };
        if action != 2 { return; }
        let ShelfOverlay::Shelves { rack } = self.overlay else { return; };
        let order = self.shelf_grid_order();
        let layout = self.shelves_layout(&order);
        let current = order.iter().position(|&r| r == rack).unwrap_or(0);
        let (dx, dy) = (x - sx, y - sy);
        // A swipe the way the page scrolls moves by as many tiles as it
        // went past, at least one.
        let swipe = match layout.scroll {
            Scroll::Across if dx.abs() > 34.0 && dx.abs() > dy.abs() =>
                Some((if dx < 0.0 { 22 } else { 21 }, dx.abs() / (layout.w + GAP))),
            Scroll::Down | Scroll::Still if dy.abs() > 34.0 && dy.abs() > dx.abs() =>
                Some((if dy < 0.0 { 20 } else { 19 }, dy.abs() / (layout.h + GAP))),
            _ => None,
        };
        if let Some((key, tiles)) = swipe {
            let mut next = current;
            for _ in 0..(tiles.round() as usize).clamp(1, 8) { next = move_tile(&layout.cells, next, key); }
            self.choose_shelf_tile(&order, current, next);
            return;
        }
        if dx.hypot(dy) > 18.0 || ms - at > 600 { return; }
        let scroll = self.shelves_scroll(&layout, current);
        for (index, &rack) in order.iter().enumerate() {
            let [mut left, mut top, w, h] = layout.cells[index];
            match layout.scroll {
                Scroll::Across => left -= scroll,
                Scroll::Down => top -= scroll,
                Scroll::Still => {}
            }
            if x >= left && x <= left + w && y >= top && y <= top + h {
                self.close_shelves(rack, true);
                return;
            }
        }
    }

    pub(super) fn prepare_shelves_grid(&mut self) {
        if self.game.is_some() || self.inserted || self.progress > 0.0 { return; }
        let open = matches!(self.overlay, ShelfOverlay::Shelves { .. });
        if !open { return; }
        if self.shelf_grid_order().is_empty() {
            self.overlay = ShelfOverlay::None;
            self.shelves_grid.reset();
            return;
        }
        if self.shelves_grid.labels.is_empty() {
            let names = Platform::ALL.iter().map(|p|p.name()).chain(["RECENTS","FAVORITES","GBA Video"]);
            self.shelves_grid.labels = names.map(|name| {
                let face = quick_value_face(name, true);
                (self.gpu.create_face(&face),face.w,face.h)
            }).collect();
            let clock = slot_ui::analog_clock_face(24, 10, 10, [250,250,250]);
            let star = slot_ui::star_face(2,[246,195,44]);
            self.shelves_grid.marks = Some([
                (self.gpu.create_face(&clock),clock.w,clock.h),
                (self.gpu.create_face(&star),star.w,star.h),
            ]);
        }
        self.prepare_showcase();
    }

    pub(super) fn draw_shelves_overlay(&self, out: &mut Vec<Draw>) -> bool {
        let (rack,alpha) = if let ShelfOverlay::Shelves { rack } = self.overlay {
            let t = self.shelves_grid.opened.map_or(1.0, |at| at.elapsed().as_secs_f32()*1000.0 / ANIM_MS);
            (rack,slot_ui::ease(t.clamp(0.0,1.0)))
        } else if matches!(self.overlay, ShelfOverlay::None) && !self.inserted {
            let Some((rack,at)) = self.shelves_grid.closing else { return false; };
            let t = at.elapsed().as_secs_f32()*1000.0 / ANIM_MS;
            if t >= 1.0 { return false; }
            (rack,1.0-slot_ui::ease(t.clamp(0.0,1.0)))
        } else { return false; };
        let width = out_w();
        if self.shelf_grid_order().is_empty() { return true; }
        // The current wallpaper remains underneath, with the theme's veil.
        // Redraw the ground over the carousel so no slot or old carts show.
        let screen = [0.0,0.0,width as f32,OUT_H as f32];
        let mut ground = menu_style::shelf_ground();
        ground[3] = alpha;
        rect(out,screen,ground);
        if let Some((tex,tint,_)) = self.active_wallpaper() {
            out.push(Draw::Tex { x:0.0,y:0.0,w:width as f32,h:OUT_H as f32,tex,alpha });
            if tint > 0.0 { rect(out,screen,[0.0,0.0,0.0,tint*alpha]); }
            let mut veil = menu_style::scrim(0.62);
            veil[3] *= alpha;
            rect(out,screen,veil);
        }
        self.draw_showcase(rack, alpha, out);
        out.push(Draw::Ink(None));
        true
    }
}

/// A rounded panel: three bands and four quarter discs, none overlapping.
fn panel(out: &mut Vec<Draw>, corners: [slot_gfx::TexId; 4], [x, y, w, h]: [f32; 4], r: f32, c: [f32; 4],
         ink: Option<slot_gfx::InkMap>) {
    let r = r.min(w / 2.0).min(h / 2.0);
    rect(out, [x + r, y, w - 2.0 * r, h], c);
    rect(out, [x, y + r, r, h - 2.0 * r], c);
    rect(out, [x + w - r, y + r, r, h - 2.0 * r], c);
    let rgb = [c[0], c[1], c[2]];
    out.push(Draw::Ink(Some(slot_gfx::InkMap { paper: rgb, ink: rgb })));
    for (tex, cx, cy) in [(corners[0], x, y), (corners[1], x + w - r, y),
        (corners[2], x, y + h - r), (corners[3], x + w - r, y + h - r)] {
        out.push(Draw::Tex { x: cx, y: cy, w: r, h: r, tex, alpha: c[3] });
    }
    out.push(Draw::Ink(ink));
}

fn quarter_discs(gpu: &mut Compositor) -> [slot_gfx::TexId; 4] {
    const N: u32 = 48;
    let mut disc = |fx: bool, fy: bool| {
        let mut rgba = vec![255u8; (N * N * 4) as usize];
        for y in 0..N {
            for x in 0..N {
                // The disc's centre at the panel's inner corner.
                let (px, py) = (if fx { x as f32 + 0.5 } else { N as f32 - x as f32 - 0.5 },
                                if fy { y as f32 + 0.5 } else { N as f32 - y as f32 - 0.5 });
                let d = (px * px + py * py).sqrt();
                let a = (N as f32 - d + 0.5).clamp(0.0, 1.0);
                rgba[((y * N + x) * 4 + 3) as usize] = (a * 255.0).round() as u8;
            }
        }
        gpu.create_texture(N, N, &rgba)
    };
    [disc(false, false), disc(true, false), disc(false, true), disc(true, true)]
}

fn soft_blob(gpu: &mut Compositor) -> slot_gfx::TexId {
    const W: u32 = 128;
    const H: u32 = 32;
    let mut rgba = vec![255u8; (W * H * 4) as usize];
    for y in 0..H {
        for x in 0..W {
            let (u, v) = ((x as f32 + 0.5) / W as f32 * 2.0 - 1.0, (y as f32 + 0.5) / H as f32 * 2.0 - 1.0);
            let d = (u * u + v * v).sqrt();
            let a = (1.0 - d).clamp(0.0, 1.0).powf(1.6);
            rgba[((y * W + x) * 4 + 3) as usize] = (a * 255.0).round() as u8;
        }
    }
    gpu.create_texture(W, H, &rgba)
}

/// How the page moves when the tiles don't all fit.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(super) enum Scroll { Still, Across, Down }

const GAP: f32 = 14.0;
/// The extra room between Recents and Favorites and the systems.
const SPLIT: f32 = 16.0;

/// Where the tiles go: as big as the screen allows for this many shelves,
/// in one to three still rows; past that, two rows that scroll sideways
/// (`down` false) or two visible rows that scroll down. Recents and
/// Favorites (the first `lead`) keep a row or a column of their own.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Showcase {
    pub w: f32,
    pub h: f32,
    pub scroll: Scroll,
    /// Each tile's place before scrolling, in the page's order.
    pub cells: Vec<[f32; 4]>,
    /// Each tile's row (or column, scrolling sideways), and where each starts.
    pub line: Vec<usize>,
    pub starts: Vec<f32>,
    /// How far the tiles reach sideways, from the margin.
    pub extent: f32,
}

fn lines(lead: usize, n: usize, len: usize) -> Vec<Vec<usize>> {
    let lead = lead.min(n);
    let mut out = Vec::new();
    for range in [0..lead, lead..n] {
        let items: Vec<usize> = range.collect();
        for chunk in items.chunks(len.max(1)) { out.push(chunk.to_vec()); }
    }
    out
}

pub(super) fn showcase(lead: usize, n: usize, width: f32, aspect: f32, down: bool) -> Showcase {
    if n == 0 {
        return Showcase { w: 0.0, h: 0.0, scroll: Scroll::Still, cells: Vec::new(), line: Vec::new(),
            starts: Vec::new(), extent: 0.0 };
    }
    let (top, bottom, margin) = (TOP - 2.0, BOTTOM + 6.0, MARGIN);
    let (aw, ah) = (width - 2.0 * margin, bottom - top);
    let split = if lead > 0 && lead < n { SPLIT } else { 0.0 };
    let lead_lines = |len: usize| lead.min(n).div_ceil(len.max(1));
    // Rows, each centred across the page; `y0` the first row's top.
    let by_rows = |len: usize, w: f32, h: f32, y0: f32, scroll: Scroll| {
        let ls = lines(lead, n, len);
        let mut sh = Showcase { w, h, scroll, cells: vec![[0.0; 4]; n], line: vec![0; n],
            starts: Vec::new(), extent: aw };
        let mut y = y0;
        for (r, row) in ls.iter().enumerate() {
            if r > 0 { y += h + GAP + if r == lead_lines(len) { split } else { 0.0 }; }
            sh.starts.push(y);
            let used = row.len() as f32 * w + (row.len() as f32 - 1.0) * GAP;
            for (j, &i) in row.iter().enumerate() {
                sh.cells[i] = [margin + (aw - used) / 2.0 + j as f32 * (w + GAP), y, w, h];
                sh.line[i] = r;
            }
        }
        sh
    };
    let least = 150.0;
    let mut best: Option<Showcase> = None;
    // Fewest rows first: more rows only for clearly bigger tiles.
    for len in (1..=n.max(1)).rev() {
        let rows = lines(lead, n, len).len();
        if rows > 3 { continue; }
        let cols = (n - lead.min(n)).min(len).max(lead.min(n).min(len));
        let mut h = (ah - (rows - 1) as f32 * GAP - split) / rows as f32;
        let mut w = h * aspect;
        if cols as f32 * w + (cols as f32 - 1.0) * GAP > aw {
            w = (aw - (cols as f32 - 1.0) * GAP) / cols as f32;
            h = h.min(w / aspect);
        }
        if h < least { continue; }
        if best.as_ref().is_none_or(|b| w * h > b.w * b.h * 1.1) {
            let used = rows as f32 * h + (rows - 1) as f32 * GAP + split;
            best = Some(by_rows(len, w, h, top + (ah - used) / 2.0, Scroll::Still));
        }
    }
    if let Some(still) = best { return still; }
    if down {
        let h = (ah - GAP - split) / 2.0;
        let mut w = h * aspect;
        let cols = (((aw + GAP) / (w + GAP)).floor() as usize).max(2);
        if cols as f32 * (w + GAP) - GAP > aw { w = (aw - (cols as f32 - 1.0) * GAP) / cols as f32; }
        return by_rows(cols, w, h, top, Scroll::Down);
    }
    // Two rows, column by column, sideways.
    let h = (ah - GAP) / 2.0;
    let w = h * aspect;
    let ls = lines(lead, n, 2);
    let mut sh = Showcase { w, h, scroll: Scroll::Across, cells: vec![[0.0; 4]; n], line: vec![0; n],
        starts: Vec::new(), extent: 0.0 };
    let mut x = margin;
    for (c, column) in ls.iter().enumerate() {
        if c > 0 { x += w + GAP + if c == lead_lines(2) { split } else { 0.0 }; }
        sh.starts.push(x);
        let used = column.len() as f32 * h + (column.len() as f32 - 1.0) * GAP;
        for (j, &i) in column.iter().enumerate() {
            sh.cells[i] = [x, top + (ah - used) / 2.0 + j as f32 * (h + GAP), w, h];
            sh.line[i] = c;
        }
    }
    sh.extent = x + w - margin;
    sh
}

impl Showcase {
    /// How far the page has moved for tile `i` chosen: sideways, its column
    /// in the middle; down, whole rows, its row last of the two in view.
    pub fn scroll_for(&self, i: usize, width: f32) -> f32 {
        let Some(&line) = self.line.get(i) else { return 0.0 };
        match self.scroll {
            Scroll::Still => 0.0,
            Scroll::Across => {
                let room = width - 2.0 * MARGIN;
                (self.starts[line] - MARGIN - (room - self.w) / 2.0).clamp(0.0, (self.extent - room).max(0.0))
            }
            Scroll::Down => self.starts[line.saturating_sub(1)] - self.starts[0],
        }
    }
}

impl Engine {
    /// A shelf's carts as the page shows them, the front one first: a
    /// system its chosen one, then the one before and after it, as the
    /// shelf shows them; Recents and Favorites their first three (face, or
    /// silhouette while it is drawn), and their shape.
    fn showcase_carts(&self, rack: usize) -> Vec<(slot_gfx::TexId, f32)> {
        let Some(shelf) = self.shelves.get(rack) else { return Vec::new() };
        let len = shelf.carts.len();
        if len == 0 { return Vec::new(); }
        let picks: Vec<usize> = if rack == RECENTS_RACK || rack == FAVORITES_RACK {
            (0..len.min(3)).collect()
        } else {
            let at = shelf.index.min(len - 1);
            match len {
                1 => vec![at],
                2 => vec![at, (at + 1) % 2],
                _ => vec![at, (at + len - 1) % len, (at + 1) % len],
            }
        };
        picks.into_iter().filter_map(|i| {
            let tex = shelf.face(i).or_else(|| shelf.silhouette(i))?;
            let (w, h) = slot_ui::shelf_box_for(&shelf.carts[i]);
            Some((tex, w / h))
        }).collect()
    }

    pub(super) fn prepare_showcase(&mut self) {
        if self.shelves_grid.corners.is_none() {
            self.shelves_grid.corners = Some(quarter_discs(&mut self.gpu));
            self.shelves_grid.blob = Some(soft_blob(&mut self.gpu));
        }
        let racks = self.shelves.len();
        if self.shelves_grid.counts.len() < racks { self.shelves_grid.counts.resize(racks, None); }
        for rack in 0..racks {
            let n = self.shelves[rack].carts.len();
            if n == 0 || self.shelves_grid.counts[rack].is_some_and(|(said, _)| said == n) { continue; }
            let face = quick_value_face(&n.to_string(), true);
            self.shelves_grid.counts[rack] = Some((n, (self.gpu.create_face(&face), face.w, face.h)));
        }
    }

    pub(super) fn draw_showcase(&self, rack: usize, alpha: f32, out: &mut Vec<Draw>) {
        let width = out_w() as f32;
        let order = self.shelf_grid_order();
        let Some(corners) = self.shelves_grid.corners else { return };
        let ink = menu_style::ink_map();
        let light = ink.is_some();
        let layout = self.shelves_layout(&order);
        let selected = order.iter().position(|&r| r == rack).unwrap_or(0);
        let scroll = self.shelves_scroll(&layout, selected);
        let (top, bottom) = (TOP - 3.0, BOTTOM + 7.0);
        if layout.scroll == Scroll::Across {
            // Each edge fades while there is more page beyond it, and the
            // fade goes as the page comes to that end, never at once.
            let feather = layout.w * 0.45;
            let max = (layout.extent - (width - 2.0 * MARGIN)).max(0.0);
            let ease = |room: f32| feather * (room / (layout.w * 0.5)).clamp(0.0, 1.0);
            out.push(Draw::Span(Some(slot_gfx::Span { left: 8.0, right: width - 8.0,
                feather_left: ease(scroll).max(0.01), feather_right: ease(max - scroll).max(0.01) })));
        }
        // Text on the theme: white on Dark and OLED, the palette's ink on
        // Light and Original's light plastics.
        let ink_rgb = ink.map_or([1.0, 1.0, 1.0], |m| m.ink);
        let pill_text = if light { [1.0, 1.0, 1.0] } else { [0.11, 0.11, 0.125] };
        for (index, &id) in order.iter().enumerate() {
            let [mut x, mut y, w, h] = layout.cells[index];
            match layout.scroll {
                Scroll::Across => x -= scroll,
                Scroll::Down => y -= scroll,
                Scroll::Still => {}
            }
            if x + w < 0.0 || x > width { continue; }
            // Scrolling down, a row on its way in or out fades at the edge.
            let outside = (top - y).max(0.0) + (y + h - bottom).max(0.0);
            if outside > h * 0.4 { continue; }
            let alpha = alpha * (1.0 - outside / (h * 0.4));
            let chosen = index == selected;
            let special = id == RECENTS_RACK || id == FAVORITES_RACK;
            let a = alpha * if chosen { 1.0 } else { 0.86 };
            let label_h = (h * 0.085).clamp(14.0, 20.0);
            // The name's pill keeps a margin under it, as at the sides.
            let pill_h = label_h + 6.0;
            let name_y = y + h - 10.0 - pill_h / 2.0;
            let foot = name_y - pill_h / 2.0 - 4.0;
            let fill = match (light, chosen) {
                (false, false) => [1.0, 1.0, 1.0, 0.065], (false, true) => [1.0, 1.0, 1.0, 0.15],
                (true, false) => [1.0, 1.0, 1.0, 0.45], (true, true) => [1.0, 1.0, 1.0, 0.85],
            };
            panel(out, corners, [x, y, w, h], 16.0, [fill[0], fill[1], fill[2], fill[3] * alpha], ink);
            // The carts, lightly fanned, inside their tile even lifted:
            // under the corner pills, over the name's pill, never touching.
            let mut carts = if id == VIDEO_RACK || id < self.shelves.len() { self.showcase_carts(id) } else { Vec::new() };
            // GRID: a system's chosen cart alone, large; Recents and
            // Favorites keep their fan (Siham 2026-11-25).
            if self.settings.shelves_grid && !special { carts.truncate(1); }
            let inner = w - 2.0 * 14.0;
            let lift_px = if chosen { 5.0 } else { 0.0 };
            let top_clear = y + 8.0 + 2.0 * PILL_R + 4.0;
            let base = foot - 6.0 - lift_px;
            // The room a lifted cart has; one at rest is a little smaller.
            let room_h = foot - 11.0 - top_clear;
            let rest = if chosen { 1.0 } else { 1.0 / 1.04 };
            let cx = x + w / 2.0;
            let widest = carts.iter().map(|c| c.1).fold(0.1f32, f32::max);
            // The chosen tile opens its fan a little wider.
            let (turn, spread, back) = if chosen { (0.15f32, 0.30f32, 0.88f32) } else { (0.12, 0.24, 0.9) };
            let front_h = if carts.len() > 1 {
                let half = spread * widest + back * 0.5 * (widest * turn.cos() + turn.sin());
                room_h.min(inner / (2.0 * half)) * rest
            } else {
                room_h.min(inner / widest) * rest
            };
            let front_w = front_h * carts.first().map_or(1.0, |c| c.1);
            if let Some(blob) = self.shelves_grid.blob {
                out.push(Draw::Ink(Some(slot_gfx::InkMap { paper: [0.0; 3], ink: [0.0; 3] })));
                let bw = front_w * if carts.len() > 1 { 1.5 } else { 1.05 };
                out.push(Draw::Tex { x: cx - bw / 2.0, y: base - 7.0 + if chosen { 5.0 } else { 0.0 }, w: bw, h: 16.0,
                    tex: blob, alpha: alpha * if light { 0.22 } else { 0.55 } });
            }
            let draw_order: &[(usize, f32)] = match carts.len() {
                3 => &[(1, -1.0), (2, 1.0), (0, 0.0)],
                2 => &[(1, 1.0), (0, 0.0)],
                1 => &[(0, 0.0)],
                _ => &[],
            };
            // The carts in their own colours, whatever the theme.
            out.push(Draw::Ink(None));
            for &(slot, side) in draw_order {
                let (tex, aspect) = carts[slot];
                let ch = if side == 0.0 { front_h } else { front_h * back };
                let cw = ch * aspect;
                let dx = side * front_h * widest * spread;
                let dy = if side == 0.0 { 0.0 } else { 0.5 * cw * turn.sin() };
                let (left, y0) = (cx + dx - cw / 2.0, base - ch - dy);
                // Opaque, chosen or not: a see-through cart muddies its label.
                if side == 0.0 {
                    out.push(Draw::Tex { x: left, y: y0, w: cw, h: ch, tex, alpha });
                } else {
                    out.push(Draw::Turned { x: left, y: y0, w: cw, h: ch, tex, alpha, turn: side * turn });
                }
            }
            out.push(Draw::Ink(ink));
            // Recents and Favorites: their sign in the corner.
            if special {
                if let Some(marks) = self.shelves_grid.marks {
                    let mark = marks[usize::from(id == FAVORITES_RACK)];
                    let r = PILL_R;
                    let (mx, my) = (x + w - r * 2.0 - 8.0, y + 8.0);
                    let disc = if light { [0.10, 0.10, 0.11] } else { [0.32, 0.32, 0.34] };
                    panel(out, corners, [mx, my, r * 2.0, r * 2.0], r, [disc[0], disc[1], disc[2], 0.95 * alpha], ink);
                    out.push(Draw::Ink(None));
                    fit(mark, mx + r, my + r, r * 1.3, r * 1.3, alpha, out);
                    out.push(Draw::Ink(ink));
                }
            }
            // The name, in a pill when chosen.
            if let Some(&face) = self.shelves_grid.labels.get(id) {
                if chosen {
                    let scale = (label_h / face.2.max(1) as f32).min((w - 28.0) / face.1.max(1) as f32);
                    let (fw, fh) = (face.1 as f32 * scale, face.2 as f32 * scale);
                    let pill = [cx - fw / 2.0 - 12.0, name_y - fh / 2.0 - 3.0, fw + 24.0, fh + 6.0];
                    let pc = [ink_rgb[0], ink_rgb[1], ink_rgb[2], alpha];
                    panel(out, corners, pill, (fh + 6.0) / 2.0, pc, ink);
                    out.push(Draw::Ink(Some(slot_gfx::InkMap { paper: pill_text, ink: pill_text })));
                    fit(face, cx, name_y, w - 28.0, label_h, alpha, out);
                } else {
                    out.push(Draw::Ink(ink));
                    fit(face, cx, name_y, w - 28.0, label_h, a * 0.92, out);
                }
                out.push(Draw::Ink(ink));
            }
            // How many games: the number alone, large, in a round pill in
            // the top-left corner, as the badge opposite; wider past 99.
            if let Some(Some((_, face))) = self.shelves_grid.counts.get(id) {
                let r = PILL_R;
                let th = r * 1.55;
                let tw = face.1 as f32 * th / face.2.max(1) as f32;
                let pw = (tw + 10.0).max(r * 2.0);
                let (px, py) = (x + 8.0, y + 8.0);
                let disc = if light { [0.10, 0.10, 0.11] } else { [0.32, 0.32, 0.34] };
                panel(out, corners, [px, py, pw, r * 2.0], r, [disc[0], disc[1], disc[2], 0.95 * alpha], ink);
                out.push(Draw::Ink(None));
                fit(*face, px + pw / 2.0, py + r, pw - 6.0, th, alpha, out);
                out.push(Draw::Ink(ink));
            }
        }
        if layout.scroll == Scroll::Across {
            out.push(Draw::Span(None));
        }
        // Scrolling down: a thin bar on the right says where the page is.
        if layout.scroll == Scroll::Down {
            let last = layout.cells.iter().map(|c| c[1] + c[3]).fold(0.0f32, f32::max);
            let (track_top, view) = (TOP - 2.0, BOTTOM + 6.0 - (TOP - 2.0));
            let total = (last - (TOP - 2.0)).max(view);
            let tone = if light { 0.0 } else { 1.0 };
            let bx = width - MARGIN / 2.0 - 1.5;
            rect(out, [bx, track_top, 3.0, view], [tone, tone, tone, 0.12 * alpha]);
            rect(out, [bx, track_top + view * scroll / total, 3.0, view * view / total], [tone, tone, tone, 0.6 * alpha]);
        }
        out.push(Draw::Ink(None));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// $SCRATCH_SHELVES (design check): the shelves page in Scroll and Grid,
    /// with every system, six, three and two, in Dark and Light, on the
    /// panel in $SCRATCH_PANEL. Labels from $SCRATCH_LABELS:
    /// <SYSTEM>__<title>.png.
    #[test]
    fn render_shelves() {
        let Ok(dir) = std::env::var("SCRATCH_SHELVES") else { return };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        if let Some((w, h)) = std::env::var("SCRATCH_PANEL").ok().as_deref()
            .and_then(|size| size.split_once('x'))
            .and_then(|(w, h)| Some((w.parse::<u64>().ok()?, h.parse::<u64>().ok()?))) {
            PANEL_SIZE.store(w << 32 | h, Ordering::Release);
        }
        let ext = |p: &str| match p {
            "GBA" => "gba", "GB" => "gb", "GBC" => "gbc", "NES" => "nes", "SNES" => "sfc", "MD" => "md",
            "SMS" => "sms", "GG" => "gg", "NGP" => "ngc", "WS" => "wsc", "LYNX" => "lnx", _ => "min",
        };
        let mut games: Vec<(String, String, Option<std::path::PathBuf>)> = Vec::new();
        if let Ok(labels) = std::env::var("SCRATCH_LABELS") {
            let mut files: Vec<_> = std::fs::read_dir(labels).unwrap().flatten().map(|f| f.path()).collect();
            files.sort();
            for file in files {
                let name = file.file_stem().unwrap().to_string_lossy().to_string();
                if let Some((system, title)) = name.split_once("__") {
                    games.push((system.to_owned(), title.to_owned(), Some(file.clone())));
                }
            }
        }
        for (system, title) in [("GBA", "Pokemon Emerald"), ("GB", "Tetris"), ("GBC", "Pokemon Crystal"),
            ("NES", "Super Mario Bros. 3"), ("SNES", "Super Metroid"), ("MD", "Sonic The Hedgehog"),
            ("SMS", "Alex Kidd in Miracle World"), ("GG", "Columns"), ("NGP", "Metal Slug - 1st Mission"),
            ("WS", "Gunpey"), ("LYNX", "California Games"), ("POKEMINI", "Pokemon Pinball Mini")] {
            if games.iter().filter(|g| g.0 == system).count() < 3 {
                for n in 0..3 { games.push((system.into(), format!("{title} {}", n + 1), None)); }
            }
        }
        let entries = |systems: &[&str]| -> Vec<RomEntry> {
            let json: Vec<String> = games.iter().enumerate().filter(|(_, g)| systems.contains(&g.0.as_str()))
                .map(|(i, (system, title, _))| format!(
                    r#"{{"title":{t},"filename":{f},"platform":"{system}","uri":"content://roms/{i}.{e}"}}"#,
                    t = serde_json::to_string(title).unwrap(),
                    f = serde_json::to_string(&format!("{title}.{}", ext(system))).unwrap(), e = ext(system)))
                .collect();
            serde_json::from_str(&format!("[{}]", json.join(","))).unwrap()
        };
        let all = ["GBA", "GB", "GBC", "NES", "SNES", "MD", "SMS", "GG", "NGP", "WS", "LYNX", "POKEMINI"];
        let sets: [(&str, &[&str], Platform, bool); 4] = [
            ("all", &all, Platform::Snes, true),
            ("six", &["GBA", "GB", "GBC", "NES", "SNES", "MD"], Platform::Gbc, true),
            ("three", &["GBA", "SNES", "MD"], Platform::Md, false),
            ("two", &["GBA", "SNES"], Platform::Snes, false),
        ];
        let _surface = slot_gfx::TestEgl::new();
        for (set, systems, selected, specials) in sets {
            menu_style::set_palette(None);
            menu_style::set_oled(false);
            let mut engine = Engine::new(1, Some(&entries(systems))).unwrap();
            engine.settings.appearance = crate::settings::Appearance::Dark;
            engine.settings.wallpaper = crate::settings::WallpaperPick::Default;
            engine.settings.show_recents = specials;
            // Each cart's own label.
            for sid in 0..engine.shelves.len() {
                for i in 0..engine.shelves[sid].carts.len() {
                    let title = engine.shelves[sid].carts[i].title.clone();
                    let system = engine.shelves[sid].carts[i].platform.system().dir;
                    if let Some((_, _, Some(file))) = games.iter().find(|g| g.1 == title && g.0 == system) {
                        engine.shelves[sid].carts[i].label = Some(file.clone());
                        engine.cart_render.invalidate((sid, i));
                    }
                }
            }
            if specials {
                let pick = |engine: &Engine, from: &[Platform]| -> Vec<slot_store::Cart> {
                    from.iter().filter_map(|p| engine.shelves[*p as usize].carts.first().cloned()).collect()
                };
                let recents = pick(&engine, &[Platform::Snes, Platform::Gba, Platform::Md, Platform::Gb]);
                let favorites = pick(&engine, &[Platform::Gbc, Platform::Nes, Platform::Md]);
                engine.shelves[RECENTS_RACK].update_carts(recents);
                engine.shelves[FAVORITES_RACK].update_carts(favorites);
            }
            for (theme, steps) in [("dark", 0), ("light", 2)] {
                for _ in 0..steps { engine.cycle_appearance(true); }
                engine.reload_wallpaper(None);
                for (style, grid) in [("scroll", false), ("grid", true)] {
                    engine.settings.shelves_grid = grid;
                    engine.active = selected as usize;
                    engine.overlay = ShelfOverlay::Shelves { rack: selected as usize };
                    engine.shelves_grid.opened = None;
                    for _ in 0..140 {
                        engine.draw(0, || {});
                        std::thread::sleep(std::time::Duration::from_millis(12));
                    }
                    let rgba = engine.gpu.read_frame();
                    let (w, h) = panel_size();
                    let file = std::io::BufWriter::new(std::fs::File::create(
                        dir.join(format!("{set}_{theme}_{style}.png"))).unwrap());
                    let mut enc = png::Encoder::new(file, w, h);
                    enc.set_color(png::ColorType::Rgba);
                    enc.set_depth(png::BitDepth::Eight);
                    enc.write_header().unwrap().write_image_data(&rgba).unwrap();
                }
            }
        }
    }

    #[test]
    fn showcase_tiles_grow_when_few_and_scroll_when_many() {
        let few = showcase(0, 3, 720.0, 1.0, false);
        assert_eq!(few.scroll, Scroll::Still);
        assert!(few.cells.iter().all(|c| c[1] == few.cells[0][1]), "three systems share one row");
        let two = showcase(0, 2, 720.0, 1.0, false);
        assert!(two.w > few.w);
        // Recents and Favorites keep their own row, a little apart.
        let mixed = showcase(2, 5, 960.0, 1.0, false);
        assert_eq!(mixed.line[..], [0, 0, 1, 1, 1]);
        assert!(mixed.cells[2][1] - mixed.cells[0][1] >= mixed.h + GAP + SPLIT - 0.01);
        let across = showcase(2, 14, 720.0, 1.0, false);
        assert_eq!(across.scroll, Scroll::Across);
        assert_eq!(across.line[..3], [0, 0, 1]);
        assert_eq!(across.scroll_for(0, 720.0), 0.0);
        let down = showcase(2, 14, 720.0, 1.0, true);
        assert_eq!(down.scroll, Scroll::Down);
        assert_eq!(down.scroll_for(0, 720.0), 0.0);
        assert!(down.scroll_for(13, 720.0) > 0.0);
        for (sh, width) in [(&few, 720.0), (&two, 720.0), (&mixed, 960.0), (&down, 720.0)] {
            for c in &sh.cells { assert!(c[0] >= MARGIN - 0.01 && c[0] + c[2] <= width - MARGIN + 0.01, "{c:?}"); }
        }
    }
    #[test]
    fn shelves_only_nonempty_and_specials_first() {
        let gba=Platform::ALL.iter().position(|p|*p==Platform::Gba).unwrap();
        let gb=Platform::ALL.iter().position(|p|*p==Platform::Gb).unwrap();
        let present=|r| [gba,gb,RECENTS_RACK,FAVORITES_RACK,VIDEO_RACK].contains(&r);
        let order=shelf_order(present,true);
        assert_eq!(&order[..2], &[RECENTS_RACK,FAVORITES_RACK]);
        assert_eq!(order[order.iter().position(|&r|r==gba).unwrap()+1],VIDEO_RACK);
        assert!(!shelf_order(present,false).contains(&RECENTS_RACK));
        assert!(shelf_order(|_|false,true).is_empty());
    }
    #[test]
    fn the_page_under_the_finger_chooses_its_column() {
        let across = showcase(2, 14, 720.0, 1.0, false);
        assert_eq!(column_at(&across, 0.0, 720.0), 0);
        // Resting on a column chooses it; past the end, the last one.
        let tile = tile_in_column(&across, 0, 3).unwrap();
        assert_eq!(column_at(&across, across.scroll_for(tile, 720.0), 720.0), 3);
        assert_eq!(column_at(&across, 1.0e6, 720.0), across.starts.len() - 1);
        // The same row in the next column; a short column gives its last.
        assert_eq!(tile_in_column(&across, 1, 1), Some(3));
        assert_eq!(tile_in_column(&across, 0, 0), Some(0));
    }

    #[test]
    fn the_dpad_goes_to_the_nearest_tile_that_way() {
        // Recents and Favorites on a row of their own over three systems.
        let still = showcase(2, 5, 960.0, 1.0, false).cells;
        assert_eq!(move_tile(&still, 0, 22), 1);
        assert_eq!(move_tile(&still, 1, 22), 1, "nothing further right");
        assert_eq!(move_tile(&still, 0, 20), 2);
        assert_eq!(move_tile(&still, 1, 20), 3);
        assert_eq!(move_tile(&still, 3, 19), 0);
        assert_eq!(move_tile(&still, 4, 19), 1);
        assert_eq!(move_tile(&still, 2, 19), 0);
        assert_eq!(move_tile(&still, 2, 102), 1);
        assert_eq!(move_tile(&still, 4, 103), 4);
        // Sideways: column by column, the specials' column first.
        let across = showcase(2, 14, 720.0, 1.0, false);
        assert_eq!(across.scroll, Scroll::Across);
        assert_eq!(move_tile(&across.cells, 0, 20), 1);
        assert_eq!(move_tile(&across.cells, 0, 22), 2);
        assert_eq!(move_tile(&across.cells, 2, 21), 0);
        assert_eq!(move_tile(&across.cells, 2, 20), 3);
        // Down: rows of three under the specials' row.
        let down = showcase(2, 14, 720.0, 1.0, true);
        assert_eq!(move_tile(&down.cells, 0, 20), 2);
        assert_eq!(move_tile(&down.cells, 2, 22), 3);
        assert_eq!(move_tile(&down.cells, 13, 20), 13);
    }
}
