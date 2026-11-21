use std::path::PathBuf;
use slot_store::{Cart, ShellChoice, ShellFinish};
use slot_ui::{density, CartFace, HomeMould};

fn cart(m: HomeMould, colour: [u8; 3], finish: ShellFinish) -> Cart {
    Cart { platform: m.platform(), stem: String::new(), title: String::new(),
        code: String::new(), rom: PathBuf::from("material-check.rom"), label: None,
        shell: Some(ShellChoice { outline: m.outline(), colour, finish }), battery: false }
}
fn front(cart: &Cart) -> CartFace {
    let (w, h) = slot_ui::label_size(cart);
    slot_ui::cart_face_with(cart, Some(vec![0; (density::px(w) * density::px(h) * 4) as usize]))
}
fn pixel(face: &CartFace, x: u32, y: u32) -> &[u8] {
    let i = ((density::px(y) * density::px(face.w) + density::px(x)) * 4) as usize;
    &face.rgba[i..i + 4]
}

#[test]
fn new_moulds_keep_registration_and_take_light_dark_and_saturated_colours() {
    use HomeMould::*;
    let cases = [
        (NeoGeoPocket, 280, (31,66,252,138), (156,230)),
        (WonderSwan, 188, (10,26,293,108), (156,140)),
        (WonderSwanColor, 188, (10,26,293,108), (156,140)),
        (GameGear, 316, (28,58,256,185), (156,282)),
        (MasterSystem, 200, (8,4,296,42), (156,150)),
        (Lynx, 326, (5,69,302,211), (156,40)),
        (PokemonMini, 300, (19,107,274,149), (156,275)),
        (MasterSystemJp, 360, (29,2,254,209), (156,300)),
        (MasterSystemCard, 500, (18,21,279,290), (156,320)),
        (GameGearCodemasters, 367, (41,17,231,295), (156,340)),
    ];
    for scale in [1.0, 4.0/3.0, 3.0] {
        density::with_scale(scale, || for (m, h, label, (x,y)) in cases {
            let black = cart(m, [18; 3], ShellFinish::Solid);
            assert_eq!(slot_ui::label_rect(&black), label, "{m:?}");
            assert_eq!(slot_ui::cart_box_for(&black), (312,h));
            let dark = front(&black);
            let white = front(&cart(m, [240; 3], ShellFinish::Solid));
            let red = front(&cart(m, [210,25,35], ShellFinish::Solid));
            assert_eq!(dark.rgba.len(), (density::px(312)*density::px(h)*4) as usize);
            assert!(pixel(&white,x,y)[0] > pixel(&dark,x,y)[0].saturating_add(100), "{m:?}: flat plastic must take white");
            let p = pixel(&red,x,y);
            assert!(p[0] > p[1].saturating_add(70), "{m:?}: saturated red must not stay black/grey: {p:?}");
            assert!(dark.rgba.chunks_exact(4).zip(white.rgba.chunks_exact(4)).all(|(a,b)| a[3] == b[3]), "{m:?}: colour must not change silhouette");
        });
    }
}

#[test]
fn mini_clear_cap_and_body_reveal_the_same_board_without_fading_the_sticker() {
    let clear = cart(HomeMould::PokemonMini, [128,150,178], ShellFinish::Clear);
    let solid = cart(HomeMould::PokemonMini, [128,150,178], ShellFinish::Solid);
    let glass = front(&clear);
    let plastic = front(&solid);
    for (x,y) in [(156,40), (156,275)] {
        assert!((120..195).contains(&pixel(&glass,x,y)[3]));
        assert_eq!(pixel(&plastic,x,y)[3], 255);
    }
    assert_eq!(slot_ui::cart_backing(&clear).rgba, slot_ui::cart_backing(&solid).rgba);
    let (w,h) = slot_ui::label_size(&clear);
    let label = [240,80,140,255].repeat((w*h) as usize);
    let face = slot_ui::cart_face_with(&clear, Some(label));
    assert_eq!(pixel(&face,156,170), &[240,80,140,255]);
}

