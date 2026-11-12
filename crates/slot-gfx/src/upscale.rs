//! Upscalers for Game Boy Advance Video carts: the 240x160 frame made
//! bigger and cleaner before the screen shader lays its grid on it (Siham
//! 2026-11-10). A cartoon on these carts changes only 8 to 12 times a
//! second, so the passes run only when the picture changed.
//!
//! Anime4K's networks run unchanged from their mpv hook files; FSR is the
//! libretro port of AMD's EASU + RCAS. Both need OpenGL ES 3 (Anime4K also
//! half-float render targets); without them the picture stays as it is.

use std::collections::HashMap;

/// Screen > Upscaler. Off keeps the pixels as they are.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum Upscaler {
    #[default]
    Off,
    Bilinear,
    Fsr,
    Anime4kFast,
    Anime4k,
}

const RESTORE_S: &str = include_str!("upscale/Anime4K_Restore_CNN_Soft_S.glsl");
const UPSCALE_S: &str = include_str!("upscale/Anime4K_Upscale_CNN_x2_S.glsl");
const RESTORE_M: &str = include_str!("upscale/Anime4K_Restore_CNN_Soft_M.glsl");
const UPSCALE_M: &str = include_str!("upscale/Anime4K_Upscale_CNN_x2_M.glsl");
const FSR_EASU: &str = include_str!("upscale/fsr-pass0.glsl");
const FSR_RCAS: &str = include_str!("upscale/fsr-pass1.glsl");

/// FSR's output, in source pixels: four times the GBA's (960x640, the
/// KPA's panel); the screen shader takes it from there.
const FSR_SCALE: f32 = 4.0;

/// One pass of an mpv hook file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HookPass {
    pub binds: Vec<String>,
    pub save: Option<String>,
    pub width: Option<String>,
    pub height: Option<String>,
    pub code: String,
}

/// The passes of an mpv hook file (//!DESC starts each one).
pub(crate) fn parse_hooks(src: &str) -> Vec<HookPass> {
    let mut passes: Vec<HookPass> = Vec::new();
    for line in src.lines() {
        if let Some(rest) = line.strip_prefix("//!") {
            let (key, value) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let value = value.trim().to_owned();
            if key == "DESC" || passes.is_empty() {
                passes.push(HookPass { binds: Vec::new(), save: None, width: None, height: None,
                    code: String::new() });
            }
            let Some(pass) = passes.last_mut() else { continue };
            match key {
                "BIND" => pass.binds.push(value),
                "SAVE" => pass.save = Some(value),
                "WIDTH" => pass.width = Some(value),
                "HEIGHT" => pass.height = Some(value),
                _ => {}
            }
        } else if let Some(pass) = passes.last_mut() {
            pass.code.push_str(line);
            pass.code.push('\n');
        }
    }
    passes
}

/// mpv's reverse-Polish size expressions: "conv2d_tf.w 2 *".
pub(crate) fn rpn(expr: &str, sizes: &HashMap<String, (u32, u32)>) -> Option<f32> {
    let mut stack: Vec<f32> = Vec::new();
    for token in expr.split_whitespace() {
        match token {
            "+" | "-" | "*" | "/" | ">" | "<" => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(match token {
                    "+" => a + b, "-" => a - b, "*" => a * b, "/" => a / b,
                    ">" => f32::from(a > b), _ => f32::from(a < b),
                });
            }
            _ => {
                if let Some((name, side)) = token.rsplit_once('.') {
                    let (w, h) = *sizes.get(name)?;
                    stack.push(if side.starts_with('w') { w as f32 } else { h as f32 });
                } else {
                    stack.push(token.parse().ok()?);
                }
            }
        }
    }
    stack.pop()
}

/// Which hook files an upscaler runs, in order.
fn hook_files(kind: Upscaler) -> &'static [&'static str] {
    match kind {
        Upscaler::Anime4kFast => &[RESTORE_S, UPSCALE_S, UPSCALE_S],
        Upscaler::Anime4k => &[RESTORE_M, UPSCALE_M, UPSCALE_M],
        _ => &[],
    }
}

pub(crate) fn version_line() -> &'static str {
    if crate::gl::es() { "#version 300 es\nprecision highp float;\nprecision highp int;\n" }
    else { "#version 330 core\n" }
}

