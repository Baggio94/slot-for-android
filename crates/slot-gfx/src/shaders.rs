use crate::surface::GfxError;

const VERT_PREAMBLE: &str = "#version 330 core\n#define attribute in\n#define varying out\n";

const FRAG_PREAMBLE: &str = "#version 330 core\n#define varying in\n\
                             #define texture2D texture\nout vec4 FRAG_COLOR;\n";

const VERT_PREAMBLE_ES: &str = "";
const FRAG_PREAMBLE_ES: &str = "#define FRAG_COLOR gl_FragColor\n";

pub fn program(vert: &str, frag: &str) -> Result<gl::types::GLuint, GfxError> {
    let (vp, fp) = match crate::gl::es() {
        true => (VERT_PREAMBLE_ES, FRAG_PREAMBLE_ES),
        false => (VERT_PREAMBLE, FRAG_PREAMBLE),
    };
    crate::gl::program(&format!("{vp}{vert}"), &format!("{fp}{frag}"))
}

pub const RECT_VERT: &str = r#"
attribute vec2 a_pos;
uniform vec4 u_rect;
uniform vec2 u_target;
varying vec2 v_uv;
void main() {
    v_uv = a_pos;
    vec2 p = (u_rect.xy + a_pos * u_rect.zw) / u_target;
    gl_Position = vec4(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, 0.0, 1.0);
}
"#;

pub const SPRITE_VERT: &str = r#"
attribute vec2 a_pos;
uniform vec4 u_rect;
uniform vec2 u_target;
uniform vec2 u_turn;
varying vec2 v_uv;
void main() {
    v_uv = a_pos;
    vec2 mid = u_rect.zw * 0.5;
    vec2 local = a_pos * u_rect.zw - mid;
    vec2 turned = vec2(u_turn.x * local.x - u_turn.y * local.y,
                       u_turn.y * local.x + u_turn.x * local.y);
    vec2 p = (u_rect.xy + a_pos * u_rect.zw + (turned - local)) / u_target;
    gl_Position = vec4(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, 0.0, 1.0);
}
"#;

/// Shared by the LCD Effect caches below.
const PT_HEAD: &str = r#"
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
float pt_hash(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}
"#;

/* Pixel Transparency's drop shadow cache, as the original's shadow cache
   pass (v3.0): one texel per game pixel and x-phase (three across), the
   three y-phases in r, g and b. The blur's taps fall in this cell or the
   next one along each axis, depending on where the screen pixel sits in
   the cell (before fract(r), between, after 1 - fract(r)), so every value
   the 3x3 blur can take in a cell is worked out here once a frame, and
   the screen pass reads one. Cells around the game (u_pad) are kept too,
   as the shadow is offset. */
pub const PT_CACHE_FRAG: &str = r#"
uniform sampler2D u_game;
uniform vec2 u_src;
uniform vec4 u_pic;
uniform vec2 u_pt_r;
uniform float u_pad;
float pt_caster(vec2 cell) {
    if (any(lessThan(cell, floor(u_pic.xy * u_src + 0.5)))
        || any(greaterThanEqual(cell, floor(u_pic.zw * u_src + 0.5))))
        return 1.0; // screen edge shadow
    return 1.0 - dot(texture2D(u_game, (cell + 0.5) / u_src).rgb, vec3(0.2126, 0.7152, 0.0722));
}
// The left, middle and right taps along row y.
vec3 pt_row(vec3 xs, float y) {
    return vec3(pt_caster(vec2(xs.x, y)), pt_caster(vec2(xs.y, y)), pt_caster(vec2(xs.z, y)));
}
void main() {
    vec2 at = floor(gl_FragCoord.xy);
    float phase = at.x - 3.0 * floor(at.x / 3.0);
    vec2 cell = vec2(floor(at.x / 3.0), at.y) - u_pad;
    vec2 whole = floor(u_pt_r);
    vec2 lower_first = step(u_pt_r - whole, vec2(0.5));
    float lx = phase > 1.5 ? 1.0 : (phase > 0.5 ? lower_first.x : 0.0);
    float ux = phase > 1.5 ? 1.0 : (phase > 0.5 ? 1.0 - lower_first.x : 0.0);
    vec3 xs = vec3(cell.x - whole.x - 1.0 + lx, cell.x, cell.x + whole.x + ux);
    vec3 r0 = pt_row(xs, cell.y - whole.y - 1.0);
    vec3 r1 = pt_row(xs, cell.y - whole.y);
    vec3 mid = pt_row(xs, cell.y);
    vec3 r3 = pt_row(xs, cell.y + whole.y);
    vec3 r4 = pt_row(xs, cell.y + whole.y + 1.0);
    const vec3 edge = vec3(1.0, 2.0, 1.0);
    float m = dot(mid, vec3(2.0, 4.0, 2.0));
    float v0 = (m + dot(r0, edge) + dot(r3, edge)) / 16.0;
    float v1 = lower_first.y > 0.5 ? (m + dot(r1, edge) + dot(r3, edge)) / 16.0
                                   : (m + dot(r0, edge) + dot(r4, edge)) / 16.0;
    float v2 = (m + dot(r1, edge) + dot(r4, edge)) / 16.0;
    FRAG_COLOR = vec4(v0, v1, v2, 1.0);
}
"#;

/* Sunlight Shimmer's light cache, as the original's (v3.0): the rainbow
   tint and the glare over the picture, 256x256, its first and last texel
   centres on the picture's edges. Drawn again only when the light moves. */
pub const LIGHT_CACHE_FRAG: &str = r#"
uniform vec2 u_light;      // light position (0..1 over the picture)
uniform float u_aspect;    // picture width / height on screen
float sun_smooth(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(pt_hash(i), pt_hash(i + vec2(1.0, 0.0)), f.x),
               mix(pt_hash(i + vec2(0.0, 1.0)), pt_hash(i + vec2(1.0, 1.0)), f.x), f.y);
}
float sun_film(vec2 uv) {
    vec2 p = uv * 0.6 * 8.0;
    return sun_smooth(p) * 0.5 + sun_smooth(p * 2.0) * 0.25 + sun_smooth(p * 4.0) * 0.125;
}
vec3 sun_qwp(float g) {
    vec3 t = vec3(0.0); float k;
    k = cos(0.031415927 * g); t += vec3(0.0, 0.0, 0.026075) * k * k;
    k = cos(0.028399638 * g); t += vec3(0.141223, 0.0, 0.368894) * k * k;
    k = cos(0.025874228 * g); t += vec3(0.0, 0.480861, 0.473477) * k * k;
    k = cos(0.023809524 * g); t += vec3(0.081095, 0.812848, 0.339825) * k * k;
    k = cos(0.021991149 * g); t += vec3(0.783356, 0.793891, 0.134941) * k * k;
    k = cos(0.020462441 * g); t += vec3(0.972432, 0.423991, 0.215476) * k * k;
    k = cos(0.019138570 * g); t += vec3(0.648322, 0.114430, 0.0) * k * k;
    return t / max(max(t.r, max(t.g, t.b)), 0.001);
}
void main() {
    vec2 uv = (floor(gl_FragCoord.xy) + 0.5 - 0.5) / 255.0;
    vec2 to_light = (uv - u_light) * vec2(u_aspect, 1.0);
    float ang = clamp(length(to_light) * 1.8, 0.0, 1.4);
    float sr = sin(ang) / 1.5;
    float g = 275.0 * sqrt(1.0 - sr * sr);
    g *= 1.0 + (sun_film(vec2(uv.x * u_aspect, uv.y)) - 0.4375) * 0.5;
    float glare = exp(-dot(to_light, to_light) / (2.0 * 0.25 * 0.25)) * 0.15;
    FRAG_COLOR = vec4(sun_qwp(g), glare);
}
"#;

/* Pixel Transparency's paper grain (r) and the final dither (g), one
   texel a screen pixel of the picture: drawn again only when the picture
   changes size on screen. */
pub const GRAIN_FRAG: &str = r#"
uniform vec2 u_size;       // picture size on screen, in pixels
float pt_paper(vec2 uv) {
    vec2 p = uv * 0.25 * 512.0;
    return pt_hash(p) * 0.5 + pt_hash(p * 2.0) * 0.25 + pt_hash(p * 4.0) * 0.125;
}
void main() {
    vec2 at = floor(gl_FragCoord.xy);
    FRAG_COLOR = vec4(pt_paper((at + 0.5) / u_size), pt_hash(at + 0.5), 0.0, 1.0);
}
"#;

/* The game, copied texel for texel into a texture the GPU filters
   bilinearly: the shaders that read the game bilinear (sharp bilinear,
   zfast CRT, CRT-Pi, Fakelottes, CRT Geom Deluxe's glow) then take one
   filtered read instead of four reads mixed by hand. */
pub const COPY_FRAG: &str = r#"
uniform sampler2D u_game;
uniform vec2 u_src;
void main() {
    FRAG_COLOR = texture2D(u_game, (floor(gl_FragCoord.xy) + 0.5) / u_src);
}
"#;

/* CRT GDV Mini Ultra's glow, once per game pixel: the screen pass used to
   read these 21 game pixels for every screen pixel, though the glow only
   depends on the game pixel it falls in. Stored before its 0.35 strength,
   for precision. */
pub const GDV_GLOW_FRAG: &str = r#"
uniform sampler2D u_game;
uniform vec2 u_src;
vec3 gdv_tap(vec2 at) {
    return texture2D(u_game, at).rgb;
}
void main() {
    vec2 tc = (floor(gl_FragCoord.xy) + 0.5) / u_src;
    // Upstream squares nothing here (its "c*c;" lines are no-ops).
    vec2 b = vec2(1.0)/u_src;
    vec3 c11 = gdv_tap(tc);
    vec3 ring1 = gdv_tap(tc+vec2(-b.x,0.0)) + gdv_tap(tc+vec2(b.x,0.0))
        + gdv_tap(tc+vec2(-b.x,-b.y)) + gdv_tap(tc+vec2(b.x,b.y))
        + gdv_tap(tc+vec2(-b.x,b.y)) + gdv_tap(tc+vec2(b.x,-b.y))
        + gdv_tap(tc+vec2(0.0,-b.y)) + gdv_tap(tc+vec2(0.0,b.y));
    vec3 ring2 = gdv_tap(tc+vec2(-2.0*b.x,0.0)) + gdv_tap(tc+vec2(2.0*b.x,0.0))
        + gdv_tap(tc+vec2(-2.0*b.x,-b.y)) + gdv_tap(tc+vec2(-b.x,-2.0*b.y))
        + gdv_tap(tc+vec2(b.x,2.0*b.y)) + gdv_tap(tc+vec2(2.0*b.x,b.y))
        + gdv_tap(tc+vec2(-2.0*b.x,b.y)) + gdv_tap(tc+vec2(-b.x,2.0*b.y))
        + gdv_tap(tc+vec2(b.x,-2.0*b.y)) + gdv_tap(tc+vec2(2.0*b.x,-b.y))
        + gdv_tap(tc+vec2(0.0,-2.0*b.y)) + gdv_tap(tc+vec2(0.0,2.0*b.y));
    FRAG_COLOR = vec4((3.0*c11 + 2.5*ring1 + 1.5*ring2)/45.0, 1.0);
}
"#;

