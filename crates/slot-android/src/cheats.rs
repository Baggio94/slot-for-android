//! Cheats, as in RetroArch: every cheat file of libretro-database comes
//! with slot. (packed by tools/cheats/pack.py, CC BY-SA 4.0), found for a
//! game by its checksum (No-Intro) or, failing that, by its name.
//!
//! The cheats a player turns on are kept per game in a RetroArch .cht file
//! (Cheats/<system>/<game>.cht) holding just those, and are applied again
//! each time the game starts.
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use slot_store::Platform;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cheat {
    pub desc: String,
    pub code: String,
}

/// One cheat file of the game: "Game Genie", "Action Replay", "Codes"...
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub label: String,
    pub cheats: Vec<Cheat>,
}

fn pack(platform: Platform) -> &'static [u8] {
    match platform {
        Platform::Gb => include_bytes!("../cheats/gb.cht.z"),
        Platform::Gbc => include_bytes!("../cheats/gbc.cht.z"),
        Platform::Gba => include_bytes!("../cheats/gba.cht.z"),
        Platform::Nes => include_bytes!("../cheats/nes.cht.z"),
        Platform::Snes => include_bytes!("../cheats/snes.cht.z"),
        Platform::Md => include_bytes!("../cheats/md.cht.z"),
    }
}

/// The tags naming a cheat device, after the game's own name.
const DEVICES: [&str; 8] = ["Action Replay", "Pro Action Replay", "Game Genie", "GameShark",
    "Code Breaker", "Xploder", "Game Genie 2", "diff"];

fn strip_devices(mut stem: &str) -> &str {
    'outer: loop {
        for tag in DEVICES {
            if let Some(rest) = stem.strip_suffix(')')
                .and_then(|s| s.strip_suffix(tag))
                .and_then(|s| s.strip_suffix(" (")) {
                stem = rest;
                continue 'outer;
            }
        }
        return stem;
    }
}

/// The bare title: no tags, lower case, words only.
fn title(name: &str) -> String {
    let mut bare = String::new();
    let mut depth = 0u32;
    for c in name.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => bare.push(c),
            _ => {}
        }
    }
    let lower = bare.to_lowercase();
    lower.split(|c: char| !c.is_ascii_alphanumeric() && c.is_ascii())
        .filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

/// The device tag of a cheat file, else its first tag (a region), else "Codes".
fn label_of(stem: &str) -> String {
    let device = DEVICES.iter().filter(|tag| **tag != "diff")
        .find(|tag| stem.contains(&format!("({tag})")));
    match device {
        Some(tag) => (*tag).to_owned(),
        None => "Codes".to_owned(),
    }
}

fn region_of(stem: &str) -> Option<&str> {
    let start = stem.find(" (")? + 2;
    let end = stem[start..].find(')')? + start;
    Some(&stem[start..end])
}

/// RetroArch's file names: the game's name with & and the characters a
/// file name can't hold as "_".
fn file_safe(name: &str) -> String {
    name.chars().map(|c| if "&*/:`<>?\\|\"".contains(c) { '_' } else { c }).collect()
}

/// The checksums the game may have in No-Intro: the whole file, and
/// without a NES (iNES) or SNES copier header.
pub fn rom_checksums(platform: Platform, rom: &[u8]) -> Vec<u32> {
    let crc = |data: &[u8]| { let mut c = flate2::Crc::new(); c.update(data); c.sum() };
    let mut sums = vec![crc(rom)];
    if platform == Platform::Nes && rom.len() > 16 && rom.starts_with(b"NES\x1a") {
        sums.push(crc(&rom[16..]));
    }
    if platform == Platform::Snes && rom.len() > 512 && rom.len() % 1024 == 512 {
        sums.push(crc(&rom[512..]));
    }
    sums
}

/// The game's cheat files, best first. Empty when there are none.
pub fn lookup(platform: Platform, checksums: &[u32], rom_stem: &str) -> Vec<Source> {
    let mut text = String::new();
    if flate2::read::ZlibDecoder::new(pack(platform)).read_to_string(&mut text).is_err() {
        return Vec::new();
    }
    let wanted: Vec<String> = checksums.iter().map(|c| format!("{c:08x}")).collect();
    let mut picked: BTreeSet<usize> = BTreeSet::new();
    let mut names: Vec<&str> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("R ") {
            let mut parts = rest.splitn(2, ' ');
            let (Some(crc), Some(files)) = (parts.next(), parts.next()) else { continue };
            if wanted.iter().any(|w| w == crc) {
                picked.extend(files.split(',').filter_map(|i| i.parse::<usize>().ok()));
            }
        } else if let Some(name) = line.strip_prefix("F ") {
            names.push(name);
        }
    }
    if picked.is_empty() {
        // Not a No-Intro dump: the ROM's own file name, as RetroArch would.
        let stem = file_safe(rom_stem);
        picked.extend(names.iter().enumerate()
            .filter(|(_, name)| strip_devices(name) == stem).map(|(i, _)| i));
        if picked.is_empty() {
            let bare = title(rom_stem);
            if !bare.is_empty() {
                picked.extend(names.iter().enumerate()
                    .filter(|(_, name)| title(name) == bare).map(|(i, _)| i));
            }
        }
    }
    if picked.is_empty() { return Vec::new(); }
    let mut sources: Vec<(String, Source)> = Vec::new();
    let mut file = 0usize;
    let mut current: Option<usize> = None;
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("F ") {
            current = None;
            if picked.contains(&file) {
                sources.push((name.to_owned(), Source { label: label_of(name), cheats: Vec::new() }));
                current = Some(sources.len() - 1);
            }
            file += 1;
        } else if let (Some(rest), Some(at)) = (line.strip_prefix("C "), current) {
            if let Some((desc, code)) = rest.split_once('\t') {
                sources[at].1.cheats.push(Cheat { desc: desc.to_owned(), code: code.to_owned() });
            }
        }
    }
    // Two files of one kind (two regions): tell them apart.
    let labels: Vec<String> = sources.iter().map(|(_, s)| s.label.clone()).collect();
    for (name, source) in &mut sources {
        if labels.iter().filter(|l| **l == source.label).count() > 1 {
            if let Some(region) = region_of(name) {
                source.label = format!("{} ({region})", source.label);
            }
        }
    }
    // The game's own codes first, then the devices, as named.
    sources.sort_by_key(|(_, s)| (s.label != "Codes", s.label.clone()));
    sources.into_iter().map(|(_, s)| s).filter(|s| !s.cheats.is_empty()).collect()
}

