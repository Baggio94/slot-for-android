use slot_store::Platform;

#[test]
fn every_platform_has_a_directory() {
    let names: Vec<&str> = Platform::ALL.iter().map(|p| p.dir_name()).collect();
    assert_eq!(names, vec!["GBA", "GB", "GBC", "NES", "SNES", "MD", "SMS", "GG", "NGP", "WS", "LYNX", "POKEMINI", "PICO8"]);
}

#[test]
fn every_platform_is_a_shelf_of_its_own() {
    assert_eq!(Platform::ALL, [Platform::Gba, Platform::Gb, Platform::Gbc,
        Platform::Nes, Platform::Snes, Platform::Md, Platform::Sms, Platform::Gg,
        Platform::Ngp, Platform::Ws, Platform::Lynx, Platform::Pokemini, Platform::Pico8]);
    for (i, p) in Platform::ALL.iter().enumerate() {
        assert!(
            !Platform::ALL[..i].contains(p),
            "{p:?} is named twice in the ring, so two shelves would hold one platform"
        );
    }
}

#[test]
fn each_platform_takes_only_its_own_extensions() {
    assert!(Platform::Gba.accepts(std::path::Path::new("Metroid Fusion.gba")));
    assert!(!Platform::Gba.accepts(std::path::Path::new("Tetris.gb")));
    assert!(Platform::Gb.accepts(std::path::Path::new("Tetris.gb")));
    assert!(Platform::Gb.accepts(std::path::Path::new("Tetris.gbc")));
    assert!(!Platform::Gb.accepts(std::path::Path::new("Metroid Fusion.gba")));
    assert!(Platform::Gba.accepts(std::path::Path::new("Shrek.GBA")));
    assert!(Platform::Nes.accepts(std::path::Path::new("Zelda.nes")));
    assert!(Platform::Nes.accepts(std::path::Path::new("Mario.fc")));
    assert!(Platform::Snes.accepts(std::path::Path::new("Super Metroid.sfc")));
    assert!(Platform::Snes.accepts(std::path::Path::new("Super Metroid.SMC")));
    assert!(Platform::Md.accepts(std::path::Path::new("Sonic.md")));
    assert!(Platform::Md.accepts(std::path::Path::new("Sonic.gen")));
    assert!(!Platform::Snes.accepts(std::path::Path::new("Zelda.nes")));
    assert!(Platform::Sms.accepts(std::path::Path::new("Alex Kidd.sms")));
    assert!(Platform::Gg.accepts(std::path::Path::new("Columns.GG")));
    assert!(!Platform::Gg.accepts(std::path::Path::new("Alex Kidd.sms")));
    assert!(Platform::Ngp.accepts(std::path::Path::new("Metal Slug.ngc")));
    assert!(Platform::Ws.accepts(std::path::Path::new("Final Fantasy.wsc")));
    assert!(!Platform::Ws.accepts(std::path::Path::new("Metal Slug.ngp")));
    assert!(Platform::Lynx.accepts(std::path::Path::new("California Games.lnx")));
    assert!(Platform::Pokemini.accepts(std::path::Path::new("Pokemon Pinball mini.min")));
    assert!(Platform::Pico8.accepts(std::path::Path::new("Celeste.p8.png")));
    assert!(Platform::Pico8.accepts(std::path::Path::new("Jelpi.P8")));
    assert!(!Platform::Pico8.accepts(std::path::Path::new("cover.png")));
    assert_eq!(slot_store::cart_stem("Celeste Classic 2.P8.PNG"), "Celeste Classic 2");
    assert_eq!(slot_store::cart_stem("Sonic (USA).md"), "Sonic (USA)");
}

#[test]
fn only_the_game_boys_draw_into_the_canvas() {
    assert!(Platform::Gba.handheld() && Platform::Gb.handheld() && Platform::Gbc.handheld());
    assert!(!Platform::Nes.handheld() && !Platform::Snes.handheld() && !Platform::Md.handheld());
    assert!(!Platform::Sms.handheld() && !Platform::Gg.handheld());
    assert!(!Platform::Ngp.handheld() && !Platform::Ws.handheld());
    assert!(!Platform::Lynx.handheld() && !Platform::Pokemini.handheld() && !Platform::Pico8.handheld());
    for p in Platform::ALL {
        assert_eq!(Platform::parse(p.dir_name()), Some(p));
    }
}
