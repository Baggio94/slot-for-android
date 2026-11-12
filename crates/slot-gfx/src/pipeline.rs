use crate::lcd3x::{LcdEffect, ScreenEffect};
use crate::lcd_cache::{CacheInput, LcdCaches, GAMMA_UNIT, GRAIN_UNIT, LIGHT_UNIT, SHADOW_UNIT, SOURCE_UNIT};
/// The upscaled GBA Video picture's texture unit, kept free of the
/// screen effects' caches (4 to 7).
const HIRES_UNIT: u32 = 3;
use std::cell::RefCell;
use crate::power::{screen_brightness, screen_rect_in};
use crate::quad::Quad;
use crate::shaders::{GAME_FRAG, RECT_VERT};
use crate::royale::Royale;
use crate::upscale::{Chain, Support, Upscaler};
use std::cell::Cell;
use crate::surface::{GfxError, OUT_H, out_w};

pub const SCALE: u32 = 3;
pub const SRC_W: u32 = crate::surface::BASE_W / SCALE;
pub const SRC_H: u32 = OUT_H / SCALE;

pub const WHOLE_TEXTURE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

/// One game program, specialised for a single screen effect.
struct GameProgram {
    prog: gl::types::GLuint,
    u_rect: gl::types::GLint,
    u_bright: gl::types::GLint,
    u_uv: gl::types::GLint,
    u_colour_correct: gl::types::GLint,
    u_paper_size: gl::types::GLint,
    u_fbo: gl::types::GLint,
    u_pic: gl::types::GLint,
    u_src: gl::types::GLint,
    u_pt_geo: gl::types::GLint,
    u_pt_r: gl::types::GLint,
    u_dm_fg: gl::types::GLint,
    u_dm_bgc: gl::types::GLint,
    u_dm_lum: gl::types::GLint,
    u_cached: gl::types::GLint,
}

impl GameProgram {
    /// The game shader with its effect fixed: the compiler drops every
    /// other effect, so a light effect stays light on a small GPU instead
    /// of paying for the heaviest one's registers.
    fn new(effect: ScreenEffect, lcd: LcdEffect, hires: bool) -> Result<Self, GfxError> {
        let prog = crate::shaders::program(RECT_VERT, &game_frag(effect, lcd, hires))?;
        let at = |name| crate::gl::uniform_location(prog, name);
        unsafe {
            gl::UseProgram(prog);
            gl::Uniform1i(at("u_game"), 0);
            gl::Uniform2f(at("u_src"), SRC_W as f32, SRC_H as f32);
            gl::Uniform2f(at("u_target"), out_w() as f32, OUT_H as f32);
            gl::Uniform1i(at("u_paper"), 1);
            gl::Uniform1i(at("u_dm_bg"), 2);
            gl::Uniform1i(at("u_hires"), HIRES_UNIT as i32);
            gl::Uniform1i(at("u_pt_cache"), SHADOW_UNIT as i32);
            gl::Uniform1i(at("u_light_cache"), LIGHT_UNIT as i32);
            gl::Uniform1i(at("u_grain"), GRAIN_UNIT as i32);
            // One of the two, as the effect reads (never both).
            gl::Uniform1i(at("u_game_linear"), SOURCE_UNIT as i32);
            gl::Uniform1i(at("u_gdv_glow"), SOURCE_UNIT as i32);
            gl::Uniform1i(at("u_edges"), SOURCE_UNIT as i32);
            // The linear-light cache exists only with 16 units (ES 3).
            let mut units = 0;
            gl::GetIntegerv(gl::MAX_TEXTURE_IMAGE_UNITS, &mut units);
            if units > GAMMA_UNIT as i32 {
                gl::Uniform1i(at("u_gamma"), GAMMA_UNIT as i32);
            }
            // Pixel Transparency's drop shadow: 3,3 at 640x480.
            gl::Uniform2f(at("u_shadow"), 3.0, 3.0);
        }
        Ok(GameProgram {
            prog,
            u_rect: at("u_rect"),
            u_bright: at("u_bright"),
            u_uv: at("u_uv"),
            u_colour_correct: at("u_colour_correct"),
            u_paper_size: at("u_paper_size"),
            u_fbo: at("u_fbo"),
            u_pic: at("u_pic"),
            u_src: at("u_src"),
            u_pt_geo: at("u_pt_geo"),
            u_pt_r: at("u_pt_r"),
            u_dm_fg: at("u_dm_fg"),
            u_dm_bgc: at("u_dm_bgc"),
            u_dm_lum: at("u_dm_lum"),
            u_cached: at("u_cached"),
        })
    }
}

