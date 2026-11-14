//! The player's own PICO-8 (Core::Pico8, cores/pico8real): whether it is
//! installed, and the HTTP fetches its Splore makes.
use std::path::Path;
use std::time::Duration;

/// Where BiosLibrary stages the Raspberry Pi build of PICO-8.
pub const DIR: &str = "BIOS/pico-8";
pub const MISSING: &str = "PICO-8 isn't in the BIOS folder.";

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

pub fn install_http() {
    slot_retro::set_http_hook(http);
}
