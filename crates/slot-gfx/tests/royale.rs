#![cfg(all(target_os = "linux", feature = "test-support"))]
//! crt-royale on a real OpenGL ES 3 context (Mesa), as on Android.
//! (Desktop GL: royale_gl.rs, its own process: the GL flavour is global.)
mod royale_common;
use slot_gfx::TestEgl;

#[test]
fn crt_royale_draws_scanlines_on_es3() {
    royale_common::draws_scanlines(TestEgl::new_es(), "ES 3");
}
