//! Cart Studio's library page (Menu > Cart Studio): every cart of a system
//! with the ones that have no label marked, A to edit one in Cart Studio.
//! The editor's changes wait on the page, the carts already wearing them,
//! until Y saves them all; B leaves, asking first when some aren't saved.
use super::*;
use super::cart_studio::{platform_key, Outcome};
use slot_store::Cart;
use std::collections::HashMap;

/// The title, the systems' tabs and the carts, with room between them
/// (Siham 2026-11-09).
const GRID_TOP: f32 = 122.0;
const GRID_BOTTOM: f32 = 416.0;
/// The systems' tabs, under the title.
const TABS_Y: f32 = 78.0;
const NAME_H: f32 = 24.0;

/// A cart changed in the editor and not saved yet.
struct Draft {
    cart: Cart,
    outcome: Outcome,
    face: slot_gfx::TexId,
}

pub(super) struct CartLibrary {
    /// The system's shelf (Platform::ALL order).
    system: usize,
    /// The system's carts as (shelf, index), in name order: the GBA tab
    /// also has the GBA Video shelf's (Siham 2026-11-10: no tab of their own).
    view: Vec<(usize, usize)>,
    cursor: usize,
    first_row: usize,
    /// Per cart of the system: it has a label file.
    labelled: Vec<bool>,
    flags_stale: bool,
    /// The carts changed in the editor, by ROM URI.
    drafts: HashMap<String, Draft>,
    /// B with changes not saved: Y saves, B leaves without them.
    confirm: bool,
    /// L2/R2: the letter jumped to, shown big in the middle for a moment.
    letter: Option<(slot_gfx::TexId, u32, u32)>,
    letter_at: Option<Instant>,
    /// The chosen cart grows, the one left shrinks (Siham 2026-11-09):
    /// (system, cursor) drawn big, the one before, and since when.
    grown: (usize, usize),
    shrinking: Option<(usize, usize)>,
    grown_at: Option<Instant>,
    /// SELECT: the size step, one for every system (kept in the settings).
    size: u8,
}

/// The chosen cart 5% over its room, growing upward into the margin above
/// it; the others at 90% (Siham 2026-11-09). More would reach the name of
/// the cart above in two-row grids.
const CHOSEN: f32 = 1.05;
const RESTING: f32 = 0.9;
/// Two big carts (the first size step): closer to one another, the chosen
/// one at its room, the others at 92% (Siham 2026-11-09).
const BIG_CHOSEN: f32 = 1.0;
const BIG_RESTING: f32 = 0.92;
/// Between a cart's bottom and its name, chosen or not: it grows upward.
const NAME_GAP: f32 = 8.0;
/// Seconds a cart takes to grow or shrink, as the carousel's.
const GROW_S: f32 = 0.12;

/// Columns and rows of the grid for the screen's width, and its cells.
struct Grid { x: f32, cols: usize, rows: usize, cell_w: f32, cell_h: f32 }

/// SELECT's size steps for a system on the screen's width, as (rows,
/// columns), the first always two big carts (Siham 2026-11-10). Carts taller
/// than wide (GB, GBC, NES) take a second row only in the last step, where
/// they are about half their one-row size; wide ones (GBA, SNES, MD) sit in
/// two rows well. Widths: square (480), Retroid Pocket Classic (551), 4:3
/// (640), 3:2 like the KPA (720), 16:9 and wider.
fn size_steps(platform: Platform) -> &'static [(usize, usize)] {
    let width = out_w() as f32;
    let tall = matches!(platform, Platform::Gb | Platform::Gbc | Platform::Nes);
    match (tall, width) {
        (true, w) if w < 520.0 => &[(1, 2), (1, 3), (2, 3)],
        (true, w) if w < 680.0 => &[(1, 2), (1, 3), (2, 4)],
        (true, w) if w < 790.0 => &[(1, 2), (1, 4), (2, 5)],
        (true, _) => &[(1, 2), (1, 4), (2, 6)],
        (false, w) if w < 600.0 => &[(1, 2), (2, 2)],
        (false, w) if w < 790.0 => &[(1, 2), (1, 3), (2, 3)],
        (false, _) => &[(1, 2), (1, 3), (2, 4)],
    }
}

