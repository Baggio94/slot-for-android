//! The player's own PICO-8 (Core::Pico8, cores/pico8real): whether it is
//! installed, the HTTP fetches its Splore makes, Splore's cart, and the
//! game saves it shares with fake-08.
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Where BiosLibrary stages the Raspberry Pi build of PICO-8.
pub const DIR: &str = "BIOS/pico-8";
pub const MISSING: &str = "PICO-8 isn't in the BIOS folder.";
/// Splore is part of PICO-8 itself, not a cart: fake-08 has nothing like it.
pub const SPLORE_ONLY: &str = "Splore is part of PICO-8 itself. fake-08 can't run it.";
pub const SPLORE_KEPT: &str = "Splore comes with PICO-8. It can't be deleted.";

/// Splore's label (assets/pico8/splore-label.png): 128x128, PICO-8's palette.
const SPLORE_LABEL: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/pico8/splore-label.png"));

/// Splore as a cart on the PICO-8 shelf, first, while PICO-8 is installed.
/// Its label is written once; Cart Studio may replace it like any other.
pub fn splore_cart(storage: &Path) -> Option<slot_store::Cart> {
    if !installed(storage) { return None; }
    let uri = crate::core_selection::SPLORE_URI;
    let label = storage.join("Labels").join(format!("{}.png", crate::core_selection::key(uri)));
    if !label.exists() {
        let _ = std::fs::create_dir_all(storage.join("Labels"));
        if let Err(err) = slot_store::atomic_write(&label, SPLORE_LABEL) {
            eprintln!("slot-konkr: Splore's label: {err}");
        }
    }
    Some(slot_store::Cart {
        platform: slot_store::Platform::Pico8,
        stem: "Splore".into(),
        title: "Splore".into(),
        code: String::new(),
        rom: uri.into(),
        label: Some(label),
        shell: None,
        battery: false,
    })
}

pub fn installed(storage: &Path) -> bool {
    let dir = storage.join(DIR);
    dir.join("pico8_64").is_file() && dir.join("pico8.dat").is_file()
}

/// Splore's `wget URL -O FILE` (and `--post-file`), answered with ureq.
pub fn http(url: &str, post: Option<&Path>, out: &Path) -> bool {
    match fetch(url, post, out) {
        Ok(()) => true,
        Err(err) => {
            eprintln!("slot-konkr: PICO-8 fetch {url}: {err}");
            let _ = std::fs::remove_file(out);
            false
        }
    }
}

