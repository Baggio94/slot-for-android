#![cfg(all(target_os = "linux", feature = "test-support"))]
//! Vignette and Curvature over every home console shader (Mesa).
use slot_gfx::{canvas_size, Compositor, Draw, Fit, ScreenEffect, TestEgl};

/// A frame of plain light grey, B,G,R,X.
fn frame((w, h): (u32, u32)) -> Vec<u8> {
    vec![[200u8, 200, 200, 0]; (w * h) as usize].concat()
}

const HOME: [ScreenEffect; 14] = [ScreenEffect::None, ScreenEffect::SharpBilinear, ScreenEffect::ZfastCrt,
    ScreenEffect::CrtPi, ScreenEffect::CrtGdvMiniUltra, ScreenEffect::CrtGeomDeluxe, ScreenEffect::Fakelottes,
    ScreenEffect::CrtAperture, ScreenEffect::CrtEasymode, ScreenEffect::CrtHyllian, ScreenEffect::Sabr,
    ScreenEffect::SuperEagle, ScreenEffect::XbrLv2, ScreenEffect::CrtRoyale];

#[test]
fn vignette_and_curvature_darken_and_bend_every_home_shader() {
    let surface = TestEgl::new();
    let mut gpu = Compositor::new(&surface).unwrap();
    gpu.fit((960, 640));
    gpu.set_game_fit(Fit::Aspect(4.0 / 3.0));
    let size = (256, 224);
    let (w, h) = canvas_size((960, 640));
    let (w, h) = (w as usize, h as usize);
    let shots = std::env::var("CRT_SHOTS").ok();
    for effect in HOME {
        gpu.set_screen_effect(effect);
        let mut shot = |gpu: &mut Compositor, vignette: f32, curve: f32| {
            gpu.set_crt(vignette, curve);
            for _ in 0..2 {
                gpu.upload_game_sized(&frame(size), size);
                gpu.begin_frame();
                gpu.draw_list(&[Draw::Game]);
            }
            assert_eq!(gpu.gl_error(), 0, "{effect:?}: no GL error");
            let frame = gpu.read_frame();
            if let Some(dir) = &shots {
                std::fs::write(format!("{dir}/{effect:?}-{vignette}-{curve}-{w}x{h}.rgba"), &frame).unwrap();
            }
            frame
        };
        // The 4:3 picture on 3:2: from x = w/2 - 2h/3 to w/2 + 2h/3.
        let (x0, x1) = (w / 2 - 2 * h / 3, w / 2 + 2 * h / 3);
        let luma = |s: &[u8], x: usize, y: usize| {
            let o = (y * w + x) * 4;
            s[o] as u32 + s[o + 1] as u32 + s[o + 2] as u32
        };
        let rows = |s: &[u8], x: usize| (h / 2 - 4..h / 2 + 4).map(|y| luma(s, x, y)).sum::<u32>();
        let plain = shot(&mut gpu, 0.0, 0.0);
        let dark = shot(&mut gpu, -1.0, 0.0);
        let border = shot(&mut gpu, 1.0, 0.0);
        let bent = shot(&mut gpu, 0.0, -1.0);
        let sphere = shot(&mut gpu, 0.0, 0.5);
        // Vignette (GDV, below 0): the picture's edge darker, its centre as
        // it was.
        assert!(rows(&dark, x0 + 2) * 2 < rows(&plain, x0 + 2), "{effect:?}: vignette edge");
        assert!(rows(&dark, w / 2) * 10 >= rows(&plain, w / 2) * 9, "{effect:?}: vignette centre");
        // crt-royale's border (above 0): a dark rim, the picture inside it
        // untouched.
        assert!(rows(&border, x0 + 2) * 2 < rows(&plain, x0 + 2), "{effect:?}: border edge");
        assert_eq!(rows(&border, x0 + 60), rows(&plain, x0 + 60), "{effect:?}: inside the border");
        assert_eq!(rows(&border, w / 2), rows(&plain, w / 2), "{effect:?}: border centre");
        // Curvature: the picture's corner falls outside the tube (black).
        let corner = luma(&bent, x0 + 1, 1);
        assert!(corner < 10, "{effect:?}: curved corner {corner}");
        // crt-royale's sphere (above 0) curves it too.
        let corner = luma(&sphere, x0 + 1, 1);
        assert!(corner < 10, "{effect:?}: sphere corner {corner}");
        // Geom Deluxe rounds its own corners.
        assert!(effect == ScreenEffect::CrtGeomDeluxe || luma(&plain, x0 + 1, 1) > 100, "{effect:?}: flat corner");
        // Outside the picture: black either way.
        assert!(x0 < 3 || luma(&bent, x0 - 2, h / 2) < 10);
        let _ = x1;
    }
}