/// The step a system shows: the one kept, or its last when it has fewer
/// (wide carts on narrow screens have two).
fn size_step(platform: Platform, size: u8) -> usize {
    (size as usize).min(size_steps(platform).len() - 1)
}

/// The grid for a system at its size step, and its cells.
fn grid(platform: Platform, step: usize) -> Grid {
    let width = out_w() as f32;
    let x = 18.0;
    let room = (width - 2.0 * x).max(200.0);
    let layouts = size_steps(platform);
    let (rows, cols) = layouts[step.min(layouts.len() - 1)];
    Grid { x, cols, rows, cell_w: room / cols as f32, cell_h: (GRID_BOTTOM - GRID_TOP) / rows as f32 }
}

impl CartLibrary {
    fn step(&self) -> usize { size_step(Platform::ALL[self.system], self.size) }
    fn grid(&self) -> Grid { grid(Platform::ALL[self.system], self.step()) }
}

/// L2/R2: the first cart of the previous or next letter, as Up/Down on the
/// shelf. L2 goes to the start of the cart's own letter first, and both
/// wrap around.
fn letter_target(carts: &[&Cart], index: usize, dir: i32) -> usize {
    let n = carts.len();
    if n < 2 || index >= n { return index; }
    let initial = |i: usize| slot_store::initial(&carts[i].stem);
    let here = initial(index);
    if (0..n).all(|i| initial(i) == here) { return index; }
    let wrap = |i: i32| i.rem_euclid(n as i32) as usize;
    let start_of = |mut i: usize| {
        let letter = initial(i);
        while i > 0 && initial(i - 1) == letter { i -= 1; }
        i
    };
    if dir > 0 {
        let mut at = index;
        for _ in 0..n {
            at = wrap(at as i32 + 1);
            if initial(at) != here { break; }
        }
        at
    } else {
        let start = start_of(index);
        if start == index { start_of(wrap(start as i32 - 1)) } else { start }
    }
}

fn label_file(root: &std::path::Path, cart: &Cart) -> std::path::PathBuf {
    root.join("Labels").join(format!("{}.png", core_selection::key(&cart.rom.to_string_lossy())))
}

fn visuals_root() -> Option<std::path::PathBuf> {
    PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|(r, _)| r.clone())
}

/// A system's carts on the page, as (shelf, index) in name order.
fn view_in(shelves: &[Shelf], system: usize) -> Vec<(usize, usize)> {
    let mut racks = vec![system];
    if system == Platform::Gba as usize { racks.push(VIDEO_RACK); }
    let mut view: Vec<(usize, usize)> = racks.into_iter()
        .filter_map(|rack| shelves.get(rack).map(|shelf| (rack, shelf.carts.len())))
        .flat_map(|(rack, n)| (0..n).map(move |i| (rack, i))).collect();
    if view.iter().any(|&(rack, _)| rack != system) {
        view.sort_by_cached_key(|&(rack, i)| slot_store::sort_key(&shelves[rack].carts[i].stem));
    }
    view
}

/// How many carts a system's tab has.
fn count_in(shelves: &[Shelf], system: usize) -> usize {
    let video = if system == Platform::Gba as usize {
        shelves.get(VIDEO_RACK).map_or(0, |s| s.carts.len())
    } else { 0 };
    shelves.get(system).map_or(0, |s| s.carts.len()) + video
}

impl Engine {
    fn library_view(&self, system: usize) -> Vec<(usize, usize)> { view_in(&self.shelves, system) }
    fn library_count(&self, system: usize) -> usize { count_in(&self.shelves, system) }

    /// Menu > Cart Studio: the library page, on the shelf's system.
    pub(super) fn open_cart_library(&mut self) -> bool {
        // Always the first tab with carts, on its first cart: X on the
        // carousel edits the cart in hand (Siham 2026-11-10).
        let systems = Platform::ALL.len().min(self.shelves.len());
        let Some(system) = (0..systems).find(|&s| self.library_count(s) > 0) else {
            set_message("No cartridges yet.".into());
            return false;
        };
        let cursor = 0;
        let view = self.library_view(system);
        self.cart_library = Some(CartLibrary {
            system, view, cursor, first_row: 0, labelled: Vec::new(), flags_stale: true,
            drafts: HashMap::new(), confirm: false, letter: None, letter_at: None,
            grown: (system, cursor), shrinking: None, grown_at: None,
            size: self.settings.cart_studio_size,
        });
        self.keep_library_cursor_visible();
        self.overlay = ShelfOverlay::CartLibrary;
        true
    }

