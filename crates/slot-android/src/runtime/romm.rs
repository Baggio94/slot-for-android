//! Library > RomM: the games of the player's own RomM server, as carts with
//! their labels, two rows across the whole width (Siham 2026-11-08: as
//! Cart Studio's library in 2.1.2-beta7, 8 carts at once on the KPA).
//! A picks a cart, Y downloads the picked ones into the ROM folder, each
//! system in its own folder; X shows the server (sign out).
//!
//! Android does the network and the files (RommPage.kt): the page asks it
//! through ROMM_REQUEST and hears back through Input::Romm*.
use super::*;
use slot_store::Cart;
use std::collections::{HashMap, HashSet};

/// The render rack of RomM's carts, after the systems, Recents and
/// Favorites (cart_render.rs counts it).
pub(crate) const ROMM_RACK: usize = Platform::ALL.len() + 3;
/// A face's key packs the cart's index and its label's version, so a face
/// drawn before its label arrived is never taken for the labelled one.
const VERSION_SHIFT: usize = 20;
const INDEX_MASK: usize = (1 << VERSION_SHIFT) - 1;

const GRID_TOP: f32 = 122.0;
const GRID_BOTTOM: f32 = 416.0;
const TABS_Y: f32 = 78.0;
const NAME_H: f32 = 24.0;
const CHOSEN: f32 = 1.05;
const RESTING: f32 = 0.9;
const NAME_GAP: f32 = 8.0;
const GROW_S: f32 = 0.12;
/// Statuses come from Android: short, plain text only on the GL thread.
const MAX_TEXT: usize = 120;

/// Requests to Android (RommPage.kt): open, server, labels, download, leave.
pub(crate) static ROMM_REQUEST: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

fn ask(request: String) {
    ROMM_REQUEST.lock().unwrap_or_else(|e| e.into_inner()).push_back(request);
}

/// One game of the server's library, as Android lists it.
#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct RommEntry {
    pub id: i64,
    pub platform: String,
    pub title: String,
    /// Where Android keeps its label (there or not yet).
    pub label: String,
    /// Read from the game's header on the server (Game Boy family only,
    /// unzipped files), as the shelf reads a local ROM's: the GBA game
    /// code; the Game Boy title, code, destination and CGB flag.
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub gb_title: String,
    #[serde(default)]
    pub gb_code: String,
    #[serde(default)]
    pub gb_japan: bool,
    #[serde(default)]
    pub gb_cgb_flag: Option<i32>,
    /// For START's INFO card: the file's whole name, its size in bytes, its
    /// regions and version as RomM read them.
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub version: String,
    /// In the RomM account's favourites.
    #[serde(default)]
    pub favorite: bool,
}

/// SELECT's filters (Siham 2026-11-10), kept by Android between visits.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub(crate) struct RommFilter {
    /// Words the title has, any case or accent.
    #[serde(default)]
    pub search: String,
    /// "", USA, Europe, Japan or World. World games show in every region.
    #[serde(default)]
    pub region: String,
    /// No hacks, aftermarket or homebrew, unlicensed, pirate, prototype,
    /// beta, demo or fan translation.
    #[serde(default)]
    pub official: bool,
    /// Not the games already on the shelf.
    #[serde(default)]
    pub hide_shelf: bool,
    /// Only the RomM account's favourites.
    #[serde(default)]
    pub favorites: bool,
}

impl RommFilter {
    fn active(&self) -> bool { *self != RommFilter::default() }

    fn keeps(&self, entry: &RommEntry, on_shelf: bool) -> bool {
        if self.hide_shelf && on_shelf { return false; }
        if self.favorites && !entry.favorite { return false; }
        if self.official && !is_official(&entry.title) { return false; }
        if !self.region.is_empty() && !in_region(&entry.title, &entry.region, &self.region) { return false; }
        let words = folded(&self.search);
        if !words.is_empty() {
            let title = folded(&entry.title);
            if !words.split(' ').all(|w| title.contains(w)) { return false; }
        }
        true
    }
}

/// Lower case, accents off, punctuation as spaces, one space between words.
fn folded(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a', 'ç' => 'c', 'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i', 'ñ' => 'n', 'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u', 'ý' | 'ÿ' => 'y', '\'' | '’' => continue,
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        if c == ' ' && (out.is_empty() || out.ends_with(' ')) { continue; }
        out.push(c);
    }
    out.trim_end().to_owned()
}

/// The (…) and […] groups of a No-Intro style name.
fn groups(title: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = title;
    while let Some(open) = rest.find(['(', '[']) {
        let close = if rest[open..].starts_with('(') { ')' } else { ']' };
        let Some(end) = rest[open + 1..].find(close) else { break };
        out.push(rest[open + 1..open + 1 + end].trim());
        rest = &rest[open + 1 + end + 1..];
    }
    out
}

const REGIONS: [&str; 22] = ["USA", "Europe", "Japan", "World", "Asia", "Korea", "China", "Taiwan", "Hong Kong",
    "France", "Germany", "Spain", "Italy", "UK", "Netherlands", "Sweden", "Australia", "Brazil", "Canada",
    "Russia", "Scandinavia", "Latin America"];
const EUROPE: [&str; 10] = ["Europe", "France", "Germany", "Spain", "Italy", "UK", "Netherlands", "Sweden",
    "Australia", "Scandinavia"];

/// The regions a name gives, from its first group of region names.
fn regions_of(title: &str) -> Vec<String> {
    groups(title).into_iter().map(|g| g.split(',').map(|r| r.trim().to_owned()).collect::<Vec<_>>())
        .find(|parts| !parts.is_empty() && parts.iter().all(|r| REGIONS.contains(&r.as_str())))
        .unwrap_or_default()
}

/// Whether a game was released in `wanted` (World games everywhere).
fn in_region(title: &str, romm: &str, wanted: &str) -> bool {
    let mut regions = regions_of(title);
    if regions.is_empty() {
        regions = romm.split(',').map(|r| r.trim().to_owned()).filter(|r| !r.is_empty()).collect();
    }
    regions.iter().any(|r| r == "World" || match wanted {
        "Europe" => EUROPE.contains(&r.as_str()),
        "World" => false,
        _ => r == wanted,
    })
}

