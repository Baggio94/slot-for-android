//! GB/GBC carts run on mGBA or Gambatte. Both keep RetroArch's per-core
//! folders (Saves/mGBA, Saves/Gambatte), so switching the chip on the board
//! must bring the newer cartridge battery along instead of starting over.
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use slot_store::{Core, Platform};

/// Library > Share Saves: off keeps each core's own battery, like mGBA/gpSP.
static SHARING: AtomicBool = AtomicBool::new(false);

pub fn set_sharing(enabled: bool) { SHARING.store(enabled, Ordering::Release); }

pub fn sharing() -> bool { SHARING.load(Ordering::Acquire) }

/// Slot's palette list uses mGBA's names; Gambatte ships the same GBC boot
/// and SGB palettes under its own names.
pub fn gambatte_palette(mgba: &str) -> Option<&'static str> {
    Some(match mgba {
        "Grayscale" | "GBC Gray ←B" => "GBC - Grayscale",
        "DMG Green" => "GB - DMG",
        "GB Pocket" => "GB - Pocket",
        "GB Light" => "GB - Light",
        "GBC Brown ↑" => "GBC - Brown",
        "GBC Red ↑A" => "GBC - Red",
        "GBC Dark Brown ↑B" => "GBC - Dark Brown",
        "GBC Pale Yellow ↓" => "GBC - Pastel Mix",
        "GBC Orange ↓A" => "GBC - Orange",
        "GBC Yellow ↓B" => "GBC - Yellow",
        "GBC Blue ←" => "GBC - Blue",
        "GBC Dark Blue ←A" => "GBC - Dark Blue",
        "GBC Green →" => "GBC - Green",
        "GBC Dark Green →A" => "GBC - Dark Green",
        "GBC Reverse →B" => "GBC - Inverted",
        // Its colours are picked by gambatte_twb64() below.
        slot_store::TWB64_GAME_COM => "TWB64 - Pack 1",
        sgb => {
            const SGB: [&str; 32] = [
                "SGB - 1A", "SGB - 1B", "SGB - 1C", "SGB - 1D",
                "SGB - 1E", "SGB - 1F", "SGB - 1G", "SGB - 1H",
                "SGB - 2A", "SGB - 2B", "SGB - 2C", "SGB - 2D",
                "SGB - 2E", "SGB - 2F", "SGB - 2G", "SGB - 2H",
                "SGB - 3A", "SGB - 3B", "SGB - 3C", "SGB - 3D",
                "SGB - 3E", "SGB - 3F", "SGB - 3G", "SGB - 3H",
                "SGB - 4A", "SGB - 4B", "SGB - 4C", "SGB - 4D",
                "SGB - 4E", "SGB - 4F", "SGB - 4G", "SGB - 4H",
            ];
            let wanted = sgb.strip_prefix("SGB ")?.replace('-', "");
            SGB.into_iter().find(|name| name[6..] == wanted)?
        }
    })
}

/// Gambatte's TWB64 Pack 1 entry for a slot. palette from that pack.
pub fn gambatte_twb64(name: &str) -> Option<&'static str> {
    (name == slot_store::TWB64_GAME_COM).then_some("TWB64 028 - Game.com Ver.")
}

/// The other core whose battery file holds the same SRAM: mGBA and
/// Gambatte for GB/GBC, FCEUmm and Nestopia for NES, Snes9x and bsnes for
/// SNES, Genesis Plus GX and PicoDrive for a Mega Drive cart's battery SRAM
/// (see md_shares_sram and carry_md). PokeDaisy keeps its battery in mGBA's
/// own file (battery_core), so it shares as mGBA does.
fn other_core(which: Core, platform: Platform) -> Option<Core> {
    let which = battery_core(which);
    if platform == Platform::Gba || !which.runs(platform) {
        return None;
    }
    let first = Core::primary_for(platform);
    Some(if which == first { Core::alternate_for(platform) } else { first })
}

/// The core whose folder holds this core's in-game save: mGBA's for
/// PokeDaisy, which is mGBA 0.10.5 with its companion over it.
pub fn battery_core(which: Core) -> Core {
    if which == Core::PokeDaisy { Core::Mgba } else { which }
}