    /// Back from the editor.
    pub(super) fn return_to_cart_library(&mut self) {
        if let Some(library) = self.cart_library.as_mut() { library.flags_stale = true; }
        self.overlay = ShelfOverlay::CartLibrary;
    }

    /// The editor's changes to a cart, kept on the page until Save. A cart
    /// edited twice keeps its first new label unless the second gives one.
    pub(super) fn keep_library_draft(&mut self, cart: Cart, mut outcome: Outcome, face: &slot_ui::CartFace) {
        let uri = cart.rom.to_string_lossy().into_owned();
        let old = self.cart_library.as_mut().and_then(|l| l.drafts.remove(&uri));
        let tex = match &old {
            Some(old) => { self.gpu.update_face(old.face, face); old.face }
            None => self.studio_texture(face),
        };
        if let Some(old) = old {
            if outcome.label.is_none() { outcome.label = old.outcome.label; }
            if outcome.shell.is_none() { outcome.shell = old.outcome.shell; }
        }
        if let Some(library) = self.cart_library.as_mut() {
            library.drafts.insert(uri, Draft { cart, outcome, face: tex });
        }
    }

    fn refresh_library_flags(&mut self) {
        let root = visuals_root();
        let Some(system) = self.cart_library.as_ref().map(|l| l.system) else { return };
        let view = self.library_view(system);
        let Some(library) = self.cart_library.as_mut() else { return };
        library.view = view;
        let carts: Vec<&Cart> = library.view.iter().map(|&(rack, i)| &self.shelves[rack].carts[i]).collect();
        library.labelled = carts.iter()
            .map(|cart| {
                library.drafts.get(cart.rom.to_string_lossy().as_ref()).is_some_and(|d| d.outcome.label.is_some())
                    || root.as_deref().is_some_and(|r| label_file(r, cart).is_file())
            })
            .collect();
        library.cursor = library.cursor.min(carts.len().saturating_sub(1));
        library.flags_stale = false;
    }

    fn keep_library_cursor_visible(&mut self) {
        let Some(library) = self.cart_library.as_mut() else { return };
        let g = library.grid();
        let row = library.cursor / g.cols;
        if row < library.first_row { library.first_row = row; }
        if row >= library.first_row + g.rows { library.first_row = row + 1 - g.rows; }
    }

    /// The carts the page shows, for the face loader to bring first.
    pub(super) fn cart_library_cells(&self) -> Vec<(usize, usize)> {
        if !matches!(self.overlay, ShelfOverlay::CartLibrary) { return Vec::new(); }
        let Some(library) = self.cart_library.as_ref() else { return Vec::new() };
        let g = library.grid();
        let n = library.view.len();
        let first = library.first_row * g.cols;
        // The page, then the next row down.
        (first..(first + g.cols * (g.rows + 1)).min(n)).map(|i| library.view[i]).collect()
    }

    /// Y: every change of the session saved, the carts redrawn in place.
    fn save_library_drafts(&mut self) {
        let Some(library) = self.cart_library.as_mut() else { return };
        let drafts: Vec<(String, Draft)> = library.drafts.drain().collect();
        library.flags_stale = true;
        if drafts.is_empty() { return; }
        let mut saved = 0;
        for (uri, draft) in drafts {
            if self.commit_cart(&uri, &draft.outcome) { saved += 1; }
            self.studio_pool.push(draft.face);
        }
        if saved > 0 {
            set_message(if saved == 1 { "1 cartridge saved.".into() } else { format!("{saved} cartridges saved.") });
        }
    }

    /// Leaving the page: the changes not saved are dropped.
    fn leave_cart_library(&mut self, to: ShelfOverlay) {
        if let Some(library) = self.cart_library.as_mut() {
            let mut faces: Vec<slot_gfx::TexId> = library.drafts.drain().map(|(_, d)| d.face).collect();
            faces.extend(library.letter.take().map(|(tex, _, _)| tex));
            library.confirm = false;
            library.letter_at = None;
            self.studio_pool.extend(faces);
        }
        self.overlay = to;
    }