const PASS_VERT: &str = "in vec2 a_pos;\nout vec2 v_pos;\nout vec4 TEX0;\n\
    void main() { v_pos = a_pos; TEX0 = vec4(a_pos, 0.0, 0.0);\n\
    gl_Position = vec4(a_pos * 2.0 - 1.0, 0.0, 1.0); }\n";

/// A hook pass as a whole fragment shader: each bound texture's macros as
/// mpv defines them, HOOKED being MAIN.
pub(crate) fn hook_fragment(pass: &HookPass) -> String {
    let mut src = String::from(version_line());
    src.push_str("in vec2 v_pos;\nout vec4 slot_out;\n");
    for name in &pass.binds {
        src.push_str(&format!(
            "uniform sampler2D {n}_raw;\nuniform vec2 {n}_size;\nuniform vec2 {n}_pt;\n\
             #define {n}_pos v_pos\n#define {n}_tex(p) texture({n}_raw, p)\n\
             #define {n}_texOff(o) texture({n}_raw, v_pos + (o) * {n}_pt)\n",
            n = name));
    }
    if pass.binds.iter().any(|b| b == "MAIN") {
        src.push_str("#define HOOKED_raw MAIN_raw\n#define HOOKED_pos v_pos\n\
            #define HOOKED_size MAIN_size\n#define HOOKED_pt MAIN_pt\n\
            #define HOOKED_tex(p) MAIN_tex(p)\n#define HOOKED_texOff(o) MAIN_texOff(o)\n");
    }
    src.push_str(&pass.code);
    src.push_str("\nvoid main() { slot_out = hook(); }\n");
    src
}

/// A RetroArch GLSL pass (FSR) as its fragment shader.
fn retroarch_fragment(src: &str) -> String {
    format!("{}#define FRAGMENT\n{}", version_line(), src)
}

/// Whether this context can run the upscalers at all, and Anime4K's
/// half-float targets in particular.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Support {
    pub passes: bool,
    pub half_float: bool,
}

const RGBA16F: gl::types::GLenum = 0x881A;
const HALF_FLOAT: gl::types::GLenum = 0x140B;

pub(crate) fn gl_major() -> u32 {
    let version = unsafe { gl::GetString(gl::VERSION) };
    if version.is_null() { return 0; }
    let text = unsafe { std::ffi::CStr::from_ptr(version as *const std::ffi::c_char) }
        .to_string_lossy().into_owned();
    text.split(|c: char| !c.is_ascii_digit()).find(|s| !s.is_empty())
        .and_then(|s| s.parse().ok()).unwrap_or(0)
}

/// A texture to render a pass into, with its framebuffer.
struct Target {
    tex: gl::types::GLuint,
    fbo: gl::types::GLuint,
    w: u32,
    h: u32,
}

fn target(w: u32, h: u32, half: bool) -> Option<Target> {
    unsafe {
        let mut tex = 0;
        gl::GenTextures(1, &mut tex);
        gl::BindTexture(gl::TEXTURE_2D, tex);
        if half {
            gl::TexImage2D(gl::TEXTURE_2D, 0, RGBA16F as i32, w as i32, h as i32, 0,
                gl::RGBA, HALF_FLOAT, std::ptr::null());
        } else {
            gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA8 as i32, w as i32, h as i32, 0,
                gl::RGBA, gl::UNSIGNED_BYTE, std::ptr::null());
        }
        for (name, value) in [(gl::TEXTURE_MIN_FILTER, gl::LINEAR), (gl::TEXTURE_MAG_FILTER, gl::LINEAR),
                              (gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE), (gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE)] {
            gl::TexParameteri(gl::TEXTURE_2D, name, value as i32);
        }
        let mut fbo = 0;
        gl::GenFramebuffers(1, &mut fbo);
        gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
        gl::FramebufferTexture2D(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, tex, 0);
        let complete = gl::CheckFramebufferStatus(gl::FRAMEBUFFER) == gl::FRAMEBUFFER_COMPLETE;
        let target = Target { tex, fbo, w, h };
        if complete { Some(target) } else { drop_target(&target); None }
    }
}

fn drop_target(t: &Target) {
    unsafe {
        gl::DeleteFramebuffers(1, &t.fbo);
        gl::DeleteTextures(1, &t.tex);
    }
}