/// `hires`: the game read from an upscaled picture (u_hires) instead of
/// its own pixels, for the shaders that take its colour as it is.
/// The game shader's cache pass for `effect` (xBR-lv2 / SABR corner flags,
/// the linear-light game), drawn once per game pixel ahead of the frame.
pub(crate) fn cache_frag(effect: ScreenEffect) -> String {
    format!("#define CACHE_PASS 1\n{}", game_frag(effect, LcdEffect::Off, false))
}

fn game_frag(effect: ScreenEffect, lcd: LcdEffect, hires: bool) -> String {
    format!("#define LCD_FX {}\n{}", lcd as u8, if hires { "#define HIRES 1\n" } else { "" })
        + &GAME_FRAG.replace("uniform float u_mode;",
        &format!("const float u_mode = {:.1};", effect.mode()))
}

const EFFECTS: usize = ScreenEffect::CrtRoyale as usize + 1;
const LCD_EFFECTS: usize = 3;

fn program_slot(effect: ScreenEffect, lcd: LcdEffect, hires: bool) -> usize {
    (effect as usize * LCD_EFFECTS + lcd as usize) * 2 + usize::from(hires)
}

fn rgb(c: u32) -> [f32; 3] {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0]
}

fn luma(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

/// How a home console frame (any size) fills the screen. The Game Boy
/// family keeps its 240x160 canvas and the source/picture rects instead.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Fit {
    /// The 240x160 canvas drawn over the game area.
    Canvas,
    /// Whole-number multiple of the frame height, at the display aspect.
    Integer(f32),
    /// The full height (or width) at the display aspect.
    Aspect(f32),
    /// RetroArch's integer overscale: the next whole multiple up when the
    /// picture doesn't fit exactly, its edges cut. Hi-res frames (448
    /// lines and up) count lines double.
    Overscale(f32),
    /// The whole screen.
    Stretch,
}

/// The texture window that shows a `tex`-sized frame at `fit` on an
/// `out`-sized screen: outside 0..1 is drawn black by the shader.
pub fn fit_window(fit: Fit, tex: (u32, u32), out: (f32, f32)) -> [f32; 4] {
    let (tw, th) = (tex.0.max(1) as f32, tex.1.max(1) as f32);
    let (ow, oh) = out;
    let picture = |aspect: f32| if aspect > 0.0 { aspect } else { tw / th };
    let (pw, ph) = match fit {
        Fit::Canvas | Fit::Stretch => return WHOLE_TEXTURE,
        Fit::Aspect(aspect) => {
            let a = picture(aspect);
            if oh * a <= ow { (oh * a, oh) } else { (ow, ow / a) }
        }
        Fit::Overscale(aspect) => {
            let a = picture(aspect);
            let unit = if th >= 400.0 { th / 2.0 } else { th };
            let k = (oh / unit).min(ow / (unit * a));
            // Within a hair of a whole multiple it fits: no cut.
            let k = if (k - k.round()).abs() < 1e-3 { k.round() } else { k.ceil() }.max(1.0);
            (unit * k * a, unit * k)
        }
        Fit::Integer(aspect) => {
            let a = picture(aspect);
            let mut k = (oh / th).floor().max(1.0);
            while k > 1.0 && th * k * a > ow { k -= 1.0; }
            let ph = th * k;
            ((ph * a).min(ow), ph)
        }
    };
    let (w, h) = (ow / pw, oh / ph);
    [(1.0 - w) / 2.0, (1.0 - h) / 2.0, w, h]
}

/// What crt-royale ran on: the frame's serial, its size, the picture size.
type RoyaleKey = (u64, (u32, u32), (u32, u32));

pub struct GamePass {
    context_lost: bool,
    /// Compiled on first use, one per effect and LCD Effect.
    programs: Vec<Option<GameProgram>>,
    game: gl::types::GLuint,
    // Reuse the Android GLES2-compatible RGBA staging pixels each frame.
    // The old per-frame flat_map().collect() allocated ~150 KiB at 60 Hz.
    #[cfg(target_os = "android")]
    rgba_upload: Vec<u8>,
    colour_correct: bool,
    fbo_scale: [f32; 2],
    pic: [f32; 4],
    paper: gl::types::GLuint,
    paper_size: f32,
    paper_pixels: Option<(u32, Vec<u8>)>,
    effect: ScreenEffect,
    lcd: LcdEffect,
    /// Sunlight Shimmer's light, 0..1 over the picture.
    light: [f32; 2],
    /// LCD Effect caches, drawn ahead of the game pass.
    caches: RefCell<LcdCaches>,
    /// Counts game frames uploaded, so the shadow cache follows the game.
    serial: u64,
    /// Dot Matrix: background, dots, then the lightest and darkest shade
    /// the game draws (0xRRGGBB).
    dot_palette: [u32; 4],
    dot_bg: gl::types::GLuint,
    dot_bg_pixels: Option<(u32, Vec<u8>)>,
    power: f32,
    src: [f32; 4],
    /// Canvas rect the whole 240x160 game texture maps to.
    area: [f32; 4],
    /// The game texture's current size: 240x160, or a home console frame.
    size: (u32, u32),
    /// Screen > Upscaler (GBA Video), as asked and as this context runs it.
    upscaler: Upscaler,
    running: Upscaler,
    support: Option<Support>,
    chain: Option<Chain>,
    /// The upscaled picture, and whether the frame changed since.
    hires: Cell<gl::types::GLuint>,
    stale: Cell<bool>,
    /// The last frame uploaded, to run the upscaler only when it changes.
    last_frame: Vec<u8>,
    chain_runs: Cell<u64>,
    /// crt-royale's passes, built when it is first chosen; the picture
    /// they made for this frame and its size (0 until there is one); what
    /// they last ran on; and whether this context can't run them.
    royale: RefCell<Option<Royale>>,
    royale_pic: Cell<(gl::types::GLuint, (u32, u32))>,
    royale_key: Cell<Option<RoyaleKey>>,
    royale_failed: Cell<bool>,
    fit: Fit,
    /// A home console picture at the top of the screen, not centred (a
    /// Watomsk logo sits in the band under it).
    top: bool,
}