/// A licensed release as it came out: no hack, aftermarket or homebrew,
/// unlicensed, pirate, prototype, beta, demo, sample, subset or fan
/// translation (an English Japanese-only game).
fn is_official(title: &str) -> bool {
    const NOT: [&str; 13] = ["hack", "aftermarket", "homebrew", "unl", "pirate", "proto", "beta", "demo",
        "sample", "subset", "translat", "t+", "t-"];
    let all = groups(title);
    if all.iter().any(|g| {
        let g = g.to_ascii_lowercase();
        // [h], [h1], [b2]: GoodTools' hack and bad dump marks.
        let mark = |c: char| g.starts_with(c) && g.len() <= 3 && g.chars().skip(1).all(|d| d.is_ascii_digit());
        NOT.iter().any(|n| g.starts_with(n)) || g.contains("translation") || mark('h') || mark('b')
    }) { return false; }
    let regions = regions_of(title);
    let eastern = !regions.is_empty() && regions.iter()
        .all(|r| matches!(r.as_str(), "Japan" | "China" | "Korea" | "Taiwan" | "Asia" | "Hong Kong"));
    !(eastern && all.iter().any(|g| *g == "En"))
}

#[derive(Default)]
struct System {
    carts: Vec<Cart>,
    ids: Vec<i64>,
    on_shelf: Vec<bool>,
    /// Bumped when a label arrives: the cart is drawn again.
    version: Vec<usize>,
}

pub(super) struct RommPage {
    /// The server's games, in its order, and the filters on them.
    all: Vec<RommEntry>,
    filter: RommFilter,
    /// START: the INFO card of the cart chosen, its lines made.
    info: Option<Vec<(String, bool)>>,
    systems: Vec<System>,
    system: usize,
    cursor: usize,
    first_row: usize,
    picked: HashSet<i64>,
    /// Loading, or why there is nothing to show.
    status: Option<String>,
    loading: bool,
    /// Picked carts being downloaded: done, of, the current one's percent.
    progress: Option<(i32, i32, i32)>,
    downloading: Option<i64>,
    /// The current system's faces by cart index, with their label version.
    faces: HashMap<usize, (slot_gfx::TexId, usize)>,
    /// The labels last asked for, not to ask every frame.
    labels_asked: Vec<i64>,
    letter: Option<(slot_gfx::TexId, u32, u32)>,
    letter_at: Option<Instant>,
    grown: (usize, usize),
    shrinking: Option<(usize, usize)>,
    grown_at: Option<Instant>,
}

/// The grid: two rows over the whole width, at least 3 columns, one more
/// every 168 pixels (4 on the KPA's 720), every system alike.
struct Grid { x: f32, cols: usize, rows: usize, cell_w: f32, cell_h: f32 }

fn grid() -> Grid {
    let width = out_w() as f32;
    let x = 18.0;
    let room = (width - 2.0 * x).max(200.0);
    let cols = ((room / 168.0) as usize).max(3);
    let rows = 2;
    Grid { x, cols, rows, cell_w: room / cols as f32, cell_h: (GRID_BOTTOM - GRID_TOP) / rows as f32 }
}

/// Plain, short text from Android.
fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(MAX_TEXT).collect()
}

/// A RomM game as a cart: the shelf's own shell for its name, its label once
/// Android has it.
fn cart_of(entry: &RommEntry) -> Option<(Platform, Cart)> {
    let platform = Platform::parse(&entry.platform)?;
    let title = entry.title.trim();
    if title.is_empty() { return None; }
    let label = (!entry.label.is_empty()).then(|| std::path::PathBuf::from(&entry.label));
    Some((platform, Cart {
        platform,
        stem: title.to_owned(),
        title: title.to_owned(),
        code: entry.code.clone(),
        rom: std::path::PathBuf::from(format!("romm://{}", entry.id)),
        label,
        shell: crate::library::remote_shell(platform, title, entry.gb_cgb_flag.map(|flag| {
            crate::library::RemoteGbHeader {
                title: entry.gb_title.clone(), code: entry.gb_code.clone(),
                japan: entry.gb_japan, cgb_flag: Some(flag),
            }
        })),
        battery: false,
    }))
}

/// The page's systems from the server's games, sorted as the shelf sorts.
fn systems_of(entries: &[RommEntry], shelf: impl Fn(Platform, &str) -> bool) -> Vec<System> {
    let mut grouped: Vec<Vec<(i64, Cart)>> = (0..Platform::ALL.len()).map(|_| Vec::new()).collect();
    for entry in entries {
        if let Some((platform, cart)) = cart_of(entry) {
            grouped[platform as usize].push((entry.id, cart));
        }
    }
    grouped.into_iter().map(|mut games| {
        games.sort_by_cached_key(|(_, cart)| slot_store::sort_key(&cart.stem));
        let mut system = System::default();
        for (id, cart) in games {
            system.on_shelf.push(shelf(cart.platform, &cart.stem));
            system.ids.push(id);
            system.version.push(0);
            system.carts.push(cart);
        }
        system
    }).collect()
}

/// L2/R2: the first cart of the previous or next letter, as the shelf.
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

impl Engine {
    /// Library > RomM: Android connects first when it has no account.
    pub(super) fn open_romm(&mut self) {
        let keep = self.romm.take();
        let mut page = keep.unwrap_or_else(|| RommPage {
            all: Vec::new(), filter: RommFilter::default(), info: None,
            systems: Vec::new(), system: 0, cursor: 0, first_row: 0, picked: HashSet::new(),
            status: Some("Connecting to RomM…".into()), loading: true, progress: None, downloading: None,
            faces: HashMap::new(), labels_asked: Vec::new(), letter: None, letter_at: None,
            grown: (0, 0), shrinking: None, grown_at: None,
        });
        page.labels_asked.clear();
        if page.systems.iter().all(|s| s.carts.is_empty()) {
            page.status = Some("Connecting to RomM…".into());
            page.loading = true;
        }
        self.romm = Some(page);
        self.external_pending = true;
        ask("open".into());
        self.overlay = ShelfOverlay::Romm;
    }

    fn leave_romm(&mut self, to: ShelfOverlay) {
        if let Some(page) = self.romm.as_mut() {
            // The faces go back to the pool; the list stays for next time.
            let faces: Vec<slot_gfx::TexId> = page.faces.drain().map(|(_, (tex, _))| tex)
                .chain(page.letter.take().map(|(tex, _, _)| tex)).collect();
            page.letter_at = None;
            page.labels_asked.clear();
            self.studio_pool.extend(faces);
        }
        self.cart_render.invalidate_shelf(ROMM_RACK);
        ask("leave".into());
        self.overlay = to;
    }