    pub(super) fn cart_library_key(&mut self, code: i32) {
        let Some(library) = self.cart_library.as_mut() else {
            self.overlay = ShelfOverlay::None;
            return;
        };
        if library.confirm {
            match code {
                100 => { self.save_library_drafts(); self.leave_cart_library(ShelfOverlay::Menu { row: 1 }); }
                97 => self.leave_cart_library(ShelfOverlay::Menu { row: 1 }),
                108 => self.leave_cart_library(ShelfOverlay::None),
                96 => library.confirm = false,
                _ => {}
            }
            return;
        }
        let n = library.view.len();
        let cols = library.grid().cols;
        match code {
            21 if library.cursor > 0 => library.cursor -= 1,
            22 if library.cursor + 1 < n => library.cursor += 1,
            19 if library.cursor >= cols => library.cursor -= cols,
            20 if library.cursor + cols < n => library.cursor += cols,
            20 if library.cursor / cols < n.saturating_sub(1) / cols => library.cursor = n - 1,
            102 | 103 => {
                let systems = Platform::ALL.len().min(self.shelves.len());
                let step = if code == 103 { 1 } else { systems - 1 };
                let mut next = library.system;
                for _ in 0..systems {
                    next = (next + step) % systems;
                    if count_in(&self.shelves, next) > 0 { break; }
                }
                if next != library.system {
                    library.view = view_in(&self.shelves, next);
                    library.system = next;
                    library.cursor = 0;
                    library.first_row = 0;
                    library.flags_stale = true;
                    library.letter_at = None;
                }
            }
            104 | 105 => {
                let carts: Vec<&Cart> = library.view.iter().map(|&(rack, i)| &self.shelves[rack].carts[i]).collect();
                if carts.is_empty() { return; }
                library.cursor = letter_target(&carts, library.cursor, if code == 105 { 1 } else { -1 });
                let letter = slot_store::initial(&carts[library.cursor].stem);
                self.show_library_letter(letter);
            }
            100 => {
                self.save_library_drafts();
                return;
            }
            109 => {
                // The next size step, back to two big carts after the last.
                let count = size_steps(Platform::ALL[library.system]).len();
                let next = (library.step() + 1) % count;
                // Every system moves to it together (Siham 2026-11-10).
                library.size = next as u8;
                library.first_row = 0;
                self.settings.cart_studio_size = library.size;
                self.persist_settings();
                self.keep_library_cursor_visible();
            }
            96 => {
                let Some(&(rack, index)) = library.view.get(library.cursor) else { return };
                let Some(saved) = self.shelves[rack].carts.get(index) else { return };
                let uri = saved.rom.to_string_lossy().into_owned();
                let (cart, pending) = match library.drafts.get(&uri) {
                    Some(draft) => (draft.cart.clone(), draft.outcome.label.clone()),
                    None => (saved.clone(), None),
                };
                self.open_cart_studio_on(cart, pending, true);
                return;
            }
            97 | 108 if !library.drafts.is_empty() => library.confirm = true,
            97 => self.leave_cart_library(ShelfOverlay::Menu { row: 1 }),
            108 => self.leave_cart_library(ShelfOverlay::None),
            _ => {}
        }
        self.keep_library_cursor_visible();
    }

    /// The letter L2/R2 landed on, big in the middle of the page.
    fn show_library_letter(&mut self, letter: char) {
        let face = slot_ui::platform_name_face(&letter.to_string());
        let old = self.cart_library.as_mut().and_then(|l| l.letter.take()).map(|(tex, _, _)| tex);
        let tex = match old.or_else(|| self.studio_pool.pop()) {
            Some(tex) => { self.gpu.update_face(tex, &face); tex }
            None => self.gpu.create_face(&face),
        };
        if let Some(library) = self.cart_library.as_mut() {
            library.letter = Some((tex, face.w, face.h));
            library.letter_at = Some(Instant::now());
        }
    }