pub fn pt_frag(body: &str) -> String {
    format!("{PT_HEAD}{body}")
}

pub const GAME_FRAG: &str = r#"
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
uniform sampler2D u_game;
uniform sampler2D u_hires;
uniform sampler2D u_paper;
uniform vec2 u_src;
uniform vec4 u_uv;
uniform float u_bright;
uniform float u_mode;
uniform float u_colour_correct;
uniform float u_paper_size;
uniform vec4 u_rect;
uniform vec2 u_fbo;
uniform vec4 u_pic;
varying vec2 v_uv;
const vec3 offsets = vec3(3.141592654) * vec3(1.0 / 2.0, 1.0 / 2.0 - 2.0 / 3.0, 1.0 / 2.0 - 4.0 / 3.0);
const vec3 luma_coeff = vec3(0.2126, 0.7152, 0.0722);
float dot_weight(vec2 f, vec2 o, float bloom) {
    // RetroArch Dot defaults: gamma 2.4, shine 0.05, blend 0.65,
    // soft 0.0. With soft 0.0, all nine taps share one source sample.
    return exp(-2.4 * length(f - (o + vec2(0.5))) * bloom);
}

/* GLES2 ports of libretro/glsl-shaders (see licenses/shaders.txt).
   One program per mode, built the first time the mode is shown.
   Multi-pass AGB/AGS and textured presets are analytic/native-scale ports
   requiring visual QA before claiming byte-for-byte RetroArch parity. */
float lcd_integral(float z, bool y) {
    float a = y ? 0.0 : -2.0/3.0;
    float b = y ? -4.0/5.0 : -1.0/5.0;
    float d = y ? 2.0/7.0 : 4.0/7.0;
    float e = y ? 4.0/9.0 : -1.0/9.0;
    float g = y ? -4.0/11.0 : -2.0/11.0;
    float z2 = z*z;
    return z*(1.0+z2*(a+z2*(b+z2*(d+z2*(e+z2*(g+z2/13.0))))));
}
float lcd_smear(float x, float dx, float radius, bool y) {
    dx = max(dx, 0.0001);
    float lo = clamp((x-dx*0.5)/radius, -1.0, 1.0);
    float hi = clamp((x+dx*0.5)/radius, -1.0, 1.0);
    return radius*(lcd_integral(hi,y)-lcd_integral(lo,y))/dx;
}
vec3 gba_color(vec3 c) {
    // hunterk/Pokefan531 official gba-color default matrix and gamma.
    c = pow(max(c,vec3(0.0)),vec3(3.2));
    c = mat3(0.82,0.125,0.195, 0.24,0.665,0.075, -0.06,0.21,0.73)*(c*0.94);
    return pow(clamp(c,0.0,1.0),vec3(1.0/2.2));
}
vec3 simpletex(vec3 c, vec2 f, float bias) {
    // jdgleaver simpletex_lcd with Russ (RetroGameCorps) settings, Siham
    // 2026-11-04: Background Intensity 0.80, Darken Colours 0.25, Darken
    // Grid 0.15, Grid Intensity 0.65. Grid Width 0.65 and Grid Bias (1.0
    // GB/GBC, 0.6 GBA) as in 2.2.0: RetroArch's defaults (1.0, 0.0) lay
    // the light grid over dark colours and wash the picture out (Siham
    // 2026-11-10, 2.2.1-beta1 on device).
    c = pow(c, vec3(1.0 + 0.25));
    vec2 d = abs(f - 0.5);
    float x2 = max(d.x, d.y);
    x2 = x2 * x2;
    float x4 = x2 * x2;
    float w = 48.0 * (x4 - (8.0 / 3.0) * x4 * x2);
    w = w * (w + (1.0 - w) * 0.65) * 0.65;
    float luma = dot(c, luma_coeff);
    w = w * (luma + (1.0 - luma) * (1.0 - bias));
    c = mix(c, vec3(1.0 - 0.15), w);
    // The paper's grain as big next to the game as on the KPA's 640 rows,
    // on any screen: one texel per screen pixel made it vanish on denser
    // ones like the Retroid Pocket Classic's 1080 (Siham 2026-11-10).
    // u_fbo: screen pixels per canvas pixel (480 rows), 4/3 on the KPA.
    float grain = max(1.0, u_fbo.y * 0.75);
    vec3 bg = texture2D(u_paper, (floor(gl_FragCoord.xy / grain) + 0.5) / u_paper_size).rgb * c;
    return mix(c, bg, dot(c, luma_coeff) * 0.80);
}
vec3 lcd_grid_v2(vec2 uv, bool bgr) {
    // cgwg LCD Grid V2 coefficient integration, rewritten without GLSL130
    // array constructors (unsupported on GLES2). Same official preset
    // parameters: gain=1.5, gamma=2.2, RGB matrix diagonal=0.75.
    vec2 xy = uv*u_src;
    vec2 tli = floor(xy-vec2(0.4999));
    // Game texels per screen pixel, as upstream's range/texelSize: each
    // screen pixel integrates the subpixel profile over its own width.
    vec2 range = (u_uv.zw*u_src)/(u_rect.zw*u_fbo);
    float sub = (xy.x-0.4999-tli.x)*3.0;
    float dx = range.x*3.0;
    vec3 left = vec3(lcd_smear(sub+1.0,dx,1.5,false),
                     lcd_smear(sub,dx,1.5,false),
                     lcd_smear(sub-1.0,dx,1.5,false));
    vec3 right = vec3(lcd_smear(sub-2.0,dx,1.5,false),
                      lcd_smear(sub-3.0,dx,1.5,false),
                      lcd_smear(sub-4.0,dx,1.5,false));
    if (bgr) { left=left.bgr; right=right.bgr; }
    float py = xy.y-0.4999-tli.y;
    float top = lcd_smear(py,range.y,0.63,true);
    float bot = lcd_smear(py-1.0,range.y,0.63,true);
    vec2 pixel = vec2(1.0)/u_src;
    vec2 pos = (tli+vec2(0.5))*pixel;
    vec3 tl=pow(1.5*texture2D(u_game,pos).rgb,vec3(2.2));
    vec3 tr=pow(1.5*texture2D(u_game,pos+vec2(pixel.x,0.0)).rgb,vec3(2.2));
    vec3 bl=pow(1.5*texture2D(u_game,pos+vec2(0.0,pixel.y)).rgb,vec3(2.2));
    vec3 br=pow(1.5*texture2D(u_game,pos+pixel).rgb,vec3(2.2));
    vec3 weighted=(tl*left+tr*right)*top+(bl*left+br*right)*bot;
    return pow(max(weighted*0.75,vec3(0.0)),vec3(1.0/2.2));
}
/* endrift AGB/AGS-001 are two passes upstream: the game at 4x, nearest,
   through the 4x4 subpixel mask, then that picture scaled to the screen
   with LINEAR filtering. sub_4x is one texel of the 4x picture and
   mgba_pass filters four of them, so the mask keeps even columns and rows
   at any scale (at exactly 4x it is the first pass as it is). */
vec3 mgba_sub_4x(vec2 i, bool ags) {
    vec2 m = mod(i, 4.0);
    vec3 c = texture2D(u_game, (floor(i/4.0) + vec2(0.5))/u_src).rgb;
    c = ags ? pow(c, vec3(1.6)) : pow(c*0.8, vec3(1.8)) + vec3(0.16);
    vec3 mask = m.x < 0.5 ? vec3(1.0,0.2,0.2)
        : m.x < 1.5 ? vec3(0.2,1.0,0.2)
        : m.x < 2.5 ? vec3(0.2,0.2,1.0) : vec3(0.4);
    return c*mask*(m.y > 2.5 ? (ags ? 0.9 : 0.8) : 1.0);
}
vec3 mgba_pass(vec2 uv, bool ags) {
    vec2 q = uv*u_src*4.0 - vec2(0.5);
    vec2 b = floor(q);
    vec2 t = fract(q);
    return mix(mix(mgba_sub_4x(b, ags), mgba_sub_4x(b + vec2(1.0,0.0), ags), t.x),
               mix(mgba_sub_4x(b + vec2(0.0,1.0), ags), mgba_sub_4x(b + vec2(1.0), ags), t.x), t.y);
}
uniform sampler2D u_game_linear;  // the game, bilinear filtered (COPY_FRAG)
uniform sampler2D u_gdv_glow;     // GDV glow per game pixel (GDV_GLOW_FRAG)
// Worked out once per game pixel ahead of this pass (CACHE_PASS below):
uniform sampler2D u_edges;   // xBR-lv2 / SABR: which corners to blend
uniform sampler2D u_gamma;   // crt-aperture / Geom Deluxe: the game in linear light
uniform float u_cached;      // 1 when this effect's cache is drawn, else work it out here
vec3 nearest_bilinear(vec2 uv) {
    // The effects that read the game bilinear have a copy of it the GPU
    // filters itself (the RetroArch presets' LINEAR filter).
    if (u_mode > 11.5 && u_mode < 17.5 && !(u_mode > 14.5 && u_mode < 15.5))
        return texture2D(u_game_linear, uv).rgb;
    // GPU sampler remains NEAREST for the other presets. Manually emulate
    // the RetroArch sharp-bilinear LINEAR filter using four source texels.
    vec2 coord=uv*u_src-vec2(0.5);
    vec2 base=floor(coord);
    vec2 t=fract(coord);
    vec2 stepuv=vec2(1.0)/u_src;
    vec2 pos=(base+vec2(0.5))*stepuv;
    vec3 a=texture2D(u_game,pos).rgb;
    vec3 b=texture2D(u_game,pos+vec2(stepuv.x,0.0)).rgb;
    vec3 c=texture2D(u_game,pos+vec2(0.0,stepuv.y)).rgb;
    vec3 d=texture2D(u_game,pos+stepuv).rgb;
    return mix(mix(a,b,t.x),mix(c,d,t.x),t.y);
}
/* Pixel AA (after fishku's pixel_aa / shimmerless): at a non-integer
   scale every game pixel keeps one width and only the screen pixel that
   straddles two of them blends them, by area, in linear light. At a whole
   scale it is exactly the nearest texel. No pass and no name of its own:
   the LCD shaders read the game through it (Siham 2026-11-04). */