/// Probes the context once: ES 3 / GL 3.3 for the passes, a complete
/// half-float framebuffer for Anime4K.
pub(crate) fn probe() -> Support {
    let passes = gl_major() >= 3;
    if !passes { return Support::default(); }
    let mut previous = 0;
    unsafe { gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut previous) };
    let half = target(4, 4, true).map(|t| drop_target(&t)).is_some();
    unsafe {
        gl::BindFramebuffer(gl::FRAMEBUFFER, previous as u32);
        // A failed probe must not leave an error for the next caller.
        while gl::GetError() != gl::NO_ERROR {}
    }
    Support { passes, half_float: half }
}

/// A compiled pass: its program, what it reads and where it writes.
struct Pass {
    prog: gl::types::GLuint,
    /// Each bound texture: its name in the registry and its uniforms.
    binds: Vec<(String, gl::types::GLint, gl::types::GLint, gl::types::GLint)>,
    /// RetroArch uniforms (FSR): TextureSize, InputSize, OutputSize.
    retro: Option<[gl::types::GLint; 3]>,
    out: Target,
    save: String,
}

/// An upscaler built for one source size: its passes and their targets.
pub(crate) struct Chain {
    source: (u32, u32),
    passes: Vec<Pass>,
}

impl Chain {
    /// Builds the chain, or None when this context can't run it.
    pub fn new(kind: Upscaler, source: (u32, u32), support: Support) -> Option<Chain> {
        if kind == Upscaler::Off || kind == Upscaler::Bilinear || !support.passes { return None; }
        let half = matches!(kind, Upscaler::Anime4kFast | Upscaler::Anime4k);
        if half && !support.half_float { return None; }
        let vert = format!("{}{}", version_line(), PASS_VERT);
        let mut chain = Chain { source, passes: Vec::new() };
        let mut sizes: HashMap<String, (u32, u32)> = HashMap::new();
        sizes.insert("MAIN".into(), source);
        let ok = if half {
            hook_files(kind).iter().all(|file| parse_hooks(file).iter().all(|hook| {
                chain.add_hook(hook, &vert, &mut sizes)
            }))
        } else {
            let out = ((source.0 as f32 * FSR_SCALE) as u32, (source.1 as f32 * FSR_SCALE) as u32);
            chain.add_retroarch(FSR_EASU, &vert, out) && chain.add_retroarch(FSR_RCAS, &vert, out)
        };
        unsafe { gl::BindFramebuffer(gl::FRAMEBUFFER, 0) };
        if ok { Some(chain) } else { chain.release(); None }
    }

    fn add_hook(&mut self, hook: &HookPass, vert: &str,
                sizes: &mut HashMap<String, (u32, u32)>) -> bool {
        let main = sizes.get("MAIN").copied().unwrap_or(self.source);
        let w = hook.width.as_deref().and_then(|e| rpn(e, sizes)).map_or(main.0, |v| v.round() as u32);
        let h = hook.height.as_deref().and_then(|e| rpn(e, sizes)).map_or(main.1, |v| v.round() as u32);
        let Ok(prog) = crate::gl::program(vert, &hook_fragment(hook)) else { return false };
        let Some(out) = target(w, h, true) else {
            unsafe { gl::DeleteProgram(prog) };
            return false;
        };
        let at = |name: &str| crate::gl::uniform_location(prog, name);
        let binds = hook.binds.iter().map(|b|
            (b.clone(), at(&format!("{b}_raw")), at(&format!("{b}_size")), at(&format!("{b}_pt"))))
            .collect();
        let save = hook.save.clone().unwrap_or_else(|| "MAIN".into());
        sizes.insert(save.clone(), (w, h));
        self.passes.push(Pass { prog, binds, retro: None, out, save });
        true
    }

    fn add_retroarch(&mut self, src: &str, vert: &str, out: (u32, u32)) -> bool {
        let Ok(prog) = crate::gl::program(vert, &retroarch_fragment(src)) else { return false };
        let Some(target) = target(out.0, out.1, false) else {
            unsafe { gl::DeleteProgram(prog) };
            return false;
        };
        let at = |name: &str| crate::gl::uniform_location(prog, name);
        let retro = Some([at("TextureSize"), at("InputSize"), at("OutputSize")]);
        let binds = vec![("MAIN".into(), at("Texture"), -1, -1)];
        self.passes.push(Pass { prog, binds, retro, out: target, save: "MAIN".into() });
        true
    }

