//! crt-royale-fast (TroggleMonkey's crt-royale, Hyllian's fast cut) for
//! the NES, Super Nintendo and Mega Drive (Siham 2026-11-11): its eight
//! passes as libretro runs them, flat (no curvature), parameters at the
//! preset's defaults. The passes come from the slang sources through
//! tools/royale/convert.py.
//!
//! The chain draws the whole game frame at the size it takes on screen;
//! the game pass then shows that picture pixel for pixel. It needs OpenGL
//! ES 3 and sRGB render targets: without them the game falls back to
//! zfast-crt.

const PASSES: [&str; 8] = [
    include_str!("royale/pass0.glsl"),
    include_str!("royale/pass1.glsl"),
    include_str!("royale/pass2.glsl"),
    include_str!("royale/pass3.glsl"),
    include_str!("royale/pass4.glsl"),
    include_str!("royale/pass5.glsl"),
    include_str!("royale/pass6.glsl"),
    include_str!("royale/pass7.glsl"),
];

/// The phosphor masks (64x64 RGBA), sampled as the preset says: linear,
/// repeated.
const MASKS: [(&str, &[u8]); 3] = [
    ("mask_grille_texture_small", include_bytes!("royale/mask-grille.rgba")),
    ("mask_slot_texture_small", include_bytes!("royale/mask-slot.rgba")),
    ("mask_shadow_texture_small", include_bytes!("royale/mask-shadow.rgba")),
];
const MASK_SIZE: u32 = 64;

/// The textures a pass may read besides its masks: the previous pass
/// (Source), the game (Original) and the preset's aliases.
const INPUTS: [&str; 7] = ["Source", "Original", "ORIG_LINEARIZED", "VERTICAL_SCANLINES",
    "MASK_RESIZE", "MASKED_SCANLINES", "BRIGHTPASS"];

const SRGB8_ALPHA8: gl::types::GLenum = 0x8C43;
const FRAMEBUFFER_SRGB: gl::types::GLenum = 0x8DB9;

/// How each pass writes: its size from the source and viewport sizes, an
/// sRGB target or not, the alias later passes read it by, and the filter
/// it is read with (the preset's).
type Size = (u32, u32);

#[derive(Copy, Clone)]
struct PassSpec {
    /// From the game frame's size and the picture's.
    size: fn(Size, Size) -> Size,
    srgb: bool,
    alias: Option<&'static str>,
    linear: bool,
}

fn sixteenth(v: u32) -> u32 {
    ((v as f32 * 0.0625).round() as u32).max(1)
}

const SPECS: [PassSpec; 8] = [
    PassSpec { size: |s, _| s, srgb: true, alias: Some("ORIG_LINEARIZED"), linear: true },
    PassSpec { size: |s, v| (s.0, v.1), srgb: true, alias: Some("VERTICAL_SCANLINES"), linear: true },
    PassSpec { size: |_, v| (64, sixteenth(v.1)), srgb: false, alias: None, linear: false },
    PassSpec { size: |_, v| (sixteenth(v.0), sixteenth(v.1)), srgb: false, alias: Some("MASK_RESIZE"), linear: true },
    PassSpec { size: |_, v| v, srgb: true, alias: Some("MASKED_SCANLINES"), linear: true },
    PassSpec { size: |_, v| v, srgb: true, alias: Some("BRIGHTPASS"), linear: true },
    PassSpec { size: |_, v| v, srgb: true, alias: None, linear: true },
    PassSpec { size: |_, v| v, srgb: false, alias: None, linear: false },
];

/// The largest picture the chain draws, per side (a 4K panel's height).
pub(crate) const MAX_VIEW: u32 = 4096;

struct Target {
    tex: gl::types::GLuint,
    fbo: gl::types::GLuint,
    size: (u32, u32),
}

