//! Background library preparation. A fingerprint avoids re-reading unchanged ROMs at boot.
use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::storage;

#[derive(Serialize, Deserialize)]
struct Entry {
    size: u64,
    modified_ns: u128,
    hash: String,
    #[serde(default)]
    checked_at: u64,
    #[serde(default)]
    badge_version: u8,
}

type Index = BTreeMap<PathBuf, Entry>;

fn fingerprint(path: &Path) -> Result<(u64, u128), String> {
    let metadata = path.metadata().map_err(|_| "Cannot read ROM metadata")?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    Ok((metadata.len(), modified))
}

pub(crate) fn hash(dir: &Path, path: &Path) -> Result<String, String> {
    let (size, modified_ns) = fingerprint(path)?;
    let index_path = dir.join("library.json");
    let mut index: Index = storage::read(&index_path).unwrap_or_default();
    if let Some(entry) = index
        .get(path)
        .filter(|e| e.size == size && e.modified_ns == modified_ns && modified_ns != 0)
    {
        return Ok(entry.hash.clone());
    }
    let hash = content_hash(path)?;
    // Fingerprinting is a convenience cache; a failure here must not stop identification.
    index.insert(
        path.into(),
        Entry {
            size,
            modified_ns,
            hash: hash.clone(),
            checked_at: 0,
            badge_version: 0,
        },
    );
    let _ = storage::write(&index_path, &index);
    Ok(hash)
}

pub(crate) fn pending(root: &Path, dir: &Path, now: u64) -> VecDeque<PathBuf> {
    let index: Index = storage::read(&dir.join("library.json")).unwrap_or_default();
    let mut paths: Vec<_> = std::fs::read_dir(root.join("Games/GBA"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("gba"))
                && !path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        })
        .filter(|path| {
            let Some(entry) = index.get(path) else {
                return true;
            };
            // Refresh definitions and server unlocks weekly, and reconsider unknown ROMs.
            fingerprint(path).ok() != Some((entry.size, entry.modified_ns))
                || entry.checked_at == 0
                || entry.badge_version < 2
                || now.saturating_sub(entry.checked_at) >= 7 * 86400
                || !dir.join(format!("{}.json", entry.hash)).exists()
        })
        .collect();
    paths.sort();
    paths.into()
}

pub(crate) fn checked(dir: &Path, path: &Path, now: u64) {
    let index_path = dir.join("library.json");
    let mut index: Index = storage::read(&index_path).unwrap_or_default();
    if let Some(entry) = index.get_mut(path) {
        entry.checked_at = now;
        entry.badge_version = 2;
    }
    let _ = storage::write(&index_path, &index);
}

fn neo_hash(path: &Path, size: u64) -> Result<String, String> {
    const HEADER: u64 = 4096;
    let mut file = std::fs::File::open(path).map_err(|_| "Cannot open ROM")?;
    let mut magic = [0u8; 4];
    if size <= HEADER || file.read_exact(&mut magic).is_err() || &magic != b"NEO\x01" {
        return Err("Not a valid .neo file".into());
    }
    file.seek(SeekFrom::Start(HEADER)).map_err(|_| "Cannot read ROM")?;
    let mut left = (size - HEADER).min(64 * 1024 * 1024);
    let mut buffer = vec![0u8; 64 * 1024];
    let mut md5 = md5::Context::new();
    while left > 0 {
        let want = left.min(buffer.len() as u64) as usize;
        file.read_exact(&mut buffer[..want]).map_err(|_| "Cannot read ROM")?;
        md5.consume(&buffer[..want]);
        left -= want as u64;
    }
    Ok(format!("{:x}", md5.compute()))
}

pub(crate) fn content_hash(path: &Path) -> Result<String, String> {
    let size = path
        .metadata()
        .map_err(|_| "Cannot read ROM metadata")?
        .len();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    // A Geolith .neo: its ROM data after the 4 KiB header (rcheevos 12.4
    // rc_hash_neogeo_cart), the first 64 MiB of it for the biggest sets.
    if ext == "neo" {
        return neo_hash(path, size);
    }
    if size == 0 || size > 64 * 1024 * 1024 {
        return Err("Unsupported GBA ROM size".into());
    }
    // An arcade set (a Neo Geo game on FinalBurn Neo) is known by its
    // name: mslug for mslug.zip (rcheevos rc_hash, Arcade).
    if ext == "zip" {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if stem.is_empty() { return Err("Unnamed arcade set".into()); }
        return Ok(format!("{:x}", md5::compute(stem.as_bytes())));
    }
    let mut file = std::fs::File::open(path).map_err(|_| "Cannot open ROM")?;
    let mut buffer = [0; 64 * 1024];
    let mut md5 = md5::Context::new();
    // RetroAchievements hashes NES carts without their iNES/FDS header and
    // SNES carts without a 512-byte copier header (rcheevos rc_hash).
    let skip = match ext.as_str() {
        "nes" | "fc" | "unf" | "unif" => {
            let mut head = [0u8; 4];
            let read = file.read(&mut head).map_err(|_| "Cannot read ROM")?;
            file.seek(SeekFrom::Start(0)).map_err(|_| "Cannot read ROM")?;
            if read == 4 && (&head == b"NES\x1a" || &head == b"FDS\x1a") { 16 } else { 0 }
        }
        "sfc" | "smc" if size % 0x2000 == 512 => 512,
        // A HuCard with a copier's 512-byte header (rc_hash: size % 1024).
        "pce" | "sgx" if size % 1024 == 512 => 512,
        // A .lnx Lynx cart carries a 64-byte "LYNX" header RetroAchievements
        // leaves out (rc_hash), as No-Intro does.
        "lnx" | "lyx" => {
            let mut head = [0u8; 5];
            let read = file.read(&mut head).map_err(|_| "Cannot read ROM")?;
            file.seek(SeekFrom::Start(0)).map_err(|_| "Cannot read ROM")?;
            if read == 5 && &head == b"LYNX\0" { 64 } else { 0 }
        }
        _ => 0,
    };
    file.seek(SeekFrom::Start(skip)).map_err(|_| "Cannot read ROM")?;
    loop {
        let n = file.read(&mut buffer).map_err(|_| "Cannot read ROM")?;
        if n == 0 {
            break;
        }
        md5.consume(&buffer[..n]);
    }
    Ok(format!("{:x}", md5.compute()))
}
