//! Cart Studio on the engine: one cart's label and shell, edited in place
//! with the controller, shown on the cart as the shelf draws it, and saved to
//! the very files the shelf reads (Labels/<key>.png, Studio/<key>.shell).
//! The label pictures come from Android (UI action 30, nativeStudioArt):
//! the art set's scan as the automatic labels prepare it, and its logo.
//! Opened from the carousel (X), A saves the cart at once; opened from the
//! library page, the changes wait there until the page's Save.
use super::*;
use slot_store::{Cart, Outline, ShellChoice};
use slot_ui::{Finish, Shell};

const LABELS: [&str; 6] = ["Current", "Real Label", "Logo Only", "slot. label", "Search game…", "Picture…"];
const SEARCH_LABEL: usize = 4;
const PICTURE_LABEL: usize = 5;
const BACKGROUNDS: [(&str, Option<f32>); 9] = [
    ("Auto", None), ("Red", Some(2.0)), ("Orange", Some(26.0)), ("Amber", Some(42.0)),
    ("Green", Some(130.0)), ("Teal", Some(178.0)), ("Blue", Some(215.0)),
    ("Purple", Some(268.0)), ("Pink", Some(325.0)),
];
/// "Original" is the finish the real cart had in the chosen colour.
const FINISHES: [&str; 5] = ["Original", "Solid", "Clear", "Solid glitter", "Clear glitter"];
const FINISH_OF: [Finish; 4] = [Finish::Solid, Finish::Translucent, Finish::SolidGlitter, Finish::ClearGlitter];
const ROWS: [&str; 5] = ["Label", "Background", "Colour", "Finish", "Shape"];
const ROW_LABEL: usize = 0;
const ROW_BACKGROUND: usize = 1;
const ROW_COLOUR: usize = 2;
const ROW_FINISH: usize = 3;
const ROW_SHAPE: usize = 4;
/// The rows start higher than a settings page's: the finish strip goes under.
const ROWS_TOP: f32 = 84.0;
const STRIP_BOTTOM: f32 = 414.0;
/// The finish strip's height under the rows.
const FINISH_STRIP_H: f32 = 70.0;
/// Search game's keyboard, QWERTY: ten keys a row; "<" deletes, "_" is a space.
const KEYS: [char; 40] = [
    '1', '2', '3', '4', '5', '6', '7', '8', '9', '0',
    'Q', 'W', 'E', 'R', 'T', 'Y', 'U', 'I', 'O', 'P',
    'A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L', '-',
    'Z', 'X', 'C', 'V', 'B', 'N', 'M', '&', '_', '<',
];
const RESULTS_SHOWN: usize = 4;
const QUERY_MAX: usize = 40;

/// A picture decoded: straight RGBA and its size.
type Rgba = (Vec<u8>, u32, u32);

/// The label of another game, chosen by name (Search game…).
pub(super) struct Alt {
    name: String,
    real_png: Option<Vec<u8>>,
    real: Option<Rgba>,
    logo: Option<Rgba>,
}

/// A picture of the player's own (Picture…): a whole label when opaque, a
/// logo on slot.'s ground when it has transparency.
pub(super) struct Picture {
    png: Vec<u8>,
    rgba: Rgba,
    transparent: bool,
}

/// One searchable game name of the art set.
pub(super) struct Name {
    pub(super) platform: &'static str,
    pub(super) crc: String,
    pub(super) name: String,
    words: Vec<String>,
}

/// The Search game page: the query, the keyboard and the matches.
pub(super) struct Search {
    query: String,
    key: usize,
    on_results: bool,
    result: usize,
    names: Vec<Name>,
    hits: Vec<usize>,
}

/// One colour of the Colour row: a swatch's plastic, or None for the cart's
/// own automatic shell (Original).
struct Colour {
    name: String,
    shell: Option<Shell>,
}

/// The tiles of the colour chart and the finish strip: the shells wanted
/// and the faces made so far.
#[derive(Default)]
struct Minis {
    wanted: Vec<Option<ShellChoice>>,
    tex: Vec<Option<(slot_gfx::TexId, u32, u32)>>,
}

/// A face at exactly its texels (a colour tile).
struct SmallFace { rgba: Vec<u8>, w: u32, h: u32 }

impl slot_ui::Face for SmallFace {
    fn rgba(&self) -> &[u8] { &self.rgba }
    fn texels(&self) -> (u32, u32) { (self.w, self.h) }
}

/// What a Cart Studio session changes on its cart.
pub(super) struct Outcome {
    /// A new label picture to save.
    pub(super) label: Option<Vec<u8>>,
    /// A new shell choice to save (None in it: the automatic shell).
    pub(super) shell: Option<Option<ShellChoice>>,
}

impl Outcome {
    pub(super) fn is_empty(&self) -> bool { self.label.is_none() && self.shell.is_none() }
}

/// A Cart Studio session: the cart as opened, and the choice on each row.
pub(super) struct Studio {
    uri: String,
    original: Cart,
    /// The cart's automatic shell, and the Studio choice it had on opening.
    auto: Option<ShellChoice>,
    stored: Option<ShellChoice>,
    row: usize,
    label: usize,
    background: usize,
    colour: usize,
    finish: usize,
    outline: usize,
    /// Colour, finish and shape as opened.
    start: (usize, usize, usize),
    colours: Vec<Colour>,
    /// Where the colour chart's FUN section starts in `colours`.
    fun_start: usize,
    /// A label already chosen in this library session, not saved yet.
    pending_label: Option<(Vec<u8>, Rgba)>,
    /// The art set's label as Android prepared it (saved as is) and decoded.
    real_png: Option<Vec<u8>>,
    real: Option<Rgba>,
    logo: Option<Rgba>,
    waiting: bool,
    alt: Option<Alt>,
    picture: Option<Picture>,
    search: Option<Search>,
    /// The colour chart is open; the colour it was opened on.
    chart: Option<usize>,
    preview: Option<(slot_gfx::TexId, u32, u32)>,
    minis: Minis,
    stale: bool,
    /// Opened from the library page: B and A go back there.
    pub(super) from_library: bool,
}

fn outlines(platform: Platform) -> Vec<(&'static str, Outline)> {
    let mut out = vec![("Automatic", Outline::Auto)];
    out.extend_from_slice(match platform {
        Platform::Gb | Platform::Gbc => &[("Game Boy", Outline::Notched), ("Game Boy Color", Outline::Rounded)][..],
        Platform::Nes => &[("NES", Outline::Nes), ("Famicom", Outline::Famicom),
            ("Famicom Namco", Outline::FamicomNamco), ("Famicom Konami", Outline::FamicomKonami)][..],
        Platform::Snes => &[("Super Nintendo", Outline::Snes), ("Super Nintendo US", Outline::SnesUs)][..],
        Platform::Md => &[("Mega Drive", Outline::MegaDrive), ("Mega Drive Japan", Outline::MegaDriveJp),
            ("Mega Drive EA", Outline::MegaDriveEa), ("Mega Drive Accolade", Outline::MegaDriveAccolade),
            ("Mega Drive Codemasters", Outline::MegaDriveCodemasters), ("Virtua Racing", Outline::MegaDriveVr),
            ("Sonic & Knuckles", Outline::MegaDriveSk)][..],
        Platform::Gba => &[][..],
    });
    out
}

/// A shell's finish read back from a saved choice.
fn finish_of(finish: slot_store::ShellFinish) -> Finish {
    use slot_store::ShellFinish as F;
    match finish {
        F::Solid => Finish::Solid,
        F::Clear => Finish::Translucent,
        F::Glitter => Finish::Glitter,
        F::SolidGlitter => Finish::SolidGlitter,
        F::ClearGlitter => Finish::ClearGlitter,
    }
}