vec3 aa_tap(vec2 at) {
    return pow(texture2D(u_game, at).rgb, vec3(2.2));
}
vec3 pixel_aa(vec2 uv) {
    vec2 s = (u_rect.zw*u_fbo)/(u_uv.zw*u_src);
    vec2 texel = uv*u_src - vec2(0.5);
    vec2 base = floor(texel);
    vec2 t = clamp((fract(texel) - vec2(0.5))*s + vec2(0.5), 0.0, 1.0);
    vec2 step1 = vec2(1.0)/u_src;
    vec2 at = (base + vec2(0.5))*step1;
    /* Same picture, less work: most screen pixels lie inside one game
       pixel (t is 0 or 1 on both axes) and are that texel as it is; one
       straddling a single edge blends two texels, a corner all four. */
    vec2 edge = step(vec2(0.001), t) * step(t, vec2(0.999));
    if (edge.x + edge.y < 0.5) {
        return texture2D(u_game, at + step1*step(vec2(0.5), t)).rgb;
    }
    if (edge.y < 0.5) {
        vec2 row = at + vec2(0.0, step1.y*step(0.5, t.y));
        return pow(mix(aa_tap(row), aa_tap(row + vec2(step1.x, 0.0)), t.x), vec3(1.0/2.2));
    }
    if (edge.x < 0.5) {
        vec2 column = at + vec2(step1.x*step(0.5, t.x), 0.0);
        return pow(mix(aa_tap(column), aa_tap(column + vec2(0.0, step1.y)), t.y), vec3(1.0/2.2));
    }
    vec3 a = aa_tap(at);
    vec3 b = aa_tap(at + vec2(step1.x, 0.0));
    vec3 c = aa_tap(at + vec2(0.0, step1.y));
    vec3 d = aa_tap(at + step1);
    return pow(mix(mix(a, b, t.x), mix(c, d, t.x), t.y), vec3(1.0/2.2));
}
/* hunterk's fakelottes: crt-lottes' shadow mask and gamma with a sine
   scanline, in one cheap pass. Its defaults (mask 1, maskDark 0.5,
   maskLight 1.5, CRT gamma 2.5, monitor gamma 2.2, scanline 0.95 + 0.40
   sine), bilinear input as its preset; without its curvature, as CRT
   Geom Deluxe here. */
vec3 fakelottes_mask(vec2 pos) {
    vec3 mask = vec3(0.5);
    float line = 1.5;
    float odd = 0.0;
    if (fract(pos.x*0.166666666) < 0.5) odd = 1.0;
    if (fract((pos.y + odd)*0.5) < 0.5) line = 0.5;
    pos.x = fract(pos.x*0.333333333);
    if (pos.x < 0.333) mask.r = 1.5;
    else if (pos.x < 0.666) mask.g = 1.5;
    else mask.b = 1.5;
    return mask*line;
}
vec3 fakelottes(vec2 uv) {
    vec3 res = pow(nearest_bilinear(uv), vec3(2.5));
    res *= fakelottes_mask(gl_FragCoord.xy*1.0001);
    // Its sine has no horizontal part at the defaults (SINE_COMP_A 0).
    float sine = sin((uv.y - 0.25/u_src.y)*6.283*u_src.y);
    res *= 0.95 + 0.40*sine;
    return pow(max(res, vec3(0.0)), vec3(1.0/2.2));
}
/* EasyMode's crt-aperture (GPL), already one pass: Gaussian glow, two
   Lanczos sharpness taps, beam-shaped scanlines and an aperture-grille
   mask, at its defaults. */
vec3 game_gamma(vec2 at) {
    // pow(texel, 2.4) as GLSL defines pow, exp2(y*log2(x)): Mesa's llvmpipe
    // miscompiles a pow straight on a texture fetch in this program.
    vec3 t = texture2D(u_game, at).rgb;
    return exp2(log2(max(t, vec3(1e-5)))*2.4);
}
vec3 aperture_tap(vec2 at) {
#ifndef CACHE_PASS
    if (u_cached > 0.5) return texture2D(u_gamma, at).rgb;
#endif
    return game_gamma(at);
}
vec3 aperture_blur(vec3 a, vec3 b, vec3 c, float dist, float rad) {
    vec3 x = vec3(dist - 1.0, dist, dist + 1.0)/rad;
    vec3 w = exp2(x*x*-1.0);
    return (a*w.x + b*w.y + c*w.z)/(w.x + w.y + w.z);
}
vec3 aperture_gaussian(vec2 co) {
    vec2 dx = vec2(1.0/u_src.x, 0.0);
    vec2 dy = vec2(0.0, 1.0/u_src.y);
    vec2 pix = co*u_src;
    vec2 tc = (floor(pix) + 0.5)/u_src;
    vec2 dist = (fract(pix) - 0.5)*-1.0;
    vec3 l0 = aperture_blur(aperture_tap(tc-dy-dx), aperture_tap(tc-dy), aperture_tap(tc-dy+dx), dist.x, 0.5);
    vec3 l1 = aperture_blur(aperture_tap(tc-dx), aperture_tap(tc), aperture_tap(tc+dx), dist.x, 0.5);
    vec3 l2 = aperture_blur(aperture_tap(tc+dy-dx), aperture_tap(tc+dy), aperture_tap(tc+dy+dx), dist.x, 0.5);
    return aperture_blur(l0, l1, l2, dist.y, 0.5);
}
vec3 aperture_lanczos(vec2 co, float sharp) {
    vec2 size = vec2(u_src.x*sharp, u_src.y);
    vec2 dx = vec2(1.0/size.x, 0.0);
    vec2 pix = co*size - vec2(0.5, 0.0);
    vec2 tc = (floor(pix) + vec2(0.5, 0.0))/size;
    float d = fract(pix.x);
    vec4 coef = 3.141592653589*vec4(d + 1.0, d, d - 1.0, d - 2.0);
    coef = max(abs(coef), vec4(1e-5));
    coef = 2.0*sin(coef)*sin(coef/2.0)/(coef*coef);
    coef /= dot(coef, vec4(1.0));
    vec3 c1 = aperture_tap(tc);
    vec3 c2 = aperture_tap(tc + dx);
    return c1*(coef.x + coef.y) + c2*(coef.z + coef.w);
}
vec3 crt_aperture(vec2 uv) {
    // Screen pixels per source row (crt_out_px().y, defined further down).
    float scale = floor((u_rect.w*u_fbo.y)/(u_uv.w*u_src.y) + 0.001);
    float offset = 1.0/scale*0.5;
    if (mod(scale, 2.0) > 0.5) offset = 0.0;
    vec2 co = (uv*u_src - vec2(0.0, offset))/u_src;
    vec3 glow = aperture_gaussian(co);
    vec3 soft = aperture_lanczos(co, 1.0);
    vec3 sharp = aperture_lanczos(co, 3.0);
    vec3 col = sqrt(sharp*soft);
    // Scanline weight, beam from 0.5 to 1.5 rows, shape 1.5.
    vec3 beam = mix(vec3(0.5), vec3(1.5), pow(soft, vec3(1.0/1.5)));
    vec3 xm = 2.0/beam;
    vec3 xo = xm*0.5;
    col *= smoothstep(0.0, 1.0, 1.0 - abs(fract(co.y*u_src.y)*xm - xo))*xo;
    glow = clamp(glow - col, 0.0, 1.0);
    col += glow*glow*0.1;
    // Two-colour aperture grille, one screen pixel per stripe, strength 0.3.
    vec3 mask = mod(floor(gl_FragCoord.x), 2.0) < 0.5 ? vec3(1.0, 0.0, 1.0) : vec3(0.0, 1.0, 0.0);
    col = mix(col, col*mask*2.0, 0.3);
    col += glow*0.05;
    return pow(max(col*1.5, vec3(0.0)), vec3(1.0/2.4));
}
/* CRT presets for the home consoles (libretro glsl/slang-shaders, GPL; see
   licenses/shaders.txt). One pass each: slot. draws the game in a single
   pass, so multi-pass parts are folded in or approximated as noted. */
vec2 crt_out_px() {
    // Screen pixels per source texel, per axis.
    return (u_rect.zw*u_fbo)/(u_uv.zw*u_src);
}
float crtpi_weight(float dist) {
    return max(1.0-dist*dist*6.0, 0.12);
}
vec3 gdv_mask(vec2 pos) {
    // Mask 11 (the Trinitron preset's), maskDark 0.0.
    pos = floor(pos);
    return fract(pos.x/3.0) > 0.333 ? vec3(1.0) : vec3(0.0);
}
float gdv_sw(float x, float l) {
    return exp2(-10.0*pow(max(x,0.0), mix(1.5,2.0,l)));
}
vec3 gdv_tap(vec2 at) {
    return texture2D(u_game, at).rgb;
}
vec3 gdv_glow(vec2 tc) {
    // Worked out once per game pixel ahead of this pass (GDV_GLOW_FRAG).
    return texture2D(u_gdv_glow, tc).rgb*0.35;
}
vec3 gdv_saturation(vec3 c) {
    float l = length(c)*0.5775;
    vec3 w = vec3(0.3,0.6,0.1);
    if (l < 0.5) w = w*w + w*w;
    return mix(vec3(dot(c,w)), c, 1.2);
}
vec3 crt_gdv_mini_ultra(vec2 uv) {
    // guest(r) + Dr. Venom crt-gdv-mini-ultra, Trinitron preset defaults,
    // CurvatureX/Y 0 (Siham 2026-11-04).
    vec2 ps = vec2(1.0)/u_src;
    vec2 at = uv*u_src;
    vec2 fp = fract(at);
    vec2 pc = floor(at)*ps + 0.5*ps;
    vec3 ul = gdv_tap(pc);
    vec3 ur = gdv_tap(pc+vec2(ps.x,0.0));
    vec3 dl = gdv_tap(pc+vec2(0.0,ps.y));
    vec3 dr = gdv_tap(pc+ps);
    float lx = pow(fp.x, 2.5);
    float rx = pow(1.0-fp.x, 2.5);
    vec3 c1 = (ur*lx + ul*rx)/(lx+rx);
    vec3 c2 = (dr*lx + dl*rx)/(lx+rx);
    float l1 = dot(c1, vec3(0.3,0.6,0.1));
    float l2 = dot(c2, vec3(0.3,0.6,0.1));
    c1 = 2.0*pow(c1,vec3(2.8)) - pow(c1,vec3(3.6));
    c2 = 2.0*pow(c2,vec3(2.8)) - pow(c2,vec3(3.6));
    vec3 mask = gdv_mask(gl_FragCoord.xy);
    c1 *= mix(mask, vec3(1.0), l1*0.4);
    c2 *= mix(mask, vec3(1.0), l2*0.4);
    vec3 c = c1*gdv_sw(fp.y,l1) + c2*gdv_sw(1.0-fp.y,l2);
    if (u_src.y >= 400.0) c = (c1+c2)/2.0;
    c = min(c, 1.0);
    float lum = dot(c, vec3(0.3,0.6,0.1));
    c = pow(max(c,vec3(0.0)), vec3(1.0/2.2));
    c += gdv_glow(pc);
    c *= mix(1.0, 1.2, lum);
    c = gdv_saturation(c);
    vec2 v = uv*(1.0-uv);
    c *= min(pow(max(v.x*v.y*45.0,0.0), 0.15), 1.0);
    return c;
}
vec3 geom_tex(vec2 at) {
    vec2 under = step(0.0,at)*step(0.0,vec2(1.0)-at);
#ifndef CACHE_PASS
    if (u_cached > 0.5) return texture2D(u_gamma, at).rgb*(under.x*under.y);
#endif
    return pow(texture2D(u_game,at).rgb, vec3(2.4))*(under.x*under.y);
}
/* The scanline profile, on the three colours shown (upstream also works
   out alpha). Its width terms depend only on the row's colour, so they
   are worked out once per row (geom_width) instead of once per sample. */