fn texture(w: u32, h: u32, srgb: bool, linear: bool, wrap: gl::types::GLenum,
           pixels: Option<&[u8]>) -> gl::types::GLuint {
    let filter = if linear { gl::LINEAR } else { gl::NEAREST };
    let format = if srgb { SRGB8_ALPHA8 } else { gl::RGBA8 };
    let mut tex = 0;
    unsafe {
        gl::GenTextures(1, &mut tex);
        gl::BindTexture(gl::TEXTURE_2D, tex);
        gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
        gl::TexImage2D(gl::TEXTURE_2D, 0, format as i32, w as i32, h as i32, 0, gl::RGBA,
            gl::UNSIGNED_BYTE, pixels.map_or(std::ptr::null(), |p| p.as_ptr() as *const _));
        for (name, value) in [(gl::TEXTURE_MIN_FILTER, filter), (gl::TEXTURE_MAG_FILTER, filter),
                              (gl::TEXTURE_WRAP_S, wrap), (gl::TEXTURE_WRAP_T, wrap)] {
            gl::TexParameteri(gl::TEXTURE_2D, name, value as i32);
        }
    }
    tex
}

fn target(spec: &PassSpec, size: (u32, u32)) -> Option<Target> {
    let tex = texture(size.0, size.1, spec.srgb, spec.linear, gl::CLAMP_TO_EDGE, None);
    unsafe {
        let mut fbo = 0;
        gl::GenFramebuffers(1, &mut fbo);
        gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
        gl::FramebufferTexture2D(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, tex, 0);
        let target = Target { tex, fbo, size };
        if gl::CheckFramebufferStatus(gl::FRAMEBUFFER) == gl::FRAMEBUFFER_COMPLETE {
            Some(target)
        } else {
            drop_target(&target);
            None
        }
    }
}

fn drop_target(t: &Target) {
    unsafe {
        gl::DeleteFramebuffers(1, &t.fbo);
        gl::DeleteTextures(1, &t.tex);
    }
}

/// A pass's vertex and fragment shaders.
pub(crate) fn sources(pass: usize) -> (String, String) {
    let head = crate::upscale::version_line();
    (format!("{head}#define VERTEX\n{}", PASSES[pass]), format!("{head}{}", PASSES[pass]))
}

struct Pass {
    prog: gl::types::GLuint,
    /// Each texture it reads (INPUTS then MASKS): sampler and size uniforms.
    reads: Vec<(usize, gl::types::GLint, gl::types::GLint)>,
    output_size: gl::types::GLint,
    bloom_size: gl::types::GLint,
    frame_count: gl::types::GLint,
    out: Option<Target>,
    spec: PassSpec,
}

/// The chain: its programs and masks, built once, and targets sized for
/// one game frame size and one picture size.
pub(crate) struct Royale {
    source: (u32, u32),
    view: (u32, u32),
    masks: [gl::types::GLuint; 3],
    passes: Vec<Pass>,
}

impl Royale {
    /// Compiles the passes, or None when this context can't (the caller
    /// has checked for ES 3).
    pub fn new() -> Option<Royale> {
        let masks = MASKS.map(|(_, rgba)|
            texture(MASK_SIZE, MASK_SIZE, false, true, gl::REPEAT, Some(rgba)));
        let mut chain = Royale { source: (0, 0), view: (0, 0), masks, passes: Vec::new() };
        let ok = (0..PASSES.len()).all(|i| chain.add(i));
        unsafe { while gl::GetError() != gl::NO_ERROR {} }
        if ok { Some(chain) } else { chain.release(); None }
    }

    fn add(&mut self, i: usize) -> bool {
        let (vert, frag) = sources(i);
        let Ok(prog) = crate::gl::program(&vert, &frag) else { return false };
        let at = |name: &str| crate::gl::uniform_location(prog, name);
        let names = INPUTS.iter().copied().chain(MASKS.iter().map(|m| m.0));
        let reads = names.enumerate()
            .map(|(k, name)| (k, at(name), at(&format!("{name}Size"))))
            .filter(|&(_, sampler, size)| sampler >= 0 || size >= 0)
            .collect();
        self.passes.push(Pass {
            prog, reads, out: None, spec: SPECS[i],
            output_size: at("OutputSize"),
            bloom_size: at("BLOOM_APPROXSize"),
            frame_count: at("FrameCount"),
        });
        true
    }

