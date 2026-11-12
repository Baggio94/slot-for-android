// slot.'s side of the emulation bench: opens a game with slot.'s own
// GameSession (the same code as the APK, same core options and defaults as a
// fresh install) and runs it frame by frame, through the bench proxy core.
//
// It is a Linux-only, ignored test: it never runs in CI and changes nothing in
// the app. tools/emulation-bench/run.sh drives it, one game at a time:
//
//   BENCH_SLOT_PROXY     bench proxy core (.so)
//   BENCH_SLOT_CORE      mgba gpsp gambatte fceumm nestopia snes9x bsnes
//                        genesis_plus_gx picodrive
//   BENCH_SLOT_PLATFORM  gba gb gbc nes snes md
//   BENCH_SLOT_ROM       the ROM
//   BENCH_SLOT_WORK      an empty work folder
//   BENCH_SLOT_FRAMES    frames to run (default 3600)
//   BENCH_SLOT_BIOS      optional folder copied as slot.'s BIOS folder
//   BENCH_SLOT_REWIND    0 to run without Rewind captures (default: as shipped)
//   BENCH_SLOT_RUNAHEAD  Run-Ahead frames (0-2), as the app runs it solo:
//                        Preemptive Frames. Then the test itself reads the
//                        pad script (BENCH_SLOT_SCRIPT), gives the buttons to
//                        slot. like a player would, and marks each shown
//                        picture in the proxy log (needs BENCH_PAD_EXTERNAL=1)
// plus the proxy's own BENCH_CORE / BENCH_LOG / BENCH_INPUT / ... variables.
use crate::game::GameSession;
use crate::settings::Settings;
use slot_store::{Core, Platform};
use std::path::PathBuf;

fn var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("bench: set {name}"))
}

#[test]
#[ignore = "emulation bench: run through tools/emulation-bench/run.sh"]
fn emulation_bench_slot_run() {
    let proxy = PathBuf::from(var("BENCH_SLOT_PROXY"));
    let core_name = var("BENCH_SLOT_CORE");
    let core = Core::parse(&core_name).unwrap_or_else(|| panic!("bench: unknown core {core_name}"));
    let platform_name = var("BENCH_SLOT_PLATFORM");
    let platform = Platform::parse(&platform_name)
        .unwrap_or_else(|| panic!("bench: unknown platform {platform_name}"));
    let rom = PathBuf::from(var("BENCH_SLOT_ROM"));
    let work = PathBuf::from(var("BENCH_SLOT_WORK"));
    let frames: u64 = std::env::var("BENCH_SLOT_FRAMES").ok()
        .and_then(|v| v.parse().ok()).unwrap_or(3600);

    // slot. recognises its cores by their APK file name.
    let library = work.join("lib");
    std::fs::create_dir_all(&library).unwrap();
    let core_file = library.join(format!("lib{}_libretro.so", core.as_str()));
    std::fs::copy(&proxy, &core_file).unwrap();

    // slot. keeps its ROM cache under a hexadecimal name; the save files
    // use the game's own name, as RetroArch does.
    let ext = rom.extension().and_then(|e| e.to_str()).unwrap_or("bin").to_ascii_lowercase();
    let cache = work.join("rom");
    std::fs::create_dir_all(&cache).unwrap();
    let cached = cache.join(format!("0123456789abcdef.{ext}"));
    std::fs::copy(&rom, &cached).unwrap();
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or("game").to_owned();

    let storage = work.join("storage");
    std::fs::create_dir_all(storage.join("BIOS")).unwrap();
    if let Ok(bios) = std::env::var("BENCH_SLOT_BIOS") {
        for entry in std::fs::read_dir(&bios).unwrap().flatten() {
            std::fs::copy(entry.path(), storage.join("BIOS").join(entry.file_name())).unwrap();
        }
    }

    // A fresh install's settings, exactly as the runtime passes them.
    let settings = Settings::first_install_defaults();
    let palette = settings.gb_palettes.then(|| settings.palette_name());
    let colour = settings.shader_for(platform).core_colour(settings.colour_for(platform));
    let mut game = GameSession::open(&cached, &core_file, &storage, core, platform, false,
        &stem, palette, colour, None).unwrap_or_else(|e| panic!("bench: open failed: {e}"));

    let rewind = match std::env::var("BENCH_SLOT_REWIND").as_deref() {
        Ok("0") => false,
        _ => settings.rewind,
    };
    let runahead: Option<u8> = std::env::var("BENCH_SLOT_RUNAHEAD").ok().and_then(|v| v.parse().ok());
    let script = runahead.map(|_| read_script(&var("BENCH_SLOT_SCRIPT")));
    let hooks = runahead.map(|_| ProxyHooks::find(&core_file));
    let mut audio_frames = 0usize;
    for at in 0..frames {
        if let (Some(n), Some(script), Some(hooks)) = (runahead, &script, &hooks) {
            // Run-Ahead: the buttons go through slot. (Preemptive Frames
            // replays only when they change), the proxy gives the core the
            // same RetroPad bits.
            let mask = script.iter().take_while(|(f, _)| *f <= at).last().map_or(0, |e| e.1);
            unsafe { (hooks.set_pad)(mask) };
            game.set_preemptive_frames(n);
            game.advance(mask as u16, false, rewind);
            unsafe { (hooks.present)() };
        } else {
            // The pad comes from the proxy's script, so both frontends press
            // the same buttons on the same frames.
            game.advance(0, settings.turbo, rewind);
        }
        let audio = game.take_audio();
        audio_frames += audio.len() / 2;
        game.recycle_audio(audio);
    }
    eprintln!("bench: slot. ran {frames} frames, {audio_frames} audio frames");
    drop(game);
}

fn read_script(path: &str) -> Vec<(u64, u32)> {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("bench: {path}: {e}"))
        .lines().filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        }).collect()
}

/// The proxy's own exports, found in the copy slot. loaded.
struct ProxyHooks {
    set_pad: unsafe extern "C" fn(u32),
    present: unsafe extern "C" fn(),
}

impl ProxyHooks {
    fn find(core: &std::path::Path) -> Self {
        use std::ffi::CString;
        let path = CString::new(core.to_str().unwrap()).unwrap();
        unsafe {
            let handle = libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_NOLOAD);
            assert!(!handle.is_null(), "bench: proxy core not loaded");
            let sym = |name: &str| {
                let name = CString::new(name).unwrap();
                let p = libc::dlsym(handle, name.as_ptr());
                assert!(!p.is_null(), "bench: proxy has no {name:?}");
                p
            };
            ProxyHooks {
                set_pad: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(u32)>(sym("bench_set_pad")),
                present: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn()>(sym("bench_present")),
            }
        }
    }
}