    /// Runs every pass from `source` (the game texture, read with linear
    /// filtering as mpv and RetroArch read it); the final texture.
    pub fn run(&self, source: gl::types::GLuint, quad: &crate::quad::Quad) -> gl::types::GLuint {
        let mut textures: HashMap<&str, (gl::types::GLuint, u32, u32)> = HashMap::new();
        textures.insert("MAIN", (source, self.source.0, self.source.1));
        unsafe {
            let mut fbo = 0;
            let mut viewport = [0i32; 4];
            gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fbo);
            gl::GetIntegerv(gl::VIEWPORT, viewport.as_mut_ptr());
            let blend = gl::IsEnabled(gl::BLEND) == gl::TRUE;
            let scissor = gl::IsEnabled(gl::SCISSOR_TEST) == gl::TRUE;
            gl::Disable(gl::BLEND);
            gl::Disable(gl::SCISSOR_TEST);
            gl::BindTexture(gl::TEXTURE_2D, source);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
            for pass in &self.passes {
                gl::BindFramebuffer(gl::FRAMEBUFFER, pass.out.fbo);
                gl::Viewport(0, 0, pass.out.w as i32, pass.out.h as i32);
                gl::UseProgram(pass.prog);
                let mut input = (0, 0);
                for (unit, (name, raw, size, pt)) in pass.binds.iter().enumerate() {
                    let Some(&(tex, w, h)) = textures.get(name.as_str()) else { continue };
                    gl::ActiveTexture(gl::TEXTURE0 + unit as u32);
                    gl::BindTexture(gl::TEXTURE_2D, tex);
                    gl::Uniform1i(*raw, unit as i32);
                    gl::Uniform2f(*size, w as f32, h as f32);
                    gl::Uniform2f(*pt, 1.0 / w as f32, 1.0 / h as f32);
                    input = (w, h);
                }
                if let Some([texture_size, input_size, output_size]) = pass.retro {
                    gl::Uniform2f(texture_size, input.0 as f32, input.1 as f32);
                    gl::Uniform2f(input_size, input.0 as f32, input.1 as f32);
                    gl::Uniform2f(output_size, pass.out.w as f32, pass.out.h as f32);
                }
                quad.draw();
                textures.insert(pass.save.as_str(), (pass.out.tex, pass.out.w, pass.out.h));
            }
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, source);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo as u32);
            gl::Viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
            if blend { gl::Enable(gl::BLEND); }
            if scissor { gl::Enable(gl::SCISSOR_TEST); }
        }
        textures.get("MAIN").map_or(source, |t| t.0)
    }

    pub fn release(&mut self) {
        for pass in self.passes.drain(..) {
            unsafe { gl::DeleteProgram(pass.prog) };
            drop_target(&pass.out);
        }
    }

    /// The context is gone: nothing to delete.
    pub fn forget(&mut self) {
        self.passes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anime4k_files_parse_into_their_passes() {
        let restore = parse_hooks(RESTORE_S);
        assert_eq!(restore.len(), 4);
        assert_eq!(restore[3].binds, ["MAIN", "conv2d_2_tf"]);
        assert_eq!(restore[3].save.as_deref(), Some("MAIN"));
        let up = parse_hooks(UPSCALE_S);
        assert_eq!(up.len(), 5);
        assert_eq!(up[4].width.as_deref(), Some("conv2d_last_tf.w 2 *"));
        assert!(up[4].code.contains("vec4 hook()"));
        assert!(!up[0].code.contains("//!"), "directives are not code");
        assert_eq!(parse_hooks(RESTORE_M).last().unwrap().binds.len(), 8, "MAIN and seven layers");
        assert!(parse_hooks(UPSCALE_M).iter().all(|p| p.binds.len() <= 8), "8 texture units at most");
    }

    #[test]
    fn sizes_follow_mpvs_expressions() {
        let mut sizes = HashMap::new();
        sizes.insert("conv2d_last_tf".to_owned(), (240, 160));
        assert_eq!(rpn("conv2d_last_tf.w 2 *", &sizes), Some(480.0));
        assert_eq!(rpn("conv2d_last_tf.h", &sizes), Some(160.0));
        assert_eq!(rpn("nothing.w", &sizes), None);
    }

    #[test]
    fn a_pass_reads_its_textures_through_mpvs_macros() {
        let pass = &parse_hooks(UPSCALE_S)[4];
        let src = hook_fragment(pass);
        assert!(src.contains("#define MAIN_tex(p) texture(MAIN_raw, p)"));
        assert!(src.contains("#define conv2d_last_tf_pt") || src.contains("uniform vec2 conv2d_last_tf_pt;"));
        assert!(src.ends_with("void main() { slot_out = hook(); }\n"));
    }
}
