//! SAF ROM library metadata. SAF content URIs are *not* POSIX file paths.
//! This module only converts metadata and never attempts filesystem I/O on URIs.
use serde::Deserialize;
use slot_store::{sort_key, Cart, Outline, Platform, ShellChoice, ShellFinish};
use slot_store::gb::{Class, Header};
use std::path::PathBuf;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// RomLibrary.MAX_ROMS: the most carts a shelf holds (issue #122).
const MAX_ROMS: usize = 10_000;

#[derive(Clone, Debug, Deserialize)]
pub struct RomEntry {
    pub title: String,
    /// The file's name with its extension, as the scan found it.
    #[serde(default)]
    pub filename: String,
    pub platform: String,
    pub uri: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub color_only: bool,
    #[serde(default)]
    pub shell_override: Option<String>,
    // Nintendo GB/GBC header fields are read once by the Android SAF scanner,
    // including from ZIPs, instead of trying to open a content URI as a file.
    #[serde(default)]
    pub gb_title: String,
    #[serde(default)]
    pub gb_code: String,
    #[serde(default)]
    pub gb_japan: bool,
    #[serde(default)]
    pub gb_cgb_flag: Option<i32>,
    // NES / SNES / Mega Drive header facts from the same scan: the
    // internal title, region (J, U, E or empty) and battery-backed save.
    #[serde(default)]
    pub home_title: String,
    #[serde(default)]
    pub home_region: String,
    #[serde(default)]
    pub home_battery: bool,
    /// The Mega Drive header's copyright field: "(C)T-50" is Electronic Arts.
    #[serde(default)]
    pub home_maker: String,
    /// The ROM's own extension inside a ZIP (the scan's rom_ext).
    #[serde(default)]
    pub rom_ext: String,
    /// The shelf name when it isn't the file's: a Neo Geo set's real title
    /// ("Metal Slug" for mslug.zip), its file keeping the set's name.
    #[serde(default)]
    pub display_title: String,
    /// A Neo Geo game found both as a set (.zip, this cart) and as a .neo:
    /// the .neo's address, which Geolith plays (pair_neo_geo_twins).
    #[serde(default)]
    pub twin_uri: String,
    /// A Neo Geo file's set, read by the scan from its name: "stakwin2" for
    /// "Stakes Winner 2 (stakwin2).neo" too (NeoGeoSets.setOf).
    #[serde(default)]
    pub neo_set: String,
}

/// A Neo Geo game both as a .zip set and as a .neo is one cart, its .zip's
/// (FinalBurn Neo by default, Geolith on its .neo by SELECT: Siham
/// 2026-11-28). The .neo leaves the list and its address goes with the
/// .zip's entry. Games are paired by their set (the scan's neo_set, else the
/// file name), wherever their folders.
pub fn pair_neo_geo_twins(entries: &mut Vec<RomEntry>) {
    use std::collections::HashMap;
    let ext_of = |entry: &RomEntry| {
        let name = if entry.filename.trim().is_empty() { file_name_of(&entry.uri) } else { entry.filename.clone() };
        name.rsplit_once('.').map(|(stem, ext)| {
            let set = entry.neo_set.trim();
            let key = if set.is_empty() { stem.trim().to_lowercase() } else { set.to_lowercase() };
            (key, ext.to_ascii_lowercase())
        })
    };
    let mut zips: HashMap<String, usize> = HashMap::new();
    for (i, entry) in entries.iter().enumerate() {
        if Platform::parse(&entry.platform) != Some(Platform::NeoGeo) { continue; }
        if let Some((stem, ext)) = ext_of(entry) {
            if ext == "zip" { zips.entry(stem).or_insert(i); }
        }
    }
    if zips.is_empty() { return; }
    let mut twins: Vec<(usize, String)> = Vec::new();
    let mut paired = std::collections::HashSet::new();
    let mut drop = vec![false; entries.len()];
    for (i, entry) in entries.iter().enumerate() {
        if Platform::parse(&entry.platform) != Some(Platform::NeoGeo) { continue; }
        let Some((stem, ext)) = ext_of(entry) else { continue };
        if ext != "neo" { continue; }
        let Some(&zip) = zips.get(&stem) else { continue };
        if !paired.insert(zip) { continue; }
        twins.push((zip, entry.uri.clone()));
        drop[i] = true;
    }
    for (zip, uri) in twins { entries[zip].twin_uri = uri; }
    let mut i = 0;
    entries.retain(|_| { let keep = !drop[i]; i += 1; keep });
}

