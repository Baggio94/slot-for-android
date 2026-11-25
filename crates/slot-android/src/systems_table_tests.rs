//! The systems table is written twice, in slot_store::SYSTEMS and in the
//! Android app's Systems.TABLE (and the label harvester keeps ScreenScraper's
//! ids): these tests keep the three identical, and check that every system
//! is complete on every screen slot. runs on.

use slot_store::{Core, Platform, SYSTEMS};

const SYSTEMS_KT: &str = include_str!("../../../android/app/src/main/java/fyi/slot/konkr/Systems.kt");
const HARVEST_PY: &str = include_str!("../../../tools/home-art/harvest.py");

/// The arguments of a Kotlin call, split at its top-level commas.
fn args(call: &str) -> Vec<String> {
    let (mut out, mut depth, mut cur, mut quoted) = (Vec::new(), 0, String::new(), false);
    for c in call.chars() {
        match c {
            '"' => { quoted = !quoted; cur.push(c) }
            '(' if !quoted => { depth += 1; if depth > 1 { cur.push(c) } }
            ')' if !quoted => { depth -= 1; if depth == 0 { break } cur.push(c) }
            ',' if !quoted && depth == 1 => out.push(std::mem::take(&mut cur).trim().to_string()),
            _ if depth >= 1 => cur.push(c),
            _ => {}
        }
    }
    if !cur.trim().is_empty() { out.push(cur.trim().to_string()) }
    out
}

/// `listOf("a", "b")` -> [a, b].
fn strings(arg: &str) -> Vec<String> {
    arg.split('"').skip(1).step_by(2).map(str::to_string).collect()
}

#[derive(Debug, PartialEq)]
struct Row {
    dir: String,
    extensions: Vec<String>,
    folders: Vec<String>,
    romm: Vec<String>,
    retroachievements: u32,
    screenscraper: u32,
    cores: Vec<String>,
    home: bool,
}

fn kotlin_rows() -> Vec<Row> {
    SYSTEMS_KT.lines()
        .map(str::trim)
        .filter(|line| line.starts_with("SystemRow(") && !line.contains("val "))
        .map(|line| {
            let a = args(line);
            assert_eq!(a.len(), 8, "Systems.TABLE row: {line}");
            Row {
                dir: a[0].clone(),
                extensions: strings(&a[1]),
                folders: strings(&a[2]),
                romm: strings(&a[3]),
                retroachievements: a[4].parse().unwrap(),
                screenscraper: a[5].parse().unwrap(),
                cores: strings(&a[6]),
                home: a[7].trim_start_matches("home =").trim() == "true",
            }
        })
        .collect()
}

fn rust_rows() -> Vec<Row> {
    let own = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    SYSTEMS.iter().map(|s| Row {
        dir: s.dir.to_string(),
        extensions: own(s.extensions),
        folders: own(s.folders),
        romm: own(s.romm),
        retroachievements: s.retroachievements,
        screenscraper: s.screenscraper,
        cores: s.cores.iter().map(|c| c.as_str().to_string()).collect(),
        home: !s.handheld,
    }).collect()
}

#[test]
fn android_table_matches_the_native_one() {
    assert_eq!(kotlin_rows(), rust_rows());
}

#[test]
fn android_names_each_core_folder_as_retroarch_does() {
    // Systems.FOLDERS: "genesis_plus_gx" to "Genesis Plus GX".
    for system in &SYSTEMS {
        for core in system.cores {
            let entry = format!("\"{}\" to \"{}\"", core.as_str(), core.text());
            assert!(SYSTEMS_KT.contains(&entry), "Systems.FOLDERS lacks {entry}");
        }
    }
}

#[test]
fn label_harvester_uses_the_same_screenscraper_ids() {
    let start = HARVEST_PY.find("SS_SYSTEM = {").expect("SS_SYSTEM");
    let line = &HARVEST_PY[start..start + HARVEST_PY[start..].find('}').expect("SS_SYSTEM's end")];
    for system in &SYSTEMS {
        let entry = format!("'{}': {}", system.dir, system.screenscraper);
        assert!(line.contains(&entry), "harvest.py SS_SYSTEM lacks {entry}");
    }
}

#[test]
fn every_system_has_a_complete_row() {
    let mut seen = std::collections::HashSet::new();
    for system in &SYSTEMS {
        let p = system.platform;
        assert!(seen.insert(system.dir), "{p:?}: dir twice");
        assert_eq!(Platform::parse(system.dir), Some(p));
        assert!(!system.name.is_empty() && !system.extensions.is_empty(), "{p:?}");
        assert!(!system.folders.is_empty() && !system.romm.is_empty(), "{p:?}");
        assert!(system.retroachievements > 0 && system.screenscraper > 0, "{p:?}");
        // The WonderSwan's board has one core: Beetle Cygne in both sockets.
        assert!(system.cores[0] != system.cores[1] || matches!(p, Platform::Ws | Platform::Pokemini), "{p:?}: two sockets, one core");
        for core in system.cores {
            assert!(core.runs(p), "{p:?} {core:?}");
            assert_eq!(Core::parse(core.as_str()), Some(core));
        }
        assert!(system.picture.0 > 0 && system.picture.1 > 0, "{p:?}");
        // The names its controls, wallpaper and Cart Studio are filed under.
        let key = system.dir.to_ascii_lowercase();
        assert!(crate::controls::SYSTEMS.contains(&key.as_str()), "{p:?}: controls");
        assert!(slot_ui::mosaic::MOSAIC_KEYS.contains(&system.dir), "{p:?}: shelf wallpaper");
    }
}

