//! Per-ROM core choice, and the file names of everything slot. keeps for one
//! game. Keyed by the game's portable name (`library::game_key`) since 2.5,
//! by its Android address before: the old files are carried over when first
//! needed, and never deleted.
use std::path::{Path, PathBuf};
use slot_store::{choose_core, core_for, core_for_platform, default_core, write_default_core, Core,
    Platform, SELECTED_CORE_FILE};

/// A game's file key: its portable name's stable digest.
pub fn key(uri: &str) -> String {
    fnv(&crate::library::game_key(uri))
}

/// The key slot. 2.4 and older used: the digest of the Android address.
pub fn legacy_key(uri: &str) -> String {
    fnv(uri)
}

/// The file key of a portable name (`library::game_key`).
pub fn key_of_name(name: &str) -> String {
    fnv(name)
}

fn fnv(text: &str) -> String {
    // Stable FNV-1a 64-bit digest; independent of RandomState and process.
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in text.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// `dir/<key><suffix>` for this game. A file slot. 2.4 kept under the old
/// key is copied there first, so nothing is lost; the old one stays.
pub fn game_file(dir: &Path, uri: &str, suffix: &str) -> PathBuf {
    let path = dir.join(format!("{}{suffix}", key(uri)));
    carry(&dir.join(format!("{}{suffix}", legacy_key(uri))), &path);
    path
}

/// Copies `old` to `new` when only the old one exists. The old file is
/// then kept as `<name>.carried`, so a later reset of the game's file is
/// not undone by carrying it over again.
pub fn carry(old: &Path, new: &Path) {
    if old == new || new.exists() || !old.is_file() { return; }
    if let Some(parent) = new.parent() { let _ = std::fs::create_dir_all(parent); }
    let temp = new.with_extension("carry.tmp");
    if std::fs::copy(old, &temp).is_err() { let _ = std::fs::remove_file(&temp); return; }
    if std::fs::rename(&temp, new).is_err() { let _ = std::fs::remove_file(&temp); return; }
    let mut kept = old.as_os_str().to_owned();
    kept.push(".carried");
    let _ = std::fs::rename(old, PathBuf::from(kept));
}

/// The per-game core choice made in slot. 2.4, under the new key.
fn carry_core(storage: &Path, uri: &str) {
    let (new, old) = (key(uri), legacy_key(uri));
    if new == old || slot_store::ini::value(storage, SELECTED_CORE_FILE, &new).is_some() { return; }
    if let Some(value) = slot_store::ini::value(storage, SELECTED_CORE_FILE, &old) {
        if slot_store::ini::write_entry(storage, SELECTED_CORE_FILE, &new, Some(&value)).is_ok() {
            // Moved, so choosing the default later isn't undone.
            let _ = slot_store::ini::write_entry(storage, SELECTED_CORE_FILE, &old, None);
        }
    }
}

pub fn selected(storage: &Path, uri: &str) -> Core {
    carry_core(storage, uri);
    match platform_of(uri) {
        Some(platform) => selected_for(storage, uri, platform),
        None => core_for(storage, &key(uri)),
    }
}

/// The system of a shelf cart, from the library Android handed over.
fn platform_of(uri: &str) -> Option<Platform> {
    let library = crate::library::LIBRARY.lock().unwrap_or_else(|e| e.into_inner());
    library.entries.as_ref()?.iter()
        .find(|entry| entry.uri == uri)
        .and_then(|entry| Platform::parse(&entry.platform))
}

/// The core this cart actually runs on: a stale choice the platform
/// cannot run (gpSP on a GB cart, Gambatte on a GBA cart) falls back.
pub fn selected_for(storage: &Path, uri: &str, platform: Platform) -> Core {
    carry_core(storage, uri);
    core_for_platform(storage, &key(uri), platform)
}

/// The board's pick for one cart; picking the system's default clears it.
pub fn choose(storage: &Path, uri: &str, platform: Platform, core: Core) -> std::io::Result<()> {
    choose_core(storage, &key(uri), platform, core)
}

pub fn default(storage: &Path, platform: Platform) -> Core {
    default_core(storage, platform)
}

pub fn set_default(storage: &Path, platform: Platform, core: Core) -> std::io::Result<()> {
    write_default_core(storage, platform, core)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_unique_rom_uri_keys() {
        // A cart outside the library keeps slot. 2.4's key.
        assert_eq!(key("content://roms/1"), "cd3adfea5b691b9d");
        assert_eq!(legacy_key("content://roms/1"), "cd3adfea5b691b9d");
        assert_ne!(key("content://roms/1"), key("content://roms/2"));
        assert_ne!(key("content://roms/a"), key("content://other/a"));
    }
    #[test]
    fn an_old_file_is_carried_and_kept() {
        let dir = std::env::temp_dir().join(format!("slot-carry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (old, new) = (dir.join("old.json"), dir.join("sub/new.json"));
        std::fs::write(&old, b"2.4").unwrap();
        carry(&old, &new);
        assert_eq!(std::fs::read(&new).unwrap(), b"2.4");
        assert_eq!(std::fs::read(dir.join("old.json.carried")).unwrap(), b"2.4", "never deleted");
        std::fs::remove_file(&new).unwrap();
        carry(&old, &new);
        assert!(!new.exists(), "a reset game stays reset");
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn core_file_matches_available_binary_names() {
        assert_eq!(Core::Mgba.as_str(), "mgba");
        assert_eq!(Core::Gpsp.as_str(), "gpsp");
        assert_eq!(Core::Gambatte.as_str(), "gambatte");
    }
}
