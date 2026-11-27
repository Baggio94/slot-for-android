//! Every settings page has one value per row, on every system and core
//! (3.2.0-beta9 drew Run-Ahead's value under PICO-8's two Emulation rows),
//! and the paused game offers only what its core does (3.2.0-beta11 audit).
use super::*;

const PANELS: [(u32, u32); 6] = [(960, 640), (1920, 1080), (1280, 960), (720, 720), (1240, 1080), (1080, 1920)];

#[test]
fn every_systems_page_has_a_value_per_row() {
    for platform in Platform::ALL {
        for panel in PANELS {
            assert_eq!(Engine::screen_rows_on(platform, panel).len(), Engine::screen_keys_on(platform, panel).len(),
                "{platform:?} Screen on {panel:?}");
            assert_eq!(Engine::system_rows_on(platform, panel).len(), Engine::system_keys_on(platform, panel).len(),
                "{platform:?} Settings > Systems on {panel:?}");
        }
        let (rows, keys) = Engine::system_page(platform);
        assert_eq!(rows.len(), keys.len(), "{platform:?} system page");
        for in_game in [false, true] {
            let (rows, keys) = Engine::extra_rows(platform, in_game);
            assert_eq!(rows.len(), keys.len(), "{platform:?} own options");
        }
        let (rows, keys) = Engine::look_rows(platform);
        assert_eq!(rows.len(), keys.len(), "{platform:?} look rows");
    }
    assert_eq!(INTERFACE_PAGE_ROWS.len(), INTERFACE_PAGE_KEYS.len());
    assert_eq!(ROTATE_ROWS.len(), ROTATE_KEYS.len());
    assert_eq!(GAMEPLAY_ROWS.len(), GAMEPLAY_KEYS.len());
}

#[test]
fn every_paused_games_page_has_a_value_per_row() {
    let cores = Core::ALL.into_iter().map(Some).chain([None]);
    for core in cores {
        for platform in Platform::ALL {
            for (solar, motion) in [(false, false), (true, false), (false, true)] {
                let (rows, keys) = pause_emulation_page(core, platform, solar, motion);
                assert_eq!(rows.len(), keys.len(), "{core:?} {platform:?} Emulation");
            }
            let (rows, keys) = pause_gameplay_page(core, platform);
            assert_eq!(rows.len(), keys.len(), "{core:?} {platform:?} Gameplay");
        }
    }
}

#[test]
fn a_paused_game_offers_only_what_its_core_does() {
    // Run-Ahead
    for core in [Core::Bsnes, Core::PokeDaisy, Core::Pico8, Core::Fake08] {
        assert!(!pause_emulation_page(Some(core), Platform::Snes, false, false).0.contains(&55), "{core:?}");
    }
    assert!(pause_emulation_page(Some(Core::Snes9x), Platform::Snes, false, false).0.contains(&55));
    assert!(!pause_emulation_page(None, Platform::Pico8, false, false).0.contains(&55));
    // Sun and Sensitivity keep their own values.
    let (rows, keys) = pause_emulation_page(Some(Core::Mgba), Platform::Gba, true, false);
    assert_eq!((rows.last(), keys.last()), (Some(&119), Some(&SUN_KEY)));
    let (rows, keys) = pause_emulation_page(Some(Core::Mgba), Platform::Gba, false, true);
    assert_eq!((rows.last(), keys.last()), (Some(&137), Some(&MOTION_KEY)));
    // Rumble: only where a core rumbles.
    for platform in [Platform::Nes, Platform::Snes, Platform::Md, Platform::Sms, Platform::Gg,
        Platform::Ngp, Platform::Ws, Platform::Lynx, Platform::Pico8] {
        assert!(!pause_gameplay_page(None, platform).0.contains(&30), "{platform:?}");
    }
    for platform in [Platform::Gb, Platform::Gbc, Platform::Gba, Platform::Pokemini] {
        assert!(pause_gameplay_page(None, platform).0.contains(&30), "{platform:?}");
    }
    // Rewind: not on the player's own PICO-8.
    assert!(!pause_gameplay_page(Some(Core::Pico8), Platform::Pico8).0.contains(&37));
    assert!(pause_gameplay_page(Some(Core::Fake08), Platform::Pico8).0.contains(&37));
    // Multiplayer: not on bsnes nor Gearsystem.
    assert!(!core_has_multiplayer(Core::Bsnes) && !core_has_multiplayer(Core::Gearsystem));
    for core in [Core::Snes9x, Core::Fceumm, Core::Nestopia, Core::GenesisPlusGx, Core::Picodrive,
        Core::Mgba, Core::Gpsp, Core::Gambatte] {
        assert!(core_has_multiplayer(core), "{core:?}");
    }
    // Cheats: not where the cores apply none.
    for platform in [Platform::Ngp, Platform::Lynx, Platform::Pokemini, Platform::Pico8] {
        assert!(!cheats_offered(platform), "{platform:?}");
        let rows = pause_rows_with(false, false, multiplayer_offered(platform), cheats_offered(platform));
        assert!(!rows.contains(&PauseRow::Cheats) && rows.contains(&PauseRow::SaveEject));
    }
    assert!(pause_rows_with(false, false, true, true).contains(&PauseRow::Cheats));
}