    /// Before drawing: the page's faces.
    pub(super) fn prepare_cart_library(&mut self) {
        if !matches!(self.overlay, ShelfOverlay::CartLibrary) { return; }
        if self.cart_library.as_ref().is_some_and(|l| l.flags_stale) { self.refresh_library_flags(); }
        if let Some(library) = self.cart_library.as_mut() {
            let now = (library.system, library.cursor);
            if library.grown != now {
                // A new system's first cart is simply big; within one, the
                // two carts trade sizes.
                library.shrinking = (library.grown.0 == now.0).then_some(library.grown);
                library.grown = now;
                library.grown_at = Some(Instant::now());
            }
        }
        let Some(library) = self.cart_library.as_ref() else { return };
        let mut texts: Vec<(String, bool)> = vec![("CART STUDIO".into(), true), ("NO LABEL".into(), true),
            ("EDITED".into(), true)];
        let systems = Platform::ALL.len().min(self.shelves.len());
        for s in 0..systems {
            let n = self.library_count(s);
            if n > 0 { texts.push((format!("{} {n}", platform_key(Platform::ALL[s])), s == library.system)); }
        }
        texts.push((unsaved_note(library.drafts.len()), true));
        if let Some(count) = label_count(&library.labelled) { texts.push((count, false)); }
        for (rack, i) in self.cart_library_cells() {
            texts.push((slot_ui::label_text(&self.shelves[rack].carts[i]), true));
        }
        for (text, lit) in texts { self.controls_face(&text, lit); }
        for (key, label) in [("B", "BACK"), ("L2 R2", "LETTER"), ("SELECT", "SIZE"), ("Y", "SAVE"), ("A", "EDIT"),
                             ("B", "DISCARD"), ("A", "KEEP EDITING"), ("L1", ""), ("R1", "")] {
            self.controls_hint(key, label);
        }
    }