    /// Sizes the targets for a `source` frame shown `view` big (a SNES
    /// hi-res screen or a Mega Drive mode switch changes them, not the
    /// programs); false when a target can't be made.
    pub fn size_for(&mut self, source: (u32, u32), view: (u32, u32)) -> bool {
        if self.source == source && self.view == view && self.passes.iter().all(|p| p.out.is_some()) {
            return true;
        }
        let mut previous = 0;
        unsafe { gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut previous) };
        let mut ok = true;
        for pass in &mut self.passes {
            if let Some(old) = pass.out.take() { drop_target(&old); }
            pass.out = target(&pass.spec, (pass.spec.size)(source, view));
            ok &= pass.out.is_some();
        }
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, previous as u32);
            while gl::GetError() != gl::NO_ERROR {}
        }
        self.source = source;
        self.view = view;
        ok
    }

    /// Runs the eight passes on `game` (read nearest, as the preset does);
    /// the finished picture, `view` sized.
    pub fn run(&self, game: gl::types::GLuint, frame: u64, quad: &crate::quad::Quad) -> gl::types::GLuint {
        let mut textures: [(gl::types::GLuint, (u32, u32)); INPUTS.len() + MASKS.len()] =
            [(0, (1, 1)); INPUTS.len() + MASKS.len()];
        textures[0] = (game, self.source);
        textures[1] = (game, self.source);
        for (k, &mask) in self.masks.iter().enumerate() {
            textures[INPUTS.len() + k] = (mask, (MASK_SIZE, MASK_SIZE));
        }
        let size4 = |(w, h): (u32, u32)| [w as f32, h as f32, 1.0 / w as f32, 1.0 / h as f32];
        unsafe {
            let mut fbo = 0;
            let mut viewport = [0i32; 4];
            gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fbo);
            gl::GetIntegerv(gl::VIEWPORT, viewport.as_mut_ptr());
            let blend = gl::IsEnabled(gl::BLEND) == gl::TRUE;
            let scissor = gl::IsEnabled(gl::SCISSOR_TEST) == gl::TRUE;
            gl::Disable(gl::BLEND);
            gl::Disable(gl::SCISSOR_TEST);
            for pass in &self.passes {
                let Some(out) = &pass.out else { break };
                // ES 3 encodes into an sRGB target by itself; desktop GL
                // only when asked.
                if !crate::gl::es() {
                    if pass.spec.srgb { gl::Enable(FRAMEBUFFER_SRGB) } else { gl::Disable(FRAMEBUFFER_SRGB) }
                }
                gl::BindFramebuffer(gl::FRAMEBUFFER, out.fbo);
                gl::Viewport(0, 0, out.size.0 as i32, out.size.1 as i32);
                gl::UseProgram(pass.prog);
                let mut unit = 0;
                for &(k, sampler, size) in &pass.reads {
                    let (tex, dims) = textures[k];
                    if sampler >= 0 {
                        gl::ActiveTexture(gl::TEXTURE0 + unit);
                        gl::BindTexture(gl::TEXTURE_2D, tex);
                        gl::Uniform1i(sampler, unit as i32);
                        unit += 1;
                    }
                    if size >= 0 {
                        gl::Uniform4fv(size, 1, size4(dims).as_ptr());
                    }
                }
                gl::Uniform4fv(pass.output_size, 1, size4(out.size).as_ptr());
                gl::Uniform4fv(pass.bloom_size, 1, size4(out.size).as_ptr());
                // Only its parity matters (interlaced fields).
                gl::Uniform1f(pass.frame_count, (frame % 1_000_000) as f32);
                quad.draw();
                textures[0] = (out.tex, out.size);
                if let Some(alias) = pass.spec.alias {
                    if let Some(k) = INPUTS.iter().position(|n| *n == alias) {
                        textures[k] = (out.tex, out.size);
                    }
                }
            }
            if !crate::gl::es() { gl::Disable(FRAMEBUFFER_SRGB); }
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo as u32);
            gl::Viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
            if blend { gl::Enable(gl::BLEND); }
            if scissor { gl::Enable(gl::SCISSOR_TEST); }
        }
        textures[0].0
    }

    pub fn release(&mut self) {
        for pass in self.passes.drain(..) {
            unsafe { gl::DeleteProgram(pass.prog) };
            if let Some(out) = &pass.out { drop_target(out); }
        }
        unsafe { gl::DeleteTextures(3, self.masks.as_ptr()) };
        self.masks = [0; 3];
    }

    /// The context is gone: nothing to delete.
    pub fn forget(&mut self) {
        self.passes.clear();
        self.masks = [0; 3];
    }
}

