//! The menus' shared look: the rounded bar under the selected row, a long
//! page's sliding rows and its scroll bar, the path line over the title, and
//! the cart beside the main and game menus.
use super::*;
use crate::controls::VISIBLE_ROWS;

/// An unselected row's ink, against the selected row's.
pub(super) const ROW_REST: f32 = 0.58;
/// A page's title: where it sits under its path line.
pub(super) const TITLE_Y: f32 = 38.0;
const PATH_Y: f32 = 20.0;
/// The path line's ink starts where the title's does.
const PATH_X: f32 = 44.0;
/// The side cart needs this much room beside the rows.
const SIDE_MIN: f32 = 236.0;
const SIDE_MAX: f32 = 340.0;

/// Where a row sits below the first row's place while its page is `scroll`
/// down a list that can go `max`, and how much of it shows: a row sliding
/// out of the six fades away, and the row at an edge dims while more rows
/// wait beyond it. None once it is out of sight.
pub(super) fn row_place(index: usize, count: usize, scroll: f32, max: f32) -> Option<(f32, f32)> {
    let dy = index as f32 * QUICK_PITCH - scroll;
    let last = (count.min(VISIBLE_ROWS).max(1) - 1) as f32 * QUICK_PITCH;
    let outside = (-dy).max(dy - last).max(0.0);
    if outside >= QUICK_PITCH { return None; }
    let mut shows = (1.0 - outside / QUICK_PITCH).powi(2);
    if max > 0.0 {
        let half = QUICK_PITCH / 2.0;
        let at_top = (1.0 - dy.abs() / half).clamp(0.0, 1.0);
        let at_bottom = (1.0 - (dy - last).abs() / half).clamp(0.0, 1.0);
        shows *= 1.0 - 0.5 * at_top * (scroll / half).clamp(0.0, 1.0);
        shows *= 1.0 - 0.5 * at_bottom * ((max - scroll) / half).clamp(0.0, 1.0);
    }
    Some((dy, shows))
}

impl Engine {
    /// How far the shown page's list is scrolled, and how far it can go.
    pub(super) fn menu_scroll(&self, count: usize) -> (f32, f32) {
        let max = count.saturating_sub(VISIBLE_ROWS) as f32 * QUICK_PITCH;
        (self.menu_scroll_y.clamp(0.0, max), max)
    }

    pub(super) fn menu_row_place(&self, index: usize, count: usize) -> Option<(f32, f32)> {
        let (scroll, max) = self.menu_scroll(count);
        row_place(index, count, scroll, max)
    }

    /// The selected row's bar from `x0` to `x1`, its top at `y`.
    pub(super) fn draw_menu_pill(&self, x0: f32, x1: f32, y: f32, alpha: f32, out: &mut Vec<Draw>) {
        let [(left, lw, h), (middle, _, _), (right, rw, _)] = self.menu_pill;
        let (x0, x1) = (x0.round(), x1.round());
        let (lw, rw, h) = (lw as f32, rw as f32, h as f32);
        if x1 - x0 < lw + rw { return; }
        out.push(Draw::Tex { x: x0, y, w: lw, h, tex: left, alpha });
        out.push(Draw::Tex { x: x0 + lw, y, w: x1 - x0 - lw - rw, h, tex: middle, alpha });
        out.push(Draw::Tex { x: x1 - rw, y, w: rw, h, tex: right, alpha });
    }

    /// The selected row's bar on a page of `count` rows whose first row is
    /// at `top`: it follows the cursor, and slides with a long page's list.
    pub(super) fn draw_menu_cursor(&self, top: f32, selected: usize, count: usize, x1: f32,
                                   alpha: f32, out: &mut Vec<Draw>) {
        let (scroll, _) = self.menu_scroll(count);
        let shown = count.min(VISIBLE_ROWS).max(1) as f32;
        let cursor = if self.menu_cursor_y.is_finite() { self.menu_cursor_y }
            else { top + selected as f32 * QUICK_PITCH };
        let y = (cursor - scroll).clamp(top - QUICK_PITCH, top + shown * QUICK_PITCH);
        self.draw_menu_pill(PILL_LEFT, x1, y + (QUICK_PITCH - PILL_H as f32) / 2.0, alpha, out);
    }