    /// Whether the local shelf already has a cart of that name.
    fn on_local_shelf(&self, platform: Platform, stem: &str) -> bool {
        let video = (platform == Platform::Gba).then_some(VIDEO_RACK);
        std::iter::once(platform as usize).chain(video).filter_map(|rack| self.shelves.get(rack))
            .any(|shelf| shelf.carts.iter().any(|c| c.stem.eq_ignore_ascii_case(stem)))
    }

    pub(super) fn romm_catalog(&mut self, entries: Vec<RommEntry>) {
        let Some(page) = self.romm.as_mut() else { return };
        page.all = entries;
        page.loading = false;
        self.romm_rebuild();
    }

    /// SELECT's filters changed (or came with the page).
    pub(super) fn romm_filter(&mut self, filter: RommFilter) {
        let Some(page) = self.romm.as_mut() else { return };
        if page.filter == filter { return; }
        page.filter = filter;
        if !page.all.is_empty() { self.romm_rebuild(); }
    }

    /// The page's systems again, from the server's games and the filters.
    fn romm_rebuild(&mut self) {
        let Some(page) = self.romm.as_ref() else { return };
        let filter = page.filter.clone();
        let shown: Vec<RommEntry> = page.all.iter().filter(|entry| {
            let on_shelf = Platform::parse(&entry.platform)
                .is_some_and(|p| self.on_local_shelf(p, entry.title.trim()));
            filter.keeps(entry, on_shelf)
        }).cloned().collect();
        let any = !page.all.is_empty();
        let systems = systems_of(&shown, |p, stem| self.on_local_shelf(p, stem));
        let first = systems.iter().position(|s| !s.carts.is_empty());
        let mut freed = Vec::new();
        if let Some(page) = self.romm.as_mut() {
            freed.extend(page.faces.drain().map(|(_, (tex, _))| tex));
            // Stay on the system and cart shown, when they're still there.
            let shown = page.systems.get(page.system).and_then(|s| s.ids.get(page.cursor)).copied();
            page.systems = systems;
            page.info = None;
            page.status = first.is_none().then(|| if any && filter.active() {
                "No games match your filters. SELECT changes them.".into()
            } else {
                "No games for slot.'s systems on this server.".into()
            });
            if page.systems.get(page.system).is_none_or(|s| s.carts.is_empty()) {
                page.system = first.unwrap_or(0);
                page.cursor = 0;
                page.first_row = 0;
            } else {
                page.cursor = shown.and_then(|id| page.systems[page.system].ids.iter().position(|&i| i == id))
                    .unwrap_or(0);
            }
            // Picks hidden by a filter stay picked.
            let ids: HashSet<i64> = page.all.iter().map(|e| e.id).collect();
            page.picked.retain(|id| ids.contains(id));
            page.labels_asked.clear();
            page.grown = (page.system, page.cursor);
        }
        self.studio_pool.extend(freed);
        self.cart_render.invalidate_shelf(ROMM_RACK);
        self.keep_romm_cursor_visible();
    }

    /// 0 loading, 1 a message, 2 signed out (forget the library), 3 close
    /// the page (the form was cancelled).
    pub(super) fn romm_status(&mut self, kind: i32, text: String) {
        if kind == 3 {
            if matches!(self.overlay, ShelfOverlay::Romm) {
                self.leave_romm(ShelfOverlay::Library { row: 1 });
            }
            return;
        }
        if kind == 2 {
            // Signed out: the library is forgotten, the form shows next
            // (and the page never flashes the old carts when it opens).
            let mut freed = Vec::new();
            if let Some(page) = self.romm.as_mut() {
                freed.extend(page.faces.drain().map(|(_, (tex, _))| tex));
                page.systems.clear();
                page.all.clear();
                page.info = None;
                page.system = 0;
                page.cursor = 0;
                page.first_row = 0;
                page.picked.clear();
                page.progress = None;
                page.downloading = None;
                page.labels_asked.clear();
                page.grown = (0, 0);
                page.shrinking = None;
                page.status = Some("Connecting to RomM…".into());
                page.loading = true;
            }
            self.studio_pool.extend(freed);
            self.cart_render.invalidate_shelf(ROMM_RACK);
            return;
        }
        let Some(page) = self.romm.as_mut() else { return };
        let text = clean(&text);
        page.loading = kind == 0;
        if kind != 0 {
            // Whatever went wrong, no download is under way any more: Y
            // comes back, the picked carts stay picked.
            page.progress = None;
            page.downloading = None;
        }
        if page.systems.iter().all(|s| s.carts.is_empty()) {
            page.status = Some(text);
        } else if kind != 0 {
            // Over a library: a note, as the shelf's. A reload stays quiet,
            // the carts already shown.
            page.status = None;
            set_message(text);
        }
    }

    pub(super) fn romm_label(&mut self, id: i64) {
        let Some(page) = self.romm.as_mut() else { return };
        for system in &mut page.systems {
            if let Some(i) = system.ids.iter().position(|&g| g == id) {
                system.version[i] += 1;
            }
        }
    }

    /// A game's header arrived: its cart wears its own shell (and, on the
    /// GBA, its code's colour), drawn again.
    pub(super) fn romm_header(&mut self, entry: RommEntry) {
        let Some(page) = self.romm.as_mut() else { return };
        if let Some(known) = page.all.iter_mut().find(|e| e.id == entry.id) { *known = entry.clone(); }
        let Some((_, mut cart)) = cart_of(&entry) else { return };
        for system in &mut page.systems {
            if let Some(i) = system.ids.iter().position(|&g| g == entry.id) {
                cart.label = system.carts[i].label.clone();
                if system.carts[i].code != cart.code || system.carts[i].shell != cart.shell {
                    system.carts[i] = cart;
                    system.version[i] += 1;
                }
                return;
            }
        }
    }

    pub(super) fn romm_progress(&mut self, done: i32, total: i32, percent: i32) {
        let Some(page) = self.romm.as_mut() else { return };
        if done >= total {
            page.progress = None;
            page.downloading = None;
        } else {
            page.progress = Some((done, total, percent));
        }
    }