    pub(super) fn draw_cart_library(&self, out: &mut Vec<Draw>) {
        let Some(library) = self.cart_library.as_ref() else { return };
        self.draw_menu_ground(0.0, out);
        let width = out_w() as f32;
        let fade = slot_ui::ease((self.menu_opened.elapsed().as_secs_f32() / 0.16).clamp(0.0, 1.0));
        let face = |text: &str, lit: bool| self.controls_faces.get(&Self::face_key(text, lit)).copied();
        let tint = |a: f32| { let mut c = edge(); c[3] *= a * fade; c };
        self.draw_page_title(out, "CART STUDIO", fade);
        // How many of this system's carts still lack a label, right of the
        // title (Siham 2026-11-09): live, like the NO LABEL badges.
        if let Some((tex, w, h)) = label_count(&library.labelled).and_then(|t| face(&t, false)) {
            let scale = 0.56f32.min((width * 0.4) / w.max(1) as f32);
            let (dw, dh) = (w as f32 * scale, h as f32 * scale);
            let title_mid = self.controls_faces.get(&Self::face_key("CART STUDIO", true))
                .map_or(menu_look::TITLE_Y + 12.0, |&(_, _, th)| menu_look::TITLE_Y + th as f32 * 0.79 / 2.0);
            out.push(Draw::Tex { x: width - 32.0 - dw, y: title_mid - dh / 2.0, w: dw, h: dh, tex, alpha: 0.8 * fade });
        }
        // The systems, the chosen one lit, between L1 and R1.
        let systems = Platform::ALL.len().min(self.shelves.len());
        let tabs: Vec<(bool, slot_gfx::TexId, f32, f32)> = (0..systems).filter_map(|s| {
            let n = self.library_count(s);
            if n == 0 { return None; }
            face(&format!("{} {n}", platform_key(Platform::ALL[s])), s == library.system)
                .map(|(tex, w, h)| (s == library.system, tex, w as f32 * 0.5, h as f32 * 0.5))
        }).collect();
        self.draw_system_tabs(out, &tabs, TABS_Y, fade);
        // The carts, the whole width.
        let g = library.grid();
        let first = library.first_row * g.cols;
        for i in first..(first + g.cols * g.rows).min(library.view.len()) {
            let (rack, index) = library.view[i];
            let shelf = &self.shelves[rack];
            let cart = &shelf.carts[index];
            let draft = library.drafts.get(cart.rom.to_string_lossy().as_ref());
            let shown = draft.map_or(cart, |d| &d.cart);
            let (col, row) = ((i - first) % g.cols, (i - first) / g.cols);
            let (cx, cy) = (g.x + col as f32 * g.cell_w, GRID_TOP + row as f32 * g.cell_h);
            let mut name_y = cy + g.cell_h - NAME_H - 4.0;
            let (room_w, room_h) = (g.cell_w - 12.0, name_y - NAME_GAP - (cy + 4.0));
            let (bw, bh) = slot_ui::cart_box_for(shown);
            let fit = (room_w / bw as f32).min(room_h / bh as f32);
            // A cart its width limits sits in the middle of its cell, name and
            // all, rather than on the bottom (Siham 2026-11-09).
            name_y -= ((room_h - bh as f32 * fit) / 2.0).max(0.0);
            let s = fit * cart_size(library, i);
            let (dw, dh) = (bw as f32 * s, bh as f32 * s);
            let (ix, iy) = (cx + (g.cell_w - dw) / 2.0, name_y - NAME_GAP - dh);
            match draft.map(|d| d.face).or_else(|| shelf.face(index)) {
                Some(tex) => out.push(Draw::Tex { x: ix, y: iy, w: dw, h: dh, tex, alpha: fade }),
                None => out.push(Draw::Rect { x: ix, y: iy, w: dw, h: dh, colour: tint(0.5) }),
            }
            let badge = if draft.is_some() { Some("EDITED") }
                else if !library.labelled.get(i).copied().unwrap_or(true) { Some("NO LABEL") } else { None };
            if let Some((tex, w, h)) = badge.and_then(|b| face(b, true)) {
                let s = 0.42f32.min((dw - 8.0).max(10.0) / w.max(1) as f32);
                let (tw, th) = (w as f32 * s, h as f32 * s);
                let (tx, ty) = (ix + (dw - tw) / 2.0, iy + dh - th - 10.0);
                out.push(Draw::Rect { x: tx - 6.0, y: ty - 3.0, w: tw + 12.0, h: th + 6.0, colour: [0.0, 0.0, 0.0, 0.72 * fade] });
                out.push(Draw::Tex { x: tx, y: ty, w: tw, h: th, tex, alpha: fade });
            }
            if let Some((tex, w, h)) = face(&slot_ui::label_text(cart), true) {
                let s = 0.5f32.min((g.cell_w - 14.0) / w.max(1) as f32);
                let tw = w as f32 * s;
                out.push(Draw::Tex { x: cx + (g.cell_w - tw) / 2.0, y: name_y,
                    w: tw, h: h as f32 * s, tex, alpha: if i == library.cursor { fade } else { 0.7 * fade } });
            }
        }
        // B with changes not saved: the question over the grid.
        if library.confirm {
            out.push(Draw::Rect { x: 0.0, y: 60.0, w: width, h: 360.0, colour: [0.0, 0.0, 0.0, 0.6 * fade] });
            if let Some((tex, w, h)) = face(&unsaved_note(library.drafts.len()), true) {
                let s = 0.8f32.min((width - 60.0) / w.max(1) as f32);
                let dw = w as f32 * s;
                out.push(Draw::Tex { x: (width - dw) / 2.0, y: 220.0, w: dw, h: h as f32 * s, tex, alpha: fade });
            }
        }
        // The letter L2/R2 jumped to, as the shelf shows it: in, held, out.
        if let (Some((tex, w, h)), Some(at)) = (library.letter, library.letter_at) {
            let elapsed = at.elapsed().as_millis() as u64;
            if platform_name_running(elapsed) {
                let alpha = slot_name_alpha(elapsed) * fade;
                let (w, h) = (w as f32, h as f32);
                let side = w.max(h) + 24.0;
                let (mx, my) = ((width - side) / 2.0, (GRID_TOP + GRID_BOTTOM - side) / 2.0);
                // See-through, the carts showing behind (Siham 2026-11-08).
                let mut ground = menu_ground();
                ground[3] *= 0.65 * alpha;
                out.push(Draw::Rect { x: mx, y: my, w: side, h: side, colour: ground });
                out.push(Draw::Tex { x: (width - w) / 2.0, y: my + (side - h) / 2.0, w, h, tex, alpha });
            }
        }
        // The legend.
        out.push(Draw::Rect { x: 0.0, y: 420.0, w: width, h: 60.0, colour: menu_ground() });
        let mut keys: Vec<(&str, &str)> = if library.confirm {
            vec![("B", "DISCARD"), ("A", "KEEP EDITING"), ("Y", "SAVE")]
        } else if library.drafts.is_empty() {
            vec![("B", "BACK"), ("L2 R2", "LETTER"), ("SELECT", "SIZE"), ("A", "EDIT")]
        } else {
            vec![("B", "BACK"), ("L2 R2", "LETTER"), ("SELECT", "SIZE"), ("A", "EDIT"), ("Y", "SAVE")]
        };
        // A narrow screen drops the obvious hints, never the spacing: A then B
        // work the same everywhere; SELECT goes last, seen before any edit
        // (Siham 2026-11-09).
        let hints_of = |keys: &[(&str, &str)]| -> Vec<(slot_gfx::TexId, u32)> {
            keys.iter().filter_map(|key| self.controls_hint_faces.get(key).copied()).collect()
        };
        for dropped in ["A", "B", "SELECT"] {
            if !legend_too_wide(&hints_of(&keys)) { break; }
            keys.retain(|(key, _)| *key != dropped);
        }
        let hints = hints_of(&keys);
        for (tex, w, x) in centred_hints(&hints, LEGEND_GAP) {
            out.push(Draw::Tex { x, y: self.legend_y(), w: w as f32, h: HINT_H as f32, tex, alpha: fade });
        }
    }
}