/// mGBA and PokeDaisy run one battery file. Which of them ran it last, and
/// the file's time then, is written beside it at each start
/// (`<save>.holder`, kept on this handheld): when the other one ran it since and the file changed,
/// this core's own save states are older than the save.
pub fn battery_changed_hands(save: &Path, which: Core) -> bool {
    let Ok(text) = std::fs::read_to_string(holder_path(save)) else { return false };
    let mut parts = text.split_whitespace();
    let (Some(core), Some(stamp)) = (parts.next(), parts.next()) else { return false };
    core != which.as_str() && stamp != modified_stamp(save)
}

pub fn mark_battery_holder(save: &Path, which: Core) -> std::io::Result<()> {
    atomic_write(&holder_path(save), format!("{} {}\n", which.as_str(), modified_stamp(save)).as_bytes())
}

fn holder_path(save: &Path) -> PathBuf {
    let mut name = save.file_name().unwrap_or_default().to_os_string();
    name.push(".holder");
    save.with_file_name(name)
}

fn modified_stamp(path: &Path) -> String {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or_else(|| "none".to_owned(), |d| d.as_nanos().to_string())
}

/// Before `which` boots a GB/GBC cart, copy the other core's battery over
/// when it is newer and different. Only the SRAM bytes both cores share are
/// replaced: mGBA's trailing RTC block survives a Gambatte save. The file
/// being replaced is kept as `<stem>.srm.before-<other>`.
///
/// Returns true when a battery came across: the caller must not resume an
/// older save state of `which`, or the progress would roll back.
pub fn carry_battery(saves: &Path, stem: &str, which: Core, platform: Platform)
    -> std::io::Result<bool>
{
    carry_battery_for(saves, stem, which, platform, None)
}

