//! LCD Effect caches (Pixel Transparency / Sunlight Shimmer v3.0 by Matt
//! Akins, adapted with his permission). As in the original, the drop
//! shadow, the light and the paper grain are worked out ahead in small
//! offscreen textures that the game pass only reads: the shadow once per
//! game pixel and frame instead of nine reads per screen pixel, the light
//! once when it moves, the grain once per picture size.

use crate::lcd3x::{LcdEffect, ScreenEffect};
use crate::quad::Quad;
use crate::shaders::{pt_frag, COPY_FRAG, GDV_GLOW_FRAG, GRAIN_FRAG, LIGHT_CACHE_FRAG, PT_CACHE_FRAG, RECT_VERT};

/// Game pixels kept around the game in the shadow cache: the shadow's
/// offset (about one game pixel) and blur (under one) never reach further.
/// Past it, every tap is screen edge and the game pass reads 1.
pub const PT_PAD: u32 = 4;

/// Texture units the game pass reads the caches from (0 to 2 are the
/// game, the paper and the Dot Matrix background, 3 the GBA Video
/// upscaler's picture; OpenGL ES 2 promises 8).
pub const SHADOW_UNIT: u32 = 4;
pub const LIGHT_UNIT: u32 = 5;
pub const GRAIN_UNIT: u32 = 6;
/// The game-sized cache the screen effect reads, if any: the game
/// bilinear filtered (a copy the GPU filters itself), or CRT GDV Mini
/// Ultra's glow, one texel a game pixel. No effect reads both.
pub const SOURCE_UNIT: u32 = 7;
/// The game in linear light (crt-aperture, CRT Geom Deluxe, which also
/// reads the bilinear copy on SOURCE_UNIT). Half floats only, so OpenGL ES 3
/// and its 16 units; without them the game pass works it out itself.
pub const GAMMA_UNIT: u32 = 8;

/// Screen effects that read the game in linear light, once per game pixel.
pub fn reads_gamma(effect: ScreenEffect) -> bool {
    matches!(effect, ScreenEffect::CrtAperture | ScreenEffect::CrtGeomDeluxe)
}

/// RGBA16F and HALF_FLOAT (OpenGL ES 3 / OpenGL 3.3).
const RGBA16F: gl::types::GLenum = 0x881A;
const HALF_FLOAT: gl::types::GLenum = 0x140B;

fn units_past_eight() -> bool {
    let (mut frag, mut all) = (0, 0);
    unsafe {
        gl::GetIntegerv(gl::MAX_TEXTURE_IMAGE_UNITS, &mut frag);
        gl::GetIntegerv(gl::MAX_COMBINED_TEXTURE_IMAGE_UNITS, &mut all);
    }
    frag > GAMMA_UNIT as i32 && all > GAMMA_UNIT as i32
}

/// Screen effects that read the game bilinear filtered.
pub fn reads_linear(effect: ScreenEffect) -> bool {
    matches!(effect, ScreenEffect::SharpBilinear | ScreenEffect::ZfastCrt | ScreenEffect::CrtPi
        | ScreenEffect::Fakelottes | ScreenEffect::CrtGeomDeluxe)
}

/// The light cache's size, as the original's.
const LIGHT_SIZE: u32 = 256;

/// An offscreen texture and the program that fills it.
struct Target {
    prog: gl::types::GLuint,
    tex: gl::types::GLuint,
    fbo: gl::types::GLuint,
    size: (u32, u32),
    filter: gl::types::GLenum,
    half: bool,
}

impl Target {
    fn new(frag: &str, filter: gl::types::GLenum) -> Option<Self> {
        Self::with_program(&pt_frag(frag), filter, false)
    }

    /// `frag` complete, with its own precision; `half` for an RGBA16F
    /// texture instead of RGBA8.
    fn with_program(frag: &str, filter: gl::types::GLenum, half: bool) -> Option<Self> {
        let prog = crate::shaders::program(RECT_VERT, frag).ok()?;
        let mut fbo = 0;
        unsafe { gl::GenFramebuffers(1, &mut fbo) };
        Some(Target { prog, tex: 0, fbo, size: (0, 0), filter, half })
    }