#[test]
fn clipped_label_corners_have_no_ink_visible_through_clear_plastic() {
    for m in [HomeMould::GameGear, HomeMould::GameGearCodemasters, HomeMould::PokemonMini] {
        let c = cart(m, [124,150,178], ShellFinish::Clear);
        let (lx,ly,w,h) = slot_ui::label_rect(&c);
        let a = slot_ui::cart_face_with(&c, Some([255,0,255,255].repeat((w*h) as usize)));
        let b = slot_ui::cart_face_with(&c, Some([0,255,0,255].repeat((w*h) as usize)));
        let y = if m == HomeMould::PokemonMini { ly+h-2 } else { ly };
        assert_eq!(pixel(&a,lx,y), pixel(&b,lx,y), "{m:?}: no sticker ink outside the cut");
        assert!(pixel(&a,lx,y)[3] < 220);
        assert_eq!(pixel(&a,lx+w/2,ly+h/2)[3], 255);
    }
}

#[test]
fn contacts_stay_fixed_and_glitter_is_repeatable() {
    for (m,x,y) in [(HomeMould::WonderSwan,156,168), (HomeMould::WonderSwanColor,156,168),
        (HomeMould::Lynx,156,300), (HomeMould::MasterSystemCard,156,430)] {
        let red = front(&cart(m,[210,25,35],ShellFinish::Solid));
        let c = cart(m,[124,79,184],ShellFinish::ClearGlitter);
        let glitter = front(&c);
        assert_eq!(pixel(&red,x,y),pixel(&glitter,x,y), "{m:?}: contacts are not plastic");
        assert_eq!(glitter.rgba,front(&c).rgba,"{m:?}: no animated glitter randomness");
    }
}

#[test]
fn codemasters_top_has_no_scan_cracks_inside_its_existing_mould() {
    let face = front(&cart(HomeMould::GameGearCodemasters,[240;3],ShellFinish::Solid));
    for x in 70..242 {
        for y in 5..12 { assert_eq!(pixel(&face,x,y)[3],255,"hole at {x},{y}"); }
    }
    assert_eq!(pixel(&face,0,0)[3],0);
}

#[test]
fn fractional_density_leaves_no_opaque_unprinted_strip_beside_a_clear_label() {
    for scale in [1.0, 4.0/3.0, 1.5, 3.0] {
        density::with_scale(scale, || {
            let c = cart(HomeMould::GameGear,[124,150,178],ShellFinish::Clear);
            let (lx,ly,w,h) = slot_ui::label_rect(&c);
            let face = slot_ui::cart_face_with(&c, Some([240,80,140,255].repeat((density::px(w)*density::px(h)) as usize)));
            let bare = front(&c);
            for (x,y) in [(density::px(lx)+density::px(w), density::px(ly+h/2)),
                (density::px(lx+w/2), density::px(ly)+density::px(h))] {
                let a = ((y*density::px(face.w)+x)*4+3) as usize;
                assert_eq!(face.rgba[a],bare.rgba[a],"density {scale}: opaque strip at {x},{y}");
            }
        });
    }
}

#[test]
fn white_plastic_left_in_decals_takes_the_cart_colour() {
    // WonderSwan guide tabs (photographed white) and My Card's window rim
    // must not stay light on a black cart.
    for (m, x, y) in [(HomeMould::WonderSwan, 6u32, 160u32), (HomeMould::WonderSwanColor, 306, 160),
        (HomeMould::MasterSystemCard, 18, 332)] {
        let black = front(&cart(m, [18; 3], ShellFinish::Solid));
        let face_w = density::px(black.w);
        let (x0, x1) = (density::px(x.saturating_sub(4)), density::px(x + 4));
        let (y0, y1) = (density::px(y - 4), density::px(y + 4));
        for yy in y0..y1 {
            for xx in x0..x1.min(face_w) {
                let i = ((yy * face_w + xx) * 4) as usize;
                let p = &black.rgba[i..i + 4];
                if p[3] < 128 { continue; }
                let l = (p[0] as u32 + p[1] as u32 + p[2] as u32) / 3;
                assert!(l < 110, "{m:?}: light residue {p:?} at {xx},{yy}");
            }
        }
    }
}

#[test]
fn japanese_master_system_body_is_one_flat_plastic_in_white() {
    let white = front(&cart(HomeMould::MasterSystemJp, [240; 3], ShellFinish::Solid));
    let frame = pixel(&white, 20, 100)[0] as i32;
    for (x, y) in [(30, 230), (156, 300), (280, 340), (156, 220)] {
        let v = pixel(&white, x, y)[0] as i32;
        assert!((v - frame).abs() <= 6, "lower body {v} vs frame {frame} at {x},{y}");
    }
}