/// The gap between two tabs, and between a tab and L1 or R1.
const TAB_GAP: f32 = 6.0;
const TAB_PAD: f32 = 14.0;
const KEY_GAP: f32 = 10.0;

/// The tabs shown when they don't all fit: the chosen one and as many of
/// its neighbours as `room` holds, both sides in turn. `start..end`.
pub(super) fn tab_window(cells: &[f32], chosen: usize, room: f32) -> (usize, usize) {
    let n = cells.len();
    if n == 0 { return (0, 0); }
    let chosen = chosen.min(n - 1);
    let (mut start, mut end, mut used) = (chosen, chosen + 1, cells[chosen]);
    loop {
        let mut grew = false;
        if end < n && used + TAB_GAP + cells[end] <= room {
            used += TAB_GAP + cells[end];
            end += 1;
            grew = true;
        }
        if start > 0 && used + TAB_GAP + cells[start - 1] <= room {
            start -= 1;
            used += TAB_GAP + cells[start];
            grew = true;
        }
        if !grew { break; }
    }
    (start, end)
}

impl Engine {
    /// The systems' tabs at `y`, centred, L1 left of the first one shown and
    /// R1 right of the last (Siham 2026-11-10: out of the legend). Tabs that
    /// don't fit scroll with the chosen one. `tabs`: chosen, face, size.
    pub(super) fn draw_system_tabs(&self, out: &mut Vec<Draw>, tabs: &[(bool, slot_gfx::TexId, f32, f32)],
                                   y: f32, fade: f32) {
        if tabs.is_empty() { return; }
        let width = out_w() as f32;
        let key = |k: &'static str| self.controls_hint_faces.get(&(k, "")).copied();
        let (l1, r1) = (key("L1"), key("R1"));
        let key_w = |k: Option<(slot_gfx::TexId, u32)>| k.map_or(0.0, |(_, w)| w as f32 + KEY_GAP);
        let room = width - 64.0 - key_w(l1) - key_w(r1);
        let cells: Vec<f32> = tabs.iter().map(|t| t.2 + TAB_PAD * 2.0).collect();
        let chosen = tabs.iter().position(|t| t.0).unwrap_or(0);
        let (start, end) = tab_window(&cells, chosen, room);
        let strip = cells[start..end].iter().sum::<f32>() + TAB_GAP * (end - start).saturating_sub(1) as f32;
        let mut x = ((width - strip - key_w(l1) - key_w(r1)) / 2.0).max(32.0);
        let mid = y + 14.0;
        let mut tint = edge();
        tint[3] *= fade;
        // Only one system: nothing to switch to, no keys.
        let keys = tabs.len() > 1;
        if let (true, Some((tex, w))) = (keys, l1) {
            let h = slot_ui::BUTTON_H as f32;
            out.push(Draw::Tex { x, y: mid - h / 2.0, w: w as f32, h, tex, alpha: fade });
        }
        if keys { x += key_w(l1); }
        for (i, &(lit, tex, w, h)) in tabs.iter().enumerate().take(end).skip(start) {
            let cell = cells[i];
            if lit { out.push(Draw::Rect { x, y, w: cell, h: 28.0, colour: tint }); }
            out.push(Draw::Tex { x: x + TAB_PAD, y: mid - h / 2.0, w, h, tex,
                alpha: if lit { fade } else { 0.6 * fade } });
            x += cell + TAB_GAP;
        }
        if let (true, Some((tex, w))) = (keys, r1) {
            let h = slot_ui::BUTTON_H as f32;
            out.push(Draw::Tex { x: x - TAB_GAP + KEY_GAP, y: mid - h / 2.0, w: w as f32, h, tex, alpha: fade });
        }
    }
}