impl GamePass {
    pub(crate) fn abandon_context(&mut self) {
        self.context_lost = true;
        self.caches.get_mut().abandon_context();
        self.drop_royale();
    }

    pub(crate) fn recovery_bytes(&self) -> usize {
        self.paper_pixels.as_ref().map_or(0, |p| p.1.len())
            + self.dot_bg_pixels.as_ref().map_or(0, |p| p.1.len())
    }

    pub(crate) fn restore_settings_into(&self, target: &mut Self) {
        target.colour_correct = self.colour_correct;
        target.fbo_scale = self.fbo_scale;
        target.pic = self.pic;
        target.lcd = self.lcd;
        target.set_effect(self.effect);
        target.light = self.light;
        target.dot_palette = self.dot_palette;
        if let Some((size, rgba)) = &self.dot_bg_pixels {
            target.set_dot_background(*size, rgba);
        }
        target.power = self.power;
        target.src = self.src;
        target.area = self.area;
        target.fit = self.fit;
        if let Some((size, rgba)) = &self.paper_pixels {
            target.set_paper(*size, rgba);
        }
        target.set_upscaler(self.upscaler);
    }

    pub fn new() -> Result<Self, GfxError> {
        // OpenGL ES 2 only guarantees RGBA, not BGRA texture uploads.
        #[cfg(target_os = "android")]
        let format = gl::RGBA;
        #[cfg(not(target_os = "android"))]
        let format = gl::BGRA;
        let effect = ScreenEffect::Lcd3x;
        let mut programs: Vec<Option<GameProgram>> = (0..EFFECTS * LCD_EFFECTS * 2).map(|_| None).collect();
        programs[program_slot(effect, LcdEffect::Off, false)] = Some(GameProgram::new(effect, LcdEffect::Off, false)?);
        let game = crate::gl::texture(SRC_W, SRC_H, gl::NEAREST, gl::CLAMP_TO_EDGE, format, None);
        let paper = crate::gl::texture(1, 1, gl::NEAREST, gl::REPEAT, gl::RGBA, Some(&[255; 4]));
        let dot_bg = crate::gl::texture(1, 1, gl::NEAREST, gl::REPEAT, gl::RGBA, Some(&[128, 128, 128, 255]));
        Ok(GamePass {
            context_lost: false,
            programs,
            game,
            #[cfg(target_os = "android")]
            rgba_upload: vec![0; (SRC_W * SRC_H * 4) as usize],
            colour_correct: false,
            fbo_scale: [1.0, 1.0],
            pic: WHOLE_TEXTURE,
            paper,
            paper_size: 1.0,
            paper_pixels: None,
            effect,
            lcd: LcdEffect::Off,
            light: [0.5, 0.5],
            caches: RefCell::new(LcdCaches::default()),
            serial: 0,
            dot_palette: [0xD9C425, 0x1D6A6B, 0xFFFFFF, 0x000000],
            dot_bg,
            dot_bg_pixels: None,
            power: 1.0,
            src: WHOLE_TEXTURE,
            area: default_area(),
            size: (SRC_W, SRC_H),
            upscaler: Upscaler::Off,
            running: Upscaler::Off,
            support: None,
            chain: None,
            hires: Cell::new(0),
            stale: Cell::new(true),
            last_frame: Vec::new(),
            chain_runs: Cell::new(0),
            royale: RefCell::new(None),
            royale_pic: Cell::new((0, (1, 1))),
            royale_key: Cell::new(None),
            royale_failed: Cell::new(false),
            fit: Fit::Canvas,
            top: false,
        })
    }