    pub(super) fn romm_downloaded(&mut self, id: i64, ok: bool) {
        let Some(page) = self.romm.as_mut() else { return };
        if ok {
            page.picked.remove(&id);
            for system in &mut page.systems {
                if let Some(i) = system.ids.iter().position(|&g| g == id) { system.on_shelf[i] = true; }
            }
        }
        if page.downloading == Some(id) { page.downloading = None; }
    }

    fn keep_romm_cursor_visible(&mut self) {
        let Some(page) = self.romm.as_mut() else { return };
        let g = grid();
        let row = page.cursor / g.cols;
        if row < page.first_row { page.first_row = row; }
        if row >= page.first_row + g.rows { page.first_row = row + 1 - g.rows; }
    }

    /// The carts the page shows and the next row, by index.
    fn romm_cells(&self) -> Vec<usize> {
        if !matches!(self.overlay, ShelfOverlay::Romm) { return Vec::new(); }
        let Some(page) = self.romm.as_ref() else { return Vec::new() };
        let g = grid();
        let n = page.systems.get(page.system).map_or(0, |s| s.carts.len());
        let first = page.first_row * g.cols;
        (first..(first + g.cols * (g.rows + 1)).min(n)).collect()
    }

    /// For the face loader: the page's carts that need drawing, keyed with
    /// their label version, first.
    pub(super) fn romm_render_wants(&self) -> Vec<((usize, usize), Cart)> {
        let Some(page) = self.romm.as_ref() else { return Vec::new() };
        let Some(system) = page.systems.get(page.system) else { return Vec::new() };
        self.romm_cells().into_iter().filter_map(|i| {
            let version = system.version[i];
            if page.faces.get(&i).is_some_and(|&(_, v)| v == version) { return None; }
            Some(((ROMM_RACK, i | (version << VERSION_SHIFT)), system.carts[i].clone()))
        }).collect()
    }

    /// A face the loader drew for the page.
    pub(super) fn romm_face(&mut self, key: usize, face: &slot_ui::CartFace) {
        let (index, version) = (key & INDEX_MASK, key >> VERSION_SHIFT);
        let wanted = self.romm_cells();
        let Some(page) = self.romm.as_mut() else { return };
        if !wanted.contains(&index) { return; }
        if page.faces.get(&index).is_some_and(|&(_, v)| v >= version) { return; }
        let old = page.faces.remove(&index).map(|(tex, _)| tex);
        let tex = match old.or_else(|| self.studio_pool.pop()) {
            Some(tex) => { self.gpu.update_face(tex, face); tex }
            None => self.gpu.create_face(face),
        };
        if let Some(page) = self.romm.as_mut() { page.faces.insert(index, (tex, version)); }
    }

    pub(super) fn romm_key(&mut self, code: i32) {
        let Some(page) = self.romm.as_mut() else {
            self.overlay = ShelfOverlay::None;
            return;
        };
        if self.external_pending && code != 97 && code != 108 { return; }
        // The INFO card: B or START closes it, nothing else.
        if page.info.is_some() {
            if matches!(code, 97 | 108) { page.info = None; }
            return;
        }
        let n = page.systems.get(page.system).map_or(0, |s| s.carts.len());
        let cols = grid().cols;
        match code {
            21 if page.cursor > 0 => page.cursor -= 1,
            22 if page.cursor + 1 < n => page.cursor += 1,
            19 if page.cursor >= cols => page.cursor -= cols,
            20 if page.cursor + cols < n => page.cursor += cols,
            20 if page.cursor / cols < n.saturating_sub(1) / cols => page.cursor = n - 1,
            102 | 103 => {
                let count = page.systems.len();
                if count == 0 { return; }
                let step = if code == 103 { 1 } else { count - 1 };
                let mut next = page.system;
                for _ in 0..count {
                    next = (next + step) % count;
                    if !page.systems[next].carts.is_empty() { break; }
                }
                if next != page.system {
                    page.system = next;
                    page.cursor = 0;
                    page.first_row = 0;
                    page.letter_at = None;
                    page.labels_asked.clear();
                    let faces: Vec<slot_gfx::TexId> = page.faces.drain().map(|(_, (tex, _))| tex).collect();
                    self.studio_pool.extend(faces);
                    self.cart_render.invalidate_shelf(ROMM_RACK);
                }
            }
            104 | 105 => {
                let Some(system) = page.systems.get(page.system) else { return };
                if system.carts.is_empty() { return; }
                page.cursor = letter_target(&system.carts, page.cursor, if code == 105 { 1 } else { -1 });
                let letter = slot_store::initial(&system.carts[page.cursor].stem);
                self.show_romm_letter(letter);
            }
            // A: pick or unpick the cart; one already on the shelf stays.
            96 => {
                let Some(system) = page.systems.get(page.system) else { return };
                let Some(&id) = system.ids.get(page.cursor) else { return };
                if system.on_shelf[page.cursor] {
                    set_message("Already on your shelf.".into());
                } else if !page.picked.remove(&id) {
                    page.picked.insert(id);
                }
            }
            // Y: download the picked carts, in the page's order.
            100 => {
                if page.progress.is_some() || page.picked.is_empty() { return; }
                let ids: Vec<String> = page.systems.iter().flat_map(|s| s.ids.iter())
                    .filter(|id| page.picked.contains(id)).map(|id| id.to_string()).collect();
                let total = ids.len() as i32;
                page.progress = Some((0, total, 0));
                ask(format!("download\t{}", ids.join(",")));
            }
            99 => {
                self.external_pending = true;
                ask("server".into());
            }
            // SELECT: the filters, an Android form (its keyboard for Search).
            109 if !page.all.is_empty() => {
                self.external_pending = true;
                ask("filter".into());
            }
            // START: everything about the chosen game's file.
            108 => {
                let Some(system) = page.systems.get(page.system) else { return };
                let Some(&id) = system.ids.get(page.cursor) else { return };
                let on_shelf = system.on_shelf[page.cursor];
                let title = slot_ui::label_text(&system.carts[page.cursor]);
                let Some(entry) = page.all.iter().find(|e| e.id == id) else { return };
                let room = info_room(out_w());
                let wide = room + 2 * slot_ui::quick_value_width("");
                page.info = Some(info_lines(&title, entry, on_shelf, room,
                    |t| slot_ui::quick_value_width_within(t, wide)));
            }
            97 => self.leave_romm(ShelfOverlay::Library { row: 1 }),
            _ => {}
        }
        self.keep_romm_cursor_visible();
    }