vec3 geom_width(vec3 col) {
    return 2.0 + 2.0*pow(col, vec3(4.0));
}
vec3 geom_weights(float dist, vec3 wid, vec3 isw, vec3 norm) {
    vec3 w = vec3(dist/0.3);
    return norm*exp(-pow(w*isw, wid));
}
vec3 geom_blur(vec2 at) {
    // Halation source: upstream blurs the frame in two extra passes; here a
    // gaussian of the same width from four bilinear taps and the centre.
    float wid = 2.0*u_src.x/320.0;
    vec2 o = vec2(0.6*wid)/u_src;
    vec3 b = 0.2*texture2D(u_game, at).rgb
        + 0.2*nearest_bilinear(at+vec2(-o.x,-o.y))
        + 0.2*nearest_bilinear(at+vec2(o.x,-o.y))
        + 0.2*nearest_bilinear(at+vec2(-o.x,o.y))
        + 0.2*nearest_bilinear(at+vec2(o.x,o.y));
    vec3 col = pow(b, vec3(2.4));
    vec2 c = min(at, vec2(1.0)-at)*vec2(1.0,0.75)/(2.0/320.0);
    vec2 e2c = exp(-c*c);
    c = (step(0.0,c)-vec2(0.5))*sqrt(vec2(1.0)-e2c)*(vec2(1.0)+vec2(0.1749)*e2c)+vec2(0.5);
    return col*c.x*c.y;
}
vec3 crt_geom_deluxe(vec2 xy) {
    // cgwg/DOLLS crt-geom-deluxe, curvature 0 (Siham 2026-11-04). Phosphor
    // persistence and raster bloom (two passes and a mipmap) are left out.
    vec2 cxy = min(xy, vec2(1.0)-xy)*vec2(1.0,0.75);
    cxy = vec2(0.01) - min(cxy, vec2(0.01));
    float cval = clamp((0.01-length(cxy))*1000.0, 0.0, 1.0);
    vec2 il = vec2(1.0, clamp(floor(u_src.y/200.0), 1.0, 2.0));
    vec2 one = il/u_src;
    vec2 rs = (xy*u_src - vec2(0.5))/il;
    float over = (1.0/crt_out_px().y)/il.y;
    vec2 ur = fract(rs);
    vec2 at = (floor(rs)*il + vec2(0.5))/u_src;
    vec4 k = 3.141592653589*vec4(1.0+ur.x, ur.x, 1.0-ur.x, 2.0-ur.x);
    k = max(abs(k), 1e-5);
    k = 2.0*sin(k)*sin(k/2.0)/(k*k);
    k /= dot(k, vec4(1.0));
    vec3 col = clamp(geom_tex(at+vec2(-one.x,0.0))*k.x + geom_tex(at)*k.y
        + geom_tex(at+vec2(one.x,0.0))*k.z + geom_tex(at+vec2(2.0*one.x,0.0))*k.w, 0.0, 1.0);
    vec3 col2 = clamp(geom_tex(at+vec2(-one.x,one.y))*k.x + geom_tex(at+vec2(0.0,one.y))*k.y
        + geom_tex(at+one)*k.z + geom_tex(at+vec2(2.0*one.x,one.y))*k.w, 0.0, 1.0);
    vec3 wid = geom_width(col), wid2 = geom_width(col2);
    vec3 isw = inversesqrt(0.5*wid), isw2 = inversesqrt(0.5*wid2);
    vec3 nrm = 1.4/(0.6+0.2*wid), nrm2 = 1.4/(0.6+0.2*wid2);
    // OVERSAMPLE: three samples a third of a screen row apart.
    vec3 w = geom_weights(ur.y, wid, isw, nrm);
    vec3 w2 = geom_weights(1.0-ur.y, wid2, isw2, nrm2);
    float y = ur.y + over/3.0;
    w = (w + geom_weights(y, wid, isw, nrm))/3.0;
    w2 = (w2 + geom_weights(abs(1.0-y), wid2, isw2, nrm2))/3.0;
    y -= 2.0*over/3.0;
    w += geom_weights(abs(y), wid, isw, nrm)/3.0;
    w2 += geom_weights(abs(1.0-y), wid2, isw2, nrm2)/3.0;
    vec3 m = col*w + col2*w2;
    vec3 blur = geom_blur(xy);
    m = mix(m, blur, 0.1)*cval;
    m = mix(m, blur, 0.1)*cval;
    // Aperture mask 1 (magenta/green), strength 0.4, brightboost 0.4.
    vec3 mask = mod(floor(gl_FragCoord.x), 2.0) < 0.5 ? vec3(1.0,0.0,1.0) : vec3(0.0,1.0,0.0);
    vec2 ts = crt_out_px();
    float fbright = 127.5/(ts.x*ts.y);
    float avg = mix(1.0-0.4*(1.0-0.4), 1.0, fbright);
    vec3 clow = vec3(0.6)*m + vec3(0.16)*m*m;
    float ifb = 1.0/fbright;
    vec3 chi = vec3(ifb*avg)*m - vec3(ifb-1.0)*clow;
    return pow(max(mix(clow, chi, mask), vec3(0.0)), vec3(1.0/2.2));
}
/* Grid as slot. original draws it: a dark band over the first third of
   each game pixel, left and top, darkening a screen pixel by the share of
   it the band covers. The band grows with the picture, so every line keeps
   a solid core at any scale; the colours come through Pixel AA. */
vec2 grid_band(vec2 x) {
    return floor(x)/3.0 + min(fract(x), vec2(1.0/3.0));
}
vec3 grid_line(vec2 uv, vec3 c) {
    vec2 hp = 0.5*(u_uv.zw*u_src)/(u_rect.zw*u_fbo);
    vec2 at = uv*u_src;
    vec2 edge = (grid_band(at + hp) - grid_band(at - hp))/(2.0*hp);
    return c*(1.0 - 0.75*max(edge.x, edge.y));
}
/* LCD Effect: Pixel Transparency / Sunlight Shimmer v3.0 by Matt Akins
   (Soft grid placement, author defaults), adapted with his permission.
   As in the original, the costly parts are worked out ahead in small
   caches (PT_CACHE_FRAG, LIGHT_CACHE_FRAG, GRAIN_FRAG) and this pass only
   reads them: the drop shadow once per game pixel, the light once when it
   moves, the paper grain and dither once per picture size. LCD_FX: 0 off,
   1 Pixel Transparency, 2 Sunlight Shimmer, fixed when the program is
   built. */
uniform vec2 u_shadow;     // shadow offset in reference pixels (3,3)
uniform sampler2D u_pt_cache;  // drop shadow, 3 x-phases by 3 y-phases a cell
uniform vec4 u_pt_geo;     // cache: padding (cells), size (texels)
uniform vec2 u_pt_r;       // blur radius, in game pixels
uniform sampler2D u_light_cache; // rainbow tint (rgb) and glare (a)
uniform sampler2D u_grain; // paper (r) and dither (g), one texel a screen pixel

float pt_luma(vec3 c) { return dot(c, vec3(0.2126, 0.7152, 0.0722)); }
// The shadow under native position `native`: the cache holds, for every
// game pixel, the nine values the 3x3 blur can take inside it (the taps
// cross into a neighbour at fract(r) and 1 - fract(r) along each axis).
float pt_shadow(vec2 native) {
    vec2 cell = floor(native);
    vec2 fr = fract(u_pt_r);
    vec2 ph = step(fr, native - cell) + step(1.0 - fr, native - cell);
    vec2 at = cell + u_pt_geo.xy;
    vec2 cells = vec2(u_pt_geo.z / 3.0, u_pt_geo.w);
    // Far outside the game: every tap is screen edge.
    if (any(lessThan(at, vec2(0.0))) || any(greaterThanEqual(at, cells))) return 1.0;
    vec3 t = texture2D(u_pt_cache, vec2(at.x * 3.0 + ph.x + 0.5, at.y + 0.5) / u_pt_geo.zw).rgb;
    return ph.y < 0.5 ? t.r : (ph.y < 1.5 ? t.g : t.b);
}
vec3 lcd_effect(vec3 lcd, vec2 uv) {
#if LCD_FX > 0
    vec2 out_px = u_rect.zw * u_fbo;          // picture size on screen
    vec2 texel = 1.0 / out_px;
    float scale = sqrt(out_px.x / 640.0 * out_px.y / 480.0);
    vec3 orig = texture2D(u_game, uv).rgb;
    float bright = pt_luma(orig);
    float alpha = clamp(0.20 * bright * 2.665, 0.0, 1.0);   // Bright mode, 0.20
    vec2 grain = texture2D(u_grain, v_uv).rg;
    vec3 bg = vec3(0.48) + (grain.r - 0.4375) * 0.065;
    // Drop shadow, 3 px at 640x480, blur 2, opacity 0.5, 9 taps.
    vec2 off = -u_shadow * scale * texel;
    vec2 native = (v_uv + off) * u_uv.zw * u_src + u_uv.xy * u_src;
    float s = pt_shadow(native) * 0.5;
    float dz = (1.0 - 0.90) * 0.5;
    s = smoothstep(0.0, dz, s) * s;
    bg = mix(bg, bg * 0.2, s);
    // Neutral backing tint (palette 3), intensity 1.
    vec3 pal = vec3(0.766, 0.73, 0.763);
    pal *= 0.675 / max(pal.r, max(pal.g, pal.b));
    bg = clamp(pal + mix(vec3(-1.0), vec3(1.0), bg), 0.0, 1.0);
    vec3 c = mix(lcd, bg, alpha);
#if LCD_FX > 1
    // Sunlight Shimmer: rainbow through the pixels, then glare on top.
    // The 256x256 light cache maps its first and last texel centres to
    // the picture's edges, as the original's.
    vec4 sun = texture2D(u_light_cache, (v_uv * 255.0 + 0.5) / 256.0);
    vec3 tint = sun.rgb * orig * orig;
    float strength = 0.15 * mix(0.03, 1.0, bright);
    if (any(notEqual(c, vec3(0.0)))) {
        vec3 tinted = c * mix(vec3(1.0), tint, strength);
        float tl = pt_luma(tinted);
        if (tl > 0.001) tinted *= pt_luma(c) / tl;
        c = tinted;
    }
#endif
    c *= mix(vec3(1.0), vec3(0.94, 1.0, 0.865), 1.0);          // polarizer
    c = clamp(c * (1.0 + 0.05 * pt_luma(c)), 0.0, 1.0);       // highlights
#if LCD_FX > 1
    c = mix(c, vec3(1.0), clamp(sun.a, 0.0, 0.85));
#endif
    c += (grain.g - 0.5) / 255.0;                              // dither
    return c;
#else
    return lcd;
#endif
}
/* Dot Matrix after LuigiRa's Gameboy Color Dot Matrix v0.9 (GPL-3),
   itself after Harlequin's Gameboy shader (GPL-3): its three passes (dots,
   gap blending, shadow and background) evaluated per screen pixel, DMG
   preset. The core draws the player's palette, so a shade's darkness is
   read between the palette's lightest and darkest colours (u_dm_lum); the
   dots take the palette's dark shade and the background its lightest. */