fn fetch(url: &str, post: Option<&Path>, out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();
    let response = match post {
        Some(body) => agent.post(url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send(&std::fs::read(body)?[..])?,
        None => agent.get(url).call()?,
    };
    let partial = out.with_extension("part");
    let mut file = std::fs::File::create(&partial)?;
    std::io::copy(&mut response.into_body().into_reader(), &mut file)?;
    drop(file);
    std::fs::rename(&partial, out)?;
    Ok(())
}

static SPLORE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the game being opened is Splore (the core only sees a cached
/// copy of the cart, under another name): read as the core option pico8_boot.
pub fn set_splore(on: bool) { SPLORE.store(on, std::sync::atomic::Ordering::Relaxed); }
pub fn splore() -> bool { SPLORE.load(std::sync::atomic::Ordering::Relaxed) }

pub fn install_http() {
    slot_retro::set_http_hook(http);
}

/// A cartdata id as PICO-8 takes it: letters, digits and _ (and -), 64 at most.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64
        && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

/// A game's save is shared by both PICO-8 cores: fake-08 keeps it as
/// Saves/fake-08/<game>.p8d.txt, with its cartdata id in <game>.p8key
/// (cores/fake08/zz-slot-game-saves.patch). The player's own PICO-8 keeps
/// its cartdata in its home's cdata/ folder: the game's save is put there
/// before it boots, and brought back when it closes (this bridge dropped,
/// after the core).
pub struct SaveBridge {
    game: PathBuf,
    cdata: PathBuf,
    stem: String,
    since: SystemTime,
}

impl SaveBridge {
    /// `saves`: slot.'s Saves folder; `home`: the player's PICO-8's home
    /// (the core's save folder's pico-8, cores/pico8real/src/libretro.c).
    pub fn open(saves: &Path, home: &Path, stem: &str) -> SaveBridge {
        let game = saves.join(slot_store::Core::Fake08.text());
        let cdata = home.join("cdata");
        let _ = std::fs::create_dir_all(&cdata);
        if let Some(id) = read_id(&game, stem) {
            let target = cdata.join(format!("{id}.p8d.txt"));
            match std::fs::read(game.join(format!("{stem}.p8d.txt"))) {
                Ok(bytes) => if let Err(err) = slot_store::atomic_write(&target, &bytes) {
                    eprintln!("slot-konkr: PICO-8 save of {stem}: {err}");
                },
                // Deleted in slot.: the game starts afresh here too.
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => { let _ = std::fs::remove_file(&target); }
                Err(err) => eprintln!("slot-konkr: PICO-8 save of {stem}: {err}"),
            }
        }
        SaveBridge { game, cdata, stem: stem.to_owned(), since: SystemTime::now() }
    }

    /// The cartdata file PICO-8 wrote last during the game, if any.
    fn written(&self) -> Option<(String, PathBuf)> {
        std::fs::read_dir(&self.cdata).ok()?.flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let id = name.strip_suffix(".p8d.txt")?.to_owned();
                let at = entry.metadata().ok()?.modified().ok()?;
                (valid_id(&id) && at > self.since).then_some((at, id, entry.path()))
            })
            .max_by_key(|(at, _, _)| *at)
            .map(|(_, id, path)| (id, path))
    }

    fn bring_back(&self) -> std::io::Result<()> {
        let Some((id, path)) = self.written() else { return Ok(()) };
        let bytes = std::fs::read(path)?;
        std::fs::create_dir_all(self.game.join("cdata"))?;
        slot_store::atomic_write(&self.game.join(format!("{}.p8d.txt", self.stem)), &bytes)?;
        slot_store::atomic_write(&self.game.join(format!("{}.p8key", self.stem)), id.as_bytes())?;
        // fake-08's shared file of the id, for the game's other carts.
        slot_store::atomic_write(&self.game.join("cdata").join(format!("{id}.p8d.txt")), &bytes)
    }
}

impl Drop for SaveBridge {
    fn drop(&mut self) {
        if let Err(err) = self.bring_back() {
            eprintln!("slot-konkr: PICO-8 save of {}: {err}", self.stem);
        }
    }
}

fn read_id(game: &Path, stem: &str) -> Option<String> {
    let id = std::fs::read_to_string(game.join(format!("{stem}.p8key"))).ok()?;
    let id = id.trim();
    valid_id(id).then(|| id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_save_goes_to_pico8_and_comes_back() {
        let root = std::env::temp_dir().join(format!("slot-pico8-bridge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (saves, home) = (root.join("Saves"), root.join("Saves/PICO-8/pico-8"));
        let game = saves.join("fake-08");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("Jelpi.p8d.txt"), "00000001\n").unwrap();
        std::fs::write(game.join("Jelpi.p8key"), "jelpi_1\n").unwrap();
        let bridge = SaveBridge::open(&saves, &home, "Jelpi");
        assert_eq!(std::fs::read_to_string(home.join("cdata/jelpi_1.p8d.txt")).unwrap(), "00000001\n");
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(home.join("cdata/jelpi_1.p8d.txt"), "00000002\n").unwrap();
        drop(bridge);
        assert_eq!(std::fs::read_to_string(game.join("Jelpi.p8d.txt")).unwrap(), "00000002\n");
        assert_eq!(std::fs::read_to_string(game.join("cdata/jelpi_1.p8d.txt")).unwrap(), "00000002\n");
        // Deleted in slot.: PICO-8's copy goes too.
        std::fs::remove_file(game.join("Jelpi.p8d.txt")).unwrap();
        drop(SaveBridge::open(&saves, &home, "Jelpi"));
        assert!(!home.join("cdata/jelpi_1.p8d.txt").exists());
        assert!(!game.join("Jelpi.p8d.txt").exists(), "nothing written, nothing brought back");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ids_are_pico8s() {
        assert!(valid_id("jelpi_1") && valid_id("celeste-2"));
        assert!(!valid_id("../x") && !valid_id("") && !valid_id(&"a".repeat(65)));
    }
}