    fn show_romm_letter(&mut self, letter: char) {
        let face = slot_ui::platform_name_face(&letter.to_string());
        let old = self.romm.as_mut().and_then(|p| p.letter.take()).map(|(tex, _, _)| tex);
        let tex = match old.or_else(|| self.studio_pool.pop()) {
            Some(tex) => { self.gpu.update_face(tex, &face); tex }
            None => self.gpu.create_face(&face),
        };
        if let Some(page) = self.romm.as_mut() {
            page.letter = Some((tex, face.w, face.h));
            page.letter_at = Some(Instant::now());
        }
    }

    /// Before drawing: the texts, the labels to fetch, the faces to drop.
    pub(super) fn prepare_romm(&mut self) {
        if !matches!(self.overlay, ShelfOverlay::Romm) { return; }
        let cells = self.romm_cells();
        let Some(page) = self.romm.as_mut() else { return };
        let now = (page.system, page.cursor);
        if page.grown != now {
            page.shrinking = (page.grown.0 == now.0).then_some(page.grown);
            page.grown = now;
            page.grown_at = Some(Instant::now());
        }
        // Faces off the page and its next row go back to the pool.
        let gone: Vec<usize> = page.faces.keys().copied().filter(|i| !cells.contains(i)).collect();
        let mut freed = Vec::new();
        for i in gone {
            if let Some((tex, _)) = page.faces.remove(&i) { freed.push(tex); }
        }
        // The labels of the carts shown, asked once per page.
        if let Some(system) = page.systems.get(page.system) {
            let ids: Vec<i64> = cells.iter().filter(|&&i| system.version[i] == 0).map(|&i| system.ids[i]).collect();
            if !ids.is_empty() && ids != page.labels_asked {
                ask(format!("labels\t{}", ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")));
                page.labels_asked = ids;
            }
        }
        self.studio_pool.extend(freed);
        let Some(page) = self.romm.as_ref() else { return };
        let mut texts: Vec<(String, bool)> = vec![("ROMM".into(), true), ("SELECTED".into(), true),
            ("ON SHELF".into(), true)];
        for (s, system) in page.systems.iter().enumerate() {
            let n = system.carts.len();
            if n > 0 { texts.push((format!("{} {n}", cart_studio::platform_key(Platform::ALL[s])), s == page.system)); }
        }
        if let Some(status) = &page.status { texts.push((status.clone(), true)); }
        if let Some(note) = corner_note(page) { texts.push((note, false)); }
        let info: Vec<(String, bool)> = page.info.clone().unwrap_or_default();
        if let Some(system) = page.systems.get(page.system) {
            for &i in &cells { texts.push((slot_ui::label_text(&system.carts[i]), true)); }
        }
        for (text, lit) in texts { self.controls_face(&text, lit); }
        let wide = info_room(out_w()) + 2 * slot_ui::quick_value_width("");
        for (text, lit) in info { self.controls_face_within(&text, lit, wide); }
        for (key, label) in [("B", "BACK"), ("L2 R2", "LETTER"), ("A", "SELECT"), ("Y", "DOWNLOAD"),
                             ("X", "SERVER"), ("SELECT", "FILTER"), ("START", "INFO"), ("B", "CLOSE"),
                             ("L1", ""), ("R1", "")] {
            self.controls_hint(key, label);
        }
    }

    pub(super) fn draw_romm(&self, out: &mut Vec<Draw>) {
        let Some(page) = self.romm.as_ref() else { return };
        self.draw_menu_ground(0.0, out);
        let width = out_w() as f32;
        let fade = slot_ui::ease((self.menu_opened.elapsed().as_secs_f32() / 0.16).clamp(0.0, 1.0));
        let face = |text: &str, lit: bool| self.controls_faces.get(&Self::face_key(text, lit)).copied();
        let tint = |a: f32| { let mut c = edge(); c[3] *= a * fade; c };
        self.draw_page_title(out, "ROMM", fade);
        // Right of the title: how many are picked, or the download.
        if let Some((tex, w, h)) = corner_note(page).and_then(|t| face(&t, false)) {
            let scale = 0.56f32.min((width * 0.5) / w.max(1) as f32);
            let (dw, dh) = (w as f32 * scale, h as f32 * scale);
            let title_mid = self.controls_faces.get(&Self::face_key("ROMM", true))
                .map_or(menu_look::TITLE_Y + 12.0, |&(_, _, th)| menu_look::TITLE_Y + th as f32 * 0.79 / 2.0);
            out.push(Draw::Tex { x: width - 32.0 - dw, y: title_mid - dh / 2.0, w: dw, h: dh, tex, alpha: 0.8 * fade });
        }
        // The systems, the chosen one lit, between L1 and R1.
        let tabs: Vec<(bool, slot_gfx::TexId, f32, f32)> = page.systems.iter().enumerate().filter_map(|(s, system)| {
            let n = system.carts.len();
            if n == 0 { return None; }
            face(&format!("{} {n}", cart_studio::platform_key(Platform::ALL[s])), s == page.system)
                .map(|(tex, w, h)| (s == page.system, tex, w as f32 * 0.5, h as f32 * 0.5))
        }).collect();
        self.draw_system_tabs(out, &tabs, TABS_Y, fade);
        // Loading, or nothing to show: the status in the middle of the screen.
        if let Some((tex, w, h)) = page.status.as_ref().and_then(|t| face(t, true)) {
            let s = 0.7f32.min((width - 60.0) / w.max(1) as f32);
            let (dw, dh) = (w as f32 * s, h as f32 * s);
            out.push(Draw::Tex { x: (width - dw) / 2.0, y: (OUT_H as f32 - dh) / 2.0, w: dw, h: dh, tex,
                alpha: if page.loading { 0.7 * fade } else { fade } });
        }
        let empty = System::default();
        let system = page.systems.get(page.system).unwrap_or(&empty);
        let g = grid();
        let first = page.first_row * g.cols;
        for i in first..(first + g.cols * g.rows).min(system.carts.len()) {
            let cart = &system.carts[i];
            let id = system.ids[i];
            let (col, row) = ((i - first) % g.cols, (i - first) / g.cols);
            let (cx, cy) = (g.x + col as f32 * g.cell_w, GRID_TOP + row as f32 * g.cell_h);
            let name_y = cy + g.cell_h - NAME_H - 4.0;
            let (room_w, room_h) = (g.cell_w - 12.0, name_y - NAME_GAP - (cy + 4.0));
            let (bw, bh) = slot_ui::cart_box_for(cart);
            let s = (room_w / bw as f32).min(room_h / bh as f32) * cart_size(page, i);
            let (dw, dh) = (bw as f32 * s, bh as f32 * s);
            let (ix, iy) = (cx + (g.cell_w - dw) / 2.0, name_y - NAME_GAP - dh);
            let picked = page.picked.contains(&id);
            let alpha = if system.on_shelf[i] { 0.45 * fade } else { fade };
            // A picked cart sits on a lit square.
            if picked {
                out.push(Draw::Rect { x: cx + 3.0, y: cy, w: g.cell_w - 6.0, h: name_y - cy - 2.0, colour: tint(0.55) });
            }
            match page.faces.get(&i) {
                Some(&(tex, _)) => out.push(Draw::Tex { x: ix, y: iy, w: dw, h: dh, tex, alpha }),
                None => out.push(Draw::Rect { x: ix, y: iy, w: dw, h: dh, colour: tint(0.5) }),
            }
            let badge = if picked { Some("SELECTED") } else if system.on_shelf[i] { Some("ON SHELF") } else { None };
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
                    w: tw, h: h as f32 * s, tex, alpha: if i == page.cursor { fade } else { 0.7 * fade } });
            }
        }
        // The letter L2/R2 jumped to, as the shelf shows it.
        if let (Some((tex, w, h)), Some(at)) = (page.letter, page.letter_at) {
            let elapsed = at.elapsed().as_millis() as u64;
            if platform_name_running(elapsed) {
                let alpha = slot_name_alpha(elapsed) * fade;
                let (w, h) = (w as f32, h as f32);
                let side = w.max(h) + 24.0;
                let (mx, my) = ((width - side) / 2.0, (GRID_TOP + GRID_BOTTOM - side) / 2.0);
                let mut ground = menu_ground();
                ground[3] *= 0.65 * alpha;
                out.push(Draw::Rect { x: mx, y: my, w: side, h: side, colour: ground });
                out.push(Draw::Tex { x: (width - w) / 2.0, y: my + (side - h) / 2.0, w, h, tex, alpha });
            }
        }
        // The legend.
        out.push(Draw::Rect { x: 0.0, y: 420.0, w: width, h: 60.0, colour: menu_ground() });
        // START's card over the page.
        if let Some(lines) = &page.info { self.draw_romm_info(out, lines, fade); }
        let mut keys: Vec<(&str, &str)> = vec![("B", "BACK")];
        if page.info.is_some() {
            keys = vec![("B", "CLOSE")];
        } else {
            if !system.carts.is_empty() {
                keys.extend([("L2 R2", "LETTER"), ("A", "SELECT")]);
            }
            if !page.picked.is_empty() && page.progress.is_none() { keys.push(("Y", "DOWNLOAD")); }
            keys.push(("X", "SERVER"));
            if !page.all.is_empty() { keys.push(("SELECT", "FILTER")); }
            if !system.carts.is_empty() { keys.push(("START", "INFO")); }
        }
        // A narrow screen drops the obvious hints first, never the spacing,
        // as Cart Studio's library.
        let hints_of = |keys: &[(&str, &str)]| -> Vec<(slot_gfx::TexId, u32)> {
            keys.iter().filter_map(|key| self.controls_hint_faces.get(key).copied()).collect()
        };
        for dropped in ["A", "B", "L2 R2", "X", "START"] {
            if !legend_too_wide(&hints_of(&keys)) { break; }
            keys.retain(|(key, _)| *key != dropped);
        }
        let hints = hints_of(&keys);
        for (tex, w, x) in centred_hints(&hints, LEGEND_GAP) {
            out.push(Draw::Tex { x, y: self.legend_y(), w: w as f32, h: HINT_H as f32, tex, alpha: fade });
        }
    }
}

