//! Per-ROM core choice keyed by the opaque SAF URI; no filename collisions.
use std::path::Path;
use slot_store::{choose_core, core_for, core_for_platform, default_core, write_default_core, Core,
    Platform};

pub fn key(uri: &str) -> String {
    // Stable FNV-1a 64-bit digest; independent of RandomState and process.
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in uri.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

pub fn selected(storage: &Path, uri: &str) -> Core {
    match platform_of(uri) {
        Some(platform) => selected_for(storage, uri, platform),
        None => core_for(storage, &key(uri)),
    }
}

/// The system of a shelf cart, from the library Android handed over.
fn platform_of(uri: &str) -> Option<Platform> {
    if is_splore(uri) { return Some(Platform::Pico8); }
    let library = crate::library::LIBRARY.lock().unwrap_or_else(|e| e.into_inner());
    library.entries.as_ref()?.iter()
        .find(|entry| entry.uri == uri)
        .and_then(|entry| Platform::parse(&entry.platform))
}

/// The core this cart actually runs on: a stale choice the platform
/// cannot run (gpSP on a GB cart, Gambatte on a GBA cart) falls back.
pub fn selected_for(storage: &Path, uri: &str, platform: Platform) -> Core {
    // Splore is the player's own PICO-8 alone: fake-08 has none.
    if platform == Platform::Pico8 && is_splore(uri) { return Core::Pico8; }
    core_for_platform(storage, &key(uri), platform)
}

/// Splore, PICO-8's own cart browser, is a cart of slot.'s own on the
/// PICO-8 shelf (library_carts): it is not a file in the ROM folder.
pub const SPLORE_URI: &str = "slot://pico8/Splore.p8";

pub fn is_splore(uri: &str) -> bool { uri == SPLORE_URI }

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
    fn splore_is_slots_own_cart() {
        assert!(is_splore(SPLORE_URI));
        // a file of that name in the ROM folder is a cart like any other
        assert!(!is_splore("content://com.android.externalstorage.documents/tree/primary%3ARoms/document/primary%3ARoms%2Fpico8%2FSplore.p8"));
        assert!(!is_splore("/sdcard/Roms/pico8/celeste.p8"));
    }
    #[test]
    fn stable_unique_rom_uri_keys() {
        assert_eq!(key("content://roms/1"), "cd3adfea5b691b9d");
        assert_ne!(key("content://roms/1"), key("content://roms/2"));
        assert_ne!(key("content://roms/a"), key("content://other/a"));
    }
    #[test]
    fn core_file_matches_available_binary_names() {
        assert_eq!(Core::Mgba.as_str(), "mgba");
        assert_eq!(Core::Gpsp.as_str(), "gpsp");
        assert_eq!(Core::Gambatte.as_str(), "gambatte");
    }
}