/// Where a game's cheats turned on are kept.
pub fn saved_path(storage: &Path, platform: Platform, rom_stem: &str) -> PathBuf {
    storage.join("Cheats").join(platform.dir_name()).join(format!("{rom_stem}.cht"))
}

/// The cheats turned on for a game (a RetroArch .cht: only enabled ones).
pub fn read_saved(path: &Path) -> Vec<Cheat> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    parse_cht(&text).into_iter().filter(|(_, on)| *on).map(|(cheat, _)| cheat).collect()
}

fn parse_cht(text: &str) -> Vec<(Cheat, bool)> {
    let mut entries: std::collections::BTreeMap<usize, (String, String, bool)> = Default::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.trim();
        let Some(rest) = key.strip_prefix("cheat") else { continue };
        let Some((index, field)) = rest.split_once('_') else { continue };
        let Ok(index) = index.parse::<usize>() else { continue };
        let value = value.trim().trim_matches('"').to_owned();
        let entry = entries.entry(index).or_default();
        match field {
            "desc" => entry.0 = value,
            "code" => entry.1 = value,
            "enable" => entry.2 = value == "true",
            _ => {}
        }
    }
    entries.into_values().filter(|(_, code, _)| !code.is_empty())
        .map(|(desc, code, on)| (Cheat { desc, code }, on)).collect()
}

fn quoted(text: &str) -> String { text.replace('"', "'") }

/// Keep the cheats turned on; none removes the file.
pub fn write_saved(path: &Path, cheats: &[Cheat]) -> std::io::Result<()> {
    if cheats.is_empty() {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() { std::fs::create_dir_all(dir)?; }
    let mut text = format!("cheats = {}\n", cheats.len());
    for (i, cheat) in cheats.iter().enumerate() {
        text.push_str(&format!("\ncheat{i}_desc = \"{}\"\ncheat{i}_code = \"{}\"\ncheat{i}_enable = true\n",
            quoted(&cheat.desc), quoted(&cheat.code)));
    }
    let partial = path.with_extension("cht.partial");
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pack_opens() {
        for platform in Platform::ALL {
            let mut text = String::new();
            flate2::read::ZlibDecoder::new(pack(platform)).read_to_string(&mut text).unwrap();
            assert!(text.starts_with("slot-cheats 1\n"), "{platform:?}");
            assert!(text.lines().any(|l| l.starts_with("R ")), "{platform:?}");
        }
    }

    #[test]
    fn found_by_name_with_its_devices() {
        let sources = lookup(Platform::Snes, &[], "Super Mario World (USA)");
        let labels: Vec<&str> = sources.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels.first(), Some(&"Codes"));
        assert!(labels.contains(&"Game Genie"), "{labels:?}");
        assert!(sources.iter().all(|s| !s.cheats.is_empty()));
    }

    #[test]
    fn found_by_bare_title() {
        assert!(!lookup(Platform::Snes, &[], "super mario world").is_empty());
        assert!(lookup(Platform::Snes, &[], "No Such Game At All (USA)").is_empty());
    }

    #[test]
    fn device_tags_come_off() {
        assert_eq!(strip_devices("Tetris (World) (Game Genie)"), "Tetris (World)");
        assert_eq!(strip_devices("A (USA, Europe) (Action Replay) (diff)"), "A (USA, Europe)");
        assert_eq!(title("Addams Family, The (USA) (Game Genie)"), "addams family the");
    }

    #[test]
    fn headers_give_a_second_checksum() {
        let mut nes = b"NES\x1a".to_vec();
        nes.resize(16 + 1024, 0);
        assert_eq!(rom_checksums(Platform::Nes, &nes).len(), 2);
        assert_eq!(rom_checksums(Platform::Gba, &[1, 2, 3]).len(), 1);
    }

    #[test]
    fn saved_cheats_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = saved_path(dir.path(), Platform::Gba, "Game (USA)");
        let cheats = vec![
            Cheat { desc: "Master Code".into(), code: "D8BAE4D9+4864DCE5".into() },
            Cheat { desc: "Say \"hi\"".into(), code: "320375D4+0000".into() },
        ];
        write_saved(&path, &cheats).unwrap();
        let back = read_saved(&path);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0], cheats[0]);
        assert_eq!(back[1].desc, "Say 'hi'");
        write_saved(&path, &[]).unwrap();
        assert!(!path.exists());
        assert!(read_saved(&path).is_empty());
    }
}