/// START's card: where it starts and how big its lines are drawn.
const INFO_X: f32 = 40.0;
const INFO_SCALE: f32 = 0.5;
const INFO_LINE: f32 = 23.0;

impl Engine {
    fn draw_romm_info(&self, out: &mut Vec<Draw>, lines: &[(String, bool)], fade: f32) {
        let width = out_w() as f32;
        out.push(Draw::Rect { x: 0.0, y: 64.0, w: width, h: 352.0, colour: [0.0, 0.0, 0.0, 0.82 * fade] });
        let mut y = 78.0;
        for (i, (text, lit)) in lines.iter().enumerate() {
            let Some(&(tex, w, h)) = self.controls_faces.get(&Self::wide_face_key(text, *lit)) else { continue };
            // The first line, the game's name, a little bigger.
            let s = if i == 0 { 0.66 } else { INFO_SCALE };
            let s = s.min((width - 2.0 * INFO_X) / w.max(1) as f32);
            out.push(Draw::Tex { x: INFO_X, y, w: w as f32 * s, h: h as f32 * s, tex, alpha: fade });
            y += if i == 0 { INFO_LINE + 10.0 } else if !*lit { INFO_LINE - 4.0 } else { INFO_LINE + 4.0 };
            if y > 404.0 { break; }
        }
    }
}

/// How wide a line of the INFO card may be, in face pixels: the card's
/// whole width at INFO_SCALE. Its faces are made that wide, so a line
/// wraps at the card's edge, not halfway (Siham 2026-11-10).
fn info_room(out_w: u32) -> u32 {
    ((out_w as f32 - 2.0 * INFO_X - 24.0) / INFO_SCALE) as u32
}