    /// Hide what a long page's sliding rows leave above and below its six.
    pub(super) fn draw_menu_bands(&self, top: f32, count: usize, out: &mut Vec<Draw>) {
        if count <= VISIBLE_ROWS { return; }
        let width = out_w() as f32;
        let bottom = top + VISIBLE_ROWS as f32 * QUICK_PITCH - 2.0;
        out.push(Draw::Rect { x: 0.0, y: 0.0, w: width, h: top + 2.0, colour: menu_ground() });
        out.push(Draw::Rect { x: 0.0, y: bottom, w: width, h: OUT_H as f32 - bottom, colour: menu_ground() });
    }

    /// A long page's scroll bar, right of its rows.
    pub(super) fn draw_menu_scrollbar(&self, top: f32, count: usize, fade: f32, out: &mut Vec<Draw>) {
        if count <= VISIBLE_ROWS { return; }
        let (scroll, _) = self.menu_scroll(count);
        let x = out_w() as f32 - 12.0;
        let (y, h) = (top + 8.0, VISIBLE_ROWS as f32 * QUICK_PITCH - 16.0);
        let all = count as f32 * QUICK_PITCH;
        out.push(Draw::Rect { x, y, w: 3.0, h, colour: [1.0, 1.0, 1.0, 0.07 * fade] });
        out.push(Draw::Rect {
            x, y: (y + scroll / all * h).round(), w: 3.0,
            h: (VISIBLE_ROWS as f32 / count as f32 * h).round(),
            colour: [0.56, 0.56, 0.60, fade],
        });
    }

    /// A page's path line, then its title under it.
    pub(super) fn draw_menu_header(&self, path: &'static str,
                                   title: Option<(slot_gfx::TexId, u32, u32)>,
                                   fade: f32, out: &mut Vec<Draw>) {
        let width = out_w() as f32;
        if let Some(&(tex, w, h)) = self.menu_paths.get(path) {
            let scale = 1.0f32.min((width - 2.0 * PATH_X) / w.max(1) as f32);
            out.push(Draw::Tex { x: PATH_X, y: PATH_Y, w: w as f32 * scale, h: h as f32 * scale,
                tex, alpha: 0.5 * fade });
        }
        if let Some((tex, w, h)) = title.filter(|t| t.1 > 0) {
            let scale = 0.79f32.min((width - 70.0) / w as f32);
            out.push(Draw::Tex { x: 32.0, y: TITLE_Y, w: w as f32 * scale, h: h as f32 * scale,
                tex, alpha: fade });
        }
    }