    /// Make the texture `w` x `h`; false if it cannot be drawn to.
    fn resize(&mut self, w: u32, h: u32) -> bool {
        if self.size == (w, h) && self.tex != 0 {
            return true;
        }
        let tex = if self.half {
            half_texture(w, h, self.filter)
        } else {
            crate::gl::texture(w, h, self.filter, gl::CLAMP_TO_EDGE, gl::RGBA, None)
        };
        unsafe {
            if self.tex != 0 {
                gl::DeleteTextures(1, &self.tex);
            }
            self.tex = tex;
            self.size = (w, h);
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
            gl::FramebufferTexture2D(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, tex, 0);
            gl::CheckFramebufferStatus(gl::FRAMEBUFFER) == gl::FRAMEBUFFER_COMPLETE
        }
    }

    fn at(&self, name: &str) -> gl::types::GLint {
        crate::gl::uniform_location(self.prog, name)
    }

    /// Fill the whole texture. The caller saves and restores the target.
    unsafe fn fill(&self, quad: &Quad, uniforms: impl FnOnce(&Self)) {
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
        gl::Viewport(0, 0, self.size.0 as i32, self.size.1 as i32);
        gl::UseProgram(self.prog);
        gl::Uniform4f(self.at("u_rect"), 0.0, 0.0, w, h);
        gl::Uniform2f(self.at("u_target"), w, h);
        uniforms(self);
        quad.draw();
    }

    fn delete(&mut self) {
        unsafe {
            gl::DeleteProgram(self.prog);
            gl::DeleteFramebuffers(1, &self.fbo);
            if self.tex != 0 {
                gl::DeleteTextures(1, &self.tex);
            }
        }
    }
}

fn half_texture(w: u32, h: u32, filter: gl::types::GLenum) -> gl::types::GLuint {
    unsafe {
        let mut tex = 0;
        gl::GenTextures(1, &mut tex);
        gl::BindTexture(gl::TEXTURE_2D, tex);
        gl::TexImage2D(gl::TEXTURE_2D, 0, RGBA16F as i32, w as i32, h as i32, 0,
            gl::RGBA, HALF_FLOAT, std::ptr::null());
        for (name, value) in [(gl::TEXTURE_MIN_FILTER, filter), (gl::TEXTURE_MAG_FILTER, filter),
                              (gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE), (gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE)] {
            gl::TexParameteri(gl::TEXTURE_2D, name, value as i32);
        }
        tex
    }
}

/// A game-sized cache drawn by the game shader itself (CACHE_PASS), one
/// texel a game pixel, so it works out exactly what the game pass would.
#[derive(Default)]
struct GameCache {
    target: Option<Target>,
    key: Option<SourceKey>,
    /// This context can't draw it: the game pass works it out itself.
    failed: bool,
}

impl GameCache {
    fn tex(&self) -> gl::types::GLuint {
        self.target.as_ref().map_or(0, |t| t.tex)
    }

    /// Drawn from the frame `key`?
    fn ready(&self, key: SourceKey) -> bool {
        !self.failed && self.key == Some(key) && self.tex() != 0
    }

    unsafe fn draw(&mut self, effect: ScreenEffect, half: bool, quad: &Quad, input: &CacheInput, key: SourceKey) {
        if self.failed {
            return;
        }
        if self.target.is_none() {
            if half && !(crate::upscale::gl_major() >= 3 && units_past_eight()) {
                self.failed = true;
                return;
            }
            self.target = Target::with_program(&crate::pipeline::cache_frag(effect), gl::NEAREST, half);
            if self.target.is_none() {
                self.failed = true;
                return;
            }
        }
        let Some(target) = self.target.as_mut() else { return };
        if !target.resize(input.size.0, input.size.1) {
            target.delete();
            self.target = None;
            self.failed = true;
            // A failed half-float target must not leave an error behind.
            while gl::GetError() != gl::NO_ERROR {}
            return;
        }
        gl::ActiveTexture(gl::TEXTURE0);
        gl::BindTexture(gl::TEXTURE_2D, input.tex);
        target.fill(quad, |t| {
            gl::Uniform1i(t.at("u_game"), 0);
            gl::Uniform2f(t.at("u_src"), input.size.0 as f32, input.size.1 as f32);
        });
        self.key = Some(key);
    }
}

