//! The player's own PICO-8 (Core::Pico8, cores/pico8real): whether it is
//! installed, and the HTTP fetches its Splore makes.
use std::path::Path;
use std::time::Duration;

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

/// Whether the game being opened is Splore.p8 (the core only sees a cached
/// copy of the cart, under another name): read as the core option pico8_boot.
pub fn set_splore(on: bool) { SPLORE.store(on, std::sync::atomic::Ordering::Relaxed); }
pub fn splore() -> bool { SPLORE.load(std::sync::atomic::Ordering::Relaxed) }

pub fn install_http() {
    slot_retro::set_http_hook(http);
}