/// The Finish row's place for a finish ("glitter" on its system's plastic).
fn finish_index(finish: Finish, platform: Platform) -> usize {
    let home = matches!(platform, Platform::Nes | Platform::Snes | Platform::Md);
    match finish {
        Finish::Solid => 1,
        Finish::Translucent => 2,
        Finish::SolidGlitter => 3,
        Finish::ClearGlitter => 4,
        Finish::Glitter => if home { 3 } else { 4 },
    }
}

/// The same finish, whatever the system calls it.
fn same_finish(a: Finish, b: Finish, platform: Platform) -> bool {
    finish_index(a, platform) == finish_index(b, platform)
}

/// The search's first query: the cart's title as its label prints it.
fn title_query(cart: &Cart) -> String {
    let title: String = slot_ui::label_text(cart).split_whitespace().collect::<Vec<_>>().join(" ");
    title.to_uppercase().chars().take(QUERY_MAX).collect()
}

/// The logical size of a colour chart or finish strip tile.
const TILE: u32 = 40;

/// A rounded square of plastic: clear shows a checker through it, glitter
/// has white flecks.
fn swatch_tile(colour: [u8; 3], clear: bool, glitter: bool) -> SmallFace {
    let n = slot_ui::density::px(TILE).max(8);
    let r = n as f32 * 0.22;
    let mut rgba = vec![0u8; (n * n * 4) as usize];
    let check = (n / 6).max(2);
    for y in 0..n {
        for x in 0..n {
            // Distance outside the rounded corner, for a soft edge.
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let cx = fx.clamp(r, n as f32 - r);
            let cy = fy.clamp(r, n as f32 - r);
            let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() - r;
            let cover = (0.5 - d).clamp(0.0, 1.0);
            if cover <= 0.0 { continue; }
            let mut px = colour.map(f32::from);
            if clear {
                let light = ((x / check) + (y / check)) % 2 == 0;
                let under = if light { 210.0 } else { 120.0 };
                for c in &mut px { *c = *c * 0.62 + under * 0.38; }
            }
            // A little light from the top, like the plastic.
            let shade = 1.08 - 0.16 * fy / n as f32;
            for c in &mut px { *c = (*c * shade).min(255.0); }
            if glitter {
                let h = (x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663)) % 97;
                if h < 6 { px = [255.0; 3]; }
            }
            let i = ((y * n + x) * 4) as usize;
            for c in 0..3 { rgba[i + c] = px[c] as u8; }
            rgba[i + 3] = (cover * 255.0) as u8;
        }
    }
    SmallFace { rgba, w: n, h: n }
}

pub(super) fn platform_key(platform: Platform) -> &'static str {
    match platform {
        Platform::Gba => "GBA", Platform::Gb => "GB", Platform::Gbc => "GBC",
        Platform::Nes => "NES", Platform::Snes => "SNES", Platform::Md => "MD",
    }
}

/// The systems whose labels a cart may borrow: its own, the Game Boy and
/// the Game Boy Color together (many games came out on both).
pub(super) fn search_platforms(platform: Platform) -> &'static [&'static str] {
    match platform {
        Platform::Gb | Platform::Gbc => &["GB", "GBC"],
        Platform::Gba => &["GBA"],
        Platform::Nes => &["NES"],
        Platform::Snes => &["SNES"],
        Platform::Md => &["MD"],
    }
}

/// Lower-case words, accents dropped: "Pokémon - Version Jaune" -> pokemon version jaune.
fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    for c in text.chars() {
        let c = match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'À' | 'Á' | 'Â' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'Ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'Ü' => 'u',
            'ç' => 'c', 'ñ' => 'n',
            c => c,
        };
        if c.is_ascii_alphanumeric() || c == '&' {
            word.push(c.to_ascii_lowercase());
        } else if !word.is_empty() {
            out.push(std::mem::take(&mut word));
        }
    }
    if !word.is_empty() { out.push(word); }
    out
}

/// Words of a cart's own title worth matching on: not the generic ones.
fn title_words(title: &str) -> Vec<String> {
    const SKIP: [&str; 16] = ["the", "of", "and", "a", "version", "edition", "hack", "rev", "beta",
        "v1", "v2", "v3", "usa", "europe", "japan", "world"];
    words(title).into_iter().filter(|w| w.len() > 1 && !SKIP.contains(&w.as_str())).collect()
}