/// What the game pass draws this time, as the caches see it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CacheInput {
    pub effect: ScreenEffect,
    pub lcd: LcdEffect,
    /// The game texture (or a still) and its size.
    pub tex: gl::types::GLuint,
    pub size: (u32, u32),
    /// Counts game frames uploaded into `tex`.
    pub serial: u64,
    pub pic: [f32; 4],
    /// The game texture's part drawn (u_uv).
    pub src: [f32; 4],
    /// The picture's size on screen, in pixels (u_rect.zw * u_fbo).
    pub out: (f32, f32),
    pub light: [f32; 2],
}

/// What the game pass needs to read the caches.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct CacheReadout {
    /// Shadow cache padding (cells) and size (texels).
    pub geo: [f32; 4],
    /// Shadow blur radius, in game pixels.
    pub radius: [f32; 2],
    /// The screen effect's game-sized cache (corner flags or linear light)
    /// is drawn for this frame.
    pub cached: bool,
}

/// The shadow blur's radius in game pixels: 2 reference pixels at 640x480,
/// scaled with the picture, as the original's.
pub fn shadow_radius(input: &CacheInput) -> [f32; 2] {
    let (ox, oy) = (input.out.0.max(1.0), input.out.1.max(1.0));
    let scale = (ox / 640.0 * oy / 480.0).sqrt();
    [
        2.0 * scale / ox * input.src[2] * input.size.0 as f32,
        2.0 * scale / oy * input.src[3] * input.size.1 as f32,
    ]
}

/// What the shadow cache was drawn from: texture, size, frame, picture,
/// blur radius.
type ShadowKey = (gl::types::GLuint, (u32, u32), u64, [f32; 4], [f32; 2]);

#[derive(Default)]
pub struct LcdCaches {
    context_lost: bool,
    shadow: Option<Target>,
    light: Option<Target>,
    grain: Option<Target>,
    shadow_key: Option<ShadowKey>,
    light_key: Option<([f32; 2], f32)>,
    grain_key: Option<(u32, u32)>,
    readout: CacheReadout,
    /// Game-sized caches for the home console shaders.
    linear: Option<Target>,
    glow: Option<Target>,
    linear_key: Option<SourceKey>,
    glow_key: Option<SourceKey>,
    xbr: GameCache,
    sabr: GameCache,
    gamma: GameCache,
}

/// What a game-sized cache was drawn from: texture, size, frame.
type SourceKey = (gl::types::GLuint, (u32, u32), u64);

impl LcdCaches {
    pub fn abandon_context(&mut self) {
        self.context_lost = true;
    }

