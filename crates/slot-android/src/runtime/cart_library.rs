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
    /// SELECT: big carts, one row of two (kept in the settings).
    big: bool,
}

/// The chosen cart 5% over its room, growing upward into the margin above
/// it; the others at 90% (Siham 2026-11-09). More would reach the name of
/// the cart above in two-row grids.
const CHOSEN: f32 = 1.05;
const RESTING: f32 = 0.9;
/// Big carts: closer to one another, the chosen one at its room, the others
/// at 92% (Siham 2026-11-09).
const BIG_CHOSEN: f32 = 1.0;
const BIG_RESTING: f32 = 0.92;
/// Between a cart's bottom and its name, chosen or not: it grows upward.
const NAME_GAP: f32 = 8.0;
/// Seconds a cart takes to grow or shrink, as the carousel's.
const GROW_S: f32 = 0.12;

/// Columns and rows of the grid for the screen's width, and its cells.
struct Grid { x: f32, cols: usize, rows: usize, cell_w: f32, cell_h: f32 }

/// The grid for a system on the screen's width. Carts taller than wide (GB,
/// GBC, NES) sit in one row: a second one would shrink them by 40%. Wide
/// ones (GBA, SNES, MD) sit in two rows at about the same size as one.
/// The columns grow with the width: a square screen, 4:3 and 3:2, 5:3 to
/// 2:1, then 21:9 (Siham 2026-11-08). Big carts (SELECT): one row, a
/// column fewer than the tall carts' (two on most screens; Siham 2026-11-09).
fn grid(platform: Platform, big: bool) -> Grid {
    let width = out_w() as f32;
    let x = 18.0;
    let room = (width - 2.0 * x).max(200.0);
    let one_row = if width < 560.0 { 2 } else if width <= 900.0 { 3 } else { 4 };
    let (rows, cols) = match platform {
        _ if big => (1, (one_row - 1).max(2)),
        Platform::Gb | Platform::Gbc | Platform::Nes => (1, one_row),
        _ => (2, if width < 560.0 { 2 } else if width < 780.0 { 3 } else if width < 1040.0 { 4 } else { 5 }),
    };
    Grid { x, cols, rows, cell_w: room / cols as f32, cell_h: (GRID_BOTTOM - GRID_TOP) / rows as f32 }
}