    /// The path line over a menu page's title: where the page is.
    pub(super) fn menu_path(&self, heading: usize) -> &'static str {
        let game = self.game.is_some();
        match heading {
            0 => "slot.",
            5 => "Menu",
            19 => "Paused",
            26 if game => "Game Menu",
            26 => "Menu",
            33 | 50 if game => "Game Menu \u{203a} Settings",
            59 => "Settings \u{203a} Interface",
            33 | 50 | 82 | 117 => "Menu \u{203a} Settings",
            91..=93 | 121..=123 => "Settings \u{203a} Systems",
            _ => "",
        }
    }

    /// The path line over a Controls or Cheats page.
    pub(super) fn controls_path(&self) -> &'static str {
        let game = self.game.is_some();
        match self.overlay {
            ShelfOverlay::ControlsSettings { .. } => "Menu \u{203a} Settings",
            ShelfOverlay::ControlsPort { .. } | ShelfOverlay::Hotkeys { .. } => "Settings \u{203a} Controls",
            ShelfOverlay::LayoutPorts { .. } | ShelfOverlay::LayoutPort { .. } if game => "Game Menu \u{203a} Settings",
            ShelfOverlay::LayoutPorts { .. } | ShelfOverlay::LayoutPort { .. } => "Settings \u{203a} Systems",
            ShelfOverlay::Cheats { .. } => "Game Menu",
            _ => "",
        }
    }

    /// Whether a menu row opens a page of its own.
    pub(super) fn menu_row_opens(&self, heading: usize, item: usize) -> bool {
        match heading {
            // The main menu, Settings (the game's too, but its Reset to
            // global) and the Systems list.
            0 | 117 => true,
            26 => item != 57,
            // The game menu's Save States, Multiplayer, Cheats and Settings.
            19 => matches!(item, 22 | 25 | 66 | 134),
            // Library's RomM Server.
            5 => item == 135,
            // A system's Controls.
            91..=93 | 121..=123 => item == 125,
            _ => false,
        }
    }

    /// The path line and side-cart faces the shown page needs.
    pub(super) fn prepare_menu_look(&mut self) {
        let path = match self.overlay {
            ShelfOverlay::None | ShelfOverlay::Core | ShelfOverlay::States
            | ShelfOverlay::Multiplayer | ShelfOverlay::About { .. } => "",
            ShelfOverlay::CartStudio | ShelfOverlay::CartLibrary | ShelfOverlay::Romm => self.studio_page_path(),
            ShelfOverlay::InterfaceSettings { .. } => self.menu_path(59),
            ShelfOverlay::ControlsSettings { .. } | ShelfOverlay::ControlsPort { .. }
            | ShelfOverlay::LayoutPorts { .. } | ShelfOverlay::LayoutPort { .. }
            | ShelfOverlay::Hotkeys { .. } | ShelfOverlay::Cheats { .. } => self.controls_path(),
            ShelfOverlay::Menu { .. } => self.menu_path(0),
            ShelfOverlay::Library { .. } => self.menu_path(5),
            ShelfOverlay::GameMenu { .. } => self.menu_path(19),
            ShelfOverlay::Settings { .. } | ShelfOverlay::PauseSettings { .. } => self.menu_path(26),
            ShelfOverlay::ScreenSystemSettings { .. } => self.menu_path(91),
            _ => self.menu_path(33),
        };
        if !path.is_empty() && !self.menu_paths.contains_key(path) {
            let face = menu_style::menu_path_face(path);
            let tex = self.gpu.create_face(&face);
            self.menu_paths.insert(path, (tex, face.w, face.h));
        }
        if !matches!(self.overlay, ShelfOverlay::GameMenu { .. }) { return; }
        let Some(cart) = self.shelves.get(self.active).and_then(|s| s.carts.get(s.index)) else {
            return;
        };
        let name = slot_ui::label_text(cart);
        let key = format!("{name}\n{}", cart.platform.name());
        if self.menu_side.as_ref().is_some_and(|(seen, _)| *seen == key) { return; }
        let title = slot_ui::quick_value_face(&name, true);
        let system = menu_style::menu_path_face(cart.platform.name());
        if title.w == 0 || system.w == 0 { return; }
        let faces = match self.menu_side.take() {
            Some((_, [a, b])) => {
                self.gpu.update_face(a.0, &title);
                self.gpu.update_face(b.0, &system);
                [(a.0, title.w, title.h), (b.0, system.w, system.h)]
            }
            None => [(self.gpu.create_face(&title), title.w, title.h),
                     (self.gpu.create_face(&system), system.w, system.h)],
        };
        self.menu_side = Some((key, faces));
    }

    /// The room right of the main or game menu's rows for the selected
    /// cart, where the screen is wide enough for both.
    pub(super) fn menu_side_room(&self, heading: usize, items: &[usize]) -> Option<f32> {
        if heading != 19 || self.menu_side.is_none() { return None; }
        let shelf = self.shelves.get(self.active)?;
        shelf.carts.get(shelf.index)?;
        shelf.face(shelf.index)?;
        let widest = items.iter()
            .filter_map(|&item| self.menu_textures.get(item)).map(|t| t.1).max().unwrap_or(0);
        let room = out_w() as f32 - (37.0 + widest as f32 * 0.79 + 40.0);
        (room >= SIDE_MIN).then_some(room.min(SIDE_MAX))
    }

    /// The selected cart, its title and its system, centred in `room` at
    /// the right of `shown` rows from `top`.
    pub(super) fn draw_menu_side(&self, room: f32, top: f32, shown: usize, fade: f32,
                                 out: &mut Vec<Draw>) {
        let Some((_, [title, system])) = self.menu_side else { return };
        let Some(shelf) = self.shelves.get(self.active) else { return };
        let (Some(cart), Some(tex)) = (shelf.carts.get(shelf.index), shelf.face(shelf.index)) else {
            return;
        };
        let (bw, bh) = slot_ui::body_box(slot_ui::body_of(cart));
        let (bw, bh) = (bw.max(1) as f32, bh.max(1) as f32);
        let k = ((room - 64.0).min(264.0) / bw).min(170.0 / bh);
        let (w, h) = ((bw * k).round(), (bh * k).round());
        let centre = (out_w() as f32 - room / 2.0).round();
        let lines = room - 28.0;
        let title_scale = 0.56f32.min(lines / title.1.max(1) as f32);
        let system_scale = 1.0f32.min(lines / system.1.max(1) as f32);
        let (title_h, system_h) = (title.2 as f32 * title_scale, system.2 as f32 * system_scale);
        let block = h + 10.0 + title_h + system_h;
        let y = (top + shown as f32 * QUICK_PITCH / 2.0 - block / 2.0).round();
        out.push(Draw::Tex { x: centre - w / 2.0, y, w, h, tex, alpha: fade });
        let title_w = title.1 as f32 * title_scale;
        out.push(Draw::Tex { x: (centre - title_w / 2.0).round(), y: y + h + 10.0,
            w: title_w, h: title_h, tex: title.0, alpha: 0.92 * fade });
        let system_w = system.1 as f32 * system_scale;
        out.push(Draw::Tex { x: (centre - system_w / 2.0).round(), y: y + h + 10.0 + title_h,
            w: system_w, h: system_h, tex: system.0, alpha: 0.5 * fade });
    }

    pub(super) fn studio_page_path(&self) -> &'static str {
        match self.overlay {
            ShelfOverlay::CartLibrary => "Menu",
            // From Menu > Cart Studio's shelves, or X on a cart of the home.
            ShelfOverlay::CartStudio if self.studio.as_ref().is_some_and(|s| s.from_library) =>
                "Menu \u{203a} Cart Studio",
            ShelfOverlay::CartStudio => "slot.",
            ShelfOverlay::Romm => "Menu \u{203a} Library",
            _ => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_page_keeps_every_row_whole() {
        for i in 0..5 {
            assert_eq!(row_place(i, 5, 0.0, 0.0), Some((i as f32 * QUICK_PITCH, 1.0)));
        }
    }

    #[test]
    fn a_long_page_dims_the_edge_row_before_hidden_rows() {
        let max = 2.0 * QUICK_PITCH;
        // At the top: nothing above, more below.
        assert_eq!(row_place(0, 8, 0.0, max).unwrap().1, 1.0);
        assert_eq!(row_place(5, 8, 0.0, max).unwrap().1, 0.5);
        assert!(row_place(6, 8, 0.0, max).is_none(), "the seventh row is out of sight");
        // At the end: more above, nothing below.
        assert_eq!(row_place(2, 8, max, max).unwrap(), (0.0, 0.5));
        assert_eq!(row_place(7, 8, max, max).unwrap().1, 1.0);
        assert!(row_place(1, 8, max, max).is_none());
    }

    #[test]
    fn a_row_fades_as_it_slides_out() {
        let max = 2.0 * QUICK_PITCH;
        let (dy, shows) = row_place(0, 8, QUICK_PITCH / 2.0, max).unwrap();
        assert_eq!(dy, -QUICK_PITCH / 2.0);
        assert!(shows > 0.0 && shows < 0.5);
        // The rows between the edges stay whole throughout.
        assert_eq!(row_place(3, 8, QUICK_PITCH / 2.0, max).unwrap().1, 1.0);
    }
}