    /// Bring the caches up to date for `input`, drawing only those whose
    /// inputs changed. `force_shadow` redraws the shadow even so (a still
    /// may be redrawn into the same texture).
    pub fn update(&mut self, quad: &Quad, input: &CacheInput, force_shadow: bool) -> CacheReadout {
        let lcd = input.lcd != LcdEffect::Off;
        let radius = shadow_radius(input);
        let shadow_key = (input.tex, input.size, input.serial, input.pic, radius);
        let shadow = lcd && (force_shadow || self.shadow_key != Some(shadow_key));
        let aspect = input.out.0.max(1.0) / input.out.1.max(1.0);
        let light_key = (input.light, aspect);
        let light = input.lcd == LcdEffect::SunlightShimmer && self.light_key != Some(light_key);
        let grain_size = (input.out.0.round().max(1.0) as u32, input.out.1.round().max(1.0) as u32);
        let grain = lcd && self.grain_key != Some(grain_size);
        let source_key = (input.tex, input.size, input.serial);
        let linear = reads_linear(input.effect) && (force_shadow || self.linear_key != Some(source_key));
        let glow = input.effect == ScreenEffect::CrtGdvMiniUltra
            && (force_shadow || self.glow_key != Some(source_key));
        let game = self.game_cache(input.effect)
            .is_some_and(|(c, _)| !c.failed && (force_shadow || c.key != Some(source_key)));
        if !(shadow || light || grain || linear || glow || game) {
            self.readout.cached = self.game_ready(input.effect, source_key);
            return self.readout;
        }
        unsafe {
            let mut prev_fbo = 0;
            let mut viewport = [0; 4];
            gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut prev_fbo);
            gl::GetIntegerv(gl::VIEWPORT, viewport.as_mut_ptr());
            let blend = gl::IsEnabled(gl::BLEND) == gl::TRUE;
            gl::Disable(gl::BLEND);
            if shadow {
                self.draw_shadow(quad, input, radius);
                self.shadow_key = Some(shadow_key);
            }
            if light {
                self.draw_light(quad, input, aspect);
                self.light_key = Some(light_key);
            }
            if grain {
                self.draw_grain(quad, grain_size);
                self.grain_key = Some(grain_size);
            }
            if linear {
                Self::draw_source(&mut self.linear, COPY_FRAG, gl::LINEAR, quad, input);
                self.linear_key = Some(source_key);
            }
            if glow {
                Self::draw_source(&mut self.glow, GDV_GLOW_FRAG, gl::NEAREST, quad, input);
                self.glow_key = Some(source_key);
            }
            if let Some((cache, half)) = self.game_cache(input.effect).filter(|_| game) {
                cache.draw(input.effect, half, quad, input, source_key);
            }
            gl::BindFramebuffer(gl::FRAMEBUFFER, prev_fbo as gl::types::GLuint);
            gl::Viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
            if blend {
                gl::Enable(gl::BLEND);
            }
            gl::ActiveTexture(gl::TEXTURE0);
        }
        self.readout.cached = self.game_ready(input.effect, source_key);
        self.readout
    }

    fn game_ready(&mut self, effect: ScreenEffect, key: SourceKey) -> bool {
        self.game_cache(effect).is_some_and(|(c, _)| c.ready(key))
    }

    /// The game-sized cache `effect` reads, and whether it is half float.
    fn game_cache(&mut self, effect: ScreenEffect) -> Option<(&mut GameCache, bool)> {
        match effect {
            ScreenEffect::XbrLv2 => Some((&mut self.xbr, false)),
            ScreenEffect::Sabr => Some((&mut self.sabr, false)),
            e if reads_gamma(e) => Some((&mut self.gamma, true)),
            _ => None,
        }
    }

    unsafe fn draw_shadow(&mut self, quad: &Quad, input: &CacheInput, radius: [f32; 2]) {
        if self.shadow.is_none() {
            self.shadow = Target::new(PT_CACHE_FRAG, gl::NEAREST);
        }
        let Some(target) = self.shadow.as_mut() else { return };
        let (w, h) = (3 * (input.size.0 + 2 * PT_PAD), input.size.1 + 2 * PT_PAD);
        if !target.resize(w, h) {
            return;
        }
        gl::ActiveTexture(gl::TEXTURE0);
        gl::BindTexture(gl::TEXTURE_2D, input.tex);
        target.fill(quad, |t| {
            gl::Uniform1i(t.at("u_game"), 0);
            gl::Uniform2f(t.at("u_src"), input.size.0 as f32, input.size.1 as f32);
            let p = input.pic;
            gl::Uniform4f(t.at("u_pic"), p[0], p[1], p[2], p[3]);
            gl::Uniform2f(t.at("u_pt_r"), radius[0], radius[1]);
            gl::Uniform1f(t.at("u_pad"), PT_PAD as f32);
        });
        self.readout = CacheReadout {
            geo: [PT_PAD as f32, PT_PAD as f32, w as f32, h as f32],
            radius,
            cached: false,
        };
    }

    /// A game-sized cache drawn from the game texture.
    unsafe fn draw_source(slot: &mut Option<Target>, frag: &str, filter: gl::types::GLenum,
                          quad: &Quad, input: &CacheInput) {
        if slot.is_none() {
            *slot = Target::new(frag, filter);
        }
        let Some(target) = slot.as_mut() else { return };
        if !target.resize(input.size.0, input.size.1) {
            return;
        }
        gl::ActiveTexture(gl::TEXTURE0);
        gl::BindTexture(gl::TEXTURE_2D, input.tex);
        target.fill(quad, |t| {
            gl::Uniform1i(t.at("u_game"), 0);
            gl::Uniform2f(t.at("u_src"), input.size.0 as f32, input.size.1 as f32);
        });
    }

    unsafe fn draw_light(&mut self, quad: &Quad, input: &CacheInput, aspect: f32) {
        if self.light.is_none() {
            self.light = Target::new(LIGHT_CACHE_FRAG, gl::LINEAR);
        }
        let Some(target) = self.light.as_mut() else { return };
        if !target.resize(LIGHT_SIZE, LIGHT_SIZE) {
            return;
        }
        target.fill(quad, |t| {
            gl::Uniform2f(t.at("u_light"), input.light[0], input.light[1]);
            gl::Uniform1f(t.at("u_aspect"), aspect);
        });
    }

    unsafe fn draw_grain(&mut self, quad: &Quad, size: (u32, u32)) {
        if self.grain.is_none() {
            self.grain = Target::new(GRAIN_FRAG, gl::NEAREST);
        }
        let Some(target) = self.grain.as_mut() else { return };
        if !target.resize(size.0, size.1) {
            return;
        }
        target.fill(quad, |t| {
            gl::Uniform2f(t.at("u_size"), size.0 as f32, size.1 as f32);
        });
    }

    /// Bind the caches for the game pass, drawing `effect`.
    pub fn bind(&self, effect: ScreenEffect) {
        let tex = |t: &Option<Target>| t.as_ref().map_or(0, |t| t.tex);
        let source = match effect {
            ScreenEffect::CrtGdvMiniUltra => tex(&self.glow),
            ScreenEffect::XbrLv2 => self.xbr.tex(),
            ScreenEffect::Sabr => self.sabr.tex(),
            _ => tex(&self.linear),
        };
        unsafe {
            gl::ActiveTexture(gl::TEXTURE0 + SOURCE_UNIT);
            gl::BindTexture(gl::TEXTURE_2D, source);
            // Only there with 16 units (OpenGL ES 3).
            if reads_gamma(effect) && self.gamma.tex() != 0 {
                gl::ActiveTexture(gl::TEXTURE0 + GAMMA_UNIT);
                gl::BindTexture(gl::TEXTURE_2D, self.gamma.tex());
            }
            gl::ActiveTexture(gl::TEXTURE0 + SHADOW_UNIT);
            gl::BindTexture(gl::TEXTURE_2D, tex(&self.shadow));
            gl::ActiveTexture(gl::TEXTURE0 + LIGHT_UNIT);
            gl::BindTexture(gl::TEXTURE_2D, tex(&self.light));
            gl::ActiveTexture(gl::TEXTURE0 + GRAIN_UNIT);
            gl::BindTexture(gl::TEXTURE_2D, tex(&self.grain));
            gl::ActiveTexture(gl::TEXTURE0);
        }
    }
}