/// L2/R2: the first cart of the previous or next letter, as Up/Down on the
/// shelf. L2 goes to the start of the cart's own letter first, and both
/// wrap around.
fn letter_target(carts: &[Cart], index: usize, dir: i32) -> usize {
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

impl Engine {
    /// Menu > Cart Studio: the library page, on the shelf's system.
    pub(super) fn open_cart_library(&mut self) -> bool {
        let systems = Platform::ALL.len().min(self.shelves.len());
        let system = if self.active < systems && !self.shelves[self.active].carts.is_empty() {
            self.active
        } else {
            match (0..systems).find(|&s| !self.shelves[s].carts.is_empty()) {
                Some(s) => s,
                None => {
                    set_message("No cartridges yet.".into());
                    return false;
                }
            }
        };
        let cursor = if system == self.active { self.shelves[system].index } else { 0 };
        self.cart_library = Some(CartLibrary {
            system, cursor, first_row: 0, labelled: Vec::new(), flags_stale: true,
            drafts: HashMap::new(), confirm: false, letter: None, letter_at: None,
            grown: (system, cursor), shrinking: None, grown_at: None,
            big: self.settings.cart_studio_big,
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
        let Some(library) = self.cart_library.as_mut() else { return };
        let carts = &self.shelves[library.system].carts;
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
        let g = grid(Platform::ALL[library.system], library.big);
        let row = library.cursor / g.cols;
        if row < library.first_row { library.first_row = row; }
        if row >= library.first_row + g.rows { library.first_row = row + 1 - g.rows; }
    }

    /// The carts the page shows, for the face loader to bring first.
    pub(super) fn cart_library_cells(&self) -> Vec<(usize, usize)> {
        if !matches!(self.overlay, ShelfOverlay::CartLibrary) { return Vec::new(); }
        let Some(library) = self.cart_library.as_ref() else { return Vec::new() };
        let g = grid(Platform::ALL[library.system], library.big);
        let n = self.shelves[library.system].carts.len();
        let first = library.first_row * g.cols;
        // The page, then the next row down.
        (first..(first + g.cols * (g.rows + 1)).min(n)).map(|i| (library.system, i)).collect()
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
        let n = self.shelves[library.system].carts.len();
        let cols = grid(Platform::ALL[library.system], library.big).cols;
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
                    if !self.shelves[next].carts.is_empty() { break; }
                }
                if next != library.system {
                    library.system = next;
                    library.cursor = 0;
                    library.first_row = 0;
                    library.flags_stale = true;
                    library.letter_at = None;
                }
            }
            104 | 105 => {
                let carts = &self.shelves[library.system].carts;
                if carts.is_empty() { return; }
                library.cursor = letter_target(carts, library.cursor, if code == 105 { 1 } else { -1 });
                let letter = slot_store::initial(&carts[library.cursor].stem);
                self.show_library_letter(letter);
            }
            100 => {
                self.save_library_drafts();
                return;
            }
            109 => {
                library.big = !library.big;
                library.first_row = 0;
                self.settings.cart_studio_big = library.big;
                self.persist_settings();
            }
            96 => {
                let Some(saved) = self.shelves[library.system].carts.get(library.cursor) else { return };
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
            let n = self.shelves[s].carts.len();
            if n > 0 { texts.push((format!("{} {n}", platform_key(Platform::ALL[s])), s == library.system)); }
        }
        texts.push((unsaved_note(library.drafts.len()), true));
        if let Some(count) = label_count(&library.labelled) { texts.push((count, false)); }
        for (_, i) in self.cart_library_cells() {
            texts.push((slot_ui::label_text(&self.shelves[library.system].carts[i]), true));
        }
        for (text, lit) in texts { self.controls_face(&text, lit); }
        for (key, label) in [("B", "BACK"), ("L1 R1", "SYSTEM"), ("L2 R2", "LETTER"), ("SELECT", "SIZE"), ("Y", "SAVE"), ("A", "EDIT"),
                             ("B", "DISCARD"), ("A", "KEEP EDITING")] {
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
                .map_or(45.0, |&(_, _, th)| 33.0 + th as f32 * 0.79 / 2.0);
            out.push(Draw::Tex { x: width - 32.0 - dw, y: title_mid - dh / 2.0, w: dw, h: dh, tex, alpha: 0.8 * fade });
        }
        // The systems, centred, the chosen one lit.
        let systems = Platform::ALL.len().min(self.shelves.len());
        let tabs: Vec<(usize, slot_gfx::TexId, f32, f32)> = (0..systems).filter_map(|s| {
            let n = self.shelves[s].carts.len();
            if n == 0 { return None; }
            face(&format!("{} {n}", platform_key(Platform::ALL[s])), s == library.system)
                .map(|(tex, w, h)| (s, tex, w as f32 * 0.5, h as f32 * 0.5))
        }).collect();
        let pad = 14.0;
        let full: f32 = tabs.iter().map(|t| t.2 + pad * 2.0).sum::<f32>() + 6.0 * tabs.len().saturating_sub(1) as f32;
        let scale = ((width - 64.0) / full.max(1.0)).min(1.0);
        let mut x = (width - full * scale) / 2.0;
        for (s, tex, w, h) in tabs {
            let cell = (w + pad * 2.0) * scale;
            if s == library.system {
                out.push(Draw::Rect { x, y: TABS_Y, w: cell, h: 28.0, colour: tint(1.0) });
            }
            out.push(Draw::Tex { x: x + pad * scale, y: TABS_Y + 14.0 - h * scale / 2.0, w: w * scale, h: h * scale, tex,
                alpha: if s == library.system { fade } else { 0.6 * fade } });
            x += cell + 6.0 * scale;
        }
        let shelf = &self.shelves[library.system];
        // The carts, the whole width.
        let g = grid(Platform::ALL[library.system], library.big);
        let first = library.first_row * g.cols;
        for i in first..(first + g.cols * g.rows).min(shelf.carts.len()) {
            let cart = &shelf.carts[i];
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
            match draft.map(|d| d.face).or_else(|| shelf.face(i)) {
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
                let mut ground = opening();
                ground[3] *= 0.65 * alpha;
                out.push(Draw::Rect { x: mx, y: my, w: side, h: side, colour: ground });
                out.push(Draw::Tex { x: (width - w) / 2.0, y: my + (side - h) / 2.0, w, h, tex, alpha });
            }
        }
        // The legend.
        out.push(Draw::Rect { x: 0.0, y: 420.0, w: width, h: 60.0, colour: opening() });
        let mut keys: Vec<(&str, &str)> = if library.confirm {
            vec![("B", "DISCARD"), ("A", "KEEP EDITING"), ("Y", "SAVE")]
        } else if library.drafts.is_empty() {
            vec![("B", "BACK"), ("L1 R1", "SYSTEM"), ("L2 R2", "LETTER"), ("SELECT", "SIZE"), ("A", "EDIT")]
        } else {
            vec![("B", "BACK"), ("L1 R1", "SYSTEM"), ("L2 R2", "LETTER"), ("SELECT", "SIZE"), ("A", "EDIT"), ("Y", "SAVE")]
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
    let (chosen, resting) = if library.big { (BIG_CHOSEN, BIG_RESTING) } else { (CHOSEN, RESTING) };
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
        let carts = shelf(&["1942", "Alleyway", "Asteroids", "Batman", "Zelda"]);
        assert_eq!(letter_target(&carts, 0, 1), 1);
        assert_eq!(letter_target(&carts, 1, 1), 3);
        assert_eq!(letter_target(&carts, 3, 1), 4);
        assert_eq!(letter_target(&carts, 4, 1), 0);
        // L2: the start of its own letter first, then the letter before.
        assert_eq!(letter_target(&carts, 2, -1), 1);
        assert_eq!(letter_target(&carts, 1, -1), 0);
        assert_eq!(letter_target(&carts, 0, -1), 4);
        // One letter only: nowhere to go.
        assert_eq!(letter_target(&shelf(&["Alpha", "Asteroids"]), 1, 1), 1);
    }
}