    pub fn set_power(&mut self, t: f32) {
        self.power = t.clamp(0.0, 1.0);
    }

    pub fn set_picture(&mut self, rect: [f32; 4]) {
        self.pic = rect;
    }

    pub fn set_fbo_scale(&mut self, x: f32, y: f32) {
        self.fbo_scale = [x, y];
    }

    pub fn set_effect(&mut self, effect: ScreenEffect) {
        if self.build(effect, self.lcd, false) {
            if self.running != Upscaler::Off { self.build(effect, self.lcd, true); }
            if effect != self.effect { self.drop_royale(); }
            if effect == ScreenEffect::CrtRoyale && !self.context_lost {
                self.support.get_or_insert_with(crate::upscale::probe);
            }
            self.effect = effect;
        }
    }

    /// crt-royale's passes and targets, let go when another shader is
    /// chosen (a few screen-sized textures).
    fn drop_royale(&mut self) {
        if let Some(mut chain) = self.royale.get_mut().take() {
            if self.context_lost { chain.forget() } else { chain.release() }
        }
        self.royale_pic.set((0, (1, 1)));
        self.royale_key.set(None);
        self.royale_failed.set(false);
    }

    /// The shader the game pass draws: crt-royale's finished picture shown
    /// as it is (None), or zfast-crt where crt-royale can't run (and for a
    /// still).
    fn shown(&self, still: bool) -> ScreenEffect {
        match self.effect {
            ScreenEffect::CrtRoyale if !still && self.royale_pic.get().0 != 0 => ScreenEffect::None,
            ScreenEffect::CrtRoyale => ScreenEffect::ZfastCrt,
            effect => effect,
        }
    }

    /// Runs crt-royale on the frame when it changed: the whole game texture
    /// at the size it takes on screen (full size, whatever the power-on
    /// animation does), before the frame's target is bound.
    fn run_royale(&self, quad: &Quad, src: [f32; 4], size: (u32, u32), area: [f32; 4]) {
        let can = self.effect == ScreenEffect::CrtRoyale && !self.context_lost
            && !self.royale_failed.get() && self.support.is_some_and(|s| s.passes);
        if !can {
            self.royale_pic.set((0, (1, 1)));
            return;
        }
        let (_, _, w, h) = screen_rect_in(area, 1.0);
        let view = crate::royale::view_size(src, (w * self.fbo_scale[0], h * self.fbo_scale[1]));
        let mut chain = self.royale.borrow_mut();
        if chain.is_none() {
            *chain = Royale::new();
        }
        let Some(royale) = chain.as_mut() else {
            self.royale_failed.set(true);
            self.royale_pic.set((0, (1, 1)));
            return;
        };
        if !royale.size_for(size, view) {
            if let Some(mut broken) = chain.take() { broken.release(); }
            self.royale_failed.set(true);
            self.royale_pic.set((0, (1, 1)));
            return;
        }
        let key = (self.serial, size, view);
        if self.royale_key.get() != Some(key) || self.royale_pic.get().0 == 0 {
            self.royale_pic.set((royale.run(self.game, self.serial, quad), view));
            self.royale_key.set(Some(key));
        }
    }

    /// Pixel Transparency / Sunlight Shimmer over the current shader.
    pub fn set_lcd_effect(&mut self, lcd: LcdEffect) {
        if self.build(self.effect, lcd, false) {
            if self.running != Upscaler::Off { self.build(self.effect, lcd, true); }
            self.lcd = lcd;
        }
    }

    /// Screen > Upscaler for a GBA Video cart. What this context can't run
    /// falls back: Anime4K to FSR, FSR to Bilinear.
    pub fn set_upscaler(&mut self, upscaler: Upscaler) {
        if upscaler == self.upscaler && (upscaler == Upscaler::Off || self.running != Upscaler::Off) {
            return;
        }
        self.upscaler = upscaler;
        if let Some(mut chain) = self.chain.take() {
            if self.context_lost { chain.forget() } else { chain.release() }
        }
        self.running = Upscaler::Off;
        self.stale.set(true);
        if upscaler == Upscaler::Off || self.context_lost {
            return;
        }
        let support = *self.support.get_or_insert_with(crate::upscale::probe);
        let tries: &[Upscaler] = match upscaler {
            Upscaler::Anime4k => &[Upscaler::Anime4k, Upscaler::Fsr],
            Upscaler::Anime4kFast => &[Upscaler::Anime4kFast, Upscaler::Fsr],
            Upscaler::Fsr => &[Upscaler::Fsr],
            _ => &[],
        };
        for &kind in tries {
            if let Some(chain) = Chain::new(kind, (SRC_W, SRC_H), support) {
                self.chain = Some(chain);
                self.running = kind;
                break;
            }
        }
        if self.running == Upscaler::Off { self.running = Upscaler::Bilinear; }
        // The programs reading the upscaled picture.
        let (effect, lcd) = (self.effect, self.lcd);
        if !self.build(effect, lcd, true) {
            self.running = Upscaler::Off;
        }
    }