/// The .neo Geolith plays for this Neo Geo cart, when it has one.
pub fn twin_of(uri: &str) -> Option<String> {
    let library = LIBRARY.lock().unwrap_or_else(|e| e.into_inner());
    library.entries.as_ref()?.iter()
        .find(|entry| entry.uri == uri && !entry.twin_uri.is_empty())
        .map(|entry| entry.twin_uri.clone())
}

pub struct LibraryState {
    pub version: u64,
    /// None: no folder selected yet, show empty shelf and onboarding.
    /// Some([]): scanned folder has no supported games; show empty shelf.
    pub entries: Option<Vec<RomEntry>>,
    /// Each cart's portable name (see `game_key`), by its Android address.
    pub keys: BTreeMap<String, String>,
}

pub static LIBRARY: Mutex<LibraryState> = Mutex::new(LibraryState {
    version: 0,
    entries: None,
    keys: BTreeMap::new(),
});

/// A game's name that stays the same on another handheld: its system and
/// its file name, in lower case (`gba/pokemon emerald (usa).gba`), where
/// the Android address of the file changes with each device and card
/// (Siham 2026-11-16, slot. 2.5 portability). Two games of a system with
/// the same file name in different folders take their path in the ROM
/// folder instead. A cart outside the library keeps its address.
pub fn game_key(uri: &str) -> String {
    let library = LIBRARY.lock().unwrap_or_else(|e| e.into_inner());
    library.keys.get(uri).cloned().unwrap_or_else(|| uri.to_owned())
}

/// Every cart's portable name (see `game_key`).
pub fn game_keys(entries: &[RomEntry]) -> BTreeMap<String, String> {
    let named: Vec<(String, String)> = entries.iter().map(|entry| {
        let name = if entry.filename.trim().is_empty() { file_name_of(&entry.uri) }
            else { entry.filename.trim().to_owned() };
        (entry.uri.clone(), format!("{}/{}", entry.platform.to_lowercase(), name.to_lowercase()))
    }).collect();
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (_, key) in &named { *seen.entry(key.as_str()).or_default() += 1; }
    let mut keys = BTreeMap::new();
    for ((uri, key), entry) in named.iter().zip(entries) {
        let key = if seen[key.as_str()] > 1 {
            match path_in_tree(uri) {
                Some(path) => format!("{}/{}", entry.platform.to_lowercase(), path.to_lowercase()),
                None => uri.clone(),
            }
        } else { key.clone() };
        keys.insert(uri.clone(), key);
    }
    keys
}

/// The file's path inside the folder chosen in Android's picker, from a
/// tree document address (`.../tree/primary%3ARoms/document/primary%3ARoms%2FGBA%2Fx.gba`
/// gives `GBA/x.gba`).
pub fn path_in_tree(uri: &str) -> Option<String> {
    let rest = uri.split_once("/tree/")?.1;
    let (tree, document) = rest.split_once("/document/")?;
    let (tree, document) = (percent_decode(tree)?, percent_decode(document)?);
    let path = document.strip_prefix(&tree)?.strip_prefix('/')?;
    (!path.is_empty()).then(|| path.to_owned())
}