uniform sampler2D u_dm_bg;
uniform vec3 u_dm_fg;
uniform vec3 u_dm_bgc;
uniform vec2 u_dm_lum;
vec2 dm_out() { return u_rect.zw * u_fbo; }
// The dot and the gap are separable: along each axis, a screen pixel
// falls in one cell and covers some share of that cell's gap. So the four
// screen columns and rows from px - 2 to px + 1 (all a pixel's blend and
// its shadow's blend look at) are worked out once each, and the dots they
// touch (two cells along each axis at most, a cell being several screen
// pixels) are read once each: four texture reads instead of up to ten.
//
// The gap is one screen pixel at the start of each cell. Each pixel
// takes the share of it that it covers, seen through a window one
// screen pixel wide at a whole scale (exactly the old one-pixel gap)
// and two wide at a scale that is not whole (Original Ratio), so that
// every gap there is as light as the next, whether it falls on one
// pixel or across two (Siham 2026-11-10).
// Shares within 2% of none or all are none or all: rounding in the
// picture's size must not light a sliver of a gap at a whole scale.
vec4 dm_gap(vec4 tp, float s) {
    float h = 0.5 / s;                          // half a screen pixel, in texels
    vec4 d = abs(fract(tp - h + 0.5) - 0.5) * s; // from the nearest gap's middle, in screen pixels
    float f = abs(s - floor(s + 0.5)) <= 0.01 ? 1.0 : 2.0;
    vec4 gap = clamp((min(d + f * 0.5, vec4(0.5)) - max(d - f * 0.5, vec4(-0.5))) / f, 0.0, 1.0);
    return clamp((gap - 0.02) / 0.96, 0.0, 1.0);
}
// A dot's opacity where the cell is lit.
float dm_cell(vec2 cell) {
    float l = dot(texture2D(u_game, (cell + 0.5) / u_src).rgb, vec3(0.299, 0.587, 0.114));
    float dark = clamp((u_dm_lum.x - l) / max(u_dm_lum.x - u_dm_lum.y, 0.001), 0.0, 1.0);
    // Dot Matrix Color: every dot shows, its colour the game's, the more
    // opaque the darker it is.
    if (u_mode > 24.5) dark = 0.55 + 0.45 * clamp(1.0 - l, 0.0, 1.0);
    return clamp(dark + 0.10, 0.0, 1.0);
}
// The blended dot under screen pixel `px` and under px - (1, 1) (the
// shadow's). Blending mode 0: a gap takes the mean of the dots around it;
// a pixel part dot, part gap, a share of each.
vec2 dm_pair(vec2 px) {
    vec2 s = dm_out() / (u_uv.zw * u_src);
    // As the screen pass had it per pixel, so a pixel on a cell's edge
    // keeps the same cell.
    vec4 tx = (u_uv.x + (px.x + vec4(-2.0, -1.0, 0.0, 1.0)) / dm_out().x * u_uv.z) * u_src.x;
    vec4 ty = (u_uv.y + (px.y + vec4(-2.0, -1.0, 0.0, 1.0)) / dm_out().y * u_uv.w) * u_src.y;
    vec4 onx = 1.0 - dm_gap(tx, s.x);
    vec4 ony = 1.0 - dm_gap(ty, s.y);
    vec4 cx = floor(tx), cy = floor(ty);
    vec4 rx = clamp(cx - cx.x, 0.0, 1.0), ry = clamp(cy - cy.x, 0.0, 1.0);
    float a00 = dm_cell(vec2(cx.x, cy.x)), a10 = dm_cell(vec2(cx.x + 1.0, cy.x));
    float a01 = dm_cell(vec2(cx.x, cy.x + 1.0)), a11 = dm_cell(vec2(cx.x + 1.0, cy.x + 1.0));
    // Dot opacity a and coverage on at column i, row j (0..3 = px-2..px+1).
    #define DM_A(i, j) mix(mix(a00, a10, rx[i]), mix(a01, a11, rx[i]), ry[j])
    #define DM_LIT(i, j) (DM_A(i, j) * onx[i] * ony[j])
    float on = onx[2] * ony[2];
    float n = DM_LIT(2, 3) + DM_LIT(2, 1) + DM_LIT(3, 2) + DM_LIT(1, 2);
    float fa = mix(clamp(n * 0.25, 0.0, 1.0), DM_A(2, 2), on);
    float on1 = onx[1] * ony[1];
    float n1 = DM_LIT(1, 2) + DM_LIT(1, 0) + DM_LIT(2, 1) + DM_LIT(0, 1);
    float sa = mix(clamp(n1 * 0.25, 0.0, 1.0), DM_A(1, 1), on1);
    #undef DM_A
    #undef DM_LIT
    return vec2(fa, sa);
}
vec3 dot_matrix() {
    vec2 px = floor(v_uv * dm_out()) + 0.5;
    const float contrast = 0.8, light = 0.8, opacity = 0.9;
    vec3 col = mix(u_dm_fg, vec3(dot(u_dm_fg, vec3(0.299, 0.587, 0.114))), 0.8);
    vec2 blend = dm_pair(px);
    float fa = blend.x;
    float sa = blend.y * contrast * 0.5;
    vec3 bg = texture2D(u_dm_bg, v_uv).rgb;
    bg = clamp(u_dm_bgc + mix(vec3(-1.0), vec3(1.0), bg), 0.0, 1.0);
    vec3 o = col * sa + bg * (1.0 - sa);
    return col * fa * (1.0 - fa * fa * contrast) + o * (light - fa * contrast * opacity);
}
/* Game Boy Color Dot Matrix: the same dots, gaps, shadow and background,
   each dot in its pixel's colour (a little softened, as on the LCD) over a
   light background (Siham 2026-11-10). */
vec3 dot_matrix_colour() {
    vec2 px = floor(v_uv * dm_out()) + 0.5;
    const float contrast = 0.8, light = 0.8, opacity = 0.9;
    vec2 tp = (u_uv.xy + px / dm_out() * u_uv.zw) * u_src;
    vec3 rgb = texture2D(u_game, (floor(tp) + 0.5) / u_src).rgb;
    vec3 col = mix(rgb, vec3(dot(rgb, vec3(0.299, 0.587, 0.114))), 0.2);
    vec2 blend = dm_pair(px);
    float fa = blend.x;
    float sa = blend.y * contrast * 0.5;
    vec3 bg = texture2D(u_dm_bg, v_uv).rgb;
    bg = clamp(vec3(0.93, 0.93, 0.90) + mix(vec3(-1.0), vec3(1.0), bg), 0.0, 1.0);
    vec3 o = col * sa + bg * (1.0 - sa);
    return col * fa * (1.0 - fa * fa * contrast) + o * (light - fa * contrast * opacity);
}
/* Home console shaders (GLSL ES 1.00 ports, one pass each):
   crt-easymode (EasyMode, GPL), crt-hyllian (Hyllian, MIT),
   SABR v3.0 (Joshua Street, GPL-2+), Super Eagle (DOSBox
   Team / Derek Liauw Kie Fa, GPL) and xBR-lv2 (Hyllian, MIT), from
   libretro/glsl-shaders, preset defaults. All single pass.            */
#define HPI 3.141592653589
vec3 hs(vec2 uv) { return texture2D(u_game, uv).rgb; }
vec2 h_out() { return u_rect.zw * u_fbo; }   // picture size in screen pixels

/* crt-easymode, ENABLE_LANCZOS path. */
float em_curve(float x, float sharp) {
    float s = step(0.5, x);
    float c = 0.5 - sqrt(0.25 - (x - s) * (x - s)) * sign(0.5 - x);
    return mix(x, c, sharp);
}
vec3 em_tex(vec2 c) { vec3 t = hs(c); return t * t; }   // dilate 1.0
vec3 em_row(vec2 co, vec2 dx, vec4 k) {
    vec3 a = em_tex(co - dx), b = em_tex(co), c = em_tex(co + dx), d = em_tex(co + 2.0 * dx);
    vec3 col = a * k.x + b * k.y + c * k.z + d * k.w;
    return clamp(col, min(b, c), max(b, c));
}
vec3 crt_easymode(vec2 uv) {
    vec2 dx = vec2(1.0 / u_src.x, 0.0), dy = vec2(0.0, 1.0 / u_src.y);
    vec2 pc = uv * u_src - 0.5;
    vec2 tc = (floor(pc) + 0.5) / u_src;
    vec2 dist = fract(pc);
    float cx = em_curve(dist.x, 0.25);
    vec4 k = HPI * vec4(1.0 + cx, cx, 1.0 - cx, 2.0 - cx);
    k = max(abs(k), 1e-5);
    k = 2.0 * sin(k) * sin(k * 0.5) / (k * k);
    k /= dot(k, vec4(1.0));
    vec3 col = em_row(tc, dx, k), col2 = em_row(tc + dy, dx, k);
    col = mix(col, col2, em_curve(dist.y, 1.0));
    col = pow(col, vec3(1.0));                     // GAMMA_INPUT 2 / (DILATION + 1)
    float luma = dot(vec3(0.2126, 0.7152, 0.0722), col);
    float bright = (max(col.r, max(col.g, col.b)) + luma) * 0.5;
    float sbright = clamp(bright, 0.35, 0.65);
    float sbeam = clamp(bright * 1.5, 1.5, 1.5);
    float sw = 1.0 - pow(cos(uv.y * 2.0 * HPI * u_src.y) * 0.5 + 0.5, sbeam);
    float m = 1.0 - 0.3;
    float dot_no = mod(floor(v_uv.x * h_out().x), 3.0);
    vec3 mw = dot_no < 0.5 ? vec3(1.0, m, m) : (dot_no < 1.5 ? vec3(m, 1.0, m) : vec3(m, m, 1.0));
    if (u_src.y >= 400.0) sw = 1.0;
    col2 = col;
    col *= sw;
    col = mix(col, col2, sbright);
    col *= mw;
    col = pow(col, vec3(1.0 / 1.8));
    return col * 1.2;
}