impl Drop for LcdCaches {
    fn drop(&mut self) {
        if self.context_lost {
            return;
        }
        for target in [&mut self.shadow, &mut self.light, &mut self.grain, &mut self.linear, &mut self.glow,
                       &mut self.xbr.target, &mut self.sabr.target, &mut self.gamma.target]
            .into_iter().flatten()
        {
            target.delete();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(out: (f32, f32), size: (u32, u32), src: [f32; 4]) -> CacheInput {
        CacheInput {
            effect: ScreenEffect::Lcd3x,
            lcd: LcdEffect::PixelTransparency,
            tex: 1,
            size,
            serial: 0,
            pic: [0.0, 0.0, 1.0, 1.0],
            src,
            out,
            light: [0.5, 0.5],
        }
    }

    #[test]
    fn the_shadow_blur_stays_inside_the_cache_padding() {
        // Whole GB/GBA canvas on the KPA, the RPC and a 4:3 panel, and a
        // Game Boy picture (160x144 of the canvas).
        for out in [(960.0, 640.0), (1620.0, 1080.0), (1440.0, 1080.0), (1200.0, 1080.0)] {
            for src in [[0.0, 0.0, 1.0, 1.0], [40.0 / 240.0, 8.0 / 160.0, 160.0 / 240.0, 144.0 / 160.0]] {
                let r = shadow_radius(&input(out, (240, 160), src));
                // Offset (3 reference px) plus blur (2) in game pixels.
                let reach = [r[0] * 2.5, r[1] * 2.5];
                assert!(reach[0] + 1.0 < PT_PAD as f32 && reach[1] + 1.0 < PT_PAD as f32, "{out:?} {r:?}");
            }
        }
    }
}