/// The file's name from its address, when the scan didn't give one.
fn file_name_of(uri: &str) -> String {
    let last = uri.rsplit('/').next().unwrap_or(uri);
    let decoded = percent_decode(last).unwrap_or_else(|| last.to_owned());
    decoded.rsplit(['/', ':']).next().unwrap_or(&decoded).to_owned()
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Select the next shelf with games in the given direction, skipping empty
/// GB/GBC/GBA systems. Returns the current shelf if no alternative exists.
pub fn next_populated(current: usize, direction: i32, count: &[usize]) -> usize {
    let n = count.len();
    if n == 0 || current >= n { return current; }
    for step in 1..=n {
        let i = (current as i32 + direction * step as i32).rem_euclid(n as i32) as usize;
        if count[i] != 0 { return i; }
    }
    current
}

/// A WonderSwan Color game (.wsc, zipped or not) comes in the Color's clear
/// cart; the WonderSwan shelf's own mould is the black one.
fn colour_swan(shell: ShellChoice, entry: &RomEntry) -> ShellChoice {
    let colour = entry.filename.to_ascii_lowercase().ends_with(".wsc")
        || entry.rom_ext.eq_ignore_ascii_case("wsc");
    if !colour || shell.outline != slot_store::Outline::WonderSwan { return shell; }
    ShellChoice { outline: slot_store::Outline::WonderSwanColor, colour: [145, 148, 148],
        finish: slot_store::ShellFinish::Clear }
}

/// A cart's automatic shell, as the shelf gives it when Cart Studio names none.
fn auto_shell(entry: &RomEntry, platform: Platform) -> Option<ShellChoice> {
    match platform {
        Platform::Gba => None,
        // The region's mould, a publisher's, and the game's colour.
        Platform::Nes | Platform::Snes | Platform::Md | Platform::Sms | Platform::Gg
        | Platform::Ngp | Platform::Ws | Platform::Lynx | Platform::Pokemini | Platform::Pico8
        | Platform::NeoGeo | Platform::Pce => slot_ui::home_shell(
            platform, entry.title.trim(), &entry.home_region, &entry.home_maker)
            .map(|shell| colour_swan(shell, entry)),
        Platform::Gb | Platform::Gbc => {
            // Cart Studio and the shelf must resolve the same automatic
            // per-game shell, even for SAF content URIs and zipped ROMs.
            // Explicit saved Studio overrides still take precedence below.
            let detected = entry.gb_cgb_flag
                .and_then(|flag| u8::try_from(flag).ok())
                .map(|cgb| {
                    let header = Header {
                        title: entry.gb_title.clone(),
                        code: entry.gb_code.clone(),
                        japan: entry.gb_japan,
                        cgb,
                    };
                    let material = slot_ui::gb_shell_from_header(&header);
                    ShellChoice {
                        outline: if header.class() == Class::ColourOnly {
                            Outline::Rounded
                        } else {
                            Outline::Notched
                        },
                        colour: material.colour,
                        finish: material.finish.stored(),
                    }
                });
            Some(detected.unwrap_or_else(|| {
                // Legacy indexes continue to render until a background
                // scan supplies GB header identity, with no ROM writes.
                let color_only = entry.color_only;
                ShellChoice {
                    outline: if color_only { Outline::Rounded } else { Outline::Notched },
                    colour: match (platform, color_only) {
                        (_, true) => [0x7c, 0x7a, 0x8a],
                        (Platform::Gbc, false) => [0x33, 0x30, 0x31],
                        _ => [0x9a, 0x97, 0x8f],
                    },
                    finish: if color_only { ShellFinish::Clear } else { ShellFinish::Solid },
                }
            }))
        }
    }
}

/// The automatic shell of a cart known only by its name (a RomM game, not
/// downloaded yet): the home systems' region mould and colour; the Game
/// Boys' from their header once slot. read it from the server, their plain
/// grey or dark plastic until then.
pub fn remote_shell(platform: Platform, title: &str, gb: Option<RemoteGbHeader>) -> Option<ShellChoice> {
    let gb = gb.unwrap_or_default();
    let entry = RomEntry {
        title: title.to_owned(), platform: String::new(), uri: String::new(), code: String::new(),
        color_only: gb.cgb_flag == Some(0xc0), shell_override: None, gb_title: gb.title,
        gb_code: gb.code, gb_japan: gb.japan, gb_cgb_flag: gb.cgb_flag, home_title: String::new(),
        home_region: String::new(), home_battery: false, home_maker: String::new(), rom_ext: String::new(), display_title: String::new(), twin_uri: String::new(), neo_set: String::new(), filename: String::new(),
    };
    auto_shell(&entry, platform)
}

/// A Game Boy cart's header fields, as RomLibrary reads them (0x134 title,
/// 0x13F code, 0x14A destination, 0x143 CGB flag).
#[derive(Clone, Debug, Default)]
pub struct RemoteGbHeader {
    pub title: String,
    pub code: String,
    pub japan: bool,
    pub cgb_flag: Option<i32>,
}

/// The shell a cart wears with a Cart Studio choice over its automatic one.
pub fn chosen_shell(platform: Platform, auto: Option<ShellChoice>, chosen: Option<ShellChoice>) -> Option<ShellChoice> {
    chosen
        .map(|mut chosen| {
            // A Studio colour on a home cart keeps the automatic mould
            // unless it names one of that system's own.
            if let Some(auto) = auto.filter(|_| {
                !platform.handheld()
                    && chosen.outline.platform() != Some(platform)
            }) {
                chosen.outline = auto.outline;
            }
            chosen
        })
        .or(auto)
}

/// The automatic shell of the cart at `uri` in the current library.
pub fn auto_shell_of(uri: &str) -> Option<ShellChoice> {
    let state = LIBRARY.lock().unwrap_or_else(|e| e.into_inner());
    let entry = state.entries.as_ref()?.iter().find(|e| e.uri == uri)?;
    auto_shell(entry, Platform::parse(&entry.platform)?)
}

/// Shells Cart Studio saved since slot. started, by URI: a library the
/// scanner prepared before the save (published later) keeps them.
static SAVED_SHELLS: std::sync::Mutex<Option<std::collections::HashMap<String, Option<String>>>> =
    std::sync::Mutex::new(None);

/// Over a library coming from Android: the shells saved since it was
/// prepared (bug: a Cart Studio colour went back to the old one).
pub fn keep_saved_shells(entries: &mut [RomEntry]) {
    let saved = SAVED_SHELLS.lock().unwrap_or_else(|e| e.into_inner());
    let Some(saved) = saved.as_ref() else { return };
    for entry in entries.iter_mut() {
        if let Some(shell) = saved.get(&entry.uri) { entry.shell_override = shell.clone(); }
    }
}

/// Cart Studio saved a shell: the library keeps it for the next rebuild.
pub fn remember_shell(uri: &str, chosen: Option<ShellChoice>) {
    SAVED_SHELLS.lock().unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(Default::default)
        .insert(uri.to_owned(), chosen.map(|c| c.to_value()));
    let mut state = LIBRARY.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = state.entries.as_mut().and_then(|e| e.iter_mut().find(|e| e.uri == uri)) {
        entry.shell_override = chosen.map(|c| c.to_value());
    }
}

/// The profile on disk is authoritative, including after a slot-folder pull.
pub fn load_shell_profiles(root: &std::path::Path, entries: &[RomEntry]) -> Vec<RomEntry> {
    let dir = root.join("Studio");
    let keys = game_keys(entries);
    let present: std::collections::HashSet<_> = std::fs::read_dir(&dir)
        .map(|files| files.flatten().map(|f| f.file_name()).collect()).unwrap_or_default();
    let mut result = entries.to_vec();
    for entry in &mut result {
        let name = keys.get(&entry.uri).map(String::as_str).unwrap_or(&entry.uri);
        let portable = format!("{}.shell", crate::core_selection::key_of_name(name));
        let legacy = format!("{}.shell", crate::core_selection::legacy_key(&entry.uri));
        if !present.contains(std::ffi::OsStr::new(&portable)) &&
            present.contains(std::ffi::OsStr::new(&legacy)) {
            crate::core_selection::carry(&dir.join(&legacy), &dir.join(&portable));
        }
        entry.shell_override = if present.contains(std::ffi::OsStr::new(&portable)) ||
            present.contains(std::ffi::OsStr::new(&legacy)) {
            std::fs::read_to_string(dir.join(portable)).ok()
                .filter(|s| s.len() <= 100 && ShellChoice::parse(s.trim()).is_some())
                .map(|s| s.trim().to_owned())
        } else { None };
    }
    result
}

pub fn carts_by_platform(entries: &[RomEntry]) -> [Vec<Cart>; Platform::ALL.len()] {
    let mut groups: [Vec<Cart>; Platform::ALL.len()] = std::array::from_fn(|_| Vec::new());
    for entry in entries.iter().take(MAX_ROMS) {
        if entry.title.trim().is_empty() || !entry.uri.starts_with("content://") {
            continue;
        }
        let Some(platform) = Platform::parse(&entry.platform) else { continue };
        let group = platform as usize;
        let shell = chosen_shell(platform, auto_shell(entry, platform),
            entry.shell_override.as_deref().and_then(ShellChoice::parse));
        let title = if entry.display_title.trim().is_empty() { entry.title.trim() }
            else { entry.display_title.trim() };
        groups[group].push(Cart {
            platform,
            stem: entry.title.trim().to_owned(),
            title: title.to_owned(),
            code: entry.code.clone(),
            // Kept as opaque URI for the planned Android ContentResolver bridge.
            // Never pass this through std::fs::File or libc open().
            rom: PathBuf::from(&entry.uri),
            label: None,
            shell,
            battery: entry.home_battery,
        });
    }
    for shelf in &mut groups {
        // Unicode casefold keys are allocated once per cart instead of on
        // every comparator call (thousands of extra allocations at startup).
        shelf.sort_by_cached_key(|cart| sort_key(&cart.title));
    }
    groups
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeSetLibrary(
    mut env: jni::JNIEnv<'_>,
    _this: jni::objects::JObject<'_>,
    json: jni::objects::JString<'_>,
) -> jni::sys::jint {
    let started = std::time::Instant::now();
    let Ok(input) = env.get_string(&json) else { return -1 };
    let Ok(mut entries) = serde_json::from_str::<Vec<RomEntry>>(&input.to_string_lossy()) else { return -1 };
    if entries.len() > MAX_ROMS { return -1; }
    keep_saved_shells(&mut entries);
    pair_neo_geo_twins(&mut entries);
    let count = entries.len() as i32;
    let keys = game_keys(&entries);
    let mut state = LIBRARY.lock().unwrap_or_else(|err| err.into_inner());
    state.keys = keys;
    state.entries = Some(entries);
    state.version = state.version.wrapping_add(1);
    crate::game::log_launch_timing(&format!(
        "ROM shelf JSON import: {count} carts in {}ms", started.elapsed().as_millis()));
    count
}

/// Something the shelves read besides the ROMs changed (PICO-8 just
/// staged from the BIOS folder: Splore's cart, 3.2.0): the same library,
/// read again at the next safe frame, its carts kept in place.
pub fn touch_library() -> bool {
    let mut state = LIBRARY.lock().unwrap_or_else(|err| err.into_inner());
    if state.entries.is_none() { return false; }
    state.version = state.version.wrapping_add(1);
    true
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_slot_konkr_MainActivity_nativeTouchLibrary(
    _env: jni::JNIEnv<'_>,
    _this: jni::objects::JObject<'_>,
) -> jni::sys::jboolean {
    jni::sys::jboolean::from(touch_library())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(platform: &str, filename: &str, uri: &str) -> RomEntry {
        RomEntry {
            platform: platform.into(), title: filename.into(), uri: uri.into(),
            code: String::new(), color_only: false, shell_override: None,
            gb_title: String::new(), gb_code: String::new(), gb_japan: false, gb_cgb_flag: None,
            home_title: String::new(), home_region: String::new(), home_battery: false,
            home_maker: String::new(), rom_ext: String::new(), display_title: String::new(), twin_uri: String::new(), neo_set: String::new(), filename: filename.into(),
        }
    }

    #[test]
    fn a_neo_geo_game_as_zip_and_neo_is_one_cart_its_zips() {
        let mut entries = vec![
            named("NEOGEO", "mslug.neo", "content://t/document/neogeo%2Fmslug.neo"),
            named("NEOGEO", "mslug.zip", "content://t/document/fbneo%2Fmslug.zip"),
            named("NEOGEO", "kof98.neo", "content://t/document/neogeo%2Fkof98.neo"),
            named("NEOGEO", "garou.zip", "content://t/document/arcade%2Fgarou.zip"),
            named("GBA", "mslug.neo", "content://t/document/gba%2Fmslug.neo"),
        ];
        entries[1].display_title = "Metal Slug - Super Vehicle-001".into();
        pair_neo_geo_twins(&mut entries);
        let uris: Vec<&str> = entries.iter().map(|e| e.uri.as_str()).collect();
        assert_eq!(uris, ["content://t/document/fbneo%2Fmslug.zip", "content://t/document/neogeo%2Fkof98.neo",
            "content://t/document/arcade%2Fgarou.zip", "content://t/document/gba%2Fmslug.neo"]);
        assert_eq!(entries[0].twin_uri, "content://t/document/neogeo%2Fmslug.neo");
        assert!(entries[1..].iter().all(|e| e.twin_uri.is_empty()));
        // Its shelf name is the set's real title; the file keeps its own.
        let carts = carts_by_platform(&entries);
        let neo_geo = &carts[Platform::NeoGeo as usize];
        let mslug = neo_geo.iter().find(|c| c.stem == "mslug.zip").unwrap();
        assert_eq!(slot_ui::label_text(mslug), "Metal Slug Super Vehicle-001");
    }

    #[test]
    fn a_neo_named_after_its_title_pairs_by_its_set() {
        // Siham's .neo files: "Spin Master (spinmast).neo" beside spinmast.zip.
        let mut entries = vec![
            named("NEOGEO", "spinmast.zip", "content://t/document/arcade%2Fspinmast.zip"),
            named("NEOGEO", "Spin Master (spinmast).neo", "content://t/document/neogeo%2Fspin.neo"),
            named("NEOGEO", "Stakes Winner 2 (stakwin2).neo", "content://t/document/neogeo%2Fstak.neo"),
        ];
        entries[0].neo_set = "spinmast".into();
        entries[1].neo_set = "spinmast".into();
        entries[2].neo_set = "stakwin2".into();
        pair_neo_geo_twins(&mut entries);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].twin_uri, "content://t/document/neogeo%2Fspin.neo");
        assert!(entries[1].twin_uri.is_empty());
    }

    #[test]
    fn a_zipped_wonderswan_color_game_takes_the_clear_cart() {
        let black = ShellChoice { outline: slot_store::Outline::WonderSwan, colour: [30, 30, 30],
            finish: slot_store::ShellFinish::Solid };
        let mut zipped = named("WS", "Final Fantasy (Japan).zip", "content://x");
        assert_eq!(colour_swan(black, &zipped).outline, slot_store::Outline::WonderSwan);
        zipped.rom_ext = "wsc".into();
        assert_eq!(colour_swan(black, &zipped).outline, slot_store::Outline::WonderSwanColor);
        let plain = named("WS", "Final Fantasy (Japan).wsc", "content://y");
        assert_eq!(colour_swan(black, &plain).finish, slot_store::ShellFinish::Clear);
    }

    #[test]
    fn shell_profiles_are_loaded_from_the_incoming_keys_and_not_stale_json() {
        let root = std::env::temp_dir().join(format!("slot-shell-profile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Studio")).unwrap();
        let mut entry = named("GBA", "Emerald.gba", "content://card/Emerald.gba");
        entry.shell_override = Some("auto ff0000 solid".into());
        let name = game_keys(&[entry.clone()])[&entry.uri].clone();
        let file = root.join("Studio").join(format!("{}.shell", crate::core_selection::key_of_name(&name)));
        std::fs::write(&file, "auto 123abc clear").unwrap();
        assert_eq!(load_shell_profiles(&root, &[entry.clone()])[0].shell_override.as_deref(),
            Some("auto 123abc clear"));
        std::fs::remove_file(&file).unwrap();
        assert!(load_shell_profiles(&root, &[entry])[0].shell_override.is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_game_keeps_its_name_on_another_handheld() {
        // The same game on a KPA's card and on a Retroid's internal storage.
        let kpa = "content://com.android.externalstorage.documents/tree/1234-ABCD%3ARoms/document/1234-ABCD%3ARoms%2FGBA%2FPokemon%20Emerald%20(USA).gba";
        let rp = "content://com.android.externalstorage.documents/tree/primary%3AGames/document/primary%3AGames%2FGame%20Boy%20Advance%2FPokemon%20Emerald%20(USA).gba";
        let a = game_keys(&[named("GBA", "Pokemon Emerald (USA).gba", kpa)]);
        let b = game_keys(&[named("GBA", "Pokemon Emerald (USA).gba", rp)]);
        assert_eq!(a[kpa], "gba/pokemon emerald (usa).gba");
        assert_eq!(a[kpa], b[rp]);
    }

    #[test]
    fn two_games_with_one_name_keep_their_folders() {
        let one = "content://com.android.externalstorage.documents/tree/primary%3ARoms/document/primary%3ARoms%2FGBA%2FTetris.gba";
        let two = "content://com.android.externalstorage.documents/tree/primary%3ARoms/document/primary%3ARoms%2FGBA%2FHacks%2FTetris.gba";
        let keys = game_keys(&[named("GBA", "Tetris.gba", one), named("GBA", "Tetris.gba", two)]);
        assert_eq!(keys[one], "gba/gba/tetris.gba");
        assert_eq!(keys[two], "gba/gba/hacks/tetris.gba");
        // Another system's Tetris is a different game already.
        let gb = game_keys(&[named("GBA", "Tetris.gba", one), named("GB", "Tetris.gb", "content://x/Tetris.gb")]);
        assert_eq!(gb[one], "gba/tetris.gba");
    }

    #[test]
    fn an_old_library_without_file_names_reads_them_from_the_address() {
        let uri = "content://com.android.externalstorage.documents/tree/primary%3ARoms/document/primary%3ARoms%2FSNES%2FChrono%20Trigger.sfc";
        let mut entry = named("SNES", "", uri);
        entry.title = "Chrono Trigger".into();
        assert_eq!(game_keys(&[entry])[uri], "snes/chrono trigger.sfc");
    }
    #[test]
    fn shelf_navigation_skips_empty_platforms() {
        assert_eq!(next_populated(0, 1, &[]), 0);
        assert_eq!(next_populated(0, 1, &[2, 0, 0]), 0);
        assert_eq!(next_populated(0, -1, &[2, 0, 0]), 0);
        assert_eq!(next_populated(0, 1, &[2, 0, 3]), 2);
        assert_eq!(next_populated(0, -1, &[2, 0, 3]), 2);
        assert_eq!(next_populated(2, 1, &[2, 0, 3]), 0);
        assert_eq!(next_populated(0, 1, &[0, 0, 0]), 0);
    }

    #[test]
    fn a_shell_saved_in_cart_studio_outlives_an_older_library() {
        SAVED_SHELLS.lock().unwrap().get_or_insert_with(Default::default)
            .insert("content://roms/studio-test".into(), Some("chosen".into()));
        let entry = |uri: &str| RomEntry {
            platform: "GBA".into(), title: "Game".into(), uri: uri.into(),
            code: String::new(), color_only: false, shell_override: Some("old".into()),
            gb_title: String::new(), gb_code: String::new(),
            gb_japan: false, gb_cgb_flag: None,
            home_title: String::new(), home_region: String::new(), home_battery: false, home_maker: String::new(), rom_ext: String::new(), display_title: String::new(), twin_uri: String::new(), neo_set: String::new(), filename: String::new(),
        };
        let mut entries = vec![entry("content://roms/studio-test"), entry("content://roms/other")];
        keep_saved_shells(&mut entries);
        assert_eq!(entries[0].shell_override.as_deref(), Some("chosen"));
        assert_eq!(entries[1].shell_override.as_deref(), Some("old"));
    }

    #[test]
    fn sorts_and_rejects_non_saf_entries() {
        let example = |platform: &str, title: &str, uri: &str| RomEntry {
            platform: platform.into(), title: title.into(), uri: uri.into(),
            code: String::new(), color_only: false, shell_override: None,
            gb_title: String::new(), gb_code: String::new(),
            gb_japan: false, gb_cgb_flag: None,
            home_title: String::new(), home_region: String::new(), home_battery: false, home_maker: String::new(), rom_ext: String::new(), display_title: String::new(), twin_uri: String::new(), neo_set: String::new(), filename: String::new(),
        };
        let grouped = carts_by_platform(&[
            example("GBA", "Zelda", "content://roms/z"),
            example("GBA", "Advance Wars", "content://roms/a"),
            example("GBA", "Not a ROM", "/storage/emulated/0/ROMs/foo.gba"),
            example("GBC", "Color Game", "content://roms/c"),
            example("SNES", "Ignored", "content://roms/s"),
        ]);
        assert_eq!(grouped[0].len(), 2);
        assert_eq!(grouped[0][0].stem, "Advance Wars");
        assert_eq!(grouped[1].len(), 0);
        assert_eq!(grouped[2].len(), 1);
    }
    #[test]
    fn all_six_platforms_are_available_to_recents_and_favorites() {
        let entry = |platform: &str, slug: &str| RomEntry {
            platform: platform.into(), title: slug.into(),
            uri: format!("content://roms/{slug}"), code: String::new(),
            color_only: false, shell_override: None,
            gb_title: String::new(), gb_code: String::new(),
            gb_japan: false, gb_cgb_flag: None,
            home_title: String::new(), home_region: String::new(), home_battery: false, home_maker: String::new(), rom_ext: String::new(), display_title: String::new(), twin_uri: String::new(), neo_set: String::new(), filename: String::new(),
        };
        let grouped = carts_by_platform(&[
            entry("GBA", "gba"), entry("GB", "gb"), entry("GBC", "gbc"),
            entry("NES", "nes"), entry("SNES", "snes"), entry("MD", "md"),
        ]);
        assert_eq!(grouped.len(), slot_store::Platform::ALL.len());
        assert!(grouped[..6].iter().all(|shelf| shelf.len() == 1));
        let sources: Vec<&[Cart]> = grouped.iter().map(|shelf| shelf.as_slice()).collect();
        let recent = crate::recents::carts(&[
            "content://roms/nes".into(), "content://roms/snes".into(),
            "content://roms/md".into(),
        ], &sources);
        assert_eq!(recent.iter().map(|c| c.stem.as_str()).collect::<Vec<_>>(),
            ["nes", "snes", "md"]);
        let favorites = crate::favorites::carts(&[
            "content://roms/nes".into(), "content://roms/snes".into(),
            "content://roms/md".into(),
        ], &sources);
        assert_eq!(favorites.len(), 3);
        assert!(favorites.iter().any(|c| c.platform == Platform::Nes));
        assert!(favorites.iter().any(|c| c.platform == Platform::Snes));
        assert!(favorites.iter().any(|c| c.platform == Platform::Md));
        assert!(grouped[..6].iter().all(|shelf| shelf.len() == 1),
            "special-rack materialization must not overwrite a system shelf");
    }

    #[test]
    fn gb_carts_use_studio_header_colours_and_preserve_overrides() {
        let entry = |platform: &str, name: &str, title: &str, code: &str, flag: i32| {
            RomEntry {
                platform: platform.into(), title: name.into(),
                uri: format!("content://roms/{name}"), code: String::new(),
                color_only: flag == 0xc0, shell_override: None,
                gb_title: title.into(), gb_code: code.into(),
                gb_japan: false, gb_cgb_flag: Some(flag),
                home_title: String::new(), home_region: String::new(), home_battery: false, home_maker: String::new(), rom_ext: String::new(), display_title: String::new(), twin_uri: String::new(), neo_set: String::new(), filename: String::new(),
            }
        };
        let mut manual = entry("GB", "Manual", "POKEMON RED", "", 0);
        manual.shell_override = Some("rounded 123abc clear".into());
        let grouped = carts_by_platform(&[
            entry("GB", "Blue", "POKEMON BLU", "", 0),
            entry("GB", "Red", "POKEMON RED", "", 0),
            entry("GBC", "Crystal", "", "BYT", 0xc0),
            manual,
        ]);
        let pick = |group: usize, title: &str| grouped[group].iter()
            .find(|cart| cart.title == title).unwrap().shell.unwrap();
        assert_eq!(pick(1, "Blue").colour, [0x2b, 0x3a, 0x88]);
        assert_eq!(pick(1, "Red").colour, [0xc0, 0x28, 0x2c]);
        assert_eq!(pick(2, "Crystal").finish, ShellFinish::Glitter);
        assert_eq!(pick(2, "Crystal").outline, Outline::Rounded);
        assert_eq!(pick(1, "Manual").colour, [0x12, 0x3a, 0xbc]);
        assert_eq!(pick(1, "Manual").finish, ShellFinish::Clear);
        assert_eq!(pick(1, "Manual").outline, Outline::Rounded);
    }

}