/* crt-hyllian, defaults (BP 0, Catmull-Rom, phosphor layout 4). */
vec4 hy_in(vec2 c) { return pow(texture2D(u_game, c), vec4(2.4)); }
vec3 crt_hyllian(vec2 uv) {
    const float B = 0.0, C = 0.5;
    mat4 invX = mat4((-B - 6.0*C)/6.0, (12.0 - 9.0*B - 6.0*C)/6.0, -(12.0 - 9.0*B - 6.0*C)/6.0, (B + 6.0*C)/6.0,
                     (3.0*B + 12.0*C)/6.0, (-18.0 + 12.0*B + 6.0*C)/6.0, (18.0 - 15.0*B - 12.0*C)/6.0, -C,
                     (-3.0*B - 6.0*C)/6.0, 0.0, (3.0*B + 6.0*C)/6.0, 0.0,
                     B/6.0, (6.0 - 2.0*B)/6.0, B/6.0, 0.0);
    vec4 prof = vec4(0.58, 0.86, 1.0, 1.25);
    vec2 dx = vec2(1.0 / u_src.x, 0.0), dy = vec2(0.0, 1.0 / u_src.y);
    vec2 pc = uv * u_src + vec2(-0.5, 0.5);
    vec2 tc = (floor(pc) + 0.5) / u_src;
    vec2 fp = fract(pc);
    vec4 c00 = hy_in(tc - dx - dy), c01 = hy_in(tc - dy), c02 = hy_in(tc + dx - dy), c03 = hy_in(tc + 2.0*dx - dy);
    vec4 c10 = hy_in(tc - dx), c11 = hy_in(tc), c12 = hy_in(tc + dx), c13 = hy_in(tc + 2.0*dx);
    vec4 lobes = vec4(fp.x*fp.x*fp.x, fp.x*fp.x, fp.x, 1.0);
    vec4 w = invX * lobes;
    vec4 color0 = mat4(c00, c01, c02, c03) * w;
    vec4 color1 = mat4(c10, c11, c12, c13) * w;
    color0 = clamp(color0, min(c01, c02), max(c01, c02));
    color1 = clamp(color1, min(c11, c12), max(c11, c12));
    vec4 lum0 = mix(vec4(prof.y), vec4(prof.z), color0);
    vec4 lum1 = mix(vec4(prof.y), vec4(prof.z), color1);
    vec4 d0 = 4.0 * prof.x * fp.y / (lum0 + 0.0000001);
    vec4 d1 = 4.0 * prof.x * (1.0 - fp.y) / (lum1 + 0.0000001);
    d0 = exp(-d0 * d0);
    d1 = exp(-d1 * d1);
    vec3 col = (prof.w * (color0 * d0 + color1 * d1)).rgb;
    float off = 1.0 - 0.5;
    col *= mix(vec3(1.0, 1.0, off), vec3(off, off, 1.0), floor(mod(gl_FragCoord.x, 2.0)));
    return pow(max(col, vec3(0.0)), vec3(1.0 / 2.2));
}

/* The per-game-pixel half of each, drawn once per frame into u_edges
   (CACHE_PASS): every flag is 0 or 1, four corners a channel, packed as
   whole numbers. The screen pass only blends by its place in the pixel. */
const vec4 edge_bit = vec4(1.0, 2.0, 4.0, 8.0);
vec4 edge_bits(float n) { return mod(floor(n/edge_bit), 2.0); }
vec4 edge_lo(float c) { float n = floor(c*255.0 + 0.5); return edge_bits(n - 16.0*floor(n/16.0)); }
vec4 edge_hi(float c) { return edge_bits(floor(floor(c*255.0 + 0.5)/16.0)); }
/* SABR v3.0 */
const vec4 sAi = vec4(1.0, -1.0, -1.0, 1.0);
const vec4 sB45 = vec4(1.0, 1.0, -1.0, -1.0), sC45 = vec4(1.5, 0.5, -0.5, 0.5);
const vec4 sB30 = vec4(0.5, 2.0, -0.5, -2.0), sC30 = vec4(1.0, 1.0, -0.5, 0.0);
const vec4 sB60 = vec4(2.0, 0.5, -2.0, -0.5), sC60 = vec4(2.0, 0.0, -1.0, 0.5);
const vec4 sM45 = vec4(0.4), sM30 = vec4(0.2, 0.4, 0.2, 0.4), sM60 = vec4(0.4, 0.2, 0.4, 0.2), sMs = vec4(0.2);
const vec3 slum = vec3(0.21, 0.72, 0.07);
bvec4 s_and(bvec4 a, bvec4 b) { return bvec4(a.x && b.x, a.y && b.y, a.z && b.z, a.w && b.w); }
bvec4 s_or(bvec4 a, bvec4 b) { return bvec4(a.x || b.x, a.y || b.y, a.z || b.z, a.w || b.w); }
vec4 s_lum(vec3 a, vec3 b, vec3 c, vec3 d) { return vec4(dot(slum, a), dot(slum, b), dot(slum, c), dot(slum, d)); }
bvec4 s_eq(vec4 a, vec4 b) { return lessThan(abs(a - b), vec4(0.32)); }
vec4 s_wd(vec4 a, vec4 b, vec4 c, vec4 d, vec4 e, vec4 f, vec4 g, vec4 h) {
    return abs(a - b) + abs(a - c) + abs(d - e) + abs(d - f) + 4.0 * abs(g - h);
}
float s_cdf(vec3 a, vec3 b) { vec3 d = abs(a - b); return d.r + d.g + d.b; }
void sabr_edges(vec2 tc, out vec4 f45, out vec4 f30, out vec4 f60, out vec4 f36, out vec4 frn, out vec4 px) {
    float x = 1.0 / u_src.x, y = 1.0 / u_src.y;
    vec3 P1 = hs(tc + vec2(-x, -2.0*y)), P2 = hs(tc + vec2(0.0, -2.0*y)), P3 = hs(tc + vec2(x, -2.0*y));
    vec3 P6 = hs(tc + vec2(-x, -y)), P7 = hs(tc + vec2(0.0, -y)), P8 = hs(tc + vec2(x, -y));
    vec3 P11 = hs(tc + vec2(-x, 0.0)), P12 = hs(tc), P13 = hs(tc + vec2(x, 0.0));
    vec3 P16 = hs(tc + vec2(-x, y)), P17 = hs(tc + vec2(0.0, y)), P18 = hs(tc + vec2(x, y));
    vec3 P21 = hs(tc + vec2(-x, 2.0*y)), P22 = hs(tc + vec2(0.0, 2.0*y)), P23 = hs(tc + vec2(x, 2.0*y));
    vec3 P5 = hs(tc + vec2(-2.0*x, -y)), P10 = hs(tc + vec2(-2.0*x, 0.0)), P15 = hs(tc + vec2(-2.0*x, y));
    vec3 P9 = hs(tc + vec2(2.0*x, -y)), P14 = hs(tc + vec2(2.0*x, 0.0)), P19 = hs(tc + vec2(2.0*x, y));
    vec4 p7 = s_lum(P7, P11, P17, P13), p8 = s_lum(P8, P6, P16, P18);
    vec4 p11 = p7.yzwx, p12 = s_lum(P12, P12, P12, P12), p13 = p7.wxyz;
    vec4 p14 = s_lum(P14, P2, P10, P22), p16 = p8.zwxy, p17 = p7.zwxy, p18 = p8.wxyz;
    vec4 p19 = s_lum(P19, P3, P5, P21), p22 = p14.wxyz, p23 = s_lum(P23, P9, P1, P15);
    vec4 e45 = s_wd(p12, p8, p16, p18, p22, p14, p17, p13);
    vec4 econt = s_wd(p17, p11, p23, p13, p7, p19, p12, p18);
    vec4 e30 = abs(p13 - p16), e60 = abs(p8 - p17);
    bvec4 r45_1 = s_and(notEqual(p12, p13), notEqual(p12, p17));
    bvec4 r45_2 = s_and(not(s_eq(p13, p7)), not(s_eq(p13, p8)));
    bvec4 r45_3 = s_and(not(s_eq(p17, p11)), not(s_eq(p17, p16)));
    bvec4 r45_4 = s_and(s_eq(p12, p18), s_or(s_and(not(s_eq(p13, p14)), not(s_eq(p13, p19))),
                                             s_and(not(s_eq(p17, p22)), not(s_eq(p17, p23)))));
    bvec4 r45_5 = s_or(s_eq(p12, p16), s_eq(p12, p8));
    bvec4 r45 = s_and(r45_1, s_or(s_or(s_or(r45_2, r45_3), r45_4), r45_5));
    bvec4 r30 = s_and(notEqual(p12, p16), notEqual(p11, p16));
    bvec4 r60 = s_and(notEqual(p12, p8), notEqual(p7, p8));
    bvec4 edr45 = s_and(lessThan(e45, econt), r45);
    bvec4 edrrn = lessThanEqual(e45, econt);
    bvec4 edr30 = s_and(lessThanEqual(2.0 * e30, e60), r30);
    bvec4 edr60 = s_and(lessThanEqual(2.0 * e60, e30), r60);
    f45 = vec4(s_and(s_and(not(edr30), not(edr60)), edr45));
    f30 = vec4(s_and(s_and(edr45, not(edr60)), edr30));
    f60 = vec4(s_and(s_and(edr45, not(edr30)), edr60));
    f36 = vec4(s_and(s_and(edr60, edr30), edr45));
    frn = vec4(s_and(not(edr45), edrrn));
    px = step(abs(p12 - p17), abs(p12 - p13));
}
vec3 sabr(vec2 uv) {
    float x = 1.0 / u_src.x, y = 1.0 / u_src.y;
    vec2 tc = uv * vec2(1.0004, 1.0);
    vec2 fp = fract(tc * u_src);
    vec4 f45, f30, f60, f36, frn, px;
#ifndef CACHE_PASS
    if (u_cached > 0.5) {
        // As xBR-lv2: from the centre of the game pixel.
        tc = (floor(tc * u_src) + 0.5) / u_src;
        vec4 fl = texture2D(u_edges, tc);
        f45 = edge_lo(fl.r); f30 = edge_hi(fl.r); f60 = edge_lo(fl.g);
        f36 = edge_hi(fl.g); frn = edge_lo(fl.b); px = edge_hi(fl.b);
    } else
#endif
    sabr_edges(tc, f45, f30, f60, f36, frn, px);
    vec3 P7 = hs(tc + vec2(0.0, -y)), P11 = hs(tc + vec2(-x, 0.0)), P12 = hs(tc);
    vec3 P13 = hs(tc + vec2(x, 0.0)), P17 = hs(tc + vec2(0.0, y));
    vec4 ma45 = smoothstep(sC45 - sM45, sC45 + sM45, sAi * fp.y + sB45 * fp.x);
    vec4 ma30 = smoothstep(sC30 - sM30, sC30 + sM30, sAi * fp.y + sB30 * fp.x);
    vec4 ma60 = smoothstep(sC60 - sM60, sC60 + sM60, sAi * fp.y + sB60 * fp.x);
    vec4 marn = smoothstep(sC45 - sM45 + sMs, sC45 + sM45 + sMs, sAi * fp.y + sB45 * fp.x);
    vec4 mac = f36 * max(ma30, ma60) + f30 * ma30 + f60 * ma60 + f45 * ma45 + frn * marn;
    vec3 r1 = P12;
    r1 = mix(r1, mix(P13, P17, px.x), mac.x);
    r1 = mix(r1, mix(P7, P13, px.y), mac.y);
    r1 = mix(r1, mix(P11, P7, px.z), mac.z);
    r1 = mix(r1, mix(P17, P11, px.w), mac.w);
    vec3 r2 = P12;
    r2 = mix(r2, mix(P17, P11, px.w), mac.w);
    r2 = mix(r2, mix(P11, P7, px.z), mac.z);
    r2 = mix(r2, mix(P7, P13, px.y), mac.y);
    r2 = mix(r2, mix(P13, P17, px.x), mac.x);
    return mix(r1, r2, step(s_cdf(P12, r1), s_cdf(P12, r2)));
}