/// The INFO card's lines: the name, then each heading (dim) and its value;
/// the file's name wrapped by words to `room` face pixels.
fn info_lines(title: &str, entry: &RommEntry, on_shelf: bool, room: u32,
              width: impl Fn(&str) -> u32) -> Vec<(String, bool)> {
    let mut lines = vec![(title.to_owned(), true)];
    let file = if entry.file.is_empty() { entry.title.clone() } else { entry.file.clone() };
    lines.push(("FILE NAME".into(), false));
    lines.extend(wrap(&file, room, width).into_iter().map(|l| (l, true)));
    let regions = regions_of(&entry.title);
    let region = if !regions.is_empty() { regions.join(", ") }
        else if !entry.region.is_empty() { entry.region.clone() } else { "Not given".into() };
    let version = version_of(&entry.title, &entry.version);
    for (head, value) in [("SIZE", size_text(entry.size)), ("REGION", region), ("VERSION", version),
                          ("ON YOUR SHELF", if on_shelf { "Yes".into() } else { "No".into() })] {
        lines.push((head.into(), false));
        lines.push((value, true));
    }
    lines
}

/// Words to lines of at most `room` (as `width` measures them); a word too
/// long for a line is cut.
fn wrap(text: &str, room: u32, width: impl Fn(&str) -> u32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        let tried = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
        if width(&tried) <= room || line.is_empty() && width(word) <= room {
            line = tried;
            continue;
        }
        if !line.is_empty() { lines.push(std::mem::take(&mut line)); }
        // A word longer than a line: cut where it fits.
        let mut rest = word.to_owned();
        while width(&rest) > room && rest.chars().count() > 1 {
            let mut cut = rest.chars().count() - 1;
            while cut > 1 && width(&rest.chars().take(cut).collect::<String>()) > room { cut -= 1; }
            lines.push(rest.chars().take(cut).collect());
            rest = rest.chars().skip(cut).collect();
        }
        line = rest;
    }
    if !line.is_empty() { lines.push(line); }
    lines
}

/// "Rev 1", "v1.1"… from the name, else RomM's; none: the first release.
fn version_of(title: &str, romm: &str) -> String {
    groups(title).into_iter().find(|g| {
        let l = g.to_ascii_lowercase();
        l.starts_with("rev ") || l.starts_with('v') && l[1..].chars().next().is_some_and(|c| c.is_ascii_digit())
    }).map(str::to_owned)
        .or_else(|| (!romm.trim().is_empty()).then(|| romm.trim().to_owned()))
        .unwrap_or_else(|| "First release".into())
}

/// Bytes as the card says them.
fn size_text(bytes: i64) -> String {
    if bytes < 0 { return "Not given".into(); }
    let b = bytes as f64;
    if b < 1024.0 { return format!("{bytes} bytes"); }
    if b < 1024.0 * 1024.0 { return format!("{} KB", (b / 1024.0).round() as i64); }
    let mb = b / (1024.0 * 1024.0);
    if mb < 10.0 { format!("{mb:.1} MB") } else { format!("{} MB", mb.round() as i64) }
}

/// Right of the title: the download under way, or how many are picked.
fn corner_note(page: &RommPage) -> Option<String> {
    if let Some((done, total, percent)) = page.progress {
        let at = (done + 1).min(total);
        return Some(if percent >= 0 {
            format!("Downloading {at} of {total}, {}%", percent / 5 * 5)
        } else {
            format!("Downloading {at} of {total}")
        });
    }
    match page.picked.len() {
        0 if page.filter.active() => Some("Filters on".into()),
        0 => None,
        1 => Some("1 game selected".into()),
        n => Some(format!("{n} games selected")),
    }
}