    /// The upscaler this context runs (after any fallback), for tests and logs.
    pub fn upscaler_running(&self) -> Upscaler {
        self.running
    }

    /// Whether crt-royale made this frame's picture.
    pub fn crt_royale_running(&self) -> bool {
        self.effect == ScreenEffect::CrtRoyale && self.royale_pic.get().0 != 0
    }

    /// How many times the upscaler ran: once per new picture.
    pub fn upscaler_runs(&self) -> u64 {
        self.chain_runs.get()
    }

    /// Whether the game is drawn from the upscaled picture: a GBA frame on
    /// its canvas, with an upscaler.
    fn upscaled(&self) -> bool {
        self.running != Upscaler::Off && self.fit == Fit::Canvas && self.size == (SRC_W, SRC_H)
    }

    fn build(&mut self, effect: ScreenEffect, lcd: LcdEffect, hires: bool) -> bool {
        if effect == ScreenEffect::CrtRoyale {
            // Its picture is shown through None; zfast-crt stands in
            // where it can't run.
            return self.build(ScreenEffect::ZfastCrt, lcd, hires)
                && self.build(ScreenEffect::None, lcd, hires);
        }
        let slot = &mut self.programs[program_slot(effect, lcd, hires)];
        if slot.is_none() {
            match GameProgram::new(effect, lcd, hires) {
                Ok(program) => *slot = Some(program),
                // Keep the current effect rather than draw nothing.
                Err(_) => return false,
            }
        }
        true
    }

    /// Sunlight Shimmer's light position, 0..1 over the picture.
    pub fn set_light(&mut self, x: f32, y: f32) {
        self.light = [x, y];
    }

    /// Dot Matrix colours, 0xRRGGBB: background, dots, then the lightest
    /// and darkest shade the game draws (they set how dark a dot is).
    pub fn set_dot_palette(&mut self, colours: [u32; 4]) {
        self.dot_palette = colours;
    }

    pub fn has_dot_background(&self) -> bool {
        self.dot_bg_pixels.is_some()
    }

    pub fn has_paper(&self) -> bool {
        self.paper_pixels.is_some()
    }

    /// Dot Matrix background grain, a square RGBA texture.
    pub fn set_dot_background(&mut self, size: u32, rgba: &[u8]) {
        let tex = crate::gl::texture(size, size, gl::LINEAR, gl::REPEAT, gl::RGBA, Some(rgba));
        unsafe { gl::DeleteTextures(1, &self.dot_bg) };
        self.dot_bg = tex;
        self.dot_bg_pixels = Some((size, rgba.to_vec()));
    }

    pub fn set_colour_correction(&mut self, enabled: bool) {
        self.colour_correct = enabled;
    }

    pub fn set_paper(&mut self, size: u32, rgba: &[u8]) {
        let tex = crate::gl::texture(size, size, gl::NEAREST, gl::REPEAT, gl::RGBA, Some(rgba));
        unsafe { gl::DeleteTextures(1, &self.paper) };
        self.paper = tex;
        self.paper_size = size as f32;
        self.paper_pixels = Some((size, rgba.to_vec()));
    }

    pub fn set_source_rect(&mut self, rect: [f32; 4]) {
        self.src = rect;
    }

    pub fn set_area(&mut self, area: [f32; 4]) {
        self.area = area;
    }

    pub fn set_fit(&mut self, fit: Fit) {
        self.fit = fit;
    }

    pub fn set_top(&mut self, top: bool) {
        self.top = top;
    }

    /// Reallocate the game texture when a frame arrives at another size
    /// (a home console, SNES hi-res, a Mega Drive mode switch).
    fn resize(&mut self, w: u32, h: u32) {
        if self.size == (w, h) {
            return;
        }
        #[cfg(target_os = "android")]
        let format = gl::RGBA;
        #[cfg(not(target_os = "android"))]
        let format = gl::BGRA;
        let tex = crate::gl::texture(w, h, gl::NEAREST, gl::CLAMP_TO_EDGE, format, None);
        unsafe { gl::DeleteTextures(1, &self.game) };
        self.game = tex;
        self.size = (w, h);
        #[cfg(target_os = "android")]
        self.rgba_upload.resize((w * h * 4) as usize, 0);
    }

    pub fn upload(&mut self, xrgb8888: &[u8]) {
        self.upload_sized(xrgb8888, (SRC_W, SRC_H));
    }