/// The picture size the chain draws for a game shown through `window`
/// (its texture window) in a `screen`-sized rect of the frame.
pub(crate) fn view_size(window: [f32; 4], screen: (f32, f32)) -> (u32, u32) {
    let side = |len: f32, part: f32| {
        ((len / part.max(1e-3)).round() as u32).clamp(1, MAX_VIEW)
    };
    (side(screen.0, window[2]), side(screen.1, window[3]))
}

/// The texture window that shows the `view`-sized picture one texel per
/// screen pixel around the same centre as `window` (a picture 597.33
/// pixels wide is drawn 597 wide: rounding it would repeat a column), on
/// whole texels. The power-on animation, which squeezes the picture,
/// keeps `window`.
pub(crate) fn pixel_window(window: [f32; 4], shown: (f32, f32), view: (u32, u32)) -> [f32; 4] {
    let (vw, vh) = (view.0 as f32, view.1 as f32);
    let (w, h) = (shown.0 / vw, shown.1 / vh);
    if (w / window[2] - 1.0).abs() > 0.01 || (h / window[3] - 1.0).abs() > 0.01 {
        return window;
    }
    let x = ((window[0] + window[2] / 2.0 - w / 2.0) * vw).round() / vw;
    let y = ((window[1] + window[3] / 2.0 - h / 2.0) * vh).round() / vh;
    [x, y, w, h]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_passes_are_flat_and_take_slots_quad() {
        for (i, src) in PASSES.iter().enumerate() {
            assert!(src.contains("#ifdef VERTEX") && src.contains("in vec2 a_pos;"), "pass {i}");
            assert!(!src.contains("global.") && !src.contains("params."), "pass {i}: parameters are fixed");
        }
        // Curvature off: the preset's flat geometry mode.
        assert!(PASSES.iter().all(|p| !p.contains("geom_mode")
            || p.contains("#define geom_mode_static (float(0.0))")), "no curvature");
    }

    #[test]
    fn masks_are_64_square_rgba() {
        for (_, rgba) in MASKS {
            assert_eq!(rgba.len(), (MASK_SIZE * MASK_SIZE * 4) as usize);
        }
    }

    #[test]
    fn the_picture_is_the_whole_frame_at_its_screen_size() {
        // SNES at x5 overscale on 1240x1080: the whole frame is 1280x1120.
        let window = crate::pipeline::fit_window(crate::pipeline::Fit::Overscale(256.0 / 224.0),
            (256, 224), (1240.0, 1080.0));
        assert_eq!(view_size(window, (1240.0, 1080.0)), (1280, 1120));
        assert_eq!(view_size([0.0, 0.0, 1.0, 1.0], (960.0, 640.0)), (960, 640));
        // Pass sizes from it.
        assert_eq!((SPECS[1].size)((256, 224), (1280, 1120)), (256, 1120));
        assert_eq!((SPECS[2].size)((256, 224), (1280, 1120)), (64, 70));
        assert_eq!((SPECS[3].size)((256, 224), (1280, 1120)), (80, 70));
    }

    #[test]
    fn the_picture_is_shown_one_texel_per_pixel() {
        // SNES at Integer x2 on 960x640: 597.33 pixels wide, drawn 597.
        let window = crate::pipeline::fit_window(crate::pipeline::Fit::Integer(4.0 / 3.0),
            (256, 224), (960.0, 640.0));
        let view = view_size(window, (960.0, 640.0));
        assert_eq!(view, (597, 448));
        let [x, y, w, h] = pixel_window(window, (960.0, 640.0), view);
        assert!((w * 597.0 - 960.0).abs() < 1e-3 && (h * 448.0 - 640.0).abs() < 1e-3);
        assert!((x * 597.0 - (x * 597.0).round()).abs() < 1e-3);
        assert!((y * 448.0 - (y * 448.0).round()).abs() < 1e-3);
        // Squeezed by the power-on animation: as it was.
        assert_eq!(pixel_window(window, (480.0, 640.0), view), window);
    }
}
