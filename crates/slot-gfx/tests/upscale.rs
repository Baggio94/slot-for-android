#![cfg(all(target_os = "linux", feature = "test-support"))]
//! GBA Video upscalers on a real OpenGL ES 3 context (Mesa), as on Android.
use slot_gfx::{canvas_size, Compositor, Draw, LcdEffect, ScreenEffect, TestEgl, Upscaler, SRC_H, SRC_W};

fn frame(seed: u8) -> Vec<u8> {
    // A soft cartoon-like picture: flat areas and an edge, B,G,R,X.
    let mut out = Vec::with_capacity((SRC_W * SRC_H * 4) as usize);
    for y in 0..SRC_H {
        for x in 0..SRC_W {
            let inside = (x as i32 - 120).pow(2) + (y as i32 - 80).pow(2) < 50 * 50;
            let (r, g, b) = if inside { (230u8, 200u8.wrapping_add(seed), 40u8) } else { (30, 60, 160) };
            out.extend_from_slice(&[b, g, r, 0]);
        }
    }
    out
}

#[test]
fn every_upscaler_runs_on_es3_and_only_when_the_picture_changes() {
    let surface = TestEgl::new_es();
    let mut gpu = Compositor::new(&surface).unwrap();
    gpu.fit((960, 640));
    gpu.set_screen_effect(ScreenEffect::Lcd3x);
    for kind in [Upscaler::Bilinear, Upscaler::Fsr, Upscaler::Anime4kFast, Upscaler::Anime4k] {
        gpu.set_upscaler(kind);
        assert_eq!(gpu.upscaler_running(), kind, "{kind:?} runs on ES 3 with half floats");
        let runs = gpu.upscaler_runs();
        gpu.upload_game(&frame(0));
        gpu.begin_frame();
        gpu.draw_list(&[Draw::Game]);
        gpu.upload_game(&frame(0));
        gpu.begin_frame();
        gpu.draw_list(&[Draw::Game]);
        let expected = if kind == Upscaler::Bilinear { 0 } else { 1 };
        assert_eq!(gpu.upscaler_runs() - runs, expected, "{kind:?}: the same picture twice, one run");
        gpu.upload_game(&frame(9));
        gpu.begin_frame();
        gpu.draw_list(&[Draw::Game]);
        assert_eq!(gpu.upscaler_runs() - runs, expected * 2, "{kind:?}: a new picture, a new run");
        let shot = gpu.read_frame();
        // The circle's yellow and the blue around it both reach the screen.
        let (w, h) = canvas_size((960, 640));
        let (w, h) = (w as usize, h as usize);
        let at = |x: usize, y: usize| { let o = (y * w + x) * 4; [shot[o], shot[o + 1], shot[o + 2]] };
        let centre = at(w / 2, h / 2);
        let corner = at(40, 40);
        assert!(centre[0] > 120 && centre[2] < 90, "{kind:?} centre {centre:?}");
        assert!(corner[2] > 80 && corner[0] < 80, "{kind:?} corner {corner:?}");
        assert_eq!(gpu.gl_error(), 0, "{kind:?} leaves no GL error");
    }
    gpu.set_upscaler(Upscaler::Off);
    assert_eq!(gpu.upscaler_running(), Upscaler::Off);
}

#[test]
fn the_lcd_effect_caches_draw_with_and_without_an_upscaler() {
    let surface = TestEgl::new_es();
    let mut gpu = Compositor::new(&surface).unwrap();
    gpu.fit((960, 640));
    for effect in [ScreenEffect::Lcd3x, ScreenEffect::Grid, ScreenEffect::ZfastCrt, ScreenEffect::CrtGdvMiniUltra] {
        gpu.set_screen_effect(effect);
        for lcd in [LcdEffect::PixelTransparency, LcdEffect::SunlightShimmer, LcdEffect::Off] {
            gpu.set_lcd_effect(lcd);
            let mut plain = [0u8; 3];
            for kind in [Upscaler::Off, Upscaler::Fsr] {
                gpu.set_upscaler(kind);
                for seed in [0, 9] {
                    gpu.upload_game(&frame(seed));
                    gpu.begin_frame();
                    gpu.draw_list(&[Draw::Game]);
                }
                let shot = gpu.read_frame();
                let (w, h) = canvas_size((960, 640));
                let o = ((h as usize / 2) * w as usize + w as usize / 2) * 4;
                // The upscaled picture and the LCD Effect caches each keep
                // their own texture unit: the same pixel either way.
                let pixel = [shot[o], shot[o + 1], shot[o + 2]];
                match kind {
                    Upscaler::Off => plain = pixel,
                    _ => assert!(pixel.iter().zip(plain).all(|(a, b)| a.abs_diff(b) <= 2),
                        "{effect:?} {lcd:?}: {pixel:?} upscaled, {plain:?} not"),
                }
                // The circle's yellow, not the blue around it (Grid may
                // darken the pixel under its line).
                assert!(pixel[0] > pixel[2] + 20,
                    "{effect:?} {lcd:?} {kind:?}: the game reaches the screen {pixel:?}");
                assert_eq!(gpu.gl_error(), 0, "{effect:?} {lcd:?} {kind:?} leaves no GL error");
            }
        }
    }
}