/// Writes the menus as drawn, one PNG a page, into $SCRATCH_MENUS: a look
/// at them without a handheld. Does nothing without the variable.
#[cfg(all(test, target_os = "linux"))]
mod render {
    use super::*;

    #[test]
    fn apps_and_bios_picker_return_to_their_original_menu_row() {
        let _surface = slot_gfx::TestEgl::new();
        let mut engine = Engine::new(1, Some(&entries())).unwrap();
        for page in [ShelfOverlay::Menu { row: 3 }, ShelfOverlay::Library { row: 2 }] {
            engine.overlay = page;
            engine.handle(Input::Key(96, true));
            assert!(engine.external_pending);
            engine.handle(Input::Suspend);
            engine.handle(Input::Reset);
            assert_eq!(engine.overlay, page);
            engine.handle(Input::ExternalReturn);
            assert!(!engine.external_pending);
            assert_eq!(engine.overlay, page);
            assert!(engine.menu_held.is_none());
        }
        // Refresh Library opens nothing: back to the shelf, where its scan shows.
        engine.overlay = ShelfOverlay::Library { row: 6 };
        engine.handle(Input::Key(96, true));
        assert_eq!(engine.overlay, ShelfOverlay::None);
        assert!(!engine.external_pending);
    }

    #[test]
    fn cart_studio_names_the_page_it_was_opened_from() {
        let _surface = slot_gfx::TestEgl::new();
        let mut engine = Engine::new(1, Some(&entries())).unwrap();
        engine.open_cart_studio();
        assert_eq!(engine.studio_page_path(), "slot.");
        let cart = engine.shelves[engine.active].carts[0].clone();
        engine.open_cart_studio_on(cart, None, true);
        assert_eq!(engine.studio_page_path(), "Menu \u{203a} Cart Studio");
    }

