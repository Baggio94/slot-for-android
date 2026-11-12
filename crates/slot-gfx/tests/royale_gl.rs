#![cfg(all(target_os = "linux", feature = "test-support"))]
//! crt-royale on desktop OpenGL (Mesa), as on the host build: sRGB
//! targets need GL_FRAMEBUFFER_SRGB there.
mod royale_common;
use slot_gfx::TestEgl;

#[test]
fn crt_royale_draws_scanlines_on_desktop_gl() {
    royale_common::draws_scanlines(TestEgl::new(), "GL");
}