/* Super Eagle: the 2x picture, read nearest (the preset's stock pass).
   Colours compare exactly (vec3 equality) instead of upstream's packed
   float, which mediump GPUs would round. */
bool se(vec3 a, vec3 b) { return all(equal(a, b)); }
float se_r(vec3 a, vec3 b, vec3 c, vec3 d) {
    float x = 0.0, y = 0.0, r = 0.0;
    if (se(a, c)) x += 1.0; else if (se(b, c)) y += 1.0;
    if (se(a, d)) x += 1.0; else if (se(b, d)) y += 1.0;
    if (x <= 1.0) r += 1.0;
    if (y <= 1.0) r -= 1.0;
    return r;
}
vec3 super_eagle(vec2 uv) {
    float dx = 1.0 / u_src.x, dy = 1.0 / u_src.y;
    vec2 fp = fract(uv * u_src);
    vec3 C0 = hs(uv + vec2(-dx, -dy)), C1 = hs(uv + vec2(-dx, 0.0)), C2 = hs(uv + vec2(dx, -dy));
    vec3 D3 = hs(uv + vec2(2.0*dx, -dy)), C3 = hs(uv + vec2(-dx, 0.0)), C4 = hs(uv);
    vec3 C5 = hs(uv + vec2(dx, 0.0)), D4 = hs(uv + vec2(2.0*dx, 0.0)), C6 = hs(uv + vec2(-dx, dy));
    vec3 C7 = hs(uv + vec2(0.0, dy)), C8 = hs(uv + vec2(dx, dy)), D5 = hs(uv + vec2(2.0*dx, dy));
    vec3 D0 = hs(uv + vec2(-dx, 2.0*dy)), D1 = hs(uv + vec2(0.0, 2.0*dy)), D2 = hs(uv + vec2(dx, 2.0*dy));
    vec3 p00, p10, p01, p11;
    if (!se(C4, C8)) {
        if (se(C7, C5)) {
            p01 = C7; p10 = C7;
            p00 = (se(C6, C7) || se(C5, C2)) ? 0.25*(3.0*C7 + C4) : 0.5*(C4 + C5);
            p11 = (se(C5, D4) || se(C7, D1)) ? 0.25*(3.0*C7 + C8) : 0.5*(C7 + C8);
        } else {
            p11 = 0.125*(6.0*C8 + C7 + C5);
            p00 = 0.125*(6.0*C4 + C7 + C5);
            p10 = 0.125*(6.0*C7 + C4 + C8);
            p01 = 0.125*(6.0*C5 + C4 + C8);
        }
    } else {
        if (!se(C7, C5)) {
            p11 = C4; p00 = C4;
            p01 = (se(C1, C4) || se(C8, D5)) ? 0.25*(3.0*C4 + C5) : 0.5*(C4 + C5);
            p10 = (se(C8, D2) || se(C3, C4)) ? 0.25*(3.0*C4 + C7) : 0.5*(C7 + C8);
        } else {
            float r = se_r(C5, C4, C6, D1) + se_r(C5, C4, C3, C1) + se_r(C5, C4, D2, D5) + se_r(C5, C4, C2, D4);
            if (r > 0.0) { p01 = C7; p10 = C7; p00 = 0.5*(C4 + C5); p11 = p00; }
            else if (r < 0.0) { p11 = C4; p00 = C4; p01 = 0.5*(C4 + C5); p10 = p01; }
            else { p11 = C4; p00 = C4; p01 = C7; p10 = C7; }
        }
    }
    return (fp.x < 0.5) ? (fp.y < 0.5 ? p00 : p10) : (fp.y < 0.5 ? p01 : p11);
}

/* xBR-lv2 (XBR_SCALE 3, small details off, no smooth tips). */
const vec3 xw = vec3(14.352, 28.176, 5.472);
vec4 x_df(vec4 a, vec4 b) { return abs(a - b); }
vec4 x_diff(vec4 a, vec4 b) { return vec4(notEqual(a, b)); }
vec4 x_eq(vec4 a, vec4 b) { return step(x_df(a, b), vec4(15.0)); }
vec4 x_neq(vec4 a, vec4 b) { return vec4(1.0) - x_eq(a, b); }
vec4 x_wd(vec4 a, vec4 b, vec4 c, vec4 d, vec4 e, vec4 f, vec4 g, vec4 h) {
    return x_df(a,b) + x_df(a,c) + x_df(d,e) + x_df(d,f) + 4.0*x_df(g,h);
}
void xbr_edges(vec2 uv, out vec4 edr, out vec4 edr_l, out vec4 edr_u, out vec4 px) {
    float dx = 1.0 / u_src.x, dy = 1.0 / u_src.y;
    vec3 A1 = hs(uv + vec2(-dx, -2.0*dy)), B1 = hs(uv + vec2(0.0, -2.0*dy)), C1 = hs(uv + vec2(dx, -2.0*dy));
    vec3 A = hs(uv + vec2(-dx, -dy)), B = hs(uv + vec2(0.0, -dy)), C = hs(uv + vec2(dx, -dy));
    vec3 D = hs(uv + vec2(-dx, 0.0)), E = hs(uv), F = hs(uv + vec2(dx, 0.0));
    vec3 G = hs(uv + vec2(-dx, dy)), H = hs(uv + vec2(0.0, dy)), I = hs(uv + vec2(dx, dy));
    vec3 G5 = hs(uv + vec2(-dx, 2.0*dy)), H5 = hs(uv + vec2(0.0, 2.0*dy)), I5 = hs(uv + vec2(dx, 2.0*dy));
    vec3 A0 = hs(uv + vec2(-2.0*dx, -dy)), D0 = hs(uv + vec2(-2.0*dx, 0.0)), G0 = hs(uv + vec2(-2.0*dx, dy));
    vec3 C4 = hs(uv + vec2(2.0*dx, -dy)), F4 = hs(uv + vec2(2.0*dx, 0.0)), I4 = hs(uv + vec2(2.0*dx, dy));
    vec4 b = vec4(dot(B, xw), dot(D, xw), dot(H, xw), dot(F, xw));
    vec4 c = vec4(dot(C, xw), dot(A, xw), dot(G, xw), dot(I, xw));
    vec4 d = b.yzwx, e = vec4(dot(E, xw)), f = b.wxyz, g = c.zwxy, h = b.zwxy, i = c.wxyz;
    vec4 i4 = vec4(dot(I4, xw), dot(C1, xw), dot(A0, xw), dot(G5, xw));
    vec4 i5 = vec4(dot(I5, xw), dot(C4, xw), dot(A1, xw), dot(G0, xw));
    vec4 h5 = vec4(dot(H5, xw), dot(F4, xw), dot(B1, xw), dot(D0, xw));
    vec4 f4 = h5.yzwx;
    vec4 irlv0 = x_diff(e, f) * x_diff(e, h);
    vec4 irlv1 = irlv0 * (x_neq(f, b) * x_neq(f, c) + x_neq(h, d) * x_neq(h, g)
        + x_eq(e, i) * (x_neq(f, f4) * x_neq(f, i4) + x_neq(h, h5) * x_neq(h, i5)) + x_eq(e, g) + x_eq(e, c));
    vec4 irlv2l = x_diff(e, g) * x_diff(d, g);
    vec4 irlv2u = x_diff(e, c) * x_diff(b, c);
    vec4 wd1 = x_wd(e, c, g, i, h5, f4, h, f);
    vec4 wd2 = x_wd(h, d, i5, f, i4, b, e, i);
    edr = step(wd1 + vec4(0.1), wd2) * step(vec4(0.5), irlv1);
    edr_l = step(2.0 * x_df(f, g), x_df(h, c)) * irlv2l * edr;
    edr_u = step(2.0 * x_df(h, c), x_df(f, g)) * irlv2u * edr;
    px = step(x_df(e, f), x_df(e, h));
}
vec3 xbr_lv2(vec2 uv) {
    float dx = 1.0 / u_src.x, dy = 1.0 / u_src.y;
    vec2 fp = fract(uv * u_src);
    vec4 edr, edr_l, edr_u, px;
#ifndef CACHE_PASS
    if (u_cached > 0.5) {
        // Flags and colours from the centre of the game pixel the screen
        // pixel falls in, so they always agree on which pixel that is.
        uv = (floor(uv * u_src) + 0.5) / u_src;
        vec4 fl = texture2D(u_edges, uv);
        edr = edge_lo(fl.r); edr_l = edge_lo(fl.g); edr_u = edge_lo(fl.b); px = edge_lo(fl.a);
    } else
#endif
    xbr_edges(uv, edr, edr_l, edr_u, px);
    vec3 B = hs(uv + vec2(0.0, -dy)), D = hs(uv + vec2(-dx, 0.0)), E = hs(uv);
    vec3 F = hs(uv + vec2(dx, 0.0)), H = hs(uv + vec2(0.0, dy));
    const vec4 Ao = vec4(1.0, -1.0, -1.0, 1.0), Bo = vec4(1.0, 1.0, -1.0, -1.0), Co = vec4(1.5, 0.5, -0.5, 0.5);
    const vec4 Bx = vec4(0.5, 2.0, -0.5, -2.0), Cx = vec4(1.0, 1.0, -0.5, 0.0);
    const vec4 By = vec4(2.0, 0.5, -2.0, -0.5), Cy = vec4(2.0, 0.0, -1.0, 0.5);
    vec4 delta = vec4(1.0 / 3.0), delta_l = vec4(0.5 / 3.0, 1.0 / 3.0, 0.5 / 3.0, 1.0 / 3.0), delta_u = delta_l.yxwz;
    vec4 fx = Ao * fp.y + Bo * fp.x, fx_l = Ao * fp.y + Bx * fp.x, fx_u = Ao * fp.y + By * fp.x;
    vec4 fx45 = clamp((fx + delta - Co) / (2.0 * delta), 0.0, 1.0);
    vec4 fx30 = clamp((fx_l + delta_l - Cx) / (2.0 * delta_l), 0.0, 1.0);
    vec4 fx60 = clamp((fx_u + delta_u - Cy) / (2.0 * delta_u), 0.0, 1.0);
    fx45 = edr * fx45; fx30 = edr_l * fx30; fx60 = edr_u * fx60;
    vec4 mx = max(max(fx30, fx60), fx45);
    vec3 r1 = E;
    r1 = mix(r1, mix(H, F, px.x), mx.x);
    r1 = mix(r1, mix(B, D, px.z), mx.z);
    vec3 r2 = E;
    r2 = mix(r2, mix(F, B, px.y), mx.y);
    r2 = mix(r2, mix(D, H, px.w), mx.w);
    return mix(r1, r2, step(s_cdf(E, r1), s_cdf(E, r2)));
}
#ifdef CACHE_PASS
/* The caches above, one texel a game pixel, drawn ahead of the screen pass
   (lcd_cache.rs) from this same source so both work out the same flags. */