    fn entries() -> Vec<RomEntry> {
        let json: Vec<String> = ["Pokemon Emerald", "Metroid Fusion"].iter().map(|n| format!(
            r#"{{"title":"{n}","platform":"GBA","uri":"content://roms/{n}.gba","code":"AXXE"}}"#)).collect();
        serde_json::from_str(&format!("[{}]", json.join(","))).unwrap()
    }

    fn shoot(engine: &mut Engine, dir: &std::path::Path, name: &str) {
        for _ in 0..40 {
            engine.draw(0, || {});
            std::thread::sleep(std::time::Duration::from_millis(12));
        }
        save(engine, dir, name);
    }

    fn save(engine: &Engine, dir: &std::path::Path, name: &str) {
        let rgba = engine.gpu.read_frame();
        let (w, h) = panel_size();
        let file = std::io::BufWriter::new(std::fs::File::create(dir.join(name)).unwrap());
        let mut enc = png::Encoder::new(file, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgba).unwrap();
    }

    #[test]
    fn render_menus() {
        let Ok(dir) = std::env::var("SCRATCH_MENUS") else { return };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        // $SCRATCH_PANEL: another panel than the KONKR's, as "1240x1080".
        if let Some((w, h)) = std::env::var("SCRATCH_PANEL").ok().as_deref()
            .and_then(|size| size.split_once('x'))
            .and_then(|(w, h)| Some((w.parse::<u64>().ok()?, h.parse::<u64>().ok()?))) {
            PANEL_SIZE.store(w << 32 | h, Ordering::Release);
        }
        let _surface = slot_gfx::TestEgl::new();
        let mut engine = Engine::new(1, Some(&entries())).unwrap();
        shoot(&mut engine, &dir, "00_home.png");
        let pages: Vec<(&str, ShelfOverlay)> = vec![
            ("01_menu", ShelfOverlay::Menu { row: 1 }),
            ("02_library_top", ShelfOverlay::Library { row: 0 }),
            ("03_library_end", ShelfOverlay::Library { row: 6 }),
            ("04_settings", ShelfOverlay::Settings { row: 1 }),
            ("05_systems", ShelfOverlay::ScreenSettings { row: 0 }),
            ("06_gba_top", ShelfOverlay::ScreenSystemSettings { platform: Platform::Gba, row: 1 }),
            ("07_gba_mid", ShelfOverlay::ScreenSystemSettings { platform: Platform::Gba, row: 5 }),
            ("08_interface", ShelfOverlay::GeneralSettings { row: 2 }),
            ("09_status_bar", ShelfOverlay::InterfaceSettings { row: 0 }),
            ("10_about", ShelfOverlay::About { row: 0 }),
            ("11_controls", ShelfOverlay::ControlsSettings { row: 0 }),
            ("12_hotkeys", ShelfOverlay::Hotkeys { row: 5 }),
        ];
        for (name, overlay) in pages {
            engine.overlay = overlay;
            shoot(&mut engine, &dir, &format!("{name}.png"));
        }
        // A long page on its way down a row.
        engine.overlay = ShelfOverlay::Library { row: 4 };
        shoot(&mut engine, &dir, "13_slide_a.png");
        engine.overlay = ShelfOverlay::Library { row: 5 };
        for _ in 0..3 {
            engine.draw(0, || {});
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        save(&engine, &dir, "14_slide_b.png");
        engine.overlay = ShelfOverlay::GameMenu { row: 0 };
        shoot(&mut engine, &dir, "15_game_menu.png");
    }
}