pub(super) fn load_names(root: &std::path::Path, platforms: &[&'static str]) -> Vec<Name> {
    let mut out = Vec::new();
    for platform in platforms {
        let Ok(text) = std::fs::read_to_string(root.join("ArtNames").join(format!("{platform}.tsv"))) else { continue };
        for line in text.lines().take(30_000) {
            let Some((crc, name)) = line.split_once('\t') else { continue };
            out.push(Name { platform: *platform, crc: crc.to_owned(), name: name.to_owned(), words: words(name) });
        }
    }
    out
}

/// The names matching a query, best first; with no query, the names closest
/// to the cart's own title (a rom hack's original game first).
pub(super) fn search_hits(names: &[Name], query: &str, title: &str) -> Vec<usize> {
    let asked = words(query);
    let mut scored: Vec<(i32, usize, usize)> = Vec::new();
    if asked.is_empty() {
        let own = title_words(title);
        if own.is_empty() { return Vec::new(); }
        for (i, n) in names.iter().enumerate() {
            let hits = own.iter().filter(|w| n.words.iter().any(|nw| nw == *w)).count() as i32;
            if hits > 0 { scored.push((-hits * 10 + region_penalty(&n.name), n.name.len(), i)); }
        }
    } else {
        for (i, n) in names.iter().enumerate() {
            if asked.iter().all(|w| n.words.iter().any(|nw| nw.starts_with(w.as_str()))) {
                scored.push((region_penalty(&n.name), n.name.len(), i));
            }
        }
    }
    scored.sort();
    scored.into_iter().take(60).map(|(_, _, i)| i).collect()
}

/// USA, Europe and World releases first, revisions and betas after.
fn region_penalty(name: &str) -> i32 {
    let mut p = 3;
    if name.contains("USA") || name.contains("Europe") || name.contains("World") { p = 0; }
    else if name.contains("Japan") { p = 1; }
    if name.contains("(Rev") || name.contains("Beta") || name.contains("Proto") || name.contains("(Demo") { p += 4; }
    p
}

pub(super) fn encode_png(rgba: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().ok()?;
    writer.write_image_data(rgba).ok()?;
    writer.finish().ok()?;
    Some(out)
}

impl Studio {
    fn new(cart: Cart, pending_label: Option<Vec<u8>>, from_library: bool) -> Studio {
        let uri = cart.rom.to_string_lossy().into_owned();
        let platform = cart.platform;
        let auto = crate::library::auto_shell_of(&uri).or_else(|| {
            // Not in the library's index (a test cart): the shelf's own.
            matches!(platform, Platform::Nes | Platform::Snes | Platform::Md)
                .then(|| slot_ui::home_shell(platform, &cart.stem, "", "")).flatten()
        });
        let stored = cart.shell.filter(|s| Some(*s) != auto);
        let mut colours = Vec::new();
        let mut start = (0, 0, 0);
        let swatches = slot_ui::system_swatches(platform);
        if let Some(s) = stored {
            let worn = finish_of(s.finish);
            match swatches.iter().chain(slot_ui::FUN_SWATCHES).position(|w| w.shell.colour == s.colour) {
                Some(i) => {
                    let swatch = swatches.iter().chain(slot_ui::FUN_SWATCHES).nth(i).expect("found");
                    start.0 = i + 1;
                    if !same_finish(swatch.shell.finish, worn, platform) {
                        start.1 = finish_index(worn, platform);
                    }
                }
                None => colours.push(Colour { name: "Custom".into(), shell: Some(Shell { colour: s.colour, finish: worn }) }),
            }
            let auto_outline = auto.map(|a| a.outline);
            if Some(s.outline) != auto_outline && s.outline != Outline::Auto {
                start.2 = outlines(platform).iter().position(|(_, o)| *o == s.outline).unwrap_or(0);
            }
        }
        let custom = colours.len();
        if custom > 0 { start.0 = 0; }
        colours.push(Colour { name: "Original".into(), shell: None });
        if stored.is_none() { start.0 = custom; }
        colours.extend(swatches.iter().map(|w| Colour { name: w.name.into(), shell: Some(w.shell) }));
        let fun_start = colours.len();
        colours.extend(slot_ui::FUN_SWATCHES.iter().map(|w| Colour { name: w.name.into(), shell: Some(w.shell) }));
        let pending_label = pending_label.and_then(|png| slot_ui::decode_png(&png).map(|rgba| (png, rgba)));
        Studio {
            uri, original: cart, auto, stored, row: 0, label: 0, background: 0,
            colour: start.0, finish: start.1, outline: start.2, start, colours, fun_start, pending_label,
            real_png: None, real: None, logo: None, waiting: true, alt: None, picture: None, search: None,
            chart: None, preview: None, minis: Minis::default(), stale: true, from_library,
        }
    }

    /// The rows this cart has: Shape only when its system has several.
    fn rows(&self) -> Vec<usize> {
        let mut rows = vec![ROW_LABEL, ROW_BACKGROUND, ROW_COLOUR, ROW_FINISH];
        if outlines(self.original.platform).len() > 1 { rows.push(ROW_SHAPE); }
        rows
    }

    fn row_id(&self) -> usize { self.rows().get(self.row).copied().unwrap_or(ROW_LABEL) }

    fn shell_changed(&self) -> bool { (self.colour, self.finish, self.outline) != self.start }

    /// The automatic shell's plastic, as the shelf draws it.
    fn auto_plastic(&self) -> Shell {
        let mut cart = self.original.clone();
        cart.shell = self.auto;
        slot_ui::shell_for(&cart)
    }

    /// The Studio choice for a colour, a finish and a shape (None: the
    /// automatic shell, no choice saved).
    fn choice(&self, colour: usize, finish: usize, outline: usize) -> Option<ShellChoice> {
        if (colour, finish, outline) == self.start { return self.stored; }
        let base = self.colours.get(colour).and_then(|c| c.shell);
        if base.is_none() && finish == 0 && outline == 0 { return None; }
        let plastic = base.unwrap_or_else(|| self.auto_plastic());
        let worn = if finish == 0 { plastic.finish } else { FINISH_OF[finish - 1] };
        let shape = match outline {
            0 => self.auto.map_or(Outline::Auto, |a| a.outline),
            k => outlines(self.original.platform).get(k).map_or(Outline::Auto, |(_, o)| *o),
        };
        Some(ShellChoice { outline: shape, colour: plastic.colour, finish: worn.stored() })
    }

    /// The shell the cart wears with that choice, as the shelf resolves it.
    fn worn(&self, colour: usize, finish: usize, outline: usize) -> Option<ShellChoice> {
        crate::library::chosen_shell(self.original.platform, self.auto, self.choice(colour, finish, outline))
    }

    fn draft_cart(&self) -> Cart {
        let mut cart = self.original.clone();
        cart.shell = self.worn(self.colour, self.finish, self.outline);
        cart
    }

    /// Which logo the label shows on slot.'s ground, if it is one.
    fn shown_logo(&self) -> Option<&Rgba> {
        match self.label {
            2 => self.logo.as_ref(),
            SEARCH_LABEL => self.alt.as_ref().filter(|a| a.real.is_none()).and_then(|a| a.logo.as_ref()),
            PICTURE_LABEL => self.picture.as_ref().filter(|p| p.transparent).map(|p| &p.rgba),
            _ => None,
        }
    }

    fn logo_art(&self, cart: &Cart, logo: &Rgba) -> Rgba {
        let luma = slot_ui::logo_label::logo_luma(&logo.0);
        let hue = BACKGROUNDS[self.background].1.unwrap_or_else(|| {
            self.real.as_ref().and_then(|(rgba, _, _)| slot_ui::logo_label::dominant_hue(rgba))
                .unwrap_or_else(|| slot_ui::logo_label::rgb_to_hsl(slot_ui::label_colour(&slot_ui::label_text(cart))).0)
        });
        let (deep, pale) = slot_ui::logo_label::stops(hue, luma);
        slot_ui::logo_label_art(cart, &logo.0, logo.1, logo.2, deep, pale)
    }

    fn fitted(cart: &Cart, picture: &Rgba) -> Option<Rgba> {
        let (lw, lh) = slot_ui::label_size(cart);
        slot_ui::label_art_from_rgba(cart, &picture.0, picture.1, picture.2)
            .map(|art| (art, slot_ui::density::px(lw), slot_ui::density::px(lh)))
    }

    /// The label art for the cart (None: the shelf's own label file), and
    /// what to tell the player about it.
    fn label_art(&self, cart: &Cart) -> (Option<Rgba>, Option<String>) {
        if let Some(logo) = self.shown_logo() {
            return (Some(self.logo_art(cart, logo)), None);
        }
        match self.label {
            0 => (self.pending_label.as_ref().and_then(|(_, rgba)| Self::fitted(cart, rgba)), None),
            1 => match &self.real {
                Some(real) => (Self::fitted(cart, real), None),
                None if self.waiting => (None, Some("Looking for the real label…".into())),
                None => (None, Some("No real label for this cart yet.".into())),
            },
            2 if self.waiting => (None, Some("Looking for the logo…".into())),
            2 => (None, Some("No logo for this cart yet.".into())),
            3 => (Some(slot_ui::generated_label_art(cart)), None),
            SEARCH_LABEL => match &self.alt {
                Some(Alt { real: Some(real), name, .. }) => (Self::fitted(cart, real), Some(name.clone())),
                Some(Alt { name, .. }) if self.waiting => (None, Some(format!("Getting {name}…"))),
                Some(Alt { name, .. }) => (None, Some(format!("No label for {name}."))),
                None => (None, Some("A: search a game by name.".into())),
            },
            PICTURE_LABEL => match &self.picture {
                Some(picture) => (Self::fitted(cart, &picture.rgba), None),
                None if self.waiting => (None, Some("Opening your picture…".into())),
                None => (None, Some("A: choose a picture.".into())),
            },
            _ => (None, None),
        }
    }

    /// The session's changes: the label to save and the shell choice.
    fn outcome(&self) -> Outcome {
        let cart = self.draft_cart();
        let composed = || self.label_art(&cart).0.and_then(|(rgba, w, h)| encode_png(&rgba, w, h));
        let label = match self.label {
            0 => self.pending_label.as_ref().map(|(png, _)| png.clone()),
            1 => self.real_png.clone(),
            2 | 3 => composed(),
            SEARCH_LABEL => match &self.alt {
                Some(Alt { real_png: Some(png), .. }) => Some(png.clone()),
                Some(_) => composed(),
                None => None,
            },
            PICTURE_LABEL => match &self.picture {
                Some(picture) if !picture.transparent => Some(picture.png.clone()),
                Some(_) => composed(),
                None => None,
            },
            _ => None,
        };
        let shell = self.shell_changed().then(|| self.choice(self.colour, self.finish, self.outline));
        Outcome { label, shell }
    }

    fn value(&self, row: usize) -> (String, bool) {
        match row {
            ROW_LABEL => (LABELS[self.label].to_owned(), true),
            ROW_BACKGROUND => if self.shown_logo().is_some() { (BACKGROUNDS[self.background].0.to_owned(), true) }
                else { ("—".to_owned(), false) },
            ROW_COLOUR => (self.colours.get(self.colour).map_or_else(String::new, |c| c.name.clone()), true),
            ROW_FINISH => (FINISHES[self.finish].to_owned(), true),
            _ => {
                let list = outlines(self.original.platform);
                (list[self.outline.min(list.len() - 1)].0.to_owned(), true)
            }
        }
    }

    fn cycle(&mut self, right: bool) {
        let step = |v: usize, n: usize| if right { (v + 1) % n } else { (v + n - 1) % n };
        match self.row_id() {
            ROW_LABEL => self.label = step(self.label, LABELS.len()),
            ROW_BACKGROUND if self.shown_logo().is_some() => self.background = step(self.background, BACKGROUNDS.len()),
            ROW_COLOUR => self.colour = step(self.colour, self.colours.len()),
            ROW_FINISH => self.finish = step(self.finish, FINISHES.len()),
            ROW_SHAPE => {
                let n = outlines(self.original.platform).len();
                if n > 1 { self.outline = step(self.outline, n); }
            }
            _ => return,
        }
        self.stale = true;
    }

    /// The minis on screen: every colour in the chart, or every finish on
    /// the Finish row.
    fn wanted_minis(&self) -> Vec<Option<ShellChoice>> {
        if self.chart.is_some() {
            (0..self.colours.len()).map(|c| self.worn(c, self.finish, self.outline)).collect()
        } else if self.search.is_none() && self.row_id() == ROW_FINISH {
            (0..FINISHES.len()).map(|f| self.worn(self.colour, f, self.outline)).collect()
        } else {
            Vec::new()
        }
    }

    /// The colour chart's two sections: the system's colours (with
    /// Original) and the fun ones; a colour's section, row and column.
    fn chart_place(&self, colour: usize, cols: usize) -> (usize, usize, usize) {
        let (section, i) = if colour < self.fun_start { (0, colour) } else { (1, colour - self.fun_start) };
        (section, i / cols, i % cols)
    }

    fn chart_move(&mut self, code: i32, cols: usize) {
        let n = self.colours.len();
        let sizes = [self.fun_start, n - self.fun_start];
        let base = [0, self.fun_start];
        let (section, row, col) = self.chart_place(self.colour, cols);
        let rows = |s: usize| sizes[s].div_ceil(cols);
        let at = |s: usize, r: usize, c: usize| base[s] + (r * cols + c).min(sizes[s] - 1);
        self.colour = match code {
            21 => (self.colour + n - 1) % n,
            22 => (self.colour + 1) % n,
            19 if row > 0 => at(section, row - 1, col),
            19 => { let s = 1 - section; at(s, rows(s) - 1, col) }
            20 if row + 1 < rows(section) => at(section, row + 1, col),
            20 => at(1 - section, 0, col),
            _ => return,
        };
        self.stale = true;
    }
}

fn key_face(key: char) -> String {
    match key { '_' => "SPACE".to_owned(), '<' => "DEL".to_owned(), c => c.to_string() }
}

/// The colour chart's columns and cell size on the right half.
struct ChartGrid { x0: f32, top: f32, cols: usize, cell_w: f32, cell_h: f32, gap: f32 }

fn chart_grid(x0: f32, x1: f32, systems: usize, fun: usize) -> ChartGrid {
    let width = x1 - x0;
    // Two section titles and the chosen colour's name share the height.
    let room = STRIP_BOTTOM - 96.0 - 3.0 * 26.0;
    // As many columns as make the carts largest, on any screen.
    let size = |cols: usize| {
        let rows = systems.div_ceil(cols) + fun.div_ceil(cols);
        let cell_w = width / cols as f32;
        let side = cell_w.min(room / rows.max(1) as f32).min(56.0);
        (side, side)
    };
    let cols = (4..=10).max_by(|&a, &b| {
        let (wa, ha) = size(a);
        let (wb, hb) = size(b);
        wa.min(ha).total_cmp(&wb.min(hb)).then(a.cmp(&b))
    }).unwrap_or(6);
    let (cell_w, cell_h) = size(cols);
    let x0 = x0 + (width - cols as f32 * cell_w) / 2.0;
    ChartGrid { x0, top: 96.0, cols, cell_w, cell_h, gap: 26.0 }
}

impl ChartGrid {
    /// A colour's cell: its top-left corner.
    fn cell(&self, section: usize, row: usize, col: usize, system_rows: usize) -> (f32, f32) {
        let y = self.top + self.gap + if section == 0 { row as f32 * self.cell_h }
            else { system_rows as f32 * self.cell_h + self.gap + row as f32 * self.cell_h };
        (self.x0 + col as f32 * self.cell_w, y)
    }
}

impl Engine {
    /// X on a cart: open Cart Studio on it and ask Android for its art.
    pub(super) fn open_cart_studio(&mut self) {
        let shelf = &self.shelves[self.active];
        let Some(cart) = shelf.carts.get(shelf.index).cloned() else { return };
        self.open_cart_studio_on(cart, None, false);
    }

    /// Cart Studio on `cart`; `pending` is a label the library session chose
    /// for it and hasn't saved yet.
    pub(super) fn open_cart_studio_on(&mut self, cart: Cart, pending: Option<Vec<u8>>, from_library: bool) {
        let uri = cart.rom.to_string_lossy().into_owned();
        let mut studio = Studio::new(cart, pending, from_library);
        if let Some(old) = self.studio.take() {
            studio.preview = old.preview;
            self.studio_pool.extend(old.minis.tex.into_iter().flatten().map(|(tex, _, _)| tex));
        }
        self.studio = Some(studio);
        self.overlay = ShelfOverlay::CartStudio;
        *LABEL_PICK_REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = Some(uri);
        UI_ACTION.lock().unwrap_or_else(|e| e.into_inner()).push_back(30);
    }

    /// Android's answer. Kind 0: the cart's own real label (as the automatic
    /// labels save it) and logo; 1: another game's, by `name`; 2: a picture
    /// of the player's, opaque in `real` or with transparency in `logo`.
    pub(super) fn studio_art(&mut self, uri: String, real: Option<Vec<u8>>, logo: Option<Vec<u8>>,
                             kind: i32, name: String) {
        let Some(studio) = self.studio.as_mut() else { return };
        if studio.uri != uri { return; }
        let decode = |png: &Option<Vec<u8>>| png.as_deref().and_then(slot_ui::decode_png);
        match kind {
            1 => {
                let decoded = decode(&real);
                studio.alt = Some(Alt { name, real_png: real.filter(|_| decoded.is_some()), real: decoded,
                    logo: decode(&logo) });
                studio.label = SEARCH_LABEL;
            }
            2 => {
                let transparent = real.is_none();
                if let Some(png) = real.or(logo) {
                    if let Some(rgba) = slot_ui::decode_png(&png) {
                        studio.picture = Some(Picture { png, rgba, transparent });
                    }
                }
                studio.label = PICTURE_LABEL;
            }
            _ => {
                studio.real = decode(&real);
                studio.real_png = real.filter(|_| studio.real.is_some());
                studio.logo = decode(&logo);
            }
        }
        studio.waiting = false;
        studio.stale = true;
    }

    pub(super) fn cart_studio_key(&mut self, code: i32) {
        let Some(studio) = self.studio.as_mut() else {
            self.overlay = ShelfOverlay::None;
            return;
        };
        if studio.search.is_some() {
            self.search_key(code);
            return;
        }
        if let Some(before) = studio.chart {
            let (x0, x1) = studio_columns();
            let cols = chart_grid(x0, x1, studio.fun_start, studio.colours.len() - studio.fun_start).cols;
            match code {
                96 => { studio.chart = None; studio.stale = true; }
                97 | 108 => { studio.colour = before; studio.chart = None; studio.stale = true; }
                _ => studio.chart_move(code, cols),
            }
            return;
        }
        let rows = studio.rows().len();
        match code {
            19 | 20 => studio.row = move_menu_row(studio.row, code, rows, self.menu_repeat_dispatch),
            21 | 102 => studio.cycle(false),
            22 | 103 => studio.cycle(true),
            100 => {
                studio.label = 0; studio.background = 0;
                (studio.colour, studio.finish, studio.outline) = studio.start;
                studio.stale = true;
            }
            96 if studio.row_id() == ROW_LABEL && studio.label == SEARCH_LABEL => self.open_search(),
            96 if studio.row_id() == ROW_LABEL && studio.label == PICTURE_LABEL => {
                studio.waiting = true;
                studio.stale = true;
                *LABEL_PICK_REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = Some(studio.uri.clone());
                // Android's picker pauses slot.: keep Cart Studio open behind it.
                self.external_pending = true;
                UI_ACTION.lock().unwrap_or_else(|e| e.into_inner()).push_back(33);
            }
            96 if studio.row_id() == ROW_COLOUR => {
                studio.chart = Some(studio.colour);
                studio.stale = true;
            }
            96 => self.finish_cart_studio(true),
            97 | 108 => self.finish_cart_studio(false),
            _ => {}
        }
    }

    fn open_search(&mut self) {
        let Some(studio) = self.studio.as_mut() else { return };
        let root = PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|(r, _)| r.clone());
        let names = root.map(|r| load_names(&r, search_platforms(studio.original.platform))).unwrap_or_default();
        let title = slot_ui::label_text(&studio.original);
        let query = title_query(&studio.original);
        let hits = search_hits(&names, &query, &title);
        studio.search = Some(Search { query, key: 0, on_results: !hits.is_empty(),
            result: 0, names, hits });
    }

    fn search_key(&mut self, code: i32) {
        let Some(studio) = self.studio.as_mut() else { return };
        let title = slot_ui::label_text(&studio.original);
        let uri = studio.uri.clone();
        let Some(search) = studio.search.as_mut() else { return };
        let mut changed = false;
        match code {
            97 | 108 => { studio.search = None; return; }
            102 | 103 => search.on_results = !search.on_results && !search.hits.is_empty(),
            99 => { search.query.pop(); changed = true; }
            100 => { search.query.push(' '); changed = true; }
            // Up from the keyboard's top row reaches the matches, down from
            // the last match comes back to the keyboard (R1 does too).
            19 if search.on_results => search.result = search.result.saturating_sub(1),
            20 if search.on_results => {
                if search.result + 1 < search.hits.len() { search.result += 1; } else { search.on_results = false; }
            }
            19 if search.key < 10 => if !search.hits.is_empty() {
                search.on_results = true;
                search.result = search.result.min(search.hits.len() - 1);
            },
            19 => search.key -= 10,
            20 => search.key = (search.key + 10) % KEYS.len(),
            21 if !search.on_results => search.key = (search.key / 10) * 10 + (search.key % 10 + 9) % 10,
            22 if !search.on_results => search.key = (search.key / 10) * 10 + (search.key % 10 + 1) % 10,
            96 if search.on_results => {
                let Some(&i) = search.hits.get(search.result) else { return };
                let pick = &search.names[i];
                let request = format!("{uri}\t{}\t{}\t{}", pick.platform, pick.crc, pick.name);
                let name = pick.name.clone();
                studio.search = None;
                studio.alt = Some(Alt { name, real_png: None, real: None, logo: None });
                studio.label = SEARCH_LABEL;
                studio.waiting = true;
                studio.stale = true;
                *LABEL_PICK_REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = Some(request);
                UI_ACTION.lock().unwrap_or_else(|e| e.into_inner()).push_back(32);
                return;
            }
            96 => {
                match KEYS[search.key] {
                    '<' => { search.query.pop(); }
                    '_' => search.query.push(' '),
                    c => if search.query.chars().count() < QUERY_MAX { search.query.push(c) },
                }
                changed = true;
            }
            _ => {}
        }
        if changed {
            search.hits = search_hits(&search.names, &search.query, &title);
            search.result = 0;
            // Typing goes on on the keyboard; deleting shows the matches.
            if search.hits.is_empty() { search.on_results = false; }
        }
    }

    /// A or B in the editor. From the carousel, A saves the cart and B
    /// leaves it as it was; from the library page, both keep the changes
    /// there until the page's Save.
    fn finish_cart_studio(&mut self, save: bool) {
        let Some(studio) = self.studio.as_ref() else { return };
        if studio.from_library && self.cart_library.is_some() {
            let outcome = studio.outcome();
            let cart = studio.draft_cart();
            let art = studio.label_art(&cart).0.map(|(rgba, _, _)| rgba);
            if !outcome.is_empty() {
                let face = slot_ui::cart_face_with_material(&cart, art);
                self.keep_library_draft(cart, outcome, &face);
            }
            self.return_to_cart_library();
            return;
        }
        self.overlay = ShelfOverlay::None;
        if !save { return; }
        let uri = studio.uri.clone();
        let outcome = studio.outcome();
        if self.commit_cart(&uri, &outcome) { set_message("Cartridge saved.".into()); }
    }

    /// Saves a cart's new label and shell, and shows them at once on every
    /// shelf that holds it. False when a file couldn't be written.
    pub(super) fn commit_cart(&mut self, uri: &str, outcome: &Outcome) -> bool {
        let Some(root) = PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|(r, _)| r.clone()) else {
            set_message("Couldn't save the cartridge.".into());
            return false;
        };
        let key = core_selection::key(uri);
        let mut ok = true;
        if let Some(png) = &outcome.label {
            if let Err(error) = slot_store::atomic_write(&root.join("Labels").join(format!("{key}.png")), png) {
                set_error_message("Couldn't save the label.", &error);
                ok = false;
            }
        }
        if let Some(choice) = outcome.shell {
            let file = root.join("Studio").join(format!("{key}.shell"));
            let result = match choice {
                Some(choice) => std::fs::create_dir_all(root.join("Studio"))
                    .and_then(|_| slot_store::atomic_write(&file, choice.to_value().as_bytes())),
                None => match std::fs::remove_file(&file) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                    _ => Ok(()),
                },
            };
            match result {
                Ok(()) => crate::library::remember_shell(uri, choice),
                Err(error) => {
                    set_error_message("Couldn't save the shell.", &error);
                    ok = false;
                }
            }
        }
        self.show_cart_change(uri, outcome.shell);
        ok
    }

    /// Redraws a cart that Cart Studio changed, in place: no library reload.
    fn show_cart_change(&mut self, uri: &str, shell: Option<Option<ShellChoice>>) {
        let auto = shell.map(|_| crate::library::auto_shell_of(uri));
        let mut redraw = Vec::new();
        for (sid, shelf) in self.shelves.iter_mut().enumerate() {
            for (index, cart) in shelf.carts.iter_mut().enumerate() {
                if cart.rom.to_string_lossy() != uri { continue; }
                if let (Some(choice), Some(auto)) = (shell, auto) {
                    cart.shell = crate::library::chosen_shell(cart.platform, auto, choice);
                }
                redraw.push((sid, index));
            }
        }
        for (sid, index) in redraw {
            if self.clear_material.is_some_and(|m| m.key == (sid, index)) {
                // The clear plastic's layers follow on the next quiet frame.
                if let Some(material) = self.clear_material.as_mut() { material.key = (usize::MAX, usize::MAX); }
            }
            let shelf = &self.shelves[sid];
            if let Some(tex) = shelf.face(index) {
                let face = cart_face_with_material(&shelf.carts[index], None);
                self.gpu.update_face(tex, &face);
            }
        }
    }

    /// A small texture for Cart Studio, from the pool when there is one.
    pub(super) fn studio_texture(&mut self, face: &impl slot_ui::Face) -> slot_gfx::TexId {
        match self.studio_pool.pop() {
            Some(tex) => { self.gpu.update_face(tex, face); tex }
            None => self.gpu.create_face(face),
        }
    }

    /// Before drawing: the faces of the page and the cart as it stands.
    pub(super) fn prepare_cart_studio(&mut self) {
        if !matches!(self.overlay, ShelfOverlay::CartStudio) { return; }
        let Some(studio) = self.studio.as_ref() else { return };
        let mut texts = vec![("CART STUDIO".to_owned(), true), (slot_ui::label_text(&studio.original), true)];
        for row in studio.rows() {
            let (value, lit) = studio.value(row);
            texts.push((ROWS[row].to_owned(), lit || row != ROW_BACKGROUND));
            texts.push((value, lit));
        }
        for f in FINISHES { texts.push((f.to_owned(), true)); texts.push((f.to_owned(), false)); }
        if studio.chart.is_some() {
            texts.push((studio.original.platform.name().to_uppercase(), false));
            texts.push(("FUN".to_owned(), false));
            texts.push(("COLOUR".to_owned(), false));
            if let Some(c) = studio.colours.get(studio.colour) {
                texts.push((c.name.clone(), true));
                if let Some(shell) = c.shell {
                    texts.push((format!("real cart: {}", FINISHES[finish_index(shell.finish, studio.original.platform)].to_lowercase()), false));
                }
            }
        }
        let cart = studio.draft_cart();
        let (art, note) = studio.label_art(&cart);
        if let Some(note) = note { texts.push((note, false)); }
        if let Some(search) = &studio.search {
            texts.push(("SEARCH GAME".to_owned(), true));
            texts.push((if search.query.is_empty() { "Type a name".to_owned() } else { search.query.clone() },
                !search.query.is_empty()));
            texts.push(("No game found yet.".to_owned(), false));
            for &i in search.hits.iter().skip(search.result.saturating_sub(RESULTS_SHOWN - 1)).take(RESULTS_SHOWN) {
                let n = &search.names[i];
                texts.push((format!("{} · {}", n.name, n.platform), true));
            }
            for key in KEYS { texts.push((key_face(key), true)); }
        }
        let stale = studio.stale;
        let wanted = studio.wanted_minis();
        for (text, lit) in texts { self.controls_face(&text, lit); }
        for (key, label) in [("B", "BACK"), ("Y", "RESET"), ("SWAP", "CHANGE"), ("A", "SAVE"), ("A", "CHOOSE"),
                             ("A", "DONE"), ("SWAP", "MOVE"), ("X", "DELETE"), ("R1", "RESULTS"), ("R1", "KEYS"),
                             ("A", "TYPE"), ("A", "USE")] {
            self.controls_hint(key, label);
        }
        self.prepare_minis(wanted);
        if !stale { return; }
        let face = slot_ui::cart_face_with_material(&cart, art.map(|(rgba, _, _)| rgba));
        let (w, h) = slot_ui::cart_box_for(&cart);
        let studio = self.studio.as_mut().expect("checked above");
        studio.preview = Some(match studio.preview {
            Some((tex, _, _)) => { self.gpu.update_face(tex, &face); (tex, w, h) }
            None => (self.gpu.create_face(&face), w, h),
        });
        studio.stale = false;
    }

    /// The tiles: keep the ones still right, draw the missing ones.
    fn prepare_minis(&mut self, wanted: Vec<Option<ShellChoice>>) {
        let Some(studio) = self.studio.as_mut() else { return };
        let minis = &mut studio.minis;
        let n = wanted.len();
        if minis.tex.len() < n { minis.tex.resize(n, None); }
        let mut freed = Vec::new();
        for i in 0..minis.tex.len() {
            let same = i < n && minis.wanted.get(i) == wanted.get(i);
            if !same {
                if let Some((tex, _, _)) = minis.tex[i].take() { freed.push(tex); }
            }
        }
        minis.tex.truncate(n);
        minis.wanted = wanted;
        self.studio_pool.extend(freed);
        let Some(studio) = self.studio.as_ref() else { return };
        let first = if studio.chart.is_some() { studio.colour } else { studio.finish };
        let order: Vec<usize> = (0..n).map(|k| (first + k) % n.max(1)).collect();
        let todo: Vec<usize> = order.into_iter().filter(|&i| studio.minis.tex[i].is_none()).collect();
        let home = matches!(studio.original.platform, Platform::Nes | Platform::Snes | Platform::Md);
        for i in todo {
            let Some(studio) = self.studio.as_ref() else { return };
            let (colour, finish) = match studio.minis.wanted[i] {
                Some(choice) => (choice.colour, finish_of(choice.finish)),
                None => { let p = studio.auto_plastic(); (p.colour, p.finish) }
            };
            let clear = matches!(finish, Finish::Translucent | Finish::ClearGlitter) || finish == Finish::Glitter && !home;
            let face = swatch_tile(colour, clear, finish.has_flecks());
            let tex = self.studio_texture(&face);
            if let Some(studio) = self.studio.as_mut() { studio.minis.tex[i] = Some((tex, TILE, TILE)); }
        }
    }

    pub(super) fn draw_cart_studio(&self, out: &mut Vec<Draw>) {
        let Some(studio) = self.studio.as_ref() else { return };
        self.draw_menu_ground(0.0, out);
        let width = out_w() as f32;
        let fade = slot_ui::ease((self.menu_opened.elapsed().as_secs_f32() / 0.16).clamp(0.0, 1.0));
        let face = |text: &str, lit: bool| self.controls_faces.get(&Self::face_key(text, lit)).copied();
        self.draw_page_title(out, "CART STUDIO", fade);
        // The cart and the rows as one block, centred, with a clear gap.
        let split = (width * 0.5).round();
        let layout = studio_layout(studio.preview.map(|(_, w, h)| (w, h)));
        let (left, right_edge) = (layout.cart_x, layout.cart_x + layout.cart_room);
        let mut below = 300.0;
        let mut cart_top = ROWS_TOP;
        if let Some((tex, _, _)) = studio.preview {
            let (dw, dh) = (layout.cart_w, layout.cart_h);
            let (x, y) = (left + (layout.cart_room - dw) / 2.0, layout.cart_y);
            cart_top = y;
            out.push(Draw::Tex { x, y, w: dw, h: dh, tex, alpha: fade });
            below = y + dh + 12.0;
        }
        let centre = (left + right_edge) / 2.0;
        if let Some((tex, w, h)) = face(&slot_ui::label_text(&studio.original), true) {
            let scale = 0.66f32.min((right_edge - left) / w.max(1) as f32);
            let dw = w as f32 * scale;
            out.push(Draw::Tex { x: centre - dw / 2.0, y: below, w: dw, h: h as f32 * scale, tex, alpha: 0.85 * fade });
        }
        let cart = studio.draft_cart();
        if let Some(note) = studio.label_art(&cart).1 {
            if let Some((tex, w, h)) = face(&note, false) {
                let scale = 0.56f32.min((right_edge - left) / w.max(1) as f32);
                let dw = w as f32 * scale;
                out.push(Draw::Tex { x: centre - dw / 2.0, y: below + 30.0, w: dw, h: h as f32 * scale, tex, alpha: fade });
            }
        }
        if let Some(search) = &studio.search {
            self.draw_search(search, split, fade, out);
            return;
        }
        let (x0, x1) = (layout.x0, layout.x1);
        let hint_keys: Vec<(&str, &str)> = if studio.chart.is_some() {
            self.draw_chart(studio, x0, x1, fade, out);
            vec![("B", "BACK"), ("SWAP", "MOVE"), ("A", "CHOOSE")]
        } else {
            // The rows, on the right half: Label level with the cart's top,
            // as low as the finish strip under them allows.
            let rows = studio.rows();
            let lowest = STRIP_BOTTOM - FINISH_STRIP_H - rows.len() as f32 * QUICK_PITCH;
            let rows_top = (cart_top - 8.0).clamp(ROWS_TOP, lowest.max(ROWS_TOP));
            self.draw_menu_pill(x0 - 8.0, x1 + 8.0,
                rows_top + studio.row as f32 * QUICK_PITCH + 4.0, fade, out);
            for (slot, &row) in rows.iter().enumerate() {
                let y = rows_top + slot as f32 * QUICK_PITCH + 8.0;
                let (value, lit) = studio.value(row);
                let chosen = slot == studio.row;
                let carets = chosen && lit;
                let mut label_room = x1 - x0;
                if let Some((tex, w, h)) = face(&value, lit) {
                    let scale = 0.72f32.min(((x1 - x0) * 0.6) / w.max(1) as f32);
                    let vw = w as f32 * scale;
                    let x = x1 - vw - if carets { 22.0 } else { 0.0 };
                    label_room = (x - x0 - 12.0 - if carets { 20.0 } else { 0.0 }).max(50.0);
                    out.push(Draw::Tex { x, y, w: vw, h: h as f32 * scale, tex,
                        alpha: if chosen { fade } else { 0.62 * fade } });
                    if carets {
                        let (lt, lw, lh) = self.setting_carets[0];
                        let (rt, rw, rh) = self.setting_carets[1];
                        out.push(Draw::Tex { x: x - lw as f32 - 8.0, y, w: lw as f32, h: lh as f32, tex: lt, alpha: fade });
                        out.push(Draw::Tex { x: x1 - rw as f32, y, w: rw as f32, h: rh as f32, tex: rt, alpha: fade });
                    }
                }
                let label_lit = lit || row != ROW_BACKGROUND;
                if let Some((tex, w, h)) = face(ROWS[row], label_lit) {
                    let scale = 0.72f32.min(label_room / w.max(1) as f32);
                    out.push(Draw::Tex { x: x0, y, w: w as f32 * scale, h: h as f32 * scale, tex,
                        alpha: if label_lit { fade } else { 0.45 * fade } });
                }
            }
            if studio.row_id() == ROW_FINISH {
                let top = rows_top + rows.len() as f32 * QUICK_PITCH + 6.0;
                self.draw_finish_strip(studio, x0, x1, top, fade, out);
            }
            let a = match (studio.row_id(), studio.label) {
                (ROW_COLOUR, _) => ("A", "CHOOSE"),
                (ROW_LABEL, SEARCH_LABEL | PICTURE_LABEL) => ("A", "CHOOSE"),
                _ if studio.from_library => ("A", "DONE"),
                _ => ("A", "SAVE"),
            };
            vec![("B", "BACK"), ("Y", "RESET"), ("SWAP", "CHANGE"), a]
        };
        // The legend, under its own band.
        out.push(Draw::Rect { x: 0.0, y: 420.0, w: width, h: 60.0, colour: menu_ground() });
        let hints_of = |keys: &[(&'static str, &'static str)]| -> Vec<(slot_gfx::TexId, u32)> {
            keys.iter().filter_map(|key| self.controls_hint_faces.get(key).copied()).collect()
        };
        let mut hints = hints_of(&hint_keys);
        // A narrow screen drops the obvious A, then B (Siham 2026-11-09).
        let mut keys = hint_keys.clone();
        for dropped in ["A", "B"] {
            if !legend_too_wide(&hints) { break; }
            keys.retain(|&(k, _)| k != dropped);
            hints = hints_of(&keys);
        }
        for (tex, w, x) in centred_hints(&hints, LEGEND_GAP) {
            out.push(Draw::Tex { x, y: self.legend_y(), w: w as f32, h: HINT_H as f32, tex, alpha: fade });
        }
    }

    /// A page's title, top left like every menu page's (Siham 2026-11-08).
    pub(super) fn draw_page_title(&self, out: &mut Vec<Draw>, title: &str, fade: f32) {
        let title = self.controls_faces.get(&Self::face_key(title, true)).copied();
        self.draw_menu_header(self.studio_page_path(), title, fade, out);
    }

    /// A mini cart centred in its box.
    fn draw_mini(&self, studio: &Studio, i: usize, bx: f32, by: f32, bw: f32, bh: f32, alpha: f32, out: &mut Vec<Draw>) {
        match studio.minis.tex.get(i).copied().flatten() {
            Some((tex, w, h)) => {
                let s = (bw / w as f32).min(bh / h as f32);
                let (dw, dh) = (w as f32 * s, h as f32 * s);
                out.push(Draw::Tex { x: bx + (bw - dw) / 2.0, y: by + (bh - dh) / 2.0, w: dw, h: dh, tex, alpha });
            }
            None => {
                let mut c = edge();
                c[3] *= 0.4 * alpha;
                out.push(Draw::Rect { x: bx + bw * 0.2, y: by + bh * 0.15, w: bw * 0.6, h: bh * 0.7, colour: c });
            }
        }
    }

    /// The colour chart: the system's colours, then the fun ones, the cart
    /// on the left wearing the one under the cursor.
    fn draw_chart(&self, studio: &Studio, x0: f32, x1: f32, fade: f32, out: &mut Vec<Draw>) {
        let face = |text: &str, lit: bool| self.controls_faces.get(&Self::face_key(text, lit)).copied();
        let fun = studio.colours.len() - studio.fun_start;
        let g = chart_grid(x0, x1, studio.fun_start, fun);
        let system_rows = studio.fun_start.div_ceil(g.cols);
        out.push(Draw::Rect { x: x0 - 10.0, y: 70.0, w: x1 - x0 + 20.0, h: STRIP_BOTTOM - 62.0,
            colour: { let mut c = edge(); c[3] *= 0.35 * fade; c } });
        let small = |out: &mut Vec<Draw>, text: &str, lit: bool, x: f32, y: f32, right: bool| {
            if let Some((tex, w, h)) = face(text, lit) {
                let s = 0.44f32.min((x1 - x0) / 2.0 / w.max(1) as f32);
                let dw = w as f32 * s;
                out.push(Draw::Tex { x: if right { x - dw } else { x }, y, w: dw, h: h as f32 * s, tex, alpha: 0.75 * fade });
            }
        };
        small(out, "COLOUR", false, x0, 78.0, false);
        small(out, &studio.original.platform.name().to_uppercase(), false, x1, 78.0, true);
        let fun_title_y = g.top + g.gap + system_rows as f32 * g.cell_h + 4.0;
        small(out, "FUN", false, x0, fun_title_y, false);
        for i in 0..studio.colours.len() {
            let (section, row, col) = studio.chart_place(i, g.cols);
            let (cx, cy) = g.cell(section, row, col, system_rows);
            if i == studio.colour {
                out.push(Draw::Rect { x: cx + 1.0, y: cy, w: g.cell_w - 2.0, h: g.cell_h - 2.0,
                    colour: { let mut c = edge(); c[3] *= fade; c } });
            }
            self.draw_mini(studio, i, cx + 4.0, cy + 3.0, g.cell_w - 8.0, g.cell_h - 8.0, fade, out);
        }
        // The chosen colour's name, and the finish its real cart had.
        let y = g.top + g.gap * 2.0 + (system_rows + fun.div_ceil(g.cols)) as f32 * g.cell_h + 4.0;
        if let Some(c) = studio.colours.get(studio.colour) {
            if let Some((tex, w, h)) = face(&c.name, true) {
                let s = 0.6f32.min((x1 - x0) * 0.55 / w.max(1) as f32);
                out.push(Draw::Tex { x: x0, y, w: w as f32 * s, h: h as f32 * s, tex, alpha: fade });
            }
            if let Some(shell) = c.shell {
                let note = format!("real cart: {}", FINISHES[finish_index(shell.finish, studio.original.platform)].to_lowercase());
                small(out, &note, false, x1, y + 4.0, true);
            }
        }
    }

    /// Under the Finish row: the cart in each finish, the chosen one framed.
    fn draw_finish_strip(&self, studio: &Studio, x0: f32, x1: f32, top: f32, fade: f32, out: &mut Vec<Draw>) {
        let face = |text: &str, lit: bool| self.controls_faces.get(&Self::face_key(text, lit)).copied();
        let n = FINISHES.len();
        let cell = (x1 - x0) / n as f32;
        let name_h = 18.0;
        let box_h = (STRIP_BOTTOM - top - name_h - 6.0).max(20.0);
        for i in 0..n {
            let cx = x0 + i as f32 * cell;
            if i == studio.finish {
                out.push(Draw::Rect { x: cx + 1.0, y: top, w: cell - 2.0, h: STRIP_BOTTOM - top,
                    colour: { let mut c = edge(); c[3] *= 0.8 * fade; c } });
            }
            self.draw_mini(studio, i, cx + 4.0, top + 3.0, cell - 8.0, box_h, fade, out);
            if let Some((tex, w, h)) = face(FINISHES[i], i == studio.finish) {
                let s = 0.4f32.min((cell - 6.0) / w.max(1) as f32);
                let dw = w as f32 * s;
                out.push(Draw::Tex { x: cx + (cell - dw) / 2.0, y: top + box_h + 6.0, w: dw, h: h as f32 * s, tex,
                    alpha: if i == studio.finish { fade } else { 0.6 * fade } });
            }
        }
    }

    /// Search game: the query, the matches and the keyboard on the right.
    fn draw_search(&self, search: &Search, split: f32, fade: f32, out: &mut Vec<Draw>) {
        let width = out_w() as f32;
        let face = |text: &str, lit: bool| self.controls_faces.get(&Self::face_key(text, lit)).copied();
        let (x0, x1) = (split + 4.0, width - 18.0);
        let col = x1 - x0;
        // The query.
        out.push(Draw::Rect { x: x0 - 6.0, y: 78.0, w: col + 12.0, h: 36.0, colour: { let mut c = edge(); c[3] *= 0.6 * fade; c } });
        let query = if search.query.is_empty() { "Type a name".to_owned() } else { search.query.clone() };
        if let Some((tex, w, h)) = face(&query, !search.query.is_empty()) {
            let scale = 0.56f32.min((col - 8.0) / w.max(1) as f32);
            out.push(Draw::Tex { x: x0, y: 84.0, w: w as f32 * scale, h: h as f32 * scale, tex, alpha: fade });
        }
        // The matches.
        let first = search.result.saturating_sub(RESULTS_SHOWN - 1);
        if search.hits.is_empty() {
            if let Some((tex, w, h)) = face("No game found yet.", false) {
                let scale = 0.5f32.min(col / w.max(1) as f32);
                out.push(Draw::Tex { x: x0, y: 128.0, w: w as f32 * scale, h: h as f32 * scale, tex, alpha: fade });
            }
        }
        for (slot, &i) in search.hits.iter().skip(first).take(RESULTS_SHOWN).enumerate() {
            let y = 122.0 + slot as f32 * 34.0;
            if search.on_results && first + slot == search.result {
                out.push(Draw::Rect { x: x0 - 6.0, y: y - 2.0, w: col + 12.0, h: 32.0,
                    colour: { let mut c = edge(); c[3] *= fade; c } });
            }
            let n = &search.names[i];
            if let Some((tex, w, h)) = face(&format!("{} · {}", n.name, n.platform), true) {
                let scale = 0.5f32.min(col / w.max(1) as f32);
                out.push(Draw::Tex { x: x0, y: y + 2.0, w: w as f32 * scale, h: h as f32 * scale, tex,
                    alpha: if search.on_results && first + slot == search.result { fade } else { 0.7 * fade } });
            }
        }
        // The keyboard.
        let (key_w, key_h) = (col / 10.0, 30.0);
        for (k, key) in KEYS.iter().enumerate() {
            let (cx, cy) = (x0 + (k % 10) as f32 * key_w, 270.0 + (k / 10) as f32 * (key_h + 4.0));
            let chosen = !search.on_results && k == search.key;
            out.push(Draw::Rect { x: cx + 1.0, y: cy, w: key_w - 2.0, h: key_h,
                colour: { let mut c = edge(); c[3] *= if chosen { fade } else { 0.35 * fade }; c } });
            if let Some((tex, w, h)) = face(&key_face(*key), true) {
                let scale = 0.42f32.min((key_w - 4.0) / w.max(1) as f32);
                let (dw, dh) = (w as f32 * scale, h as f32 * scale);
                out.push(Draw::Tex { x: cx + (key_w - dw) / 2.0, y: cy + (key_h - dh) / 2.0, w: dw, h: dh, tex,
                    alpha: if chosen { fade } else { 0.7 * fade } });
            }
        }
        out.push(Draw::Rect { x: 0.0, y: 26.0, w: width, h: 40.0, colour: menu_ground() });
        self.draw_page_title(out, "SEARCH GAME", fade);
        out.push(Draw::Rect { x: 0.0, y: 420.0, w: width, h: 60.0, colour: menu_ground() });
        let keys = [("B", "BACK"), ("X", "DELETE"),
            if search.on_results { ("R1", "KEYS") } else { ("R1", "RESULTS") },
            if search.on_results { ("A", "USE") } else { ("A", "TYPE") }];
        let hints: Vec<(slot_gfx::TexId, u32)> = keys.iter()
            .filter_map(|key| self.controls_hint_faces.get(key).copied()).collect();
        for (tex, w, x) in centred_hints(&hints, LEGEND_GAP) {
            out.push(Draw::Tex { x, y: self.legend_y(), w: w as f32, h: HINT_H as f32, tex, alpha: fade });
        }
    }
}