    pub fn upload_sized(&mut self, xrgb8888: &[u8], (w, h): (u32, u32)) {
        if w == 0 || h == 0 || xrgb8888.len() < (w * h * 4) as usize {
            return;
        }
        self.resize(w, h);
        if self.running != Upscaler::Off {
            // A GBA Video picture changes 8 to 12 times a second: the
            // upscaler runs only when it did.
            let frame = &xrgb8888[..(w * h * 4) as usize];
            if self.last_frame.as_slice() != frame {
                self.last_frame.clear();
                self.last_frame.extend_from_slice(frame);
                self.stale.set(true);
            }
        }
        self.serial = self.serial.wrapping_add(1);
        #[cfg(target_os = "android")]
        {
            // ES2 requires RGBA; the libretro frame arrives as XRGB8888
            // in little-endian B,G,R,X order. Conversion is still required,
            // but happens in a persistent buffer without heap churn.
            for (src, dst) in xrgb8888
                .chunks_exact(4)
                .zip(self.rgba_upload.chunks_exact_mut(4))
            {
                dst.copy_from_slice(&[src[2], src[1], src[0], 255]);
            }
        }
        #[cfg(target_os = "android")]
        let pixels = self.rgba_upload.as_ptr();
        #[cfg(not(target_os = "android"))]
        let pixels = xrgb8888.as_ptr();
        #[cfg(target_os = "android")]
        let format = gl::RGBA;
        #[cfg(not(target_os = "android"))]
        let format = gl::BGRA;
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.game);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::TexSubImage2D(
                gl::TEXTURE_2D,
                0,
                0,
                0,
                w as i32,
                h as i32,
                format,
                gl::UNSIGNED_BYTE,
                pixels as *const std::ffi::c_void,
            );
        }
    }

    /// What the live game draws with: source rect, texture size, picture
    /// and canvas rects.
    fn live(&self) -> ([f32; 4], (u32, u32), [f32; 4], [f32; 4]) {
        if self.fit != Fit::Canvas {
            // A home console frame fills the whole canvas through its fit window.
            let canvas = [0.0, 0.0, out_w() as f32, OUT_H as f32];
            let out = (canvas[2] * self.fbo_scale[0], canvas[3] * self.fbo_scale[1]);
            let mut window = fit_window(self.fit, self.size, out);
            if self.top { window[1] = 0.0; }
            return (window, self.size, WHOLE_TEXTURE, canvas);
        }
        let src = match self.effect {
            ScreenEffect::Grid => whole_scale_source(self.src, self.area, self.fbo_scale),
            _ => self.src,
        };
        (src, self.size, self.pic, self.area)
    }

    /// Draw the LCD Effect caches for the coming frame, before the frame's
    /// target is bound (switching targets mid-frame costs a small GPU a
    /// copy of the whole frame).
    pub fn prepare(&self, quad: &Quad) {
        let (src, size, pic, area) = self.live();
        self.run_royale(quad, src, size, area);
        self.update_caches(quad, self.shown(false), self.game, src, size, pic, area, false);
    }

    #[allow(clippy::too_many_arguments)]
    fn update_caches(&self, quad: &Quad, effect: ScreenEffect, tex: gl::types::GLuint, src: [f32; 4],
                     size: (u32, u32), pic: [f32; 4], area: [f32; 4], still: bool)
                     -> crate::lcd_cache::CacheReadout {
        let (_, _, w, h) = screen_rect_in(area, self.power);
        let input = CacheInput {
            effect,
            lcd: self.lcd,
            tex,
            size,
            serial: if still { 0 } else { self.serial },
            pic,
            src,
            out: (w * self.fbo_scale[0], h * self.fbo_scale[1]),
            light: self.light,
        };
        self.caches.borrow_mut().update(quad, &input, still)
    }

    pub fn draw(&self, quad: &Quad) {
        let (src, size, pic, area) = self.live();
        let effect = self.shown(false);
        let (royale, view) = self.royale_pic.get();
        if self.effect == ScreenEffect::CrtRoyale && royale != 0 {
            let readout = self.update_caches(quad, effect, self.game, src, size, pic, area, false);
            let (_, _, w, h) = screen_rect_in(area, self.power);
            let shown = (w * self.fbo_scale[0], h * self.fbo_scale[1]);
            let window = crate::royale::pixel_window(src, shown, view);
            self.draw_program(effect, false, royale, quad, window, view, pic, area, readout);
            return;
        }
        if self.upscaled() {
            let hires = self.upscaled_picture(quad);
            if self.running == Upscaler::Bilinear {
                unsafe {
                    gl::BindTexture(gl::TEXTURE_2D, self.game);
                    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
                    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
                }
            }
            self.draw_hires(hires, quad, src);
            if self.running == Upscaler::Bilinear {
                unsafe {
                    gl::BindTexture(gl::TEXTURE_2D, self.game);
                    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
                    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
                }
            }
            return;
        }
        let readout = self.update_caches(quad, effect, self.game, src, size, pic, area, false);
        self.draw_program(effect, false, self.game, quad, src, size, pic, area, readout);
    }

    /// The upscaled picture, made again when the frame changed.
    fn upscaled_picture(&self, quad: &Quad) -> gl::types::GLuint {
        let Some(chain) = &self.chain else { return self.game };
        if self.stale.get() || self.hires.get() == 0 {
            self.hires.set(chain.run(self.game, quad));
            self.stale.set(false);
            self.chain_runs.set(self.chain_runs.get() + 1);
        }
        self.hires.get()
    }

    fn draw_hires(&self, hires: gl::types::GLuint, quad: &Quad, src: [f32; 4]) {
        let effect = self.shown(false);
        let cache = self.update_caches(quad, effect, self.game, src, self.size, self.pic, self.area, false);
        if self.programs[program_slot(effect, self.lcd, true)].is_none() {
            self.draw_program(effect, false, self.game, quad, src, self.size, self.pic, self.area, cache);
            return;
        }
        unsafe {
            gl::ActiveTexture(gl::TEXTURE0 + HIRES_UNIT);
            gl::BindTexture(gl::TEXTURE_2D, hires);
            gl::ActiveTexture(gl::TEXTURE0);
        }
        self.draw_program(effect, true, self.game, quad, src, self.size, self.pic, self.area, cache);
    }

    pub fn draw_still(&self, tex: gl::types::GLuint, quad: &Quad) {
        let (src, size, pic, area) = (WHOLE_TEXTURE, (SRC_W, SRC_H), self.pic, self.area);
        let effect = self.shown(true);
        let readout = self.update_caches(quad, effect, tex, src, size, pic, area, true);
        self.draw_program(effect, false, tex, quad, src, size, pic, area, readout);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_program(&self, effect: ScreenEffect, hires: bool, tex: gl::types::GLuint, quad: &Quad,
                    src: [f32; 4], size: (u32, u32), pic: [f32; 4], area: [f32; 4],
                    cache: crate::lcd_cache::CacheReadout) {
        let Some(p) = &self.programs[program_slot(effect, self.lcd, hires)] else { return };
        let [bg, dots, light, dark] = self.dot_palette.map(rgb);
        let (x, y, w, h) = screen_rect_in(area, self.power);
        unsafe {
            gl::UseProgram(p.prog);
            gl::Uniform2f(p.u_src, size.0 as f32, size.1 as f32);
            gl::Uniform4f(p.u_rect, x, y, w, h);
            gl::Uniform4f(p.u_uv, src[0], src[1], src[2], src[3]);
            gl::Uniform1f(p.u_bright, screen_brightness(self.power));
            gl::Uniform1f(
                p.u_colour_correct,
                if self.colour_correct { 1.0 } else { 0.0 },
            );
            gl::Uniform1f(p.u_paper_size, self.paper_size);
            gl::Uniform2f(p.u_fbo, self.fbo_scale[0], self.fbo_scale[1]);
            gl::Uniform4f(p.u_pic, pic[0], pic[1], pic[2], pic[3]);
            gl::Uniform4f(p.u_pt_geo, cache.geo[0], cache.geo[1], cache.geo[2], cache.geo[3]);
            gl::Uniform2f(p.u_pt_r, cache.radius[0], cache.radius[1]);
            gl::Uniform3f(p.u_dm_fg, dots[0], dots[1], dots[2]);
            gl::Uniform3f(p.u_dm_bgc, bg[0], bg[1], bg[2]);
            gl::Uniform2f(p.u_dm_lum, luma(light), luma(dark));
            gl::Uniform1f(p.u_cached, if cache.cached { 1.0 } else { 0.0 });
            self.caches.borrow().bind(effect);
            gl::ActiveTexture(gl::TEXTURE2);
            gl::BindTexture(gl::TEXTURE_2D, self.dot_bg);
            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, self.paper);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, tex);
        }
        quad.draw();
    }
}