/// The label count right of the title: "12 carts missing a label", or
/// "All 48 carts labelled" once every cart has one (Siham 2026-11-09).
fn label_count(labelled: &[bool]) -> Option<String> {
    if labelled.is_empty() { return None; }
    match labelled.iter().filter(|l| !**l).count() {
        0 if labelled.len() == 1 => Some("1 cart labelled".into()),
        0 => Some(format!("All {} carts labelled", labelled.len())),
        1 => Some("1 cart missing a label".into()),
        n => Some(format!("{n} carts missing a label")),
    }
}

/// How big cart `i` is drawn: the chosen one at CHOSEN, the others at
/// RESTING, easing between the two as the cursor moves. No cart is chosen
/// while the unsaved question is up.
fn cart_size(library: &CartLibrary, i: usize) -> f32 {
    let t = library.grown_at.map_or(1.0, |at| slot_ui::ease((at.elapsed().as_secs_f32() / GROW_S).clamp(0.0, 1.0)));
    let me = (library.system, i);
    let (chosen, resting) = if library.step() == 0 { (BIG_CHOSEN, BIG_RESTING) } else { (CHOSEN, RESTING) };
    if library.confirm { resting }
    else if me == library.grown { resting + (chosen - resting) * t }
    else if Some(me) == library.shrinking { chosen - (chosen - resting) * t }
    else { resting }
}

fn unsaved_note(n: usize) -> String {
    match n {
        1 => "1 cartridge not saved".into(),
        n => format!("{n} cartridges not saved"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn label_count_reads_missing_or_all() {
        assert_eq!(super::label_count(&[]), None);
        assert_eq!(super::label_count(&[true, false, false]).as_deref(), Some("2 carts missing a label"));
        assert_eq!(super::label_count(&[true, false]).as_deref(), Some("1 cart missing a label"));
        assert_eq!(super::label_count(&[true; 48]).as_deref(), Some("All 48 carts labelled"));
        assert_eq!(super::label_count(&[true]).as_deref(), Some("1 cart labelled"));
    }

    use super::*;

    fn shelf(stems: &[&str]) -> Vec<Cart> {
        stems.iter().map(|stem| Cart {
            platform: Platform::Gb, stem: (*stem).into(), title: (*stem).into(), code: String::new(),
            rom: std::path::PathBuf::from(*stem), label: None, shell: None, battery: false,
        }).collect()
    }

    #[test]
    fn l2_r2_go_letter_to_letter_and_wrap() {
        let owned = shelf(&["1942", "Alleyway", "Asteroids", "Batman", "Zelda"]);
        let carts: Vec<&Cart> = owned.iter().collect();
        assert_eq!(letter_target(&carts, 0, 1), 1);
        assert_eq!(letter_target(&carts, 1, 1), 3);
        assert_eq!(letter_target(&carts, 3, 1), 4);
        assert_eq!(letter_target(&carts, 4, 1), 0);
        // L2: the start of its own letter first, then the letter before.
        assert_eq!(letter_target(&carts, 2, -1), 1);
        assert_eq!(letter_target(&carts, 1, -1), 0);
        assert_eq!(letter_target(&carts, 0, -1), 4);
        // One letter only: nowhere to go.
        let one = shelf(&["Alpha", "Asteroids"]);
        assert_eq!(letter_target(&one.iter().collect::<Vec<_>>(), 1, 1), 1);
    }
}