fn cart_size(page: &RommPage, i: usize) -> f32 {
    let t = page.grown_at.map_or(1.0, |at| slot_ui::ease((at.elapsed().as_secs_f32() / GROW_S).clamp(0.0, 1.0)));
    let me = (page.system, i);
    if me == page.grown { RESTING + (CHOSEN - RESTING) * t }
    else if Some(me) == page.shrinking { CHOSEN - (CHOSEN - RESTING) * t }
    else { RESTING }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i64, platform: &str, title: &str) -> RommEntry {
        RommEntry {
            id, platform: platform.into(), title: title.into(), label: String::new(),
            code: String::new(), gb_title: String::new(), gb_code: String::new(),
            gb_japan: false, gb_cgb_flag: None, file: String::new(), size: -1, region: String::new(),
            version: String::new(), favorite: false,
        }
    }

    #[test]
    fn games_sort_into_their_systems_and_know_the_shelf() {
        let systems = systems_of(&[
            entry(3, "GBA", "Zelda - The Minish Cap (Europe)"),
            entry(1, "GBA", "Advance Wars (USA)"),
            entry(7, "MD", "Sonic The Hedgehog (USA, Europe)"),
            entry(9, "PSX", "Not one of ours"),
            entry(4, "GB", "  "),
        ], |p, stem| p == Platform::Gba && stem == "Advance Wars (USA)");
        assert_eq!(systems.len(), Platform::ALL.len());
        let gba = &systems[Platform::Gba as usize];
        assert_eq!(gba.ids, [1, 3]);
        assert_eq!(gba.on_shelf, [true, false]);
        assert_eq!(gba.carts[0].rom, std::path::PathBuf::from("romm://1"));
        assert_eq!(systems[Platform::Md as usize].ids, [7]);
        assert!(systems[Platform::Md as usize].carts[0].shell.is_some(), "home carts wear their mould");
        assert!(systems[Platform::Gb as usize].carts.is_empty());
    }

    #[test]
    fn a_game_boy_header_gives_the_shelf_shell() {
        let plain = cart_of(&entry(5, "GBC", "Some Game (USA)")).unwrap().1;
        let mut read = entry(5, "GBC", "Some Game (USA)");
        read.gb_title = "SOMEGAME".into();
        read.gb_cgb_flag = Some(0xc0);
        let colour_only = cart_of(&read).unwrap().1;
        let local = crate::library::remote_shell(Platform::Gbc, "x", Some(crate::library::RemoteGbHeader {
            title: "SOMEGAME".into(), code: String::new(), japan: false, cgb_flag: Some(0xc0),
        }));
        assert_eq!(colour_only.shell, local);
        assert_ne!(colour_only.shell, plain.shell, "a Game Boy Color only cart is clear");
        let mut gba = entry(6, "GBA", "Some Game (USA)");
        gba.code = "AXVE".into();
        assert_eq!(cart_of(&gba).unwrap().1.code, "AXVE");
    }

    #[test]
    fn the_grid_is_two_rows_of_four_on_the_kpa() {
        let g = grid();
        assert_eq!(g.rows, 2);
        assert_eq!(g.cols, ((out_w() as f32 - 36.0) / 168.0) as usize);
        if out_w() == 720 { assert_eq!(g.cols, 4); }
    }

    #[test]
    fn statuses_are_short_plain_text() {
        assert_eq!(clean("a\nb\tc"), "abc");
        assert_eq!(clean(&"x".repeat(500)).len(), MAX_TEXT);
    }

    #[test]
    fn face_keys_carry_the_label_version() {
        let key = 37 | (2 << VERSION_SHIFT);
        assert_eq!(key & INDEX_MASK, 37);
        assert_eq!(key >> VERSION_SHIFT, 2);
    }

    #[test]
    fn corner_note_reads_picks_then_the_download() {
        let mut page = RommPage {
            all: Vec::new(), filter: RommFilter::default(), info: None,
            systems: Vec::new(), system: 0, cursor: 0, first_row: 0, picked: HashSet::new(),
            status: None, loading: false, progress: None, downloading: None, faces: HashMap::new(),
            labels_asked: Vec::new(), letter: None, letter_at: None, grown: (0, 0), shrinking: None, grown_at: None,
        };
        assert_eq!(corner_note(&page), None);
        page.picked.extend([1, 2]);
        assert_eq!(corner_note(&page).as_deref(), Some("2 games selected"));
        page.progress = Some((1, 2, 47));
        assert_eq!(corner_note(&page).as_deref(), Some("Downloading 2 of 2, 45%"));
    }

    #[test]
    fn filters_keep_what_they_say() {
        let mut f = RommFilter::default();
        let crash = entry(1, "GBA", "Crash Bandicoot - The Huge Adventure (USA)");
        assert!(f.keeps(&crash, false));
        f.search = "huge  CRASH".into();
        assert!(f.keeps(&crash, false));
        f.search = "pokemon".into();
        assert!(f.keeps(&entry(2, "GBA", "Pokémon - Version Émeraude (France)"), false));
        assert!(!f.keeps(&crash, false));
        f = RommFilter { hide_shelf: true, ..Default::default() };
        assert!(!f.keeps(&crash, true));
        f = RommFilter { favorites: true, ..Default::default() };
        assert!(!f.keeps(&crash, false));
        let mut fav = crash.clone();
        fav.favorite = true;
        assert!(f.keeps(&fav, false));
    }

    #[test]
    fn regions_world_games_show_everywhere() {
        assert!(in_region("Tetris (World)", "", "Japan"));
        assert!(in_region("Sonic (USA, Europe)", "", "Europe"));
        assert!(in_region("Pokemon (France)", "", "Europe"));
        assert!(!in_region("Pokemon (France)", "", "USA"));
        assert!(!in_region("Mother 3 (Japan)", "", "World"));
        assert!(in_region("Some Game", "USA", "USA"), "RomM's regions when the name has none");
        assert_eq!(regions_of("Zelda (Japan, USA) (Rev 1)"), ["Japan", "USA"]);
    }

    #[test]
    fn official_only_leaves_out_hacks_homebrew_and_translations() {
        assert!(is_official("Metroid Fusion (USA)"));
        assert!(is_official("Pokemon Red (USA, Europe) (SGB Enhanced)"));
        assert!(is_official("Zelda (Japan) (Rev 1)"));
        assert!(is_official("Kirby (Japan) (En,Ja)"));
        for name in ["Pokemon - Professor Oak Challenge (Red Version) (Hack)",
                     "A P.T. Silent Hills Demake (World) (GB Studio) (Jonshaw01) (Aftermarket)",
                     "King James Bible (USA) (Unl)", "Mario (USA) (Proto)", "Game (Europe) (Beta 2)",
                     "SonyC (World) (Demo - MSX Port) (Unl)",
                     "Cave Noire (Japan) (En) (Aeon Genesis) (v1.0)",
                     "Ayakashi no Shiro (Japan) (En) (KingMike's Translations) (v1.1)",
                     "SaGa 2 - A Haniwas Contingency (Subset - Insane Mode)", "Tetris [h1]", "Zelda [b]"] {
            assert!(!is_official(name), "{name}");
        }
    }

    #[test]
    fn the_info_card_says_name_size_region_version_and_shelf() {
        let mut e = entry(1, "GBA", "Golden Sun (USA, Europe) (Rev 1)");
        e.file = "Golden Sun (USA, Europe) (Rev 1).zip".into();
        e.size = 4_404_019;
        let lines = info_lines("GOLDEN SUN", &e, true, 10_000, |t| t.len() as u32);
        let texts: Vec<&str> = lines.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(texts, ["GOLDEN SUN", "FILE NAME", "Golden Sun (USA, Europe) (Rev 1).zip", "SIZE", "4.2 MB",
                           "REGION", "USA, Europe", "VERSION", "Rev 1", "ON YOUR SHELF", "Yes"]);
        assert_eq!(version_of("Tetris (World)", ""), "First release");
        assert_eq!(version_of("Wario (USA) (v1.1)", ""), "v1.1");
        assert_eq!(size_text(524_288), "512 KB");
        assert_eq!(size_text(16_777_216), "16 MB");
    }

    #[test]
    fn long_file_names_wrap_by_words() {
        let width = |s: &str| s.chars().count() as u32;
        assert_eq!(wrap("Castlevania - Aria of Sorrow (USA).zip", 16, width),
                   ["Castlevania -", "Aria of Sorrow", "(USA).zip"]);
        assert_eq!(wrap("abcdefghij", 4, width), ["abcd", "efgh", "ij"]);
    }

    #[test]
    fn a_long_file_name_wraps_at_the_cards_edge() {
        let (out_w, pad) = (720, 16);
        let width = |t: &str| t.chars().count() as u32 * 14 + pad;
        let room = info_room(out_w);
        // Drawn at INFO_SCALE, a full line reaches the card's right edge.
        assert_eq!(room as f32 * INFO_SCALE, out_w as f32 - 2.0 * INFO_X - 24.0);
        let name = "Game Boy Advance Video - Cartoon Network Collection - Platinum Edition (USA, Europe) (Rev 1).gba";
        let lines = wrap(name, room, width);
        assert!(lines.len() >= 2);
        assert!(lines.iter().all(|l| width(l) <= room));
        assert_eq!(name, lines.join(" "), "every word is shown");
    }

}