/// A Game Boy or Game Boy Color Pokemon game now started on PokeDaisy by
/// default (3.2.0-beta13), Share Core Saves off: the other core's battery
/// comes across once, when PokeDaisy (mGBA's folder) has none yet, so the
/// player's game is there. Never over a save PokeDaisy already has.
pub fn carry_first_battery(saves: &Path, stem: &str, which: Core, platform: Platform)
    -> std::io::Result<bool>
{
    if which != Core::PokeDaisy || !matches!(platform, Platform::Gb | Platform::Gbc) { return Ok(false); }
    let Some(other) = other_core(which, platform) else { return Ok(false) };
    let own_path = saves.join(battery_core(which).text()).join(format!("{stem}.srm"));
    if own_path.exists() { return Ok(false); }
    let theirs = match std::fs::read(saves.join(other.text()).join(format!("{stem}.srm"))) {
        Ok(bytes) if !bytes.is_empty() => bytes,
        Ok(_) => return Ok(false),
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    if let Some(parent) = own_path.parent() { std::fs::create_dir_all(parent)?; }
    atomic_write(&own_path, &theirs)?;
    Ok(true)
}

/// As carry_battery, with the cart's ROM: a Mega Drive cart shares its
/// battery only when md_shares_sram says its save is plain battery SRAM.
pub fn carry_battery_for(saves: &Path, stem: &str, which: Core, platform: Platform,
                         rom: Option<&Path>) -> std::io::Result<bool>
{
    let Some(other) = other_core(which, platform) else { return Ok(false) };
    let which = battery_core(which);
    if platform == Platform::Md {
        let header = match rom {
            Some(rom) => read_head(rom, 0x200)?,
            None => return Ok(false),
        };
        if !md_shares_sram(&header) { return Ok(false); }
    }
    let own_path = saves.join(which.text()).join(format!("{stem}.srm"));
    let their_path = saves.join(other.text()).join(format!("{stem}.srm"));
    let theirs = match std::fs::read(&their_path) {
        Ok(bytes) if !bytes.is_empty() => bytes,
        Ok(_) => return Ok(false),
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    let own = match std::fs::read(&own_path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let mut carried = theirs.clone();
    if let Some(own) = &own {
        let newer = std::fs::metadata(&their_path)?.modified()?
            > std::fs::metadata(&own_path)?.modified()?;
        if !newer { return Ok(false); }
        if platform == Platform::Md {
            carried = carry_md(&theirs, Some(own.len()), which);
            if &carried == own { return Ok(false); }
        } else if matches!(platform, Platform::Sms | Platform::Gg) {
            // Genesis Plus GX cuts its file after the last byte that isn't
            // 0xFF, Gearsystem keeps the cart's 32 KiB: what the shorter file
            // leaves out is 0xFF (unwritten), never the older file's bytes.
            let len = own.len().max(theirs.len());
            carried.resize(len, 0xFF);
            let mut own_full = own.clone();
            own_full.resize(len, 0xFF);
            if carried == own_full { return Ok(false); }
        } else {
            let common = own.len().min(theirs.len());
            if own[..common] == theirs[..common] { return Ok(false); }
            carried.extend_from_slice(&own[common..]);
        }
        atomic_write(&backup_path(&own_path, other), own)?;
    } else if platform == Platform::Md {
        carried = carry_md(&theirs, None, which);
    }
    std::fs::create_dir_all(own_path.parent().unwrap_or(saves))?;
    atomic_write(&own_path, &carried)?;
    Ok(true)
}

/// Mega Drive battery SRAM sits at the same offsets in both cores' files:
/// Genesis Plus GX indexes it by address in a 64 KiB block it cuts after
/// the last byte that is not 0xFF; PicoDrive by offset from the SRAM start,
/// at the cart's real size (both from the ROM header's 0x1B4, word-aligned).
/// So the bytes carry over as they are: into PicoDrive's own file at its
/// size, the rest 0xFF as Genesis Plus GX left it; into Genesis Plus GX
/// as they are, its unwritten bytes staying 0xFF. Never the old file's tail,
/// which would bring back bytes the newer save had erased.
fn carry_md(theirs: &[u8], own_len: Option<usize>, which: Core) -> Vec<u8> {
    let mut out = theirs.to_vec();
    if which == Core::Picodrive {
        if let Some(len) = own_len { out.resize(len, 0xFF); }
    }
    out
}

/// Mega Drive saves kept in an EEPROM (Genesis Plus GX's eeprom_i2c
/// database, pinned f27655b), whose files the two cores need not lay out
/// alike: their product codes, as at 0x183 in the header.
const MD_EEPROM_CODES: [&str; 28] = [
    "T-50176", "T-50396", "T-50446", "T-50516", "T-50606", "T-12046", "T-12053",
    "MK-1215", "MK-1228", "G-5538", "PR-1993", "G-4060", "00001211", "00004076",
    "G-4524", "00054503", "T-81033", "T-081326", "T-081276", "T-81406", "T-081586",
    "T-81476", "T-81576", "T-120106", "T-120096", "T-120146", "T-120066", "T-123456",
];

/// Whether a Mega Drive cart's save is plain battery SRAM: its header
/// declares backup RAM ("RA" at 0x1B0, not the 0xE8 EEPROM mark) and its
/// product code is not one of the EEPROM games.
pub fn md_shares_sram(header: &[u8]) -> bool {
    let Some(ra) = header.get(0x1B0..0x1B3) else { return false };
    if &ra[..2] != b"RA" || ra[2] == 0xE8 { return false; }
    let product = header.get(0x180..0x18E).map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default();
    !MD_EEPROM_CODES.iter().any(|code| product.contains(code))
}

/// Neo Geo Pocket flash saves: Beetle NeoPop's `.flash` and RACE's `.ngf`
/// share NeoPop's layout (0x53, a block count, the file's length, then
/// blocks of NGPC address, length and bytes), but NeoPop's block length is
/// 16 bits (and 2 bytes of padding), RACE's 32: RACE writes whole flash
/// blocks, up to 64 KiB, which NeoPop would read as empty. Each is written
/// the way its core reads it, before the core boots (the cores read their
/// file when the game loads). True when a newer save came across.
pub fn carry_ngp_flash(saves: &Path, stem: &str, which: Core, rom: &Path) -> std::io::Result<bool> {
    let other = match which {
        Core::Neopop => Core::Race,
        Core::Race => Core::Neopop,
        _ => return Ok(false),
    };
    let name = |core: Core| saves.join(core.text())
        .join(format!("{stem}.{}", if core == Core::Neopop { "flash" } else { "ngf" }));
    let (own_path, their_path) = (name(which), name(other));
    let theirs = match std::fs::read(&their_path) {
        Ok(bytes) if !bytes.is_empty() => bytes,
        Ok(_) => return Ok(false),
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    let own = match std::fs::read(&own_path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    if own.is_some() && std::fs::metadata(&their_path)?.modified()?
        <= std::fs::metadata(&own_path)?.modified()? {
        return Ok(false);
    }
    let Some(blocks) = ngp_blocks(&theirs, other == Core::Race) else { return Ok(false) };
    let carried = match which {
        Core::Race => ngp_file(&race_blocks(blocks), true),
        _ => match neopop_blocks(&blocks, rom)? {
            Some(blocks) => ngp_file(&blocks, false),
            None => return Ok(false),
        },
    };
    if own.as_deref() == Some(&carried[..]) { return Ok(false); }
    if let Some(own) = &own { atomic_write(&backup_path(&own_path, other), own)?; }
    std::fs::create_dir_all(own_path.parent().unwrap_or(saves))?;
    atomic_write(&own_path, &carried)?;
    Ok(true)
}

/// (NGPC address, bytes) of a NeoPop-layout file; `wide`: 32-bit lengths (RACE).
fn ngp_blocks(file: &[u8], wide: bool) -> Option<Vec<(u32, Vec<u8>)>> {
    let u16_at = |at: usize| file.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |at: usize| file.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if u16_at(0)? != 0x53 { return None; }
    let count = u16_at(2)? as usize;
    let mut at = 8;
    let mut blocks = Vec::with_capacity(count);
    for _ in 0..count {
        let addr = u32_at(at)?;
        let len = if wide { u32_at(at + 4)? as usize } else { u16_at(at + 4)? as usize };
        at += 8;
        let bytes = file.get(at..at.checked_add(len)?)?.to_vec();
        at += len;
        let flash = (0x200000..0x400000).contains(&addr) || (0x800000..0xA00000).contains(&addr);
        if !flash { return None; }
        blocks.push((addr, bytes));
    }
    Some(blocks)
}

fn ngp_file(blocks: &[(u32, Vec<u8>)], wide: bool) -> Vec<u8> {
    let len = 8 + blocks.iter().map(|(_, b)| 8 + b.len()).sum::<usize>();
    let mut out = Vec::with_capacity(len);
    out.extend_from_slice(&0x53u16.to_le_bytes());
    out.extend_from_slice(&(blocks.len() as u16).to_le_bytes());
    out.extend_from_slice(&(len as u32).to_le_bytes());
    for (addr, bytes) in blocks {
        out.extend_from_slice(&addr.to_le_bytes());
        if wide { out.extend_from_slice(&(bytes.len() as u32).to_le_bytes()); }
        else { out.extend_from_slice(&(bytes.len() as u16).to_le_bytes()); out.extend_from_slice(&[0, 0]); }
        out.extend_from_slice(bytes);
    }
    out
}

/// RACE marks the flash block a range starts in as its own (it writes
/// that whole block back later): ranges are cut where its blocks end, at
/// each 64 KiB and at the boot blocks' 32, 40 and 48 KiB (every cart size).
fn race_blocks(blocks: Vec<(u32, Vec<u8>)>) -> Vec<(u32, Vec<u8>)> {
    let cut = |chip_offset: u32| -> bool {
        chip_offset % 0x10000 == 0 || [0x1F0000u32, 0xF0000, 0x70000].iter()
            .any(|boot| [0x8000u32, 0xA000, 0xC000].iter().any(|o| chip_offset == boot + o))
    };
    let mut out = Vec::new();
    for (addr, bytes) in blocks {
        let mut start = 0usize;
        for i in 1..bytes.len() {
            if cut((addr + i as u32) & 0x1FFFFF) {
                out.push((addr + start as u32, bytes[start..i].to_vec()));
                start = i;
            }
        }
        if start < bytes.len() { out.push((addr + start as u32, bytes[start..].to_vec())); }
    }
    out
}

/// NeoPop keeps only the bytes a game wrote: the ranges where RACE's whole
/// blocks differ from the ROM, close ones joined. None when one would not
/// fit NeoPop's 16-bit length (NeoPop joins ranges that touch).
fn neopop_blocks(blocks: &[(u32, Vec<u8>)], rom: &Path) -> std::io::Result<Option<Vec<(u32, Vec<u8>)>>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(rom)?;
    let size = file.metadata()?.len();
    let mut out: Vec<(u32, Vec<u8>)> = Vec::new();
    for (addr, bytes) in blocks {
        // The cart's own bytes there (the second chip follows the first in the file).
        let offset = u64::from(if *addr >= 0x800000 { addr - 0x600000 } else { addr - 0x200000 });
        let mut original = vec![0xFFu8; bytes.len()];
        if offset < size {
            file.seek(SeekFrom::Start(offset))?;
            let n = (size - offset).min(bytes.len() as u64) as usize;
            file.read_exact(&mut original[..n])?;
        }
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == original[i] { i += 1; continue; }
            let start = i;
            let mut end = i + 1;
            // Runs fewer than 16 bytes apart are one range.
            while end < bytes.len() {
                match (end..bytes.len().min(end + 16)).find(|&j| bytes[j] != original[j]) {
                    Some(j) => end = j + 1,
                    None => break,
                }
            }
            out.push((addr + start as u32, bytes[start..end].to_vec()));
            i = end;
        }
    }
    out.sort_by_key(|(addr, _)| *addr);
    // NeoPop joins ranges that touch: their sum must fit in 16 bits.
    let mut run_start = None::<u32>;
    let mut run_end = 0u32;
    for (addr, bytes) in &out {
        let end = addr + bytes.len() as u32;
        match run_start {
            Some(start) if *addr <= run_end => {
                run_end = run_end.max(end);
                if run_end - start > 0xFFFF { return Ok(None); }
            }
            _ => {
                if bytes.len() > 0xFFFF { return Ok(None); }
                run_start = Some(*addr);
                run_end = end;
            }
        }
    }
    Ok(Some(out))
}

fn read_head(path: &Path, len: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut head = Vec::with_capacity(len);
    std::fs::File::open(path)?.take(len as u64).read_to_end(&mut head)?;
    Ok(head)
}

fn backup_path(own: &Path, other: Core) -> PathBuf {
    let mut name = own.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".before-{}", other.as_str()));
    own.with_file_name(name)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let temp = path.with_file_name(name);
    let mut file = std::fs::File::create(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn put(root: &Path, core: Core, bytes: &[u8], age_s: u64) {
        let dir = root.join(core.text());
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Pokemon Crystal.srm");
        std::fs::write(&path, bytes).unwrap();
        let at = SystemTime::now() - Duration::from_secs(age_s);
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(at).unwrap();
    }

    fn get(root: &Path, core: Core) -> Vec<u8> {
        std::fs::read(root.join(core.text()).join("Pokemon Crystal.srm")).unwrap()
    }

    #[test]
    fn pokedaisy_by_default_finds_the_other_cores_save_once() {
        let d = tempfile::tempdir().unwrap();
        let other = other_core(Core::PokeDaisy, Platform::Gbc).unwrap();
        put(d.path(), other, b"crystal", 100);
        assert!(carry_first_battery(d.path(), "Pokemon Crystal", Core::PokeDaisy, Platform::Gbc).unwrap());
        assert_eq!(get(d.path(), Core::Mgba), b"crystal");
        // Never again, never over PokeDaisy's own.
        put(d.path(), other, b"newer", 0);
        assert!(!carry_first_battery(d.path(), "Pokemon Crystal", Core::PokeDaisy, Platform::Gbc).unwrap());
        assert_eq!(get(d.path(), Core::Mgba), b"crystal");
        // Not another core, not the GBA (one battery already).
        assert!(!carry_first_battery(d.path(), "Pokemon Crystal", Core::Mgba, Platform::Gbc).unwrap());
        assert!(!carry_first_battery(d.path(), "Pokemon Crystal", Core::PokeDaisy, Platform::Gba).unwrap());
    }

    #[test]
    fn every_slot_palette_has_a_gambatte_twin() {
        for palette in slot_store::GbPalette::all() {
            assert!(gambatte_palette(palette.core_name()).is_some(), "{}", palette.core_name());
        }
        assert_eq!(gambatte_palette("SGB 3-F"), Some("SGB - 3F"));
        assert_eq!(gambatte_palette("SGB 9-Z"), None);
    }

    #[test]
    fn first_gambatte_boot_takes_the_mgba_battery() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), Core::Mgba, &[1, 2, 3, 4, 0xEE, 0xEE], 60);
        assert!(carry_battery(d.path(), "Pokemon Crystal", Core::Gambatte, Platform::Gbc).unwrap());
        assert_eq!(get(d.path(), Core::Gambatte), [1, 2, 3, 4, 0xEE, 0xEE]);
    }

    #[test]
    fn newer_gambatte_progress_returns_to_mgba_and_keeps_its_rtc_tail() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), Core::Mgba, &[1, 2, 3, 4, 0xEE, 0xEE], 60);
        put(d.path(), Core::Gambatte, &[9, 9, 9, 9], 10);
        assert!(carry_battery(d.path(), "Pokemon Crystal", Core::Mgba, Platform::Gbc).unwrap());
        assert_eq!(get(d.path(), Core::Mgba), [9, 9, 9, 9, 0xEE, 0xEE]);
        let backup = d.path().join("mGBA/Pokemon Crystal.srm.before-gambatte");
        assert_eq!(std::fs::read(backup).unwrap(), [1, 2, 3, 4, 0xEE, 0xEE]);
    }

    #[test]
    fn same_battery_or_older_one_changes_nothing() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), Core::Mgba, &[1, 2, 3, 4, 0xEE], 10);
        put(d.path(), Core::Gambatte, &[1, 2, 3, 4], 60);
        assert!(!carry_battery(d.path(), "Pokemon Crystal", Core::Mgba, Platform::Gbc).unwrap());
        put(d.path(), Core::Gambatte, &[1, 2, 3, 4], 5);
        assert!(!carry_battery(d.path(), "Pokemon Crystal", Core::Mgba, Platform::Gbc).unwrap());
        put(d.path(), Core::Gambatte, &[7, 7, 7, 7], 60);
        assert!(!carry_battery(d.path(), "Pokemon Crystal", Core::Mgba, Platform::Gbc).unwrap());
        assert_eq!(get(d.path(), Core::Mgba), [1, 2, 3, 4, 0xEE]);
    }

    fn md_header(product: &str, ra: &[u8; 3]) -> Vec<u8> {
        let mut h = vec![0x20u8; 0x200];
        h[0x100..0x104].copy_from_slice(b"SEGA");
        h[0x180..0x180 + product.len()].copy_from_slice(product.as_bytes());
        h[0x1B0..0x1B3].copy_from_slice(ra);
        h
    }

    fn md_put(root: &Path, core: Core, bytes: &[u8], age_s: u64) {
        let dir = root.join(core.text());
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Phantasy Star IV.srm");
        std::fs::write(&path, bytes).unwrap();
        let when = SystemTime::now() - Duration::from_secs(age_s);
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(when).unwrap();
    }

    #[test]
    fn mega_drive_battery_sram_carries_between_genesis_plus_gx_and_picodrive() {
        let d = tempfile::tempdir().unwrap();
        let rom = d.path().join("psiv.md");
        std::fs::write(&rom, md_header("GM MK-1307 -00", b"RA\xF8")).unwrap();
        // PicoDrive: the cart's 16 KiB; odd bytes saved, even ones 0.
        let mut pico = vec![0u8; 0x4000];
        for (i, b) in pico.iter_mut().enumerate() { if i % 2 == 1 { *b = (i / 2) as u8; } }
        md_put(d.path(), Core::Picodrive, &pico, 10);
        // Genesis Plus GX: an older save, cut after its last written byte.
        md_put(d.path(), Core::GenesisPlusGx, &[0xFF, 7, 0xFF, 7], 60);
        assert!(carry_battery_for(d.path(), "Phantasy Star IV", Core::GenesisPlusGx,
            Platform::Md, Some(&rom)).unwrap());
        assert_eq!(std::fs::read(d.path().join("Genesis Plus GX/Phantasy Star IV.srm")).unwrap(), pico,
            "the newer bytes, at the same offsets");
        // Back to PicoDrive after a shorter Genesis Plus GX save: its size,
        // the rest 0xFF, none of PicoDrive's older bytes.
        md_put(d.path(), Core::GenesisPlusGx, &[0xFF, 9, 0xFF, 9], 1);
        assert!(carry_battery_for(d.path(), "Phantasy Star IV", Core::Picodrive,
            Platform::Md, Some(&rom)).unwrap());
        let back = std::fs::read(d.path().join("PicoDrive/Phantasy Star IV.srm")).unwrap();
        assert_eq!(back.len(), 0x4000);
        assert_eq!(&back[..4], &[0xFF, 9, 0xFF, 9]);
        assert!(back[4..].iter().all(|b| *b == 0xFF));
    }

    #[test]
    fn mega_drive_eeprom_games_and_carts_without_backup_ram_keep_their_own_saves() {
        assert!(md_shares_sram(&md_header("GM MK-1307 -00", b"RA\xF8")));
        assert!(!md_shares_sram(&md_header("GM T-081326 00", b"RA\xF8")), "NBA Jam: EEPROM");
        assert!(!md_shares_sram(&md_header("GM G-4060  -00", b"RA\xF8")), "Monster World: EEPROM");
        assert!(!md_shares_sram(&md_header("GM MK-1307 -00", b"RA\xE8")), "EEPROM mark");
        assert!(!md_shares_sram(&md_header("GM MK-1307 -00", b"  \x20")), "no backup RAM");
        let d = tempfile::tempdir().unwrap();
        let rom = d.path().join("nbajam.md");
        std::fs::write(&rom, md_header("GM T-081326 00", b"RA\xF8")).unwrap();
        md_put(d.path(), Core::Picodrive, &[1, 2, 3], 1);
        assert!(!carry_battery_for(d.path(), "Phantasy Star IV", Core::GenesisPlusGx,
            Platform::Md, Some(&rom)).unwrap());
        assert!(!carry_battery(d.path(), "Phantasy Star IV", Core::GenesisPlusGx, Platform::Md).unwrap(),
            "without its ROM a Mega Drive cart never shares");
    }

    #[test]
    fn master_system_saves_carry_with_ff_where_the_shorter_file_ends() {
        let d = tempfile::tempdir().unwrap();
        let at = |core: Core| d.path().join(core.text()).join("Phantasy Star.srm");
        let put = |core: Core, bytes: &[u8], age_s: u64| {
            std::fs::create_dir_all(d.path().join(core.text())).unwrap();
            std::fs::write(at(core), bytes).unwrap();
            let when = SystemTime::now() - Duration::from_secs(age_s);
            std::fs::File::options().write(true).open(at(core)).unwrap().set_modified(when).unwrap();
        };
        // Gearsystem's 32 KiB, an older save with a byte at 0x100.
        let mut gear = vec![0xFFu8; 0x8000];
        gear[0] = 1; gear[0x100] = 9;
        put(Core::Gearsystem, &gear, 60);
        // Genesis Plus GX's newer save, cut after its last written byte.
        put(Core::GenesisPlusGx, &[2, 0xFF, 3], 1);
        assert!(carry_battery_for(d.path(), "Phantasy Star", Core::Gearsystem, Platform::Sms, None).unwrap());
        let back = std::fs::read(at(Core::Gearsystem)).unwrap();
        assert_eq!(back.len(), 0x8000);
        assert_eq!(&back[..3], &[2, 0xFF, 3]);
        assert!(back[3..].iter().all(|b| *b == 0xFF), "never the older file's 9");
        // The same save the other way: nothing to carry.
        put(Core::GenesisPlusGx, &[2, 0xFF, 3], 0);
        std::fs::File::options().write(true).open(at(Core::Gearsystem)).unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(30)).unwrap();
        assert!(!carry_battery_for(d.path(), "Phantasy Star", Core::GenesisPlusGx, Platform::Gg, None).unwrap());
    }

    #[test]
    fn neo_geo_pocket_flash_carries_between_neopop_and_race() {
        let d = tempfile::tempdir().unwrap();
        // A 2 MiB cart whose ROM is 0x11 everywhere.
        let rom = d.path().join("game.ngc");
        std::fs::write(&rom, vec![0x11u8; 0x200000]).unwrap();
        let put = |core: Core, ext: &str, bytes: &[u8], age_s: u64| {
            let dir = d.path().join(core.text());
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join(format!("Game.{ext}"));
            std::fs::write(&path, bytes).unwrap();
            std::fs::File::options().write(true).open(&path).unwrap()
                .set_modified(SystemTime::now() - Duration::from_secs(age_s)).unwrap();
        };
        // RACE saved the whole 16 KiB boot block at 0x1FC000: three bytes changed.
        let mut block = vec![0x11u8; 0x4000];
        block[4] = 1; block[5] = 2; block[0x3000] = 3;
        put(Core::Race, "ngf", &ngp_file(&[(0x3FC000, block.clone())], true), 1);
        assert!(carry_ngp_flash(d.path(), "Game", Core::Neopop, &rom).unwrap());
        let flash = std::fs::read(d.path().join("Beetle NeoPop/Game.flash")).unwrap();
        let blocks = ngp_blocks(&flash, false).unwrap();
        assert_eq!(blocks, vec![(0x3FC004, vec![1, 2]), (0x3FF000, vec![3])], "only what the game wrote");
        // Back to RACE: NeoPop's ranges, cut at RACE's block ends.
        put(Core::Neopop, "flash", &ngp_file(&[(0x3EFFFE, vec![7; 4])], false), 0);
        std::fs::File::options().write(true).open(d.path().join("RACE/Game.ngf")).unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(30)).unwrap();
        assert!(carry_ngp_flash(d.path(), "Game", Core::Race, &rom).unwrap());
        let ngf = std::fs::read(d.path().join("RACE/Game.ngf")).unwrap();
        assert_eq!(ngp_blocks(&ngf, true).unwrap(), vec![(0x3EFFFE, vec![7; 2]), (0x3F0000, vec![7; 2])]);
        assert!(std::fs::read(d.path().join("RACE/Game.ngf.before-mednafen_ngp")).is_ok(), "RACE's own kept");
    }

    #[test]
    fn gba_carts_never_share_through_this_path() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), Core::Gpsp, &[5; 8], 1);
        assert!(!carry_battery(d.path(), "Pokemon Crystal", Core::Mgba, Platform::Gba).unwrap());
    }

    #[test]
    fn pokedaisy_runs_mgbas_battery_and_shares_it_with_gambatte() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), Core::Mgba, &[1, 1, 1, 1, 0xEE], 60);
        put(d.path(), Core::Gambatte, &[2, 2, 2, 2], 10);
        assert!(carry_battery(d.path(), "Pokemon Crystal", Core::PokeDaisy, Platform::Gbc).unwrap());
        assert_eq!(get(d.path(), Core::Mgba), [2, 2, 2, 2, 0xEE]);
        assert!(!d.path().join("PokeDaisy").exists(), "no battery of its own");
        assert!(!carry_battery(d.path(), "Pokemon Crystal", Core::PokeDaisy, Platform::Gba).unwrap());
    }

    #[test]
    fn a_battery_the_other_core_changed_outdates_this_cores_states() {
        let d = tempfile::tempdir().unwrap();
        let save = d.path().join("Emerald.srm");
        std::fs::write(&save, [1u8; 8]).unwrap();
        assert!(!battery_changed_hands(&save, Core::PokeDaisy), "nobody ran it yet");
        mark_battery_holder(&save, Core::Mgba).unwrap();
        assert!(!battery_changed_hands(&save, Core::PokeDaisy), "mGBA ran it but saved nothing");
        assert!(!battery_changed_hands(&save, Core::Mgba));
        let later = std::time::SystemTime::now() + Duration::from_secs(5);
        std::fs::File::options().write(true).open(&save).unwrap().set_modified(later).unwrap();
        assert!(battery_changed_hands(&save, Core::PokeDaisy), "mGBA saved since");
        assert!(!battery_changed_hands(&save, Core::Mgba), "its own save");
        mark_battery_holder(&save, Core::PokeDaisy).unwrap();
        assert!(!battery_changed_hands(&save, Core::PokeDaisy));
    }
}