/// The screens slot. ships on, by their panel: every system must offer a
/// picture, a default ratio and its Settings rows on each of them.
pub(crate) const SCREENS: [(&str, (u32, u32)); 7] = [
    ("KPA (3:2)", (960, 640)),
    ("RPC / 1240x1080", (1240, 1080)),
    ("RP5 / 16:9", (1920, 1080)),
    ("RG Rotate (1:1)", (720, 720)),
    ("4:3", (1024, 768)),
    ("16:9 720p", (1280, 720)),
    ("5:3", (1920, 1152)),
];

#[test]
fn every_system_starts_with_a_picture_on_every_screen() {
    use crate::video_mode::{default_mode, picture_rect, VideoMode};
    for system in &SYSTEMS {
        for (name, panel) in SCREENS {
            let mode = default_mode(system.platform, panel);
            for m in [mode, VideoMode::Actual, VideoMode::Aspect, VideoMode::Stretch] {
                let [x, y, w, h] = picture_rect(system.platform, m, panel);
                assert!(w > 0.0 && h > 0.0, "{:?} {m:?} on {name}", system.platform);
                let (native_w, native_h) = system.picture;
                let cx = (-x).max(x + w - panel.0 as f32).max(0.0);
                let cy = (-y).max(y + h - panel.1 as f32).max(0.0);
                if m == VideoMode::Actual {
                    // Integer may deliberately crop at most its limit of native
                    // dots per edge (8 home, 4 handheld; one output pixel of
                    // placement rounding).
                    let limit = crate::video_mode::crop_limit(system.platform);
                    assert!(cx <= limit * w / native_w as f32 + 1.0
                        && cy <= limit * h / native_h as f32 + 1.0,
                        "{:?} {m:?} on {name} crops too much: {cx}, {cy}", system.platform);
                } else {
                    assert!(cx <= 0.5 && cy <= 0.5,
                        "{:?} {m:?} on {name} leaves the screen", system.platform);
                }
            }
        }
    }
}

#[test]
fn the_six_systems_keep_what_they_had_before_the_table() {
    // 2.5.0's hand-written values, before slot_store::SYSTEMS.
    use Platform::*;
    let before: [(Platform, &str, &str, &[&str], (u32, u32), bool, [Core; 2]); 6] = [
        (Gba, "Game Boy Advance", "GBA", &["gba"], (240, 160), true, [Core::Mgba, Core::Gpsp]),
        (Gb, "Game Boy", "GB", &["gb", "gbc"], (160, 144), true, [Core::Mgba, Core::Gambatte]),
        (Gbc, "Game Boy Color", "GBC", &["gb", "gbc"], (160, 144), true, [Core::Mgba, Core::Gambatte]),
        (Nes, "NES", "NES", &["nes", "fc", "unf", "unif"], (256, 240), false, [Core::Fceumm, Core::Nestopia]),
        (Snes, "Super Nintendo", "SNES", &["sfc", "smc"], (256, 224), false, [Core::Snes9x, Core::Bsnes]),
        (Md, "Mega Drive", "MD", &["md", "gen", "smd", "bin"], (320, 224), false, [Core::GenesisPlusGx, Core::Picodrive]),
    ];
    assert_eq!(Platform::ALL[..6], before.map(|b| b.0));
    for (p, name, dir, ext, picture, handheld, cores) in before {
        assert_eq!((p.name(), p.dir_name(), p.extensions(), p.picture(), p.handheld()),
            (name, dir, ext, picture, handheld), "{p:?}");
        assert_eq!([Core::primary_for(p), Core::alternate_for(p), Core::default_for(p)],
            [cores[0], cores[1], cores[0]], "{p:?}");
    }
    // The default core ini keys stay "gba", "gb"... (Config/default_core.ini).
    let dir = std::env::temp_dir().join(format!("slot-systems-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Config")).unwrap();
    std::fs::write(dir.join(slot_store::DEFAULT_CORE_FILE),
        "gba=gpsp\ngb=gambatte\ngbc=gambatte\nnes=nestopia\nsnes=bsnes\nmd=picodrive\n").unwrap();
    for p in &Platform::ALL[..6] {
        assert_eq!(slot_store::default_core(&dir, *p), Core::alternate_for(*p), "{p:?}");
    }
    let _ = std::fs::remove_dir_all(dir);
}