void main() {
    vec2 tc = (floor(gl_FragCoord.xy) + 0.5) / u_src;
    if (u_mode > 23.5 && u_mode < 24.5) {
        vec4 edr, edr_l, edr_u, px;
        xbr_edges(tc, edr, edr_l, edr_u, px);
        FRAG_COLOR = vec4(dot(edr, edge_bit), dot(edr_l, edge_bit), dot(edr_u, edge_bit), dot(px, edge_bit))/255.0;
    } else if (u_mode > 21.5 && u_mode < 22.5) {
        vec4 f45, f30, f60, f36, frn, px;
        sabr_edges(tc, f45, f30, f60, f36, frn, px);
        FRAG_COLOR = vec4(dot(f45, edge_bit) + 16.0*dot(f30, edge_bit),
                          dot(f60, edge_bit) + 16.0*dot(f36, edge_bit),
                          dot(frn, edge_bit) + 16.0*dot(px, edge_bit), 0.0)/255.0;
    } else {
        FRAG_COLOR = vec4(game_gamma(tc), 1.0);
    }
}
#else
void main() {
    vec2 uv = u_uv.xy + v_uv * u_uv.zw;
    vec2 f = fract(uv * u_src);
    vec3 c;
    // The LCD shaders (LCD3x, Grid, Dot, Simpletex, zfast LCD, LCD1x,
    // Simpletex on the GBA) read the game through Pixel AA. LCD Grid V2 and
    // AGB/AGS-001 filter the game themselves; the others as before.
#ifdef HIRES
    // A GBA Video cart's upscaled picture (Screen > Upscaler): the shaders
    // that take the game's colour as it is lay their grid over it, still
    // one cell per GBA pixel (u_src stays 240x160).
    c = clamp(texture2D(u_hires, uv).rgb, 0.0, 1.0);
#else
    if (u_mode > 0.5 && u_mode < 9.5 && !(u_mode > 4.5 && u_mode < 6.5)) {
        c = pixel_aa(uv);
    } else {
        c = texture2D(u_game, uv).rgb;
    }
#endif
    bool inside = all(greaterThanEqual(uv, u_pic.xy)) && all(lessThan(uv, u_pic.zw));

    if (!inside) {
        // Outside the picture (a home console's side bands at Integer or
        // Original Ratio): black, as RetroArch, never the edge texel smeared.
        c = vec3(0.0);
    } else if (u_mode > 24.5) {
        c = dot_matrix_colour();
    } else if (u_mode > 23.5) {
        c = xbr_lv2(uv);
    } else if (u_mode > 22.5) {
        c = super_eagle(uv);
    } else if (u_mode > 21.5) {
        c = sabr(uv);
    } else if (u_mode > 20.5) {
        c = crt_hyllian(uv);
    } else if (u_mode > 19.5) {
        c = crt_easymode(uv);
    } else if (u_mode > 18.5) {
        c = dot_matrix();
    } else if (u_mode > 17.5) {
        c = crt_aperture(uv);
    } else if (u_mode > 16.5) {
        c = fakelottes(uv);
    } else if (u_mode > 15.5) {
        c = crt_geom_deluxe(uv);
    } else if (u_mode > 14.5) {
        c = crt_gdv_mini_ultra(uv);
    } else if (u_mode > 13.5) {
        // davej crt-pi defaults: scanlines, multisample, gamma, mask 1.
        vec2 tp = uv*u_src;
        float ty = floor(tp.y)+0.5;
        float dy = tp.y-ty;
        float fw = (1.0/crt_out_px().y)/3.0;
        float sw = (crtpi_weight(dy)+crtpi_weight(dy-fw)+crtpi_weight(dy+fw))*0.3333333;
        float sy = sign(dy);
        dy = dy*dy; dy = dy*dy; dy *= 8.0; dy /= u_src.y; dy *= sy;
        vec3 col = nearest_bilinear(vec2(uv.x, ty/u_src.y + dy));
        col = pow(col, vec3(2.4))*sw*1.5;
        col = pow(col, vec3(1.0/2.2));
        c = col*(fract(gl_FragCoord.x*0.5) < 0.5 ? vec3(0.7,1.0,0.7) : vec3(1.0,0.7,1.0));
    } else if (u_mode > 12.5) {
        // SoltanGris42 zfast_crt_standard, RetroArch's parameter defaults.
        vec2 p = uv*u_src;
        vec2 i = floor(p)+0.5;
        vec2 ff = p-i;
        p = (i+4.0*ff*ff*ff)/u_src;
        p.x = mix(p.x, uv.x, 0.30);
        float yy1 = ff.y*ff.y;
        float yy2 = yy1*yy1;
        float mask = 1.0 - (fract(floor(gl_FragCoord.x)*-0.5) < 0.5 ? 0.25 : 0.0);
        vec3 col = nearest_bilinear(p);
        float w1 = 1.25 - 6.0*(yy1-2.05*yy2);
        float w2 = 1.0 - 8.0*(yy2-2.8*yy2*yy1);
        c = col*mix(w1*mask, w2, dot(col, vec3(0.3333*0.8)));
    } else if (u_mode > 11.5) {
        // Themaister sharp-bilinear: automatic prescale from the shown rows.
        vec2 texel=uv*u_src;
        vec2 s=fract(texel);
        float scale=max(1.0,floor((u_rect.w*u_fbo.y)/(u_uv.w*u_src.y)+0.01));
        float bound=0.5-0.5/scale;
        vec2 delta=s-vec2(0.5);
        vec2 filtered=(delta-clamp(delta,vec2(-bound),vec2(bound)))*scale+0.5;
        c=nearest_bilinear((floor(texel)+filtered)/u_src);
    } else if (u_mode > 10.5) {
        // endrift AGS-001: frontlit SP gamma, subpixel mask and light.
        c=mgba_pass(uv,true);
        vec2 radius=pow(abs((v_uv-vec2(0.5))*1.5),vec2(4.0));
        vec3 bleed=vec3(0.12,0.14,0.19) +
            (dot(radius,radius)+vec3(0.02,0.03,0.05))*vec3(0.14,0.18,0.2);
        c+=bleed+0.07*mgba_pass(uv-vec2(0.0,0.025),true);
    } else if (u_mode > 9.5) {
        // endrift AGB-001: original non-backlit screen color subpixels.
        c=mgba_pass(uv,false);
    } else if (u_mode > 8.5) {
        // Simpletex on the GBA: the same shader as GB/GBC since 2.2.1,
        // without its own GBA Color. Color Correction is the player's
        // choice, as with every other shader (Siham 2026-11-10).
        c = simpletex(c, f, 0.6);
    } else if (u_mode > 7.5) {
        // jdgleaver LCD1x: proper scanline alignment, no RGB separation.
        vec2 angle=6.283185307*(uv*u_src-vec2(0.25));
        c*=((16.0+sin(angle.y))/17.0)*((4.0+sin(angle.x))/5.0);
    } else if (u_mode > 6.5) {
        // SoltanGris42 zfast_lcd_standard default BORDERMULT=14,
        // GBAGAMMA=1, nearest sampling.
        vec2 px=uv*u_src;
        vec2 dist=abs(floor(px)+vec2(0.5)-px);
        float y=max(dist.x,dist.y); y*=y;
        float yy=y*y;
        float weight=1.0-14.0*(yy-2.7*yy*y);
        c*=weight;
        c*=0.6+0.4*c;
    } else if (u_mode > 5.5) {
        // CGWG LCD Grid V2 followed by RetroArch GBA Color.
        c=gba_color(lcd_grid_v2(uv,true));
    } else if (u_mode > 4.5) {
        c=lcd_grid_v2(uv,false);
    } else if (u_mode > 3.5) {
        c = simpletex(c, f, 1.0);
    } else if (u_mode > 2.5) {
        float bloom = mix(1.05, 0.95, dot(c, vec3(0.30, 0.59, 0.11)));
        float mid = dot_weight(f, vec2(0.0), bloom);
        float sum = 0.0;
        for (int j = -1; j <= 1; j++) {
            for (int i = -1; i <= 1; i++) {
                sum += dot_weight(f, vec2(float(i), float(j)), bloom);
            }
        }
        c = mix(1.2 * c * mid, c * sum, 0.65);
    } else if (u_mode > 1.5) {
        c = grid_line(uv, c);
    } else if (u_mode > 0.5) {
        vec2 angle = f * 6.283185307;
        float yfactor = (16.0 + sin(angle.y)) / 17.0;
        vec3 xfactors = (4.0 + sin(angle.x + offsets)) / 5.0;
        c *= yfactor * xfactors;
    }
    if (inside) c = lcd_effect(c, uv);
    // Grid V2 + GBA already contains GBA Color. Never unintentionally
    // apply the global correction twice.
    if (u_colour_correct > 0.5 && inside &&
        !(u_mode > 5.5 && u_mode < 6.5)) {
        // GBA-era display simulation: subtly warmer, lower-saturation
        // colours and a restrained luminance response. GPU only; libretro
        // framebuffer, SRAM and save states remain untouched.
        float luminance = dot(c, vec3(0.299, 0.587, 0.114));
        c = mix(vec3(luminance), c, 0.80);
        c *= vec3(1.02, 1.00, 0.95);
        c = pow(clamp(c, 0.0, 1.0), vec3(1.05));
    }
    FRAG_COLOR = vec4(c * u_bright, 1.0);
}
#endif
"#;

pub const SPRITE_FRAG: &str = r#"
precision mediump float;
uniform sampler2D u_tex;
uniform vec4 u_colour;
// Draw::Ink: light ink (text, glyphs) redrawn from u_paper to u_ink by its
// brightness, for menus on a light ground; u_map 0 leaves the texel alone.
uniform float u_map;
uniform vec3 u_paper;
uniform vec3 u_ink;
varying vec2 v_uv;
void main() {
    vec4 texel = texture2D(u_tex, v_uv);
    float light = clamp(dot(texel.rgb, vec3(0.299, 0.587, 0.114)) * 1.04, 0.0, 1.0);
    texel.rgb = mix(texel.rgb, mix(u_paper, u_ink, light), u_map);
    FRAG_COLOR = texel * u_colour;
}
"#;

pub const BLIT_VERT: &str = r#"
attribute vec2 a_pos;
uniform vec4 u_uv;
varying vec2 v_uv;
void main() {
    v_uv = u_uv.xy + a_pos * u_uv.zw;
    gl_Position = vec4(a_pos * 2.0 - 1.0, 0.0, 1.0);
}
"#;

pub const BLIT_FRAG: &str = r#"
precision mediump float;
uniform sampler2D u_tex;
uniform vec3 u_gain;
varying vec2 v_uv;
void main() {
    FRAG_COLOR = vec4(texture2D(u_tex, v_uv).rgb * u_gain, 1.0);
}
"#;

pub const BEZEL_FRAG: &str = r#"
precision mediump float;
uniform sampler2D u_tex;
uniform vec3 u_gain;
uniform float u_alpha;
varying vec2 v_uv;
void main() {
    vec4 c = texture2D(u_tex, v_uv);
    FRAG_COLOR = vec4(c.rgb * u_gain, c.a * u_alpha);
}
"#;