/// The right half's columns: their width is the rows', the chart's and the
/// search's.
fn studio_columns() -> (f32, f32) {
    let width = out_w() as f32;
    ((width * 0.5).round() + 4.0, width - 22.0)
}

/// Gap between the cart and the rows' highlight.
const CART_GAP: f32 = 40.0;
/// The widest the cart is drawn, so a wide cart never crowds the rows.
const CART_MAX_W: f32 = 264.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct StudioLayout {
    /// Left of the cart's room; the cart is centred in it.
    cart_x: f32,
    cart_room: f32,
    cart_y: f32,
    cart_w: f32,
    cart_h: f32,
    x0: f32,
    x1: f32,
}

/// The editor's cart and rows as one block centred on the screen. The rows
/// keep the width of the right half, so the finish squares and the colour
/// chart keep their size, and the cart sits CART_GAP to their left.
fn studio_layout(cart: Option<(u32, u32)>) -> StudioLayout {
    studio_layout_in(out_w() as f32, cart)
}

fn studio_layout_in(width: f32, cart: Option<(u32, u32)>) -> StudioLayout {
    let rows_w = (width - 22.0) - ((width * 0.5).round() + 4.0);
    let box_w = rows_w + 16.0;
    let room = CART_MAX_W.min(width - 32.0 - CART_GAP - box_w).max(80.0);
    let (cart_w, cart_h) = match cart {
        Some((cw, ch)) => {
            let scale = (room / cw.max(1) as f32).min(250.0 / ch.max(1) as f32).min(1.7);
            (cw as f32 * scale, ch as f32 * scale)
        }
        None => (room, 250.0),
    };
    let cart_left = ((width - (cart_w + CART_GAP + box_w)) / 2.0).max(16.0).round();
    let x0 = cart_left + cart_w + CART_GAP + 8.0;
    StudioLayout {
        cart_x: cart_left + (cart_w - room) / 2.0,
        cart_room: room,
        cart_y: 84.0 + (250.0 - cart_h) / 2.0,
        cart_w,
        cart_h,
        x0,
        x1: x0 + rows_w,
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn editor_keeps_a_gap_and_centres_cart_and_rows() {
        for width in [640.0, 720.0, 854.0, 1066.0] {
            for cart in [(1080, 636), (936, 1011), (1296, 560)] {
                let l = studio_layout_in(width, Some(cart));
                let cart_left = l.cart_x + (l.cart_room - l.cart_w) / 2.0;
                let cart_right = cart_left + l.cart_w;
                assert!(l.cart_w <= CART_MAX_W + 0.01);
                assert!((l.x0 - 8.0) - cart_right >= CART_GAP - 0.01, "gap at {width} {cart:?}");
                let (left_margin, right_margin) = (cart_left, width - (l.x1 + 8.0));
                assert!(left_margin >= 16.0 && right_margin >= 16.0 - 0.6, "{width} {cart:?} {l:?}");
                assert!((left_margin - right_margin).abs() <= 1.0 || left_margin == 16.0, "{width} {cart:?} {l:?}");
                // The rows keep the right half's width, so the squares don't shrink.
                let (c0, c1) = ((width * 0.5).round() + 4.0, width - 22.0);
                assert!(((l.x1 - l.x0) - (c1 - c0)).abs() < 0.01);
            }
        }
    }
}