/// The largest 3:2 rect centred on the canvas: the whole canvas on a 3:2
/// panel, full height on a wider one, full width on a narrower one.
pub fn default_area() -> [f32; 4] {
    let (cw, ch) = (out_w() as f32, OUT_H as f32);
    let s = (cw / SRC_W as f32).min(ch / SRC_H as f32);
    let (w, h) = (SRC_W as f32 * s, SRC_H as f32 * s);
    [(cw - w) / 2.0, (ch - h) / 2.0, w, h]
}

fn whole_scale_source(src: [f32; 4], area: [f32; 4], fbo: [f32; 2]) -> [f32; 4] {
    let (x, w) = whole_scale(src[0], src[2], SRC_W as f32, area[2] * fbo[0]);
    let (y, h) = whole_scale(src[1], src[3], SRC_H as f32, area[3] * fbo[1]);
    [x, y, w, h]
}

fn whole_scale(at: f32, len: f32, texels: f32, out: f32) -> (f32, f32) {
    let shown = len * texels;
    let scale = (out / shown).ceil();
    let trim = scale * shown - out;
    if trim >= 0.1 * shown {
        return (at, len);
    }
    let top = (trim / 2.0).floor();
    (at + top / scale / texels, out / scale / texels)
}

impl Drop for GamePass {
    fn drop(&mut self) {
        if self.context_lost {
            if let Some(chain) = self.chain.as_mut() { chain.forget(); }
            if let Some(royale) = self.royale.get_mut() { royale.forget(); }
            return;
        }
        if let Some(chain) = self.chain.as_mut() { chain.release(); }
        if let Some(royale) = self.royale.get_mut() { royale.release(); }
        unsafe {
            gl::DeleteTextures(1, &self.game);
            gl::DeleteTextures(1, &self.paper);
            gl::DeleteTextures(1, &self.dot_bg);
            for program in self.programs.iter().flatten() {
                gl::DeleteProgram(program.prog);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_effect_gets_its_own_constant_mode() {
        for effect in [ScreenEffect::None, ScreenEffect::ZfastLcd, ScreenEffect::SharpBilinear,
                       ScreenEffect::DotMatrix, ScreenEffect::XbrLv2, ScreenEffect::DotMatrixColour] {
            let frag = game_frag(effect, LcdEffect::SunlightShimmer, false);
            assert!(!frag.contains("uniform float u_mode;"));
            assert!(frag.contains(&format!("const float u_mode = {:.1};", effect.mode())));
            assert!(frag.starts_with("#define LCD_FX 2\n"));
        }
        assert_eq!(EFFECTS, 27);
        assert_eq!(program_slot(ScreenEffect::CrtRoyale, LcdEffect::SunlightShimmer, true),
                   EFFECTS * LCD_EFFECTS * 2 - 1);
        assert!(game_frag(ScreenEffect::Lcd3x, LcdEffect::Off, true).contains("#define HIRES 1\n"));
    }

    #[test]
    fn original_ratio_pillarboxes_a_4_3_picture_on_3_2() {
        let [x, y, w, h] = fit_window(Fit::Aspect(4.0 / 3.0), (256, 224), (960.0, 640.0));
        assert!((h - 1.0).abs() < 1e-6 && y.abs() < 1e-6);
        // 853.3 of 960 columns show the picture.
        assert!((w - 960.0 / (640.0 * 4.0 / 3.0)).abs() < 1e-5);
        assert!((x + w / 2.0 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn overscale_takes_the_next_whole_multiple_and_cuts_the_edges() {
        // SNES at 8:7 on the Retroid Pocket Classic: x5, 1280x1120.
        let [x, y, w, h] = fit_window(Fit::Overscale(256.0 / 224.0), (256, 224), (1240.0, 1080.0));
        assert!((1080.0 / h - 1120.0).abs() < 1e-2 && (1240.0 / w - 1280.0).abs() < 1e-2);
        assert!(x > 0.0 && y > 0.0, "the window crops the picture");
        // The same picture size for a hi-res interlaced frame.
        let [_, _, w2, h2] = fit_window(Fit::Overscale(256.0 / 224.0), (512, 448), (1240.0, 1080.0));
        assert!((w - w2).abs() < 1e-6 && (h - h2).abs() < 1e-6);
        // NES on a 720x720 square: x3, 768x720.
        let [_, _, w, h] = fit_window(Fit::Overscale(256.0 / 240.0), (256, 240), (720.0, 720.0));
        assert!((720.0 / w - 768.0).abs() < 1e-2 && (720.0 / h - 720.0).abs() < 1e-2);
    }

    #[test]
    fn integer_keeps_whole_line_multiples() {
        // 224 lines on a 640-line panel: twice, 448 lines tall.
        let [_, _, _, h] = fit_window(Fit::Integer(4.0 / 3.0), (256, 224), (960.0, 640.0));
        assert!((640.0 / h - 448.0).abs() < 1e-3);
        // A hi-res interlaced frame keeps the same picture size.
        let [_, _, _, h2] = fit_window(Fit::Integer(4.0 / 3.0), (512, 448), (960.0, 640.0));
        assert!((h - h2).abs() < 1e-6);
    }

    #[test]
    fn stretch_and_canvas_show_the_whole_texture() {
        assert_eq!(fit_window(Fit::Stretch, (320, 224), (960.0, 640.0)), WHOLE_TEXTURE);
        assert_eq!(fit_window(Fit::Canvas, (240, 160), (960.0, 640.0)), WHOLE_TEXTURE);
    }
}
