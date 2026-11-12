//! The crt-royale check shared by the ES 3 and desktop GL tests.
use slot_gfx::{canvas_size, Compositor, Draw, Fit, ScreenEffect, TestEgl};

/// A frame: a grey box on blue, B,G,R,X.
fn frame((w, h): (u32, u32)) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let inside = x > w / 4 && x < 3 * w / 4 && y > h / 4 && y < 3 * h / 4;
            let (r, g, b) = if inside { (150u8, 150u8, 150u8) } else { (20, 40, 170) };
            out.extend_from_slice(&[b, g, r, 0]);
        }
    }
    out
}

pub fn draws_scanlines(surface: TestEgl, name: &str) {
    let mut gpu = Compositor::new(&surface).unwrap();
    gpu.fit((960, 640));
    gpu.set_screen_effect(ScreenEffect::CrtRoyale);
    for (size, fit) in [((256, 224), Fit::Integer(4.0 / 3.0)), ((320, 224), Fit::Aspect(4.0 / 3.0)),
                        ((512, 448), Fit::Integer(4.0 / 3.0))] {
        gpu.set_game_fit(fit);
        for _ in 0..2 {
            gpu.upload_game_sized(&frame(size), size);
            gpu.begin_frame();
            gpu.draw_list(&[Draw::Game]);
        }
        assert!(gpu.crt_royale_running(), "{name} {size:?}: crt-royale runs");
        assert_eq!(gpu.gl_error(), 0, "{name} {size:?}: no GL error");
        let shot = gpu.read_frame();
        let (w, h) = canvas_size((960, 640));
        if let Ok(dir) = std::env::var("ROYALE_SHOTS") {
            std::fs::write(format!("{dir}/{name}-{}x{}.rgba", size.0, size.1), &shot).unwrap();
        }
        let (w, h) = (w as usize, h as usize);
        let luma = |x: usize, y: usize| {
            let o = (y * w + x) * 4;
            shot[o] as u32 + shot[o + 1] as u32 + shot[o + 2] as u32
        };
        // The grey box is lit, with scanlines: dark and bright rows
        // down the middle column.
        let column: Vec<u32> = (h / 2 - 12..h / 2 + 12).map(|y| luma(w / 2, y)).collect();
        let (lo, hi) = (*column.iter().min().unwrap(), *column.iter().max().unwrap());
        assert!(hi > 250, "{name} {size:?}: the box is lit ({hi})");
        assert!(lo * 10 < hi * 9, "{name} {size:?}: scanlines ({lo}..{hi})");
        // The blue around it stays blue.
        let o = ((h / 2) * w + w / 2 - w / 4 - 30) * 4;
        assert!(shot[o + 2] > shot[o], "{name} {size:?}: blue border {:?}", &shot[o..o + 3]);
    }
    // Another shader lets crt-royale go.
    gpu.set_screen_effect(ScreenEffect::ZfastCrt);
    gpu.begin_frame();
    gpu.draw_list(&[Draw::Game]);
    assert!(!gpu.crt_royale_running());
    assert_eq!(gpu.gl_error(), 0);
}
