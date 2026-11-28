use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::{c_char, c_void, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU8, AtomicU64, Ordering};
use crate::game::GameSession;
use crate::settings::Settings;
use crate::emulation::EmulationProfile;
use std::time::Duration;
use crate::video_mode::{self, VideoMode};
use crate::core_selection;
use crate::cart_render::CartRender;
use crate::core_picker::{CorePicker, Press};
use slot_retro::ButtonMask;
use jni::{JNIEnv, objects::{JObject, JString, JShortArray}, sys::{jint, jboolean, jlongArray, jstring}};
use std::sync::Mutex;
use std::time::Instant;
use std::net::Ipv4Addr;
use crate::multiplayer::{LinkStarter, Progress as LinkProgress, Role as LinkRole};
use crate::link_kind::{self, LinkKind};
use crate::app::{GameMenu as LinkMenu, LinkRow, LinkFail, LinkStep, ReleaseFrom};
use crate::link_screen::{self, LinkSprites, Marks, Sprite};
use slot_ui::link_hardware::{self, Hardware};

use slot_gfx::{Compositor, GfxError, Surface, OUT_H, out_w};
use slot_store::{Platform, Core, stamp_now};
use slot_power::{Battery, Charge};
use crate::library::{carts_by_platform, RomEntry, LIBRARY};
use slot_ui::{
    board_face, chip_face, pak_chip_face, chip_shadow_face, socket_face, quick_value_face, cart_face_with_material,
    cart_shadow, gb_cart_shadow, lift_of,
    lid_toward, board_toward,
    grown, Draw, GbShell, Shelf, SlotChrome, SOCKET_W, SOCKET_H,
    TURN_PAD,
    hint_face, arrows_hint_face, title_face, photo_face, Polaroids, Printed, HINT_H, HINT_EDGE, draw_slot_name, draw_platform_name, platform_name_face,
    icon_face, badge_face, menu_face, Icon, LinkBadge, BOLT_PX, HUD_INK,
    Hud, HudKind, FfState, Toast, toast_face, QUICK_PITCH, opening, edge, centred_hints, LEGEND_GAP, MOUTH_H,
    sticker_face_konkr, StickerFields, STICKER_W, STICKER_H, quick_caret_face,
    onboarding_instruction_face, onboarding_note_face,
};
use slot_ui::menu_style::{self, menu_ground, PILL_H, PILL_LEFT, PILL_R, PILL_RIGHT};

/// Upload a Slot face at whatever density it was rendered at. Draws keep
/// using the face's logical `w`/`h`; only the texel count differs.
trait FaceTextures {
    fn create_face(&mut self, face: &impl slot_ui::Face) -> slot_gfx::TexId;
    fn update_face(&mut self, tex: slot_gfx::TexId, face: &impl slot_ui::Face);
}

impl FaceTextures for Compositor {
    fn create_face(&mut self, face: &impl slot_ui::Face) -> slot_gfx::TexId {
        let (w, h) = face.texels();
        self.create_texture(w, h, face.rgba())
    }

    fn update_face(&mut self, tex: slot_gfx::TexId, face: &impl slot_ui::Face) {
        let (w, h) = face.texels();
        self.update_texture(tex, w, h, face.rgba());
    }
}

// Android GLSurfaceView owns EGL context creation, current context and buffer swaps.
#[link(name = "EGL")]
unsafe extern "C" {
    fn eglGetProcAddress(name: *const c_char) -> *const c_void;
}

struct AndroidSurface {
    size: (u32, u32),
}

impl Surface for AndroidSurface {
    fn make_current(&mut self) -> Result<(), GfxError> {
        Ok(())
    }

    fn window_size(&self) -> (u32, u32) {
        self.size
    }

    fn swap(&mut self) -> Result<(), GfxError> {
        Ok(())
    }

    fn proc_address(&self, name: &str) -> *const c_void {
        CString::new(name)
            .map(|c| unsafe { eglGetProcAddress(c.as_ptr()) })
            .unwrap_or(std::ptr::null())
    }
}

#[derive(Clone)]
enum Input {
    Key(i32, bool),
    /// A finger on the screen (runtime/touch.rs): action, x and y from 0
    /// to 1 across the screen, Android's event time in ms.
    Touch(u8, f32, f32, i64),
    /// A key of one of Android's input devices (Settings > Controls).
    PadKey { device: i32, code: i32, pressed: bool, stick: bool },
    /// Every controller connected now.
    Controllers(Vec<crate::controls::Device>),
    // Predefined shelf UI states from Android: no untrusted or long text on GL.
    ShelfStatus(u8),
    /// A scan's progress: carts read and how many, 0 while listing folders.
    ScanProgress(u32, u32),
    UpdateStatus(u8, String, String, bool),
    /// The update's download, 0 to 100 %.
    UpdateProgress(u8),
    Reset,
    GameReady { uri: String, path: String },
    GameError { uri: String, message: String },
    Exit,
    Suspend,
    ReloadVisuals,
    ReloadCartLabel { uri: String },
    /// Cart Studio's pictures for a cart: the real label and the logo.
    StudioArt { uri: String, real: Option<Vec<u8>>, logo: Option<Vec<u8>>, kind: i32, name: String },
    // Android announces network return; only wake the HTTP retry timer.
    // Never infer RA API authentication/reachability from Wi-Fi alone.
    RaNetworkAvailable,
    RaAccountConfigured,
    // Kotlin acquires the multicast lock before returning a Wi-Fi IPv4.
    LinkNetworkReady(String),
    LinkNetworkError(String),
    LinkPeerRomConsent(i32),
    RaPrefetchHash(String, u32),
    ExternalReturn,
    /// A folder was chosen from Library: back to the shelf, to see it fill.
    ExternalClose,
    /// Library > RomM (runtime/romm.rs): the server's games, a status (0
    /// loading, 1 a message, 3 close), a label fetched, a download's
    /// progress and each download's end; a Game Boy family game's header,
    /// read from the server, for its shell.
    RommCatalog(Vec<romm::RommEntry>),
    RommStatus(i32, String),
    RommLabel(i64),
    RommHeader(romm::RommEntry),
    RommProgress(i32, i32, i32),
    RommDownloaded(i64, bool),
    /// Library > RomM: SELECT's filters.
    RommFilter(romm::RommFilter),
    /// Android deleted (or couldn't delete) a game's ROM file.
    GameDeleted(String, bool),
    /// A slot. button released by the system rather than by the player
    /// (RG Rotate): `held` is a long press Android took over (eject), else
    /// the press is just forgotten, with no tap.
    SlotButtonReleased { held: bool },
    /// RG Rotate lid: closed (true) or opened (false).
    /// Shut, and whether at once (no cover animation).
    LidCover(bool, bool),
    /// While closed with no game: the shelf moves on by one cart.
    LidStep,
}

static INPUT: Mutex<VecDeque<Input>> = Mutex::new(VecDeque::new());
static REQUEST: Mutex<Option<String>> = Mutex::new(None);
// The core the requested cart boots on, as its platform resolves it: Android
// names the save folder from it before the ROM is opened.
static LAUNCH_CORE: Mutex<Option<(String, Core)>> = Mutex::new(None);
static LABEL_PICK_REQUEST: Mutex<Option<String>> = Mutex::new(None);
/// File name in Wallpapers/ that Import / Delete Wallpaper acts on: user.png,
/// or the current shelf's <shelf>.png while the Shelf wallpaper is picked.
pub(crate) static WALLPAPER_TARGET: Mutex<Option<String>> = Mutex::new(None);
static UI_ACTION: Mutex<VecDeque<i32>> = Mutex::new(VecDeque::new());
static SAVE_SYNC: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
static SHARE_SAVES_ENABLED: AtomicBool = AtomicBool::new(false);
/// The display's landscape size in pixels, set by Android before the first
/// surface. 0 keeps the KONKR's 960x640.
static PANEL_SIZE: AtomicU64 = AtomicU64::new(0);

/// Paused Screen row choosing the game's size: GBA off the 3:2 KONKR, and
/// the home consoles everywhere (Integer / Original Ratio / Stretch).
const SCREEN_SIZE_KEY: usize = 40;
use crate::settings::{CORE_SMS_KEY, CORE_GG_KEY, CORE_NGP_KEY, CORE_LYNX_KEY, CORE_PICO8_KEY,
    SHADER_SMS_KEY, SHADER_GG_KEY, SHADER_NGP_KEY, SHADER_WS_KEY, SHADER_LYNX_KEY,
    SHADER_POKEMINI_KEY, BEZEL_SMS_KEY, BEZEL_GG_KEY, BEZEL_NGP_KEY, BEZEL_WS_KEY,
    BEZEL_LYNX_KEY, BEZEL_POKEMINI_KEY, POKEMINI_FILTER_KEY, SHADER_PICO8_KEY,
    CORE_PCE_KEY, SHADER_NEOGEO_KEY, SHADER_PCE_KEY, BEZEL_NEOGEO_KEY, BEZEL_PCE_KEY};
/// "INTEGER", "ORIGINAL RATIO", "STRETCH" in the setting value faces.
const SCREEN_SIZE_VALUE: usize = 67;

/// A canvas fitted to a panel that isn't 3:2: the game uses the real panel.
fn fitted_canvas() -> bool {
    slot_gfx::out_w() != slot_gfx::BASE_W
}

/// Whether the 3:2 game frame is a whole multiple of 240x160 on `panel`.
fn whole_game_scale(panel: (u32, u32)) -> bool {
    if fitted_canvas() { return true; }
    let h = slot_gfx::default_area()[3] * panel.1 as f32 / OUT_H as f32;
    let k = h / slot_gfx::SRC_H as f32;
    k >= 1.0 && (k - k.round()).abs() < 0.01
}

/// A KONKR Pocket Advance: its 960x640 panel, or its Android model GT78-VN
/// (maker ARBOR; getprop on Siham's KPA, 2026-11-05).
fn is_konkr() -> bool {
    panel_size() == (960, 640) ||
        DEVICE_NAME.lock().unwrap_or_else(|e| e.into_inner()).1.eq_ignore_ascii_case("GT78-VN")
}

pub(crate) fn panel_size() -> (u32, u32) {
    let packed = PANEL_SIZE.load(Ordering::Acquire);
    let (w, h) = ((packed >> 32) as u32, packed as u32);
    if w == 0 || h == 0 { (960, 640) } else { (w.max(h), w.min(h)) }
}
// -1 = preserve ordinary Slot rule, 0 = start from SRAM, 1 = user chose
// existing destination-core state in the explicit Share Saves dialogue.
static NEXT_RESUME_MODE: AtomicI32 = AtomicI32::new(-1);
static CART_SFX: Mutex<VecDeque<i32>> = Mutex::new(VecDeque::new());
const CART_INSERT_SFX: i32 = 1;
const CART_EJECT_SFX: i32 = 2;
// Exact upstream Slot timeline: 450ms mechanical seating + 280ms hold.
const INSERT_S: f32 = 0.73;
const SEATED_AT: f32 = 0.45;
// Original Slot starts the 48-kHz insert clip 97ms before the cart seats,
// so the recorded contact lands at 450ms, not 143ms early.
const INSERT_SOUND_LEAD_S: f32 = 0.097;
const INSERT_SOUND_PROGRESS: f32 = (SEATED_AT - INSERT_SOUND_LEAD_S) / SEATED_AT;
// Upstream Slot screen power transition times (app.rs).
const SCREEN_POWER_ON_S: f32 = 0.22;
const SCREEN_POWER_OFF_S: f32 = 0.16;
const EJECT_S: f32 = 0.45;
const EJECT_HOLD_S: f32 = 0.35; // Original Slot hold after CRT-off.
// Slightly later than upstream's first-motion eject click, by request:
// retain the original 160ms CRT + 350ms hold + 450ms travel unchanged.
const EJECT_SOUND_DELAY_S: f32 = 0.040;
const EJECT_SOUND_PROGRESS: f32 = 1.0 - EJECT_SOUND_DELAY_S / EJECT_S;
const SELECT_CHORD_MS: u128 = 600;
const SELECT_TAP_FRAMES: u8 = 3;
/// slot. original's in-game menu: the paused game under this much black.
const GAME_MENU_SCRIM: f32 = 0.65;
/// The game menu waits this long for a second slot. tap (Save States).
const MENU_REVEAL_MS: u64 = 120;
/// A second slot. press this soon after a tap opens Save States.
const DOUBLE_TAP_MS: u128 = 360;
const SHELF_STATUS_TEXT: [&str; 10] = [
    "", "SCANNING CARTRIDGES...", "NO GAMES FOUND",
    "LIBRARY IMPORT FAILED", "FOLDER ACCESS EXPIRED",
    "CANNOT READ ROM FOLDER", "FOLDER PICKER UNAVAILABLE",
    "FOLDER PERMISSION FAILED",
    // 8: RomM's games downloading onto an empty shelf, drawn as the first
    // scan, never "NO GAMES FOUND" (Siham 2026-11-16).
    "DOWNLOADING GAMES...",
    // 9: the ROM folder was deleted (its storage is here).
    "ROM FOLDER NOT FOUND",
];
/// The statuses drawn as the first scan (a bar and a count).
fn status_has_bar(status: u8) -> bool { matches!(status, 1 | 8) }
// Original Slot records ~30 rewind states/sec and replays ~60/sec (~2x).
// Android v1.2.2 recorded/replayed ~10/sec (~1x). Playback rate is computed
// by settings::rewind_step_seconds, never confused with capture frequency.

const FF_DOUBLE_TAP_MS: u128 = 320;
static MESSAGE: Mutex<Option<String>> = Mutex::new(None);
// RA worker runs off the GL thread. The engine samples its nonblocking status
// and forwards bounded, sanitized user-facing notices to Android's main thread.
static RA_NOTICES: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
static RA_STATUS: Mutex<Option<String>> = Mutex::new(None);
// F10: do not allocate, stringify and lock a new RA JSON status on every
// 60/120Hz GL draw. Only publish on actual status/progress changes.
static RA_STATUS_KEY: AtomicU64 = AtomicU64::new(u64::MAX);
const RA_NOTICE_LIMIT: usize = 8;
// Up to 96 prepared cartridge faces (~70 MiB worst-case native RGBA artwork).
// KONKR has 4 GiB shared RAM; the LRU still reuses existing GLES textures.
const CART_TEXTURE_CACHE_CAPACITY: usize = 160;

/// Steps between two carts on a shelf that wraps around.
fn cyclic_distance(a: usize, b: usize, len: usize) -> usize {
    if len == 0 { return 0; }
    let d = a.abs_diff(b) % len;
    d.min(len - d)
}
const CART_IDLE_PREFETCH_CAP: usize = 155;
/// Carts asked for ahead of the screen while the shelf moves.
const CART_LOOK_AHEAD: i32 = 6;
/// The shelf at rest this long: its carts go into the disk cache.
const DISK_FILL_AFTER: Duration = Duration::from_secs(3);
/// Carts a disk fill covers (the disk cache keeps 3000 faces).
const DISK_FILL_CAP: usize = 2500;
const CART_IDLE_GRACE: Duration = Duration::from_millis(400);
const CART_GPU_UPLOAD_BUDGET: Duration = Duration::from_millis(2);

fn ra_notice_payload(title: &str, detail: &str, earned: bool, badge: Option<&Path>) -> String {
    let clean = |value: &str, limit: usize| -> String {
        value.chars().filter(|ch| !ch.is_control()).take(limit).collect()
    };
    // BadgeName is validated by slot_achievements::badges::path before being
    // attached to Notice. Android must still verify canonical app-private path
    // before reading the image. Empty means fall back to a Slot glyph.
    serde_json::json!({
        "title": clean(title, 100),
        "detail": clean(detail, 160),
        "earned": earned,
        "badge": badge.and_then(|path| path.to_str()).unwrap_or(""),
    }).to_string()
}

fn queue_ra_notice(queue: &mut VecDeque<String>, payload: String) {
    if queue.len() >= RA_NOTICE_LIMIT { queue.pop_front(); }
    queue.push_back(payload);
}

static PATHS: Mutex<Option<(PathBuf, PathBuf)>> = Mutex::new(None);
// F9: loss diagnostics only; never used to pace or drop native PCM.
// Cumulative atomics, sampled as deltas by the Android audio worker
// every ~5 seconds.
static AUDIO_FIFO_OVERFLOW_EVENTS: AtomicU64 = AtomicU64::new(0);
static AUDIO_FIFO_DROPPED_SHORTS: AtomicU64 = AtomicU64::new(0);

static AUDIO: Mutex<VecDeque<i16>> = Mutex::new(VecDeque::new());
static PLAYING: AtomicBool = AtomicBool::new(false);
static SAMPLE_RATE: AtomicI32 = AtomicI32::new(0);
// RETROARCH pacing inputs from Kotlin: the panel refresh rate (mHz) and the
// AudioTrack queue depth/capacity in frames, sampled by the audio worker.
static DISPLAY_REFRESH_MILLIHZ: AtomicI32 = AtomicI32::new(0);

/// Match a core's native refresh if the panel can show every frame at an
/// integer refresh multiple; otherwise use its own 60Hz pulldown mode.
/// PokeMini and Beetle WonderSwan preserve game/audio speed by emulating
/// their missing intermediate frames inside a 60Hz libretro callback.
/// Unknown Android display rate falls back to 60Hz (KPA's normal display).
pub(crate) fn handheld_core_60hz(native_hz: f64) -> bool {
    let display_hz = f64::from(DISPLAY_REFRESH_MILLIHZ.load(Ordering::Acquire)) / 1000.0;
    handheld_core_60hz_for_display(native_hz, display_hz)
}

pub(crate) fn handheld_core_60hz_for_display(native_hz: f64, display_hz: f64) -> bool {
    if !(native_hz > 0.0) || !(display_hz > 0.0) { return true; }
    let multiple = (display_hz / native_hz).round().max(1.0);
    ((display_hz / native_hz) - multiple).abs() > 0.02
}
static AUDIO_OUT_QUEUED_FRAMES: AtomicI32 = AtomicI32::new(0);
static AUDIO_OUT_CAPACITY_FRAMES: AtomicI32 = AtomicI32::new(0);
static DISPLAY_SYNC: AtomicBool = AtomicBool::new(false);
// Frame Pacing NATIVE for the running solo game: Kotlin renders
// continuously and the blocking buffer swap paces the core (RetroArch's own
// loop). False (RESPONSIVE, menus, Link) renders on Choreographer VSyncs.
static RA_SWAP_LOOP: AtomicBool = AtomicBool::new(false);
// Updated by the GL owner after an entirely settled, fully preloaded shelf frame.
// Kotlin may switch the idle carousel to requestRender without affecting games.
static CAROUSEL_QUIET: AtomicBool = AtomicBool::new(false);
// Android reports the exact device model before the first EGL surface is made.
// Used only to choose initial settings when no stored settings file exists.
static RPC_DEFAULT_RESPONSIVE: AtomicBool = AtomicBool::new(false);
// How the handheld names itself (Kotlin: model, with its maker before a short
// code such as RG405M) and its Android model, for the About sticker.
pub(crate) static DEVICE_NAME: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));
// The GLES owner reports a stable selection, never dereferencing SAF URIs itself.
static WARM_ROM_URI: Mutex<Option<String>> = Mutex::new(None);
// Read-only GL-owned library swap gate for Kotlin's background scanner.
static LIBRARY_UPDATE_SAFE: AtomicBool = AtomicBool::new(false);
// A12 diagnostic A/B. Default keeps A11 recovery; flag file selects legacy.
static MGBA_CABLE_RECOVERY: AtomicBool = AtomicBool::new(true);
// Android AudioTrack time-stretches 2x game audio while preserving pitch.
// Expose the current transport speed separately from libretro's sample rate.
static AUDIO_SPEED_PERMILLE: AtomicI32 = AtomicI32::new(1000);
/// Fast Forward MAX: at most this many frames, for at most this long, in
/// one refresh (about two thirds of a 60 Hz one).
const FF_MAX_FRAMES: usize = 120;
const FF_MAX_BUDGET_US: u64 = 11_000;
static FF_AUDIO_SUPPORTED: AtomicBool = AtomicBool::new(true);
static FF_AUDIO_ENABLED: AtomicBool = AtomicBool::new(true);
static RUMBLE_STRENGTH: AtomicI32 = AtomicI32::new(0);
static RUMBLE_PEAK: AtomicI32 = AtomicI32::new(0);

fn clear_rumble() {
    RUMBLE_STRENGTH.store(0, Ordering::Release);
    RUMBLE_PEAK.store(0, Ordering::Release);
}
fn scale_rumble(strength: u16, level: u8) -> i32 {
    // Preserve nonzero signals, even at Low. Four levels are 25/50/75/100%.
    if strength == 0 || level == 0 { return 0; }
    (i32::from(strength) * i32::from(level.min(4)) / 4).max(1)
}
// Updated once per second by the GL thread; read-only and optional on Android.
static PERF_STATUS: Mutex<Option<String>> = Mutex::new(None);
static PERF_ACTIVE: AtomicBool = AtomicBool::new(false);
// Read-only 1 Hz emulator telemetry, independent of Show Stats UI.
static EMULATION_DIAGNOSTICS: Mutex<Option<String>> = Mutex::new(None);
// Read-only status bar snapshot for Android's upper shelf overlays.
// Updated on the GL thread alongside the *existing* Slot footer gate.
static SHELF_INDICATOR_FLAGS: AtomicI32 = AtomicI32::new(0);
// Read-only lifecycle bit polled by Kotlin; never touch WifiManager on GL.
static LINK_NEEDS_MULTICAST: AtomicBool = AtomicBool::new(false);
// Set while a Link session plays, so Kotlin holds Wi-Fi out of power save.
static LINK_ACTIVE: AtomicBool = AtomicBool::new(false);
// Authenticated peer key fingerprint, never a user-supplied device name.
static LINK_PEER_LABEL: Mutex<Option<String>> = Mutex::new(None);
// Internet Multiplayer: the message Kotlin's share sheet sends (UI action 23).
static LINK_SHARE_TEXT: Mutex<Option<String>> = Mutex::new(None);

#[derive(Clone)]
struct SystemStatus {
    clock: String,
    battery: Option<Battery>,
    /// Today in Android's language ("Sat 14 Nov"), for the Rotate's clocks.
    date: String,
}
static SYSTEM_STATUS: Mutex<SystemStatus> = Mutex::new(SystemStatus {
    clock: String::new(), battery: None, date: String::new(),
});
// Android's existing status push owns the generation. The GL loop should not
// clone a String, acquire SYSTEM_STATUS, or reformat the clock every frame.
static SYSTEM_STATUS_REV: AtomicU64 = AtomicU64::new(1);

fn controls(code: i32) -> u16 {
    match code {
        97 => ButtonMask::B, 100 => ButtonMask::Y, 109 => ButtonMask::SELECT,
        108 => ButtonMask::START, 19 => ButtonMask::UP, 20 => ButtonMask::DOWN,
        21 => ButtonMask::LEFT, 22 => ButtonMask::RIGHT,
        96 => ButtonMask::A, 99 => ButtonMask::X,
        102 => ButtonMask::L, 103 => ButtonMask::R,
        // Six-button pads: slot.'s own C and Z, which Controls can map.
        98 => crate::controls::bit::C, 101 => crate::controls::bit::Z,
        _ => 0,
    }
}

// A consumed SELECT chord must also consume that key's release.
fn select_chord_bit(code: i32) -> u16 {
    match code {
        96 => 1, 97 => 2, 100 => 4, 102 => 8,
        103 => 16, 104 => 32, 105 => 64,
        19 => 128, 20 => 256, 21 => 512, 22 => 1024,
        _ => 0,
    }
}

/// Upstream slot-input: SELECT held, then HOME/MODE opens the Link menu.
/// Consume both the press and its release, otherwise HOME also opens the
/// regular game menu (or cancels the Link picker on release).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum SelectHomeAction { Pass, OpenLink, Swallow }

fn select_home_action(
    pressed: bool,
    select_held: bool,
    in_game: bool,
    may_open: bool,
    home_consumed: &mut bool,
) -> SelectHomeAction {
    if *home_consumed {
        if !pressed { *home_consumed = false; }
        return SelectHomeAction::Swallow;
    }
    if pressed && select_held && in_game && may_open {
        *home_consumed = true;
        return SelectHomeAction::OpenLink;
    }
    SelectHomeAction::Pass
}

fn set_message(message: String) {
    *MESSAGE.lock().unwrap_or_else(|e| e.into_inner()) = Some(message);
}

/// What the player reads when Android couldn't prepare a game: Kotlin
/// puts the step that failed before the reason (Siham 2026-11-16).
fn game_error_text(message: &str) -> &'static str {
    if message.starts_with("rom:") {
        "Couldn't read this game file. Refresh the library, or choose your ROM folder again."
    } else if message.starts_with("saves:") {
        "Couldn't read your save folder. Choose it again in Library."
    } else {
        "Couldn't open this game. Check the ROM and core."
    }
}

/// Short user-facing sentence on screen; the technical cause goes to logcat.
/// Writes a game's settings off the GL thread (beta42); false without a
/// folder to write them in.
fn save_profile_later(profile: EmulationProfile, uri: &str, core: Core) -> bool {
    let Some(root) = PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|(root, _)| root.clone())
    else { return false };
    let uri = uri.to_owned();
    crate::later_write::write(root.join(format!("game settings\u{1f}{uri}\u{1f}{core:?}")), move || {
        if let Err(error) = profile.save(&root, &uri, core) {
            set_error_message("Couldn't save this game's settings.", &error);
        }
    });
    true
}

/// Android's SystemClock.uptimeMillis(), to set native moments against the
/// Kotlin side's in the journal.
pub(crate) fn uptime_ms() -> i64 {
    let mut now = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: a valid out pointer for clock_gettime.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
    now.tv_sec as i64 * 1000 + now.tv_nsec as i64 / 1_000_000
}

fn set_error_message(message: &str, detail: &dyn std::fmt::Display) {
    crate::game::log_launch_timing(&format!("{message} ({detail})"));
    set_message(message.into());
}


// A12 GL-thread-only diagnostic; no extra simulation step or GPU readback.
// Native draw duration is measured BEFORE GLSurfaceView's EGL buffer swap.
struct A12DrawProbe {
    started: Instant,
    callbacks: u64,
    steps0: u64,
    steps1: u64,
    steps2: u64,
    native_us_max: u64,
    over_16ms: u64,
    over_33ms: u64,
}
impl A12DrawProbe {
    fn new() -> Self {
        Self { started: Instant::now(), callbacks: 0, steps0: 0,
            steps1: 0, steps2: 0, native_us_max: 0,
            over_16ms: 0, over_33ms: 0 }
    }
    fn record(&mut self, steps: usize, us: u64, recovery: bool) {
        self.callbacks += 1;
        match steps {
            0 => self.steps0 += 1,
            1 => self.steps1 += 1,
            _ => self.steps2 += 1,
        }
        self.native_us_max = self.native_us_max.max(us);
        if us > 16_667 { self.over_16ms += 1; }
        if us > 33_333 { self.over_33ms += 1; }
        let span = self.started.elapsed();
        if span >= Duration::from_secs(5) {
            crate::game::log_launch_timing(&format!(
                "Link display pacing: recovery={} callbacks={} emu_fps={:.1} steps0={} steps1={} steps2={} native_us_max={} native_over_16ms={} native_over_33ms={}",
                if recovery { "a11" } else { "legacy" }, self.callbacks,
                (self.steps1 + 2 * self.steps2) as f64 / span.as_secs_f64(),
                self.steps0, self.steps1, self.steps2, self.native_us_max,
                self.over_16ms, self.over_33ms));
            *self = Self::new();
        }
    }
}
thread_local! {
    static A12_DRAW_PROBE: RefCell<Option<A12DrawProbe>> = const { RefCell::new(None) };
    // GL-owner only, no atomics, Java JNI roundtrip or per-frame logging.
    static FRAME_PHASE_PROBE: RefCell<Option<crate::frame_phases::PhaseWindow>> =
        const { RefCell::new(None) };
}
fn a12_draw_sample(linked: bool, steps: usize, native_us: u64, recovery: bool) {
    A12_DRAW_PROBE.with(|probe| {
        let mut probe = probe.borrow_mut();
        if linked {
            probe.get_or_insert_with(A12DrawProbe::new).record(steps, native_us, recovery);
        } else {
            *probe = None;
        }
    });
}

// All engine / OpenGL work occurs on GLSurfaceView's dedicated GL thread.
thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum LinkEntry { Shortcut, GameMenu, Stopped }

impl LinkEntry {
    fn back(self, linked: bool, requires_relaunch: bool) -> ShelfOverlay {
        match self {
            Self::Shortcut => ShelfOverlay::None,
            Self::Stopped => ShelfOverlay::GameMenu { row: 0 },
            Self::GameMenu => ShelfOverlay::GameMenu {
                row: pause_rows(linked, requires_relaunch, true).iter()
                    .position(|item| *item == PauseRow::Multiplayer).unwrap_or(0),
            },
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ShelfOverlay {
    None,
    Shelves { rack: usize },
    Menu { row: usize },
    Library { row: usize },
    /// Library > Saves & States.
    SavesStates { row: usize },
    Settings { row: usize },
    ScreenSettings { row: usize },
    ScreenSystemSettings { platform: Platform, row: usize },
    GameplaySettings { row: usize },
    EmulationSettings { row: usize },
    /// Settings > Interface > Status Bar.
    InterfaceSettings { row: usize },
    /// Settings > Handheld: the RG Rotate's own page.
    RotateSettings { row: usize },
    /// Settings > Interface.
    GeneralSettings { row: usize },
    /// Settings > Interface > Wallpaper: General's, or a shelf's own.
    WallpaperSettings { row: usize },
    /// Settings > Controls, and its Port 1 / Port 2 pages.
    ControlsSettings { row: usize },
    ControlsPort { port: usize, row: usize },
    /// Systems > [system] > Controls (game: false), or the in-game
    /// Settings > Controls of the game being played (game: true).
    LayoutPorts { platform: Platform, game: bool, row: usize },
    /// Settings > Controls > Hotkeys.
    Hotkeys { row: usize },
    LayoutPort { platform: Platform, game: bool, port: usize, row: usize },
    // Pause submenus never mutate the global Settings object.
    PauseSettings { row: usize },
    PauseScreenSettings { row: usize },
    PauseGameplaySettings { row: usize },
    PauseEmulationSettings { row: usize },
    About { row: usize },
    Core,
    GameMenu { row: usize },
    Multiplayer,
    States,
    /// Game menu > Cheats: the game's cheat files, each cheat on or off.
    Cheats { row: usize },
    /// X on a cart: its label and shell (runtime/cart_studio.rs).
    CartStudio,
    CartLibrary,
    /// Library > RomM: the server's games to download (runtime/romm.rs).
    Romm,
}

const MENU_TEXT: [&str; 189] = [
    "MENU",                             // 0
    "Library",                          // 1
    "Cart Studio",                      // 2
    "RetroAchievements",               // 3
    "A Select     B Back",              // 4
    "LIBRARY",                          // 5
    "Choose ROM Folder",               // 6
    "Choose Save Folder",              // 7
    "Choose Save State Folder",        // 8
    "Refresh Library",                 // 9
    "RetroArch core folders",           // 10
    "CART STUDIO",                      // 11
    "A: Open Cart Studio",             // 12
    "RETROACHIEVEMENTS",               // 13
    "A: RA account / sign in",         // 14
    "SELECT CORE",                      // 15
    "mGBA",                             // 16
    "gpSP",                            // 17
    "A Confirm     B Back",             // 18
    "GAME MENU",                        // 19
    "Resume",                           // 20
    "Save and Eject",                   // 21
    "Save States",                      // 22
    "A Select     B Back",              // 23
    "Choose BIOS Folder",               // 24
    "Settings",                         // 25
    "SETTINGS",                         // 26
    "Fast Forward",                     // 27
    "Fast Forward Sound",               // 28
    "Color Correction",                 // 29
    "Rumble",                           // 30
    "About",                            // 31
    "SCREEN",                           // 32
    "GAMEPLAY",                         // 33
    "GBA Shader",                       // 34
    "GB / GBC Shader",                  // 35
    "GB Palette",                       // 36
    "Rewind",                           // 37
    "X/Y Buttons",                       // 38
    "Auto Save on Eject",               // 39
    "",                                 // 40 retired
    "",                                 // 41 retired
    "",                                 // 42 retired
    "Import Wallpaper",                 // 43
    "",                                 // 44 retired
    "Delete Wallpaper",                 // 45
    "",                                 // 46 retired
    "",                                 // 47 retired
    "Apps",                             // 48
    "Emulation",                        // 49
    "EMULATION",                        // 50
    "Show Stats",                       // 51
    "Frame pacing",                     // 52
    "STANDARD",                         // 53
    "RESPONSIVE",                       // 54
    "Run-Ahead",                        // 55
    "GAME EMULATION",                   // 56
    "Reset to global",                  // 57
    "Status Bar",                       // 58
    "STATUS BAR",                       // 59
    "Wi-Fi",                            // 60
    "Bluetooth",                        // 61
    "App Integrations",                 // 62 legacy
    "Wi-Fi Icon",                       // 63
    "Bluetooth Icon",                   // 64
    "App Icons",                        // 65
    "Multiplayer",                      // 66
    "End Link",                         // 67
    "",                                 // 68 retired: old non-Slot Multiplayer title
    "",                                 // 69 retired: LinkRow::text() is upstream
    "",                                 // 70 retired: LinkRow::text() is upstream
    "",                                 // 71 retired: no extraneous mode heading
    "",                                 // 72 retired: no extraneous mode heading
    "",                                 // 73 retired: LinkStep::line() is upstream
    "",                                 // 74 retired: upstream says Linked
    "",                                 // 75 retired: LinkFail::line() is upstream
    "",                                 // 76 retired: no middle-page helper text
    "",                                 // 77 retired: upstream shows A OK
    "Eject to play solo",               // 78
    "Share Core Saves",                 // 79
    "",                                  // 80 retired: value rendered at right
    "Interface",                          // 81, Slot v1.7's General (Siham 2026-11-09)
    "INTERFACE",                          // 82
    "Wallpaper",                          // 83
    "Wallpaper Tint",                     // 84
    "Theme",                              // 85
    "Recents",                            // 86
    "Game Titles",                        // 87
    "GBC Shader",                         // 88
    "GB Shader",                          // 89
    "Bezel",                              // 90
    "Game Boy Advance",                   // 91
    "Game Boy Color",                     // 92
    "Game Boy",                           // 93
    "SOFTWARE UPDATE",                  // 94
    "Check for Updates",                // 95
    "Release Notes",                    // 96
    "Download & Install",              // 97
    "Automatic Checks",                 // 98
    "Up to date",                       // 99
    "New update available",             // 100
    "Checking for updates",             // 101
    "Downloading update",               // 102
    "Unable to check",                  // 103
    "OFF",                              // 104
    "ON",                               // 105
    "Update available",                 // 106
    "SOFTWARE UPDATE  >",              // 107, actionable About row
    "INSTALLED VERSION",                // 108
    "AVAILABLE VERSION",                // 109
    "Install Update",                    // 110
    "UP TO DATE",                        // 111
    "CHECKING...",                       // 112
    "CHECK FAILED",                      // 113
    "DOWNLOADING...",                    // 114
    "AVAILABLE",                         // 115
    "Time Format",                       // 116, Its-TroyM PR #2
    "SYSTEMS",                           // 117
    "Default Core",                      // 118
    "Sun",                               // 119, Boktai's solar sensor
    "Screen Size",                        // 120
    "NES",                               // 121
    "Super Nintendo",                    // 122
    "Mega Drive",                        // 123
    "Shader",                            // 124, NES / SNES / Mega Drive
    "Controls",                          // 125, Settings > Controls
    "Frame Blending",                    // 126, Systems / in-game Screen
    "Clock & Battery",                   // 127, Status Bar
    "Icons",                             // 128
    "Wi-Fi & Bluetooth",                 // 129
    "Apps",                              // 130
    "LCD Effect",                        // 131, Game Boys (Siham 2026-11-09)
    "Dither Blending",                    // 132, Mega Drive GDAPT
    "Shelf",                              // 133, Interface: whose wallpaper
    "Cheats",                             // 134, game menu (Siham 2026-11-09)
    "RomM Server",                        // 135, Library, after Choose ROM Folder
    "Upscaler",                           // 136, GBA Video Screen (Siham 2026-11-10)
    "Sensitivity",                        // 137, tilt and gyro games (Siham 2026-11-12)
    "RG Rotate",                          // 138, Interface, on that console only (Siham 2026-11-14)
    "RG ROTATE",                          // 139
    // 140-141: what the closed Rotate shows in a game and with none
    // (renamed Siham 2026-11-15).
    "Playing",                            // 140
    "Standby",                            // 141
    "Dim After",                          // 142
    "Sleep After",                        // 143
    "Sleep When Closed",                  // 144, the "Lock the screen" right
    "Game Info",                          // 145, Playing's title / time (Siham 2026-11-15)
    "Handheld",                           // 146, Settings: this console's own page (Siham 2026-11-14)
    "HANDHELD \u{b7} RG ROTATE",            // 147
    "WALLPAPER",                          // 148, Interface > Wallpaper
    // 149-152: Library in 2.5 (Siham 2026-11-16)
    "slot. Folder",                       // 149, its place at right (Siham 2026-11-19)
    "Saves & States",                     // 150, Library's page
    "SAVES & STATES",                     // 151
    "Sync Saves with RomM",               // 152
    // 153-155: Interface > Wallpaper's looks (Siham 2026-11-19).
    "Blur",                               // 153
    "Halo",                               // 154
    "Tint",                               // 155, Darken in 2.5's first betas
    "3.0 Test Shelf",                     // 156, Interface (3.0.0-cartes1)
    "Master System",                      // 157, Systems (3.0)
    "Game Gear",                          // 158
    "Neo Geo Pocket",                     // 159
    "WonderSwan",                         // 160
    "Lynx",                               // 161
    "Pokemon mini",                       // 162
    "LCD Filter",                         // 163, Pokemon mini (Siham 2026-11-21)
    // 164-166: the 3.0 systems' own options (3.0.0-materials3).
    "Sprite Limit",                       // 164, Master System, Game Gear
    "Language",                           // 165, Neo Geo Pocket
    "LCD Mode",                           // 166, Pokemon mini
    "Palette",                            // 167, WonderSwan, Pokemon mini
    "Hide Left Bar",                      // 168, Master System
    "FM Sound",                           // 169, Master System
    "Screen Shake",                       // 170, Pokemon mini
    "WS Palette",                         // 171, WonderSwan (Siham 2026-11-22)
    "PK Palette",                         // 172, Pokemon mini
    "Bezel",                              // 173, no longer shown (plain Bezel)
    "Sync States with RomM",              // 174, Library > Saves & States (3.0, beta)
    "Hide Side Bars",                     // 175, NES: left and right (Siham 2026-11-23)
    "Audio Filter",                       // 176, NES + WonderSwan optional analogue audio
    "Shelves",                            // 177, Interface: Scroll or Grid (3.0.1-beta12)
    "Companion",                          // 178, Game Menu: PokeDaisy's companion (X)
    "Quick Menu",                         // 179, Game Menu: PokeDaisy's quick menu (L3)
    "PICO-8",                             // 180, Systems
    "Intro",                              // 181, Screen: PokeDaisy's start-up screens
    "Neo Geo",                            // 182, Systems (3.2)
    "PC Engine",                          // 183
    "BIOS",                               // 184, Neo Geo: AES, MVS or Universe BIOS
    "Region",                             // 185, Neo Geo: Europe, USA or Japan
    "Pad",                                // 186, PC Engine: 2 or 6 buttons
    "Vignette",                           // 187, home consoles: darker edges
    "Curvature",                          // 188, home consoles: a curved tube
];

/// Settings > Systems, top to bottom.
const SYSTEM_ORDER: [Platform; 15] = [
    Platform::Gba, Platform::Gbc, Platform::Gb,
    Platform::Nes, Platform::Snes, Platform::Md,
    Platform::Sms, Platform::Gg, Platform::Ngp, Platform::Ws,
    Platform::Lynx, Platform::Pokemini, Platform::Pico8,
    Platform::NeoGeo, Platform::Pce,
];
const SYSTEM_TITLES: [usize; 15] = [91, 92, 93, 121, 122, 123, 157, 158, 159, 160, 161, 162, 180, 182, 183];
// Special racks follow the supported platforms, not hard-coded GB-era indices.
const RECENTS_RACK: usize = Platform::ALL.len();
const FAVORITES_RACK: usize = RECENTS_RACK + 1;
/// The 3.0 test shelf (3.0.0-cartes1), after Library > RomM's rack.
const TEST_RACK: usize = Platform::ALL.len() + 4;

/// The 3.0 test shelf's carts (3.0.0-cartes1). Every 3.0 system has real
/// carts since 3.0.0-lynxmini1, so the shelf is empty and Interface no
/// longer offers it.
fn test_carts() -> Vec<slot_store::Cart> {
    Vec::new()
}

/// Every Settings value face, by index: setting_face gives these indices.
const SETTING_VALUE_NAMES: [&str; 217] = ["2×", "3×", "4×", "6×", "ON", "OFF",
                              "LCD3x", "Grid", "Dot", "Simpletex",
                              "STANDARD", "RESPONSIVE", "1 FRAME", "2 FRAMES",
                              "DISPLAY SYNC", "TURBO A/B", "A/B",
                              "None", "Random", "Default", "Default Blur",
                              "Dark", "Light", "Low", "Medium", "High", "Custom",
                               "LCD Grid V2", "Grid V2 + GBA", "zfast LCD",
                               "LCD1x", "Simpletex", "AGB-001",
                               "AGS-001", "Sharp Bilinear", "12 HR", "24 HR",
                               "SYNC LAB", "1×", "2×", "3×", "5×", "BUILT-IN",
                               "RA CLOCK LAB", "RETROARCH", "NATIVE",
                               "SLOT.", "GAME BOY", "POCKET", "COLOR",
                               "mGBA", "gpSP", "Gambatte", "SENSOR",
                               "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10",
                               "Shelf", "AUTO",
                               "INTEGER", "ORIGINAL RATIO", "STRETCH",
                               "FCEUmm", "Nestopia", "Snes9x", "bsnes",
                               "Genesis Plus GX", "PicoDrive", "Original",
                               "GBA CLASSIC", "GBA", "GBA SP", "GBA ALT",
                               "NES", "NES SILVER",
                               "SNES", "SNES ALT", "SNES SILVER", "SNES SILVER ALT",
                               "GENESIS CLASSIC", "GENESIS", "MEGA DRIVE",
                               "zfast-crt", "GDV Trinitron", "CRT Geom Deluxe", "crt-pi",
                               "fakelottes", "crt-aperture", "MIX", "GHOSTING",
                               "BOTTOM", "TOP", "BOTH", "WI-FI", "BLUETOOTH", "ALL", "RUNNING",
                               "Dot Matrix", "crt-easymode", "crt-hyllian", "SABR",
                               "Super Eagle", "xBR-lv2",
                               "Pixel Transparency", "Sunlight Shimmer",
                               "Dot Matrix Color",
                               "Bilinear", "FSR", "Anime4K Fast", "Anime4K",
                               "MAX", "crt-royale", "OLED",
                               // 121: a shelf showing General's wallpaper.
                               "GENERAL",
                               // 122-123: the wood that was Default until 2.5.
                               "slot.", "slot. Blur",
                               // 124: the mosaic in a soft glow.
                               "Default Halo",
                               // 125-134 (PERCENT_VALUES): Wallpaper's Blur,
                               // Halo and Tint; 0 % shows OFF (5).
                               "10%", "20%", "30%", "40%", "50%",
                               "60%", "70%", "80%", "90%", "100%",
                               // 135: the Master System and Game Gear's second core.
                               "Gearsystem",
                               // 136, 137: the Neo Geo Pocket's two cores.
                               "Beetle NeoPop", "RACE",
                               // 138, 139: the Lynx's two cores.
                               "Handy", "Beetle Lynx",
                               // 140-148 (OWN_BEZEL_VALUES): slot.'s own 3.0 overlays.
                               "MASTER SYSTEM", "GAME GEAR", "NEO GEO POCKET", "NGP COLOR",
                               "WONDERSWAN", "WONDERSWAN COLOR", "LYNX", "LYNX GOLD",
                               "POKEMON MINI",
                               // 149: Pokemon mini's LCD Filter (Dot Matrix 105, None 17).
                               "Scanlines",
                               // 150-163 (EXTRA_VALUES): the 3.0 systems' own
                               // options; Default (19) leads both palettes.
                               "English", "Japanese",
                               "WonderSwan", "WonderSwan Color", "SwanCrystal",
                               "Old", "Monochrome", "Green", "Red", "Blue", "Sepia",
                               "Analog", "3 Shades", "2 Shades",
                               // 164-168: optional audio coloration; these must stay
                               // before Rotate's 21 terminal values.
                               "RF", "Soft", "MAME", "Nuked", "Analog",
                               // 169-170 (SHELVES_LAYOUT_VALUES): Interface > Shelves.
                               "SCROLL", "GRID",
                               // 171-177 (POKEDAISY_VIEW_VALUES): PokeDaisy's Screen Size,
                               // ZOOM's first step, EXPAND, ORIGINAL, then ZOOM's steps 2 to 5.
                               "ZOOM 1", "EXPAND", "ORIGINAL", "ZOOM 2", "ZOOM 3", "ZOOM 4", "ZOOM 5",
                               // 178-179 (PICO8_CORE_VALUES): PICO-8's two cores.
                               "fake-08", "PICO-8",
                               // 180-183 (NEOGEO_PCE_CORE_VALUES): the Neo Geo's and PC
                               // Engine's cores; 184-187 (NEOGEO_BIOS_VALUES): the Neo
                               // Geo's BIOS (AUTO is 66), a choice whose file is missing.
                               "FinalBurn Neo", "Geolith", "Beetle PCE Fast", "Beetle PCE",
                               "AES", "MVS", "UNIVERSE BIOS", "NO aes.zip",
                               // WATOMSK_3_2_VALUES: Watomsk's Neo Geo,
                               // PC Engine and TurboGrafx-16 overlays.
                               "NEO GEO", "PC ENGINE", "TURBOGRAFX-16",
                               // SPECIFIC_3_2_VALUES: the Neo Geo's Region, the PC
                               // Engine's Pad.
                               "EUROPE", "USA", "JAPAN", "2 BUTTONS", "6 BUTTONS",
                               // RG Rotate (ROTATE_VALUES on): keep these 21 last.
                               "INSERTED CART", "FULL CART", "LAST FRAME",
                               "DRIFTING SHELF", "LAST PLAYED", "RANDOM CART", "BLACK",
                               "5 S", "10 S", "15 S", "30 S", "1 MIN", "2 MIN", "5 MIN", "NEVER",
                               "NEEDS SWIPE LOCK", "DIGITAL CLOCK", "ANALOG CLOCK",
                               // 18-20: Game Info.
                               "TITLE", "TIME", "ALL"];
/// The first of slot.'s own 3.0 overlays in SETTING_VALUE_NAMES.
const OWN_BEZEL_VALUES: usize = 140;
/// The first of the 3.0 systems' own option values in SETTING_VALUE_NAMES.
const EXTRA_VALUES: usize = 150;
/// NES/WonderSwan filter and Mega Drive FM values (before Rotate values).
const AUDIO_OPTION_VALUES: usize = EXTRA_VALUES + 14;
/// Interface > Shelves' Scroll and Grid in SETTING_VALUE_NAMES.
const SHELVES_LAYOUT_VALUES: usize = AUDIO_OPTION_VALUES + 5;
/// PokeDaisy's views (ZOOM 1, EXPAND, ORIGINAL, ZOOM 2 to 5) in SETTING_VALUE_NAMES.
const POKEDAISY_VIEW_VALUES: usize = SHELVES_LAYOUT_VALUES + 2;
/// PICO-8's cores' names, after PokeDaisy's seven views.
const PICO8_CORE_VALUES: usize = POKEDAISY_VIEW_VALUES + 7;
/// The Neo Geo's and PC Engine's cores, after PICO-8's two.
const NEOGEO_PCE_CORE_VALUES: usize = PICO8_CORE_VALUES + 2;
/// The Neo Geo's BIOS: AES, MVS, Universe BIOS, then "NO aes.zip".
const NEOGEO_BIOS_VALUES: usize = NEOGEO_PCE_CORE_VALUES + 4;
/// Watomsk's Neo Geo, PC Engine and TurboGrafx-16 overlays' names.
const WATOMSK_3_2_VALUES: usize = NEOGEO_BIOS_VALUES + 4;
/// The Neo Geo's Region and the PC Engine's Pad in SETTING_VALUE_NAMES.
const SPECIFIC_3_2_VALUES: usize = WATOMSK_3_2_VALUES + 3;

/// A PokeDaisy view's value face.
/// A game's screen settings, read before its screen goes on.
struct ScreenReady {
    mode: VideoMode,
    source: &'static str,
    frame: (u32, u32),
    square: bool,
    view: crate::pokedaisy_view::View,
    intro: bool,
}

fn pokedaisy_view_value(view: crate::pokedaisy_view::View) -> usize {
    use crate::pokedaisy_view::View;
    POKEDAISY_VIEW_VALUES + match view {
        View::Zoom(step) if step >= 2 => 3 + usize::from(step.min(5)) - 2,
        View::Zoom(_) => 0,
        View::Expand => 1,
        View::Original => 2,
    }
}
/// The first RG Rotate value in SETTING_VALUE_NAMES.
const ROTATE_VALUES: usize = SETTING_VALUE_NAMES.len() - 21;
/// Library's values (2.5): where the slot. folder is, and RomM's sync
/// before RomM is connected.
const LIBRARY_VALUE_NAMES: [&str; 3] = ["INTERNAL STORAGE", "SD CARD", "NEEDS ROMM"];
/// Values' short forms, shown only where the whole one has no room even
/// 10 % smaller (the RG Rotate, Siham 2026-11-19); others scroll.
const SHORT_VALUES: [(&str, &str); 3] = [
    ("ORIGINAL RATIO", "ORIG. RATIO"),
    ("INTERNAL STORAGE", "INTERNAL"),
    ("Genesis Plus GX", "GPGX"),
];
/// Labels too long for their value on a narrow screen (the RG Rotate's
/// square one, Siham 2026-11-19).
fn narrow_label(label: &str) -> &str {
    if out_w() >= 600 { return label; }
    match label {
        "Fast Forward Sound" => "FF Sound",
        "Sync Saves with RomM" => "Sync with RomM",
        "Sync States with RomM" => "Sync States",
        "Wi-Fi & Bluetooth" => "Wi-Fi & BT",
        _ => label,
    }
}
/// The slot. folder's place: 0 none, 1 internal storage, 2 SD card.
static SLOT_FOLDER: AtomicU8 = AtomicU8::new(0);
/// The slot. folder's first copy under way: its percent, -1 none.
static SLOT_FOLDER_COPY: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);
/// The task pill (beta39): Android's long tasks, most important first, a
/// line each, "kind<TAB>progress<TAB>words" (progress -1 for none); its
/// revision changes with it.
static TASK_PILL: Mutex<String> = Mutex::new(String::new());
static TASK_PILL_REV: AtomicU64 = AtomicU64::new(0);
/// Android saw a touchscreen among the input devices (beta41): Controls
/// shows Touchscreen only then. True until known.
static HAS_TOUCHSCREEN: AtomicBool = AtomicBool::new(true);
/// Wi-Fi and Bluetooth's width on the status line, from Android.
static STATUS_ICONS_W: AtomicU32 = AtomicU32::new(0);
/// When the GL surface was last made (uptime ms), until its first frame is
/// drawn and timed: after sleep, how long the screen stays black (beta43).
static FIRST_FRAME_SINCE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
/// The last frame drawn (uptime ms): a frame after a long gap (sleep, the
/// screen off) is written down with its gap.
static LAST_FRAME_AT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
const TASK_PROBLEM: u8 = 0;
const TASK_COPY: u8 = 1;
const TASK_DOWNLOADS: u8 = 3;
const TASK_SCAN: u8 = 5;
/// Sync Saves with RomM: 0 RomM not connected, 1 off, 2 on.
static ROMM_SYNC: AtomicU8 = AtomicU8::new(0);
/// Sync States with RomM (3.0): as ROMM_SYNC, and 3 off and idle (Sync
/// Saves with RomM is off: a state holds its save).
pub(crate) static ROMM_STATES: AtomicU8 = AtomicU8::new(0);
/// The ROM folder's SD card isn't in this handheld.
static ROMS_MISSING: AtomicBool = AtomicBool::new(false);
/// Carts whose SD card is out: grey, as a handheld switched off, opaque
/// (Siham 2026-11-18). Lighter than beta38's (Siham 2026-11-19, beta39):
/// "D" over a light ground, "C" over a dark one, which look alike there.
const MISSING_INK_LIGHT: slot_gfx::InkMap = slot_gfx::InkMap { paper: [0.36, 0.36, 0.37], ink: [0.80, 0.80, 0.82] };
const MISSING_INK_DARK: slot_gfx::InkMap = slot_gfx::InkMap { paper: [0.22, 0.22, 0.23], ink: [0.68, 0.68, 0.70] };
const MISSING_FADE_S: f32 = 0.3;
/// Settings > Interface > RG Rotate: its rows and their keys (85, Sleep
/// When Closed, is Android's "Lock the screen" right, not a setting).
/// Game Info (84) right under Playing, which it shows on; Animations are
/// always on (Siham 2026-11-15).
const ROTATE_ROWS: [usize; 6] = [140, 145, 141, 142, 143, 144];
const ROTATE_KEYS: [usize; 6] = [80, 84, 81, 82, 83, 85];
/// A game runs with nothing over it (Android keeps the screen on).
pub(crate) static GAME_UNPAUSED: AtomicBool = AtomicBool::new(false);
/// Sleep When Closed (RG Rotate): 0 off, 1 on (slot. holds Android's "Lock
/// the screen" right), 2 on but the Rotate has no lock screen to sleep under.
pub(crate) static ROTATE_SLEEP_STATE: AtomicU8 = AtomicU8::new(0);
/// RG Rotate's dim and sleep delays in s, packed dim << 16 | sleep, bit 30
/// the black cover with no game, bit 31 animations on.
pub(crate) static ROTATE_DELAYS: AtomicU32 = AtomicU32::new(15 << 16 | 60 | 1 << 31);

/// An Anbernic RG Rotate (ro.product.model "RG Rotate", Siham's, 2026-11-14).
fn rg_rotate() -> bool {
    DEVICE_NAME.lock().unwrap_or_else(|e| e.into_inner()).1.contains("RG Rotate")
}

/// A near-square panel (the RG Rotate, the Retroid Pocket Classic): no room
/// on the console bar for the clock, battery or icons, so they stay on top
/// and Status Bar doesn't offer the bar (Siham 2026-11-14). 4:3 still does.
fn near_square_panel() -> bool {
    let packed = PANEL_SIZE.load(Ordering::Acquire);
    let (w, h) = ((packed >> 32) as u32, packed as u32);
    if w == 0 || h == 0 { return false; }
    (w.max(h) as f32 / w.min(h) as f32) < 1.25
}

/// The RG Rotate's close and open animations, in s.
const LID_ANIMATION_S: f32 = 0.45;

/// A draw scaled `k` times about (`cx`, `cy`), moved down `dy`, faded.
/// The closed Rotate's clocks, in canvas units (480 high). Faces are drawn
/// LID_CLOCK_FINE times finer than the status bar's, so they stay sharp.
const LID_CLOCK_FINE: u32 = 8;
/// A console slot bar's middle (slot_ui::refit_band keeps 160 px either side
/// of the centre), where its opening and housing sit.
const HOME_SLOT_BAY_W: f32 = 320.0;
/// Inserted Cart: the time's middle and size, above the seated cart.
const LID_INSERTED_CLOCK_Y: f32 = 175.0;
const LID_INSERTED_CLOCK_SCALE: f32 = 4.0;
/// Digital Clock: the time's middle and size.
const LID_DIGITAL_CLOCK_Y: f32 = 190.0;
const LID_DIGITAL_CLOCK_SCALE: f32 = 5.0;
/// Analog Clock: the dial's size and top.
const LID_DIAL_SIZE: f32 = 290.0;
const LID_DIAL_TOP: f32 = 52.0;
const LID_DATE_SCALE: f32 = 1.5;
const LID_BATTERY_SCALE: f32 = 1.4;

fn lid_place(draw: Draw, cx: f32, cy: f32, k: f32, dy: f32, fade: f32) -> Draw {
    let place = |x: f32, y: f32, w: f32, h: f32| (cx + (x - cx) * k, cy + (y - cy) * k + dy, w * k, h * k);
    match draw {
        Draw::Rect { x, y, w, h, colour } => {
            let (x, y, w, h) = place(x, y, w, h);
            Draw::Rect { x, y, w, h, colour: [colour[0], colour[1], colour[2], colour[3] * fade] }
        }
        Draw::Tex { x, y, w, h, tex, alpha } => {
            let (x, y, w, h) = place(x, y, w, h);
            Draw::Tex { x, y, w, h, tex, alpha: alpha * fade }
        }
        Draw::Turned { x, y, w, h, tex, alpha, turn } => {
            let (x, y, w, h) = place(x, y, w, h);
            Draw::Turned { x, y, w, h, tex, alpha: alpha * fade, turn }
        }
        other => other,
    }
}

fn interface_rows() -> (&'static [usize], &'static [usize]) {
    if near_square_panel() { (&INTERFACE_ROWS[2..], &INTERFACE_KEYS[2..]) }
    else { (&INTERFACE_ROWS, &INTERFACE_KEYS) }
}

/// Whether the Neo Geo BIOS `name` (aes.zip, neogeo.zip) is among the BIOS
/// files Android staged from the ROM and BIOS folders.
fn neogeo_bios_present(name: &str) -> bool {
    PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        .is_some_and(|(root, _)| root.join("BIOS").join(name).is_file())
}

/// The value face (an index in SETTING_VALUE_NAMES) a Settings key shows,
/// None for an action-only row. Every key a menu lists must have one: the
/// test every_settings_row_shows_its_value checks all menus on all screens.
fn setting_face(key: usize, settings: &Settings, scoped: &Settings,
                profile: &EmulationProfile, cart: Platform, video: VideoMode,
                cores: &[Core]) -> Option<usize> {
    use crate::settings::Shader;
    let shader = |shader| match shader {
        Shader::Off => 5, Shader::Lcd3x => 6, Shader::Grid => 7,
        Shader::Dot => 8, Shader::Simpletex => 9,
         Shader::LcdGridV2 => 27, Shader::LcdGridV2Gba => 28,
         Shader::ZfastLcd => 29, Shader::Lcd1x => 30,
         Shader::SimpletexGba => 31, Shader::Agb001 => 32,
         Shader::Ags001 => 33, Shader::SharpBilinear => 34,
         Shader::ZfastCrt => 90, Shader::CrtGdvTrinitron => 91,
         Shader::CrtGeomDeluxe => 92, Shader::CrtPi => 93,
         Shader::Fakelottes => 94, Shader::CrtAperture => 95,
         Shader::DotMatrix => 105, Shader::CrtEasymode => 106,
         Shader::CrtHyllian => 107, Shader::Sabr => 108,
         Shader::SuperEagle => 109, Shader::XbrLv2 => 110,
         Shader::DotMatrixColor => 113, Shader::CrtRoyale => 119,
    };
    Some(match key {
        0 => shader(scoped.shader_gba),
        1 => shader(scoped.shader_gb),
        2 => if scoped.shader_for(cart).includes_gba_color() {42}
             else if scoped.colour_for(cart) {4}else{5},
        3 => if scoped.gb_palettes { 4 } else { 5 },
        4 => match scoped.ff_speed { 0 => 5, 2 => 0, 3 => 1, 4 => 2,
            crate::settings::FF_MAX => 118, _ => 3 },
        5 => if scoped.ff_sound { 4 } else { 5 },
        6 => if !scoped.rewind { 5 } else {
            match scoped.rewind_speed { 1 => 38, 2 => 39, 3 => 40, _ => 41 }
        },
        7 => if scoped.turbo { 15 } else { 16 },
        8 => if !scoped.rumble { 5 } else {
            match scoped.rumble_level.clamp(1,4) { 1 => 23, 2 => 24, 3 => 25, _ => 118 }
        },
        9 => if scoped.eject_save { 4 } else { 5 },
        10 => if profile.show_fps { 4 } else { 5 },
        11 => if profile.responsive_loop { 11 } else { 45 },
        12 => match profile.run_ahead_frames { 1 => 12, 2 => 13, _ => 5 },
        13 => match (settings.show_wifi, settings.show_bluetooth) {
            (true, true) => 100, (true, false) => 101, (false, true) => 102, (false, false) => 5,
        },
        14 => if settings.show_bluetooth { 4 } else { 5 },
        15 => match (settings.show_integrations, settings.apps_running_only) {
            (false, _) => 5, (true, false) => 103, (true, true) => 104,
        },
        32 => if settings.clock_on_top { 99 } else { 98 },
        33 => if settings.icons_on_bottom { 98 } else { 99 },
        16 => match &settings.wallpaper {
            crate::settings::WallpaperPick::None => 17,
            crate::settings::WallpaperPick::Random => 18,
            crate::settings::WallpaperPick::Shelf => 65,
            crate::settings::WallpaperPick::Default => 19,
            crate::settings::WallpaperPick::Blur => 20,
            crate::settings::WallpaperPick::Halo => HALO_WALLPAPER_VALUE,
            crate::settings::WallpaperPick::Slot => SLOT_WALLPAPER_VALUE,
            crate::settings::WallpaperPick::SlotBlur => SLOT_BLUR_WALLPAPER_VALUE,
            crate::settings::WallpaperPick::File(_) => 26,
        },
        17 => match settings.wallpaper_tint {
            crate::settings::WallpaperTint::Off => 5,
            crate::settings::WallpaperTint::Low => 23,
            crate::settings::WallpaperTint::Medium => 24,
            crate::settings::WallpaperTint::High => 25,
        },
        18 => match settings.appearance {
            crate::settings::Appearance::Dark => 21,
            crate::settings::Appearance::Light => 22,
            crate::settings::Appearance::Original => 76,
            crate::settings::Appearance::Oled => 120,
        },
        19 => if settings.show_recents {4}else{5},
        20 => if settings.show_titles {4}else{5},
        SHELVES_LAYOUT_KEY => SHELVES_LAYOUT_VALUES + usize::from(settings.shelves_grid),
        TEST_SHELF_KEY => if settings.test_shelf {4}else{5},
        21 => shader(scoped.shader_gbc),
        47 => shader(scoped.shader_nes),
        48 => shader(scoped.shader_snes),
        49 => shader(scoped.shader_md),
        crate::settings::SHADER_SMS_KEY => shader(scoped.shader_sms),
        crate::settings::SHADER_NGP_KEY => shader(scoped.shader_ngp),
        crate::settings::SHADER_WS_KEY => shader(scoped.shader_ws),
        crate::settings::SHADER_LYNX_KEY => shader(scoped.shader_lynx),
        crate::settings::SHADER_POKEMINI_KEY => shader(scoped.shader_pokemini),
        crate::settings::SHADER_PICO8_KEY => shader(scoped.shader_pico8),
        crate::settings::SHADER_GG_KEY => shader(scoped.shader_gg),
        crate::settings::SHADER_NEOGEO_KEY => shader(scoped.shader_neogeo),
        crate::settings::SHADER_PCE_KEY => shader(scoped.shader_pce),
        // Auto, AES, MVS, Universe BIOS; AES without aes.zip says so (the
        // game then starts on the MVS BIOS).
        crate::settings::NEOGEO_BIOS_KEY => match scoped.neogeo_bios.min(3) {
            0 => 66,
            1 if !neogeo_bios_present("aes.zip") => NEOGEO_BIOS_VALUES + 3,
            n => NEOGEO_BIOS_VALUES + usize::from(n) - 1,
        },
        22 => if scoped.shader_gba.includes_gba_color() { 42 }
            else if scoped.colour_correction_gba {4}else{5},
        23 => if scoped.colour_correction_gbc {4}else{5},
        24 => if scoped.colour_correction_gb {4}else{5},
        25 => bezel_value(scoped.bezel_for(Platform::Gba)),
        44 => bezel_value(scoped.bezel_nes),
        45 => bezel_value(scoped.bezel_snes),
        46 => bezel_value(scoped.bezel_md),
        crate::settings::BEZEL_SMS_KEY..=crate::settings::BEZEL_POKEMINI_KEY
        | crate::settings::BEZEL_NEOGEO_KEY | crate::settings::BEZEL_PCE_KEY =>
            bezel_value(scoped.bezel_for(Settings::bezel_platform(key)?)),
        crate::settings::NEOGEO_REGION_KEY => SPECIFIC_3_2_VALUES + usize::from(scoped.neogeo_region.min(2)),
        crate::settings::PCE_PAD_KEY => SPECIFIC_3_2_VALUES + 3 + usize::from(scoped.pce_six_buttons),
        crate::settings::VIGNETTE_NES_KEY..=crate::settings::CURVATURE_PCE_KEY => {
            let (platform, curve) = Settings::crt_platform(key)?;
            percent_value(scoped.crt_look(platform, curve) * 10)
        }
        crate::settings::POKEMINI_FILTER_KEY => match scoped.pokemini_lcd_filter {
            1 => 149, 2 => 17, _ => 105,
        },
        // The 3.0 systems' own options: the limit is ON as on the console.
        crate::settings::SPRITE_LIMIT_SMS_KEY => if scoped.no_sprite_limit_sms {5}else{4},
        crate::settings::SPRITE_LIMIT_GG_KEY => if scoped.no_sprite_limit_gg {5}else{4},
        crate::settings::NGP_LANGUAGE_KEY => EXTRA_VALUES + usize::from(scoped.ngp_language.min(1)),
        crate::settings::WS_PALETTE_KEY => match scoped.ws_palette.min(3) {
            0 => 19, n => EXTRA_VALUES + 1 + usize::from(n) },
        crate::settings::POKEMINI_PALETTE_KEY => match scoped.pokemini_palette.min(6) {
            0 => 19, n => EXTRA_VALUES + 4 + usize::from(n) },
        crate::settings::POKEMINI_LCD_MODE_KEY =>
            EXTRA_VALUES + 11 + usize::from(scoped.pokemini_lcd_mode.min(2)),
        crate::settings::LEFT_BAR_SMS_KEY => if scoped.hide_left_bar_sms {4}else{5},
        crate::settings::LEFT_BAR_NES_KEY => if scoped.hide_left_bar_nes {4}else{5},
        // FM sound on its own for the games that have it, as the Japanese console.
        crate::settings::FM_SMS_KEY => if scoped.fm_off_sms {5}else{66},
        crate::settings::WS_LANGUAGE_KEY => EXTRA_VALUES + usize::from(scoped.ws_language.min(1)),
        crate::settings::POKEMINI_SHAKE_KEY => if scoped.pokemini_no_shake {5}else{4},
        crate::settings::NES_AUDIO_FILTER_KEY => if !scoped.nes_audio_filter { 5 }
            else if cores[Platform::Nes as usize] == Core::Nestopia { AUDIO_OPTION_VALUES + 4 }
            else { AUDIO_OPTION_VALUES },
        crate::settings::MD_FM_SOUND_KEY =>
            AUDIO_OPTION_VALUES + 2 + usize::from(scoped.md_fm_nuked),
        crate::settings::WS_AUDIO_FILTER_KEY => if scoped.ws_audio_filter {
            AUDIO_OPTION_VALUES + 1
        } else { 5 },
        SCREEN_SIZE_KEY => SCREEN_SIZE_VALUE + match video {
            VideoMode::Actual => 0, VideoMode::Aspect => 1, VideoMode::Stretch => 2,
        },
        26 => bezel_value(scoped.bezel_gbc),
        27 => bezel_value(scoped.bezel_gb),
        28 => if settings.time_format==crate::settings::TimeFormat::TwelveHour
            {35}else{36},
        29..=31 | 41..=43 | crate::settings::CORE_SMS_KEY | crate::settings::CORE_GG_KEY
        | crate::settings::CORE_NGP_KEY | crate::settings::CORE_LYNX_KEY
        | crate::settings::CORE_PICO8_KEY | crate::settings::CORE_PCE_KEY =>
            core_value(Engine::core_key_platform(key)
            .map_or(Core::Mgba, |platform| cores[platform as usize])),
        // Frame Blending, LCD Effect and Dither Blending: the game's own in
        // game, the system's default in Settings > Systems.
        60..=65 | crate::settings::BLENDING_SMS_KEY | crate::settings::BLENDING_GG_KEY
        | crate::settings::BLENDING_NGP_KEY | crate::settings::BLENDING_WS_KEY
        | crate::settings::BLENDING_LYNX_KEY | crate::settings::BLENDING_POKEMINI_KEY
        | crate::settings::BLENDING_PICO8_KEY | crate::settings::BLENDING_NEOGEO_KEY
        | crate::settings::BLENDING_PCE_KEY => {
            match scoped.blending_for(Settings::blending_platform(key)?) {
                crate::settings::Blending::Off => 5,
                crate::settings::Blending::Mix => 96,
                crate::settings::Blending::Ghosting => 97,
            }
        }
        k if Settings::lcd_effect_platform(k).is_some() => {
            match scoped.lcd_effect_for(Settings::lcd_effect_platform(k)?) {
                crate::settings::LcdEffect::Off => 5,
                crate::settings::LcdEffect::Transparency => 111,
                crate::settings::LcdEffect::Sunlight => 112,
            }
        }
        69 => if scoped.dither_blend_md { 4 } else { 5 },
        80 => ROTATE_VALUES + usize::from(settings.rotate_game_cover.min(2)),
        81 => match settings.rotate_shelf_cover {
            4 => ROTATE_VALUES + 16, 5 => ROTATE_VALUES + 17,
            n => ROTATE_VALUES + 3 + usize::from(n.min(3)) },
        82 => ROTATE_VALUES + 7 + usize::from(settings.rotate_dim.min(3)),
        83 => match settings.rotate_delays().1 { 30 => ROTATE_VALUES + 10, 60 => ROTATE_VALUES + 11,
            120 => ROTATE_VALUES + 12, 300 => ROTATE_VALUES + 13, _ => ROTATE_VALUES + 14 },
        // Game Info: Off, Title, Time, All.
        84 => match settings.rotate_game_info { 0 => 5, n => ROTATE_VALUES + 17 + usize::from(n.min(3)) },
        85 => match ROTATE_SLEEP_STATE.load(Ordering::Acquire) {
            1 => 4, 2 => ROTATE_VALUES + 15, _ => 5 },
        70 => match scoped.upscaler_gbav {
            crate::settings::Upscaler::Off => 5,
            crate::settings::Upscaler::Bilinear => 114,
            crate::settings::Upscaler::Fsr => 115,
            crate::settings::Upscaler::Anime4kFast => 116,
            crate::settings::Upscaler::Anime4k => 117,
        },
        _ => return None,
    })
}

/// The library's carts by system shelf, and the GBA Video carts (their own
/// shelf, behind the GBA's, as slot. original 1.15), each with its
/// app-private label path.
fn library_carts(library: Option<&[RomEntry]>, root: Option<&std::path::Path>)
    -> (Vec<Vec<slot_store::Cart>>, Vec<slot_store::Cart>)
{
    let mut grouped = library.map(|entries| {
        if let Some(root) = root {
            carts_by_platform(&crate::library::load_shell_profiles(root, entries))
        } else { carts_by_platform(entries) }
    }).unwrap_or_else(|| std::array::from_fn(|_| Vec::new()));
    let (video, gba): (Vec<slot_store::Cart>, Vec<slot_store::Cart>) =
        std::mem::take(&mut grouped[Platform::Gba as usize]).into_iter()
            .partition(carousel_delete::is_gba_video);
    grouped[Platform::Gba as usize] = gba;
    let mut systems: Vec<Vec<slot_store::Cart>> = grouped.into_iter().collect();
    systems.push(video);
    if let Some(root) = root {
        // Labels by each game's portable name (2.5); a 2.4 label, named by
        // the Android address, is carried over. One listing of the folder
        // rather than a check per cart (thousands at startup).
        let keys = crate::library::game_keys(library.unwrap_or_default());
        let labels = root.join("Labels");
        let present: std::collections::HashSet<std::ffi::OsString> = std::fs::read_dir(&labels)
            .map(|dir| dir.flatten().map(|f| f.file_name()).collect()).unwrap_or_default();
        for cart in systems.iter_mut().flatten() {
            let uri = cart.rom.to_string_lossy().into_owned();
            let name = keys.get(&uri).map(String::as_str).unwrap_or(&uri);
            let filename = format!("{}.png", core_selection::key_of_name(name));
            let legacy = format!("{}.png", core_selection::legacy_key(&uri));
            if !present.contains(std::ffi::OsStr::new(&filename))
                && present.contains(std::ffi::OsStr::new(&legacy)) {
                core_selection::carry(&labels.join(&legacy), &labels.join(&filename));
            }
            // Only app-private labels are used; never rewrite ROMs.
            cart.label = Some(labels.join(filename));
        }
        // A Master System or Game Gear cart whose game has kept a battery
        // save shows its battery, as the known ones do (Siham 2026-11-22).
        let saved: std::collections::HashSet<std::ffi::OsString> = ["Genesis Plus GX", "Gearsystem"]
            .iter().flat_map(|core| std::fs::read_dir(root.join("Saves").join(core)).into_iter().flatten())
            .flatten().map(|f| f.file_name()).collect();
        for platform in [Platform::Sms, Platform::Gg] {
            for cart in systems[platform as usize].iter_mut().filter(|cart| !cart.battery) {
                cart.battery = saved.contains(std::ffi::OsStr::new(&format!("{}.srm", cart.stem)));
            }
        }
        // Splore, first on the PICO-8 shelf while PICO-8 is in the BIOS folder.
        if let Some(splore) = library.and_then(|_| crate::pico8::splore_cart(root)) {
            systems[Platform::Pico8 as usize].insert(0, splore);
        }
    }
    let video = systems.pop().unwrap_or_default();
    (systems, video)
}

/// Simpletex's paper (slot.'s own, restored in 2.1.2) and Dot Matrix's
/// background grain (LuigiRa's dmg-bg, GPL-3), both greyscale PNGs.
const PAPER_PNG: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/shaders/paper.png"));
const DOT_MATRIX_PNG: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/shaders/dmg-bg.png"));

/// A square greyscale PNG as RGBA pixels for the GPU.
fn shader_texture(png: &[u8]) -> Option<(u32, Vec<u8>)> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.width != info.height || info.bit_depth != png::BitDepth::Eight { return None; }
    let channels = info.color_type.samples();
    let pixels = (info.width * info.height) as usize;
    let rgba = buf[..pixels * channels].chunks_exact(channels).flat_map(|p| match channels {
        1 | 2 => [p[0], p[0], p[0], 255],
        _ => [p[0], p[1], p[2], 255],
    }).collect();
    Some((info.width, rgba))
}

/// Dot Matrix colours for the game's palette: its lightest shade behind,
/// its dark shade for the dots. Without a palette the cores colour the
/// game, and the dots take the classic DMG preset's green.
fn dot_matrix_colours(view: &Settings) -> [u32; 4] {
    if view.gb_palettes {
        let palette = slot_store::GbPalette::all().nth(view.gb_palette as usize)
            .unwrap_or(slot_store::GbPalette::DEFAULT);
        let [light, dots, dark] = palette.dot_matrix();
        [light, dots, light, dark]
    } else {
        [0xD9C425, 0x1D6A6B, 0xFFFFFF, 0x000000]
    }
}

/// A core's name among the setting values.
fn core_value(core: Core) -> usize {
    match core {
        Core::Mgba => 50, Core::Gpsp => 51, Core::Gambatte => 52,
        Core::Gearsystem => 135, Core::Neopop => 136, Core::Race => 137,
        Core::Handy => 138, Core::BeetleLynx => 139,
        Core::Fake08 => PICO8_CORE_VALUES, Core::Pico8 => PICO8_CORE_VALUES + 1,
        Core::FbNeo => NEOGEO_PCE_CORE_VALUES, Core::Geolith => NEOGEO_PCE_CORE_VALUES + 1,
        Core::BeetlePceFast => NEOGEO_PCE_CORE_VALUES + 2, Core::BeetlePce => NEOGEO_PCE_CORE_VALUES + 3,
        other => 70 + other.index() - Core::Fceumm.index(),
    }
}

/// Pause Emulation's Sun row, drawn with the sun values, not a Settings key.
const SUN_KEY: usize = 100;
/// Pause Emulation's Sensitivity row (tilt and gyro games), drawn with the
/// number values 1 to 10.
const MOTION_KEY: usize = 102;
/// Pause Screen's Intro row (PokeDaisy in EXPAND or ZOOM): the start-up
/// screens in ORIGINAL or EXPAND, kept per game.
const POKEDAISY_INTRO_KEY: usize = 161;
/// The longest a core picker waits for a clear cart's lid before opening.
const PICKER_LID_WAIT_MS: u64 = 600;
/// Interface > Wallpaper > Shelf's value: GENERAL or a shelf name.
const SHELF_PICK_KEY: usize = 101;
/// "GENERAL" among the setting values: a shelf showing General's wallpaper.
const GENERAL_VALUE: usize = 121;
/// About's X: MainActivity saves slot.'s logs where the player picks (#107).
const EXPORT_LOGS_ACTION: i32 = 80;
/// Wallpaper slot. and slot. Blur's value faces.
const SLOT_WALLPAPER_VALUE: usize = 122;
const SLOT_BLUR_WALLPAPER_VALUE: usize = 123;
const HALO_WALLPAPER_VALUE: usize = 124;
/// Interface > Wallpaper > Wallpaper's value: the edited shelf's own
/// wallpaper (GENERAL when it shows General's), or General's.
const WALLPAPER_PICK_KEY: usize = 103;

/// Hand a motion game's sensitivity to the sensors; every other game
/// reads them as they are.
fn apply_motion(game: Option<&GameSession>, level: u8) {
    let motion = game.is_some_and(GameSession::motion);
    slot_retro::sensors::set_sensitivity(
        if motion { crate::emulation::motion_factor(level) } else { 1.0 });
}

fn light_sensor() -> bool {
    slot_retro::sensors::available() & slot_retro::sensors::ILLUMINANCE != 0
}

/// GB/GBC Bezel row value: OFF, SLOT., or a Watomsk overlay's name.
fn bezel_value(bezel: crate::settings::Bezel) -> usize {
    use crate::settings::Bezel::*;
    match bezel {
        None => 5,
        Default => 46,
        Gb => 47,
        Pocket => 48,
        Color => 49,
        // Watomsk's other designs: setting_values from 77, in this order.
        GbaClassic => 77, Gba => 78, GbaSp => 79, GbaAlt => 80,
        Nes => 81, NesGrayLogo => 82,
        Snes => 83, SnesAlt => 84, SnesGrayLogo => 85, SnesGrayLogoAlt => 86,
        GenesisClassic => 87, Genesis => 88, MegaDrive => 89,
        // slot.'s own 3.0 overlays: OWN_BEZEL_VALUES on, AUTO is 66.
        NgpAuto | WsAuto => 66,
        MasterSystem => 46, GameGear => OWN_BEZEL_VALUES + 1,
        Ngp => OWN_BEZEL_VALUES + 2, NgpColor => OWN_BEZEL_VALUES + 3,
        Ws => OWN_BEZEL_VALUES + 4, WsColor => OWN_BEZEL_VALUES + 5,
        Lynx => OWN_BEZEL_VALUES + 6, LynxGold => OWN_BEZEL_VALUES + 7,
        PokemonMini => OWN_BEZEL_VALUES + 8,
        // 3.2: Watomsk's Master System takes the name, slot.'s own is SLOT.
        WatomskMasterSystem => OWN_BEZEL_VALUES,
        PceAuto => 66,
        NeoGeo => WATOMSK_3_2_VALUES, PcEngine => WATOMSK_3_2_VALUES + 1,
        TurboGrafx => WATOMSK_3_2_VALUES + 2,
    }
}

/// Gameplay page labels and the Settings keys they change, in row order.
/// Settings: General, Systems, Controls, Gameplay, Emulation, Status Bar.
// Interface, Systems, Gameplay, Emulation, Controls, then Handheld on a
// console with a page of its own (the RG Rotate). Status Bar is under
// Interface (Siham 2026-11-14).
const SETTINGS_ROWS: [usize; 6] = [81, 117, 33, 49, 125, 146];
fn settings_rows() -> &'static [usize] {
    if rg_rotate() { &SETTINGS_ROWS } else { &SETTINGS_ROWS[..5] }
}

/// Settings > Interface: Theme, Wallpaper (its page), Recents, Game Titles,
/// Shelves (Scroll or Grid), Status Bar (its page).
/// 3.0 Test Shelf (156) left it in 3.0.0-lynxmini1: no test cart is left.
const INTERFACE_PAGE_ROWS: [usize; 6] = [85, 83, 86, 87, 177, 58];
const INTERFACE_PAGE_KEYS: [usize; 6] = [18, usize::MAX, 19, 20, SHELVES_LAYOUT_KEY, usize::MAX];
/// Interface > Shelves' value.
/// (161 is PokeDaisy's Intro, 162 the Neo Geo BIOS, settings::NEOGEO_BIOS_KEY.)
const SHELVES_LAYOUT_KEY: usize = 160;
/// Interface > 3.0 Test Shelf's value (3.0.0-cartes1).
const TEST_SHELF_KEY: usize = 107;
const INTERFACE_WALLPAPER_ROW: usize = 1;
const INTERFACE_STATUS_ROW: usize = 5;
/// Settings > Interface > Wallpaper (Engine::wallpaper_rows): Shelf
/// (General or a shelf), its Wallpaper, Blur, then Halo with Default or
/// Tint (idle but for an image), Import, Delete.
const BLUR_ROW: usize = 153;
const HALO_ROW: usize = 154;
const DARKEN_ROW: usize = 155;
const BLUR_KEY: usize = 104;
const HALO_KEY: usize = 105;
const DARKEN_KEY: usize = 106;
/// "10%" among the setting values; 20 % to 100 % follow.
const PERCENT_VALUES: usize = 125;
/// A look's value face: OFF at 0 %, then 10 % to 100 %.
fn percent_value(percent: u8) -> usize {
    match percent.min(100) / 10 { 0 => 5, tens => PERCENT_VALUES + usize::from(tens) - 1 }
}

/// Library: ROM folder, RomM, BIOS, Save and State folders, Share Saves,
/// Refresh. Seven rows: the page scrolls, six shown (Siham 2026-11-09).
/// Library, six rows (Siham 2026-11-16, slot. 2.5): slot. Folder, ROM,
/// BIOS, Saves & States, RomM Server, Refresh Library.
const LIBRARY_ROWS: [usize; 6] = [149, 6, 24, 150, 135, 9];
const LIBRARY_SLOT_ROW: usize = 0;
const LIBRARY_SAVES_ROW: usize = 3;
const LIBRARY_ROMM_ROW: usize = 4;
/// Library > Saves & States: Save Folder, Save State Folder, Share Core
/// Saves, Sync Saves with RomM.
const SAVES_ROWS: [usize; 5] = [7, 8, 79, 152, 174];
const SAVES_SHARE_ROW: usize = 2;
const SAVES_ROMM_ROW: usize = 3;
const SAVES_STATES_ROW: usize = 4;

/// X/Y Buttons (38, key 7) moved to Systems > Controls (X / Y), where its
/// old value is the default until a system or game chooses.
const GAMEPLAY_ROWS: [usize; 5] = [27, 28, 37, 30, 39];
const GAMEPLAY_KEYS: [usize; 5] = [4, 5, 6, 8, 9];

/// Settings > Status Bar (Siham 2026-11-09).
const INTERFACE_ROWS: [usize; 5] = [127, 128, 129, 130, 116];
const INTERFACE_KEYS: [usize; 5] = [32, 33, 13, 15, 28];

#[derive(Clone, Copy)]
struct LiveGameOptions {
    ff_speed: u8,
    ff_sound: bool,
    rewind: bool,
    rewind_speed: u8,
    turbo: bool, // KONKR X/Y Buttons
    rumble: bool,
    rumble_level: u8,
    eject_save: bool,
}
impl LiveGameOptions {
    fn from_profile(global: &crate::settings::Settings, p: Option<EmulationProfile>) -> Self {
        let ff_speed=p.and_then(|v|v.ff_speed_override).unwrap_or(global.ff_speed);
        Self {
            ff_speed,
            ff_sound:p.and_then(|v|v.ff_sound_override).unwrap_or(global.ff_sound),
            rewind:p.and_then(|v|v.rewind_override).unwrap_or(global.rewind),
            rewind_speed:p.and_then(|v|v.rewind_speed_override).unwrap_or(global.rewind_speed),
            turbo:p.and_then(|v|v.turbo_override).unwrap_or(global.turbo),
            rumble:p.and_then(|v|v.rumble_override).unwrap_or(global.rumble),
            rumble_level:p.and_then(|v|v.rumble_level_override).unwrap_or(global.rumble_level).clamp(1,4),
            eject_save:p.and_then(|v|v.eject_save_override).unwrap_or(global.eject_save),
        }
    }
}

#[derive(Clone,Copy)]
struct ClearMaterial {
    key:(usize,usize),
    tex:[slot_gfx::TexId;3], // official back -> PCB -> plastic front
}

struct Engine {
    gpu: Compositor,
    size: (u32, u32),
    shelves: Vec<Shelf>,
    active: usize,
    inserted: bool,
    progress: f32,
    // Original slot. 300ms/6px refusal shake; shelf visuals only.
    refusal: Option<slot_ui::Refusal>,
    insert_started: Option<Instant>,
    screen_power: f32,
    exiting_screen: bool,
    eject_hold_remaining: f32,
    select_down_at: Option<Instant>,
    // HOME release must not close the Link screen opened by SELECT + HOME.
    select_home_consumed: bool,
    select_chorded: bool,
    select_forwarded: bool,
    select_tap_frames: u8,
    chord_releases: u16,
    born: Instant,
    last: Instant,
    library_version: u64,
    /// Built from a real library (an empty one too), not the placeholder
    /// of a cold start before the cached library came: its shelves can take
    /// new games in place, with no 5 s rebuild (RomM's first games).
    has_library: bool,
    texture_cache: VecDeque<(usize, usize, slot_gfx::TexId)>,
    /// Labels that arrived (the label search, RomM), each redrawn on its
    /// cart one per frame when the shelf isn't moving: no wait for a still
    /// carousel or a closed menu (RetroPup's video, 2026-11-16).
    relabel: VecDeque<String>,
    /// Carts whose new label is being drawn by the raster workers: their
    /// old face stays on the shelf until the new one replaces it (beta41).
    relabelled: Vec<(usize, usize)>,
    last_cart_navigation: Instant,
    cart_render: CartRender,
    clear_material: Option<ClearMaterial>,
    game: Option<GameSession>,
    pending_game: Option<GameSession>,
    link_identity: Option<crate::link_rom::GbaIdentity>,
    link_role: LinkRow,
    link_mode: LinkKind,
    // Optional mode chosen from original Slot's SELECT Mode key. Reset on
    // each new cartridge; keep it when returning from a cancelled link menu.
    link_mode_override: Option<LinkKind>,
    link_phase: LinkMenu,
    link_entry: LinkEntry,
    link_worked_at: u64,
    link_starter: Option<LinkStarter>,
    // Internet tab, typed room code and relayed attempt (link_internet.rs),
    // and the one heading face it draws instead of the stock ones.
    link_inet: crate::link_internet::InternetUi,
    link_inet_face: Option<(slot_gfx::TexId, u32, u32)>,
    link_inet_face_text: String,
    link_art: Option<LinkSprites>,
    link_marks: Marks,
    // A single account-scoped engine outlives individual games, allowing
    // queued offline awards to sync even after the cartridge is ejected.
    ra_service: Option<slot_achievements::Service>,
    ra_signing_out: bool,
    buttons: u16,
    // Settings > Controls: who plays on which port, and Port 2's buttons.
    controls: crate::controls::Controls,
    pad_devices: Vec<crate::controls::Device>,
    pad_held: crate::controls::Held,
    buttons2: u16,
    unseen2: u16,
    held_releases2: u16,
    controls_listen: Option<crate::controls::Listen>,
    controls_faces: std::collections::HashMap<String, (slot_gfx::TexId, u32, u32)>,
    controls_spare_faces: Vec<slot_gfx::TexId>,
    controls_hint_faces: std::collections::HashMap<(&'static str, &'static str), (slot_gfx::TexId, u32)>,
    /// Each legend's twin for Light and Original: the keycap as drawn, the
    /// word in the page's ink (menu_look::theme_hints).
    ink_hints: Vec<menu_look::InkHint>,
    /// The ink those twins are written in (None: the white of Dark).
    hint_ink: Option<[u8; 3]>,
    hint_swap_ab: bool,
    controls_view: controls_ui::ControlsView,
    /// Game menu > Cheats: the game's cheat files, and the one shown.
    cheat_sources: Vec<crate::cheats::Source>,
    cheat_source: usize,
    port2_remote_noticed: bool,
    // A key came from the handheld's own controls this session.
    builtin_seen: bool,
    // Bumped on every Controls change: the game's layouts are resolved
    // again (with the X/Y Buttons setting they were resolved with).
    controls_rev: u64,
    layouts_for: Option<(u64, bool)>,
    // Changed hotkeys: the Hotkey Button held, whether it made a hotkey or
    // reached the game, keys whose release belongs to a hotkey, and the
    // Hotkey Button held while a hotkey is being mapped.
    hk_mod_down: bool,
    hk_chorded: bool,
    hk_forwarded: bool,
    hk_releases: std::collections::BTreeMap<i32, crate::controls::Action>,
    hk_listen_mod: bool,
    // The game menu waits out a possible second slot. tap (Save States).
    menu_reveal_at: Option<Instant>,
    // Save States came from the game menu: B goes back there.
    states_from_menu: bool,
    // The second press of a double tap opened Save States: its release is spent.
    mode_double_press: bool,
    // The slot under the in-game menu: 0 down out of sight, 1 up.
    footer: f32,
    /// B on the in-game Screen page: the slot is sliding down before the
    /// opaque Settings page comes back.
    screen_closing: bool,
    /// The Screen page was opened by its hotkey: Back returns to the game.
    screen_direct: bool,
    /// The Hotkey Button is held on an in-game settings page.
    screen_hotkey_held: bool,
    frame_blend: crate::frame_blend::FrameBlend,
    /// Mega Drive Dither Blending (GDAPT), after Frame Blending.
    dither_blend: crate::dither_blend::DitherBlend,
    /// Sunlight Shimmer's light, and whether it follows the accelerometer.
    tilt: crate::tilt_light::TiltLight,
    tilt_on: bool,
    // Presses no core frame has read yet, and releases held back until one
    // has: a quick tap between two frames must still reach the game.
    unseen_presses: u16,
    held_releases: u16,
    ff_held: bool,
    ff_latched: bool,
    ff_last_release: Option<Instant>,
    rewind_held: bool,
    rewind_accum: f32,
    awaiting_game: bool,
    requested_uri: Option<String>,
    prepared_rom: Option<String>,
    game_accum: f64,
    ra_clock: crate::ra_clock_lab::Clock,
    ra_sync_steps: crate::ra_sync::VsyncSteps,
    ra_resampler: crate::ra_sync::Resampler,
    ra_resampled: Vec<i16>,
    ra_adjust: f64,
    /// RETROARCH mode's automatic Frame Delay; `frame_delay_used_ns` is the
    /// wait applied in this callback.
    frame_delay: crate::ra_sync::FrameDelay,
    frame_delay_used_ns: Option<i64>,
    perf_last: Instant,
    perf_rendered: u32,
    perf_emulated: u32,
    perf_catchup: u32,
    // Read-only cadence: real libretro steps per GL callback, not RA guesses.
    perf_step0: u32,
    perf_step1: u32,
    perf_step_multi: u32,
    external_pending: bool,
    overlay: ShelfOverlay,
    studio: Option<cart_studio::Studio>,
    cart_library: Option<cart_library::CartLibrary>,
    /// Library > RomM's page, kept between visits (the server's list).
    romm: Option<romm::RommPage>,
    /// L2 / R2 on the carousel: delete held, GBA Video tapped.
    triggers: carousel_delete::Triggers,
    touch: Option<touch::Touch>,
    shelves_grid: shelves_grid::GridState,
    /// The cart flying between the shelf and Cart Studio (cart_studio.rs).
    studio_flight: Option<cart_studio::StudioFlight>,
    /// A long press on a side cart: Cart Studio once it reaches the middle.
    studio_when_settled: bool,
    /// The GBA Video shelf was the one of the two used last.
    gba_video: bool,
    /// Textures Cart Studio drew its small carts in, free to draw again.
    studio_pool: Vec<slot_gfx::TexId>,
    settings: Settings,
    game_emulation: Option<EmulationProfile>,
    emulation_core_labels: [(slot_gfx::TexId, u32, u32); Core::ALL.len()],
    video_mode: VideoMode,
    /// PokeDaisy's view of the game played (SELECT + Up / Down).
    pokedaisy_view: crate::pokedaisy_view::View,
    /// INTRO for the game played: its start-up screens in EXPAND.
    pokedaisy_intro: bool,
    /// The coming game's screen, read while its cart is seated (3.2 perf):
    /// its screen size, where that came from and the frame it was chosen
    /// for, PokeDaisy's view and Intro. Taken when its screen goes on.
    screen_ready: Option<ScreenReady>,
    /// The whole-library disk fill (3.2 perf): the next system shelf and
    /// cart to draw into the disk cache, how many carts the pass covers
    /// (a new count starts it again) and how many it has sent.
    disk_fill: (usize, usize, usize, usize),
    /// PokeDaisy's HUD, drawn by slot. over the picture.
    pokedaisy_hud: pokedaisy_hud::PokedaisyHud,
    /// PokeDaisy's companion or quick menu is over the game: no shader, LCD
    /// Effect or Frame Blending on it.
    pokedaisy_menu: bool,
    wallpaper_texture: Option<slot_gfx::TexId>,
    wallpaper_personal: bool,
    /// How light the wallpaper is behind the status bar's top line.
    wallpaper_top_luma: Option<f32>,
    /// Shelf wallpaper: one texture per shelf, decoded off the GL thread.
    shelf_wallpaper_texs: Vec<Option<slot_gfx::TexId>>,
    shelf_wallpaper_ready: Vec<bool>,
    shelf_wallpaper_luma: Vec<Option<f32>>,
    /// A shelf's own wallpaper is an image (its Tint shades it).
    shelf_wallpaper_personal: Vec<bool>,
    shelf_wallpaper_rx: Option<std::sync::mpsc::Receiver<(usize, Vec<u8>, f32, bool)>>,
    /// General's wallpaper being made (reload_wallpaper): the one on screen
    /// stays until it is ready.
    wallpaper_rx: Option<std::sync::mpsc::Receiver<(Option<Vec<u8>>, bool)>>,
    wallpaper_name_face: Option<(slot_gfx::TexId, u32, u32)>,
    /// Interface > Wallpaper > Shelf: None for General, else the shelf
    /// (SHELF_KEYS) whose own wallpaper the page shows and changes.
    wallpaper_edit: Option<usize>,
    /// The shelf an Import Wallpaper writes Wallpapers/<shelf>.png for,
    /// until the reload after it makes that the shelf's wallpaper.
    wallpaper_import: Option<usize>,
    /// The edited shelf's image name, as Wallpaper's value.
    wallpaper_edit_face: Option<(slot_gfx::TexId, u32, u32)>,
    shelf_name_faces: Vec<(slot_gfx::TexId, u32, u32)>,
    /// Live rear/front v1.7.0 slot textures. Always draw in proper depth order.
    slot_faces: Option<(slot_gfx::TexId, slot_gfx::TexId)>,
    slot_dark_faces: (slot_gfx::TexId, slot_gfx::TexId),
    /// Every console's slot bar in the current appearance's bar theme.
    home_slot_faces: [(slot_gfx::TexId, slot_gfx::TexId); slot_ui::bitmaps::CONSOLE_SLOTS],
    /// The console bars in the three bar themes, made once like slot.'s own.
    home_slot_themes: [[(slot_gfx::TexId, slot_gfx::TexId); slot_ui::bitmaps::CONSOLE_SLOTS];
        slot_ui::bitmaps::BAR_THEMES],
    /// Whether the footer's clock and percent textures are inked black.
    footer_inked_dark: bool,
    /// What closes those slots' holes (none for the Famicom's open slot).
    home_closers: [Option<slot_ui::SlotCloser>; 4],
    slot_light_faces: (slot_gfx::TexId, slot_gfx::TexId),
    gb_bezel_tex: slot_gfx::TexId,
    gbc_bezel_tex: slot_gfx::TexId,
    /// The bezel fitted to the panel, and what it was fitted for (the
    /// picture rect it frames, rounded).
    fitted_bezel: Option<(slot_gfx::TexId, (crate::settings::Bezel, Platform, (u32, u32), [i32; 4]))>,
    /// The game picture sits at the top of the screen: a Watomsk logo is
    /// in the band under it.
    picture_top: bool,
    custom_wallpaper_present: bool,
    /// user.png's modified time, to tell a fresh import from any other reload.
    user_wallpaper_stamp: Option<std::time::SystemTime>,
    /// The Screenshot hotkey: read the next frame shown.
    screenshot_requested: bool,
    setting_values: Vec<(slot_gfx::TexId, u32, u32)>,
    // Settings > Systems default cores, by Platform order.
    default_cores: [Core; Platform::ALL.len()],
    /// Settings > Systems' Screen Size, per system.
    system_video_modes: [VideoMode; Platform::ALL.len()],
    // The live Palette row shows the palette's own name, one face per palette.
    palette_values: Vec<(slot_gfx::TexId, u32, u32)>,
    setting_carets: [(slot_gfx::TexId, u32, u32); 2],
    /// slot_ui::quick_caret_mid: where the carets' ink is centred.
    caret_mid: f32,
    settings_hints: [(slot_gfx::TexId, u32); 2],
    /// The in-game Screen page's L1 R1 Size, right of the cart.
    screen_size_hint: (slot_gfx::TexId, u32),
    /// The in-game Screen page's legends as keys alone (B, L1+R1, arrows),
    /// beside a slot too wide for their words.
    screen_key_hints: [(slot_gfx::TexId, u32); 3],
    about_texture: Option<slot_gfx::TexId>,
    update_status: u8,
    update_auto: bool,
    update_installed_face: Option<(slot_gfx::TexId,u32,u32)>,
    update_version_face: Option<(slot_gfx::TexId,u32,u32)>,
    menu_seen: u8,
    menu_opened: Instant,
    /// The long text scrolling now (page, row) and since when.
    /// (page, row, travel), since when, last drawn.
    marquee: std::cell::Cell<Option<((usize, usize, u32), Instant, Instant)>>,
    /// The selected game's title's own scroll, apart from the menus' (#119).
    title_marquee: std::cell::Cell<Option<((usize, usize, u32), Instant, Instant)>>,
    menu_cursor_y: f32,
    /// A long page's first row shown, and where its list is on the way there.
    menu_scroll_top: usize,
    menu_scroll_y: f32,
    /// The selected row's bar: left end, strip, right end.
    menu_pill: [(slot_gfx::TexId, u32, u32); 3],
    /// Where the selection bar was last drawn (x0, x1, y, h): the in-game
    /// Screen page writes what sits on it in the theme's ink.
    pill_band: std::cell::Cell<Option<[f32; 4]>>,
    /// The mark on a row that opens a page.
    menu_opens: (slot_gfx::TexId, u32, u32),
    /// About's yellow "Update available" capsule.
    update_badge: (slot_gfx::TexId, u32, u32),
    /// LIBRARY_VALUE_NAMES' faces.
    library_values: [(slot_gfx::TexId, u32, u32); 3],
    /// Library's slot. folder row while the first copy runs: COPYING n%.
    slot_copy_face: (slot_gfx::TexId, u32, u32),
    slot_copy_shown: i32,
    /// The task pill: the tasks as last read, and its face (beta39).
    task_pill_rev: u64,
    task_pill: Vec<(u8, f32, String)>,
    task_pill_face: Option<(slot_gfx::TexId, u32, u32)>,
    task_pill_key: String,
    /// How grey the carts are without their SD card (0 normal, 1 grey) and
    /// when it was last moved: a 0.3 s fade (beta38).
    missing_mix: f32,
    missing_at: Instant,
    /// Each page's path line, made when first shown.
    menu_paths: std::collections::HashMap<&'static str, (slot_gfx::TexId, u32, u32)>,
    /// The cart beside the main and game menus: its name, then its title
    /// and system lines.
    menu_side: Option<(String, [(slot_gfx::TexId, u32, u32); 2])>,
    menu_textures: Vec<(slot_gfx::TexId, u32, u32)>,
    /// Values' short forms where the whole word has no room (SHORT_VALUES).
    short_values: Vec<(slot_gfx::TexId, (slot_gfx::TexId, u32, u32))>,
    menu_hints: [(slot_gfx::TexId, u32); 2],
    /// About's third legend, X Logs: saves slot.'s logs to a file (#107).
    about_logs_hint: (slot_gfx::TexId, u32),
    first_run_hint: (slot_gfx::TexId, u32),
    onboarding_note: (slot_gfx::TexId, u32),
    shelf_no_games_hint: (slot_gfx::TexId, u32),
    shelf_status: u8,
    /// The scan under way: carts read, of how many (0 while listing).
    scan_progress: (u32, u32),
    /// Its count under the bar, and the text it was drawn from.
    scan_count: Option<(slot_gfx::TexId, u32, u32)>,
    scan_count_text: String,
    shelf_status_faces: [(slot_gfx::TexId, u32, u32); 10],
    shelf_status_start: (slot_gfx::TexId, u32),
    game_menu_hints: [(slot_gfx::TexId, u32); 3],
    // Same centered role, progress and failure faces as original slot.
    link_role_faces: [(slot_gfx::TexId, u32, u32); 2],
    link_step_faces: [(slot_gfx::TexId, u32, u32); 2],
    link_fail_faces: [(slot_gfx::TexId, u32, u32); 4],
    link_connected_face: (slot_gfx::TexId, u32, u32),
    link_hints: [[(slot_gfx::TexId, u32); 3]; 2],
    link_switch_hints: [[(slot_gfx::TexId, u32); 4]; 2],
    link_swap_icon: [(slot_gfx::TexId, u32); 2],
    /// Multiplayer's LAN / WAN slide switch: track, knob, house, globe.
    net_switch_faces: [(slot_gfx::TexId, u32, u32); 4],
    /// Where its knob is: 0 at the house (LAN), 1 at the globe (WAN).
    net_switch_t: f32,
    link_connected_hints: [[(slot_gfx::TexId, u32); 2]; 2],
    link_failed_hints: [[(slot_gfx::TexId, u32); 1]; 2],
    // R1 WAN, L1 LAN, Y Share, UP Digit, arrows Move, A Join.
    link_inet_hints: [[(slot_gfx::TexId, u32); 6]; 2],
    board_texture: Option<slot_gfx::TexId>,
    /// A Pokemon game's board: PokeDaisy's chip beside the two (3.1.1).
    picker_three: Option<slot_ui::ThreeChips>,
    /// The three-chip pak board's shorter chips: PokeDaisy, mGBA, Gambatte
    /// and blank, made the first time one opens.
    pak_three_textures: Option<[slot_gfx::TexId; 4]>,
    /// The front that lifts off a cart that leaves its contacts on the board
    /// (the Lynx), and whether the open picker's cart has one.
    picker_lid: (Option<slot_gfx::TexId>, bool),
    picker: Option<CorePicker>,
    /// A finger chose a core: A once the chip has hopped there (ms, born).
    picker_auto_seat: Option<u64>,
    socket_textures: [slot_gfx::TexId; Core::ALL.len()],
    chip_textures: [slot_gfx::TexId; Core::ALL.len()],
    chip_blank: slot_gfx::TexId,
    pak_chip_textures: [slot_gfx::TexId; Core::ALL.len()],
    pak_chip_blank: slot_gfx::TexId,
    /// The NES, Super Nintendo and Mega Drive chips with no core name.
    home_chip_blanks: [slot_gfx::TexId; 10],
    chip_shadow: slot_gfx::TexId,
    core_legend_faces: [(slot_gfx::TexId, u32); 3],
    /// The same, their words in Light's dark ink: over a light shelf
    /// (shelf_ink), as the shelf's name (Siham 2026-11-18).
    core_legend_dark: [(slot_gfx::TexId, u32); 3],
    a_down_at: Option<Instant>,
    home_launch_held: bool,
    fresh_launch: bool,
    mode_down_at: Option<Instant>,
    /// RG Rotate closed over its controls: the game pauses under its
    /// cartridge; with no game the shelf drifts, back where it was on opening.
    lid_covered: bool,
    lid_shelf_index: Option<(usize, usize)>,
    /// 0 open .. 1 closed: the close and open animations' progress.
    lid_t: f32,
    lid_title_started: Option<Instant>,
    /// LidStep ticks (6 s) since the last Random Cart.
    lid_ticks: u32,
    /// The clock and battery percentage drawn three times finer, so they
    /// grow without blurring: (text, dark ink, texture, logical width).
    lid_faces: [(String, bool, Option<slot_gfx::TexId>, u32); 2],
    /// The closed Rotate's big time, date and dial: (what they show, texture,
    /// pixel width, pixel height), redrawn only when that changes.
    /// 0 time, 1 date, 2 dial; 3-5 Game Info: NOW PLAYING, the title, the
    /// system and time.
    lid_clock_faces: [(String, Option<slot_gfx::TexId>, u32, u32); 6],
    /// The game's time played this session, in s: only while it runs (no
    /// menu, no cover, slot. on screen); from 0 at each start.
    play_time: f32,
    /// SELECT is physically down, whatever screen has the input.
    select_physical: bool,
    /// SELECT+X / SELECT+START presses whose releases are still to consume.
    combo_releases: u8,
    eject_sound_armed: bool,
    mode_last_tap: Option<Instant>,
    polaroids: Option<Polaroids>,
    photo_textures: Vec<slot_gfx::TexId>,
    polaroid_title_texture: Option<slot_gfx::TexId>,
    hud: Hud,
    clock_text: String,
    /// SystemStatus::date, as last read.
    date_text: String,
    last_system_status_rev: u64,
    clock_texture: Option<slot_gfx::TexId>,
    clock_face: Printed,
    battery_percent: Option<u8>,
    /// The percent printed in red, the battery low (#87).
    battery_low_shown: bool,
    /// The game's blinking low battery sign (#87).
    low_battery_badge: slot_gfx::TexId,
    battery_texture: Option<slot_gfx::TexId>,
    battery_face: Printed,
    battery: Option<Battery>,
    bolt_texture: slot_gfx::TexId,
    bolt_light_texture: slot_gfx::TexId,
    letter_texture: Option<slot_gfx::TexId>,
    letter_face: Printed,
    letter_shown_at: Option<Instant>,
    // Unmodified upstream system names: 200ms in, 1200ms hold, 800ms out.
    platform_faces: [Printed; Platform::ALL.len()],
    recents_platform_face: Printed,
    favorites_platform_face: Printed,
    video_platform_face: Printed,
    test_platform_face: Printed,
    focus_seen: Option<(usize, String)>,
    focus_dirty_since: Option<Instant>,
    // Upstream v1.7 menu key-repeat belongs to the UI clock, never libretro.
    menu_held: Option<(i32,u64,usize)>,
    menu_repeat_dispatch: bool,
    platform_shown_at: Option<Instant>,
    selected_title_name: String,
    selected_title_face: Printed,
    selected_title_texture: Option<slot_gfx::TexId>,
}

/// The upstream wallpaper.deck expects Config/ to exist on an SD card.
/// Android's fresh app-private filesDir does not necessarily contain it yet.
fn pick_wallpaper_android(root: &Path) -> Option<PathBuf> {
    if let Err(error) = std::fs::create_dir_all(root.join("Config")) {
        crate::log_warn(&format!("cannot initialize wallpaper deck: {error}"));
    }
    crate::wallpaper::pick(root, 0)
}

/// Reuse Slot v1.7.0's exact wallpaper selector. Only Android app-private
/// The Screenshot hotkey's PNG, for Android to file in Pictures/Screenshots.
pub(crate) static SCREENSHOT: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// The screen as shown, encoded off the GL thread; Android saves it.
fn capture_screenshot(gpu: &Compositor, size: (u32, u32)) {
    let (w, h) = size;
    if w == 0 || h == 0 { return; }
    let rgba = gpu.read_window(size);
    std::thread::spawn(move || {
        if let Some(png) = cart_studio::encode_png(&rgba, w, h) {
            *SCREENSHOT.lock().unwrap_or_else(|e| e.into_inner()) = Some(png);
            UI_ACTION.lock().unwrap_or_else(|e| e.into_inner()).push_back(40);
        }
    });
}

/// When Wallpapers/user.png was last written, if it exists.
fn user_wallpaper_stamp(root: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(root.join("Wallpapers/user.png")).and_then(|m| m.modified()).ok()
}

/// Wallpapers/ files are eligible: never access ROMs or external SAF paths.
/// Default is shelf `shelf`'s mosaic in `theme`; with no shelf yet (the
/// engine is being built) there is none to make. `look`'s Blur softens
/// every wallpaper and its Halo lights Default's lines.
fn wallpaper_pixels(root: Option<&Path>, selection: &crate::settings::WallpaperPick,
    look: crate::settings::WallpaperLook, shelf: Option<usize>, theme: slot_ui::MosaicTheme)
    -> (Option<Vec<u8>>, bool)
{
    use crate::settings::WallpaperPick;
    let blurred = |mut rgba: Vec<u8>| {
        slot_ui::blur_wallpaper(&mut rgba, slot_ui::native_w(), slot_ui::native_h(), look.blur);
        rgba
    };
    let mosaic = || shelf.and_then(|shelf| slot_ui::mosaic_native(shelf, theme, look.mosaic()));
    let wood = || slot_ui::default_background_native().map(|v| blurred(v.to_vec()));
    let default = || if shelf.is_some() { mosaic() } else { wood() };
    let personal = match (selection, root) {
        (WallpaperPick::None, _) => return (None, false),
        // Default Blur and Halo are read as Default with their look.
        (WallpaperPick::Default | WallpaperPick::Blur | WallpaperPick::Halo, _) =>
            return (mosaic(), false),
        (WallpaperPick::Slot | WallpaperPick::SlotBlur, _) => return (wood(), false),
        // Shelves without their own file show the bundled default.
        (WallpaperPick::Shelf, _) => return (default(), false),
        (WallpaperPick::File(name), Some(root)) => {
            if crate::wallpaper::list(root).contains(name) {
                Some(root.join("Wallpapers").join(name))
            } else { None }
        }
        (WallpaperPick::Random, Some(root)) => pick_wallpaper_android(root),
        _ => None,
    };
    if let Some(image) = personal.and_then(|p|slot_ui::wallpaper_face_native(&p)) {
        (Some(blurred(image)), true)
    } else {
        // Official Slot falls back to the bundled default for an empty random
        // deck or a deleted file. Never silently mutate the saved selection.
        (default(), false)
    }
}

/// A shelf's own wallpaper as its worker makes it: None when its image is
/// gone (the shelf shows General's).
fn shelf_wallpaper_pixels(root: Option<&Path>, pick: &crate::settings::WallpaperPick,
    look: crate::settings::WallpaperLook, shelf: usize, theme: slot_ui::MosaicTheme)
    -> (Option<Vec<u8>>, bool)
{
    use crate::settings::WallpaperPick;
    match pick {
        WallpaperPick::File(name) => (root
            .map(|root| root.join("Wallpapers").join(name))
            .filter(|path| path.is_file())
            .and_then(|path| slot_ui::wallpaper_face_native(&path))
            .map(|mut rgba| {
                slot_ui::blur_wallpaper(&mut rgba, slot_ui::native_w(), slot_ui::native_h(), look.blur);
                rgba
            }), true),
        WallpaperPick::Default | WallpaperPick::Blur | WallpaperPick::Halo
        | WallpaperPick::Slot | WallpaperPick::SlotBlur =>
            wallpaper_pixels(root, pick, look, Some(shelf), theme),
        _ => (None, false),
    }
}

impl Engine {
    fn new(library_version: u64, library: Option<&[RomEntry]>) -> Result<Self, String> {
        let mut steps = crate::frame_watch::Steps::new();
        let size = panel_size();
        // The canvas stays 480 high and takes the panel's shape (720 wide on
        // the KONKR's 3:2). Text, glyphs and vector art are drawn for the
        // panel, not stretched. Set both before the first face is built.
        slot_gfx::set_canvas_for_panel(size);
        slot_ui::density::set_scale(size.1 as f32 / OUT_H as f32);
        // The slot bars and bezels decode on worker threads while GL starts.
        let start_pixels = start_pixels();
        let device = crate::settings::Device {
            konkr: is_konkr(),
            retroid_pocket_classic: RPC_DEFAULT_RESPONSIVE.load(Ordering::Acquire),
            three_two: crate::settings::Device::is_three_two(size),
        };
        let mut settings = PATHS.lock().unwrap_or_else(|e| e.into_inner())
            .as_ref().map(|(root, _)| Settings::load_for_device(root, device))
            .unwrap_or_else(|| Settings::first_install_defaults_for_device(device));
        settings.own_bezels_by_room(size);
        settings.bezels_3_2_by_room(size);
        crate::settings::CoreExtras::store(&settings);
        // General's wallpaper too (a blur takes a moment): Default's mosaic
        // follows the shelf on screen, not known yet, and is made once the
        // engine is (with_shelf_wallpapers).
        let visuals_root = PATHS.lock().unwrap_or_else(|e| e.into_inner())
            .as_ref().map(|(root, _)| root.clone());
        let general_wallpaper = {
            let (root, pick, look) = (visuals_root.clone(), settings.wallpaper.clone(), settings.wallpaper_look);
            let theme = settings.appearance.mosaic_theme();
            let make = move || wallpaper_pixels(root.as_deref(), &pick, look, None, theme);
            std::thread::Builder::new().name("slot-wallpaper-start".into()).spawn(make.clone())
                .map_err(|_| make)
        };
        let mut gpu = Compositor::new(&AndroidSurface { size }).map_err(|e| e.to_string())?;
        steps.mark("gl");
        let mut controls = PATHS.lock().unwrap_or_else(|e| e.into_inner())
            .as_ref().map(|(root, _)| crate::controls::Controls::load(root))
            .unwrap_or_default();
        // Until Android lists the controllers: L3/R3 only if seen before.
        controls.hotkeys.sticks = controls.builtin_sticks;
        // Upstream Slot delegates colour correction to libretro core options.
        // Disable the dev13 approximate GPU grade to avoid double correction.
        gpu.enable_context_recovery();
        // Bind the palette to this renderer's thread before the first draw.
        // A matching global fallback must not leave the local palette unset.
        menu_style::set_palette(None);
        menu_style::set_oled(settings.appearance == crate::settings::Appearance::Oled);
        gpu.set_colour_correction(false);
        FF_AUDIO_ENABLED.store(settings.ff_sound, Ordering::Release);
        if let Some(root) = visuals_root.as_deref() {
            slot_ui::set_theme(slot_store::Theme::read(root));
        }
        let custom_wallpaper_present = visuals_root.as_deref()
            .is_some_and(|root| root.join("Wallpapers/user.png").is_file());
        let user_wallpaper_stamp = visuals_root.as_deref().and_then(user_wallpaper_stamp);
        let (wallpaper_pixels, wallpaper_personal) = match general_wallpaper {
            Ok(worker) => worker.join().unwrap_or((None, false)),
            Err(make) => make(),
        };
        steps.mark("settings and wallpaper read");
        let wallpaper_top_luma = wallpaper_pixels.as_deref()
            .map(|rgba| slot_ui::top_line_luma(rgba, slot_ui::native_w()));
        let wallpaper_texture = wallpaper_pixels.map(|rgba|
            gpu.create_texture(slot_ui::native_w(), slot_ui::native_h(), &rgba));
        // Preserve existing Config/theme.txt custom colours as an opt-out.
        steps.mark("wallpaper upload");
        let custom_colours = visuals_root.as_deref()
            .is_some_and(|root| root.join("Config/theme.txt").exists());
        let mut pixels = start_pixels.join().unwrap_or_else(|_| {
            crate::log_warn("start pictures decoded on the GL thread (a worker failed)");
            StartJob::all().iter().map(StartJob::make).collect()
        }).into_iter();
        let mut upload = |gpu: &mut Compositor| match pixels.next().expect("every start picture decoded") {
            StartPicture::Texture(rgba, w, h) => gpu.create_texture(w, h, &rgba),
            StartPicture::Face(face) => gpu.create_face(&face),
        };
        // Both official themes are cached once. Toggling doesn't allocate
        // textures on the GL thread or shift the original Slot animation.
        let slot_dark_faces = (upload(&mut gpu), upload(&mut gpu));
        let slot_light_faces = (upload(&mut gpu), upload(&mut gpu));
        let footer_inked_dark_at_start = settings.appearance == crate::settings::Appearance::Light;
        let home_slot_themes = [0usize, 1, 2].map(|_| [(); slot_ui::bitmaps::CONSOLE_SLOTS]
            .map(|_| (upload(&mut gpu), upload(&mut gpu))));
        steps.mark("slot bars");
        let home_slot_faces = home_slot_themes[settings.appearance.bar_theme()];
        // The home consoles' bars are open recesses like the GB/GBA bar
        // (Siham 2026-11-02): no flap or doors close them any more.
        let home_closers: [Option<slot_ui::SlotCloser>; 4] = [None; 4];
        let slot_faces = if custom_colours { None }
            else if settings.appearance == crate::settings::Appearance::Light {
                Some(slot_light_faces)
            } else { Some(slot_dark_faces) };
        // Official 720x480 GB/GBC screen bezels, sampled once for KONKR.
        let gb_bezel_tex = upload(&mut gpu);
        let gbc_bezel_tex = upload(&mut gpu);

        steps.mark("bezels");
        let mut shelves = Vec::new();

        // First-run empty shelf: never synthesize fake demo cartridges.
        let (systems, video_carts) = library_carts(library, visuals_root.as_deref());
        for carts in systems { shelves.push(Shelf::new(carts)); }
        // The rack following all supported systems mirrors ten recent ROMs.
        // Do not modify the system shelves or their ROM roots.
        let recent_ids=visuals_root.as_deref().map(crate::recents::load)
            .unwrap_or_default();
        let system_carts: Vec<&[slot_store::Cart]> = shelves.iter()
            .take(Platform::ALL.len()).map(|shelf| shelf.carts.as_slice())
            .chain(std::iter::once(video_carts.as_slice())).collect();
        let recent_carts=crate::recents::carts(&recent_ids, &system_carts);
        shelves.push(Shelf::new(if settings.show_recents {
            recent_carts
        } else { Vec::new() }));
        // The Favorites rack contains cartridges from every supported system.
        // URI identity keeps identical ROM filenames in distinct folders separate.
        let favorite_ids=visuals_root.as_deref().map(crate::favorites::load)
            .unwrap_or_default();
        let system_carts: Vec<&[slot_store::Cart]> = shelves.iter()
            .take(Platform::ALL.len()).map(|shelf| shelf.carts.as_slice())
            .chain(std::iter::once(video_carts.as_slice())).collect();
        let favorite_carts=crate::favorites::carts(&favorite_ids, &system_carts);
        shelves.push(Shelf::new(favorite_carts));
        debug_assert_eq!(shelves.len(), VIDEO_RACK);
        shelves.push(Shelf::new(video_carts));
        // Library > RomM's page has its rack but no shelf: an empty one
        // keeps the 3.0 test shelf at its own rack (3.0.0-cartes1).
        shelves.push(Shelf::new(Vec::new()));
        debug_assert_eq!(shelves.len(), TEST_RACK);
        shelves.push(Shelf::new(if settings.test_shelf { test_carts() } else { Vec::new() }));

        // Original Slot names, not abbreviations: Game Boy Advance,
        // Game Boy, Game Boy Color. Keep the existing 3x antialiased 16px
        // font, centered placement and official 200/1200/800ms fade.
        // Cache textures once rather than rebuilding on every L1/R1.
        steps.mark("library");
        let platform_faces = Platform::ALL.map(|platform| {
            let face = platform_name_face(platform.name());
            let tex = gpu.create_face(&face);
            Printed::new(tex, face.w)
        });

        let recent_label = platform_name_face("RECENTS");
        let recents_platform_face=Printed::new(
            gpu.create_face(&recent_label),
            recent_label.w);
        let favorites_label=platform_name_face("FAVORITES");
        let favorites_platform_face=Printed::new(
            gpu.create_face(&favorites_label),
            favorites_label.w);
        let video_label=platform_name_face("Game Boy Advance Video");
        let video_platform_face=Printed::new(gpu.create_face(&video_label), video_label.w);
        let test_label=platform_name_face("3.0 TEST");
        let test_platform_face=Printed::new(gpu.create_face(&test_label), test_label.w);

        steps.mark("platform names");
        let gba = upload(&mut gpu);
        let gb = upload(&mut gpu);
        let gbc = upload(&mut gpu);
        let home_shadows = slot_ui::HomeMould::ALL.map(|m| (m, upload(&mut gpu)));
        // The exact original Slot 9x9 pixel star, bright yellow with dark rim.
        steps.mark("shadows");
        let star=slot_ui::star_face(2, [0xf6,0xc3,0x2c]);
        let star_tex=gpu.create_face(&star);
        for shelf in &mut shelves {
            shelf.set_shadow(gba);
            shelf.set_gb_shadow(GbShell::Notched, gb);
            shelf.set_gb_shadow(GbShell::Rounded, gbc);
            for (m, tex) in home_shadows { shelf.set_home_shadow(m, tex); }
            shelf.set_star(star_tex,star.w,star.h);
            shelf.mark_starred(|cart|crate::favorites::holds(&favorite_ids,
                cart.rom.to_string_lossy().as_ref()));
        }

        steps.mark("shelf faces");
        let art = slot_ui::link_marks();
        let mut sprite = |face: slot_ui::CartFace| Sprite {
            tex: gpu.create_face(&face), w: face.w, h: face.h,
        };
        let link_marks = Marks {
            arcs_right: art.arcs_right.map(&mut sprite),
            arcs_left: art.arcs_left.map(&mut sprite),
            clicks: sprite(art.clicks),
            arrow_left: sprite(art.arrow_left),
            arrow_right: sprite(art.arrow_right),
        };
        let menu_textures = MENU_TEXT.iter().map(|label| {
            let face = quick_value_face(narrow_label(label), true);
            let tex = gpu.create_face(&face);
            (tex, face.w, face.h)
        }).collect();
        let menu_hints = [("A", "Select"), ("B", "Back")].map(|(key, label)| {
            let face = hint_face(key, label);
            let tex = gpu.create_face(&face);
            (tex, face.w)
        });
        let about_logs_hint = {
            let face = hint_face("X", "Logs");
            (gpu.create_face(&face), face.w)
        };
        // Authentic Slot raster/keycap for a clean library. Render it in the
        // GL scene rather than above it in an Android TextView.
        let first_run_hint = {
            let face = onboarding_instruction_face(
                "PRESS", "START", ", THEN SELECT LIBRARY TO ADD GAMES");
            (gpu.create_face(&face), face.w)
        };
        let shelf_no_games_hint = {
            let face = onboarding_instruction_face(
                "PRESS", "START", "TO SELECT YOUR ROM FOLDER");
            (gpu.create_face(&face), face.w)
        };
        let onboarding_note = {
            let face = onboarding_note_face("(ZIP ROMS SUPPORTED)");
            (gpu.create_face(&face), face.w)
        };
        let shelf_status_faces = SHELF_STATUS_TEXT.map(|label| {
            let face = quick_value_face(label, true);
            (gpu.create_face(&face), face.w, face.h)
        });
        let shelf_status_start = {
            let face = hint_face("START", "Open Library");
            (gpu.create_face(&face), face.w)
        };
        // Original Slot keycap typography also describes the actual in-game
        // shortcuts, so users need not guess physical controller buttons.
        let game_menu_hints = [
            ("B", "Back"), ("SEL+R1", "Save"), ("SEL+L1", "Load"),
        ].map(|(key, label)| {
            let face = hint_face(key, label);
            let tex = gpu.create_face(&face);
            (tex, face.w)
        });
        let make_link_text = |gpu: &mut Compositor, label: &str| {
            // Identical font, 30px scale, ink, centering and side padding
            // to original slot/src/frontend.rs menu_faces(), not our
            // Android Settings quick_value_face fallback.
            let face = menu_face(label);
            (gpu.create_face(&face), face.w, face.h)
        };
        let link_role_faces = LinkRow::ALL
            .map(|role| make_link_text(&mut gpu, role.text()));
        let link_step_faces = LinkStep::ALL
            .map(|step| make_link_text(&mut gpu, step.line()));
        let link_fail_faces = LinkFail::SHOWN
            .map(|failure| make_link_text(&mut gpu, failure.line()));
        let link_connected_face = make_link_text(&mut gpu, "Linked");
        let mut link_hint = |key, label, ink| {
            let face = if key == "SWAP" { slot_ui::arrows_hint_face_ink(label, ink) }
                else { slot_ui::hint_face_ink(key, label, ink) };
            (gpu.create_face(&face), face.w)
        };
        let inks = [[246,244,239], [23,23,25]];
        let link_switch_hints = inks.map(|ink| [("B","Cancel"), ("SELECT","Mode"),
            ("SWAP","Swap"), ("A","Link")].map(|(k,v)| link_hint(k,v,ink)));
        let link_hints = link_switch_hints.map(|[cancel, _, swap, link]| [cancel, swap, link]);
        let link_connected_hints = inks.map(|ink| [("B","Back"), ("A","End Link")]
            .map(|(k,v)| link_hint(k,v,ink)));
        let link_failed_hints = inks.map(|ink| [link_hint("A","OK",ink)]);
        let link_inet_hints = inks.map(|ink| [("R1","WAN"), ("L1","LAN"),
            ("Y","Share"), ("UP","Digit"), ("SWAP","Move"), ("A","Join")]
            .map(|(k,v)| link_hint(k,v,ink)));
        let net_switch_faces = [slot_ui::NetPart::Track, slot_ui::NetPart::Knob,
            slot_ui::NetPart::House, slot_ui::NetPart::Globe].map(|part| {
            let face = slot_ui::net_switch_face(part);
            (gpu.create_face(&face), face.w, face.h)
        });
        // The arrows alone, for a legend too wide for a narrow screen.
        let link_swap_icon = inks.map(|ink| {
            let face = slot_ui::arrows_hint_face_ink("", ink);
            (gpu.create_face(&face), face.w)
        });
        steps.mark("link faces");
        let setting_values = SETTING_VALUE_NAMES
            .into_iter().map(|value| {
                let face = quick_value_face(value, true);
                let tex = gpu.create_face(&face);
                (tex, face.w, face.h)
            }).collect::<Vec<_>>();
        // Library's own values (2.5): the slot. folder's place, RomM's sync.
        let library_values = LIBRARY_VALUE_NAMES.map(|value| {
            let face = quick_value_face(value, true);
            (gpu.create_face(&face), face.w, face.h)
        });
        let short_values = SHORT_VALUES.iter().filter_map(|&(whole, short)| {
            let full = SETTING_VALUE_NAMES.iter().position(|v| *v == whole)
                .and_then(|at| setting_values.get(at))
                .or_else(|| LIBRARY_VALUE_NAMES.iter().position(|v| *v == whole).map(|at| &library_values[at]))?;
            let face = quick_value_face(short, true);
            Some((full.0, (gpu.create_face(&face), face.w, face.h)))
        }).collect::<Vec<_>>();
        let slot_copy_face = {
            let face = quick_value_face("COPYING...", true);
            (gpu.create_face(&face), face.w, face.h)
        };
        let palette_values = slot_store::GbPalette::all().map(|palette| {
            let face = quick_value_face(palette.label(), true);
            (gpu.create_face(&face), face.w, face.h)
        }).collect();
        let wallpaper_name_face = match &settings.wallpaper {
            crate::settings::WallpaperPick::File(_) => {
                let name = settings.wallpaper.label();
                let short: String = name.chars().take(18).collect();
                let face = quick_value_face(&short, true);
                Some((gpu.create_face(&face), face.w, face.h))
            }
            _ => None,
        };
        let shelf_name_faces = crate::wallpaper::SHELF_KEYS.iter().map(|key| {
            let face = quick_value_face(key, true);
            (gpu.create_face(&face), face.w, face.h)
        }).collect();
        let emulation_core_labels = Core::ALL.map(|core| {
            let face = quick_value_face(core.text(), true);
            (gpu.create_face(&face), face.w, face.h)
        });
        let menu_pill = menu_style::pill_faces().map(|face| (gpu.create_face(&face), face.w, face.h));
        let menu_opens = {
            let face = menu_style::menu_opens_face();
            (gpu.create_face(&face), face.w, face.h)
        };
        let update_badge = {
            let face = menu_style::update_badge_face();
            (gpu.create_face(&face), face.w, face.h)
        };
        let setting_carets = [false, true].map(|right| {
            let face = quick_caret_face(right);
            let tex = gpu.create_face(&face);
            (tex, face.w, face.h)
        });
        let settings_hints = [
            hint_face("B", "Back"), arrows_hint_face("Change")
        ].map(|face| {
            let tex = gpu.create_face(&face);
            (tex, face.w)
        });
        let screen_size_hint = {
            let face = hint_face("L1+R1", "Size");
            (gpu.create_face(&face), face.w)
        };
        let screen_key_hints = [hint_face("B", ""), hint_face("L1+R1", ""), arrows_hint_face("")]
            .map(|face| (gpu.create_face(&face), face.w));
        // Rasterize the detailed barcode About label only when opened.
        // Creating SVG / fonts during every cold boot needlessly delays shelf.
        steps.mark("menu faces");
        let about_texture = None;
        let chip = chip_face(Some(Core::Mgba));
        let chip_texture = gpu.create_face(&chip);
        let socket_textures = Core::ALL.map(|core| {
            let face = socket_face(core);
            gpu.create_face(&face)
        });
        let chip_textures = Core::ALL.map(|core| {
            if core == Core::Mgba { return chip_texture; }
            let face = chip_face(Some(core));
            gpu.create_face(&face)
        });
        let blank = chip_face(None);
        let chip_blank = gpu.create_face(&blank);
        // GB/GBC boards hop their own big pak chip between two footprints.
        let pak_chip_textures = Core::ALL.map(|core| gpu.create_face(&pak_chip_face(Some(core))));
        let pak_chip_blank = gpu.create_face(&pak_chip_face(None));
        let home_chip_blanks = [Platform::Nes, Platform::Snes, Platform::Md, Platform::Sms, Platform::Gg,
            Platform::Ngp, Platform::Ws, Platform::Lynx, Platform::Pokemini, Platform::Pico8]
            .map(|p| gpu.create_face(&slot_ui::home_chip_face(p, None)));
        let shadow = chip_shadow_face();
        let chip_shadow = gpu.create_face(&shadow);
        // The actual Slot legend is a set of three compact keycaps, not
        // oversized menu labels. These are the original Slot UI generators.
        let core_legend_faces = [
            hint_face("B", "Cancel"),
            arrows_hint_face("Swap"),
            hint_face("A", "Choose"),
        ].map(|face| {
            let texture = gpu.create_face(&face);
            (texture, face.w)
        });
        let low_battery_badge = gpu.create_face(&slot_ui::low_battery_badge());
        let dark_ink = menu_style::MenuPalette::LIGHT.ink.unwrap_or([0; 3]);
        let core_legend_dark = [
            slot_ui::hint_face_ink("B", "Cancel", dark_ink),
            slot_ui::arrows_hint_face_ink("Swap", dark_ink),
            slot_ui::hint_face_ink("A", "Choose", dark_ink),
        ].map(|face| {
            let texture = gpu.create_face(&face);
            (texture, face.w)
        });

        // Use the original Slot toast faces and HUD, not Android text banners.
        steps.mark("chips");
        let mut hud = Hud::new();
        let toasts = Toast::all().into_iter().map(|toast| {
            let face = toast_face(toast);
            gpu.create_face(&face)
        }).collect();
        hud.set_toasts(toasts);
        // The upstream glyph subset is already packaged. Enable its exact
        // fast-forward badge and rewind progress icon in the Android HUD.
        let icons = Icon::ALL.into_iter().map(|icon| {
            let face = icon_face(icon, slot_ui::HUD_ICON_PX, HUD_INK);
            gpu.create_face(&face)
        }).collect();
        hud.set_icons(icons);
        // Exact upstream slot. Link badge glyphs, colour and 24px halo.
        // The HUD already places these in the top-right of the game image,
        // but Android previously never populated its link texture atlas.
        let link_faces = LinkBadge::FACES.into_iter().map(|badge| {
            let glyph = badge.badge().expect("only visible LinkBadge variants");
            let colour = badge.colour().expect("visible LinkBadge has a colour");
            let face = badge_face(glyph, slot_ui::HUD_ICON_PX, colour);
            gpu.create_face(&face)
        }).collect();
        hud.set_link_faces(link_faces);
        let bolt = icon_face(Icon::Charging, BOLT_PX, slot_ui::footer_ink(false));
        let bolt_texture = gpu.create_face(&bolt);
        let bolt = icon_face(Icon::Charging, BOLT_PX, slot_ui::footer_ink(true));
        let bolt_light_texture = gpu.create_face(&bolt);

        // On a GBA-only library do not begin on an empty GB or GBC shelf.
        steps.mark("hud");
        let saved_focus=visuals_root.as_deref().and_then(crate::selection::load);
        let active=saved_focus.as_ref().and_then(|focus|restore_saved_focus(&mut shelves,focus))
            .unwrap_or_else(||shelves.iter().position(|s|!s.carts.is_empty()).unwrap_or(0));
        let active = gba_before_video(&shelves, active);
        let now = Instant::now();
        let has_multiple_systems = shelves.iter().filter(|s| !s.carts.is_empty()).count() > 1;
        steps.mark("rest");
        crate::game::log_launch_timing(&steps.line());
        // All themes stay preloaded. Pack only the static CPU recovery copies
        // after startup uploads, not on the GL thread or for live cart faces.
        if let Some(tex) = wallpaper_texture { gpu.pack_static_recovery(tex); }
        for (back, front) in std::iter::once(slot_dark_faces)
            .chain(std::iter::once(slot_light_faces))
            .chain(home_slot_themes.iter().flatten().copied()) {
            gpu.pack_static_recovery(back);
            gpu.pack_static_recovery(front);
        }
        Ok(Self {
            gpu,
            size,
            shelves,
            active,
            inserted: false,
            progress: 0.0,
            refusal: None,
            insert_started: None,
            screen_power: 0.0,
            exiting_screen: false,
            eject_hold_remaining: 0.0,
            select_down_at: None,
            select_home_consumed: false,
            select_chorded: false,
            select_forwarded: false,
            select_tap_frames: 0,
            chord_releases: 0,
            born: now,
            last: now,
            library_version,
            has_library: library.is_some(),
            texture_cache: VecDeque::new(),
            relabel: VecDeque::new(),
            relabelled: Vec::new(),
            last_cart_navigation: now,
            cart_render: CartRender::start(visuals_root.as_deref())?,
            clear_material: None,
            game: None,
            pending_game: None,
            link_identity: None,
            link_role: LinkRow::Host,
            link_mode: LinkKind::Cable,
            link_mode_override: None,
            link_phase: LinkMenu::Pick(LinkRow::Host),
            link_entry: LinkEntry::GameMenu,
            link_worked_at: 0,
            link_starter: None,
            link_inet: Default::default(),
            link_inet_face: None,
            link_inet_face_text: String::new(),
            link_art: None,
            link_marks,
            ra_service: None,
            ra_signing_out: false,
            buttons: 0,
            controls,
            pad_devices: Vec::new(),
            pad_held: Default::default(),
            buttons2: 0,
            unseen2: 0,
            held_releases2: 0,
            controls_listen: None,
            controls_faces: Default::default(),
            controls_spare_faces: Vec::new(),
            controls_hint_faces: Default::default(),
            ink_hints: Vec::new(),
            hint_ink: None,
            hint_swap_ab: false,
            controls_view: Default::default(),
            cheat_sources: Vec::new(),
            cheat_source: 0,
            port2_remote_noticed: false,
            builtin_seen: false,
            controls_rev: 0,
            layouts_for: None,
            hk_mod_down: false,
            hk_chorded: false,
            hk_forwarded: false,
            hk_releases: Default::default(),
            hk_listen_mod: false,
            menu_reveal_at: None,
            states_from_menu: false,
            mode_double_press: false,
            footer: 0.0,
            screen_closing: false,
            screen_direct: false,
            screen_hotkey_held: false,
            frame_blend: Default::default(),
            dither_blend: Default::default(),
            tilt: Default::default(),
            tilt_on: false,
            unseen_presses: 0,
            held_releases: 0,
            ff_held: false,
            ff_latched: false,
            ff_last_release: None,
            rewind_held: false,
            rewind_accum: 0.0,
            awaiting_game: false,
            requested_uri: None,
            prepared_rom: None,
            game_accum: 0.0,
            ra_clock: crate::ra_clock_lab::Clock::default(),
            ra_sync_steps: crate::ra_sync::VsyncSteps::default(),
            ra_resampler: crate::ra_sync::Resampler::default(),
            ra_resampled: Vec::with_capacity(4096),
            ra_adjust: 1.0,
            frame_delay: crate::ra_sync::FrameDelay::default(),
            frame_delay_used_ns: None,
            perf_last: now,
            perf_rendered: 0,
            perf_emulated: 0,
            perf_catchup: 0,
            perf_step0: 0,
            perf_step1: 0,
            perf_step_multi: 0,
            external_pending: false,
            overlay: ShelfOverlay::None,
            studio: None,
            cart_library: None,
            romm: None,
            triggers: Default::default(),
            touch: None,
            shelves_grid: Default::default(),
            studio_flight: None,
            studio_when_settled: false,
            gba_video: false,
            studio_pool: Vec::new(),
            settings,
            game_emulation: None,
            emulation_core_labels,
            video_mode: VideoMode::Actual,
            pokedaisy_view: crate::pokedaisy_view::View::Expand,
            pokedaisy_intro: true,
            screen_ready: None,
            disk_fill: (0, 0, 0, 0),
            pokedaisy_hud: Default::default(),
            pokedaisy_menu: false,
            wallpaper_texture,
            wallpaper_personal,
            wallpaper_top_luma,
            shelf_wallpaper_texs: Vec::new(),
            shelf_wallpaper_ready: Vec::new(),
            shelf_wallpaper_luma: Vec::new(),
            shelf_wallpaper_personal: Vec::new(),
            shelf_wallpaper_rx: None,
            wallpaper_rx: None,
            wallpaper_name_face,
            wallpaper_edit: None,
            wallpaper_import: None,
            wallpaper_edit_face: None,
            shelf_name_faces,
            slot_faces,
            slot_dark_faces,
            home_slot_faces,
            home_slot_themes,
            footer_inked_dark: footer_inked_dark_at_start,
            home_closers,
            slot_light_faces,
            gb_bezel_tex,
            gbc_bezel_tex,
            fitted_bezel: None,
            picture_top: false,
            custom_wallpaper_present,
            user_wallpaper_stamp,
            screenshot_requested: false,
            setting_values,
            default_cores: Platform::ALL.map(Core::default_for),
            system_video_modes: Platform::ALL.map(|platform| video_mode::default_mode(platform, panel_size())),
            palette_values,
            setting_carets,
            caret_mid: slot_ui::quick_caret_mid(),
            settings_hints,
            screen_size_hint,
            screen_key_hints,
            about_texture,
            update_status: 0,
            update_auto: true,
            update_installed_face: None,
            update_version_face: None,
            menu_seen: 0,
            menu_opened: now,
            marquee: std::cell::Cell::new(None),
            title_marquee: std::cell::Cell::new(None),
            menu_cursor_y: 0.0,
            menu_scroll_top: 0,
            menu_scroll_y: 0.0,
            menu_pill,
            pill_band: std::cell::Cell::new(None),
            menu_opens,
            update_badge,
            library_values,
            slot_copy_face,
            slot_copy_shown: -1,
            task_pill_rev: 0,
            task_pill: Vec::new(),
            task_pill_face: None,
            task_pill_key: String::new(),
            missing_mix: if ROMS_MISSING.load(Ordering::Acquire) { 1.0 } else { 0.0 },
            missing_at: Instant::now(),
            menu_paths: Default::default(),
            menu_side: None,
            menu_textures,
            short_values,
            menu_hints,
            about_logs_hint,
            first_run_hint,
            onboarding_note,
            shelf_no_games_hint,
            shelf_status: 0,
            scan_progress: (0, 0),
            scan_count: None,
            scan_count_text: String::new(),
            shelf_status_faces,
            shelf_status_start,
            game_menu_hints,
            link_role_faces,
            link_step_faces,
            link_fail_faces,
            link_connected_face,
            link_hints,
            link_switch_hints,
            link_swap_icon,
            net_switch_faces,
            net_switch_t: 0.0,
            link_connected_hints,
            link_failed_hints,
            link_inet_hints,
            board_texture: None,
            picker_three: None,
            pak_three_textures: None,
            picker_lid: (None, false),
            picker: None,
            picker_auto_seat: None,
            socket_textures,
            chip_textures,
            chip_blank,
            pak_chip_textures,
            pak_chip_blank,
            home_chip_blanks,
            chip_shadow,
            core_legend_faces,
            core_legend_dark,
            a_down_at: None,
            home_launch_held: false,
            fresh_launch: false,
            mode_down_at: None,
            lid_covered: false,
            lid_shelf_index: None,
            lid_t: 0.0,
            lid_ticks: 0,
            lid_faces: Default::default(),
            lid_clock_faces: Default::default(),
            lid_title_started: None,
            play_time: 0.0,
            select_physical: false,
            combo_releases: 0,
            eject_sound_armed: false,
            mode_last_tap: None,
            polaroids: None,
            photo_textures: Vec::new(),
            polaroid_title_texture: None,
            hud,
            clock_text: String::new(),
            date_text: String::new(),
            last_system_status_rev: 0,
            clock_texture: None,
            clock_face: Printed::default(),
            battery_percent: None,
            battery_low_shown: false,
            low_battery_badge,
            battery_texture: None,
            battery_face: Printed::default(),
            battery: None,
            bolt_texture,
            bolt_light_texture,
            letter_texture: None,
            letter_face: Printed::default(),
            letter_shown_at: None,
            platform_faces,
            recents_platform_face,
            favorites_platform_face,
            video_platform_face,
            test_platform_face,
            focus_seen: None,
            focus_dirty_since: None,
            menu_held: None,
            menu_repeat_dispatch: false,
            platform_shown_at: has_multiple_systems.then_some(now),
            selected_title_name: String::new(),
            selected_title_face: Printed::default(),
            selected_title_texture: None,
        }.with_shelf_wallpapers(visuals_root.as_deref()))
    }

    fn with_shelf_wallpapers(mut self, root: Option<&Path>) -> Self {
        if self.settings.wallpaper.is_mosaic() {
            self.reload_wallpaper(root);
        } else {
            self.reload_shelf_wallpapers(root);
        }
        self
    }

    /// Upstream Slot v1.13 decodes shelf wallpapers on a worker after boot so
    /// L1/R1 never waits on a PNG. The current shelf is decoded first. Only
    /// the shelves with a wallpaper of their own get a texture: the others
    /// draw General's, and None draws the theme's ground.
    fn reload_shelf_wallpapers(&mut self, root: Option<&Path>) {
        use crate::settings::WallpaperPick;
        let count = self.shelves.len();
        self.shelf_wallpaper_texs.resize(count, None);
        self.shelf_wallpaper_ready.resize(count, false);
        self.shelf_wallpaper_luma.resize(count, None);
        self.shelf_wallpaper_personal.resize(count, false);
        self.shelf_wallpaper_rx = None;
        let mut order: Vec<usize> = (0..count).collect();
        order.sort_by_key(|i| *i != self.active);
        // A shelf's own wallpaper, and General's Default: each shelf has its
        // own mosaic. Each with the look it shows (Blur, Halo).
        let jobs: Vec<(usize, WallpaperPick, crate::settings::WallpaperLook)> = order.into_iter()
            .filter_map(|i| self.settings.shelf_wallpaper(i).cloned()
                .or_else(|| self.settings.wallpaper.is_mosaic().then(|| self.settings.wallpaper.clone()))
                .map(|pick| (i, pick, self.settings.look_on(i))))
            .filter(|(_, pick, _)| matches!(pick, WallpaperPick::Default | WallpaperPick::Blur
                | WallpaperPick::Halo | WallpaperPick::Slot | WallpaperPick::SlotBlur | WallpaperPick::File(_)))
            .collect();
        // A shelf being remade keeps its wallpaper until the new one is
        // ready (no flash of General's while Blur or Halo change); the
        // others show General's.
        for shelf in 0..count {
            if !jobs.iter().any(|(i, _, _)| *i == shelf) {
                self.shelf_wallpaper_ready[shelf] = false;
                self.shelf_wallpaper_luma[shelf] = None;
                self.shelf_wallpaper_personal[shelf] = false;
            }
        }
        if jobs.is_empty() { return; }
        let root = root.map(Path::to_path_buf);
        let theme = self.settings.appearance.mosaic_theme();
        // Only a few native-sized RGBA images may wait for the GL uploader.
        // The producer alone waits; the GL thread still polls without blocking.
        let (tx, rx) = std::sync::mpsc::sync_channel(3);
        let spawned = std::thread::Builder::new().name("slot-wallpaper".into()).spawn(move || {
            for (index, pick, look) in jobs {
                let (rgba, personal) = shelf_wallpaper_pixels(root.as_deref(), &pick, look, index, theme);
                // A missing or unreadable image: the shelf shows General's.
                let Some(rgba) = rgba else {
                    if tx.send((index, Vec::new(), 0.0, false)).is_err() { return; }
                    continue
                };
                let luma = slot_ui::top_line_luma(&rgba, slot_ui::native_w());
                if tx.send((index, rgba, luma, personal)).is_err() { return; }
            }
        });
        match spawned {
            Ok(_) => self.shelf_wallpaper_rx = Some(rx),
            Err(error) => crate::log_warn(&format!("shelf wallpaper worker failed: {error}")),
        }
    }

    /// Upload at most one decoded shelf wallpaper per frame.
    fn poll_shelf_wallpapers(&mut self) {
        let Some(rx) = self.shelf_wallpaper_rx.as_ref() else { return };
        let (index, rgba, luma, personal) = match rx.try_recv() {
            Ok(made) => made,
            // Every shelf done: nothing more to wait for.
            Err(std::sync::mpsc::TryRecvError::Disconnected) => { self.shelf_wallpaper_rx = None; return; }
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
        };
        if rgba.is_empty() {
            // Its image is gone: General's shows.
            if let Some(ready) = self.shelf_wallpaper_ready.get_mut(index) { *ready = false; }
            return;
        }
        crate::frame_watch::did(crate::frame_watch::WORK_WALLPAPER);
        let Some(slot) = self.shelf_wallpaper_texs.get_mut(index) else { return };
        // The size General's wallpaper is made at (reload_wallpaper).
        let (w, h) = (slot_ui::native_w(), slot_ui::native_h());
        *slot = Some(if let Some(tex) = *slot {
            self.gpu.update_texture(tex, w, h, &rgba);
            tex
        } else {
            self.gpu.create_texture(w, h, &rgba)
        });
        if let Some(tex) = *slot { self.gpu.pack_static_recovery(tex); }
        self.shelf_wallpaper_ready[index] = true;
        if let Some(slot) = self.shelf_wallpaper_luma.get_mut(index) { *slot = Some(luma); }
        if let Some(slot) = self.shelf_wallpaper_personal.get_mut(index) { *slot = personal; }
    }

    /// The wallpaper the shelf on screen shows: its own once decoded (its
    /// own pick, or its mosaic for General's Default), else General's; with
    /// how much its Tint shades it (an image only) and its top line's
    /// luma. None for Wallpaper None, the theme's ground.
    fn active_wallpaper(&self) -> Option<(slot_gfx::TexId, f32, Option<f32>)> {
        use crate::settings::WallpaperPick;
        let shelf = self.active;
        if *self.settings.wallpaper_on(shelf) == WallpaperPick::None { return None; }
        if (self.settings.shelf_wallpaper(shelf).is_some() || self.settings.wallpaper.is_mosaic())
            && self.shelf_wallpaper_ready.get(shelf).copied().unwrap_or(false) {
            if let Some(tex) = self.shelf_wallpaper_texs.get(shelf).copied().flatten() {
                let personal = self.shelf_wallpaper_personal.get(shelf).copied().unwrap_or(false);
                let darken = if personal { self.settings.look_on(shelf).darken_alpha() } else { 0.0 };
                return Some((tex, darken, self.shelf_wallpaper_luma.get(shelf).copied().flatten()));
            }
        }
        if self.settings.wallpaper == WallpaperPick::None { return None; }
        let darken = if self.wallpaper_personal { self.settings.wallpaper_look.darken_alpha() } else { 0.0 };
        self.wallpaper_texture.map(|tex| (tex, darken, self.wallpaper_top_luma))
    }

    /// The shelf on screen shows Wallpaper None: the theme's own colour.
    fn wallpaper_none(&self) -> bool {
        *self.settings.wallpaper_on(self.active) == crate::settings::WallpaperPick::None
    }

    /// The status bar's top line is inked black on a light wallpaper, white
    /// on a dark one, whatever the theme: the ground the carousel draws
    /// (the shelf's own wallpaper, else the chosen one, darkened by Wallpaper
    /// Tint; None is the dark ground). Siham 2026-11-11.
    fn top_line_dark_ink(&self) -> bool {
        // None is the theme's own colour (menu_style::shelf_ground).
        if self.wallpaper_none() { return menu_style::shelf_takes_dark_ink(); }
        let Some((_, tint, luma)) = self.active_wallpaper() else { return false };
        luma.unwrap_or(0.0) * (1.0 - tint) > slot_ui::LIGHT_WALLPAPER
    }

    /// The shelf's name, letter and game title in dark ink over the light
    /// colour Wallpaper None shows in Light, and Original's GBA and Game Boy
    /// Color; and over a light mosaic (Default, Default Blur: every one in
    /// Light, all but the Mega Drive's in Original. Siham 2026-11-18).
    fn shelf_ink(&self) -> Option<slot_gfx::InkMap> {
        let ink = menu_style::MenuPalette::LIGHT.ink.unwrap_or([0; 3]).map(|v| v as f32 / 255.0);
        if self.wallpaper_none() {
            if !menu_style::shelf_takes_dark_ink() { return None; }
            let [r, g, b, _] = menu_style::shelf_ground();
            return Some(slot_gfx::InkMap { paper: [r, g, b], ink });
        }
        if !self.settings.wallpaper_on(self.active).is_mosaic() { return None; }
        let ground = slot_ui::mosaic_ground(self.active, self.settings.appearance.mosaic_theme())?
            .map(|v| v as f32 / 255.0);
        let light = 0.299 * ground[0] + 0.587 * ground[1] + 0.114 * ground[2] > 0.5;
        light.then_some(slot_gfx::InkMap { paper: ground, ink })
    }

    fn refresh_task_pill(&mut self) {
        let rev = TASK_PILL_REV.load(Ordering::Acquire);
        if rev == self.task_pill_rev { return; }
        self.task_pill_rev = rev;
        let spec = TASK_PILL.lock().unwrap_or_else(|e| e.into_inner()).clone();
        self.task_pill = spec.lines().filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let kind = parts.next()?.parse::<u8>().ok()?;
            let progress = parts.next()?.parse::<f32>().ok()?;
            let words = parts.next()?.trim();
            (!words.is_empty()).then(|| (kind, progress, words.to_string()))
        }).collect();
    }

    /// The task the pill shows here: the first its own page doesn't show
    /// already (the copy and the scan on Library, the downloads on RomM's),
    /// with how many others run.
    fn task_pill_now(&self) -> Option<(u8, f32, String, usize)> {
        let hidden = |kind: u8| match self.overlay {
            ShelfOverlay::Library { .. } => kind == TASK_COPY || kind == TASK_SCAN,
            ShelfOverlay::Romm => kind == TASK_DOWNLOADS,
            _ => false,
        };
        let shown: Vec<&(u8, f32, String)> = self.task_pill.iter().filter(|t| !hidden(t.0)).collect();
        let first = shown.first()?;
        Some((first.0, first.1, first.2.clone(), shown.len() - 1))
    }

    /// The pill's texture now, made again only when what it says changes
    /// (its ring in 40 steps); its logical size.
    fn task_pill_tex(&mut self) -> Option<(u8, slot_gfx::TexId, f32, f32)> {
        self.refresh_task_pill();
        let (kind, progress, words, more) = self.task_pill_now()?;
        let filled = (progress >= 0.0).then(|| (progress.min(1.0) * 40.0).round() / 40.0);
        let key = format!("{kind}|{words}|{more}|{filled:?}");
        if key != self.task_pill_key || self.task_pill_face.is_none() {
            let face = slot_ui::task_pill_face(&words, more, filled, kind == TASK_PROBLEM);
            let tex = match self.task_pill_face {
                Some((tex, _, _)) => { self.gpu.update_face(tex, &face); tex }
                None => self.gpu.create_face(&face),
            };
            self.task_pill_face = Some((tex, face.w, face.h));
            self.task_pill_key = key;
        }
        self.task_pill_face.map(|(tex, w, h)| (kind, tex, w as f32, h as f32))
    }

    /// The pill at `x`, `y`, white or inked as `ink` (a problem stays red).
    fn push_task_pill(kind: u8, tex: slot_gfx::TexId, x: f32, y: f32, w: f32, h: f32,
                      ink: Option<slot_gfx::InkMap>, out: &mut Vec<Draw>) {
        let ink = ink.filter(|_| kind != TASK_PROBLEM);
        out.push(Draw::Ink(ink));
        out.push(Draw::Tex { x: x.round(), y, w, h, tex, alpha: 1.0 });
        out.push(Draw::Ink(None));
    }

    /// On the shelf: by the battery when they share the top line, else at
    /// the top right (below the icons when they hold that corner). The room
    /// it takes beside the battery, for the icons Android places there.
    fn draw_shelf_task_pill(&mut self, clock_top: bool, icons_top: bool, battery_w: f32,
                            dark_ink: bool, out: &mut Vec<Draw>) -> f32 {
        let Some((kind, tex, w, h)) = self.task_pill_tex() else { return 0.0 };
        let margin = if out_w() < 640 { 12.0 } else { 24.0 };
        let (x, y, room) = if clock_top {
            // Before Wi-Fi and Bluetooth when they show (Android places them
            // 10.75 before the battery), else by the battery.
            let icons = STATUS_ICONS_W.load(Ordering::Acquire) as f32;
            if icons > 0.0 {
                (out_w() as f32 - margin - battery_w - 10.75 - icons - 12.0 - w, 14.0, 0.0)
            } else {
                (out_w() as f32 - margin - battery_w - 12.0 - w, 14.0, w + 12.0)
            }
        } else if icons_top {
            (out_w() as f32 - margin - w, 40.0, 0.0)
        } else {
            (out_w() as f32 - margin - w, 14.0, 0.0)
        };
        let ink = dark_ink.then_some(slot_gfx::InkMap { paper: [0.0; 3], ink: [0.0; 3] });
        Self::push_task_pill(kind, tex, x, y, w, h, ink, out);
        room
    }

    /// Over a menu page: at the top right, in the page's colours.
    fn draw_menu_task_pill(&mut self, out: &mut Vec<Draw>) {
        if self.game.is_some() || self.progress > 0.0
            // About: it would cover the version's label (Siham 2026-11-25).
            || matches!(self.overlay, ShelfOverlay::None | ShelfOverlay::CartStudio | ShelfOverlay::Shelves { .. }
                | ShelfOverlay::About { .. }) { return; }
        let Some((kind, tex, w, h)) = self.task_pill_tex() else { return };
        let margin = if out_w() < 640 { 12.0 } else { 24.0 };
        let ink = self.menu_ink();
        Self::push_task_pill(kind, tex, out_w() as f32 - margin - w, 14.0, w, h, ink, out);
    }

    /// The grey of carts whose card is out, for the ground behind them: the
    /// shelf's own colour or mosaic as its words see it (the Mega Drive's
    /// dark Original mosaic takes the dark ground's grey), else the image's
    /// brightness, as the clock's ink.
    fn missing_ink(&self) -> slot_gfx::InkMap {
        let light = if self.wallpaper_none() || self.settings.wallpaper_on(self.active).is_mosaic() {
            self.shelf_ink().is_some()
        } else { self.top_line_dark_ink() };
        if light { MISSING_INK_LIGHT } else { MISSING_INK_DARK }
    }

    /// The carts' ink while their card is out (most of the fade), for the
    /// paths that draw carts outside the shelf's own row: the core picker's
    /// neighbours and Cart Studio's flight (beta39: they showed in colour).
    pub(super) fn cart_ink(&self) -> Option<slot_gfx::InkMap> {
        (self.missing_mix > 0.5).then(|| self.missing_ink())
    }

    /// Whether Delete Wallpaper has a file to remove: the edited shelf's own
    /// Wallpapers/<shelf>.png, user.png for General.
    fn removable_wallpaper(&self) -> bool {
        match self.wallpaper_edit {
            Some(shelf) => Self::recents_root().is_some_and(|root|
                crate::wallpaper::for_shelf(&root, shelf).is_some()),
            None => self.custom_wallpaper_present,
        }
    }

    fn set_wallpaper_target(&self) {
        let target = self.wallpaper_edit.and_then(crate::wallpaper::shelf_target);
        *WALLPAPER_TARGET.lock().unwrap_or_else(|e| e.into_inner()) = target;
    }

    /// Black or white footer ink: the console bar's own (the GBA's with no
    /// cart yet), or the theme's over a custom-coloured bar.
    fn footer_dark_ink(&self) -> bool {
        let shelf = &self.shelves[self.active];
        match self.slot_faces {
            Some(_) => slot_ui::bitmaps::console_slot_takes_dark_ink(
                shelf.carts.get(shelf.index).map_or(GBA_SLOT, slot_ui::bitmaps::console_slot_family),
                self.settings.appearance.bar_theme()),
            None => self.settings.appearance == crate::settings::Appearance::Light,
        }
    }

    fn recolour_footer_text(&mut self) {
        self.recolour_footer_text_to(self.footer_dark_ink());
    }

    fn recolour_footer_text_to(&mut self, dark: bool) {
        self.footer_inked_dark = dark;
        let ink = slot_ui::footer_ink(dark);
        if let Some(tex) = self.clock_texture {
            let face = slot_ui::word_ink(&self.clock_text, ink);
            self.gpu.update_face(tex, &face);
            self.clock_face = Printed::new(tex, face.w);
        }
        if let (Some(tex), Some(percent)) = (self.battery_texture, self.battery_percent) {
            let face = slot_ui::word_ink(&format!("{percent}%"), self.battery_ink(ink));
            self.gpu.update_face(tex, &face);
            self.battery_face = Printed::new(tex, face.w);
        }
    }

    fn refresh_system_status(&mut self) {
        let revision = SYSTEM_STATUS_REV.load(Ordering::Acquire);
        if self.last_system_status_rev == revision { return; }
        let status = SYSTEM_STATUS.lock().unwrap_or_else(|p| p.into_inner()).clone();
        self.last_system_status_rev = revision;
        let clock = self.settings.time_format.format_clock(&status.clock);
        if clock != self.clock_text {
            self.clock_text = clock;
            if !self.clock_text.is_empty() {
                let face = slot_ui::word_ink(&self.clock_text, slot_ui::footer_ink(self.footer_inked_dark));
                let tex = match self.clock_texture {
                    Some(tex) => { self.gpu.update_face(tex, &face); tex }
                    None => { let tex = self.gpu.create_face(&face);
                        self.clock_texture = Some(tex); tex }
                };
                self.clock_face = Printed::new(tex, face.w);
            }
        }
        self.battery = status.battery;
        self.date_text = status.date;
        let percent = self.battery.map(|b| b.percent);
        let low = slot_ui::battery_low(self.battery, slot_ui::LOW_PERCENT);
        if percent != self.battery_percent || low != self.battery_low_shown {
            self.battery_percent = percent;
            self.battery_low_shown = low;
            // Just as with original Slot, the About label reflects the live
            // battery without rebuilding it on every animation frame.
            if let Some(tex) = self.about_texture {
                // The same sticker as prepare_about: the KONKR's own, or the
                // handheld's (it went back to the KONKR's on every battery
                // step, Siham 2026-11-15).
                let sticker = about_sticker(&StickerFields {
                    battery: percent, serial: "0000130", dirty_digit: '0',
                });
                self.gpu.update_face(tex, &sticker);
            }
            if let Some(percent) = percent {
                let face = slot_ui::word_ink(&format!("{percent}%"),
                    self.battery_ink(slot_ui::footer_ink(self.footer_inked_dark)));
                let tex = match self.battery_texture {
                    Some(tex) => { self.gpu.update_face(tex, &face); tex }
                    None => { let tex = self.gpu.create_face(&face);
                        self.battery_texture = Some(tex); tex }
                };
                self.battery_face = Printed::new(tex, face.w);
            } else { self.battery_face = Printed::default(); }
        }
    }

    /// The battery percent's ink: red while it is low (#87).
    fn battery_ink(&self, ink: [u8; 3]) -> [u8; 3] {
        if slot_ui::battery_low(self.battery, slot_ui::LOW_PERCENT) { slot_ui::LOW_RED } else { ink }
    }

    /// Over the game, under 10 % and not charging: a small red battery in
    /// the top right corner, a second on, a second off (#87).
    fn push_low_battery(&self, out: &mut Vec<Draw>) {
        if !slot_ui::battery_low(self.battery, slot_ui::VERY_LOW_PERCENT) { return; }
        if (self.born.elapsed().as_millis() / 1000) % 2 == 1 { return; }
        out.push(Draw::Tex {
            x: out_w() as f32 - 12.0 - slot_ui::BADGE_W, y: 10.0,
            w: slot_ui::BADGE_W, h: slot_ui::BADGE_H, tex: self.low_battery_badge, alpha: 1.0,
        });
    }

    /// COPYING n% on Library's slot. folder row, redrawn as it moves.
    /// Moves the carts' grey toward the card's state: 0.3 s each way.
    fn advance_missing(&mut self) -> f32 {
        let target = if ROMS_MISSING.load(Ordering::Acquire) { 1.0 } else { 0.0 };
        let dt = self.missing_at.elapsed().as_secs_f32().min(0.1);
        self.missing_at = Instant::now();
        let step = dt / MISSING_FADE_S;
        self.missing_mix = if self.missing_mix < target { (self.missing_mix + step).min(target) }
            else { (self.missing_mix - step).max(target) };
        self.missing_mix
    }

    fn refresh_slot_copy_face(&mut self) {
        let percent = SLOT_FOLDER_COPY.load(Ordering::Acquire);
        if percent == self.slot_copy_shown { return; }
        self.slot_copy_shown = percent;
        if percent < 0 { return; }
        // "COPYING n%", or just "n%" where it would reach the row's own
        // label (a square screen like the Pocket Classic's, beta38).
        let long = quick_value_face(&format!("COPYING {percent}%"), true);
        let label = self.menu_textures[LIBRARY_ROWS[LIBRARY_SLOT_ROW]].1 as f32 * 0.79;
        let room = out_w() as f32 - 34.0 - 37.0 - label - 24.0;
        let face = if long.w as f32 * 0.79 <= room { long }
            else { quick_value_face(&format!("{percent}%"), true) };
        self.gpu.update_face(self.slot_copy_face.0, &face);
        self.slot_copy_face = (self.slot_copy_face.0, face.w, face.h);
    }

    fn reset_time_controls(&mut self) {
        self.ff_held = false;
        self.ff_latched = false;
        self.ff_last_release = None;
        self.rewind_held = false;
        self.rewind_accum = 0.0;
        self.game_accum = 0.0;
        self.select_down_at = None;
        self.select_home_consumed = false;
        self.select_chorded = false;
        self.select_forwarded = false;
        self.select_tap_frames = 0;
        self.chord_releases = 0;
        self.hud.set_ff(FfState::Off);
        self.hud.release_rewind();
        AUDIO_SPEED_PERMILLE.store(1000, Ordering::Release);
        clear_rumble();
    }


    /// A short press of the slot. button in game: the game menu, or the
    /// Polaroids when it follows another press within 360 ms.
    fn slot_button_tap(&mut self) {
        if self.game.as_ref().is_some_and(GameSession::requires_relaunch) {
            self.mode_last_tap = None;
            self.overlay = ShelfOverlay::GameMenu { row: 0 };
            set_message("Press A to eject, then reinsert to play solo.".into());
        } else if self.game.is_some() {
            let is_double=self.mode_last_tap.take()
                .is_some_and(|t|t.elapsed().as_millis()<360);
            self.buttons=0;
            self.game_accum=0.0;
            AUDIO.lock().unwrap_or_else(|e|e.into_inner()).clear();
            self.reset_time_controls();
            clear_rumble();
            if is_double {
                self.menu_reveal_at=None;
                self.states_from_menu=false;
                self.open_polaroids();
            } else {
                self.mode_last_tap=Some(Instant::now());
                // Opening: show the menu only once a second tap (Save
                // States) can no longer come. The game is paused already.
                if matches!(self.overlay, ShelfOverlay::None) {
                    self.menu_reveal_at=Some(Instant::now()+Duration::from_millis(MENU_REVEAL_MS));
                }
                self.overlay=match self.overlay {
                    ShelfOverlay::GameMenu { .. }
                    | ShelfOverlay::PauseSettings { .. }
                    | ShelfOverlay::PauseScreenSettings { .. }
                    | ShelfOverlay::PauseGameplaySettings { .. }
                    | ShelfOverlay::PauseEmulationSettings { .. }
                    | ShelfOverlay::LayoutPorts { game: true, .. }
                    | ShelfOverlay::LayoutPort { game: true, .. }
                    | ShelfOverlay::Cheats { .. }
                    | ShelfOverlay::States => ShelfOverlay::None,
                    _=>ShelfOverlay::GameMenu {row:0},
                };
            }
        }
    }

    /// The slot. button on any handheld, in game: SELECT+X is a short press
    /// (game menu, twice for the Polaroids) and SELECT+START ejects at once
    /// (save and eject). Both keys' releases are consumed too.
    fn select_slot_combo(&mut self, code: i32, pressed: bool) -> bool {
        let bit = match code { 99 => 1u8, 108 => 2, _ => return false };
        if !pressed {
            if self.combo_releases & bit == 0 { return false; }
            self.combo_releases &= !bit;
            return true;
        }
        if !self.select_physical || self.game.is_none()
            || !matches!(self.overlay, ShelfOverlay::None | ShelfOverlay::GameMenu { .. }) {
            return false;
        }
        // A+B+START+SELECT is the Game Boy soft reset: leave it to the game.
        if code == 108 && self.buttons & (ButtonMask::A | ButtonMask::B) != 0 {
            return false;
        }
        self.combo_releases |= bit;
        // SELECT belongs to the gesture, never to the game.
        self.select_chorded = true;
        self.select_forwarded = false;
        self.select_tap_frames = 0;
        self.buttons &= !ButtonMask::SELECT;
        if code == 99 {
            self.slot_button_tap();
        } else {
            self.mode_last_tap = None;
            self.mode_down_at = None;
            crate::game::log_launch_timing("Exit requested by SELECT+START");
            self.handle(Input::Exit);
        }
        true
    }

    // Upstream slot-input gestures: SELECT chords have priority over game input.
    // Distinct release bits avoid leaking A/B/shoulder releases into libretro.
    fn handle_select_chord(&mut self, code: i32, pressed: bool) -> bool {
        let bit = select_chord_bit(code);
        if !pressed && bit != 0 && self.chord_releases & bit != 0 {
            self.chord_releases &= !bit;
            return true;
        }
        if !pressed || bit == 0 || self.select_down_at.is_none() { return false; }
        self.chord_releases |= bit;
        self.select_chorded = true;
        self.select_tap_frames = 0;
        self.select_forwarded = false;
        self.buttons &= !ButtonMask::SELECT;
        // SELECT + Y: the Screen page (Color Correction is on it).
        if code == 100 { self.open_screen_shortcut(); } else { self.shortcut(code); }
        true
    }

    /// Hotkey + Y: the in-game Screen page straight from the game, the slot
    /// rising with it; Back or slot. sends it down and the game resumes
    /// (Siham 2026-11-06).
    fn open_screen_shortcut(&mut self) {
        let Some(game) = self.game.as_ref() else { return };
        if game.requires_relaunch() || !matches!(self.overlay, ShelfOverlay::None) { return; }
        self.buttons = 0;
        self.game_accum = 0.0;
        AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
        self.reset_time_controls();
        clear_rumble();
        self.menu_reveal_at = None;
        self.mode_last_tap = None;
        self.footer = 0.0;
        self.screen_closing = false;
        self.screen_direct = true;
        // Its Hotkey Button is still down: Y again closes the page.
        self.screen_hotkey_held = true;
        self.overlay = ShelfOverlay::PauseScreenSettings { row: 0 };
    }

    /// The SELECT + button shortcuts, also played by L1/R1 when a system's
    /// Controls give them a shortcut (Load / Save, Shader, Palette).
    fn shortcut(&mut self, code: i32) {
        let time = self.born.elapsed().as_millis() as u64;
        match code {
            96 | 97 => {
                let platform = self.current_cart_platform();
                let key = if platform == Platform::Gba { 0 } else {
                    crate::settings::Settings::home_shader_key(platform).unwrap_or(1)
                };
                if self.game.is_some() {
                    self.change_game_screen_or_gameplay(key,code == 96);
                } else { self.change_setting(key,code == 96); }
                let shader = self.game_shader();
                self.apply_game_look();
                self.hud.toast(match shader {
                    crate::settings::Shader::Off => Toast::ShaderOff,
                    crate::settings::Shader::Lcd3x => Toast::ShaderLcd3x,
                    crate::settings::Shader::Grid => Toast::ShaderGrid,
                    crate::settings::Shader::Dot => Toast::ShaderDot,
                    crate::settings::Shader::Simpletex => Toast::ShaderSimpletex,
                     crate::settings::Shader::LcdGridV2 => Toast::ShaderLcdGridV2,
                     crate::settings::Shader::LcdGridV2Gba => Toast::ShaderLcdGridV2Gba,
                     crate::settings::Shader::ZfastLcd => Toast::ShaderZfastLcd,
                     crate::settings::Shader::Lcd1x => Toast::ShaderLcd1x,
                     crate::settings::Shader::SimpletexGba => Toast::ShaderSimpletexGba,
                     crate::settings::Shader::Agb001 => Toast::ShaderAgb001,
                     crate::settings::Shader::Ags001 => Toast::ShaderAgs001,
                     crate::settings::Shader::SharpBilinear => Toast::ShaderSharpBilinear,
                     crate::settings::Shader::ZfastCrt => Toast::ShaderZfastCrt,
                     crate::settings::Shader::CrtGdvTrinitron => Toast::ShaderCrtGdv,
                     crate::settings::Shader::CrtGeomDeluxe => Toast::ShaderCrtGeomDeluxe,
                     crate::settings::Shader::CrtPi => Toast::ShaderCrtPi,
                     crate::settings::Shader::Fakelottes => Toast::ShaderFakelottes,
                     crate::settings::Shader::CrtAperture => Toast::ShaderCrtAperture,
                     crate::settings::Shader::DotMatrix => Toast::ShaderDotMatrix,
                     crate::settings::Shader::CrtEasymode => Toast::ShaderCrtEasymode,
                     crate::settings::Shader::CrtHyllian => Toast::ShaderCrtHyllian,
                     crate::settings::Shader::Sabr => Toast::ShaderSabr,
                     crate::settings::Shader::SuperEagle => Toast::ShaderSuperEagle,
                     crate::settings::Shader::XbrLv2 => Toast::ShaderXbrLv2,
                     crate::settings::Shader::DotMatrixColor => Toast::ShaderDotMatrixColor,
                     crate::settings::Shader::CrtRoyale => Toast::ShaderCrtRoyale,
                }, time);
            }
            100 => {
                let platform=self.current_cart_platform();
                // Game Boy colours are the palette's: nothing to correct.
                if platform==Platform::Gb {return;}
                if self.game_settings().shader_for(platform).includes_gba_color() {
                    set_message("This shader already includes color correction.".into());
                    return;
                }
                if self.game.is_some() {
                    self.change_game_screen_or_gameplay(2,true);
                } else {
                    // Global shortcut should toggle the selected platform,
                    // not the old GBA-only compatibility setting.
                    let key=match platform {
                        Platform::Gba=>22, Platform::Gbc=>23, Platform::Gb=>24,
                        _=>return,
                    };
                    self.change_setting(key,true);
                }
                self.hud.toast(if self.game_settings().colour_for(platform) {
                    Toast::ColourOn
                } else { Toast::ColourOff }, time);
            }
            102 => {
                if let Some(game) = self.game.as_mut() {
                    match game.load_latest() {
                        Ok(()) => self.hud.toast(Toast::StateLoaded, time),
                        Err(err) => set_error_message("Couldn't load this save state.", &err),
                    }
                }
            }
            103 => {
                if let Some(game) = self.game.as_mut() {
                    match game.save_manual() {
                        Ok(stamp) => {
                            self.hud.toast(Toast::StateSaved, time);
                            if let Some(uri) = self.requested_uri.as_deref() {
                                let core = game.core_kind().as_str();
                                let event = serde_json::json!({
                                    "uri": uri, "core": core, "manual_stamp": stamp
                                }).to_string();
                                SAVE_SYNC.lock().unwrap_or_else(|e| e.into_inner())
                                    .push_back(event);
                            }
                        }
                        Err(err) => set_error_message("State saved, but not copied to your folder.", &err),
                    }
                }
            }
            // SELECT + Up / Down: one screen size up / down on every system.
            19 | 20 => self.step_screen_size(code == 19),
            // SELECT + Left / Right: Run-Ahead OFF, 1 FRAME, 2 FRAMES.
            21 | 22 => {
                let frames = |s: &Self| s.game_emulation
                    .map_or(s.settings.run_ahead_frames, |p| p.run_ahead_frames);
                let before = frames(self);
                self.change_game_emulation(12, code == 22);
                let after = frames(self);
                if after != before {
                    self.hud.toast(match after {
                        1 => Toast::RunAhead1, 2 => Toast::RunAhead2, _ => Toast::RunAheadOff,
                    }, time);
                }
            }
            // Boktai's sun, like L3 / R3, for handhelds without stick clicks.
            104 | 105 if self.game.as_ref().is_some_and(GameSession::solar) => {
                self.nudge_sun(code == 105);
            }
            // A tilt or gyro game's sensitivity, on the same keys.
            104 | 105 if self.game.as_ref().is_some_and(GameSession::motion) => {
                self.nudge_motion(code == 105);
            }
            // The WonderSwan's and the Pokemon mini's palettes, as the Game
            // Boy's (Siham 2026-11-22). A colour WonderSwan game has none.
            104 | 105 if matches!(self.current_cart_platform(), Platform::Ws | Platform::Pokemini)
                && self.game.as_ref().is_some_and(|g| !g.linked())
                && !(self.current_cart_platform() == Platform::Ws
                    && self.game.as_ref().is_some_and(|g| crate::game::colour_cart(Platform::Ws, g.rom_path()))) => {
                    let platform = self.current_cart_platform();
                    let (key, first) = if platform == Platform::Ws {
                        (crate::settings::WS_PALETTE_KEY, 0u8)
                    } else {
                        (crate::settings::POKEMINI_PALETTE_KEY, 4)
                    };
                    // This game's palette, as the Screen page's row (2026-11-22).
                    self.change_game_screen_or_gameplay(key, code == 105);
                    let view = self.game_settings();
                    let at = if platform == Platform::Ws { view.ws_palette } else { view.pokemini_palette };
                    self.hud.toast(Toast::OwnPalette(first + at), time);
                }
            // Also valid for GB-compatible .gbc ROMs (CGB flag bit 7 unset).
            104 | 105 if self.game_settings().gb_palettes
                && self.game.as_ref().is_some_and(|g| g.can_change_gb_palette()) => {
                    let index = if code == 105 {
                        (self.settings.gb_palette + 1) % slot_store::GbPalette::COUNT as u8
                    } else {
                        let count = slot_store::GbPalette::COUNT as u8;
                        (self.settings.gb_palette + count - 1) % count
                    };
                    self.settings.gb_palette = index;
                    if let Some(game) = self.game.as_mut() {
                        game.set_gb_palette(self.settings.palette_name());
                    }
                    let palette = slot_store::GbPalette::all().nth(index as usize)
                        .unwrap_or(slot_store::GbPalette::DEFAULT);
                    self.hud.toast(Toast::Palette(palette), time);
                    self.persist_settings();
                }
            _ => {}
        }
    }

    /// SELECT + slot. (or its hotkey): the Multiplayer screen over the game.
    fn open_multiplayer_shortcut(&mut self) {
        // The SELECT+HOME gesture belongs to Slot, not to
        // libretro or Android's normal HOME handler.
        self.select_down_at = None;
        self.select_chorded = true;
        self.select_forwarded = false;
        self.select_tap_frames = 0;
        self.buttons = 0;
        self.mode_down_at = None;
        self.mode_last_tap = None;
        self.game_accum = 0.0;
        if self.game.as_ref().is_some_and(GameSession::linked) {
            self.link_entry = LinkEntry::Shortcut;
            // Upstream SELECT+MENU reopens the animated
            // linked adapter and A=End Link / B=Back.
            self.link_phase = LinkMenu::Linked {
                role: self.link_role,
                worked: self.link_worked_at,
                since: self.link_millis(),
                opened: true,
            };
            self.overlay = ShelfOverlay::Multiplayer;
        } else if self.multiplayer_offered() {
            self.enter_multiplayer_menu();
            if matches!(self.overlay, ShelfOverlay::Multiplayer) {
                self.link_entry = LinkEntry::Shortcut;
            }
        }
    }

    /// Rewind's key (L2 by default) held or released.
    fn rewind_key(&mut self, pressed: bool) {
        if !self.live_game_options().rewind { return; }
        if pressed {
            self.rewind_held = true;
            self.rewind_accum = 0.0;
            self.ff_held = false;
            self.ff_latched = false;
            self.ff_last_release = None;
            self.hud.set_ff(FfState::Off);
            AUDIO_SPEED_PERMILLE.store(1000, Ordering::Release);
            AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
        } else {
            self.rewind_held = false;
            self.rewind_accum = 0.0;
            self.game_accum = 0.0;
            self.hud.release_rewind();
            AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
    }

    /// Fast Forward's key (R2 by default): held, or double-tapped to lock.
    fn fast_forward_key(&mut self, pressed: bool) {
        // Fast Forward Off (original Slot 1.8.1): R2 does nothing.
        if pressed && self.live_game_options().ff_speed == 0 { return; }
        if pressed {
            if self.ff_latched {
                self.ff_latched = false;
                self.ff_held = false;
                self.ff_last_release = None;
            } else {
                let double = self.ff_last_release.take().is_some_and(
                    |at| at.elapsed().as_millis() <= FF_DOUBLE_TAP_MS);
                self.ff_latched = double;
                self.ff_held = true;
            }
            self.rewind_held = false;
            self.hud.release_rewind();
            AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
        } else if self.ff_held {
            self.ff_held = false;
            self.ff_last_release = Some(Instant::now());
        }
        AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
        AUDIO_SPEED_PERMILLE.store(
            if self.ff_latched || self.ff_held {
                // MAX: Android stretches sound up to 6x at most.
                i32::from(self.live_game_options().ff_speed.min(6)) * 1000
            } else { 1000 },
            Ordering::Release);
        self.hud.set_ff(match (self.ff_latched, self.ff_held) {
            (true, _) => FfState::Latched,
            (false, true) => FfState::Held,
            (false, false) => FfState::Off,
        });
    }

    fn link_millis(&self) -> u64 {
        self.born.elapsed().as_millis() as u64
    }

    fn cancel_link_discovery(&mut self) {
        if let Some(starter) = self.link_starter.take() { starter.cancel(); }
        LINK_NEEDS_MULTICAST.store(false, Ordering::Release);
        self.link_inet.end_attempt();
    }

    /// Keys of the Internet tab. True when the key was used here.
    fn internet_link_key(&mut self, code: i32) -> bool {
        use crate::link_internet as inet;
        if let Some(entry) = self.link_inet.entry.as_mut() {
            if !matches!(self.link_phase, LinkMenu::Pick(_)) {
                self.link_inet.entry = None;
                return false;
            }
            match code {
                inet::KEY_UP => entry.up(),
                inet::KEY_DOWN => entry.down(),
                inet::KEY_LEFT => entry.left(),
                inet::KEY_RIGHT => entry.right(),
                inet::KEY_0..=inet::KEY_9 => entry.type_digit((code - inet::KEY_0) as u8),
                96 => {
                    let typed = entry.code();
                    self.link_inet.entry = None;
                    self.link_inet.last_code = Some(typed.clone());
                    self.start_internet_link(Some(typed));
                }
                97 | 108 => self.link_inet.entry = None,
                _ => {}
            }
            self.refresh_internet_heading();
            return true;
        }
        match self.link_phase {
            LinkMenu::Pick(role) => match code {
                inet::KEY_L1 => { self.link_inet.internet = false; true }
                inet::KEY_R1 => { self.link_inet.internet = true; true }
                96 if self.link_inet.internet => {
                    if role == LinkRow::Host {
                        self.start_internet_link(None);
                    } else {
                        self.link_inet.entry = Some(inet::CodeEntry::new(
                            self.link_inet.last_code.as_deref()));
                        self.refresh_internet_heading();
                    }
                    true
                }
                _ => false,
            },
            LinkMenu::Working { .. } if code == inet::KEY_Y
                && self.link_inet.can_share(self.link_phase) => {
                self.share_room_code();
                true
            }
            _ => false,
        }
    }

    /// Internet Multiplayer: both consoles dial the relay. Host gets a room
    /// code to share; Join brings the code it was sent.
    fn start_internet_link(&mut self, code: Option<String>) {
        if !matches!(self.overlay, ShelfOverlay::Multiplayer) { return; }
        if matches!(self.link_phase, LinkMenu::Working {..} | LinkMenu::Linked {..}) { return; }
        if self.game.as_ref().is_some_and(|g| g.core_kind() == Core::Gambatte) {
            set_message("WAN link isn't ready yet. L1 for LAN.".into());
            return;
        }
        let storage = PATHS.lock().unwrap_or_else(|e| e.into_inner())
            .clone().map(|(storage, _)| storage);
        let Some(relay) = crate::link_internet::relay_address(storage.as_deref()) else {
            set_message("WAN Multiplayer isn't set up in this build.".into());
            return;
        };
        if let Err(error) = self.ensure_gpsp_link_serial() {
            self.link_worked_at = self.link_millis();
            self.fail_multiplayer(&error, LinkFail::Radio);
            return;
        }
        let config = match self.link_config() {
            Ok(config) => config,
            Err(error) => {
                self.link_worked_at = self.link_millis();
                self.fail_multiplayer(&error, LinkFail::Radio);
                return;
            }
        };
        self.cancel_link_discovery();
        let now = self.link_millis();
        self.link_worked_at = now;
        self.link_phase = LinkMenu::Working {
            role: self.link_role, step: LinkStep::Waiting, since: now,
        };
        let role = if self.link_role == LinkRow::Host { LinkRole::Host } else { LinkRole::Join };
        let network = crate::multiplayer::Network::Internet { relay, code: code.clone() };
        match LinkStarter::spawn_on(role, network, config) {
            Ok(starter) => {
                self.link_starter = Some(starter);
                self.link_inet.session = true;
                self.link_inet.joining = code;
                // Ready before the next draw: never a frame of Local text.
                self.refresh_internet_heading();
            }
            Err(error) => self.fail_multiplayer(&error.to_string(), LinkFail::Radio),
        }
    }

    /// Y on a waiting Internet host: Android's share sheet (WhatsApp, SMS...).
    fn share_room_code(&mut self) {
        let Some(code) = self.link_inet.room.clone() else { return };
        let game = self.game.as_ref().map(|g| {
            let stem = g.rom_stem();
            stem.split(" (").next().unwrap_or(stem).trim().to_owned()
        }).unwrap_or_default();
        *LINK_SHARE_TEXT.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(crate::link_internet::share_text(&code, &game));
        UI_ACTION.lock().unwrap_or_else(|e| e.into_inner()).push_back(23);
    }

    /// The Internet heading face follows the text it should show; it is
    /// rendered here, on the GL thread, only when that text changes.
    fn refresh_internet_heading(&mut self) {
        let Some(text) = self.link_inet.heading(self.link_phase, self.link_millis()) else { return };
        if text == self.link_inet_face_text && self.link_inet_face.is_some() { return; }
        let face = menu_face(&text);
        let tex = match self.link_inet_face {
            Some((tex, _, _)) => { self.gpu.update_face(tex, &face); tex }
            None => self.gpu.create_face(&face),
        };
        self.link_inet_face = Some((tex, face.w, face.h));
        self.link_inet_face_text = text;
    }

    fn enter_multiplayer_menu(&mut self) {
        if self.game.is_none() { return; }
        if self.game.as_ref().is_some_and(GameSession::requires_relaunch) {
            set_message("Eject and reinsert the cartridge to link again.".into());
            return;
        }
        if self.link_identity.is_none() {
            set_message("Multiplayer needs a valid cartridge.".into());
            return;
        }
        let game = self.game.as_ref().expect("checked above");
        let identity = self.link_identity.as_ref().expect("checked above");
        if game.core_kind() == Core::Bsnes {
            set_message("Multiplayer needs the Snes9x core.".into());
            return;
        }
        // As bsnes (Siham 2026-11-22): Gearsystem has no netplay.
        if game.core_kind() == Core::Gearsystem {
            set_message("Multiplayer needs Genesis Plus GX.".into());
            return;
        }
        if game.core_kind() != Core::Gpsp {
            // Slot original lockstep uses Cable only. mGBA is reopened in
            // dual-GBA (or dual-Game Boy) mode after the network connection
            // succeeds; Gambatte plugs its own Game Link cable in place.
            self.link_mode = LinkKind::Cable;
        } else {
            if !game.wireless_capable() {
                set_message("No Multiplayer with gpSP. Try mGBA.".into());
                return;
            }
            if !link_kind::link_carried(&identity.code, &identity.title) {
                crate::game::log_launch_timing(&format!(
                    "Multiplayer unsupported protocol: game_code={}, title={}, clean_header={}",
                    identity.code, identity.title, identity.clean_header
                ));
                set_message("gpSP can't link this game yet. Try mGBA.".into());
                return;
            }
            self.link_mode = self.link_mode_override.unwrap_or_else(||
                link_kind::preferred_mode(&identity.code, &identity.title,
                    identity.clean_header));
            if !link_kind::gpsp_runtime_mode_supported(&identity.code, self.link_mode) {
                set_message("gpSP can't link this game. Try mGBA on both.".into());
                return;
            }
        }
        self.cancel_link_discovery();
        self.link_role = LinkRow::Host;
        self.link_phase = LinkMenu::Pick(self.link_role);
        self.game_accum = 0.0;
        self.buttons = 0;
        self.overlay = ShelfOverlay::Multiplayer;
    }

    /// Upstream SELECT Mode is only offered if its two values produce
    /// genuinely different gpSP serial hardware configurations. mGBA is
    /// intentionally Cable-only with this Android dual-GBA Lab backend.
    fn link_mode_switchable(&self) -> bool {
        let Some(identity) = self.link_identity.as_ref() else { return false };
        let Some(game) = self.game.as_ref() else { return false };
        link_mode_can_switch(game.core_kind(), self.link_mode,
            &identity.code, &identity.title, identity.clean_header)
    }

    /// Original Slot/fork reload the gpSP core when SELECT changes the
    /// serial hardware. Never begin a connection with a cosmetic mode that
    /// differs from the live libretro serial implementation.
    fn ensure_gpsp_link_serial(&mut self) -> Result<(), String> {
        let game = self.game.as_ref().ok_or("No running GBA game")?;
        if game.core_kind() != Core::Gpsp { return Ok(()); }
        let identity = self.link_identity.as_ref().ok_or("No valid GBA ROM identity")?;
        let automatic = link_kind::link_kind(&identity.code, &identity.title,
            identity.clean_header);
        let wanted = link_kind::serial_option(self.link_mode, automatic,
            &identity.code, &identity.title);
        let previous = game.loaded_serial();
        if wanted == previous { return Ok(()); }

        let (storage, library) = PATHS.lock().unwrap_or_else(|e| e.into_inner())
            .clone().ok_or("Slot storage not mounted")?;
        let (rom, stem) = (game.rom_path().to_path_buf(), game.rom_stem().to_owned());
        let core_file = library.join("libgpsp_libretro.so");
        // An atomic private state+SRAM+RTC snapshot is required BEFORE the
        // existing core is dropped. The one-shot marker makes the reloaded
        // core prefer this state over a stale RetroArch auto-save.
        self.game.as_mut().ok_or("Game disappeared before serial switch")?
            .preserve_solo_for_link()?;
        drop(self.game.take());
        let resume = |serial| GameSession::open_gpsp_serial(
            &rom, &core_file, &storage, &stem, self.settings.colour_correction,
            self.ra_service.as_ref(), serial);
        let replacement = match resume(wanted) {
            Ok(game) => game,
            Err(error) => {
                // Reopen the last known working mode if the requested one
                // fails, retaining the pre-switch state and battery data.
                match resume(previous) {
                    Ok(original) => {
                        SAMPLE_RATE.store(original.sample_rate, Ordering::Release);
                        self.game = Some(original);
                        crate::game::log_launch_timing(&format!(
                            "Cannot activate gpSP {wanted}: {error}; previous mode restored"));
                        return Err("Couldn't switch the link mode. Your game is unchanged.".into());
                    }
                    Err(recovery) => {
                        SAMPLE_RATE.store(0, Ordering::Release);
                        PLAYING.store(false, Ordering::Release);
                        self.handle(Input::Exit);
                        crate::game::log_launch_timing(&format!(
                            "gpSP Link mode failed: {error}; recovery requires cartridge relaunch ({recovery})"));
                        return Err("Couldn't switch the link mode. Reinsert the cartridge; your saves are safe.".into());
                    }
                }
            }
        };
        SAMPLE_RATE.store(replacement.sample_rate, Ordering::Release);
        AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
        clear_rumble();
        self.game_accum = 0.0;
        self.buttons = 0;
        self.game = Some(replacement);
        crate::game::log_launch_timing(&format!(
            "gpSP serial reload: {previous} -> {wanted} (upstream Link hardware selection)"));
        Ok(())
    }

    fn request_multiplayer_network(&mut self) {
        if !matches!(self.overlay, ShelfOverlay::Multiplayer) { return; }
        if matches!(self.link_phase, LinkMenu::Working {..} | LinkMenu::Linked {..}) { return; }
        if let Err(error) = self.ensure_gpsp_link_serial() {
            if self.game.is_some() {
                self.link_worked_at = self.link_millis();
                self.fail_multiplayer(&error, LinkFail::Radio);
            } else {
                set_message(error);
            }
            return;
        }
        self.cancel_link_discovery();
        let now = self.link_millis();
        self.link_worked_at = now;
        self.link_phase = LinkMenu::Working {
            role: self.link_role, step: LinkStep::Radio, since: now,
        };
        LINK_NEEDS_MULTICAST.store(true, Ordering::Release);
        UI_ACTION.lock().unwrap_or_else(|e| e.into_inner()).push_back(16);
    }

    /// A Lab session must negotiate exact backend, mode, ROM revision,
    /// compiled core and BIOS before touching the current solo timeline.
    /// The worker hashes all files; the GL/render thread only passes paths.
    fn link_config(&self) -> Result<crate::multiplayer::LinkConfig, String> {
        let session = self.game.as_ref().ok_or("No running game")?;
        let identity = self.link_identity.as_ref().ok_or("No Link identity")?;
        let (storage, library) = PATHS.lock().unwrap_or_else(|e| e.into_inner())
            .clone().ok_or("Slot storage not mounted")?;
        let kind = session.core_kind();
        let backend = match kind {
            Core::Mgba => crate::multiplayer::Backend::MgbaSlotCable,
            Core::Gpsp => crate::multiplayer::Backend::GpspNetpacket,
            Core::Gambatte => crate::multiplayer::Backend::GambatteGameLink,
            other if other.netplay() => crate::multiplayer::Backend::HomeNetplay,
            other => return Err(format!("{} has no Multiplayer", other.text())),
        };
        Ok(crate::multiplayer::LinkConfig {
            game: identity.code.clone(),
            rom: session.rom_path().to_path_buf(),
            core_file: library.join(format!("lib{}_libretro.so", kind.as_str())),
            // A Game Boy link starts from the two consoles' exchanged states,
            // so the boot ROM never runs and cannot tell the two sides apart.
            bios_file: if self.current_cart_platform() == Platform::Gba {
                storage.join("BIOS/gba_bios.bin")
            } else { PathBuf::new() },
            backend,
            mode: self.link_mode,
            trusted_dir: storage.join("TrustedLinkDevices"),
        })
    }

    fn fail_multiplayer(&mut self, reason: &str, failure: LinkFail) {
        self.cancel_link_discovery();
        // No link has been established: never display a false connected icon.
        self.hud.set_link(LinkBadge::Off);
        self.link_phase = LinkMenu::Failed {
            role: self.link_role, fail: failure,
            worked: self.link_worked_at, since: self.link_millis(),
        };
        set_message(reason.into());
    }

    fn begin_link_session(&mut self, role: LinkRole,
                          wire: crate::link_net::TcpLink,
                          peer_rom: Option<PathBuf>,
                          peer_bios: crate::link_net::PeerBios) -> Result<(), String> {
        let game = self.game.as_ref().ok_or("No game active")?;
        if game.core_kind().netplay() {
            self.game.as_mut().ok_or("No game active")?
                .begin_home_netplay(role, Box::new(wire))?;
            self.reset_time_controls();
            return Ok(());
        }
        if game.core_kind() == Core::Gambatte {
            // Join dials the Host's Game Link server; the Host listens.
            let host = match role {
                LinkRole::Host => Ipv4Addr::UNSPECIFIED,
                LinkRole::Join => wire.peer_ipv4()
                    .ok_or("Couldn't read the other console's address")?,
            };
            self.game.as_mut().ok_or("No game active")?
                .begin_game_link(role, host, Box::new(wire))?;
            self.reset_time_controls();
            return Ok(());
        }
        if game.core_kind() == Core::Gpsp {
            if game.linked() || game.is_link_sandbox() {
                return Err("gpSP Link requires a fresh private session".into());
            }
            let (storage, library) = PATHS.lock().unwrap_or_else(|e| e.into_inner())
                .clone().ok_or("Slot storage is not mounted")?;
            let (rom, stem, serial) = (game.rom_path().to_path_buf(),
                game.rom_stem().to_owned(), game.loaded_serial());
            let core_file = library.join("libgpsp_libretro.so");
            // Close solo only AFTER its exact state/SRAM/RTC are durable.
            // A live gpSP core keeps the solo libretro GET_SAVE_DIRECTORY;
            // redirecting Slot's own save paths is insufficient isolation.
            let solo_snapshot = self.game.as_mut().ok_or("No game active")?
                .preserve_solo_for_link()?;
            drop(self.game.take());
            let mut private = match GameSession::open_gpsp_linked(
                &rom, &core_file, &storage, &stem,
                self.settings.colour_correction, role, &solo_snapshot, serial) {
                Ok(game) => game,
                Err(error) => {
                    let recovery = self.restore_gpsp_solo_after_link_failure(
                        &rom, &core_file, &storage, &stem, serial, &solo_snapshot);
                    return Err(link_boot_failure("Cannot initialize private gpSP Link",
                        &error, recovery));
                }
            };
            if let Err(error) = private.begin_multiplayer(role, self.link_mode, Box::new(wire)) {
                drop(private);
                let recovery = self.restore_gpsp_solo_after_link_failure(
                    &rom, &core_file, &storage, &stem, serial, &solo_snapshot);
                return Err(link_boot_failure("Cannot begin private gpSP Link",
                    &error, recovery));
            }
            SAMPLE_RATE.store(private.sample_rate, Ordering::Release);
            AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
            clear_rumble();
            self.game_accum = 0.0;
            self.buttons = 0;
            self.game = Some(private);
            return Ok(());
        }
        if self.link_mode != LinkKind::Cable || game.linked() || game.is_link_sandbox() {
            return Err("mGBA Link requires a fresh Cable session".into());
        }
        let (storage, library) = PATHS.lock().unwrap_or_else(|e| e.into_inner())
            .clone().ok_or("Slot storage is not mounted".to_owned())?;
        let rom = game.rom_path().to_path_buf();
        let stem = game.rom_stem().to_owned();
        let core_file = library.join("libmgba_libretro.so");
        let platform = self.current_cart_platform();
        let (palette, colour) = self.solo_core_look(platform);
        // Save the current solo state/ram/RTC atomically BEFORE replacing
        // the core. Failure stops link without losing the solo instance.
        let solo_snapshot = self.game.as_mut()
            .ok_or("No game active".to_owned())?
            .preserve_solo_for_link()?;
        // The exact private solo snapshot is passed in-memory; the linked
        // engine loads it as both GBA halves before Host/Join state exchange.
        // The existing solo core remains alive until its checkpoint is durable.
        drop(self.game.take());
        let mut linked = match GameSession::open_mgba_linked(&rom, &core_file,
            &storage, platform, &stem, palette.as_deref(), colour, role,
            &solo_snapshot, peer_rom.as_deref(), &peer_bios) {
            Ok(session) => session,
            Err(error) => {
                let recovery = self.restore_solo_after_link_failure(
                    &rom, &core_file, &storage, &stem, &solo_snapshot);
                return Err(link_boot_failure("Cannot initialize mGBA Link",
                    &error, recovery));
            }
        };
        if let Err(error) = linked.begin_multiplayer(role, LinkKind::Cable, Box::new(wire)) {
            drop(linked);
            let recovery = self.restore_solo_after_link_failure(
                &rom, &core_file, &storage, &stem, &solo_snapshot);
            return Err(link_boot_failure("Cannot begin mGBA Link",
                &error, recovery));
        }
        SAMPLE_RATE.store(linked.sample_rate, Ordering::Release);
        AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
        self.game_accum = 0.0;
        self.buttons = 0;
        self.game = Some(linked);
        Ok(())
    }

    /// An explicit unplug is a core transition, not a cartridge ejection.
    /// Capture the live LOCAL player's GBA, destroy its private Link core,
    /// verify a fresh non-Link emulator before modifying any solo save,
    /// and promote an immutable, conflict-checked battery/state generation.
    fn return_link_to_solo(&mut self) -> Result<(), String> {
        let linked = self.game.as_mut().ok_or("No Link game to unplug")?;
        let which = linked.core_kind();
        if which == Core::Gambatte || linked.home_netplay() {
            // Game Link never left the solo game: just pull the cable.
            // Home netplay: Host keeps playing, Join gets its own game back.
            linked.end_multiplayer();
            if linked.requires_relaunch() {
                // Join's own game could not be put back: the session stays
                // stopped and unsaved until the cartridge is reinserted.
                return Err("Join’s solo state could not be restored. Eject and reinsert the cartridge.".into());
            }
            self.game_accum = 0.0;
            self.buttons = 0;
            return Ok(());
        }
        let serial = linked.loaded_serial();
        let rom = linked.rom_path().to_path_buf();
        let stem = linked.rom_stem().to_owned();
        let rejoin = linked.prepare_solo_rejoin()?;
        let (storage, library) = PATHS.lock().unwrap_or_else(|e| e.into_inner())
            .clone().ok_or("Slot storage not configured")?;
        // libretro cores cannot safely have two independent active instances
        // in this single process. Store the complete private snapshot before
        // dropping the Link instance, never write to solo until verified.
        linked.end_multiplayer();
        drop(self.game.take());
        let dylib = library.join(format!("lib{}_libretro.so", which.as_str()));
        let platform = self.current_cart_platform();
        let (palette, colour) = self.solo_core_look(platform);
        let mut solo = match which {
            Core::Mgba => GameSession::open(&rom, &dylib, &storage,
                which, platform, false, &stem, palette.as_deref(),
                colour, self.ra_service.as_ref()),
            Core::Gpsp => GameSession::open_gpsp_solo_unresumed(
                &rom, &dylib, &storage, &stem,
                self.settings.colour_correction, self.ra_service.as_ref(), serial),
            other => Err(format!("{} never links", other.text())),
        }?;
        solo.restore_local_after_link(&rejoin)?;
        let backup = solo.commit_rejoined_solo(&rejoin)?;
        crate::game::log_launch_timing(&format!(
            "Link -> SOLO validated for {} in {}, immutable backup={}",
            which.text(), stem, backup.display()));
        SAMPLE_RATE.store(solo.sample_rate, Ordering::Release);
        AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
        clear_rumble();
        self.game_accum = 0.0;
        self.buttons = 0;
        self.reset_time_controls();
        self.game = Some(solo);
        // Kotlin may now export current SOLO SRAM and the newly regenerated
        // compatible RetroArch auto-state. It must not export private Link
        // session files, nor mix FireRed with LeafGreen.
        if let Some(uri) = self.requested_uri.as_deref() {
            let event = serde_json::json!({
                "uri": uri, "core": which.as_str(),
            }).to_string();
            SAVE_SYNC.lock().unwrap_or_else(|e| e.into_inner()).push_back(event);
        }
        Ok(())
    }

    /// Game Boy palette and core colour correction for an mGBA core opened
    /// around Link, as the cartridge was launched. GBA keeps its own rule.
    fn solo_core_look(&self, platform: Platform) -> (Option<String>, bool) {
        if platform == Platform::Gba {
            return (None, self.settings.colour_correction);
        }
        let settings = self.game_settings();
        (settings.gb_palettes.then(|| settings.palette_name().to_owned()),
         settings.shader_for(platform).core_colour(settings.colour_for(platform)))
    }

    fn restore_gpsp_solo_after_link_failure(&mut self, rom: &Path,
                                             core_file: &Path, storage: &Path,
                                             stem: &str, serial: &'static str,
                                             snapshot: &[u8]) -> Result<(), String> {
        // A successful core open is NOT evidence of a successful state
        // resume: libretro can reject the saved state and start at the title.
        // Re-apply the in-memory checkpoint and fail closed if it is rejected.
        let result = (|| {
            let mut game = GameSession::open_gpsp_serial(rom, core_file, storage, stem,
                self.settings.colour_correction, self.ra_service.as_ref(), serial)?;
            game.restore_checkpoint_after_link_failure(snapshot)?;
            SAMPLE_RATE.store(game.sample_rate, Ordering::Release);
            self.game = Some(game);
            Ok(())
        })();
        if result.is_err() {
            SAMPLE_RATE.store(0, Ordering::Release);
            PLAYING.store(false, Ordering::Release);
        }
        result
    }

    fn restore_solo_after_link_failure(&mut self, rom: &Path,
                                       core_file: &Path, storage: &Path,
                                       stem: &str, snapshot: &[u8]) -> Result<(), String> {
        let platform = self.current_cart_platform();
        let (palette, colour) = self.solo_core_look(platform);
        let result = (|| {
            let mut game = GameSession::open(rom, core_file, storage,
                Core::Mgba, platform, true, stem, palette.as_deref(),
                colour, self.ra_service.as_ref())?;
            game.restore_checkpoint_after_link_failure(snapshot)?;
            SAMPLE_RATE.store(game.sample_rate, Ordering::Release);
            self.game = Some(game);
            Ok(())
        })();
        if result.is_err() {
            SAMPLE_RATE.store(0, Ordering::Release);
            PLAYING.store(false, Ordering::Release);
        }
        result
    }

    fn poll_multiplayer(&mut self) {
        if !matches!(self.overlay, ShelfOverlay::Multiplayer) { return; }
        self.prepare_link_art();
        self.refresh_internet_heading();
        let internet = self.link_inet.session;
        let event = self.link_starter.as_mut().and_then(LinkStarter::poll);
        if let Some(event) = event {
            match event {
                LinkProgress::RoomCode(code) => {
                    // The heading shows the code; Y Share is in the legend.
                    self.link_inet.room = Some(code);
                    self.refresh_internet_heading();
                }
                LinkProgress::Looking => {
                    self.link_phase = LinkMenu::Working {
                        role: self.link_role, step: LinkStep::Waiting,
                        since: self.link_worked_at,
                    };
                }
                LinkProgress::PeerRomConsent(label) => {
                    *LINK_PEER_LABEL.lock().unwrap_or_else(|e| e.into_inner()) = Some(label);
                    // Kotlin displays a Slot-styled choice and returns the
                    // decision asynchronously; GL stays nonblocking.
                    UI_ACTION.lock().unwrap_or_else(|e| e.into_inner()).push_back(18);
                }
                LinkProgress::Ready(wire, peer_rom, peer_bios) => {
                    self.cancel_link_discovery();
                    crate::link_play::set_internet_profile(internet);
                    let role = if self.link_role == LinkRow::Host {
                        LinkRole::Host
                    } else { LinkRole::Join };
                    let joined = self.begin_link_session(role, wire, peer_rom, peer_bios);
                    match joined {
                        Ok(()) => {
                            // Host purple / Join grey, permanently shown in
                            // the exact upstream top-right in-game HUD.
                            self.hud.set_link(link_badge_for(self.link_role, false));
                            self.link_phase = LinkMenu::Linked {
                                role: self.link_role,
                                worked: self.link_worked_at,
                                since: self.link_millis(), opened: false,
                            };
                        }
                        Err(reason) => self.fail_multiplayer(&reason, LinkFail::PeerVanished),
                    }
                }
                LinkProgress::Failed(reason) => {
                    let fail = match reason {
                        crate::multiplayer::Failure::NoNetwork => LinkFail::Radio,
                        crate::multiplayer::Failure::NobodyCame => LinkFail::NobodyCame,
                        crate::multiplayer::Failure::IncompatiblePeer => LinkFail::PeerVanished,
                        crate::multiplayer::Failure::Cancelled => LinkFail::Cancelled,
                        crate::multiplayer::Failure::PeerVanished => LinkFail::PeerVanished,
                        crate::multiplayer::Failure::WrongCode => LinkFail::WrongCode,
                        crate::multiplayer::Failure::NoRelay => LinkFail::Radio,
                    };
                    let details = if reason == crate::multiplayer::Failure::IncompatiblePeer {
                        "The other console refused. Use the same game, core and slot. version."
                    } else if internet {
                        match reason {
                            crate::multiplayer::Failure::WrongCode => "No room has this code. Check it with your friend.",
                            crate::multiplayer::Failure::NoRelay => "Couldn't reach the slot. WAN server. Check your connection.",
                            crate::multiplayer::Failure::NobodyCame => "Nobody joined in time. Open a new room and share the code again.",
                            crate::multiplayer::Failure::Cancelled => "Connection cancelled.",
                            _ => "Connection lost with the other console.",
                        }
                    } else {
                        match reason {
                            crate::multiplayer::Failure::NobodyCame => "No host found. Make sure it's waiting on the same Wi-Fi.",
                            crate::multiplayer::Failure::NoNetwork => "Connect to Wi-Fi to play Multiplayer.",
                            crate::multiplayer::Failure::Cancelled => "Connection cancelled.",
                            _ => "Connection lost with the other console.",
                        }
                    };
                    self.fail_multiplayer(details, fail);
                }
            }
        }
        if link_menu_auto_closes(self.link_phase, self.link_millis()) {
            self.overlay = ShelfOverlay::None;
            self.game_accum = 0.0;
            self.buttons = 0;
        }
        if link_unplug_complete(self.link_phase, self.link_millis()) {
            // A successfully rebuilt solo cartridge resumes *in place*.
            // A failed conversion remains in the fail-closed Eject menu.
            // A cancelled search or a failure returns to the Host/Join
            // picker (same role, same Local/Internet tab) to try again;
            // only B on the picker leaves Multiplayer.
            let relaunch = self.game.as_ref().is_some_and(GameSession::requires_relaunch);
            let seated = matches!(self.link_phase, LinkMenu::Unplug { from: ReleaseFrom::Seated, .. });
            self.overlay = if relaunch { ShelfOverlay::GameMenu { row: 0 } }
                else if seated { ShelfOverlay::None }
                else {
                    self.link_phase = LinkMenu::Pick(self.link_role);
                    self.game_accum = 0.0;
                    self.buttons = 0;
                    ShelfOverlay::Multiplayer
                };
        }
    }

    fn show_unplug(&mut self, from: ReleaseFrom) {
        self.link_phase = LinkMenu::Unplug { role: self.link_role, since: self.link_millis(), from };
        self.overlay = ShelfOverlay::Multiplayer;
        self.game_accum = 0.0;
        self.buttons = 0;
    }

    fn dismiss_link(&mut self) {
        self.cancel_link_discovery();
        match self.link_phase {
            LinkMenu::Working { since, .. } => self.show_unplug(ReleaseFrom::Search { worked: since }),
            LinkMenu::Failed { worked, since, .. } => self.show_unplug(ReleaseFrom::Failure { worked, failed: since }),
            LinkMenu::Unplug { .. } => {}
            _ => self.overlay = self.link_entry.back(
                self.game.as_ref().is_some_and(GameSession::linked),
                self.game.as_ref().is_some_and(GameSession::requires_relaunch)),
        }
    }

    fn link_recovery_failed(&mut self, message: &str, reason: &str) {
        if let Some(game) = self.game.as_mut() {
            game.end_multiplayer();
            self.hud.set_link(link_badge_for(self.link_role, true));
            self.link_entry = LinkEntry::Stopped;
            self.link_phase = LinkMenu::Failed { role: self.link_role, fail: LinkFail::PeerVanished,
                worked: self.link_worked_at, since: self.link_millis() };
            self.overlay = ShelfOverlay::Multiplayer;
        } else { self.handle(Input::Exit); }
        set_error_message(message, &reason);
    }

    fn peer_link_stopped(&mut self, fault: crate::link_play::LinkError) {
        use crate::link_play::LinkError::{Disconnected, PeerEnded};
        self.ff_held = false;
        self.ff_latched = false;
        self.rewind_held = false;
        self.hud.set_ff(FfState::Off);
        AUDIO_SPEED_PERMILLE.store(1000, Ordering::Release);
        if matches!(fault, PeerEnded | Disconnected) {
            match self.return_link_to_solo() {
                Ok(()) => {
                    self.hud.set_link(LinkBadge::Off);
                    self.show_unplug(ReleaseFrom::Seated);
                }
                Err(reason) => self.link_recovery_failed("Connection lost with the other console.", &reason),
            }
        } else {
            let now = self.link_millis();
            self.hud.toast(link_fault_toast(fault), now);
            self.link_recovery_failed("Multiplayer paused. Check the connection and try again.", &format!("{fault:?}"));
        }
    }

    fn handle(&mut self, input: Input) {
        match input {
            Input::LinkPeerRomConsent(choice) => {
                if let Some(starter) = self.link_starter.as_ref() {
                    let decision = match choice {
                        1 => crate::trusted_link::PeerPermission::AllowOnce,
                        2 => crate::trusted_link::PeerPermission::Remember,
                        _ => crate::trusted_link::PeerPermission::Reject,
                    };
                    starter.approve_peer_rom(decision);
                    if choice != 1 && choice != 2 {
                        self.cancel_link_discovery();
                        self.fail_multiplayer("Link cancelled. The cartridge wasn't shared.",
                            LinkFail::Cancelled);
                    }
                }
            }
            Input::LinkNetworkReady(address) => {
                if matches!(self.overlay, ShelfOverlay::Multiplayer) &&
                    matches!(self.link_phase, LinkMenu::Working {..}) {
                    let start = address.parse::<Ipv4Addr>()
                        .map_err(|_| "Couldn't read this console's Wi-Fi address.".to_string())
                        .and_then(|ip| {
                            let config = self.link_config()?;
                            let role = if self.link_role == LinkRow::Host {
                                LinkRole::Host
                            } else { LinkRole::Join };
                            LinkStarter::spawn(role, ip, config).map_err(|e| e.to_string())
                        });
                    match start {
                        Ok(starter) => self.link_starter = Some(starter),
                        Err(e) => self.fail_multiplayer(&e, LinkFail::Radio),
                    }
                } else {
                    LINK_NEEDS_MULTICAST.store(false, Ordering::Release);
                }
            }
            Input::LinkNetworkError(reason) => {
                if matches!(self.overlay, ShelfOverlay::Multiplayer) &&
                    matches!(self.link_phase, LinkMenu::Working {..}) {
                    self.fail_multiplayer(&reason, LinkFail::Radio);
                }
            }
            Input::RaAccountConfigured => {
                // A newly authenticated account may start a fresh worker.
                self.ra_signing_out = false;
                self.ensure_ra_service();
            }
            Input::RaPrefetchHash(hash, console) => {
                // In-flight Android SAF scans from the old account must not
                // relaunch native networking during a hot logout.
                if !self.ra_signing_out {
                    self.ensure_ra_service();
                    if let Some(service) = self.ra_service.as_ref() {
                        service.prefetch_rom_hash_for_console(&hash, console);
                    }
                }
            }
            Input::RaNetworkAvailable => {
                if let Some(service) = self.ra_service.as_ref() {
                    service.network_available();
                }
            }
            Input::ReloadVisuals => {
                // Triggered from Android SAF, applied on the GL thread only.
                let root = PATHS.lock().unwrap_or_else(|e| e.into_inner())
                    .as_ref().map(|(root, _)| root.clone());
                if let Some(root) = root {
                    slot_ui::set_theme(slot_store::Theme::read(&root));
                    self.refresh_menu_pill();
                    let custom_colours=root.join("Config/theme.txt").is_file();
                    self.slot_faces=if custom_colours { None }
                        else if self.settings.appearance == crate::settings::Appearance::Light {
                            Some(self.slot_light_faces)
                        } else { Some(self.slot_dark_faces) };
                    self.custom_wallpaper_present = root.join("Wallpapers/user.png").is_file();
                    // A freshly imported user.png shows at once, whatever was
                    // picked, None included (Siham 2026-11-09). An image
                    // imported for a shelf becomes that shelf's wallpaper.
                    let stamp = user_wallpaper_stamp(&root);
                    if let Some(shelf) = self.wallpaper_import.take() {
                        if let Some(file) = crate::wallpaper::for_shelf(&root, shelf) {
                            self.settings.set_shelf_wallpaper(shelf,
                                Some(crate::settings::WallpaperPick::File(file)));
                            self.persist_settings();
                        }
                    } else if stamp.is_some() && stamp != self.user_wallpaper_stamp {
                        self.settings.wallpaper =
                            crate::settings::WallpaperPick::File("user.png".into());
                        self.persist_settings();
                    }
                    self.user_wallpaper_stamp = stamp;
                    // A deleted custom image falls back to Default only if
                    // it was the current selection.
                    if let crate::settings::WallpaperPick::File(name) = &self.settings.wallpaper {
                        if !crate::wallpaper::list(&root).contains(name) {
                            self.settings.wallpaper = crate::settings::WallpaperPick::Default;
                            self.persist_settings();
                        }
                    }
                    // A shelf whose image is gone shows General's again.
                    let gone: Vec<String> = self.settings.wallpaper_shelves.iter()
                        .filter(|(_, pick)| matches!(pick, crate::settings::WallpaperPick::File(name)
                            if !root.join("Wallpapers").join(name).is_file()))
                        .map(|(key, _)| key.clone()).collect();
                    if !gone.is_empty() {
                        for key in gone { self.settings.wallpaper_shelves.remove(&key); }
                        self.persist_settings();
                    }
                    self.reload_wallpaper(Some(&root));
                    self.refresh_wallpaper_name();
                    self.refresh_wallpaper_edit_face();
                }
            }
            Input::StudioArt { uri, real, logo, kind, name } => self.studio_art(uri, real, logo, kind, name),
            Input::ReloadCartLabel { uri } => if !self.relabel.contains(&uri) { self.relabel.push_back(uri) },
            Input::RommCatalog(entries) => self.romm_catalog(entries),
            Input::RommStatus(kind, text) => self.romm_status(kind, text),
            Input::RommLabel(id) => self.romm_label(id),
            Input::RommHeader(entry) => self.romm_header(entry),
            Input::RommProgress(done, total, percent) => self.romm_progress(done, total, percent),
            Input::RommDownloaded(id, ok) => self.romm_downloaded(id, ok),
            Input::GameDeleted(uri, ok) => self.game_deleted(uri, ok),
            Input::RommFilter(filter) => self.romm_filter(filter),
            Input::ExternalClose => {
                // A ROM, BIOS or slot. folder chosen from Library: the shelf,
                // as before 2.4, so its scan shows (Siham 2026-11-16). A
                // cancelled picker keeps Library and its row (ExternalReturn).
                // Also after a picker slot. opened by itself (the ROM folder
                // a slot folder names), with nothing pending.
                self.external_pending = false;
                self.menu_held = None;
                self.menu_seen = 0;
                if matches!(self.overlay, ShelfOverlay::Library { .. }) {
                    self.overlay = ShelfOverlay::None;
                }
            }
            Input::ExternalReturn => {
                if self.external_pending {
                    self.external_pending = false;
                    self.menu_held = None;
                    self.menu_seen = 0;
                    // Keep the original Menu and selected row when coming back
                    // from Cart Studio, Apps or the RetroAchievements panel.
                }
            }
            Input::Reset => {
                self.reset_time_controls();
                self.shelves_grid.reset();
                self.menu_held = None;
                self.buttons = 0;
                // Android won't tell the releases of keys held while away.
                self.buttons2 = 0;
                self.unseen2 = 0;
                self.held_releases2 = 0;
                self.controls_listen = None;
                self.hk_mod_down = false;
                self.hk_releases.clear();
                self.menu_reveal_at = None;
                self.mode_down_at = None;
                self.mode_last_tap = None;
                self.home_launch_held = false;
                self.select_home_consumed = false;
                self.a_down_at = None;
                self.polaroids = None;
                // Android calls Reset onPause. Retain the *native menu* behind
                // Apps/Studio while the new Activity animates in, otherwise
                // the user briefly sees the cartridge carousel again. An
                // Internet host away sharing its code keeps its room screen.
                let sharing_room = matches!(self.overlay, ShelfOverlay::Multiplayer)
                    && self.link_inet.keeps_room_in_background(self.link_phase);
                // Library > RomM closed by Android (Home, another app): its
                // page is left as with B, so the games downloading go to
                // the shelf one by one (they waited for a page shut long ago).
                if !self.external_pending && matches!(self.overlay, ShelfOverlay::Romm) {
                    self.leave_romm(ShelfOverlay::None);
                }
                if !self.external_pending && !sharing_room {
                    self.overlay = if self.game.as_ref().is_some_and(GameSession::requires_relaunch) {
                        ShelfOverlay::GameMenu { row: 0 }
                    } else { ShelfOverlay::None };
                }
                self.picker = None;
                for shelf in &mut self.shelves {
                    shelf.release_hold();
                }
            }
            Input::Suspend if matches!(self.overlay, ShelfOverlay::Multiplayer)
                && self.link_inet.keeps_room_in_background(self.link_phase) => {
                // The host is away sending its room code (WhatsApp, SMS):
                // the relay connection lives on a worker thread, so keep
                // the room and pick up where it is when slot. returns.
                if let Some(session) = self.game.as_mut() {
                    let _ = session.save(true);
                }
                self.buttons = 0;
                AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
            }
            Input::Suspend => {
                self.cancel_link_discovery();
                if let Some(session) = self.game.as_mut() { session.end_multiplayer(); }
                self.hud.set_link(LinkBadge::Off);
                // Android can go to sleep while a linked game is fullscreen,
                // not just while its Multiplayer menu is visible. The peer
                // has now been disconnected and continuing in-game would
                // accidentally run a private Link core as SOLO. Always
                // resume through the safe Eject-to-solo menu in this case.
                if suspend_returns_to_eject_menu(
                    self.game.as_ref().is_some_and(GameSession::requires_relaunch),
                    matches!(self.overlay, ShelfOverlay::Multiplayer)) {
                    self.overlay = ShelfOverlay::GameMenu { row: 0 };
                    self.game_accum = 0.0;
                }
                if let Some(session) = self.game.as_mut() {
                    if session.save(true) && !session.is_link_sandbox() {
                        if let Some(uri) = self.requested_uri.as_deref() {
                            let core = session.core_kind().as_str();
                            let event = serde_json::json!({"uri": uri, "core": core}).to_string();
                            SAVE_SYNC.lock().unwrap_or_else(|e| e.into_inner()).push_back(event);
                        }
                    }
                }
                self.buttons = 0;
                AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
            }
            Input::SlotButtonReleased { held } => {
                let down = self.mode_down_at.take().is_some();
                self.mode_last_tap = None;
                self.home_launch_held = false;
                if held && down && self.game.is_some() && !self.exiting_screen {
                    crate::game::log_launch_timing("Exit requested by system long press");
                    self.handle(Input::Exit);
                }
            }
            Input::LidCover(closed, now) => {
                if closed == self.lid_covered { return; }
                self.lid_covered = closed;
                self.lid_title_started = None;
                // Animations are always on (Siham 2026-11-15).
                if now { self.lid_t = if closed { 1.0 } else { 0.0 }; }
                if self.game.is_some() { return; }
                if closed {
                    self.lid_ticks = 0;
                    self.lid_shelf_index = self.shelves.get(self.active).map(|s| (self.active, s.index));
                    // Drifting Shelf drifts on from the cart in view
                        // (Siham 2026-11-15); Random Cart draws one.
                    match self.settings.rotate_shelf_cover {
                        2 => self.lid_random_cart(),
                        1 => self.lid_last_played(),
                        _ => {}
                    }
                } else if let Some((active, index)) = self.lid_shelf_index.take() {
                    // Drifting Shelf, Last Played, Random Cart: opened on the
                    // cart shown while closed (Siham 2026-11-15). The clocks
                    // and the black screen give the cart left before.
                    if matches!(self.settings.rotate_shelf_cover, 0..=2) { return; }
                    if active < self.shelves.len() && !self.shelves[active].carts.is_empty() {
                        self.active = active;
                        if index < self.shelves[active].carts.len() { self.shelves[active].select(index); }
                    }
                }
            }
            Input::LidStep => {
                if self.lid_covered && self.game.is_none() && matches!(self.overlay, ShelfOverlay::None) {
                    match self.settings.rotate_shelf_cover {
                        0 => if let Some(shelf) = self.shelves.get_mut(self.active) { shelf.right(); },
                        2 => {
                            self.lid_ticks += 1;
                            // One a minute: ten of Android's 6 s steps.
                            if self.lid_ticks >= 10 { self.lid_ticks = 0; self.lid_random_cart(); }
                        }
                        _ => {}
                    }
                }
            }
            Input::Exit => {
                // A failed battery/RTC/auto-state write must not destroy the
                // only live emulator state. Keep gameplay and the save menu
                // available so a storage issue can be resolved and retried.
                let with_state = self.live_game_options().eject_save;
                let mut eject_save_took = None;
                let eject_saved = match self.game.as_mut() {
                    Some(session) if session.runahead_rollback_failed() => {
                        // The in-memory core may be from speculative time.
                        // First Exit must NEVER persist it; require a second
                        // explicit Exit to discard and reload from last disk
                        // checkpoint. This branch has NO SAVE_SYNC event.
                        if !session.confirm_rollback_discard() {
                            set_message("Paused to protect your save. Eject again to quit.".into());
                            self.overlay = ShelfOverlay::GameMenu { row: 0 };
                            self.game_accum = 0.0;
                            return;
                        }
                        crate::game::log_launch_timing(
                            "Unsafe Run-Ahead core discarded WITHOUT SRAM/state/RetroArch export");
                        false
                    }
                    Some(session) => match {
                        let save_at = Instant::now();
                        let saved = session.save_for_eject_in_background(with_state);
                        eject_save_took = Some(save_at.elapsed());
                        saved
                    } {
                        Ok(saved) => {
                            if session.take_kept_flash_save() {
                                set_message("In-game save kept: this state has none yet.".into());
                            }
                            saved
                        }
                        Err(error) => {
                            crate::game::log_launch_timing(&format!(
                                "Save & Eject cancelled; core preserved: {error}"));
                            set_error_message("Couldn't save. Try Save and Eject again.", &error);
                            self.overlay = ShelfOverlay::GameMenu { row: 0 };
                            self.game_accum = 0.0;
                            return;
                        }
                    },
                    None => false,
                };
                self.hud.set_link(LinkBadge::Off);
                self.cancel_link_discovery();
                self.link_identity = None;
                self.reset_time_controls();
                self.pending_game = None; self.screen_ready = None;
                // Preserve the final video texture for the original 160ms CRT
                // power-off before the mechanical 450ms cartridge ejection.
                if self.game.is_some() {
                    self.exiting_screen = true;
                    crate::frame_watch::eject_started(eject_save_took);
                    self.eject_hold_remaining = EJECT_HOLD_S;
                    // The next input is most likely a scroll: keep the shelf
                    // at full rate (no 4 Hz idle) for 3.2 s after the eject.
                    self.last_cart_navigation = Instant::now();
                }
                // The reversed cart animation will emit the original eject sound.
                if self.progress > 0.0 || self.inserted { self.eject_sound_armed = true; }
                if let Some(session) = self.game.take() {
                    // A durable checkpoint was verified BEFORE taking/drop-
                    // ping the session. Never report a false success from a
                    // battery error masked by an unrelated RTC/state write.
                    let event = (eject_saved && !session.is_link_sandbox()).then(|| {
                        self.requested_uri.as_deref().map(|uri| serde_json::json!(
                            {"uri": uri, "core": session.core_kind().as_str()}).to_string())
                    }).flatten();
                    // Unloaded first: a core that writes its own save as the
                    // game closes (fake-08's cartdata) has written it when the
                    // save is copied on.
                    let mut session = session;
                    let parts = session.take_save_parts();
                    let drop_at = Instant::now();
                    drop(session);
                    crate::frame_watch::eject_detail(format!("{parts}{}core closed {}ms",
                        if parts.is_empty() { "" } else { ", " }, drop_at.elapsed().as_millis()));
                    // Once the state is on disk (a big one is written off
                    // this thread: the eject doesn't wait for it).
                    crate::game::after_state_writes(move |written| match written {
                        Ok(()) => if let Some(event) = event {
                            SAVE_SYNC.lock().unwrap_or_else(|e| e.into_inner()).push_back(event);
                        },
                        Err(error) => {
                            crate::game::log_launch_timing(&format!("Save & Eject's state not written: {error}"));
                            set_message("Couldn't save the game's state.".into());
                        }
                    });
                }
                PLAYING.store(false, Ordering::Release);
                SAMPLE_RATE.store(0, Ordering::Release);
                AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
                self.buttons = 0;
                self.awaiting_game = false;
                self.inserted = false;
                self.overlay = ShelfOverlay::None;
                self.picker = None;
                self.a_down_at = None;
                self.home_launch_held = false;
                self.fresh_launch = false;
                self.mode_down_at = None;
                self.mode_last_tap = None;
                self.select_home_consumed = false;
                self.polaroids = None;
                self.requested_uri = None;
                self.prepared_rom = None;
                self.link_identity = None;
                self.link_mode_override = None;
                self.game_emulation = None;
                *REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
            Input::UpdateProgress(percent) => {
                if self.update_status == 3 {
                    self.set_download_text(&format!("DOWNLOADING {}%", percent.min(100)));
                }
            }
            Input::UpdateStatus(status, installed, available, automatic) => {
                // A new download starts without a percentage until the first.
                if status == 3 && self.update_status != 3 { self.set_download_text(MENU_TEXT[114]); }
                self.update_status = status.min(5);
                self.update_auto = automatic;
                // Two selection rows regardless of update status.
                for (label, is_installed) in [(installed, true), (available, false)] {
                    if label.is_empty() || !label.is_ascii() || label.len() > 48 {
                        continue;
                    }
                    let face = quick_value_face(&label, true);
                    let existing = if is_installed { self.update_installed_face }
                        else { self.update_version_face };
                    let tex = match existing {
                        Some((tex,_,_)) => {
                            self.gpu.update_face(tex, &face);
                            tex
                        }
                        None => self.gpu.create_face(&face),
                    };
                    if is_installed {
                        self.update_installed_face = Some((tex,face.w,face.h));
                    } else {
                        self.update_version_face = Some((tex,face.w,face.h));
                    }
                }
            }
            Input::ShelfStatus(code) => {
                self.shelf_status = if usize::from(code) < SHELF_STATUS_TEXT.len() {
                    code
                } else { 0 };
                if status_has_bar(self.shelf_status) { self.set_scan_progress(0, 0); }
            }
            Input::ScanProgress(done, total) => self.set_scan_progress(done, total),
            Input::GameError { uri, message } => {
                if !self.inserted || !self.awaiting_game
                    || self.requested_uri.as_deref() != Some(uri.as_str())
                {
                    return;
                }
                // "say:" a sentence for the player as is; "quiet:" the player
                // chose not to play (B in a window): no message (beta38).
                if let Some(text) = message.strip_prefix("say:") {
                    crate::game::log_launch_timing(&format!("Launch held: {text}"));
                    set_message(text.to_string());
                } else if let Some(why) = message.strip_prefix("quiet:") {
                    crate::game::log_launch_timing(&format!("Launch cancelled: {why}"));
                } else {
                    set_error_message(game_error_text(&message), &message);
                }
                crate::frame_watch::cancel();
                self.awaiting_game = false;
                self.inserted = false;
                self.requested_uri = None;
                self.prepared_rom = None;
            }
            Input::GameReady { uri, path } => {
                // ROM preparation is asynchronous: a previously cancelled cart must
                // never launch after selecting another cartridge.
                if self.awaiting_game && self.inserted
                    && self.requested_uri.as_deref() == Some(uri.as_str())
                {
                    self.awaiting_game = false;
                    self.prepared_rom = Some(path);
                    crate::frame_watch::rom_ready();
                }
            }
            Input::PadKey { device, code, pressed, stick } =>
                self.pad_key(device, code, pressed, stick),
            Input::Controllers(devices) => self.controllers_changed(devices),
            Input::Touch(action, x, y, ms) => self.touch(action, x, y, ms),
            Input::Key(code, pressed) => {
                // A Map or Detect waits for a controller's button (pad_key);
                // the slot. button cancels it.
                if self.controls_listen.is_some() {
                    if pressed && code == crate::controls::MODE { self.controls_listen = None; }
                    return;
                }
                // A key during the double-tap wait shows the game menu at once.
                if pressed && code != crate::controls::MODE && self.menu_reveal_at.take().is_some() {
                    self.menu_seen = 0;
                }
                // Changed hotkeys have their own dispatcher; untouched ones
                // run through slot.'s original gestures below.
                if self.game.is_some() && !self.controls.hotkeys.is_default()
                    && self.custom_hotkey(code, pressed) { return; }
                if code == 109 { self.select_physical = pressed; }
                if self.select_slot_combo(code, pressed) { return; }
                if matches!(code, 19 | 20) {
                    if !pressed {
                        if self.menu_held.is_some_and(|(held,_,_)|held==code) {
                            self.menu_held=None;
                        }
                    } else if !self.menu_repeat_dispatch
                        && !matches!(self.overlay,ShelfOverlay::None)
                    {
                        self.menu_held=Some((code,self.born.elapsed().as_millis() as u64+400,0));
                    }
                } else if pressed && matches!(code,96|97|108) {
                    self.menu_held=None;
                }
                if code == 110 {
                    match select_home_action(
                        pressed,
                        self.select_down_at.is_some(),
                        self.game.is_some(),
                        matches!(self.overlay, ShelfOverlay::None | ShelfOverlay::GameMenu { .. }),
                        &mut self.select_home_consumed,
                    ) {
                        SelectHomeAction::Swallow => return,
                        SelectHomeAction::OpenLink => {
                            self.open_multiplayer_shortcut();
                            return;
                        }
                        SelectHomeAction::Pass => {}
                    }
                }
                if code == 110 && matches!(self.overlay, ShelfOverlay::Multiplayer) {
                    if !pressed {
                        if matches!(self.link_phase, LinkMenu::Linked { opened: true, .. }) {
                            self.overlay = ShelfOverlay::None;
                        } else if !matches!(self.link_phase, LinkMenu::Unplug { .. }) {
                            self.dismiss_link();
                        }
                    }
                    return;
                }
                // KONKR's top-round controller key emits Linux BTN_MODE. Android
                // normally delivers that as KEYCODE_BUTTON_MODE (110); this is
                // separate from the system HOME launcher key.
                if code == 110 {
                    // On the carousel ONLY, Home/MODE mirrors A's insert and
                    // 400ms fresh-launch logic. Consumed releases never enter
                    // the game menu or the system launcher.
                    if self.home_launch_held {
                        if !pressed {
                            self.home_launch_held = false;
                            if self.game.is_none() {
                                self.handle(Input::Key(96, false));
                            } else {
                                self.a_down_at = None;
                            }
                        }
                        return;
                    }
                    if pressed && self.game.is_none() && !self.inserted
                        && matches!(self.overlay, ShelfOverlay::None)
                        && self.shelves.get(self.active).is_some_and(|s| !s.carts.is_empty())
                    {
                        self.home_launch_held = true;
                        self.handle(Input::Key(96, true));
                        return;
                    }
                    // Second press of a double tap: Save States at once,
                    // without waiting for the release.
                    if pressed && self.game.is_some()
                        && matches!(self.overlay, ShelfOverlay::GameMenu { .. })
                        && self.mode_last_tap.is_some_and(|t| t.elapsed().as_millis() < DOUBLE_TAP_MS)
                    {
                        self.mode_last_tap = None;
                        self.menu_reveal_at = None;
                        self.states_from_menu = false;
                        self.mode_double_press = true;
                        self.open_polaroids();
                        return;
                    }
                    if !pressed && self.mode_double_press {
                        self.mode_double_press = false;
                        return;
                    }
                    if pressed {
                        if self.mode_down_at.is_none() {
                            self.mode_down_at = Some(Instant::now());
                        }
                    } else if let Some(since) = self.mode_down_at.take() {
                        if since.elapsed().as_millis() >= 650 {
                            self.mode_last_tap = None;
                            if self.game.is_some() {
                                crate::game::log_launch_timing(&format!(
                                    "Exit requested by MODE key release (rewind_held={})",
                                    self.rewind_held));
                                self.handle(Input::Exit);
                            }
                        } else {
                            self.slot_button_tap();
                        }
                        // START remains the ONLY menu key on the carousel.
                    }
                    return;
                }
                if self.game.is_some() && matches!(self.overlay, ShelfOverlay::Multiplayer) {
                    if pressed && self.internet_link_key(code) { return; }
                    if pressed {
                        match self.link_phase {
                            LinkMenu::Pick(_) => match code {
                                109 if self.link_mode_switchable() => {
                                    self.link_mode = self.link_mode.other();
                                    self.link_mode_override = Some(self.link_mode);
                                    // The actual gpSP option is applied by an
                                    // atomic core reload before linking.
                                }
                                21 | 22 => {
                                    self.link_role = if self.link_role == LinkRow::Host {
                                        LinkRow::Join
                                    } else { LinkRow::Host };
                                    self.link_phase = LinkMenu::Pick(self.link_role);
                                }
                                96 => self.request_multiplayer_network(),
                                97 | 108 => {
                                    self.cancel_link_discovery();
                                    self.overlay = self.link_entry.back(
                                        self.game.as_ref().is_some_and(GameSession::linked),
                                        self.game.as_ref().is_some_and(GameSession::requires_relaunch));
                                }
                                _ => {}
                            },
                            LinkMenu::Working { .. } => {
                                if code == 97 || code == 108 { self.dismiss_link(); }
                            }
                            LinkMenu::Failed { .. } => {
                                if code == 96 || code == 97 || code == 108 { self.dismiss_link(); }
                            }
                            LinkMenu::Linked { opened: true, .. } => match code {
                                96 => {
                                    // End Link is a virtual unplug, not a
                                    // cartridge ejection. Solo save states and
                                    // Polaroids become available ONLY after
                                    // a verified one-GBA recovery + promotion.
                                    match self.return_link_to_solo() {
                                        Ok(()) => {
                                            self.hud.set_link(LinkBadge::Off);
                                            // The physical unplug animation is the success cue.
                                            // No additional transient toast over gameplay.
                                            self.show_unplug(ReleaseFrom::Seated);
                                        }
                                        Err(reason) => {
                                            // Never continue in a stale Link
                                            // core and never overwrite solo
                                            // progress following recovery error.
                                            self.link_recovery_failed("Couldn't return to solo. Your solo progress is safe.", &reason);
                                        }
                                    }
                                }
                                97 | 108 => self.overlay = self.link_entry.back(
                                        self.game.as_ref().is_some_and(GameSession::linked),
                                        self.game.as_ref().is_some_and(GameSession::requires_relaunch)),
                                _ => {}
                            },
                            LinkMenu::Linked { opened: false, .. } | LinkMenu::Unplug { .. } => {}
                        }
                    }
                    return;
                }
                if self.game.is_some() && matches!(self.overlay, ShelfOverlay::States)
                    && self.polaroids.as_ref().is_some_and(Polaroids::confirming) {
                    // Original Slot 1.8: while "Delete this state?" shows,
                    // only X deletes and B keeps; every other key is ignored.
                    if pressed {
                        match code {
                            99 => self.delete_selected_polaroid(),
                            97 => {
                                if let Some(p) = self.polaroids.as_mut() { p.keep(); }
                                self.update_polaroid_title();
                            }
                            _ => {}
                        }
                    }
                    return;
                }
                if self.game.is_some() && matches!(self.overlay, ShelfOverlay::States) {
                    if pressed {
                        match code {
                            21 => {
                                if let Some(p) = self.polaroids.as_mut() { p.left(); }
                                self.update_polaroid_title();
                            }
                            22 => {
                                if let Some(p) = self.polaroids.as_mut() { p.right(); }
                                self.update_polaroid_title();
                            }
                            96 => {
                                if let (Some(game),Some(p))=(self.game.as_mut(),self.polaroids.as_ref()) {
                                    if let Some(entry)=p.selected() {
                                        if let Err(err)=game.load_manual(&entry.stamp) {
                                            set_error_message("Couldn't load this save state.", &err);
                                        }
                                    }
                                }
                                self.overlay=ShelfOverlay::None;
                            }
                            100 => { // Y asks before deleting one Slot history entry.
                                if self.polaroids.as_mut().is_some_and(Polaroids::ask_delete) {
                                    self.update_polaroid_title();
                                }
                            }
                            99 => { // X: original Slot's 30-second undo.
                                let undone=self.game.as_mut().map(|game|
                                    (game.core_kind().as_str(), game.undo_manual()));
                                match undone {
                                    Some((core, Ok(Some(stamp)))) => self.forget_polaroid(core, &stamp),
                                    Some((_, Err(err))) => set_message(err),
                                    _ => {}
                                }
                                self.overlay=ShelfOverlay::None;
                            }
                            // Back where Save States was opened from.
                            97=>self.overlay=if self.states_from_menu {
                                ShelfOverlay::GameMenu {
                                    row: self.game_menu_rows().iter()
                                        .position(|item| *item==PauseRow::States).unwrap_or(0),
                                }
                            } else { ShelfOverlay::None },
                            _=>{}
                        }
                    }
                    return;
                }
        if self.game.is_some() && matches!(self.overlay, ShelfOverlay::Cheats { .. }) {
                    if pressed { self.cheats_menu_key(code); }
                    return;
                }
        if self.game.is_some() && matches!(self.overlay, ShelfOverlay::GameMenu { .. }) {
                    if pressed {
                        let linked = self.game.as_ref().is_some_and(GameSession::linked);
                        let relaunch = self.game.as_ref().is_some_and(GameSession::requires_relaunch);
                        if let ShelfOverlay::GameMenu { row } = self.overlay {
                            match code {
                                19 | 20 | 102 | 103 => self.overlay = ShelfOverlay::GameMenu {
                                    row: move_menu_row(row, code, self.game_menu_rows().len(), self.menu_repeat_dispatch),
                                },
                                96 => match self.game_menu_rows().get(row).copied() {
                                    Some(PauseRow::SaveEject) => self.handle(Input::Exit),
                                    Some(PauseRow::Resume) => self.overlay = ShelfOverlay::None,
                                    Some(PauseRow::States) if !linked && !relaunch => {
                                        self.states_from_menu=true;
                                        self.open_polaroids();
                                    }
                                    Some(PauseRow::Multiplayer) => {
                                        self.link_entry = LinkEntry::GameMenu;
                                        if linked {
                                            // Show existing connected adapter, never unplug
                                            // when merely opening the Multiplayer menu.
                                            self.link_phase = LinkMenu::Linked {
                                                role: self.link_role,
                                                worked: self.link_worked_at,
                                                since: self.link_millis(), opened: true,
                                            };
                                            self.overlay = ShelfOverlay::Multiplayer;
                                        } else { self.enter_multiplayer_menu(); }
                                    },
                                    Some(PauseRow::Cheats) if !linked && !relaunch => self.open_cheats(),
                                    Some(PauseRow::Settings) if !relaunch =>
                                        self.overlay = ShelfOverlay::PauseSettings { row: 0 },
                                    _ => {}
                                },
                                97 => if !relaunch {
                                    self.overlay = ShelfOverlay::None
                                } else { set_message("Eject the cartridge to play solo.".into()); },
                                _ => {}
                            }
                        }
                    }
                    return;
                }
                if self.game.is_some() && matches!(self.overlay, ShelfOverlay::PauseSettings { .. }
                    | ShelfOverlay::PauseScreenSettings { .. }
                    | ShelfOverlay::PauseGameplaySettings { .. }
                    | ShelfOverlay::PauseEmulationSettings { .. }
                    | ShelfOverlay::LayoutPorts { game: true, .. }
                    | ShelfOverlay::LayoutPort { game: true, .. }) {
                    // The Screen hotkey (Hotkey + Y) again on the Screen
                    // page: the slot slides down and the game resumes.
                    let hotkey = self.controls.hotkeys.hotkey_button();
                    if code == hotkey { self.screen_hotkey_held = pressed; }
                    if pressed && self.screen_hotkey_held && code != hotkey && !self.screen_closing
                        && matches!(self.overlay, ShelfOverlay::PauseScreenSettings { .. })
                        && self.controls.hotkeys.action_for(true, code, false)
                            == Some(crate::controls::Action::Screen) {
                        self.screen_direct = true;
                        self.screen_closing = true;
                        return;
                    }
                    if pressed {
                        match self.overlay {
                            ShelfOverlay::LayoutPorts { .. } | ShelfOverlay::LayoutPort { .. } =>
                                self.controls_menu_key(code),
                            ShelfOverlay::PauseSettings { row } => match code {
                                19 | 20 | 102 | 103 => self.overlay = ShelfOverlay::PauseSettings {
                                    row: move_menu_row(row, code, self.pause_settings_rows().len(), self.menu_repeat_dispatch),
                                },
                                96 => {
                                    let linked=self.game.as_ref().is_some_and(GameSession::linked);
                                    self.overlay=match row {
                                        0 => ShelfOverlay::PauseScreenSettings { row: 0 },
                                        1 if !linked => ShelfOverlay::PauseGameplaySettings { row: 0 },
                                        2 if !linked => ShelfOverlay::PauseEmulationSettings { row: 0 },
                                        3 if !linked => Self::layout_entry(self.current_cart_platform(), true),
                                        4 if !linked => {self.reset_game_emulation();
                                            ShelfOverlay::PauseSettings { row }},
                                        _ => ShelfOverlay::PauseSettings { row },
                                    };
                                },
                                97 => self.overlay = ShelfOverlay::GameMenu {
                                    row: self.game_menu_rows()
                                        .iter().position(|item| *item==PauseRow::Settings).unwrap_or(0),
                                },
                                _ => {}
                            },
                            ShelfOverlay::PauseScreenSettings { row } => match code {
                                // The slot is sliding down on the way out.
                                _ if self.screen_closing => {}
                                // L1 / R1: the picture one size up / down, for
                                // every system (the game doesn't get them here).
                                102 | 103 => if self.screen_page_sizes() {
                                    self.step_screen_size(code == 102);
                                },
                                19 | 20 => self.overlay = ShelfOverlay::PauseScreenSettings {
                                    row: move_menu_row(row,code,self.pause_screen_page_rows().len(), self.menu_repeat_dispatch),
                                },
                                21 | 22 => if let Some(key)=self.pause_screen_page_key(row) {
                                    self.change_game_screen_or_gameplay(key,code==22);
                                    // Shader keys can switch the core's own GBA Color.
                                    // (Frame Blending acts between frames: a
                                    // still picture can't show it.)
                                    // The 3.0 systems' picture options too
                                    // (Siham 2026-11-22): seen in the pause.
                                    if matches!(key, 0..=3 | 21..=24 | 66..=69)
                                        || Self::previewed_extra(key) {
                                        // A Game Boy palette shows only once
                                        // the game draws again (key 3).
                                        self.redraw_paused_game(key == 3);
                                    }
                                },
                                // Back: the slot slides down, then Settings.
                                97 => self.screen_closing = true,
                                _ => {}
                            },
                            ShelfOverlay::PauseGameplaySettings { row } => match code {
                                19 | 20 | 102 | 103 => self.overlay = ShelfOverlay::PauseGameplaySettings {
                                    row: move_menu_row(row,code,self.pause_gameplay().1.len(), self.menu_repeat_dispatch),
                                },
                                21 | 22 => if let Some(&key) = self.pause_gameplay().1.get(row) {
                                    self.change_game_screen_or_gameplay(key,code==22)
                                },
                                97 => self.overlay = ShelfOverlay::PauseSettings { row: 1 },
                                _ => {}
                            },
                            ShelfOverlay::PauseEmulationSettings { row } => match code {
                                19 | 20 | 102 | 103 => self.overlay = ShelfOverlay::PauseEmulationSettings {
                                    row: move_menu_row(row, code, self.pause_emulation_rows().len(), self.menu_repeat_dispatch),
                                },
                                21 | 22 => match self.pause_emulation().1.get(row).copied() {
                                    Some(MOTION_KEY) => self.change_game_motion(code == 22),
                                    Some(SUN_KEY) => self.change_game_sun(code == 22),
                                    Some(key) => self.change_game_emulation(key, code == 22),
                                    None => {}
                                },
                                97 => self.overlay = ShelfOverlay::PauseSettings { row: 2 },
                                _ => {}
                            },
                            _ => {}
                        }
                    }
                    return;
                }

                if self.game.is_some() {
                    // Defer SELECT to reserve chord handling, as in upstream Gestures.
                    if code == 109 {
                        if pressed {
                            self.select_down_at = Some(Instant::now());
                            self.select_chorded = false;
                            self.select_forwarded = false;
                            self.select_tap_frames = 0;
                            self.buttons &= !ButtonMask::SELECT;
                        } else {
                            if self.select_down_at.take().is_some() && !self.select_chorded {
                                if self.select_forwarded {
                                    self.buttons &= !ButtonMask::SELECT;
                                } else {
                                    self.buttons |= ButtonMask::SELECT;
                                    self.select_tap_frames = SELECT_TAP_FRAMES;
                                }
                            } else { self.buttons &= !ButtonMask::SELECT; }
                            self.select_chorded = false;
                            self.select_forwarded = false;
                        }
                        return;
                    }
                    if self.handle_select_chord(code, pressed) { return; }
                    if pressed && self.select_down_at.is_some() && !self.select_chorded {
                        self.select_forwarded = true;
                        self.buttons |= ButtonMask::SELECT;
                    }
                    // Linked frames are ordered by both players. Never allow
                    // local time travel or accelerated single-player frames.
                    if self.game.as_ref().is_some_and(GameSession::linked)
                        && (code == 104 || code == 105) { return; }
                    // L3 opens PokeDaisy's quick menu, as in its own app.
                    if code == 106 && self.pokedaisy_running() {
                        if pressed { self.buttons |= ButtonMask::L3; }
                        else { self.buttons &= !ButtonMask::L3; }
                        return;
                    }
                    // Stick clicks (L3 / R3) set Boktai's sun, as in mGBA.
                    if code == 106 || code == 107 {
                        if pressed && self.game.as_ref().is_some_and(GameSession::solar) {
                            self.nudge_sun(code == 107);
                        } else if pressed && self.game.as_ref().is_some_and(GameSession::motion) {
                            self.nudge_motion(code == 107);
                        }
                        return;
                    }
                    if code == 104 {
                        self.rewind_key(pressed);
                        return;
                    }
                    if code == 105 {
                        self.fast_forward_key(pressed);
                        return;
                    }
                    // GB/GBC: L1 = one size up, R1 = one size down
                    // (Integer, Original Ratio, Stretch).
                    // On GBA the shoulder buttons remain owned by the core.
                    // SELECT+shoulder save/load is handled below and takes priority.
                    // SNES and Mega Drive games use L/R themselves.
                    // Systems > Controls can give L1/R1 another shortcut, or
                    // make them buttons (then they reach the game below).
                    // C / Z (six-button pads) take the same shortcuts.
                    let cz_key = code == 98 || code == 101;
                    // PokeDaisy: L1 / R1 are its companion's tabs (and the
                    // GBA's shoulders); its views are SELECT + Up / Down.
                    let pokedaisy_shoulder = !cz_key && self.pokedaisy_running();
                    if self.buttons & ButtonMask::SELECT == 0 && !pokedaisy_shoulder
                        && (code == 102 || code == 103 || cz_key) {
                        use crate::controls::ShoulderRole as Role;
                        let platform = self.current_cart_platform();
                        let session = self.game.as_ref();
                        let role = match session {
                            Some(game) if game.has_layouts() =>
                                if cz_key { game.cz_role() } else { game.shoulder_role() },
                            _ if !cz_key && !matches!(platform, Platform::Gba | Platform::Snes | Platform::Md) =>
                                Some(Role::ScreenSize),
                            _ => None,
                        };
                        let linked = session.is_some_and(GameSession::linked);
                        let first = code == 102 || code == 98;
                        let shortcut = match role {
                            Some(Role::ScreenSize) => Some(0),
                            // A Link never loads or saves a state on its own.
                            Some(Role::States) if linked => Some(-1),
                            // L1 loads and R1 saves; C saves and Z loads.
                            Some(Role::States) => Some(if first != cz_key { 102 } else { 103 }),
                            Some(Role::Shader) => Some(if first { 97 } else { 96 }),
                            Some(Role::Palette) => Some(if first { 104 } else { 105 }),
                            _ => None,
                        };
                        if let Some(shortcut) = shortcut {
                            if pressed {
                                match shortcut {
                                    0 => self.step_screen_size(first),
                                    -1 => {}
                                    other => self.shortcut(other),
                                }
                            }
                            return;
                        }
                    }
                    let mask = controls(code);
                    if pressed {
                        self.buttons |= mask;
                        self.unseen_presses |= mask;
                        self.held_releases &= !mask;
                    } else if self.unseen_presses & mask != 0 {
                        self.held_releases |= mask;
                    } else {
                        self.buttons &= !mask;
                    }
                    return;
                }
                if matches!(self.overlay, ShelfOverlay::Shelves { .. }) && matches!(code, 104 | 105) {
                    self.shelves_trigger(code, pressed);
                    return;
                }
                if matches!(self.overlay, ShelfOverlay::Shelves { .. }) && !pressed && matches!(code, 21 | 22) {
                    self.shelves_release(code);
                    return;
                }
                if self.overlay != ShelfOverlay::None {
                    if pressed {
                        match self.overlay {
                            ShelfOverlay::Shelves { .. } => self.shelves_key(code),
                            ShelfOverlay::Menu { row } => match code {
                                19 | 20 | 102 | 103 => {
                                    self.overlay = ShelfOverlay::Menu { row: move_menu_row(row, code, 6, self.menu_repeat_dispatch) };
                                }
                                // Debounce taps while Apps, Studio or RA is opening.
                                96 if !self.external_pending => {
                                    self.overlay = match row {
                                        0 => ShelfOverlay::Library { row: 0 },
                                        // Cart Studio's library page.
                                        1 => if self.open_cart_library() { ShelfOverlay::CartLibrary }
                                            else { ShelfOverlay::Menu { row } },
                                        2 => {
                                            // Keep Menu rendered behind the RA dialog until
                                            // Android has actually shown it, just like Apps.
                                            // Closing the native overlay immediately flashes
                                            // the carousel before Dialog.show() is reached.
                                            UI_ACTION.lock().unwrap_or_else(|e| e.into_inner())
                                                .push_back(3);
                                            self.external_pending = true;
                                            ShelfOverlay::Menu { row }
                                        },
                                        3 => {
                                            UI_ACTION.lock().unwrap_or_else(|e| e.into_inner()).push_back(15);
                                            self.external_pending = true;
                                            ShelfOverlay::Menu { row }
                                        },
                                        4 => ShelfOverlay::Settings { row: 0 },
                                        _ => {
                                            self.prepare_about();
                                            ShelfOverlay::About { row: 0 }
                                        },
                                    };
                                }
                                97 | 108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::Library { row } => match code {
                                19 | 20 | 102 | 103 => {
                                    let n = LIBRARY_ROWS.len();
                                    let next = move_menu_row(row, code, n, self.menu_repeat_dispatch);
                                    self.overlay = ShelfOverlay::Library { row: next };
                                }
                                96 if row == LIBRARY_SAVES_ROW =>
                                    self.overlay = ShelfOverlay::SavesStates { row: 0 },
                                // RomM, after Saves & States (Siham 2026-11-16).
                                96 if row == LIBRARY_ROMM_ROW && !self.external_pending => self.open_romm(),
                                96 if !self.external_pending => if let Some(action) = match row {
                                    // slot. Folder (its window, then Android's
                                    // picker), ROM, BIOS, Refresh.
                                    LIBRARY_SLOT_ROW => Some(70),
                                    1 => Some(1),
                                    2 => Some(4),
                                    5 => Some(2),
                                    _ => None,
                                } {
                                    UI_ACTION.lock().unwrap_or_else(|e| e.into_inner())
                                        .push_back(action);
                                    // Keep Library and its row behind the Android picker:
                                    // both selection and cancellation send ExternalReturn.
                                    // Refresh has no picker and shows its scan on the shelf.
                                    if action == 2 { self.overlay = ShelfOverlay::None; }
                                    else { self.external_pending = true; }
                                },
                                97 => self.overlay = ShelfOverlay::Menu { row: 0 },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::SavesStates { row } => match code {
                                19 | 20 | 102 | 103 => {
                                    let next = move_menu_row(row, code, SAVES_ROWS.len(), self.menu_repeat_dispatch);
                                    self.overlay = ShelfOverlay::SavesStates { row: next };
                                }
                                // Identical left/right + A toggle to Settings.
                                // Keep the selected row and refresh value in place.
                                21 | 22 | 96 if row == SAVES_SHARE_ROW => {
                                    UI_ACTION.lock().unwrap_or_else(|e| e.into_inner()).push_back(17);
                                }
                                21 | 22 | 96 if row == SAVES_ROMM_ROW => {
                                    UI_ACTION.lock().unwrap_or_else(|e| e.into_inner()).push_back(71);
                                }
                                21 | 22 | 96 if row == SAVES_STATES_ROW => {
                                    UI_ACTION.lock().unwrap_or_else(|e| e.into_inner()).push_back(72);
                                }
                                96 if row < SAVES_SHARE_ROW && !self.external_pending => {
                                    // SRAM root, State root.
                                    UI_ACTION.lock().unwrap_or_else(|e| e.into_inner())
                                        .push_back(if row == 0 { 5 } else { 6 });
                                    self.external_pending = true;
                                }
                                97 => self.overlay = ShelfOverlay::Library { row: LIBRARY_SAVES_ROW },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::ControlsSettings { .. } | ShelfOverlay::ControlsPort { .. }
                            | ShelfOverlay::LayoutPorts { .. } | ShelfOverlay::LayoutPort { .. }
                            | ShelfOverlay::Hotkeys { .. } =>
                                self.controls_menu_key(code),
                            ShelfOverlay::CartStudio => self.cart_studio_key(code),
                            ShelfOverlay::CartLibrary => self.cart_library_key(code),
                            ShelfOverlay::Romm => self.romm_key(code),
                            ShelfOverlay::Settings { row } => match code {
                                19 | 20 | 102 | 103 => self.overlay = ShelfOverlay::Settings {
                                    row: move_menu_row(row, code, settings_rows().len(), self.menu_repeat_dispatch),
                                },
                                96 => match settings_rows().get(row).copied() {
                                    Some(81) => self.overlay = ShelfOverlay::GeneralSettings { row: 0 },
                                    Some(117) => self.overlay = ShelfOverlay::ScreenSettings { row: 0 },
                                    Some(33) => self.overlay = ShelfOverlay::GameplaySettings { row: 0 },
                                    Some(49) => self.overlay = ShelfOverlay::EmulationSettings { row: 0 },
                                    Some(125) => self.overlay = ShelfOverlay::ControlsSettings { row: 0 },
                                    Some(146) => self.overlay = ShelfOverlay::RotateSettings { row: 0 },
                                    _ => return,
                                },
                                97 => self.overlay = ShelfOverlay::Menu { row: 4 },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::GeneralSettings { row } => match code {
                                19 | 20 | 102 | 103 => self.overlay = ShelfOverlay::GeneralSettings {
                                    row: move_menu_row(row, code, self.general_rows().len(), self.menu_repeat_dispatch),
                                },
                                21 | 22 | 96 => {
                                    self.cycle_general_row(row,code!=21,code==96);
                                },
                                97 => self.overlay = ShelfOverlay::Settings { row: 0 },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::WallpaperSettings { row } => {
                                // Six rows; a row past the end (an older
                                // page) comes back onto it.
                                let count = self.wallpaper_rows().0.len();
                                let row = row.min(count - 1);
                                match code {
                                    19 | 20 | 102 | 103 => self.overlay = ShelfOverlay::WallpaperSettings {
                                        row: move_menu_row(row, code, count, self.menu_repeat_dispatch),
                                    },
                                    21 | 22 | 96 => self.cycle_wallpaper_row(row, code != 21, code == 96),
                                    97 => self.overlay = ShelfOverlay::GeneralSettings { row: INTERFACE_WALLPAPER_ROW },
                                    108 => self.overlay = ShelfOverlay::None,
                                    _ => {}
                                }
                            },
                            ShelfOverlay::InterfaceSettings { row } => match code {
                                19 | 20 | 102 | 103 => self.overlay = ShelfOverlay::InterfaceSettings {
                                    row: move_menu_row(row, code, interface_rows().0.len(), self.menu_repeat_dispatch),
                                },
                                21 | 22 | 96 => {
                                    if let Some(&key) = interface_rows().1.get(row) {
                                        self.change_setting(key, code != 21);
                                    }
                                },
                                97 => self.overlay = ShelfOverlay::GeneralSettings { row: INTERFACE_STATUS_ROW },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::RotateSettings { row } => match code {
                                19 | 20 | 102 | 103 => self.overlay = ShelfOverlay::RotateSettings {
                                    row: move_menu_row(row, code, ROTATE_ROWS.len(), self.menu_repeat_dispatch),
                                },
                                21 | 22 | 96 => match ROTATE_KEYS.get(row).copied() {
                                    // Android's own page grants the right; slot. gives it back,
                                    // asked first, on the arrows as on A (Siham 2026-11-15).
                                    Some(85) => UI_ACTION.lock().unwrap_or_else(|e| e.into_inner())
                                        .push_back(match ROTATE_SLEEP_STATE.load(Ordering::Acquire) {
                                            1 => 61, 2 => 62, _ => 60 }),
                                    Some(key) => self.change_setting(key, code != 21),
                                    None => {}
                                },
                                97 => self.overlay = ShelfOverlay::Settings {
                                    row: settings_rows().iter().position(|&r| r == 146).unwrap_or(0),
                                },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::ScreenSettings { row } => match code {
                                19 | 20 | 102 | 103 => self.overlay = ShelfOverlay::ScreenSettings {
                                    row: move_menu_row(row, code, SYSTEM_ORDER.len(), self.menu_repeat_dispatch),
                                },
                                96 => {
                                    self.load_default_cores();
                                    self.overlay=ShelfOverlay::ScreenSystemSettings {
                                        platform:Self::screen_submenu_platform(row),row:0,
                                    };
                                },
                                97 => self.overlay = ShelfOverlay::Settings { row: 1 },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::ScreenSystemSettings { platform,row } => match code {
                                19 | 20 | 102 | 103 => self.overlay=ShelfOverlay::ScreenSystemSettings {
                                    platform,row:move_menu_row(row,code,Self::system_page_rows(platform).len(), self.menu_repeat_dispatch),
                                },
                                96 if row+1==Self::system_page_rows(platform).len() =>
                                    self.overlay=Self::layout_entry(platform, false),
                                21 | 22 | 96 => if let Some(key)=Self::system_page_keys(platform).get(row).copied()
                                    .filter(|key| *key!=usize::MAX) {
                                    self.change_setting(key,code!=21);
                                },
                                97 => self.overlay=ShelfOverlay::ScreenSettings {
                                    row:SYSTEM_ORDER.iter().position(|p| *p==platform).unwrap_or(0),
                                },
                                108 => self.overlay=ShelfOverlay::None,
                                _=>{}
                            },
                            ShelfOverlay::EmulationSettings { row } => match code {
                                19 | 20 | 102 | 103 => self.overlay = ShelfOverlay::EmulationSettings {
                                    row: move_menu_row(row, code, 3, self.menu_repeat_dispatch),
                                },
                                21 | 22 => self.change_setting(row + 10, code == 22),
                                97 => self.overlay = ShelfOverlay::Settings { row: 3 },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::GameplaySettings { row } => match code {
                                19 | 20 | 102 | 103 => self.overlay = ShelfOverlay::GameplaySettings {
                                    row: move_menu_row(row, code, GAMEPLAY_KEYS.len(), self.menu_repeat_dispatch),
                                },
                                21 | 22 => self.change_setting(GAMEPLAY_KEYS[row], code == 22),
                                97 => self.overlay = ShelfOverlay::Settings { row: 2 },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::About { row } => match code {
                                19 | 20 | 102 | 103 => self.overlay = ShelfOverlay::About {
                                    row: move_menu_row(row, code, 2, self.menu_repeat_dispatch),
                                },
                                96 => {
                                    let action = if row == 0 {
                                        about_primary_action(self.update_status)
                                    } else { 22 };
                                    UI_ACTION.lock().unwrap_or_else(|e| e.into_inner())
                                        .push_back(action);
                                },
                                21 | 22 if row == 1 =>
                                    UI_ACTION.lock().unwrap_or_else(|e| e.into_inner()).push_back(22),
                                // X: slot.'s logs saved to a file (#107).
                                99 => UI_ACTION.lock().unwrap_or_else(|e| e.into_inner())
                                    .push_back(EXPORT_LOGS_ACTION),
                                97 => self.overlay = ShelfOverlay::Menu { row: 5 },
                                108 => self.overlay = ShelfOverlay::None,
                                _ => {}
                            },
                            ShelfOverlay::Core => {
                                let now = self.born.elapsed().as_millis() as u64;
                                // A pak board's PokeDaisy chip sits over mGBA's.
                                let column = self.picker_three.is_some_and(|t| t.socket.is_none());
                                if let Some(picker) = self.picker.as_mut() {
                                    match code {
                                        21 | 22 => {
                                            let press = if code == 21 { Press::Left } else { Press::Right };
                                            // Splore's chip stays on PICO-8: say why.
                                            let to = if code == 21 { picker.left() } else { picker.right() };
                                            if picker.press(press, now) == crate::core_picker::Outcome::Refused
                                                && !picker.allows(to) {
                                                set_message(crate::pico8::SPLORE_ONLY.to_string());
                                            }
                                        }
                                        19 if column => { picker.press(Press::To(0), now); }
                                        20 if column => { picker.press(Press::To(1), now); }
                                        96 => self.seat_core_picker(),
                                        97 | 109 => { picker.press(Press::Back, now); }
                                        _ => {}
                                    }
                                }
                            },
                            ShelfOverlay::GameMenu { .. } | ShelfOverlay::Multiplayer |
                            ShelfOverlay::PauseSettings { .. } | ShelfOverlay::PauseScreenSettings { .. } |
                            ShelfOverlay::PauseGameplaySettings { .. } | ShelfOverlay::PauseEmulationSettings { .. } |
                            ShelfOverlay::States | ShelfOverlay::Cheats { .. } | ShelfOverlay::None => {}
                        }
                    }
                    return;
                }
                // L2 / R2 on the carousel: a tap shows the GBA Video shelf,
                // both held delete the game (Siham 2026-11-10).
                if matches!(code, 104 | 105) {
                    if !self.inserted && self.progress == 0.0 { self.carousel_trigger(code, pressed); }
                    return;
                }
                if pressed {
                    match code {
                        // X edits the current cartridge directly without insertion.
                        99 if !self.inserted && !self.shelves[self.active].carts.is_empty() => {
                            self.open_cart_studio();
                            return;
                        }
                        // Original Slot v1.7: Y silently toggles the star.
                        // All per-ROM settings remain in Pause -> Settings.
                        100 if !self.inserted => {
                            self.toggle_favorite();
                            return;
                        }
                        108 if !self.inserted => {
                            self.shelves[self.active].release_hold();
                            self.overlay = ShelfOverlay::Menu { row: 0 };
                            return;
                        }
                        109 if !self.inserted && !self.shelves[self.active].carts.is_empty() => {
                            self.open_core_picker();
                            return;
                        }
                        102 | 103 if !self.inserted => {
                            let delta = if code == 102 { -1 } else { 1 };
                            // An empty platform is never a visible stop.
                            let next = self.next_shelf(self.active, delta);
                            if next != self.active {
                                self.shelves[self.active].release_hold();
                                self.active = next;
                                self.progress = 0.0;
                                self.letter_shown_at = None;
                                self.platform_shown_at = Some(Instant::now());
                                self.last_cart_navigation = Instant::now();
                            }
                            return;
                        }
                        96 if !self.inserted && self.shelves[self.active].carts.is_empty() => {
                            self.refusal = Some(slot_ui::Refusal::started(
                                self.born.elapsed().as_millis() as u64));
                            return;
                        }
                        96 if !self.inserted && ROMS_MISSING.load(Ordering::Acquire) => {
                            self.refusal = Some(slot_ui::Refusal::started(
                                self.born.elapsed().as_millis() as u64));
                            set_message("The SD card with your games isn't in.".into());
                            return;
                        }
                        96 if !self.inserted && !self.shelves[self.active].carts.is_empty()
                            && self.selected_test_cart() => {
                            // A 3.0 test cart goes in and stays seated: no
                            // core to start yet (3.0.0-cartes1).
                            self.inserted = true;
                            self.prepared_rom = None;
                            self.pending_game = None; self.screen_ready = None;
                            self.insert_started = Some(Instant::now());
                            self.a_down_at = self.insert_started;
                            self.fresh_launch = false;
                            set_message("No core for this system yet: 3.0 test cart.".into());
                            return;
                        }
                        // Splore without PICO-8 in the BIOS folder (taken out since): say so.
                        96 if !self.inserted && self.shelves[self.active].carts
                            .get(self.shelves[self.active].index)
                            .is_some_and(|cart| core_selection::is_splore(&cart.rom.to_string_lossy()))
                            && !PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
                                .is_some_and(|(storage, _)| crate::pico8::installed(storage)) => {
                            set_message(crate::pico8::MISSING.to_string());
                        }
                        96 if !self.inserted && !self.shelves[self.active].carts.is_empty() => {
                            self.load_selected_emulation();
                            self.inserted = true;
                            // A launch owns the CPU: the disk fill waits.
                            self.cart_render.stop_filling();
                            let selected = &self.shelves[self.active].carts[self.shelves[self.active].index];
                            let uri = selected.rom.to_string_lossy();
                                        self.prepared_rom = None;
                            self.pending_game = None; self.screen_ready = None;
                            self.insert_started = Some(Instant::now());
                            crate::frame_watch::insert_started();
                            self.a_down_at = self.insert_started;
                            self.fresh_launch = false;
                            // ROM files, and Splore (no file: Android hands the
                            // core a stand-in, MainActivity.loadGameFromSaf).
                            if uri.starts_with("content://") || core_selection::is_splore(&uri) {
                                let uri = uri.into_owned();
                                let platform = selected.platform;
                                *LAUNCH_CORE.lock().unwrap_or_else(|e| e.into_inner()) =
                                    PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
                                        .map(|(storage, _)| (uri.clone(),
                                            core_selection::selected_for(storage, &uri, platform)));
                                self.requested_uri = Some(uri.clone());
                                self.awaiting_game = true;
                                *REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = Some(uri);
                            }
                            return;
                        }
                        97 if !self.inserted && self.progress <= 0.0
                            && self.triggers.hold.is_none() && self.triggers.ejecting.is_none()
                            && !self.lid_covered && self.lid_t <= 0.0 => {
                            self.open_shelves();
                            return;
                        }
                        97 => {
                            if self.inserted && self.progress > 0.0 {
                                self.eject_sound_armed = true;
                            }
                            if self.inserted { crate::frame_watch::cancel(); }
                            self.inserted = false;
                            self.a_down_at = None;
                            self.fresh_launch = false;
                            self.awaiting_game = false;
                            self.prepared_rom = None;
                            self.pending_game = None; self.screen_ready = None;
                            self.requested_uri = None;
                                        *REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = None;
                            return;
                        }
                        _ => {}
                    }
                }
                if code == 96 && !pressed {
                    if let Some(when) = self.a_down_at.take() {
                        self.fresh_launch = when.elapsed().as_millis() >= 400;
                    }
                    return;
                }
                let ms = self.born.elapsed().as_millis() as u64;
                if !self.inserted && pressed && matches!(code,19..=22) {
                    self.last_cart_navigation=Instant::now();
                }
                let shelf = &mut self.shelves[self.active];
                let before = shelf.index;
                match (code, pressed) {
                    (21, true) if !self.inserted => shelf.hold_left(ms),
                    (22, true) if !self.inserted => shelf.hold_right(ms),
                    (21, false) => shelf.release_left(),
                    (22, false) => shelf.release_right(),
                    (19, true) if !self.inserted => shelf.hold_up(ms),
                    (20, true) if !self.inserted => shelf.hold_down(ms),
                    (19, false) => shelf.release_up(),
                    (20, false) => shelf.release_down(),
                    _ => {}
                }
                let jumped= !self.inserted && pressed && (code==19 || code==20)
                    && shelf.index!=before;
                if jumped { self.show_selected_letter(); }
            }
        }
    }

    fn upload_clear_material(&mut self, key:(usize,usize), layers:[slot_ui::CartFace;3]) {
        let textures=if let Some(existing)=self.clear_material {
            for (tex,face) in existing.tex.iter().copied().zip(layers) {
                self.gpu.update_face(tex, &face);
            }
            existing.tex
        } else {
            layers.map(|face|self.gpu.create_face(&face))
        };
        self.clear_material=Some(ClearMaterial{key,tex:textures});
    }
    /// Select's core picker for the selected cart (also a finger pushing
    /// the middle cart up, touch.rs).
    pub(super) fn open_core_picker(&mut self) {
        let Some(selected) = self.shelves[self.active].carts.get(self.shelves[self.active].index) else { return };
        if !slot_ui::cart_opens(selected) {
            // One core only: the cart shakes, as on an empty shelf
            // (Siham 2026-11-22).
            self.refusal = Some(slot_ui::Refusal::started(self.born.elapsed().as_millis() as u64));
            return;
        }
        // PokeDaisy's chip on the board of a Pokemon game it plays, when
        // this slot. carries its core.
        let uri = selected.rom.to_string_lossy().into_owned();
        let carried = PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
            .is_some_and(|(_, library)| library.join(format!("lib{}_libretro.so", Core::PokeDaisy.as_str())).is_file());
        let three = (carried && core_selection::pokedaisy_plays(&uri, selected.platform))
            .then(|| slot_ui::three_chips(selected)).flatten();
        let face = if three.is_some() { slot_ui::board_face_three(selected) } else { board_face(selected) };
        if three.as_ref().is_some_and(|t| t.socket.is_none()) && self.pak_three_textures.is_none() {
            let gpu = &mut self.gpu;
            self.pak_three_textures = Some([Some(Core::PokeDaisy), Some(Core::Mgba), Some(Core::Gambatte), None]
                .map(|core| gpu.create_face(&slot_ui::three_pak_chip_face(core))));
        }
        if let Some(texture) = self.board_texture {
            self.gpu.update_face(texture, &face);
        } else {
            self.board_texture = Some(self.gpu.create_face(&face));
        }
        let lid = slot_ui::lid_face(selected);
        self.picker_lid.1 = lid.is_some();
        if let Some(lid) = lid {
            match self.picker_lid.0 {
                Some(texture) => self.gpu.update_face(texture, &lid),
                None => self.picker_lid.0 = Some(self.gpu.create_face(&lid)),
            }
        }
        let now = self.born.elapsed().as_millis() as u64;
        let current_core = PATHS.lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|(storage, _)| core_selection::selected_for(
                storage, &selected.rom.to_string_lossy(), selected.platform))
            .unwrap_or_else(|| Core::default_for(selected.platform));
        // The file's own core, whatever the saved choice (Siham 2026-11-27).
        let current_core = core_selection::imposed(&uri, selected.platform).unwrap_or(current_core);
        let [primary, alternate] = slot_ui::board_cores(selected);
        // A 3.0 test cart keeps no core: its first chip is seated.
        let current_core = if slot_ui::is_test_cart(selected) { primary } else { current_core };
        let mut picker = match three {
            Some(three) => CorePicker::open_three(current_core, three.cores, now),
            None => CorePicker::open_between(current_core, primary, alternate, now),
        };
        if core_selection::is_splore(&selected.rom.to_string_lossy()) {
            picker = picker.only(Core::Pico8);
        }
        // A Neo Geo .zip or .neo, a SuperGrafx game: the file's core only,
        // the other chip greyed, no swap (Siham 2026-11-27).
        if let Some(core) = core_selection::imposed(&uri, selected.platform) {
            picker = picker.only(core);
        }
        self.picker_three = three;
        // A clear cart's lid waits for its plastic without
        // the board (start_waiting_picker), or the shelf
        // face would lift with the board drawn on it.
        if self.picker_lid_ready() { picker.start(now); }
        self.picker = Some(picker);
        self.picker_auto_seat = None;
        self.overlay = ShelfOverlay::Core;
    }

    /// A on the core picker: the chip's core becomes the cart's.
    pub(super) fn seat_core_picker(&mut self) {
        let now = self.born.elapsed().as_millis() as u64;
        let Some(core) = self.picker.map(|picker| picker.seat()) else { return };
        let cart = &self.shelves[self.active].carts[self.shelves[self.active].index];
        if slot_ui::is_test_cart(cart) {
            set_message(format!("{} comes with slot. 3.0.", core.text()));
            return;
        }
        let uri = cart.rom.to_string_lossy().into_owned();
        let platform = cart.platform;
        let paths = PATHS.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some((storage, library)) = paths {
            let core_file = library.join(format!("lib{}_libretro.so", core.as_str()));
            if !core_file.is_file() {
                set_message(format!("{} isn't installed.", core.text()));
            } else if core == Core::Pico8 && !crate::pico8::installed(&storage) {
                set_message(crate::pico8::MISSING.to_string());
            } else {
                match core_selection::choose(&storage, &uri, platform, core) {
                    Ok(()) => {
                        // A core swap must invalidate the previous core's
                        // cached profile immediately, even when Y was opened
                        // before selecting this chip.
                        self.game_emulation = EmulationProfile::load(&storage, &uri, core);
                        if let Some(picker) = self.picker.as_mut() { picker.press(Press::Keep, now); }
                    }
                    Err(err) => set_error_message("Couldn't save the selected core.", &err),
                }
            }
        }
    }

    /// Whether the selected cart's lid can lift: not clear, or its clear
    /// layers are uploaded.
    fn picker_lid_ready(&self) -> bool {
        let shelf = &self.shelves[self.active];
        !shelf.carts.get(shelf.index).is_some_and(slot_ui::is_clear) || self.clear_layers().1.is_some()
    }

    /// A core picker held for a clear cart's lid opens once the lid is
    /// ready, or after PICKER_LID_WAIT_MS at worst (Siham 2026-11-09).
    fn start_waiting_picker(&mut self) {
        let now = self.born.elapsed().as_millis() as u64;
        let ready = self.picker_lid_ready();
        if let Some(picker) = self.picker.as_mut() {
            if picker.waiting() && (ready || picker.waited(now) >= PICKER_LID_WAIT_MS) {
                picker.start(now);
            }
        }
    }

    fn clear_layers(&self) -> ([Option<slot_gfx::TexId>;2],Option<slot_gfx::TexId>) {
        let selected=self.shelves.get(self.active).map(|s|(self.active,s.index));
        match (self.clear_material,selected) {
            (Some(layer),Some(key)) if layer.key==key =>
                ([Some(layer.tex[0]),Some(layer.tex[1])],Some(layer.tex[2])),
            _ => ([None,None],None),
        }
    }

    // Upstream Slot rasterizes its artwork on worker threads. Keep the same
    // separation on Android: no slow PNG decode/embossing on the GLES thread.
    // GPU texture uploads stay on their GL owner and are bounded per frame.
    /// A cart's label file changed: redraw its face on every shelf.
    /// One arrived label onto its cart, when no game is on or starting and
    /// the shelf has been still a moment (its face is drawn on this thread).
    fn relabel_one(&mut self) {
        if self.relabel.is_empty() || self.game.is_some() || self.inserted || self.awaiting_game
            || self.pending_game.is_some()
            || self.shelves.get(self.active).is_some_and(|s| s.is_scrolling())
            || self.last_cart_navigation.elapsed() < Duration::from_millis(250)
            // Nor while a cart flies to or from Cart Studio.
            || self.studio_flight.is_some_and(|f| f.flying()) { return; }
        if let Some(uri) = self.relabel.pop_front() {
            self.reload_cart_label(&uri);
            crate::frame_watch::did(crate::frame_watch::WORK_LABEL);
        }
    }

    fn reload_cart_label(&mut self, uri: &str) {
        for (sid, shelf) in self.shelves.iter().enumerate() {
            for (index, cart) in shelf.carts.iter().enumerate() {
                if cart.rom.to_string_lossy() == uri {
                    // A face rendered before the label came is never shown.
                    self.cart_render.invalidate((sid, index));
                    // A face on the shelf is drawn again by the raster
                    // workers, not here: drawing it on this thread held the
                    // shelf up to 250ms per label on the KONKR (beta41).
                    if shelf.face(index).is_some() {
                        if !self.relabelled.contains(&(sid, index)) { self.relabelled.push((sid, index)); }
                        self.cart_render.request((sid, index), cart, false);
                    }
                }
            }
        }
    }

    fn prepare_visible(&mut self) {
        // During scrolling cap uploads to one per frame and never work on
        // background disk tasks on the OpenGL thread.
        // A launch owns the CPU: an inserted cartridge stops the idle fill.
        let quiet=!self.inserted
            && self.last_cart_navigation.elapsed()>=CART_IDLE_GRACE
            && !self.shelves[self.active].is_scrolling()
            && matches!(self.overlay,ShelfOverlay::None);
        // Two faces a frame, moving or not, within the 2 ms budget (3.2
        // perf: one while the shelf moved).
        let limit=2;
        let upload_started=Instant::now();
        let mut uploads=0u32;
        for _ in 0..limit {
            if upload_started.elapsed()>=CART_GPU_UPLOAD_BUDGET {break}
            let Some(((shelf_id,index),face,layers))=self.cart_render.ready() else {break};
            crate::frame_watch::did(crate::frame_watch::WORK_LABEL);
            uploads+=1;
            // Library > RomM's carts live on its page, not on a shelf.
            if shelf_id==romm::ROMM_RACK { self.romm_face(index,&face); continue; }
            let Some(shelf)=self.shelves.get(shelf_id) else {continue};
            if index>=shelf.carts.len(){continue}
            if let Some(layers)=layers {
                if shelf_id==self.active && index==shelf.index {
                    self.upload_clear_material((shelf_id,index),layers);
                }
            }
            if let Some(tex) = self.shelves[shelf_id].face(index) {
                // A new label's face over the old one.
                if let Some(at) = self.relabelled.iter().position(|k| *k == (shelf_id, index)) {
                    self.relabelled.swap_remove(at);
                    self.gpu.update_face(tex, &face);
                }
                continue
            }
            let texture = if self.texture_cache.len() >= CART_TEXTURE_CACHE_CAPACITY {
                // Pin selected +/- one cart from each nonempty shelf (up to
                // 15 across GBA/GB/GBC/Recents/Favorites). Preserve warm art
                // while the 96-face LRU bounds shared GPU memory.
                // Distances wrap: on the first cart the left neighbour is the
                // last one, and evicting it made it flicker on every upload.
                // Nothing on screen on the active shelf is ever taken.
                let active = self.active;
                let on_screen = self.shelves[active].on_screen();
                let victim = self.texture_cache.iter().position(|(sid, index, _)| {
                    self.shelves.get(*sid).is_none_or(|shelf|
                        shelf.carts.is_empty()
                            || (cyclic_distance(shelf.index, *index, shelf.carts.len()) > 1
                                && !(*sid == active && on_screen.contains(index))))
                }).unwrap_or(0);
                let (old_shelf, old_index, tex) = self.texture_cache
                    .remove(victim).expect("nonempty texture pool");
                if let Some(shelf) = self.shelves.get_mut(old_shelf) {
                    shelf.take_face(old_index);
                }
                self.gpu.update_face(tex, &face);
                tex
            } else {
                self.gpu.create_face(&face)
            };
            self.shelves[shelf_id].set_face(index, texture);
            self.texture_cache.push_back((shelf_id, index, texture));
        }
        let shelf_id = self.active;
        let selected = self.shelves[shelf_id].index;
        let blank = if matches!(self.overlay, ShelfOverlay::None) && !self.inserted {
            self.shelves[shelf_id].on_screen().into_iter()
                .filter(|index| self.shelves[shelf_id].face(*index).is_none()).count() as u32
        } else { 0 };
        crate::frame_watch::labels_frame(uploads, blank);
        if quiet && self.last_cart_navigation.elapsed() >= DISK_FILL_AFTER
            && self.cart_render.is_idle() && self.game.is_none() {
            self.fill_disk_cache();
        }
        // Priorities are recomputed every frame, so a rapid D-pad or L1/R1
        // turn promotes new visible carts ahead of stale background requests.
        // Order: selected, active +/-1, next/previous system, the rest.
        let mut priority: Vec<(usize,usize)> = selected_prefetch_entry(&self.shelves,shelf_id)
            .into_iter().collect();
        for delta in [-1isize,1] {
            let pos=(selected as isize+delta).rem_euclid(
                self.shelves[shelf_id].carts.len().max(1) as isize) as usize;
            if !self.shelves[shelf_id].carts.is_empty()
                && !priority.contains(&(shelf_id,pos)) {
                priority.push((shelf_id,pos));
            }
        }
        for direction in [1,-1] {
            let sid=next_nonempty_shelf(&self.shelves,shelf_id,direction);
            if sid!=shelf_id && !priority.contains(&(sid,self.shelves[sid].index)) {
                priority.push((sid,self.shelves[sid].index));
            }
        }
        for index in self.shelves[shelf_id].on_screen() {
            if !priority.contains(&(shelf_id,index)) {
                priority.push((shelf_id,index));
            }
        }
        // While the shelf moves, the carts about to come on screen, the
        // way it goes (3.2 perf): their faces are ready when they arrive.
        for index in self.shelves[shelf_id].coming(CART_LOOK_AHEAD) {
            if !priority.contains(&(shelf_id,index)) {
                priority.push((shelf_id,index));
            }
        }
        for direction in [1,-1] {
            let sid=next_nonempty_shelf(&self.shelves,shelf_id,direction);
            if sid==shelf_id { continue; }
            let shelf=&self.shelves[sid];
            for delta in [-1isize,1] {
                let index=(shelf.index as isize+delta)
                    .rem_euclid(shelf.carts.len().max(1) as isize) as usize;
                if !priority.contains(&(sid,index)) {
                    priority.push((sid,index));
                }
            }
        }
        for (sid,shelf) in self.shelves.iter().enumerate() {
            if !shelf.carts.is_empty() && !priority.contains(&(sid,shelf.index)) {
                priority.push((sid,shelf.index));
            }
        }
        // The shelves page shows each system's chosen cart (above) with the
        // one before and after it, and the first three of Recents and
        // Favorites, fanned.
        if matches!(self.overlay, ShelfOverlay::Shelves { .. }) {
            // GBA Video too: its rack follows Recents and Favorites.
            for (sid,shelf) in self.shelves.iter().enumerate() {
                if sid == RECENTS_RACK || sid == FAVORITES_RACK { continue; }
                let len=shelf.carts.len();
                if len < 2 { continue; }
                for index in [(shelf.index+len-1)%len,(shelf.index+1)%len] {
                    if !priority.contains(&(sid,index)) { priority.push((sid,index)); }
                }
            }
            for sid in [RECENTS_RACK,FAVORITES_RACK] {
                let len=self.shelves.get(sid).map_or(0,|shelf| shelf.carts.len());
                for index in 0..len.min(3) {
                    if !priority.contains(&(sid,index)) { priority.push((sid,index)); }
                }
            }
        }
        if quiet {
            // Fill the 160-entry LRU while idle, not while the D-pad is
            // moving. Active system first; other systems around their focus.
            // A set says what is listed already (3.2 perf: the list was
            // searched for each of up to 155 carts, every frame).
            let mut listed: std::collections::HashSet<(usize,usize)> = priority.iter().copied().collect();
            for (sid,shelf) in self.shelves.iter().enumerate() {
                let radius=if sid==shelf_id {80} else {16};
                for index in shelf.around(radius) {
                    if priority.len()>=CART_IDLE_PREFETCH_CAP {break}
                    if listed.insert((sid,index)) {priority.push((sid,index));}
                }
                if priority.len()>=CART_IDLE_PREFETCH_CAP {break}
            }
        }
        // Cart Studio's library page: the carts it shows come first.
        let mut cells = self.cart_library_cells();
        if !cells.is_empty() {
            let rest: Vec<(usize,usize)> = priority.into_iter().filter(|p| !cells.contains(p)).collect();
            cells.extend(rest);
            priority = cells;
        }
        // Library > RomM: its page's carts before anything else.
        let romm = if matches!(self.overlay, ShelfOverlay::Romm) { self.romm_render_wants() } else { Vec::new() };
        if !romm.is_empty() {
            let mut first: Vec<(usize,usize)> = romm.iter().map(|(key, _)| *key).collect();
            first.extend(priority.iter().copied());
            self.cart_render.prioritize(&first);
            for (key, cart) in &romm { self.cart_render.request(*key, cart, false); }
        } else {
            self.cart_render.prioritize(&priority);
        }
        // Touch the LRU least important first, so the selected cart and its
        // neighbours end up the most recently used: the faces wanted go to
        // the back in that order, the others keep theirs in front (one
        // stable sort, 3.2 perf, instead of a search per wanted cart).
        let rank: std::collections::HashMap<(usize,usize),usize> = priority.iter().copied().enumerate()
            .filter(|(_, (sid, index))| self.shelves.get(*sid).is_some_and(|shelf| shelf.face(*index).is_some()))
            .map(|(rank, key)| (key, rank)).rev().collect();
        if !rank.is_empty() {
            self.texture_cache.make_contiguous().sort_by_key(|(sid, index, _)|
                rank.get(&(*sid, *index)).map_or((0, 0), |rank| (1, usize::MAX - rank)));
        }
        for (sid, index) in priority {
            // Clear-cart composite is expensive: delay its three material
            // uploads until the carousel has settled.
            // The core picker lifts a clear cart's lid: its plastic without
            // the board is wanted at once, settled carousel or not.
            let wants_material=(quiet || self.picker.is_some()) && sid==self.active && index==selected &&
                self.shelves[sid].carts.get(index).is_some_and(slot_ui::is_clear) &&
                self.clear_material.map(|m|m.key)!=Some((sid,index));
            if self.shelves[sid].face(index).is_some() {
                if !wants_material && !self.relabelled.contains(&(sid, index)) {continue;}
            }
            // A freshly configured AVD has no ROMs yet. Skip the empty
            // shelf instead of indexing carts[0] and aborting the GL thread.
            let Some(cart) = self.shelves.get(sid).and_then(|shelf| shelf.carts.get(index))
            else { continue; };
            self.cart_render.request((sid, index), cart, wants_material);
        }
    }

    /// While the shelf rests, every system's carts drawn into the disk
    /// cache once, two at a time behind any face wanted (3.2 perf): later
    /// each one comes back from the disk instead of being drawn again from
    /// its label. One pass per library; a library of another size starts
    /// a new one. At most DISK_FILL_CAP carts.
    fn fill_disk_cache(&mut self) {
        let systems = Platform::ALL.len().min(self.shelves.len());
        let total: usize = self.shelves[..systems].iter().map(|shelf| shelf.carts.len()).sum();
        if total != self.disk_fill.2 { self.disk_fill = (0, 0, total, 0); }
        while self.disk_fill.3 < total.min(DISK_FILL_CAP) {
            let (sid, index, _, _) = self.disk_fill;
            if sid >= systems { self.disk_fill.3 = total; break; }
            let Some(cart) = self.shelves[sid].carts.get(index) else {
                self.disk_fill.0 += 1;
                self.disk_fill.1 = 0;
                continue;
            };
            if !self.cart_render.fill_disk((sid, index), cart) { break; }
            self.disk_fill.1 += 1;
            self.disk_fill.3 += 1;
        }
    }

    fn ensure_ra_service(&mut self) {
        if self.ra_signing_out || self.ra_service.is_some() { return; }
        let root = PATHS.lock().unwrap_or_else(|e| e.into_inner())
            .as_ref().map(|(root, _)| root.clone());
        if let Some(root) = root {
            // Explicit opt-in from the Android account login. Never start RA
            // until an actual private config exists.
            if root.join("Config/retroachievements.toml").is_file() {
                self.ra_service = Some(slot_achievements::Service::start(root));
            }
        }
    }

    fn prepare_game_core(&mut self, local: &str) {
        let paths = PATHS.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some((storage, library)) = paths {
            let uri = self.requested_uri.as_deref().unwrap_or_default();
            let platform = self.shelves[self.active].carts
                .get(self.shelves[self.active].index)
                .map_or(Platform::Gba, |cart|cart.platform);
            let mut core = core_selection::selected_for(&storage, uri, platform);
            if core == Core::Gambatte && crate::game::gambatte_cannot_run(Path::new(local)) {
                crate::game::log_launch_timing("MBC7 tilt cartridge: mGBA instead of Gambatte");
                core = Core::Mgba;
            }
            if core == Core::Gpsp && crate::game::gpsp_cannot_run(Path::new(local)) {
                crate::game::log_launch_timing("Sensor cartridge: mGBA instead of gpSP");
                core = Core::Mgba;
            }
            let core_file = library.join(format!("lib{}_libretro.so", core.as_str()));
            // The core sees a cached copy, never the cart's name.
            crate::pico8::set_splore(core == Core::Pico8 && core_selection::is_splore(uri));
            // First game still wakes RA if the asynchronous SAF scan has not run.
            // Perform this mutable operation BEFORE borrowing rom_stem.
            // GBA, GB and GBC use the verified per-console memory maps.
            self.ensure_ra_service();
            let rom_stem = self.shelves[self.active].carts
                .get(self.shelves[self.active].index)
                .map_or("", |cart| cart.stem.as_str());
            // The game isn't running yet: PokeDaisy's own defaults (cart_globals
            // gives them once it is) for the palette and colours it starts with.
            let mut effective_settings = self.cart_globals();
            if core == Core::PokeDaisy { effective_settings.apply_pokedaisy_defaults(platform); }
            if let Some(profile) = self.game_emulation {
                profile.apply_visual_and_gameplay(&mut effective_settings, platform);
            }
            // The 3.0 options this game starts with: its own, else its system's.
            crate::settings::CoreExtras::store(&effective_settings);
            let started = Instant::now();
            let resume_preference = NEXT_RESUME_MODE.swap(-1, Ordering::AcqRel);
            let wants_state = match resume_preference {
                0 => false, 1 => true,
                _ => effective_settings.eject_save && !self.fresh_launch,
            };
            match GameSession::open(Path::new(local), &core_file, &storage,
                core, platform, wants_state,
                rom_stem, effective_settings.gb_palettes.then(|| effective_settings.palette_name()),
                effective_settings.shader_for(platform)
                    .core_colour(effective_settings.colour_for(platform)),
                self.ra_service.as_ref()) {
                Ok(session) => {
                    self.link_identity = if platform == Platform::Gba {
                        crate::link_rom::identify(Path::new(local)).ok()
                    } else if platform.handheld() {
                        crate::link_rom::identify_gb(Path::new(local)).ok()
                    } else {
                        crate::link_rom::identify_home(Path::new(local)).ok()
                    };
                    self.link_mode_override = None;
                    // Create libretro on the same GL thread, but defer audio
                    // and CRT presentation until Slot's seated hold completes.
                    crate::game::log_launch_timing(&format!(
                        "Core prepared: {}ms loading, {}ms since A",
                        started.elapsed().as_millis(),
                        self.insert_started.map_or(0, |at| at.elapsed().as_millis())
                    ));
                    // Keep launch bookkeeping INSIDE the cartridge seated
                    // hold, not the first visible gameplay frame. Recording
                    // the selected game performs synchronous focus/Recents
                    // writes and can rebuild the carousel rack on the GL
                    // thread. That caused avoidable work right when the CRT
                    // first switched on. A failed core launch never reaches
                    // this point, so failed games are not recorded.
                    let cheats_on = session.cheats().len();
                    if cheats_on > 0 {
                        set_message(if cheats_on == 1 { "1 cheat on".into() }
                            else { format!("{cheats_on} cheats on") });
                    }
                    self.pending_game = Some(session);
                    self.screen_ready = None;
                    let recents_started = Instant::now();
                    self.record_recent();
                    crate::game::log_launch_timing(&format!(
                        "Recents staged during cartridge hold: {}ms",
                        recents_started.elapsed().as_millis()));
                }
                Err(error) => {
                    set_error_message(&format!("Couldn't start {}.", core.text()), &error);
                    self.inserted = false;
                    self.requested_uri = None;
                }
            }
        } else {
            set_message("Choose your ROM folder first.".into());
            self.inserted = false;
            self.requested_uri = None;
        }
    }

    /// A row label at `scale` (fit_scales), centred on the row.
    fn text_scaled(&self, index: usize, x: f32, y: f32, scale: f32,
                   alpha: f32, out: &mut Vec<Draw>) {
        let (tex, raw_w, raw_h) = self.menu_textures[index];
        if raw_w == 0 || raw_h == 0 { return; }
        let scale = scale.min((out_w() as f32 - 70.0) / raw_w as f32);
        out.push(Draw::Tex {
            x, y: y + raw_h as f32 * (0.79 - scale) / 2.0,
            w: raw_w as f32 * scale, h: raw_h as f32 * scale,
            tex, alpha,
        });
    }

    fn text_fit(&self, index: usize, x: f32, y: f32, max_w: f32,
                alpha: f32, out: &mut Vec<Draw>) {
        let (tex, raw_w, raw_h) = self.menu_textures[index];
        // Keep original Slot typography, but fit long settings labels without
        // horizontally stretching them or overflowing a 3:2 display.
        if raw_w == 0 || raw_h == 0 { return; }
        let scale = 0.79f32.min(max_w / raw_w as f32);
        out.push(Draw::Tex {
            x, y, w: raw_w as f32 * scale, h: raw_h as f32 * scale,
            tex, alpha,
        });
    }

    fn prepare_about(&mut self) {
        if self.about_texture.is_none() {
            let sticker = about_sticker(&StickerFields {
                battery: self.battery_percent, serial: "0000130", dirty_digit: '0',
            });
            self.about_texture = Some(self.gpu.create_face(&sticker));
        }
    }

    /// The selected cart is a GBA Video one (game code M...).
    fn playing_video(&self) -> bool {
        self.shelves[self.active].carts.get(self.shelves[self.active].index)
            .is_some_and(carousel_delete::is_gba_video)
    }
    /// The global Settings the selected cart starts from: a GBA Video
    /// cart's own Screen in place of the GBA one.
    fn cart_globals(&self) -> crate::settings::Settings {
        if self.playing_video() { return self.settings.as_gba_video(); }
        let mut globals = self.settings.clone();
        if self.pokedaisy_running() { globals.apply_pokedaisy_defaults(self.current_cart_platform()); }
        globals
    }
    /// Whether the running game's system has Multiplayer at all.
    fn multiplayer_offered(&self) -> bool {
        multiplayer_offered(self.current_cart_platform()) && !self.pokedaisy_running()
            && self.game.as_ref().map_or(true, |game| core_has_multiplayer(game.core_kind()))
    }

    /// The Game Menu's rows for the game played.
    fn game_menu_rows(&self) -> &'static [PauseRow] {
        let linked = self.game.as_ref().is_some_and(GameSession::linked);
        let relaunch = self.game.as_ref().is_some_and(GameSession::requires_relaunch);
        pause_rows_with(linked, relaunch, self.multiplayer_offered(),
            cheats_offered(self.current_cart_platform()))
    }

    /// The cart played is an American release: (USA) or (U) in its name,
    /// or a TurboGrafx-16 folder.
    fn american_cart(&self) -> bool {
        self.shelves[self.active].carts.get(self.shelves[self.active].index)
            .is_some_and(|cart| video_mode::american_name(&cart.stem)
                || cart.rom.to_string_lossy().to_ascii_lowercase().contains("tg16"))
    }

    fn current_cart_platform(&self) -> Platform {
        self.shelves[self.active].carts
            .get(self.shelves[self.active].index)
            .map_or(Platform::Gba, |cart| cart.platform)
    }

    /// Whether Integer and Original Ratio are the same picture for the
    /// game as it is now: a home console by its own frame and shape (a
    /// title screen, a PAL game or a 320-wide one can differ from its
    /// system's usual picture), a handheld by its system's.
    fn game_integer_is_ratio(&self) -> bool {
        let (platform, panel) = (self.current_cart_platform(), panel_size());
        match self.game.as_ref() {
            Some(session) if video_mode::home_console(platform) => video_mode::home_same_picture(platform,
                session.display_aspect(), session.frame_size(), self.game_square(), panel),
            _ => video_mode::integer_is_ratio(platform, panel),
        }
    }

    /// The screen [session]'s game starts with: its screen size, chosen
    /// once from the core's first geometry (later frame changes update the
    /// fit, not the size), PokeDaisy's view and Intro. Reads the game's
    /// files.
    fn screen_settings(&self, session: &GameSession) -> ScreenReady {
        let name = self.shelves[self.active].carts
            .get(self.shelves[self.active].index)
            .map_or("", |cart| cart.stem.as_str());
        let platform = self.current_cart_platform();
        let square = video_mode::square_game(platform, panel_size(), session.square_listed());
        let frame = video_mode::launch_frame(platform, session.frame_size(), square, session.fps < 55.0);
        let paths = PATHS.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let root = paths.as_ref().map(|(root, _)| root.as_path());
        let mode = match root {
            None => video_mode::game_default_mode(platform, panel_size(), square, frame)
                .at_launch(platform, panel_size()),
            Some(root) => video_mode::video_mode_for(root, name, platform, panel_size(), square, frame),
        };
        let source = root.map_or("auto", |root| video_mode::video_mode_source(root, name, platform));
        let view = root.map_or_else(Default::default, |root| crate::pokedaisy_view::view_for(root, name));
        let intro = root.is_none_or(|root| crate::pokedaisy_view::intro_expands(root, name));
        ScreenReady { mode, source, frame, square, view, intro }
    }

    /// While the cart is seated (3.2 perf): the coming game's screen
    /// settings are read, and its shader compiled and its pictures (paper,
    /// dot matrix) loaded, so its screen goes on without that work. Nothing
    /// is applied: the screen still goes on as before (apply_video_geometry).
    fn ready_coming_screen(&mut self) {
        if self.screen_ready.is_some() { return; }
        let Some(session) = self.pending_game.as_ref() else { return };
        let started = Instant::now();
        let ready = self.screen_settings(session);
        let pokedaisy = session.core_kind() == Core::PokeDaisy;
        let effect = self.game_effect_for(ready.mode, pokedaisy);
        let lcd = if pokedaisy { crate::settings::POKEDAISY_LCD_EFFECT }
            else { self.game_lcd_effect(self.current_cart_platform()) };
        self.screen_ready = Some(ready);
        use slot_gfx::ScreenEffect as E;
        if matches!(effect, E::Simpletex | E::SimpletexGba) && !self.gpu.has_paper() {
            if let Some((size, rgba)) = shader_texture(PAPER_PNG) { self.gpu.set_paper(size, &rgba); }
        }
        if matches!(effect, E::DotMatrix | E::DotMatrixColour) && !self.gpu.has_dot_background() {
            if let Some((size, rgba)) = shader_texture(DOT_MATRIX_PNG) { self.gpu.set_dot_background(size, &rgba); }
        }
        self.gpu.prepare_screen_effect(effect, lcd.gpu());
        crate::game::log_launch_timing(&format!(
            "Coming screen ready while seated: {}ms ({effect:?})", started.elapsed().as_millis()));
    }

    fn apply_video_geometry(&mut self) {
        let platform = self.current_cart_platform();
        if let Some(frame) = self.game.as_ref()
            .filter(|session| session.core_kind() == Core::PokeDaisy)
            .map(GameSession::frame_size) {
            // PokeDaisy draws its own canvas at the panel's shape (EXPAND,
            // the companion): the whole screen, no bezel.
            self.gpu.set_picture(slot_gfx::WHOLE_TEXTURE);
            self.gpu.set_game_source_rect(slot_gfx::WHOLE_TEXTURE);
            self.gpu.set_game_fit(video_mode::pokedaisy_fit(self.pokedaisy_view, platform,
                self.video_mode, frame));
            self.gpu.set_game_top(false);
            self.apply_game_look();
            return;
        }
        if !platform.handheld() {
            let aspect = self.game.as_ref().map_or(4.0 / 3.0, GameSession::display_aspect);
            self.gpu.set_picture(slot_gfx::WHOLE_TEXTURE);
            self.gpu.set_game_source_rect(slot_gfx::WHOLE_TEXTURE);
            let frame = self.game.as_ref().map_or((256, 224), GameSession::frame_size);
            self.gpu.set_game_fit(video_mode::home_fit(platform, self.video_mode, aspect,
                frame, self.game_square()));
            self.gpu.set_game_top(self.picture_top);
            self.apply_game_look();
            return;
        }
        self.gpu.set_game_fit(slot_gfx::Fit::Canvas);
        let (w, h) = platform.picture();
        let x = (slot_gfx::SRC_W - w) as f32 / (2 * slot_gfx::SRC_W) as f32;
        let y = (slot_gfx::SRC_H - h) as f32 / (2 * slot_gfx::SRC_H) as f32;
        let x2 = 1.0 - x;
        let y2 = 1.0 - y;
        self.gpu.set_picture([x, y, x2, y2]);
        if fitted_canvas() {
            // Off the 3:2 KONKR each size is laid out on the real panel.
            let panel = panel_size();
            let [px, py, pw, ph] = video_mode::picture_rect(platform, self.video_mode, panel);
            let py = if self.picture_top { 0.0 } else { py };
            let (sx, sy) = (slot_gfx::out_w() as f32 / panel.0 as f32,
                OUT_H as f32 / panel.1 as f32);
            self.gpu.set_game_area([px * sx, py * sy, pw * sx, ph * sy]);
            self.gpu.set_game_source_rect(video_mode::picture_source(platform));
        } else {
            self.gpu.set_game_area(slot_gfx::default_area());
            self.gpu.set_game_source_rect(video_mode::source_rect(platform, self.video_mode));
        }
        self.apply_game_look();
    }

    /// Square pixels for the game played (video_mode::square_game).
    fn game_square(&self) -> bool {
        video_mode::square_game(self.current_cart_platform(), panel_size(),
            self.game.as_ref().is_some_and(GameSession::square_listed))
    }

    fn set_video_mode(&mut self, mode: VideoMode) {
        if self.video_mode == mode
            || (self.current_cart_platform() == Platform::Gba && !fitted_canvas()) {
            return;
        }
        self.video_mode = mode;
        self.apply_video_geometry();
        let name = self.shelves[self.active].carts
            .get(self.shelves[self.active].index)
            .map_or("", |cart| cart.stem.as_str());
        if let Some((root, _)) = PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            if let Err(err) = video_mode::write_video_mode(root, name, mode) {
                set_error_message("Couldn't save the screen size.", &err);
            }
        }
    }

    /// Merge this ROM+core's optional visual/gameplay overrides on top of
    /// globally persisted Settings without mutating them or another ROM.
    fn game_settings(&self) -> crate::settings::Settings {
        let mut effective = self.cart_globals();
        if let Some(profile) = self.game_emulation {
            profile.apply_visual_and_gameplay(&mut effective, self.current_cart_platform());
        }
        effective
    }
    /// Frame Blending, LCD Effect and Dither Blending of the game played:
    /// its own when changed in game, else the system's (Siham 2026-11-10).
    /// Read every frame, so without cloning the settings.
    fn game_blending(&self, platform: Platform) -> crate::settings::Blending {
        if self.playing_video() { return crate::settings::Blending::Off; }
        self.game_emulation.and_then(|p| p.blending_override)
            .unwrap_or_else(|| if self.pokedaisy_running() { crate::settings::Blending::Off }
                else { self.settings.blending_for(platform) })
    }
    fn game_lcd_effect(&self, platform: Platform) -> crate::settings::LcdEffect {
        if Settings::lcd_effect_key(platform).is_none() || self.playing_video() {
            return crate::settings::LcdEffect::Off;
        }
        self.game_emulation.and_then(|p| p.lcd_effect_override)
            .unwrap_or_else(|| if self.pokedaisy_running() { crate::settings::POKEDAISY_LCD_EFFECT }
                else { self.settings.lcd_effect_for(platform) })
    }
    fn game_dither(&self) -> bool {
        self.game_emulation.and_then(|p| p.dither_override)
            .unwrap_or(self.settings.dither_blend_md)
    }
    fn live_game_options(&self) -> LiveGameOptions {
        LiveGameOptions::from_profile(&self.settings, self.game_emulation)
    }
    fn game_shader(&self) -> crate::settings::Shader {
        self.game_settings().shader_for(self.current_cart_platform())
    }
    /// The game's shader and what it needs: Simpletex's paper, Dot
    /// Matrix's grain and the game's palette, the system's LCD Effect
    /// (with the accelerometer for Sunlight Shimmer).
    fn apply_game_look(&mut self) {
        use slot_gfx::ScreenEffect as E;
        let effect = self.game_effect();
        if matches!(effect, E::Simpletex | E::SimpletexGba) && !self.gpu.has_paper() {
            if let Some((size, rgba)) = shader_texture(PAPER_PNG) { self.gpu.set_paper(size, &rgba); }
        }
        if matches!(effect, E::DotMatrix | E::DotMatrixColour) {
            if !self.gpu.has_dot_background() {
                if let Some((size, rgba)) = shader_texture(DOT_MATRIX_PNG) {
                    self.gpu.set_dot_background(size, &rgba);
                }
            }
            self.gpu.set_dot_palette(dot_matrix_colours(&self.game_settings()));
        }
        let upscaler = if self.playing_video() {
            self.game_settings().upscaler_gbav.gpu()
        } else { slot_gfx::Upscaler::Off };
        self.gpu.set_upscaler(upscaler);
        self.gpu.set_screen_effect(effect);
        let lcd = if self.pokedaisy_menu && self.pokedaisy_running() {
            crate::settings::LcdEffect::Off
        } else { self.game_lcd_effect(self.current_cart_platform()) };
        self.gpu.set_lcd_effect(lcd.gpu());
        // A home console's Vignette and Curvature, over every shader.
        let crt = if self.playing_video() || self.pokedaisy_running() { (0, 0) } else {
            let (view, platform) = (self.game_settings(), self.current_cart_platform());
            (view.crt_look(platform, false), view.crt_look(platform, true))
        };
        self.gpu.set_crt(f32::from(crt.0) / 10.0, f32::from(crt.1) / 10.0);
        let sunlight = lcd == crate::settings::LcdEffect::Sunlight && self.game.is_some();
        if sunlight != self.tilt_on {
            self.tilt_on = sunlight;
            self.tilt.reset();
            self.gpu.set_light(0.5, 0.5);
            slot_retro::sensors::set_tilt_wanted(sunlight);
        }
    }
    fn game_effect(&self) -> slot_gfx::ScreenEffect {
        self.game_effect_for(self.video_mode, self.pokedaisy_running())
    }

    /// The game's shader in [mode], PokeDaisy's or another core's.
    fn game_effect_for(&self, mode: VideoMode, pokedaisy: bool) -> slot_gfx::ScreenEffect {
        // PokeDaisy's companion and quick menu: its own drawing, no shader.
        // Its canvas is a whole multiple of the screen on most panels, not
        // all: Sharp Bilinear keeps every pixel the same size (the same
        // picture where the multiple is whole).
        if pokedaisy {
            let effect = video_mode::screen_effect(self.game_shader(),
                self.current_cart_platform(), mode);
            return if self.pokedaisy_menu || effect == slot_gfx::ScreenEffect::None {
                slot_gfx::ScreenEffect::SharpBilinear
            } else { effect };
        }
        let effect = video_mode::screen_effect(self.game_shader(),
            self.current_cart_platform(), mode);
        // Off the KONKR Original Ratio is rarely a whole multiple of the
        // picture: keep every pixel the same size, as on the KONKR.
        let platform = self.current_cart_platform();
        let uneven = fitted_canvas() && mode != VideoMode::Stretch
            && !video_mode::whole_scale(platform,
                video_mode::picture_rect(platform, mode, panel_size()));
        // An upscaled GBA Video picture is already smooth.
        let upscaled = self.playing_video()
            && self.game_settings().upscaler_gbav != crate::settings::Upscaler::Off;
        if effect == slot_gfx::ScreenEffect::None && !upscaled
            && (uneven || !whole_game_scale(panel_size())) {
            slot_gfx::ScreenEffect::SharpBilinear
        } else { effect }
    }

    /// The top banner naming the new screen size.
    /// One screen size up (Integer -> Original Ratio -> Stretch) or down,
    /// with its banner. GBA on the KONKR has a single size.
    fn step_screen_size(&mut self, bigger: bool) {
        if self.pokedaisy_running() {
            self.step_pokedaisy_view(bigger);
            return;
        }
        let same = self.game_integer_is_ratio();
        let mode = if bigger { self.video_mode.bigger_when(same) }
            else { self.video_mode.smaller_when(same) };
        if mode == self.video_mode { return; }
        self.set_video_mode(mode);
        if self.video_mode != mode { return; }
        self.hud.toast(self.video_mode_toast(), self.born.elapsed().as_millis() as u64);
        self.tell_hidden_bezel();
    }

    /// The game played runs on PokeDaisy.
    fn pokedaisy_running(&self) -> bool {
        self.game.as_ref().is_some_and(|session| session.core_kind() == Core::PokeDaisy)
    }

    /// ZOOM, EXPAND, ORIGINAL in place of the screen sizes: PokeDaisy lays
    /// out its own canvas. Kept for this game.
    fn step_pokedaisy_view(&mut self, up: bool) {
        use crate::pokedaisy_view::View;
        let steps = self.pokedaisy_zoom_steps();
        let shown = self.pokedaisy_view.within(steps);
        let view = shown.step(up, steps);
        if view == shown { return; }
        if let Some(game) = self.game.as_mut() { game.set_pokedaisy_view(view); }
        self.keep_pokedaisy_view(view);
        self.hud.toast(match view {
            View::Zoom(step) => Toast::ScreenZoom(step),
            View::Expand => Toast::ScreenExpand,
            View::Original => Toast::ScreenOriginal,
        }, self.born.elapsed().as_millis() as u64);
        // A held game (the Screen page) shows it at once.
        if !matches!(self.overlay, ShelfOverlay::None) { self.redraw_paused_game(false); }
    }

    /// The view this game shows from now on, kept for it (from SELECT +
    /// Up / Down, the Screen page or PokeDaisy's companion).
    fn keep_pokedaisy_view(&mut self, view: crate::pokedaisy_view::View) {
        self.pokedaisy_view = view;
        let name = self.shelves[self.active].carts
            .get(self.shelves[self.active].index)
            .map_or("", |cart| cart.stem.as_str());
        if let Some((root, _)) = PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            if let Err(err) = crate::pokedaisy_view::write_view(root, name, view) {
                set_error_message("Couldn't save the screen size.", &err);
            }
        }
    }

    /// INTRO: ORIGINAL or EXPAND for this game's start-up screens, kept for
    /// it. The core takes it at its next frame.
    fn toggle_pokedaisy_intro(&mut self) {
        let expand = !self.pokedaisy_intro;
        self.pokedaisy_intro = expand;
        if let Some(game) = self.game.as_mut() { game.set_pokedaisy_intro(expand); }
        let name = self.shelves[self.active].carts
            .get(self.shelves[self.active].index)
            .map_or("", |cart| cart.stem.as_str());
        if let Some((root, _)) = PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            if let Err(err) = crate::pokedaisy_view::write_intro(root, name, expand) {
                set_error_message("Couldn't save the intro setting.", &err);
            }
        }
    }

    /// ZOOM's steps for the game played on this panel, as PokeDaisy has them.
    fn pokedaisy_zoom_steps(&self) -> u8 {
        crate::pokedaisy_view::zoom_steps(panel_size(),
            crate::pokedaisy_view::game_frame(self.current_cart_platform()))
    }

    fn video_mode_toast(&self) -> Toast {
        match self.video_mode {
            VideoMode::Actual => Toast::ScreenInteger,
            VideoMode::Aspect => Toast::ScreenRatio,
            VideoMode::Stretch => Toast::ScreenStretch,
        }
    }

    /// A short Android message when the chosen bezel can't frame this size.
    fn tell_hidden_bezel(&self) {
        if self.current_cart_platform() == Platform::Gba {return;}
        if !video_mode::bezel_offered(self.current_cart_platform(),panel_size()) {return;}
        let bezel=self.game_settings().bezel_for(self.current_cart_platform());
        if let Some(message)=video_mode::bezel_hidden_message(bezel,self.video_mode) {
            set_message(message.into());
        }
    }

    /// The home console's display aspect and frame size, for its overlay.
    fn home_shape(&self) -> (f32, (u32, u32)) {
        let platform = self.current_cart_platform();
        self.game.as_ref().map_or((4.0 / 3.0, (256, 224)), |session| (
            video_mode::home_aspect(platform, session.display_aspect(), session.frame_size(),
                self.game_square()),
            video_mode::logical_frame(platform, session.frame_size())))
    }

    /// Native 3:2 KONKR: GBA fills the panel and has no overlay. GB/GBC
    /// keep the official 720x480 slot. bezel; every system has Watomsk's
    /// overlays, placed by video_mode::watomsk_fit for the real panel (the
    /// picture moves to the top when his logo goes under it). A bezel that
    /// can't frame the screen size is hidden, and the setting is kept.
    /// Returns the texture and the canvas rect it covers.
    fn game_bezel(&mut self) -> Option<(slot_gfx::TexId, [f32; 4])> {
        use crate::settings::Bezel;
        use video_mode::WatomskFit;
        // PokeDaisy fills the screen itself: no bezel, no overlay.
        if self.game.as_ref().is_some_and(|session| session.core_kind() == Core::PokeDaisy) {
            return None;
        }
        let platform=self.current_cart_platform();
        // AUTO: the Neo Geo Pocket Color or WonderSwan Color logo for a
        // Color game, the plain one otherwise.
        let colour=self.game.as_ref().is_some_and(GameSession::colour);
        let bezel=match self.game_settings().bezel_for(platform) {
            Bezel::NgpAuto=>if colour {Bezel::NgpColor} else {Bezel::Ngp},
            Bezel::WsAuto=>if colour {Bezel::WsColor} else {Bezel::Ws},
            // PC Engine AUTO: TurboGrafx-16 for an American HuCard.
            Bezel::PceAuto=>if self.american_cart() {Bezel::TurboGrafx} else {Bezel::PcEngine},
            bezel=>bezel,
        };
        let panel=panel_size();
        let fit=video_mode::watomsk_fit(bezel,platform,self.video_mode,panel,self.home_shape());
        let top=matches!(fit,Some(WatomskFit::Band(_)|WatomskFit::Own(slot_ui::own_overlay::Place::Band,_)));
        if top!=self.picture_top {
            self.picture_top=top;
            self.apply_video_geometry();
        }
        let whole=[0.0,0.0,slot_gfx::out_w() as f32,OUT_H as f32];
        let rect=|r: [f32; 4]| r.map(|v| v.round() as i32);
        let (key,rgba)=if bezel==Bezel::Default {
            if !video_mode::bezel_fits(bezel,self.video_mode) ||
                !matches!(platform,Platform::Gb|Platform::Gbc) {return None;}
            if !fitted_canvas() {
                let tex=if platform==Platform::Gb {self.gb_bezel_tex} else {self.gbc_bezel_tex};
                // Bezels frame the 3:2 game frame, wherever it sits.
                return Some((tex,slot_gfx::default_area()));
            }
            if !video_mode::bezel_has_room(bezel,platform,panel) {return None;}
            let window=video_mode::bezel_window(bezel,platform,panel);
            let key=(bezel,platform,panel,rect(window));
            if let Some((tex,fitted))=self.fitted_bezel {
                if fitted==key {return Some((tex,whole));}
            }
            use slot_ui::{bitmaps, fit_overlay::{fit_overlay, slot_bezel_sized}};
            let (master,mw,mh)=slot_ui::decode_png(bitmaps::bezel_master(platform==Platform::Gbc))?;
            (key,fit_overlay(&slot_bezel_sized(&master,(mw,mh)),panel,window))
        } else {
            let fit=fit?;
            let key=(bezel,platform,panel,rect(match fit {
                WatomskFit::Side(_,r)|WatomskFit::Band(r)|WatomskFit::Own(_,r)=>r,
            }));
            // On the KONKR the 3:2 game frame is the whole panel.
            let area=if fitted_canvas() {whole} else {slot_gfx::default_area()};
            if let Some((tex,fitted))=self.fitted_bezel {
                if fitted==key {return Some((tex,area));}
            }
            use slot_ui::{watomsk, own_overlay, fit_overlay::{fit_band, fit_overlay}};
            let rgba=match fit {
                WatomskFit::Side(shape,window)=>{
                    let side=watomsk::side(bezel.watomsk()?,shape)?;
                    fit_overlay(&side.art(),panel,window)
                }
                WatomskFit::Band(pic)=>{
                    let band=watomsk::band(bezel.watomsk()?)?;
                    fit_band(&band.art(),panel,pic)
                }
                WatomskFit::Own(place,pic)=>own_overlay::draw(bezel.own(colour)?,place,panel,pic)?,
            };
            let tex=self.bezel_texture(panel,&rgba);
            self.fitted_bezel=Some((tex,key));
            return Some((tex,area));
        };
        let tex=self.bezel_texture(panel,&rgba);
        self.fitted_bezel=Some((tex,key));
        Some((tex,whole))
    }

    fn bezel_texture(&mut self, panel: (u32, u32), rgba: &[u8]) -> slot_gfx::TexId {
        match self.fitted_bezel {
            Some((tex,_)) => { self.gpu.update_texture(tex,panel.0,panel.1,rgba); tex }
            None => self.gpu.create_texture(panel.0,panel.1,rgba),
        }
    }

    // The Bezel row shows only where an overlay can appear on this panel
    // (video_mode::bezel_offered): not GBA on the KONKR, not the Game Boys
    // on near-square screens, not NES/SNES on 4:3 or narrower.
    fn screen_rows(platform: Platform)-> &'static [usize] {
        Self::screen_rows_on(platform,panel_size())
    }
    fn screen_rows_on(platform: Platform, panel: (u32,u32))-> &'static [usize] {
        let bezel=video_mode::bezel_offered(platform,panel);
        // A system whose shape the panel already has offers no Screen Size,
        // in game as on its Settings page (Siham 2026-11-21).
        let fits=video_mode::screen_fits(platform,panel);
        match (platform,bezel) {
            (Platform::Snes|Platform::Md|Platform::Ws|Platform::Lynx,false) if fits=>&[124],
            (Platform::Snes|Platform::Md|Platform::Ws|Platform::Lynx,true) if fits=>&[124,90],
            // The Pokemon mini has no Shader: its LCD Filter is the core's
            // own (Siham 2026-11-22), in the rows after these.
            (Platform::Pokemini,false) if fits=>&[],
            (Platform::Pokemini,true) if fits=>&[90],
            (Platform::Pokemini,false)=>&[120],
            (Platform::Pokemini,true)=>&[120,90],
            (Platform::Gba,false)=>&[34,29],
            (Platform::Gba,true)=>&[34,29,90],
            (Platform::Gbc,false)=>&[88,29],
            (Platform::Gbc,true)=>&[88,29,90],
            // No Color Correction row: a Game Boy picture is its palette,
            // the cores correct only GBC/GBA colours (Siham, 2026-11-04).
            (Platform::Gb,false)=>&[89,36],
            (Platform::Gb,true)=>&[89,36,90],
            // NES sizes with L1/R1 in game: no Screen Size row (Siham
            // 2026-11-06); SNES and Mega Drive games use L/R themselves.
            (Platform::Nes,false)=>&[124],
            (Platform::Nes,true)=>&[124,90],
            (Platform::Snes|Platform::Md,false)=>&[120,124],
            (Platform::Snes|Platform::Md,true)=>&[120,124,90],
            // Master System, Game Gear and Neo Geo Pocket size with L1/R1,
            // as the NES; the WonderSwan's L1/R1 are its Y buttons, the
            // Lynx's its Options, the Pokemon mini's Shake and C.
            (Platform::Sms|Platform::Gg|Platform::Ngp,false)=>&[124],
            (Platform::Sms|Platform::Gg|Platform::Ngp,true)=>&[124,90],
            (Platform::Ws|Platform::Lynx,false)=>&[120,124],
            (Platform::Ws|Platform::Lynx,true)=>&[120,124,90],
            // PICO-8: no overlay, so no Bezel row.
            (Platform::Pico8,_) if fits=>&[124],
            (Platform::Pico8,_)=>&[120,124],
            // Neo Geo as the Mega Drive, PC Engine as the SNES (Siham
            // 2026-11-27): Screen Size unless the panel fits, then Shader.
            (Platform::NeoGeo|Platform::Pce,false) if fits=>&[124],
            (Platform::NeoGeo|Platform::Pce,true) if fits=>&[124,90],
            (Platform::NeoGeo|Platform::Pce,false)=>&[120,124],
            (Platform::NeoGeo|Platform::Pce,true)=>&[120,124,90],
        }
    }
    fn screen_keys(platform: Platform)-> &'static [usize] {
        Self::screen_keys_on(platform,panel_size())
    }
    fn screen_keys_on(platform: Platform, panel: (u32,u32))-> &'static [usize] {
        let bezel=video_mode::bezel_offered(platform,panel);
        let fits=video_mode::screen_fits(platform,panel);
        match (platform,bezel) {
            (Platform::Snes,false) if fits=>&[48],
            (Platform::Md,false) if fits=>&[49],
            (Platform::Snes,true) if fits=>&[48,45],
            (Platform::Md,true) if fits=>&[49,46],
            (Platform::Ws,false) if fits=>&[SHADER_WS_KEY],
            (Platform::Ws,true) if fits=>&[SHADER_WS_KEY,BEZEL_WS_KEY],
            (Platform::Lynx,false) if fits=>&[SHADER_LYNX_KEY],
            (Platform::Lynx,true) if fits=>&[SHADER_LYNX_KEY,BEZEL_LYNX_KEY],
            (Platform::Pokemini,false) if fits=>&[],
            (Platform::Pokemini,true) if fits=>&[BEZEL_POKEMINI_KEY],
            (Platform::Gba,false)=>&[0,22],
            (Platform::Gba,true)=>&[0,22,25],
            (Platform::Gbc,false)=>&[21,23],
            (Platform::Gbc,true)=>&[21,23,26],
            (Platform::Gb,false)=>&[1,3],
            (Platform::Gb,true)=>&[1,3,27],
            (Platform::Nes,false)=>&[47],
            (Platform::Snes,false)=>&[SCREEN_SIZE_KEY,48],
            (Platform::Md,false)=>&[SCREEN_SIZE_KEY,49],
            (Platform::Nes,true)=>&[47,44],
            (Platform::Snes,true)=>&[SCREEN_SIZE_KEY,48,45],
            (Platform::Md,true)=>&[SCREEN_SIZE_KEY,49,46],
            (Platform::Sms,false)=>&[SHADER_SMS_KEY],
            (Platform::Sms,true)=>&[SHADER_SMS_KEY,BEZEL_SMS_KEY],
            (Platform::Gg,false)=>&[SHADER_GG_KEY],
            (Platform::Gg,true)=>&[SHADER_GG_KEY,BEZEL_GG_KEY],
            (Platform::Ngp,false)=>&[SHADER_NGP_KEY],
            (Platform::Ngp,true)=>&[SHADER_NGP_KEY,BEZEL_NGP_KEY],
            (Platform::Ws,false)=>&[SCREEN_SIZE_KEY,SHADER_WS_KEY],
            (Platform::Ws,true)=>&[SCREEN_SIZE_KEY,SHADER_WS_KEY,BEZEL_WS_KEY],
            (Platform::Lynx,false)=>&[SCREEN_SIZE_KEY,SHADER_LYNX_KEY],
            (Platform::Lynx,true)=>&[SCREEN_SIZE_KEY,SHADER_LYNX_KEY,BEZEL_LYNX_KEY],
            (Platform::Pokemini,false)=>&[SCREEN_SIZE_KEY],
            (Platform::Pokemini,true)=>&[SCREEN_SIZE_KEY,BEZEL_POKEMINI_KEY],
            (Platform::Pico8,_) if fits=>&[SHADER_PICO8_KEY],
            (Platform::Pico8,_)=>&[SCREEN_SIZE_KEY,SHADER_PICO8_KEY],
            (Platform::NeoGeo,false) if fits=>&[SHADER_NEOGEO_KEY],
            (Platform::NeoGeo,true) if fits=>&[SHADER_NEOGEO_KEY,BEZEL_NEOGEO_KEY],
            (Platform::Pce,false) if fits=>&[SHADER_PCE_KEY],
            (Platform::Pce,true) if fits=>&[SHADER_PCE_KEY,BEZEL_PCE_KEY],
            (Platform::NeoGeo,false)=>&[SCREEN_SIZE_KEY,SHADER_NEOGEO_KEY],
            (Platform::NeoGeo,true)=>&[SCREEN_SIZE_KEY,SHADER_NEOGEO_KEY,BEZEL_NEOGEO_KEY],
            (Platform::Pce,false)=>&[SCREEN_SIZE_KEY,SHADER_PCE_KEY],
            (Platform::Pce,true)=>&[SCREEN_SIZE_KEY,SHADER_PCE_KEY,BEZEL_PCE_KEY],
        }
    }
    // Settings > Systems opens with the system's default core, then its
    // Screen Size, then the screen rows a paused game of that system also
    // shows. A system whose shape the panel already has (GBA on the KONKR)
    // offers neither Screen Size nor a bezel (Siham 2026-11-02).
    /// The rows after a system's own ones: LCD Effect (Game Boys) or
    /// Dither Blending (Mega Drive), then Frame Blending, as row labels
    /// and Settings keys.
    fn look_rows(platform: Platform) -> (Vec<usize>, Vec<usize>) {
        let mut rows = Vec::new();
        let mut keys = Vec::new();
        // Vignette and Curvature, moved under Shader (crt_after_shader).
        if let (Some(vignette), Some(curve)) = (Settings::crt_key(platform, false), Settings::crt_key(platform, true)) {
            rows.extend([187, 188]);
            keys.extend([vignette, curve]);
        }
        if let Some(key) = Settings::look_key(platform) {
            rows.push(if platform == Platform::Md { 132 } else { 131 });
            keys.push(key);
        }
        rows.push(126);
        keys.push(Settings::blending_key(platform));
        (rows, keys)
    }
    /// The 3.0 systems' own core options (3.0.0-materials3), as row
    /// labels and Settings keys. `in_game` leaves out what needs the game
    /// restarted (the Neo Geo Pocket's language).
    fn extra_rows(platform: Platform, in_game: bool) -> (Vec<usize>, Vec<usize>) {
        use crate::settings::{SPRITE_LIMIT_SMS_KEY, SPRITE_LIMIT_GG_KEY, NGP_LANGUAGE_KEY,
            WS_PALETTE_KEY, POKEMINI_PALETTE_KEY, POKEMINI_LCD_MODE_KEY, LEFT_BAR_SMS_KEY,
            FM_SMS_KEY, WS_LANGUAGE_KEY, POKEMINI_SHAKE_KEY, LEFT_BAR_NES_KEY,
            NES_AUDIO_FILTER_KEY, MD_FM_SOUND_KEY, WS_AUDIO_FILTER_KEY};
        // What changes the picture shows in game too (Siham 2026-11-22);
        // sound and language, read when the game starts, only here.
        let (mut rows, mut keys): (Vec<usize>, Vec<usize>) = match platform {
            Platform::Sms => (vec![164, 168], vec![SPRITE_LIMIT_SMS_KEY, LEFT_BAR_SMS_KEY]),
            Platform::Gg => (vec![164], vec![SPRITE_LIMIT_GG_KEY]),
            Platform::Nes => (vec![175], vec![LEFT_BAR_NES_KEY]),
            Platform::Ws => (vec![171], vec![WS_PALETTE_KEY]),
            Platform::Pokemini => (vec![163, 172, 166, 170],
                vec![POKEMINI_FILTER_KEY, POKEMINI_PALETTE_KEY, POKEMINI_LCD_MODE_KEY, POKEMINI_SHAKE_KEY]),
            _ => (Vec::new(), Vec::new()),
        };
        if !in_game {
            match platform {
                Platform::Sms => { rows.push(169); keys.push(FM_SMS_KEY); }
                Platform::Nes => { rows.push(176); keys.push(NES_AUDIO_FILTER_KEY); }
                Platform::Md => { rows.push(169); keys.push(MD_FM_SOUND_KEY); }
                Platform::Ngp => { rows.push(165); keys.push(NGP_LANGUAGE_KEY); }
                // Read when the game starts (Siham 2026-11-27).
                Platform::NeoGeo => {
                    rows.push(184); keys.push(crate::settings::NEOGEO_BIOS_KEY);
                    rows.push(185); keys.push(crate::settings::NEOGEO_REGION_KEY);
                }
                Platform::Pce => { rows.push(186); keys.push(crate::settings::PCE_PAD_KEY); }
                Platform::Ws => {
                    rows.push(165); keys.push(WS_LANGUAGE_KEY);
                    rows.push(176); keys.push(WS_AUDIO_FILTER_KEY);
                }
                _ => {}
            }
        }
        (rows, keys)
    }
    /// A system's page: its own rows, LCD Effect or Dither Blending, Frame
    /// Blending, its own options, then Controls (a page).
    fn system_page_rows(platform: Platform) -> Vec<usize> {
        Self::system_page(platform).0
    }
    fn system_page_keys(platform: Platform) -> Vec<usize> {
        Self::system_page(platform).1
    }
    fn system_page(platform: Platform) -> (Vec<usize>, Vec<usize>) {
        let mut rows = Self::system_rows(platform).to_vec();
        let mut keys = Self::system_keys(platform).to_vec();
        rows.extend(Self::look_rows(platform).0);
        keys.extend(Self::look_rows(platform).1);
        rows.extend(Self::extra_rows(platform, false).0);
        keys.extend(Self::extra_rows(platform, false).1);
        // Bezel, even where the WonderSwan's overlay fits only its upright
        // games (Siham 2026-11-22).
        Self::palette_after_shader(&mut rows, &mut keys);
        Self::lcd_after_shader(platform, &mut rows, &mut keys);
        Self::crt_after_shader(platform, &mut rows, &mut keys);
        Self::blending_before_shake(&mut rows, &mut keys);
        rows.push(125);
        keys.push(usize::MAX);
        (rows, keys)
    }
    /// The in-game Screen page: its rows, then LCD Effect or Dither
    /// Blending, then Frame Blending.
    fn pause_screen_page_rows(&self) -> Vec<usize> {
        self.pause_screen_page().0
    }
    fn pause_screen_page_key(&self, row: usize) -> Option<usize> {
        self.pause_screen_page().1.get(row).copied()
    }
    /// The in-game Screen page's rows and their Settings keys, side by side.
    fn pause_screen_page(&self) -> (Vec<usize>, Vec<usize>) {
        let own = self.pause_screen_rows();
        let mut rows = Vec::new();
        let mut keys = Vec::new();
        for (row, &label) in own.iter().enumerate() {
            if let Some(key) = self.pause_screen_key(row) { rows.push(label); keys.push(key); }
        }
        // PokeDaisy's Intro, under Screen Size, while that is EXPAND or a
        // ZOOM step (ORIGINAL has no wide canvas to choose for).
        if self.pokedaisy_running() && self.pokedaisy_view.within(self.pokedaisy_zoom_steps())
            != crate::pokedaisy_view::View::Original {
            if let Some(at) = keys.iter().position(|&k| k == SCREEN_SIZE_KEY) {
                rows.insert(at + 1, 181); keys.insert(at + 1, POKEDAISY_INTRO_KEY);
            }
        }
        // A GBA Video cart has no LCD Effect and no Frame Blending.
        if !self.playing_video() {
            let platform = self.current_cart_platform();
            let (look_rows, look_keys) = Self::look_rows(platform);
            rows.extend(look_rows); keys.extend(look_keys);
            let (extra_rows, extra_keys) = Self::extra_rows(platform, true);
            rows.extend(extra_rows); keys.extend(extra_keys);
            // A 3.0 game's Bezel row only when this game has room for its
            // overlay: on the KPA a WonderSwan game held upright has, one
            // played across has none (Siham 2026-11-22).
            if matches!(platform, Platform::Sms | Platform::Gg | Platform::Ngp | Platform::Ws
                | Platform::Lynx | Platform::Pokemini) && !self.game_has_bezel_room() {
                while let Some(at) = keys.iter().position(|&k| Settings::bezel_platform(k).is_some()) {
                    rows.remove(at); keys.remove(at);
                }
            }
            Self::palette_after_shader(&mut rows, &mut keys);
            Self::lcd_after_shader(platform, &mut rows, &mut keys);
            Self::crt_after_shader(platform, &mut rows, &mut keys);
            Self::blending_before_shake(&mut rows, &mut keys);
        }
        (rows, keys)
    }
    /// The Pokemon mini's Frame Blending second to last, before Screen
    /// Shake (Siham 2026-11-22).
    fn blending_before_shake(rows: &mut Vec<usize>, keys: &mut Vec<usize>) {
        let Some(from) = keys.iter().position(|&k| k == Settings::blending_key(Platform::Pokemini))
            else { return };
        let Some(shake) = keys.iter().position(|&k| k == crate::settings::POKEMINI_SHAKE_KEY)
            else { return };
        if from > shake { return; }
        let (row, key) = (rows.remove(from), keys.remove(from));
        rows.insert(shake - 1, row); keys.insert(shake - 1, key);
    }
    /// Whether the game played has room on this screen for its system's own
    /// overlay (its own shape: a WonderSwan game is wide or upright).
    fn game_has_bezel_room(&self) -> bool {
        let (aspect, frame) = self.home_shape();
        video_mode::own_place(aspect, frame, VideoMode::Aspect, panel_size(),
            video_mode::crop_limit(self.current_cart_platform())).is_some()
    }
    /// WS Palette straight after the Shader row (Siham 2026-11-22); the
    /// Pokemon mini's PK Palette already follows its LCD Filter.
    fn palette_after_shader(rows: &mut Vec<usize>, keys: &mut Vec<usize>) {
        let Some(from) = keys.iter().position(|&k| k == crate::settings::WS_PALETTE_KEY) else { return };
        let Some(shader) = keys.iter().position(|&k| k == SHADER_WS_KEY) else { return };
        let (row, key) = (rows.remove(from), keys.remove(from));
        let to = if from < shader { shader } else { shader + 1 };
        rows.insert(to, row); keys.insert(to, key);
    }
    /// The new LCD handhelds put LCD Effect directly after Shader, before
    /// the optional bezel and (on WonderSwan) the palette. Keep the original
    /// Game Boy and Mega Drive layouts unchanged.
    /// A home console's Vignette and Curvature straight under its Shader
    /// (Siham 2026-11-27).
    fn crt_after_shader(platform: Platform, rows: &mut Vec<usize>, keys: &mut Vec<usize>) {
        let Some(shader) = Settings::home_shader_key(platform) else { return };
        for curve in [false, true] {
            let Some(key) = Settings::crt_key(platform, curve) else { return };
            let Some(from) = keys.iter().position(|&k| k == key) else { continue };
            let (row, key) = (rows.remove(from), keys.remove(from));
            let Some(at) = keys.iter().position(|&k| k == shader) else {
                rows.insert(from, row); keys.insert(from, key); continue
            };
            let to = at + 1 + usize::from(curve);
            rows.insert(to.min(rows.len()), row); keys.insert(to.min(keys.len()), key);
        }
    }
    fn lcd_after_shader(platform: Platform, rows: &mut Vec<usize>, keys: &mut Vec<usize>) {
        let shader = match platform {
            Platform::Gg => SHADER_GG_KEY, Platform::Ngp => SHADER_NGP_KEY,
            Platform::Ws => SHADER_WS_KEY, Platform::Lynx => SHADER_LYNX_KEY,
            _ => return,
        };
        let Some(lcd) = Settings::lcd_effect_key(platform) else { return };
        let Some(from) = keys.iter().position(|&k| k == lcd) else { return };
        let Some(at) = keys.iter().position(|&k| k == shader) else { return };
        let (row, key) = (rows.remove(from), keys.remove(from));
        let to = if from < at { at } else { at + 1 };
        rows.insert(to, row); keys.insert(to, key);
    }
    fn system_rows(platform: Platform)-> &'static [usize] {
        Self::system_rows_on(platform,panel_size())
    }
    fn system_rows_on(platform: Platform, panel: (u32,u32))-> &'static [usize] {
        let fits=video_mode::screen_fits(platform,panel);
        let bezel=video_mode::bezel_offered(platform,panel);
        match (platform,fits,bezel) {
            (Platform::Gba,true,_)=>&[118,34,29],
            (Platform::Gba,false,false)=>&[118,120,34,29],
            (Platform::Gba,false,true)=>&[118,120,34,29,90],
            (Platform::Gbc,true,_)=>&[118,88,29],
            (Platform::Gbc,false,false)=>&[118,120,88,29],
            (Platform::Gbc,false,true)=>&[118,120,88,29,90],
            (Platform::Gb,true,_)=>&[118,89,36],
            (Platform::Gb,false,false)=>&[118,120,89,36],
            (Platform::Gb,false,true)=>&[118,120,89,36,90],
            (Platform::Nes|Platform::Snes|Platform::Md,true,_)=>&[118,124],
            (Platform::Nes|Platform::Snes|Platform::Md,false,false)=>&[118,120,124],
            (Platform::Nes|Platform::Snes|Platform::Md,false,true)=>&[118,120,124,90],
            (Platform::Sms|Platform::Gg|Platform::Ngp|Platform::Lynx,true,_)=>&[118,124],
            (Platform::Sms|Platform::Gg|Platform::Ngp|Platform::Lynx,false,false)=>&[118,120,124],
            (Platform::Sms|Platform::Gg|Platform::Ngp|Platform::Lynx,false,true)=>&[118,120,124,90],
            // One core only: no Default Core row.
            (Platform::Ws,true,_)=>&[124],
            (Platform::Ws,false,false)=>&[120,124],
            (Platform::Ws,false,true)=>&[120,124,90],
            // No Shader: its LCD Filter follows, with its own options.
            (Platform::Pokemini,true,_)=>&[],
            (Platform::Pokemini,false,false)=>&[120],
            (Platform::Pokemini,false,true)=>&[120,90],
            // One core, no overlay.
            (Platform::Pico8,true,_)=>&[118,124],
            (Platform::Pico8,false,_)=>&[118,120,124],
            // The Neo Geo's core is the file's (FinalBurn Neo a .zip,
            // Geolith a .neo): no Default Core row (Siham 2026-11-27).
            (Platform::NeoGeo,true,false)=>&[124],
            (Platform::NeoGeo,true,true)=>&[124,90],
            (Platform::NeoGeo,false,false)=>&[120,124],
            (Platform::NeoGeo,false,true)=>&[120,124,90],
            (Platform::Pce,true,false)=>&[118,124],
            (Platform::Pce,true,true)=>&[118,124,90],
            (Platform::Pce,false,false)=>&[118,120,124],
            (Platform::Pce,false,true)=>&[118,120,124,90],
        }
    }
    fn system_keys(platform: Platform)-> &'static [usize] {
        Self::system_keys_on(platform,panel_size())
    }
    fn system_keys_on(platform: Platform, panel: (u32,u32))-> &'static [usize] {
        let fits=video_mode::screen_fits(platform,panel);
        let bezel=video_mode::bezel_offered(platform,panel);
        match (platform,fits,bezel) {
            (Platform::Gba,true,_)=>&[29,0,22],
            (Platform::Gba,false,false)=>&[29,SCREEN_SIZE_KEY,0,22],
            (Platform::Gba,false,true)=>&[29,SCREEN_SIZE_KEY,0,22,25],
            (Platform::Gbc,true,_)=>&[30,21,23],
            (Platform::Gbc,false,false)=>&[30,SCREEN_SIZE_KEY,21,23],
            (Platform::Gbc,false,true)=>&[30,SCREEN_SIZE_KEY,21,23,26],
            (Platform::Gb,true,_)=>&[31,1,3],
            (Platform::Gb,false,false)=>&[31,SCREEN_SIZE_KEY,1,3],
            (Platform::Gb,false,true)=>&[31,SCREEN_SIZE_KEY,1,3,27],
            (Platform::Nes,true,_)=>&[41,47],
            (Platform::Nes,false,false)=>&[41,SCREEN_SIZE_KEY,47],
            (Platform::Nes,false,true)=>&[41,SCREEN_SIZE_KEY,47,44],
            (Platform::Snes,true,_)=>&[42,48],
            (Platform::Snes,false,false)=>&[42,SCREEN_SIZE_KEY,48],
            (Platform::Snes,false,true)=>&[42,SCREEN_SIZE_KEY,48,45],
            (Platform::Md,true,_)=>&[43,49],
            (Platform::Md,false,false)=>&[43,SCREEN_SIZE_KEY,49],
            (Platform::Md,false,true)=>&[43,SCREEN_SIZE_KEY,49,46],
            (Platform::Sms,true,_)=>&[CORE_SMS_KEY,SHADER_SMS_KEY],
            (Platform::Sms,false,false)=>&[CORE_SMS_KEY,SCREEN_SIZE_KEY,SHADER_SMS_KEY],
            (Platform::Sms,false,true)=>&[CORE_SMS_KEY,SCREEN_SIZE_KEY,SHADER_SMS_KEY,BEZEL_SMS_KEY],
            (Platform::Gg,true,_)=>&[CORE_GG_KEY,SHADER_GG_KEY],
            (Platform::Gg,false,false)=>&[CORE_GG_KEY,SCREEN_SIZE_KEY,SHADER_GG_KEY],
            (Platform::Gg,false,true)=>&[CORE_GG_KEY,SCREEN_SIZE_KEY,SHADER_GG_KEY,BEZEL_GG_KEY],
            (Platform::Ngp,true,_)=>&[CORE_NGP_KEY,SHADER_NGP_KEY],
            (Platform::Ngp,false,false)=>&[CORE_NGP_KEY,SCREEN_SIZE_KEY,SHADER_NGP_KEY],
            (Platform::Ngp,false,true)=>&[CORE_NGP_KEY,SCREEN_SIZE_KEY,SHADER_NGP_KEY,BEZEL_NGP_KEY],
            (Platform::Ws,true,_)=>&[SHADER_WS_KEY],
            (Platform::Ws,false,false)=>&[SCREEN_SIZE_KEY,SHADER_WS_KEY],
            (Platform::Ws,false,true)=>&[SCREEN_SIZE_KEY,SHADER_WS_KEY,BEZEL_WS_KEY],
            (Platform::Lynx,true,_)=>&[CORE_LYNX_KEY,SHADER_LYNX_KEY],
            (Platform::Lynx,false,false)=>&[CORE_LYNX_KEY,SCREEN_SIZE_KEY,SHADER_LYNX_KEY],
            (Platform::Lynx,false,true)=>&[CORE_LYNX_KEY,SCREEN_SIZE_KEY,SHADER_LYNX_KEY,BEZEL_LYNX_KEY],
            (Platform::Pokemini,true,_)=>&[],
            (Platform::Pokemini,false,false)=>&[SCREEN_SIZE_KEY],
            (Platform::Pokemini,false,true)=>&[SCREEN_SIZE_KEY,BEZEL_POKEMINI_KEY],
            (Platform::Pico8,true,_)=>&[CORE_PICO8_KEY,SHADER_PICO8_KEY],
            (Platform::Pico8,false,_)=>&[CORE_PICO8_KEY,SCREEN_SIZE_KEY,SHADER_PICO8_KEY],
            (Platform::NeoGeo,true,false)=>&[SHADER_NEOGEO_KEY],
            (Platform::NeoGeo,true,true)=>&[SHADER_NEOGEO_KEY,BEZEL_NEOGEO_KEY],
            (Platform::NeoGeo,false,false)=>&[SCREEN_SIZE_KEY,SHADER_NEOGEO_KEY],
            (Platform::NeoGeo,false,true)=>&[SCREEN_SIZE_KEY,SHADER_NEOGEO_KEY,BEZEL_NEOGEO_KEY],
            (Platform::Pce,true,false)=>&[CORE_PCE_KEY,SHADER_PCE_KEY],
            (Platform::Pce,true,true)=>&[CORE_PCE_KEY,SHADER_PCE_KEY,BEZEL_PCE_KEY],
            (Platform::Pce,false,false)=>&[CORE_PCE_KEY,SCREEN_SIZE_KEY,SHADER_PCE_KEY],
            (Platform::Pce,false,true)=>&[CORE_PCE_KEY,SCREEN_SIZE_KEY,SHADER_PCE_KEY,BEZEL_PCE_KEY],
        }
    }
    fn core_key_platform(key:usize)->Option<Platform> {
        match key {
            29=>Some(Platform::Gba), 30=>Some(Platform::Gbc), 31=>Some(Platform::Gb),
            41=>Some(Platform::Nes), 42=>Some(Platform::Snes), 43=>Some(Platform::Md),
            crate::settings::CORE_SMS_KEY=>Some(Platform::Sms),
            crate::settings::CORE_GG_KEY=>Some(Platform::Gg),
            crate::settings::CORE_NGP_KEY=>Some(Platform::Ngp),
            crate::settings::CORE_LYNX_KEY=>Some(Platform::Lynx),
            crate::settings::CORE_PICO8_KEY=>Some(Platform::Pico8),
            crate::settings::CORE_PCE_KEY=>Some(Platform::Pce),
            _=>None,
        }
    }
    fn load_default_cores(&mut self) {
        let paths=PATHS.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some((storage,_))=paths {
            self.default_cores=Platform::ALL.map(|platform| core_selection::default(&storage,platform));
            self.system_video_modes=Platform::ALL.map(|platform| video_mode::system_mode(&storage,platform,panel_size()));
        }
    }
    /// Settings > Systems' Screen Size: Integer, Original Ratio, Stretch.
    fn cycle_system_video_mode(&mut self, platform: Platform, right: bool) {
        let at=platform as usize;
        let now=self.system_video_modes[at];
        let mode=if right { now.bigger_on(platform,panel_size()) } else { now.smaller_on(platform,panel_size()) };
        if mode==now { return; }
        let paths=PATHS.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let Some((root,_))=paths else { return };
        match video_mode::write_system_mode(&root,platform,mode) {
            Ok(()) => self.system_video_modes[at]=mode,
            Err(err) => set_error_message("Couldn't save the screen size.", &err),
        }
    }
    /// The Default Core row swaps between the system's two cores.
    fn cycle_default_core(&mut self, platform: Platform) {
        let Some((storage,library))=PATHS.lock().unwrap_or_else(|e| e.into_inner()).clone()
            else { return };
        let at=platform as usize;
        let core=if self.default_cores[at]==Core::primary_for(platform) {
            Core::alternate_for(platform)
        } else { Core::primary_for(platform) };
        if !library.join(format!("lib{}_libretro.so", core.as_str())).is_file() {
            set_message(format!("{} isn't installed.", core.text()));
            return;
        }
        if core == Core::Pico8 && !crate::pico8::installed(&storage) {
            set_message(crate::pico8::MISSING.to_string());
            return;
        }
        match core_selection::set_default(&storage,platform,core) {
            Ok(()) => self.default_cores[at]=core,
            Err(err) => set_error_message("Couldn't save the default core.", &err),
        }
    }
    /// The heading of Settings > Systems > <system>: the same face as its
    /// row in the Systems list (MENU_TEXT 121..=123 are NES, Super
    /// Nintendo, Mega Drive; 120 is "Screen Size").
    fn screen_title(platform:Platform)->usize {
        SYSTEM_TITLES[SYSTEM_ORDER.iter().position(|p| *p==platform).unwrap_or(0)]
    }
    fn screen_submenu_platform(row:usize)->Platform {
        SYSTEM_ORDER.get(row).copied().unwrap_or(Platform::Gb)
    }

    fn pause_screen_rows(&self) -> &'static [usize] {
        // PokeDaisy: its views in Screen Size, first; no bezel (it fills
        // the screen).
        if self.pokedaisy_running() {
            return match self.current_cart_platform() {
                Platform::Gba => &[120,34,29],
                Platform::Gbc => &[120,88,29],
                _ if self.game.as_ref().is_some_and(GameSession::can_change_gb_palette) => &[120,89,36],
                _ => &[120,89],
            };
        }
        // GB/GBC resize with L1/R1; GBA's shoulders belong to the game.
        let bezel=video_mode::bezel_offered(self.current_cart_platform(),panel_size());
        // GBA Video: Shader, Upscaler, Color Correction (Siham 2026-11-10).
        if self.playing_video() {
            return match (fitted_canvas(),bezel) {
                (true,true)=>&[34,136,29,120,90], (true,false)=>&[34,136,29,120],
                (false,true)=>&[34,136,29,90], (false,false)=>&[34,136,29],
            };
        }
        if self.current_cart_platform()==Platform::Gba && fitted_canvas() {
            return if bezel {&[34,29,120,90]} else {&[34,29,120]};
        }
        if self.current_cart_platform()==Platform::Gb &&
            !self.game.as_ref().is_some_and(GameSession::can_change_gb_palette) {
            if bezel {&[89,90]} else {&[89]}
        } else {
            Self::screen_rows(self.current_cart_platform())
        }
    }
    fn pause_screen_key(&self,row:usize)->Option<usize> {
        if self.pokedaisy_running() {
            let keys: &[usize] = match self.current_cart_platform() {
                Platform::Gba => &[SCREEN_SIZE_KEY,0,22],
                Platform::Gbc => &[SCREEN_SIZE_KEY,21,23],
                _ if self.game.as_ref().is_some_and(GameSession::can_change_gb_palette) => &[SCREEN_SIZE_KEY,1,3],
                _ => &[SCREEN_SIZE_KEY,1],
            };
            return keys.get(row).copied();
        }
        if self.playing_video() {
            let rows=self.pause_screen_rows();
            return rows.get(row).map(|label| match label {
                34=>0, 136=>70, 29=>22, 120=>SCREEN_SIZE_KEY, _=>25,
            });
        }
        if self.current_cart_platform()==Platform::Gba && fitted_canvas() {
            return [0,22,SCREEN_SIZE_KEY,25].get(row).copied();
        }
        if self.current_cart_platform()==Platform::Gb &&
            !self.game.as_ref().is_some_and(GameSession::can_change_gb_palette) {
            [1,27].get(row).copied()
        } else {
            Self::screen_keys(self.current_cart_platform()).get(row).copied()
        }
    }
    /// The paused game's Emulation page, its rows and their Settings keys
    /// (pause_emulation_page).
    fn pause_emulation(&self) -> (Vec<usize>, Vec<usize>) {
        let game = self.game.as_ref();
        pause_emulation_page(game.map(GameSession::core_kind), self.current_cart_platform(),
            game.is_some_and(GameSession::solar), game.is_some_and(GameSession::motion))
    }
    fn pause_emulation_rows(&self) -> Vec<usize> { self.pause_emulation().0 }
    /// The paused game's Gameplay page, rows and keys (pause_gameplay_page).
    fn pause_gameplay(&self) -> (Vec<usize>, Vec<usize>) {
        pause_gameplay_page(self.game.as_ref().map(GameSession::core_kind), self.current_cart_platform())
    }
    fn pause_settings_rows(&self) -> &'static [usize] {
        if self.game.as_ref().is_some_and(GameSession::linked) {
            // Display-only settings cannot alter GBA Link/RFU state.
            &[32]
        } else {
            &[32,33,49,125,57]
        }
    }

    /// The Frame Delay to apply in this callback: only while a solo game
    /// runs at 1x in RETROARCH mode with the display clocking the core.
    fn frame_delay_wanted(&self, vsync_frame_ns: i64) -> Option<i64> {
        let session = self.game.as_ref()?;
        let display_hz = f64::from(DISPLAY_REFRESH_MILLIHZ.load(Ordering::Acquire)) / 1000.0;
        let clocked = vsync_frame_ns > 0
            && matches!(self.overlay, ShelfOverlay::None)
            && !self.exiting_screen && !self.rewind_held
            && !self.ff_held && !self.ff_latched && !session.linked()
            && !RA_SWAP_LOOP.load(Ordering::Acquire)
            && crate::ra_sync::display_can_clock_core(display_hz, session.fps);
        clocked.then(|| self.frame_delay.delay_ns())
    }

    /// Called once the frame is drawn: feeds this callback's cost (VSync to
    /// now, minus the delay) back into the automatic Frame Delay.
    fn observe_frame_delay(&mut self, vsync_frame_ns: i64) {
        let Some(delay) = self.frame_delay_used_ns.take() else { return };
        let reported = f64::from(DISPLAY_REFRESH_MILLIHZ.load(Ordering::Acquire)) / 1000.0;
        if !(reported > 0.0) { return; }
        let period = (1e9 / self.ra_sync_steps.display_hz(reported)) as i64;
        let cost = crate::ra_sync::monotonic_ns() - vsync_frame_ns - delay;
        self.frame_delay.observe_frame(cost, period);
    }

    fn current_emulation(&self) -> EmulationProfile {
        let mut profile = self.game_emulation
            .unwrap_or_else(|| EmulationProfile::from_settings(&self.settings));
        // Legacy per-cart profiles inherit global Show Stats. Newly
        // customized profiles can override it explicitly per cartridge.
        profile.show_fps = profile.show_fps_override.unwrap_or(self.settings.show_fps);
        // Only gpSP needs the private mixer snapshot ABI. If an unexpected
        // unpatched core is loaded, do not run speculative frames with audio
        // corruption; preserve the user's requested profile on disk.
        if self.game.as_ref().is_some_and(|game| !game.runahead_supported()) {
            profile.run_ahead_frames = 0;
        }
        profile
    }
    fn selected_emulation_core(&self) -> Core {
        // GB/GBC never invoke gpSP, nor GBA Gambatte, even if a stale core
        // preference exists.
        // A running game names its own core (MBC7 carts run on mGBA).
        if let Some(game) = self.game.as_ref() { return game.core_kind(); }
        let platform = self.current_cart_platform();
        let Some(uri) = self.selected_rom_uri() else { return Core::default_for(platform); };
        PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
            .map(|(root, _)| core_selection::selected_for(root, &uri, platform))
            .unwrap_or_else(|| Core::default_for(platform))
    }
    fn selected_rom_uri(&self) -> Option<String> {
        self.shelves.get(self.active)?.carts
            .get(self.shelves[self.active].index)
            .map(|cart| cart.rom.to_string_lossy().into_owned())
    }
    fn load_selected_emulation(&mut self) {
        let uri = self.selected_rom_uri();
        let core = self.selected_emulation_core();
        self.game_emulation = uri.as_deref().and_then(|uri| {
            PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
                .and_then(|(root, _)| EmulationProfile::load(root, uri, core))
        });
    }
    /// The in-game Screen page previews a look option the core draws itself
    /// at once: the paused game redrawn, the game not moved (Siham
    /// 2026-11-06).
    /// PokeDaisy: drawn again without running (its view, its colours), or,
    /// with `runs`, a few frames run and undone as for mGBA (a Game Boy
    /// palette, which the game applies as it draws).
    fn redraw_paused_game(&mut self, runs: bool) {
        if self.pokedaisy_running() && !runs {
            let platform = self.current_cart_platform();
            let (view, mode) = (self.pokedaisy_view, self.video_mode);
            let Some(session) = self.game.as_mut() else { return };
            if !session.redraw_held() { return; }
            let size = session.frame_size();
            self.gpu.upload_game_sized(session.frame(), size);
            self.gpu.set_game_fit(video_mode::pokedaisy_fit(view, platform, mode, size));
            if let Some(hud) = session.take_pokedaisy_hud() { self.pokedaisy_hud.update(&mut self.gpu, hud); }
            if let Some(view) = session.take_pokedaisy_view() { self.keep_pokedaisy_view(view); }
            return;
        }
        let platform = self.current_cart_platform();
        let blending = self.game_blending(platform);
        let dither = platform == Platform::Md && self.game_dither();
        let Some(session) = self.game.as_mut() else { return };
        session.set_frame_blending(blending);
        // The frames before the last one go through slot.'s Frame Blending
        // as in play: the paused picture mixes with its own game's frames,
        // not with what was shown before the change.
        let (mode, blend) = (session.slot_blending(), &mut self.frame_blend);
        if !session.redraw_paused(|frame, size| { blend.apply(mode, frame, size); }) { return; }
        if session.frame_size() == (slot_retro::GBA_W, slot_retro::GBA_H)
            && session.display_aspect() <= 0.0 {
            self.gpu.upload_game(session.frame());
        } else {
            let size = session.frame_size();
            let picture = self.frame_blend.apply(session.slot_blending(), session.frame(), size);
            let picture = if dither { self.dither_blend.apply(picture, size) } else { picture };
            self.gpu.upload_game_sized(picture, size);
        }
    }

    fn change_game_screen_or_gameplay(&mut self, key: usize, right: bool) {
        if key == POKEDAISY_INTRO_KEY {
            if self.pokedaisy_running() { self.toggle_pokedaisy_intro(); }
            return;
        }
        if key == SCREEN_SIZE_KEY && self.pokedaisy_running() {
            self.step_pokedaisy_view(right);
            return;
        }
        if key == SCREEN_SIZE_KEY {
            let same = self.game_integer_is_ratio();
            let mode = if right { self.video_mode.bigger_when(same) }
                else { self.video_mode.smaller_when(same) };
            if mode != self.video_mode {
                self.set_video_mode(mode);
                self.hud.toast(self.video_mode_toast(), self.born.elapsed().as_millis() as u64);
            }
            return;
        }
        let Some(uri)=self.selected_rom_uri() else { return };
        let core=self.selected_emulation_core();
        let platform=self.current_cart_platform();
        if key == 2 && self.game_settings().shader_for(platform).includes_gba_color() {
            // These two RetroArch presets bake GBA Color into the shader.
            // Never silently toggle an option that cannot affect the picture.
            set_message("This shader already includes color correction.".into());
            return;
        }
        let linked=self.game.as_ref().is_some_and(GameSession::linked);
        let relaunch=self.game.as_ref().is_some_and(GameSession::requires_relaunch);
        // Shader and color-correction are display-only and safe while linked;
        // never change input, save states, emulation speed or rewind in Link.
        if relaunch || (linked && !matches!(key,0|1|2|21|22|23|24|25|26|27|44|45|46|47|48|49|60..=70
            |crate::settings::SHADER_SMS_KEY..=crate::settings::BEZEL_POKEMINI_KEY
            |crate::settings::SHADER_NEOGEO_KEY..=crate::settings::BEZEL_PCE_KEY
            |crate::settings::VIGNETTE_NES_KEY..=crate::settings::CURVATURE_PCE_KEY)
            && Settings::lcd_effect_platform(key) != Some(platform)) {
            set_message("Not available during Multiplayer.".into());
            return;
        }
        // The 3.0 systems' sound and language are the system's (read when a
        // game starts); their picture options are this game's, below.
        if matches!(key, crate::settings::NGP_LANGUAGE_KEY | crate::settings::WS_LANGUAGE_KEY
            | crate::settings::FM_SMS_KEY) {
            if !self.settings.change(key,right) { return; }
            self.persist_settings();
            return;
        }
        // This method is only called from the paused game, not the carousel.
        let mut profile=self.game_emulation
            .unwrap_or_else(|| EmulationProfile::from_settings(&self.settings));
        // The per-game profile keeps only Palette ON/OFF; which palette is
        // the shared choice, exactly as the in-game SELECT shortcut keeps it.
        let palette = (key==3).then(|| {
            let mut view=self.game_settings();
            view.change(3,right);
            view.gb_palette
        });
        if !profile.change_visual_and_gameplay(&self.cart_globals(),platform,key,right) { return; }
        if let Some(index)=palette {
            if self.settings.gb_palette!=index {
                self.settings.gb_palette=index;
                self.persist_settings();
            }
        }
        if !save_profile_later(profile, &uri, core) { return; }
        self.game_emulation=Some(profile);
        if (crate::settings::POKEMINI_FILTER_KEY..=crate::settings::POKEMINI_SHAKE_KEY).contains(&key)
            || key == crate::settings::LEFT_BAR_NES_KEY {
            // The core takes the game's own values at its next frame.
            let extras=crate::settings::CoreExtras::of(&self.game_settings());
            if let Some(session)=self.game.as_mut() { session.apply_core_extras(extras,platform); }
        }
        if matches!(key,26|27) {self.tell_hidden_bezel();}
        let view=self.game_settings();
        if Settings::lcd_effect_platform(key).is_some() || matches!(key,0|1|3|21|47|48|49|70) || matches!(key,
            crate::settings::SHADER_SMS_KEY|crate::settings::SHADER_GG_KEY
            |crate::settings::SHADER_NGP_KEY|crate::settings::SHADER_WS_KEY
            |crate::settings::SHADER_LYNX_KEY|crate::settings::SHADER_POKEMINI_KEY
            |crate::settings::SHADER_PICO8_KEY|SHADER_NEOGEO_KEY|SHADER_PCE_KEY)
            || Settings::crt_platform(key).is_some() {
            self.apply_game_look();
        }
        // A shader change can switch to/from a preset with its own GBA Color.
        // Recompute core colour here as well as when the correction toggle moves.
        if matches!(key,0|1|21|2|22|23|24) {
            if let Some(session)=self.game.as_mut() {
                session.set_colour_correction(view.shader_for(platform)
                    .core_colour(view.colour_for(platform)));
            }
        }
        if key==3 {
            if let Some(session)=self.game.as_mut() {
                session.set_gb_palette_mode(view.gb_palettes,view.palette_name());
            }
        }
        if key==5 {
            FF_AUDIO_ENABLED.store(view.ff_sound,Ordering::Release);
            AUDIO.lock().unwrap_or_else(|e|e.into_inner()).clear();
        }
        // No stale FF, rewind or frame debt after an option changes.
        if (4..=9).contains(&key) { self.reset_time_controls(); }
    }

    fn change_game_emulation(&mut self, key: usize, right: bool) {
        if self.game.as_ref().is_some_and(|game|game.linked() || game.requires_relaunch()) {
            // In a linked session, do not alter the effective frame cadence
            // or speculative time travel. Keep the current session in sync.
            set_message("Not available during Multiplayer.".into());
            return;
        }
        if key == 12 && self.game.as_ref().is_some_and(|game|
            game.sma4_runahead_blocked()) {
            set_message("Run-Ahead off to protect this game's saves.".into());
            return;
        }
        if key == 12 && self.game.as_ref().is_some_and(|game|
            game.mgba_flash_runahead_blocked()) {
            set_message("Run-Ahead off with mGBA to protect saves.".into());
            return;
        }
        if key == 12 && self.game.as_ref().is_some_and(|game|
            !game.runahead_supported()) {
            set_message("Run-Ahead isn't available for this game.".into());
            return;
        }
        let Some(uri) = self.selected_rom_uri() else { return; };
        let core = self.selected_emulation_core();
        // Edit the persisted intent, not the safety-clamped effective profile.
        // A core that temporarily disables speculative frames must not erase
        // saved Run-Ahead preference when we change Show Stats / pacing.
        let mut profile = self.game_emulation
            .unwrap_or_else(|| EmulationProfile::from_settings(&self.settings));
        profile.show_fps = profile.show_fps_override.unwrap_or(self.settings.show_fps);
        if !profile.change(key, right) { return; }
        if !save_profile_later(profile, &uri, core) { return; }
        self.game_emulation = Some(profile);
        // Avoid reinterpreting a frame debt accumulated under the previous
        // timing policy, and discard any prior predictive video/audio frame.
        // The core stays alive; edits take effect on the first resumed frame.
        self.game_accum = 0.0;
        self.ra_clock.reset();
        self.ra_sync_steps.reset();
        self.frame_delay.reset();
        AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
    fn sun_top(&self) -> u8 {
        self.game.as_ref().map_or(10, |game| game.sun_top())
    }
    fn change_game_sun(&mut self, right: bool) {
        if self.game.as_ref().is_some_and(|game|game.linked() || game.requires_relaunch()) {
            set_message("Not available during Multiplayer.".into());
            return;
        }
        let Some(uri) = self.selected_rom_uri() else { return; };
        let core = self.selected_emulation_core();
        let mut profile = self.game_emulation
            .unwrap_or_else(|| EmulationProfile::from_settings(&self.settings));
        profile.change_sun(light_sensor(), self.sun_top(), right);
        self.save_game_sun(profile, &uri, core);
    }
    /// L3 / R3 (and SELECT + L2 / R2) in a solar game, as mGBA's own R3
    /// brighten / L3 darken: one step of the same round as the menu,
    /// 0 … top, SENSOR, AUTO, 0.
    fn nudge_sun(&mut self, brighter: bool) {
        if self.game.as_ref().is_some_and(|game|game.linked() || game.requires_relaunch()) {
            return;
        }
        let Some(uri) = self.selected_rom_uri() else { return; };
        let core = self.selected_emulation_core();
        let mut profile = self.game_emulation
            .unwrap_or_else(|| EmulationProfile::from_settings(&self.settings));
        let time = self.born.elapsed().as_millis() as u64;
        profile.change_sun(light_sensor(), self.sun_top(), brighter);
        // The same banner as State Saved or a shader change.
        self.hud.toast(match profile.sun(light_sensor(), self.sun_top()) {
            crate::emulation::SUN_SENSOR => Toast::SunSensor,
            crate::emulation::SUN_AUTO => Toast::SunAuto,
            sun => Toast::Sun(sun),
        }, time);
        self.save_game_sun(profile, &uri, core);
    }
    fn change_game_motion(&mut self, up: bool) {
        let Some(uri) = self.selected_rom_uri() else { return; };
        let core = self.selected_emulation_core();
        let mut profile = self.game_emulation
            .unwrap_or_else(|| EmulationProfile::from_settings(&self.settings));
        profile.change_motion(up);
        self.save_game_motion(profile, &uri, core);
    }
    /// L3 / R3 (and SELECT + L2 / R2) in a tilt or gyro game, as for
    /// Boktai's sun: one step of its sensitivity, with the banner.
    fn nudge_motion(&mut self, up: bool) {
        let Some(uri) = self.selected_rom_uri() else { return; };
        let core = self.selected_emulation_core();
        let mut profile = self.game_emulation
            .unwrap_or_else(|| EmulationProfile::from_settings(&self.settings));
        let time = self.born.elapsed().as_millis() as u64;
        profile.change_motion(up);
        self.hud.toast(Toast::Motion(profile.motion()), time);
        self.save_game_motion(profile, &uri, core);
    }
    fn save_game_motion(&mut self, profile: EmulationProfile, uri: &str, core: Core) {
        if !save_profile_later(profile, uri, core) { return; }
        self.game_emulation = Some(profile);
        apply_motion(self.game.as_ref(), profile.motion());
    }
    fn save_game_sun(&mut self, profile: EmulationProfile, uri: &str, core: Core) {
        if !save_profile_later(profile, uri, core) { return; }
        self.game_emulation = Some(profile);
        if let Some(session) = self.game.as_mut() {
            session.set_sun(profile.sun(light_sensor(), session.sun_top()));
        }
    }
    fn reset_game_emulation(&mut self) {
        if self.game.as_ref().is_some_and(|game|game.linked() || game.requires_relaunch()) {
            set_message("Not available during Multiplayer.".into());
            return;
        }
        let Some(uri) = self.selected_rom_uri() else { return; };
        let core = self.selected_emulation_core();
        if let Some((root, _)) = PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            if let Err(error) = EmulationProfile::clear(root, &uri, core) {
                set_error_message("Couldn't reset this game's settings.", &error);
                return;
            }
        } else { return; }
        self.game_emulation = None;
        if let Some(session) = self.game.as_mut() {
            session.set_sun(crate::emulation::default_sun(light_sensor()));
        }
        apply_motion(self.game.as_ref(), crate::emulation::MOTION_DEFAULT);
        let globals=self.game_settings();
        let platform=self.current_cart_platform();
        self.apply_game_look();
        // The 3.0 picture options back to the system's.
        let extras=crate::settings::CoreExtras::of(&globals);
        if let Some(session)=self.game.as_mut() { session.apply_core_extras(extras,platform); }
        if let Some(session)=self.game.as_mut() {
            session.set_colour_correction(globals.shader_for(platform)
                .core_colour(globals.colour_for(platform)));
            session.set_gb_palette_mode(globals.gb_palettes,globals.palette_name());
        }
        FF_AUDIO_ENABLED.store(globals.ff_sound,Ordering::Release);
        self.reset_time_controls();
        AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
    fn persist_settings(&self) {
        crate::settings::CoreExtras::store(&self.settings);
        if let Some((root, _)) = PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            let (root, settings) = (root.clone(), self.settings.clone());
            crate::later_write::write(root.join("Settings.json"), move || {
                if let Err(error) = settings.save(&root) {
                    set_error_message("Couldn't save settings.", &error);
                }
            });
        }
    }

    /// General's wallpaper, made on a worker (a Blur takes a moment): the
    /// one on screen stays until it is ready. In tests, at once.
    fn reload_wallpaper(&mut self, root: Option<&Path>) {
        let (pick, look) = (self.settings.wallpaper.clone(), self.settings.wallpaper_look);
        let (shelf, theme) = (Some(self.active), self.settings.appearance.mosaic_theme());
        self.wallpaper_rx = None;
        if cfg!(test) {
            let made = wallpaper_pixels(root, &pick, look, shelf, theme);
            self.take_wallpaper(made);
        } else {
            let owned = root.map(Path::to_path_buf);
            let (tx, rx) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new().name("slot-wallpaper-general".into())
                .spawn(move || { let _ = tx.send(wallpaper_pixels(owned.as_deref(), &pick, look, shelf, theme)); });
            match spawned {
                Ok(_) => self.wallpaper_rx = Some(rx),
                Err(error) => {
                    crate::log_warn(&format!("wallpaper worker failed: {error}"));
                    let made = wallpaper_pixels(root, &self.settings.wallpaper,
                        self.settings.wallpaper_look, shelf, theme);
                    self.take_wallpaper(made);
                }
            }
        }
        self.reload_shelf_wallpapers(root);
    }

    /// General's new wallpaper, once made; true when it was uploaded.
    fn poll_wallpaper(&mut self) -> bool {
        let Some(made) = self.wallpaper_rx.as_ref().and_then(|rx| rx.try_recv().ok()) else { return false };
        self.wallpaper_rx = None;
        crate::frame_watch::did(crate::frame_watch::WORK_WALLPAPER);
        self.take_wallpaper(made);
        true
    }

    fn take_wallpaper(&mut self, (image, personal): (Option<Vec<u8>>, bool)) {
        self.wallpaper_personal = personal;
        if let Some(rgba) = image.as_deref() {
            self.wallpaper_top_luma = Some(slot_ui::top_line_luma(rgba, slot_ui::native_w()));
        }
        if let Some(rgba) = image {
            self.wallpaper_texture = Some(if let Some(tex) = self.wallpaper_texture {
                self.gpu.update_texture(tex, slot_ui::native_w(), slot_ui::native_h(), &rgba);
                tex
            } else {
                self.gpu.create_texture(slot_ui::native_w(), slot_ui::native_h(), &rgba)
            });
            if let Some(tex) = self.wallpaper_texture { self.gpu.pack_static_recovery(tex); }
        }
        // None deliberately leaves the texture allocated for quick reactivation
        // but never draws it, avoiding repeated GL allocations on toggling.
    }

    fn refresh_wallpaper_name(&mut self) {
        let crate::settings::WallpaperPick::File(_) = &self.settings.wallpaper
            else { return };
        let name = self.settings.wallpaper.label();
        let short: String = name.chars().take(18).collect();
        let face = quick_value_face(&short, true);
        self.wallpaper_name_face = Some(
            if let Some((tex, _, _)) = self.wallpaper_name_face {
                self.gpu.update_face(tex, &face);
                (tex, face.w, face.h)
            } else {
                (self.gpu.create_face(&face), face.w, face.h)
            });
    }

    fn cycle_wallpaper(&mut self, right: bool) {
        let root = PATHS.lock().unwrap_or_else(|e| e.into_inner())
            .as_ref().map(|(root,_)|root.clone());
        let Some(root) = root.as_deref() else { return };
        self.settings.wallpaper = self.settings.wallpaper.step(root, right);
        self.reload_wallpaper(Some(root));
        self.refresh_wallpaper_name();
        self.persist_settings();
    }

    fn recents_root() -> Option<PathBuf> {
        PATHS.lock().unwrap_or_else(|e|e.into_inner())
            .as_ref().map(|(root,_)|root.clone())
    }
    fn persist_focus_if_idle(&mut self) {
        let current=self.shelves.get(self.active)
            .and_then(|shelf|shelf.carts.get(shelf.index))
            .map(|cart|(self.active,cart.rom.to_string_lossy().into_owned()));
        if current!=self.focus_seen {
            self.focus_seen=current;
            self.focus_dirty_since=Some(Instant::now());
        }
        if self.focus_dirty_since.is_some_and(|since|since.elapsed().as_millis()>=500) {
            self.focus_dirty_since=None;
            if let Some((rack,uri))=self.focus_seen.as_ref() {
                if let Some(root)=Self::recents_root() {
                    let focus=crate::selection::Focus { rack:*rack,uri:uri.clone() };
                    crate::later_write::write(root.join("selection"), move || {
                        if let Err(err)=crate::selection::store(&root,&focus) {
                            crate::log_warn(&format!("last selection could not persist: {err}"));
                        }
                    });
                }
            }
        }
    }
    /// Every system's carts, GBA Video's included (Recents and Favorites
    /// draw from them).
    fn system_carts(&self) -> Vec<&[slot_store::Cart]> {
        self.shelves.iter().take(Platform::ALL.len())
            .chain(self.shelves.get(VIDEO_RACK))
            .map(|shelf| shelf.carts.as_slice()).collect()
    }
    fn toggle_favorite(&mut self) {
        if self.selected_test_cart() { return; }
        let Some(uri)=self.selected_rom_uri() else {return};
        let Some(root)=Self::recents_root() else {return};
        match crate::favorites::toggle(&root,&uri) {
            Ok(_) => self.rebuild_favorites(),
            Err(error)=>set_error_message("Couldn't save this favorite.", &error),
        }
    }
    /// A new library with other ROMs (a scan after RomM downloads, Refresh
    /// Library): the shelves take it in place, every unchanged cart keeping
    /// its face, so no shelf has to be drawn again (Siham 2026-11-10: L1/R1
    /// froze after RomM downloads). The very same carts sent again: nothing
    /// to do (true). False when the same ROMs come with other titles or
    /// shells: the caller then builds everything again.
    pub(super) fn update_library(&mut self, version: u64, library: Option<&[RomEntry]>) -> bool {
        let (systems, video) = library_carts(library, Self::recents_root().as_deref());
        let roms = |carts: &mut dyn Iterator<Item = &slot_store::Cart>| {
            let mut roms: Vec<PathBuf> = carts.map(|c| c.rom.clone()).collect();
            roms.sort();
            roms
        };
        let before = roms(&mut self.system_carts().into_iter().flatten());
        let after = roms(&mut systems.iter().flatten().chain(video.iter()));
        if self.shelves.len() <= VIDEO_RACK { return false; }
        if before == after {
            // The very same carts (a library sent again): nothing to do. A
            // rebuild threw every face away for nothing, 5 s on an empty
            // shelf (2.4.1-beta15 log). Other changes (titles, shells) still
            // rebuild, as before.
            let racks = (0..Platform::ALL.len()).chain(std::iter::once(VIDEO_RACK));
            let same = racks.zip(systems.iter().chain(std::iter::once(&video)))
                .all(|(rack, carts)| self.shelves[rack].carts == *carts);
            if same {
                self.library_version = version;
                self.has_library |= library.is_some();
            }
            return same;
        }
        let racks = (0..Platform::ALL.len()).chain(std::iter::once(VIDEO_RACK));
        for (rack, carts) in racks.zip(systems.into_iter().chain(std::iter::once(video))) {
            // A shelf the download left alone keeps its faces being drawn.
            if self.shelves[rack].carts == carts { continue; }
            self.cart_render.invalidate_shelf(rack);
            let moves = self.shelves[rack].update_carts(carts);
            self.rekey_rack_visuals(rack, &moves);
        }
        self.rebuild_recents();
        self.rebuild_favorites();
        self.library_version = version;
        self.has_library |= library.is_some();
        if self.shelves.get(self.active).is_none_or(|s| s.carts.is_empty()) {
            self.active = self.next_shelf(self.active.min(Platform::ALL.len()), 1);
            self.platform_shown_at = Some(Instant::now());
        }
        self.focus_dirty_since = Some(Instant::now());
        true
    }
    fn rebuild_favorites(&mut self) {
        let Some(root)=Self::recents_root() else {return};
        let ids=crate::favorites::load(&root);
        let system_carts = self.system_carts();
        let carts=crate::favorites::carts(&ids, &system_carts);
        let was_favorite=self.active==FAVORITES_RACK;
        let previous_index=self.shelves[FAVORITES_RACK].index;
        let previous_uri=self.shelves[FAVORITES_RACK].carts.get(previous_index).map(|c|c.rom.clone());
        if self.shelves[FAVORITES_RACK].carts != carts {
            self.cart_render.invalidate_shelf(FAVORITES_RACK);
            let moves=self.shelves[FAVORITES_RACK].update_carts(carts);
            self.rekey_rack_visuals(FAVORITES_RACK,&moves);
        }
        // Unstarring from Favorites selects the next cart, exactly as upstream.
        if was_favorite && !self.shelves[FAVORITES_RACK].carts.is_empty() &&
            previous_uri.as_ref().is_some_and(|uri|
                !self.shelves[FAVORITES_RACK].carts.iter().any(|cart|&cart.rom==uri)) {
            let last=self.shelves[FAVORITES_RACK].carts.len()-1;
            self.shelves[FAVORITES_RACK].select(previous_index.min(last));
        }
        for shelf in &mut self.shelves {
            shelf.mark_starred(|cart|crate::favorites::holds(&ids, cart.rom.to_string_lossy().as_ref()));
        }
        if was_favorite && self.shelves[FAVORITES_RACK].carts.is_empty() {
            self.active=self.next_shelf(0,1);
            self.platform_shown_at=Some(Instant::now());
        }
    }
    // Preserve cart textures/material across Recents or Favorites reorder.
    fn rekey_rack_visuals(&mut self, rack:usize, moves:&[(usize,usize)]) {
        let moved: std::collections::HashMap<usize,usize> = moves.iter().copied().collect();
        for (shelf,index,_) in &mut self.texture_cache {
            if *shelf!=rack {continue;}
            if let Some(new)=moved.get(index) {
                *index=*new;
            } else {
                *shelf=usize::MAX;
                *index=usize::MAX;
            }
        }
        if let Some(material)=self.clear_material.as_mut() {
            if material.key.0==rack {
                if let Some(new)=moved.get(&material.key.1) {
                    material.key.1=*new;
                } else {
                    material.key=(usize::MAX,usize::MAX);
                }
            }
        }
    }
    /// Interface > 3.0 Test Shelf turned on or off (3.0.0-cartes1).
    fn rebuild_test_shelf(&mut self) {
        let carts = if self.settings.test_shelf { test_carts() } else { Vec::new() };
        let Some(shelf) = self.shelves.get_mut(TEST_RACK) else { return };
        if shelf.carts == carts { return; }
        self.cart_render.invalidate_shelf(TEST_RACK);
        let moves = self.shelves[TEST_RACK].update_carts(carts);
        self.rekey_rack_visuals(TEST_RACK, &moves);
        if self.active == TEST_RACK && self.shelves[TEST_RACK].carts.is_empty() {
            self.active = self.next_shelf(0, 1);
        }
    }
    /// The selected cart is one of the 3.0 test shelf's.
    fn selected_test_cart(&self) -> bool {
        let shelf = &self.shelves[self.active];
        shelf.carts.get(shelf.index).is_some_and(slot_ui::is_test_cart)
    }
    fn rebuild_recents(&mut self) {
        let Some(root)=Self::recents_root() else { return };
        let ids=crate::recents::load(&root);
        let system_carts = self.system_carts();
        let carts=crate::recents::carts(&ids, &system_carts);
        let was_recent=self.active==RECENTS_RACK;
        let carts=if self.settings.show_recents { carts } else { Vec::new() };
        if self.shelves[RECENTS_RACK].carts != carts {
            // Reject completed worker art from the previous URI->index mapping.
            self.cart_render.invalidate_shelf(RECENTS_RACK);
            let moves=self.shelves[RECENTS_RACK].update_carts(carts);
            self.rekey_rack_visuals(RECENTS_RACK,&moves);
        }
        // Restocking Recents must not erase Favorite stars: its Shelf rebuild
        // necessarily reconstructs the per-cart starred flags from scratch.
        let favorites=crate::favorites::load(&root);
        self.shelves[RECENTS_RACK].mark_starred(|cart|crate::favorites::holds(&favorites,
            cart.rom.to_string_lossy().as_ref()));
        if was_recent && self.shelves[RECENTS_RACK].carts.is_empty() {
            self.active=self.next_shelf(0,1);
        }
    }
    fn record_recent(&mut self) {
        let Some(root)=Self::recents_root() else { return };
        let Some(uri)=self.selected_rom_uri() else { return };
        // Launches may immediately leave the shelf renderer. Flush the selected
        // game identity now, rather than relying on a 500ms idle debounce.
        let focus=crate::selection::Focus {rack:self.active,uri:uri.clone()};
        if let Err(err)=crate::selection::store(&root,&focus) {
            crate::log_warn(&format!("launch shelf-focus save failed: {err}"));
        } else {
            self.focus_seen=Some((focus.rack,focus.uri));
            self.focus_dirty_since=None;
        }
        let next=crate::recents::append(&crate::recents::load(&root),&uri);
        if let Err(err)=crate::recents::store(&root,&next) {
            crate::log_warn(&format!("unable to update recents rack: {err}"));
        } else {
            self.rebuild_recents();
        }
    }
    /// Settings > Interface's rows.
    fn general_rows(&self) -> Vec<usize> { INTERFACE_PAGE_ROWS.to_vec() }
    /// Interface > Wallpaper's rows shown but idle (Siham 2026-11-09):
    /// Blur over None, a shelf's looks while it shows General's (they are
    /// General's), Delete Wallpaper with nothing to delete.
    fn general_row_dimmed(&self, item: usize) -> bool {
        match item {
            BLUR_ROW | HALO_ROW | DARKEN_ROW => self.wallpaper_look_idle(item),
            45 => !self.removable_wallpaper(),
            _ => false,
        }
    }
    /// The wallpaper the page's shelf shows (General's for General).
    fn edited_wallpaper(&self) -> &crate::settings::WallpaperPick {
        match self.wallpaper_edit {
            Some(shelf) => self.settings.wallpaper_on(shelf),
            None => &self.settings.wallpaper,
        }
    }
    /// The look the page's shelf shows.
    fn edited_look(&self) -> crate::settings::WallpaperLook {
        match self.wallpaper_edit {
            Some(shelf) => self.settings.look_on(shelf),
            None => self.settings.wallpaper_look,
        }
    }
    fn wallpaper_look_idle(&self, row: usize) -> bool {
        use crate::settings::WallpaperPick;
        let pick = self.edited_wallpaper();
        self.wallpaper_edit.is_some_and(|shelf| self.settings.shelf_wallpaper(shelf).is_none())
            || (row == BLUR_ROW && *pick == WallpaperPick::None)
            || (row == DARKEN_ROW && !matches!(pick, WallpaperPick::File(_) | WallpaperPick::Random))
    }
    /// Interface > Wallpaper's six rows, whatever the wallpaper, and their
    /// values' keys: the third is Halo with Default, Tint otherwise, idle
    /// but for an image (Random deals images) (Siham 2026-11-19).
    fn wallpaper_rows(&self) -> (Vec<usize>, Vec<usize>) {
        let (third, key) = if self.edited_wallpaper().is_mosaic() { (HALO_ROW, HALO_KEY) }
            else { (DARKEN_ROW, DARKEN_KEY) };
        (vec![133, 83, BLUR_ROW, third, 43, 45],
         vec![SHELF_PICK_KEY, WALLPAPER_PICK_KEY, BLUR_KEY, key, usize::MAX, usize::MAX])
    }
    /// Blur, Halo or Tint one step, on General or the page's shelf; a new
    /// Blur or Halo remakes the wallpapers it shows on.
    fn cycle_wallpaper_look(&mut self, row: usize, right: bool) {
        use crate::settings::WallpaperLook;
        let mut look = self.edited_look();
        let value = match row { BLUR_ROW => &mut look.blur, HALO_ROW => &mut look.halo, _ => &mut look.darken };
        let next = WallpaperLook::step(*value, right);
        if next == *value { return; }
        *value = next;
        self.settings.set_look(self.wallpaper_edit, look);
        if row != DARKEN_ROW {
            let root = Self::recents_root();
            match self.wallpaper_edit {
                None => self.reload_wallpaper(root.as_deref()),
                Some(_) => self.reload_shelf_wallpapers(root.as_deref()),
            }
        }
        self.persist_settings();
    }
    /// Interface > Wallpaper > Shelf: General, then the shelves with carts
    /// (Recents while it is on, Favorites once a cart is in it).
    fn cycle_wallpaper_shelf(&mut self, right: bool) {
        let n = crate::wallpaper::SHELF_KEYS.len().min(self.shelves.len());
        let mut choices: Vec<Option<usize>> = vec![None];
        choices.extend((0..n).filter(|&i| !self.shelves[i].carts.is_empty()
            && (i != RECENTS_RACK || self.settings.show_recents)).map(Some));
        let at = choices.iter().position(|c| *c == self.wallpaper_edit).unwrap_or(0);
        let m = choices.len();
        self.wallpaper_edit = choices[if right { (at + 1) % m } else { (at + m - 1) % m }];
        self.refresh_wallpaper_edit_face();
    }
    /// Interface > Wallpaper > Wallpaper: General's, or the edited shelf's
    /// own (GENERAL to show General's again).
    fn cycle_edited_wallpaper(&mut self, right: bool) {
        let Some(shelf) = self.wallpaper_edit else {
            self.cycle_wallpaper(right);
            return;
        };
        let Some(root) = Self::recents_root() else { return };
        let next = crate::settings::WallpaperPick::shelf_step(
            self.settings.shelf_wallpaper(shelf), &root, shelf, right);
        self.settings.set_shelf_wallpaper(shelf, next);
        self.reload_shelf_wallpapers(Some(&root));
        self.refresh_wallpaper_edit_face();
        self.persist_settings();
    }
    /// The edited shelf's image name, for Wallpaper's value.
    fn refresh_wallpaper_edit_face(&mut self) {
        let Some(pick @ crate::settings::WallpaperPick::File(_)) =
            self.wallpaper_edit.and_then(|shelf| self.settings.shelf_wallpaper(shelf)).cloned()
            else { return };
        let short: String = pick.label().chars().take(18).collect();
        let face = quick_value_face(&short, true);
        self.wallpaper_edit_face = Some(
            if let Some((tex, _, _)) = self.wallpaper_edit_face {
                self.gpu.update_face(tex, &face);
                (tex, face.w, face.h)
            } else {
                (self.gpu.create_face(&face), face.w, face.h)
            });
    }
    fn cycle_general_row(&mut self, row: usize, right: bool, confirm: bool) {
        let Some(which)=self.general_rows().get(row).copied() else {return};
        match which {
            85 => self.cycle_appearance(right),
            83 if confirm => {
                self.wallpaper_edit = None;
                self.overlay = ShelfOverlay::WallpaperSettings { row: 0 };
            }
            86 => {
                self.settings.show_recents=!self.settings.show_recents;
                self.rebuild_recents();
                if !self.settings.show_recents && self.wallpaper_edit == Some(RECENTS_RACK) {
                    self.wallpaper_edit = None;
                }
                self.persist_settings();
            }
            177 => {
                self.settings.shelves_grid=!self.settings.shelves_grid;
                self.persist_settings();
            }
            87 => {
                self.settings.show_titles=!self.settings.show_titles;
                self.selected_title_name.clear();
                self.persist_settings();
            }
            156 => {
                self.settings.test_shelf=!self.settings.test_shelf;
                self.rebuild_test_shelf();
                self.persist_settings();
            }
            58 if confirm => self.overlay = ShelfOverlay::InterfaceSettings { row: 0 },
            _ => {}
        }
    }
    fn cycle_wallpaper_row(&mut self, row: usize, right: bool, confirm: bool) {
        let Some(which)=self.wallpaper_rows().0.get(row).copied() else {return};
        match which {
            133 => self.cycle_wallpaper_shelf(right),
            83 => self.cycle_edited_wallpaper(right),
            BLUR_ROW | HALO_ROW | DARKEN_ROW if !confirm && !self.wallpaper_look_idle(which) =>
                self.cycle_wallpaper_look(which, right),
            43 if confirm => {
                self.wallpaper_import = self.wallpaper_edit;
                self.set_wallpaper_target();
                UI_ACTION.lock().unwrap_or_else(|e|e.into_inner()).push_back(8);
            }
            45 if confirm && self.removable_wallpaper() => {
                self.wallpaper_import = None;
                self.set_wallpaper_target();
                UI_ACTION.lock().unwrap_or_else(|e|e.into_inner()).push_back(10);
                if self.wallpaper_edit.is_none() {
                    self.custom_wallpaper_present=false;
                }
            }
            _ => {}
        }
    }
    /// The count under the first scan's bar: carts found while the folders
    /// are listed, then carts read of how many.
    fn set_scan_progress(&mut self, done: u32, total: u32) {
        self.scan_progress = (done, total);
        let text = if total == 0 {
            if done == 0 { String::new() } else { format!("{done} FOUND") }
        } else {
            format!("{} / {total}", done.min(total))
        };
        if text == self.scan_count_text { return; }
        self.scan_count_text = text;
        if self.scan_count_text.is_empty() { return; }
        let face = quick_value_face(&self.scan_count_text, false);
        let tex = match self.scan_count {
            Some((tex, _, _)) => { self.gpu.update_face(tex, &face); tex }
            None => self.gpu.create_face(&face),
        };
        self.scan_count = Some((tex, face.w, face.h));
    }

    /// Rasterise only when the selected game changes; no repeated label
    /// processing on 60/120Hz draw callbacks.
    fn show_selected_letter(&mut self) {
        let Some(cart)=self.shelves.get(self.active)
            .and_then(|shelf|shelf.carts.get(shelf.index)) else {return};
        let letter=slot_store::initial(&cart.stem);
        let face=slot_ui::word_ink(&letter.to_string(),[255,255,255]);
        let tex=match self.letter_texture {
            Some(tex)=>{self.gpu.update_face(tex, &face);tex},
            None=>{let tex=self.gpu.create_face(&face);
                self.letter_texture=Some(tex);tex},
        };
        self.letter_face=Printed::new(tex,face.w);
        // Upstream clears the previous system-name display when a letter jump
        // occurs. Otherwise a quick UP just after L1/R1 stays invisible.
        self.platform_shown_at=None;
        self.letter_shown_at=Some(Instant::now());
    }

    /// The first scan, on an empty shelf: a smaller "SCANNING CARTRIDGES...",
    /// a thin bar and the count. The bar sweeps while the folders are listed
    /// and fills as the carts are read (Siham 2026-11-09).
    fn draw_first_scan(&self, out: &mut Vec<Draw>) {
        let width = out_w() as f32;
        let (tex, w, h) = self.shelf_status_faces[usize::from(self.shelf_status)];
        let s = 0.75;
        let (tw, th) = (w as f32 * s, h as f32 * s);
        out.push(Draw::Tex { x: (width - tw) / 2.0, y: 74.0, w: tw, h: th, tex, alpha: 1.0 });
        let (bw, bh) = (220.0f32, 3.0f32);
        let (bx, by) = ((width - bw) / 2.0, 74.0 + th + 10.0);
        out.push(Draw::Rect { x: bx, y: by, w: bw, h: bh, colour: [1.0, 1.0, 1.0, 0.22] });
        let (done, total) = self.scan_progress;
        let (fx, fw) = if total == 0 {
            let t = (self.born.elapsed().as_millis() % 1600) as f32 / 1600.0;
            let swing = if t < 0.5 { t * 2.0 } else { 2.0 - t * 2.0 };
            let seg = bw * 0.3;
            (bx + slot_ui::ease(swing) * (bw - seg), seg)
        } else {
            (bx, bw * (done.min(total) as f32 / total as f32))
        };
        out.push(Draw::Rect { x: fx, y: by, w: fw, h: bh, colour: [1.0, 1.0, 1.0, 0.85] });
        if let (Some((tex, w, h)), false) = (self.scan_count, self.scan_count_text.is_empty()) {
            let s = 0.6;
            let (cw, ch) = (w as f32 * s, h as f32 * s);
            out.push(Draw::Tex { x: (width - cw) / 2.0, y: by + bh + 8.0, w: cw, h: ch, tex, alpha: 0.76 });
        }
    }

    /// The CRT-off / black-hold picture: dark slot, seated cartridge.
    fn push_hold_scene(&self, commands: &mut Vec<Draw>) {
        commands.push(Draw::Rect {
            x: 0.0, y: 0.0, w: out_w() as f32, h: OUT_H as f32,
            // The shelf's own ground: black in OLED, as the shelf after it;
            // with Wallpaper None the theme's colour, as the shelf (Siham
            // 2026-11-14: no dark flash on insert/eject in Light/Original).
            colour: if self.wallpaper_none() {
                menu_style::shelf_ground()
            } else { menu_style::page_ground([0.085, 0.085, 0.093, 1.0]) },
        });
        // A wallpaper stays under the cart as on the shelf, tinted the same:
        // no dark screen before it on insert or eject (Siham 2026-11-18).
        if let Some((tex, tint, _)) = self.active_wallpaper() {
            slot_ui::draw_ground_tinted(Some(tex), tint, commands);
        }
        if let Some(shelf) = self.shelves.get(self.active) {
            if let Some(cart) = shelf.carts.get(shelf.index) {
                let (rest, scale) = shelf.selected_at();
                // Match upstream slot. v1.7: neighbours move out/fade in sync with
                // the 450ms seat animation and return on reverse ejection.
                shelf.draw_row(Some(cart.stem.as_str()), 0.0, self.progress, 1.0, commands);
                SlotChrome {
                    cart, face: shelf.face(shelf.index), rest, scale,
                    seat: self.progress, alert: None, dim: 0.0,
                    screen: self.screen_power, game: true, over: false,
                    slot_faces: pick_slot_faces(self.slot_faces, self.home_slot_faces, Some(cart)),
                }.draw_with_closer(commands,self.clear_layers().0,self.clear_layers().1,
                    pick_closer(self.slot_faces, self.home_closers, Some(cart)),
                    !self.inserted && self.progress > 0.0);
            } else {
                commands.push(Draw::Game);
            }
        }
    }

    /// The closed Rotate's shelf on a cart at random (its own shelf).
    fn lid_random_cart(&mut self) {
        let Some(shelf) = self.shelves.get_mut(self.active) else { return };
        let n = shelf.carts.len();
        if n < 2 { return; }
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos() as usize ^ d.as_secs() as usize);
        let mut next = seed % n;
        if next == shelf.index { next = (next + 1) % n; }
        shelf.select(next);
    }

    /// The closed Rotate's shelf on the last cart played, on its system's
    /// shelf (Recents keeps the order; the cart is found by its ROM).
    fn lid_last_played(&mut self) {
        let Some(root) = Self::recents_root() else { return };
        let Some(uri) = crate::recents::load(&root).into_iter().next() else { return };
        for (rack, shelf) in self.shelves.iter_mut().enumerate() {
            if rack >= RECENTS_RACK { break; }
            if let Some(i) = shelf.carts.iter().position(|c|
                crate::favorites::holds(std::slice::from_ref(&uri), c.rom.to_string_lossy().as_ref())) {
                self.active = rack;
                shelf.select(i);
                return;
            }
        }
    }

    /// The clock and percentage faces, finer, in `dark` or white ink.
    fn refresh_lid_faces(&mut self, dark: bool) {
        let percent = self.battery.map(|b| format!("{}%", b.percent.min(100))).unwrap_or_default();
        let texts = [self.clock_text.clone(), percent];
        for (slot, text) in self.lid_faces.iter_mut().zip(texts) {
            if slot.0 == text && slot.1 == dark && slot.2.is_some() { continue; }
            if text.is_empty() { *slot = (text, dark, slot.2, 0); continue; }
            let face = slot_ui::word_ink_scaled(&text, slot_ui::footer_ink(dark), 3);
            let tex = match slot.2 {
                Some(tex) => { self.gpu.update_face(tex, &face); tex }
                None => self.gpu.create_face(&face),
            };
            *slot = (text.clone(), dark, Some(tex), slot_ui::word_width(&text));
        }
    }

    /// The closed Rotate's clock and battery, grown from their corners
    /// on the top line as the lid closes (`t` 0..1).
    fn push_lid_status(&mut self, dark: bool, alpha: f32, commands: &mut Vec<Draw>) {
        self.refresh_lid_faces(dark);
        let printed = |f: &(String, bool, Option<slot_gfx::TexId>, u32)|
            f.2.filter(|_| f.3 > 0).map_or(Printed::default(), |tex| Printed::new(tex, f.3));
        let (clock, percent) = (printed(&self.lid_faces[0]), printed(&self.lid_faces[1]));
        let bolt = Some(if dark { self.bolt_light_texture } else { self.bolt_texture });
        slot_ui::draw_status_clock_grown(true, 1.0 + self.lid_k(), self.battery, percent, bolt,
            clock, slot_ui::footer_ink(dark), alpha, commands);
    }

    /// One of `lid_clock_faces`, made again only when `key` changes.
    fn lid_clock_face(&mut self, i: usize, key: &str, make: impl FnOnce() -> slot_ui::UndoFace)
        -> Option<(slot_gfx::TexId, f32, f32)> {
        if key.is_empty() { return None; }
        let slot = &mut self.lid_clock_faces[i];
        if slot.0 != key || slot.1.is_none() {
            let face = make();
            if face.w == 0 || face.h == 0 { return None; }
            let tex = match slot.1 {
                Some(tex) => { self.gpu.update_face(tex, &face); tex }
                None => self.gpu.create_face(&face),
            };
            *slot = (key.to_string(), Some(tex), face.w, face.h);
        }
        slot.1.map(|tex| (tex, slot.2 as f32, slot.3 as f32))
    }

    /// The closed Rotate's big time centred on `cx`, its middle at `y`,
    /// `scale` times the status bar's, then the date (`date`) and the
    /// battery under it.
    fn push_lid_big_clock(&mut self, cx: f32, y: f32, scale: f32, date: bool, alpha: f32,
        commands: &mut Vec<Draw>) {
        let mut next = y;
        let clock = self.clock_text.clone();
        if let Some((tex, w, h)) = self.lid_clock_face(0, &clock,
            || slot_ui::word_ink_scaled(&clock, slot_ui::footer_ink(false), LID_CLOCK_FINE)) {
            let k = scale / LID_CLOCK_FINE as f32;
            let (w, h) = (w * k, h * k);
            commands.push(Draw::Tex { x: cx - w / 2.0, y: y - h / 2.0, w, h, tex, alpha });
            next = y + h / 2.0;
        }
        if date {
            let text = self.date_text.clone();
            if let Some((tex, w, h)) = self.lid_clock_face(1, &text,
                || slot_ui::word_ink_scaled(&text, slot_ui::footer_ink(false), 3)) {
                let k = LID_DATE_SCALE / 3.0;
                let (w, h) = (w * k, h * k);
                commands.push(Draw::Tex { x: cx - w / 2.0, y: next, w, h, tex, alpha: alpha * 0.8 });
                next += h + 4.0;
            }
        }
        self.push_lid_battery(cx, next + 6.0, alpha, commands);
    }

    /// The battery as the status bar draws it, larger, centred on `cx`
    /// below `top`.
    fn push_lid_battery(&mut self, cx: f32, top: f32, alpha: f32, commands: &mut Vec<Draw>) {
        self.refresh_lid_faces(false);
        let f = &self.lid_faces[1];
        let percent = f.2.filter(|_| f.3 > 0).map_or(Printed::default(), |tex| Printed::new(tex, f.3));
        let width = slot_ui::status_gauge_width(self.battery, percent);
        let mut parts = Vec::new();
        slot_ui::draw_status_gauge(cx + width / 2.0, top, self.battery, percent,
            Some(self.bolt_texture), [1.0, 1.0, 1.0, 1.0], &mut parts);
        commands.extend(parts.into_iter().map(|d| lid_place(d, cx, top, LID_BATTERY_SCALE, 0.0, alpha)));
    }

    /// Analog Clock: the dial in the middle, the date and battery below.
    fn push_lid_dial(&mut self, cx: f32, alpha: f32, commands: &mut Vec<Draw>) {
        let size = LID_DIAL_SIZE;
        let top = LID_DIAL_TOP;
        if let Some((h, m)) = slot_ui::clock_hours_minutes(&self.clock_text) {
            let px = (size * panel_size().1 as f32 / OUT_H as f32).round().max(16.0) as u32;
            let key = format!("{h:02}:{m:02}/{px}");
            if let Some((tex, _, _)) = self.lid_clock_face(2, &key,
                || slot_ui::analog_clock_face(px, h, m, slot_ui::footer_ink(false))) {
                commands.push(Draw::Tex { x: cx - size / 2.0, y: top, w: size, h: size, tex, alpha });
            }
        }
        let mut next = top + size + 14.0;
        let text = self.date_text.clone();
        if let Some((tex, w, h)) = self.lid_clock_face(1, &text,
            || slot_ui::word_ink_scaled(&text, slot_ui::footer_ink(false), 3)) {
            let k = LID_DATE_SCALE / 3.0;
            let (w, h) = (w * k, h * k);
            commands.push(Draw::Tex { x: cx - w / 2.0, y: next, w, h, tex, alpha: alpha * 0.8 });
            next += h;
        }
        self.push_lid_battery(cx, next + 6.0, alpha, commands);
    }

    /// Game Info (Settings > Handheld > Game Info): NOW PLAYING at `top`, then
    /// the title and the system / time, under it or from `below` (Full
    /// Cart's, under the cart). Nothing when Off.
    fn push_game_info(&mut self, cx: f32, top: f32, below: Option<f32>, alpha: f32,
                      commands: &mut Vec<Draw>) {
        let info = self.settings.rotate_game_info.min(3);
        if info == 0 || alpha <= 0.0 { return; }
        let (title, time) = (info & 1 != 0, info & 2 != 0);
        let width = out_w() as f32;
        // At `scale` of the menus' 30 px face, centred, cut with a fade if
        // too wide for the screen.
        let line = |engine: &mut Self, slot: usize, text: String, y: f32, scale: f32,
                        lit: bool, a: f32, commands: &mut Vec<Draw>| -> f32 {
            let key = format!("{text}/{lit}");
            if slot == 4 && engine.lid_clock_faces[slot].0 != key {
                engine.lid_title_started = None;
            }
            let Some((tex, w, h)) = engine.lid_clock_face(slot, &key,
                || slot_ui::quick_value_face_within(&text, lit, 1400)) else { return 0.0 };
            let (w, h) = (w * scale, h * scale);
            let room = width - 48.0;
            if w <= room {
                commands.push(Draw::Tex { x: cx - w / 2.0, y, w, h, tex, alpha: a });
            } else {
                let travel = w - room;
                let shift = if slot == 4 && engine.lid_covered && a >= 1.0 {
                    let since = engine.lid_title_started.get_or_insert_with(Instant::now);
                    finite_title_scroll(travel, since.elapsed().as_secs_f32()).0
                } else { 0.0 };
                commands.push(Draw::Span(Some(slot_gfx::Span {
                    left: 24.0, right: width - 24.0,
                    feather_left: if shift > 0.5 { 24.0 } else { 0.01 },
                    feather_right: if travel - shift > 0.5 { 24.0 } else { 0.01 } })));
                commands.push(Draw::Tex { x: 24.0 - shift, y, w, h, tex, alpha: a });
                commands.push(Draw::Span(None));
            }
            h
        };
        line(self, 3, "NOW PLAYING".into(), top, 0.62, false, alpha * 0.85, commands);
        let mut y = below.unwrap_or(top + 34.0);
        let cart_title = self.shelves.get(self.active).and_then(|s| s.carts.get(s.index))
            .map(slot_ui::label_text).unwrap_or_default();
        let system = self.current_cart_platform().name().to_string();
        let played = played_text(self.play_time);
        if title && !cart_title.is_empty() {
            y += line(self, 4, cart_title, y, 0.78, true, alpha, commands) + 2.0;
        }
        let rest = match (title, time) {
            (true, true) => format!("{system}  \u{b7}  {played}"),
            (true, false) => system,
            _ => played,
        };
        let scale = if title { 0.52 } else { 0.7 };
        line(self, 5, rest, y, scale, !title, alpha * if title { 0.8 } else { 0.9 }, commands);
    }

    /// The closed Rotate in a game (Settings > Interface > RG Rotate > In
    /// Game): the screen switches off like a CRT, then the cover comes in.
    /// How far the lid's animation is, eased: it starts and ends softly,
    /// as the screen's own turn does (beta42).
    fn lid_k(&self) -> f32 {
        let t = self.lid_t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    fn draw_lid_game(&mut self) {
        let t = self.lid_k();
        let mode = self.settings.rotate_game_cover;
        let mut commands = Vec::new();
        if mode == 2 {
            // The game's last frame, dimmed.
            self.gpu.set_screen_power(1.0);
            commands.push(Draw::Game);
            commands.push(Draw::Rect { x: 0.0, y: 0.0, w: out_w() as f32, h: OUT_H as f32,
                colour: [0.0, 0.0, 0.0, 0.62 * t] });
            self.push_lid_status(false, t, &mut commands);
        } else {
            let screen = (1.0 - t * 2.0).clamp(0.0, 1.0);
            let cover = ((t - 0.5) * 2.0).clamp(0.0, 1.0);
            // First half: the game's screen switching off, alone. The cover
            // drawn over it hid it under black (beta42: the first 225 ms
            // were a black screen, then the cover all at once).
            if screen > 0.0 {
                self.gpu.set_screen_power(screen);
                commands.push(Draw::Game);
                self.gpu.fit(self.size);
                self.gpu.begin_frame();
                self.gpu.draw_list(&commands);
                self.gpu.end_frame(self.size);
                return;
            }
            let mut scene = Vec::new();
            if mode == 0 {
                self.push_lid_scene(&mut scene);
            } else {
                scene.push(Draw::Rect { x: 0.0, y: 0.0, w: out_w() as f32, h: OUT_H as f32,
                    colour: menu_style::page_ground([0.085, 0.085, 0.093, 1.0]) });
                if let Some(shelf) = self.shelves.get(self.active) {
                    shelf.draw_selected_centred(out_w() as f32 / 2.0, OUT_H as f32 * 0.55,
                        out_w() as f32 * 0.62, OUT_H as f32 * 0.56, 1.0, 1.0, &mut scene);
                }
            }
            // Full Cart settles in, a touch closer as it comes. Inserted
            // Cart keeps its size: shrunk, it pulled the part of the seated
            // cart below the screen's edge into view, label and all
            // (Siham 2026-11-14).
            let k = if mode == 0 { 1.0 } else { 0.94 + 0.06 * cover };
            let (cx, cy) = (out_w() as f32 / 2.0, OUT_H as f32 / 2.0);
            // Drawn whole, then faded from the switched-off screen's black:
            // fading each layer let the cart show through the slot's front,
            // label and all (Siham 2026-11-14).
            commands.extend(scene.into_iter().map(|d| lid_place(d, cx, cy, k, 0.0, 1.0)));
            if cover < 1.0 {
                commands.push(Draw::Rect { x: 0.0, y: 0.0, w: out_w() as f32, h: OUT_H as f32,
                    colour: [0.0, 0.0, 0.0, 1.0 - cover] });
            }
            if mode == 0 {
                // The cart sits low in its slot: the time and battery take
                // the free band above it, large, as in the first Rotate
                // betas (Siham 2026-11-14), and leave the corners empty.
                self.push_lid_big_clock(cx, LID_INSERTED_CLOCK_Y, LID_INSERTED_CLOCK_SCALE,
                    false, cover, &mut commands);
                // Game Info above the slot: NOW PLAYING, the title, then the
                // system and time on one line.
                self.push_game_info(cx, OUT_H as f32 - MOUTH_H - 132.0, None, cover, &mut commands);
            } else {
                self.push_lid_status(false, cover, &mut commands);
                // Full Cart: NOW PLAYING over the cart, the title, system
                // and time under it, like a music player's (Siham 2026-11-15).
                let cart = self.shelves.get(self.active).and_then(|shelf| shelf.selected_centred_box(
                    out_w() as f32 / 2.0, OUT_H as f32 * 0.55, out_w() as f32 * 0.62, OUT_H as f32 * 0.56));
                if let Some((_, y, _, h)) = cart {
                    // As lid_place puts it: k times as far from the middle.
                    let (top, bottom) = (cy + (y - cy) * k, cy + (y + h - cy) * k);
                    self.push_game_info(cx, top - 36.0, Some(bottom + 14.0), cover, &mut commands);
                }
            }
        }
        self.gpu.fit(self.size);
        self.gpu.begin_frame();
        self.gpu.draw_list(&commands);
        self.gpu.end_frame(self.size);
    }

    /// The closed Rotate with no game, over the shelf already drawn: the
    /// shelf steps back from the top line (Drifting Shelf), or the cover
    /// rises over it (Last Played, Random Cart), or the screen goes dark.
    /// `row`: the shelf's carts in `commands`, all Drifting Shelf shrinks
    /// (the slot bar keeps its size, Siham 2026-11-14).
    fn push_lid_shelf(&mut self, commands: &mut Vec<Draw>, row: std::ops::Range<usize>) {
        let t = self.lid_k();
        match self.settings.rotate_shelf_cover {
            0 => {
                // Shrunk from its foot, the shelf leaves the top line free.
                let k = 1.0 - 0.12 * t;
                let (cx, cy) = (out_w() as f32 / 2.0, OUT_H as f32);
                // Only the carts: the wallpaper and the slot bar stay put.
                let end = row.end.min(commands.len());
                for draw in &mut commands[row.start.min(end)..end] {
                    *draw = lid_place(*draw, cx, cy, k, 0.0, 1.0);
                }
            }
            mode @ (4 | 5) => {
                // Digital Clock, Analog Clock: the theme's ground, a big
                // time or a dial, the date and the battery.
                let g = menu_style::page_ground([0.085, 0.085, 0.093, 1.0]);
                commands.push(Draw::Rect { x: 0.0, y: 0.0, w: out_w() as f32, h: OUT_H as f32,
                    colour: [g[0], g[1], g[2], t] });
                let cx = out_w() as f32 / 2.0;
                if mode == 4 {
                    self.push_lid_big_clock(cx, LID_DIGITAL_CLOCK_Y, LID_DIGITAL_CLOCK_SCALE,
                        true, t, commands);
                } else {
                    self.push_lid_dial(cx, t, commands);
                }
            }
            mode => {
                commands.push(Draw::Rect { x: 0.0, y: 0.0, w: out_w() as f32, h: OUT_H as f32,
                    colour: if mode == 3 { [0.0, 0.0, 0.0, t] }
                        else { let g = menu_style::page_ground([0.085, 0.085, 0.093, 1.0]);
                               // Opaque once closed: no shelf or slot bar
                               // ghosting through (Siham 2026-11-15).
                               [g[0], g[1], g[2], t] } });
                if mode != 3 {
                    if let Some(shelf) = self.shelves.get(self.active) {
                        // The shelf's own cart lifts to the middle.
                        shelf.draw_selected_centred(out_w() as f32 / 2.0, OUT_H as f32 * 0.55,
                            out_w() as f32 * 0.62, OUT_H as f32 * 0.56, t,
                            (t * 4.0).min(1.0), commands);
                    }
                }
            }
        }
    }

    /// The closed RG Rotate during a game: the hold scene's cartridge,
    /// seated, in front of a switched-off screen.
    fn push_lid_scene(&self, commands: &mut Vec<Draw>) {
        commands.push(Draw::Rect {
            x: 0.0, y: 0.0, w: out_w() as f32, h: OUT_H as f32,
            colour: menu_style::page_ground([0.085, 0.085, 0.093, 1.0]),
        });
        let Some(shelf) = self.shelves.get(self.active) else { return };
        let Some(cart) = shelf.carts.get(shelf.index) else { return };
        let (rest, scale) = shelf.selected_at();
        shelf.draw_row(Some(cart.stem.as_str()), 0.0, 1.0, 1.0, commands);
        SlotChrome {
            cart, face: shelf.face(shelf.index), rest, scale,
            seat: 1.0, alert: None, dim: 0.0,
            screen: 0.0, game: true, over: false,
            slot_faces: pick_slot_faces(self.slot_faces, self.home_slot_faces, Some(cart)),
        }.draw_with_closer(commands, self.clear_layers().0, self.clear_layers().1,
            pick_closer(self.slot_faces, self.home_closers, Some(cart)), false);
    }

    fn refresh_selected_title(&mut self) {
        if !self.settings.show_titles || self.game.is_some() {return;}
        // Original v1.7 shelf Game Titles use label_text() (cleaned
        // filename), NOT the GBA internal ROM header. Reuse the same value
        // and Slot's native fit/ellipsis, without arbitrary 39-char clipping.
        let name=self.shelves.get(self.active)
            .and_then(|s|s.carts.get(s.index))
            .map(slot_ui::label_text).unwrap_or_default();
        // The screen's width is part of it: the title fits its window.
        let name=format!("{name}\u{1f}{}", out_w());
        if name==self.selected_title_name {return;}
        self.selected_title_name=name.clone();
        let name=name.split('\u{1f}').next().unwrap_or_default().to_string();
        if name.is_empty(){self.selected_title_face=Printed::default();return;}
        let image=slot_ui::title_line_face(&name);
        if image.w==0 || image.h==0 {
            self.selected_title_face=Printed::default();
            return;
        }
        let tex=if let Some(tex)=self.selected_title_texture {
            self.gpu.update_face(tex, &image);
            tex
        }else{
            let tex=self.gpu.create_face(&image);
            self.selected_title_texture=Some(tex);
            tex
        };
        self.selected_title_face=Printed::new(tex,image.w);
    }

    fn cycle_appearance(&mut self, right: bool) {
        self.settings.appearance=self.settings.appearance.step(right);
        menu_style::set_oled(self.settings.appearance == crate::settings::Appearance::Oled);
        self.home_slot_faces=self.home_slot_themes[self.settings.appearance.bar_theme()];
        let custom_colours = PATHS.lock().unwrap_or_else(|e| e.into_inner())
            .as_ref().is_some_and(|(root,_)|
                root.join("Config/theme.txt").is_file());
        self.slot_faces=if custom_colours { None }
            else if self.settings.appearance == crate::settings::Appearance::Light {
                Some(self.slot_light_faces)
            } else { Some(self.slot_dark_faces) };
        self.recolour_footer_text();
        // Default's mosaics take the theme's colours.
        if self.settings.wallpaper.is_mosaic()
            || self.settings.wallpaper_shelves.values().any(|pick| pick.is_mosaic()) {
            let root = PATHS.lock().unwrap_or_else(|e| e.into_inner())
                .as_ref().map(|(root, _)| root.clone());
            self.reload_wallpaper(root.as_deref());
        }
        self.persist_settings();
    }

    fn change_setting(&mut self, key: usize, right: bool) {
        if key == SCREEN_SIZE_KEY {
            if let ShelfOverlay::ScreenSystemSettings { platform, .. } = self.overlay {
                self.cycle_system_video_mode(platform, right);
            }
            return;
        }
        if let Some(platform)=Self::core_key_platform(key) {
            self.cycle_default_core(platform);
            return;
        }
        if matches!(key,2|22) && self.settings.shader_gba.includes_gba_color() {
            set_message("This shader already includes color correction.".into());
            return;
        }
        if !self.settings.change(key, right) { return; }
        // Clock formatting can change independently of Android's next
        // status event. Force one re-render of the existing clock value.
        if key == 28 { self.last_system_status_rev = 0; }
        if Settings::lcd_effect_platform(key).is_some() && self.game.is_some() { self.apply_game_look(); }
        if key == 5 {
            FF_AUDIO_ENABLED.store(self.settings.ff_sound, Ordering::Release);
            AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
        if matches!(key,0|1|21|2|22|23|24) {
            let platform=self.current_cart_platform();
            let view=self.game_settings();
            let colour=view.shader_for(platform).core_colour(view.colour_for(platform));
            if let Some(game) = self.game.as_mut() {
                game.set_colour_correction(colour);
            }
        }
        self.persist_settings();
    }

    /// The Palette row's value face while a palette is on; None shows OFF.
    fn palette_value(&self) -> Option<(slot_gfx::TexId, u32, u32)> {
        let scoped = if matches!(self.overlay, ShelfOverlay::PauseScreenSettings { .. }) {
            self.game_settings()
        } else { self.settings.clone() };
        scoped.gb_palettes.then(|| self.palette_values.get(scoped.gb_palette as usize).copied()).flatten()
    }

    /// The value face a Settings key shows, None for an action-only row.
    fn setting_value(&self, key: usize) -> Option<usize> {
        if key == SCREEN_SIZE_KEY && self.pokedaisy_running()
            && matches!(self.overlay, ShelfOverlay::PauseScreenSettings { .. }) {
            return Some(pokedaisy_view_value(self.pokedaisy_view.within(self.pokedaisy_zoom_steps())));
        }
        if key == POKEDAISY_INTRO_KEY {
            return Some(pokedaisy_view_value(if self.pokedaisy_intro {
                crate::pokedaisy_view::View::Expand
            } else { crate::pokedaisy_view::View::Original }));
        }
        let profile = if matches!(self.overlay, ShelfOverlay::PauseEmulationSettings { .. }) {
            self.current_emulation()
        } else { EmulationProfile::from_settings(&self.settings) };
        let scoped = if matches!(self.overlay,
            ShelfOverlay::PauseScreenSettings { .. } |
            ShelfOverlay::PauseGameplaySettings { .. }) {
            self.game_settings()
        } else { self.settings.clone() };
        let video = match self.overlay {
            ShelfOverlay::ScreenSystemSettings { platform, .. } =>
                self.system_video_modes[platform as usize],
            // Original Ratio shown as Integer while they are the same picture.
            _ => self.video_mode.on_when(self.game_integer_is_ratio()),
        };
        setting_face(key, &self.settings, &scoped, &profile,
            self.current_cart_platform(), video, &self.default_cores)
    }

    /// Library's values: the slot. folder's place (nothing without one);
    /// COPYING n% while its first copy runs. (row, face, toggle)
    fn library_value_faces(&self) -> Vec<(usize, (slot_gfx::TexId, u32, u32), bool)> {
        if SLOT_FOLDER_COPY.load(Ordering::Acquire) >= 0 {
            return vec![(LIBRARY_SLOT_ROW, self.slot_copy_face, false)];
        }
        let place = SLOT_FOLDER.load(Ordering::Acquire);
        if (1..=2).contains(&place) {
            vec![(LIBRARY_SLOT_ROW, self.library_values[usize::from(place) - 1], false)]
        } else { Vec::new() }
    }

    /// Saves & States' values: Share Core Saves and Sync Saves with RomM,
    /// with the exact Settings ON/OFF textures and carets.
    fn saves_value_faces(&self) -> Vec<(usize, (slot_gfx::TexId, u32, u32), bool)> {
        let share = if SHARE_SAVES_ENABLED.load(Ordering::Acquire) { 4 } else { 5 };
        // Without RomM it reads OFF; turning it on says where to connect
        // (Siham 2026-11-19).
        let romm = match ROMM_SYNC.load(Ordering::Acquire) {
            2 => self.setting_values[4],
            _ => self.setting_values[5],
        };
        let states = match ROMM_STATES.load(Ordering::Acquire) {
            2 => self.setting_values[4],
            _ => self.setting_values[5],
        };
        vec![(SAVES_SHARE_ROW, self.setting_values[share], true), (SAVES_ROMM_ROW, romm, true),
            (SAVES_STATES_ROW, states, true)]
    }

    /// Saves & States' row shown but idle: Sync States with RomM while Sync
    /// Saves with RomM is off.
    fn saves_row_idle(at: usize) -> bool {
        at == SAVES_STATES_ROW && ROMM_STATES.load(Ordering::Acquire) == 3
    }

    /// A value's short form when the whole one has no room at `scale`,
    /// even 10 % smaller, and the short one has more.
    fn value_within(&self, value: (slot_gfx::TexId, u32, u32), scale: f32, room: f32)
        -> (slot_gfx::TexId, u32, u32) {
        if value.1 as f32 * scale * 0.9 <= room { return value; }
        self.short_values.iter().find(|(whole, _)| *whole == value.0).map_or(value, |&(_, short)| short)
    }

    fn draw_page_values(&self, heading: usize, items: &[usize], row: usize,
                        values: &[(usize, (slot_gfx::TexId, u32, u32), bool)],
                        scales: &[(f32, f32)], out: &mut Vec<Draw>) {
        for &(at, face, toggle) in values {
            let scale = scales.get(at).map_or(0.79, |s| s.1);
            let label_w = items.get(at).and_then(|&l| self.menu_textures.get(l)).map_or(0, |t| t.1) as f32;
            self.draw_row_value((heading, at), items.len(), row, face, toggle, scale, label_w * 0.79, out);
        }
    }

    /// A label beside its value gives way to it (beta39: on the RG Rotate's
    /// square screen, SD CARD and ON / OFF lay over their labels): the room
    /// the value and its carets leave, the same selected or not. Rows
    /// without a value fit as before (0.0).
    fn value_row_scales(&self, items: &[usize],
                        values: &[(usize, (slot_gfx::TexId, u32, u32), bool)]) -> Vec<(f32, f32)> {
        // Every label at one size, as on every value page (Siham
        // 2026-11-19): a value too long for its room scrolls instead.
        (0..items.len()).map(|i| if values.iter().any(|v| v.0 == i) { (0.79, 0.79) } else { (0.0, 0.79) })
            .collect()
    }

    /// A value at the right of a menu row; a toggle shows its carets when
    /// selected.
    #[allow(clippy::too_many_arguments)]
    fn draw_row_value(&self, (heading, at): (usize, usize), rows: usize, row: usize,
                      (tex, width, height): (slot_gfx::TexId, u32, u32), toggle: bool,
                      scale: f32, label_w: f32, out: &mut Vec<Draw>) {
        // Its row among the six shown while the page scrolls.
        let Some((dy, shows)) = self.menu_row_place(at, rows) else { return };
        let right = out_w() as f32 - 34.0;
        let idle = heading == 151 && Self::saves_row_idle(at);
        let selected = row == at && !idle;
        let carets = toggle && selected;
        let caret = if toggle { self.setting_carets[0].1 as f32 + 8.0 } else { 0.0 };
        // Past its label, a gap and the left caret: a little too long, up
        // to 10 % smaller (Siham 2026-11-19); longer, it scrolls (as
        // draw_values).
        // Faces carry clear margins (MENU_PAD): the label's right one needs
        // no room of its own.
        let from = 37.0 + label_w + 18.0 + caret - slot_ui::MENU_PAD as f32 * 0.79;
        let end = right - if toggle { 22.0 } else { 0.0 };
        let (tex, width, height) = self.value_within((tex, width, height), scale, end - from);
        let scale = match width as f32 * scale {
            w if w > end - from && w * 0.9 <= end - from => scale * (end - from) / w,
            _ => scale,
        };
        let y = self.menu_rows_top(rows.min(self.visible_rows())) + dy + 8.0
            + height as f32 * (0.79 - scale) / 2.0;
        let w = width as f32 * scale;
        let end = right - if carets { 22.0 } else { 0.0 };
        let alpha = shows * if selected { 1.0 } else if idle { 0.3 } else { menu_look::ROW_REST };
        let x = if end - w >= from {
            out.push(Draw::Tex { x: end - w, y, w, h: height as f32 * scale, tex, alpha });
            end - w
        } else {
            let travel = w - (end - from);
            let shift = if selected { self.marquee_offset((heading, at), travel) } else { 0.0 };
            out.push(Draw::Span(Some(slot_gfx::Span {
                left: from, right: end,
                feather_left: shift.min(18.0), feather_right: (travel - shift).min(18.0),
            })));
            out.push(Draw::Tex { x: from - shift, y, w, h: height as f32 * scale, tex, alpha });
            out.push(Draw::Span(None));
            from
        };
        if carets {
            let (left, lw, lh) = self.setting_carets[0];
            let (right_tex, rw, rh) = self.setting_carets[1];
            let y = self.caret_y(y, scale);
            out.push(Draw::Tex { x: x - lw as f32 - 8.0, y,
                w: lw as f32, h: lh as f32, tex: left, alpha: shows });
            out.push(Draw::Tex { x: right - rw as f32, y,
                w: rw as f32, h: rh as f32, tex: right_tex, alpha: shows });
        }
    }

    /// The face of a setting's current value; None for an action row.
    fn value_face(&self, key: usize) -> Option<(slot_gfx::TexId, u32, u32)> {
        // Only action rows (usize::MAX) have no value: setting_face
        // answers every other key, so a new key can't go blank.
        Some(if key == MOTION_KEY {
                    self.setting_values[54 + usize::from(self.current_emulation().motion())]
                } else if key == SUN_KEY {
                    let sun = self.current_emulation().sun(light_sensor(), self.sun_top());
                    self.setting_values[match sun {
                        crate::emulation::SUN_SENSOR => 53,
                        crate::emulation::SUN_AUTO => 66,
                        sun => 54 + sun as usize,
                    }]
                } else if key == SHELF_PICK_KEY {
                    match self.wallpaper_edit {
                        Some(shelf) => *self.shelf_name_faces.get(shelf)?,
                        None => self.setting_values[GENERAL_VALUE],
                    }
                } else if key == WALLPAPER_PICK_KEY {
                    use crate::settings::WallpaperPick;
                    let Some(shelf) = self.wallpaper_edit else { return self.value_face(16) };
                    match self.settings.shelf_wallpaper(shelf) {
                        None => self.setting_values[GENERAL_VALUE],
                        Some(WallpaperPick::File(_)) =>
                            self.wallpaper_edit_face.unwrap_or(self.setting_values[26]),
                        Some(WallpaperPick::None) => self.setting_values[17],
                        Some(WallpaperPick::Random) => self.setting_values[18],
                        Some(WallpaperPick::Blur) => self.setting_values[20],
                        Some(WallpaperPick::Halo) => self.setting_values[HALO_WALLPAPER_VALUE],
                        Some(WallpaperPick::Slot) => self.setting_values[SLOT_WALLPAPER_VALUE],
                        Some(WallpaperPick::SlotBlur) => self.setting_values[SLOT_BLUR_WALLPAPER_VALUE],
                        Some(_) => self.setting_values[19],
                    }
                } else if matches!(key, BLUR_KEY | HALO_KEY | DARKEN_KEY) {
                    let look = self.edited_look();
                    self.setting_values[percent_value(match key {
                        BLUR_KEY => look.blur, HALO_KEY => look.halo, _ => look.darken,
                    })]
                } else if key == 16 && matches!(self.settings.wallpaper,
                    crate::settings::WallpaperPick::File(_)) {
                    self.wallpaper_name_face.unwrap_or(self.setting_values[26])
                } else if let Some(face) = (key == 3).then(|| self.palette_value()).flatten() {
                    face
                } else if let Some(face) = self.setting_value(key)
                    .and_then(|value| self.setting_values.get(value)) {
                    *face
                } else { return None })
    }

    /// Every label and value at one size (Siham 2026-11-19: rows of
    /// different sizes looked wrong); a value too long scrolls
    /// (draw_values).
    fn fit_scales(&self, items: &[usize], _keys: &[usize]) -> Vec<(f32, f32)> {
        vec![(0.79, 0.79); items.len()]
    }

    /// A settings page: its rows, then their values, fitted together.
    fn draw_settings_menu(&self, heading: usize, items: &[usize], row: usize,
                          keys: &[usize], out: &mut Vec<Draw>) {
        // One value per row, never more: a value left over from a row
        // taken away pushed every value off its row (3.2.0-beta9).
        if keys.len() != items.len() {
            static TOLD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !TOLD.swap(true, Ordering::Relaxed) {
                crate::log_warn(&format!("settings page {heading}: {} rows, {} values", items.len(), keys.len()));
            }
        }
        let keys = &keys[..keys.len().min(items.len())];
        let scales = self.fit_scales(items, keys);
        self.draw_menu_rows(heading, items, row, &scales, out);
        self.draw_values(heading, items, row, keys, &scales, out);
        self.draw_menu_chrome(heading, items.len(), row, true, out);
    }

    /// How far a long text has scrolled in its window, `travel` px in all:
    /// still a moment, along at an even pace, still a moment at its end,
    /// then back to its start. 0 for text that fits.
    pub(super) fn marquee_offset(&self, what: (usize, usize), travel: f32) -> f32 {
        marquee_shift(&self.marquee, what, travel)
    }

    /// The selected game's title too long for its window (#119): from the
    /// moment it shows, three passes as the menus' long values (a moment
    /// still, along, a moment at its end), then at rest at its start, so
    /// the settled shelf still dozes. With whether it is moving.
    fn title_scroll(&self) -> (f32, bool) {
        let travel = slot_ui::slot_title_travel(self.selected_title_face);
        if !self.settings.show_titles || travel <= 0.5 { return (0.0, false); }
        let now = Instant::now();
        let key = (self.active, self.shelves[self.active].index, travel.to_bits());
        let since = match self.title_marquee.get() {
            Some((seen, at, _)) if seen == key => at,
            _ => now,
        };
        self.title_marquee.set(Some((key, since, now)));
        finite_title_scroll(travel, now.duration_since(since).as_secs_f32())
    }
}

/// Same finite marquee for the carousel and the closed Rotate cover.
fn finite_title_scroll(travel: f32, t: f32) -> (f32, bool) {
    if travel <= 0.5 { return (0.0, false); }
    const HOLD: f32 = 1.2;
    const PACE: f32 = 40.0;
    const PASSES: f32 = 3.0;
    let run = travel / PACE;
    let pass = HOLD + run + HOLD;
    if t >= PASSES * pass { return (0.0, false); }
    (((t % pass - HOLD) / run).clamp(0.0, 1.0) * travel, true)
}

#[cfg(test)]
#[test]
fn finite_title_scroll_holds_moves_and_stops_after_three_passes() {
    assert_eq!(finite_title_scroll(0.0, 2.0), (0.0, false));
    assert_eq!(finite_title_scroll(80.0, 1.0), (0.0, true));
    assert!((finite_title_scroll(80.0, 2.2).0 - 40.0).abs() < 0.01);
    assert_eq!(finite_title_scroll(80.0, 3.5), (80.0, true));
    assert_eq!(finite_title_scroll(80.0, 4.5), (0.0, true));
    assert_eq!(finite_title_scroll(80.0, 13.3), (0.0, false));
}

/// How far a long text has scrolled, kept in `cell` (see marquee_offset).
fn marquee_shift(cell: &std::cell::Cell<Option<((usize, usize, u32), Instant, Instant)>>,
                 what: (usize, usize), travel: f32) -> f32 {
    {
        if travel <= 0.5 { return 0.0; }
        let now = Instant::now();
        // From its start whenever it comes back into view or changes
        // (another value is another travel): it once picked up where an
        // earlier pass had left off, at its end (Siham 2026-11-15).
        let key = (what.0, what.1, travel.to_bits());
        let since = match cell.get() {
            Some((seen, at, last)) if seen == key
                && now.duration_since(last) < Duration::from_millis(200) => at,
            _ => now,
        };
        cell.set(Some((key, since, now)));
        const HOLD: f32 = 1.2;
        const PACE: f32 = 40.0;
        let run = travel / PACE;
        let t = now.duration_since(since).as_secs_f32() % (HOLD + run + HOLD);
        ((t - HOLD) / run).clamp(0.0, 1.0) * travel
    }
}

impl Engine {
    fn draw_values(&self, heading: usize, items: &[usize], row: usize, keys: &[usize],
                   scales: &[(f32, f32)], out: &mut Vec<Draw>) {
        // Six rows at most on screen; a longer page scrolls (as Controls).
        // Placed by the rows: a value always sits on its own row.
        let top = self.menu_rows_top(items.len().min(self.visible_rows()));
        let caret = self.setting_carets[0].1 as f32 + 8.0;
        for (index, key) in keys.iter().copied().enumerate() {
            let Some((dy, shows)) = self.menu_row_place(index, items.len()) else { continue };
            let Some((tex, width, height)) = self.value_face(key) else { continue };
            let (label_scale, scale) = scales.get(index).copied().unwrap_or((0.79, 0.79));
            let right = out_w() as f32 - 34.0;
            // Where the value may start: past its label, a gap and the
            // left caret.
            let label_w = items.get(index).and_then(|&l| self.menu_textures.get(l))
                .map_or(0, |t| t.1) as f32 * label_scale;
            // Faces carry clear margins (MENU_PAD): the label's right one
            // needs no room of its own.
            let from = 37.0 + label_w + 18.0 + caret - slot_ui::MENU_PAD as f32 * 0.79;
            // A little too long beside the carets: up to 10 % smaller, the
            // same chosen or not, rather than a scroll (Last Played, Siham
            // 2026-11-15; 10 %, 2026-11-19).
            let room = right - 22.0 - from;
            // Too long even so: its short form, if it has one (ORIG. RATIO).
            let (tex, width, height) = self.value_within((tex, width, height), scale, room);
            let scale = match width as f32 * scale {
                w if w > room && w * 0.9 <= room => scale * room / w,
                _ => scale,
            };
            let y = top + dy + 8.0 + height as f32 * (0.79 - scale) / 2.0;
            let w = width as f32 * scale;
            // Interface > Wallpaper's looks, idle over None or General's.
            let idle = matches!(self.overlay, ShelfOverlay::WallpaperSettings { .. })
                && match key {
                    BLUR_KEY => self.wallpaper_look_idle(BLUR_ROW),
                    HALO_KEY => self.wallpaper_look_idle(HALO_ROW),
                    DARKEN_KEY => self.wallpaper_look_idle(DARKEN_ROW),
                    _ => false,
                };
            let chosen = row == index && !idle;
            let end = right - if chosen { 22.0 } else { 0.0 };
            let alpha = shows * if chosen { 1.0 } else if idle { 0.3 } else { menu_look::ROW_REST };
            let x = if end - w >= from {
                out.push(Draw::Tex { x: end - w, y, w, h: height as f32 * scale, tex, alpha });
                end - w
            } else {
                // Too long: its start at rest, cut with a fade; scrolling
                // through while chosen (Siham 2026-11-15).
                let travel = w - (end - from);
                let shift = if chosen { self.marquee_offset((heading, index), travel) } else { 0.0 };
                const FEATHER: f32 = 18.0;
                out.push(Draw::Span(Some(slot_gfx::Span {
                    left: from, right: end,
                    feather_left: shift.min(FEATHER), feather_right: (travel - shift).min(FEATHER),
                })));
                out.push(Draw::Tex { x: from - shift, y, w, h: height as f32 * scale, tex, alpha });
                out.push(Draw::Span(None));
                from
            };
            if chosen {
                // A smaller value stays centred on the 0.79 label's middle.
                let y = self.caret_y(top + dy + 8.0, 0.79);
                let (left, lw, lh) = self.setting_carets[0];
                let (right_tex, rw, rh) = self.setting_carets[1];
                out.push(Draw::Tex {
                    x: x - lw as f32 - 8.0, y, w: lw as f32,
                    h: lh as f32, tex: left, alpha: shows,
                });
                out.push(Draw::Tex {
                    x: right - rw as f32, y, w: rw as f32,
                    h: rh as f32, tex: right_tex, alpha: shows,
                });
            }
        }
    }

    /// slot. original's legends with the slot up: the first at the left
    /// edge, the second at the right, clear of the bay.
    /// The legends either side of the seated cart's slot, each with its
    /// word or, beside a slot too wide for it, its key alone (the Super
    /// Nintendo's and Mega Drive's on the RG Rotate, Siham 2026-11-15).
    fn draw_flanking_hints(&self, hints: [((slot_gfx::TexId, u32), (slot_gfx::TexId, u32)); 2],
                           out: &mut Vec<Draw>) {
        let seen = |w: u32| w.saturating_sub(slot_ui::HINT_EDGE) as f32;
        let width = out_w() as f32;
        // Closer to the edges on the narrowest screens (1:1), so the right
        // legend still clears the bay.
        let margin = if width < 520.0 { 8.0 } else { 16.0 };
        // A console's own slot (NES, Super Nintendo, Mega Drive, Famicom)
        // keeps its 320 px middle at every width; the GBA bar its mouth.
        let home = self.shelves.get(self.active).and_then(|s| s.carts.get(s.index))
            .and_then(slot_ui::bitmaps::home_slot_family).is_some();
        let bay = if home && self.slot_faces.is_some() { HOME_SLOT_BAY_W } else { slot_ui::MOUTH_W };
        let (bay_left, bay_right) = ((width - bay) / 2.0, (width + bay) / 2.0);
        let y = self.legend_y();
        let (full, short) = hints[0];
        let (tex, w) = if margin + seen(full.1) <= bay_left - 4.0 { full } else { short };
        out.push(Draw::Tex { x: margin, y, w: w as f32, h: HINT_H as f32, tex, alpha: 1.0 });
        let (full, short) = hints[1];
        let (tex, w) = if width - margin - seen(full.1) >= bay_right + 4.0 { full } else { short };
        let x = (width - margin - seen(w)).max(bay_right + 4.0).round();
        out.push(Draw::Tex { x, y, w: w as f32, h: HINT_H as f32, tex, alpha: 1.0 });
    }

    /// The in-game Screen page: rows above the slot, the game in view above,
    /// as slot. original's in-game menu. Every other in-game page is opaque
    /// (Siham 2026-11-06).
    /// The game's system has a Screen Size (the screen hasn't the system's
    /// own shape, as Systems > Screen Size), so L1 / R1 size the picture
    /// on the in-game Screen page, row or not.
    /// A 3.0 system's option the paused game shows at once, redrawn.
    fn previewed_extra(key: usize) -> bool {
        use crate::settings::*;
        Settings::lcd_effect_platform(key).is_some()
            || matches!(key, SPRITE_LIMIT_SMS_KEY | SPRITE_LIMIT_GG_KEY | LEFT_BAR_SMS_KEY | LEFT_BAR_NES_KEY
            | WS_PALETTE_KEY
            | POKEMINI_PALETTE_KEY | POKEMINI_FILTER_KEY | POKEMINI_LCD_MODE_KEY)
            || (SHADER_SMS_KEY..=SHADER_GG_KEY).contains(&key)
            || matches!(key, SHADER_NGP_KEY | SHADER_WS_KEY | SHADER_LYNX_KEY | SHADER_POKEMINI_KEY | SHADER_PICO8_KEY
                | SHADER_NEOGEO_KEY | SHADER_PCE_KEY)
    }
    fn screen_page_sizes(&self) -> bool {
        !video_mode::screen_fits(self.current_cart_platform(), panel_size())
    }

    fn screen_panel(&self) -> bool {
        self.game.is_some() && matches!(self.overlay, ShelfOverlay::PauseScreenSettings { .. })
    }

    /// Where a menu's first row goes.
    fn menu_rows_top(&self, count: usize) -> f32 {
        if self.screen_panel() {
            (OUT_H as f32 - MOUTH_H - 6.0 - QUICK_PITCH * count as f32).round()
        } else { settings_row_top(count) }
    }

    /// How far the slot still is below its place (0 when up).
    fn footer_dy(&self) -> f32 { (1.0 - slot_ui::ease(self.footer)) * MOUTH_H }

    /// The legend's line: in the slot on the in-game Screen page, at the
    /// bottom otherwise.
    fn legend_y(&self) -> f32 {
        if self.screen_panel() {
            OUT_H as f32 - MOUTH_H + ((MOUTH_H - HINT_H as f32) / 2.0).round() + self.footer_dy()
        } else { 427.0 }
    }

    /// slot. original's seated slot: the slot with the playing cart in it,
    /// rising from below with the in-game menu.
    fn draw_game_footer(&self, out: &mut Vec<Draw>) {
        if self.footer <= 0.0 || self.game.is_none() { return; }
        let Some(shelf) = self.shelves.get(self.active) else { return };
        let Some(cart) = shelf.carts.get(shelf.index) else { return };
        let (rest, scale) = shelf.selected_at();
        let start = out.len();
        SlotChrome {
            cart, face: shelf.face(shelf.index), rest, scale,
            seat: 1.0, alert: None, dim: 0.0, screen: 0.0, game: false, over: true,
            slot_faces: pick_slot_faces(self.slot_faces, self.home_slot_faces, Some(cart)),
        }.draw_with_closer(out, [None, None], None,
            pick_closer(self.slot_faces, self.home_closers, Some(cart)), false);
        shift_down(&mut out[start..], self.footer_dy());
    }

    /// A menu's ground: opaque, except on the in-game Screen page: there the
    /// paused game, a scrim behind the rows, then the slot.
    fn draw_menu_ground(&self, rows_top: f32, out: &mut Vec<Draw>) {
        if !self.screen_panel() {
            out.push(Draw::Rect {
                x: 0.0, y: 0.0, w: out_w() as f32, h: OUT_H as f32,
                colour: menu_ground(),
            });
            return;
        }
        let y = rows_top - 8.0;
        let bottom = OUT_H as f32 - MOUTH_H + self.footer_dy();
        out.push(Draw::Rect {
            x: 0.0, y, w: out_w() as f32, h: (bottom - y).max(0.0),
            // The same in OLED: the Screen page tunes the picture behind it.
            colour: [0.0, 0.0, 0.0, GAME_MENU_SCRIM],
        });
        self.draw_game_footer(out);
    }

    fn draw_original_style_menu(&self, heading: usize, items: &[usize],
                                selected: usize, out: &mut Vec<Draw>) {
        self.draw_menu_rows(heading, items, selected, &[], out);
        self.draw_menu_chrome(heading, items.len(), selected, false, out);
    }

    /// The fade a menu page opens with.
    /// The carets' top beside a value drawn from `text_y` at `scale`: their
    /// ink centred on its capitals, not hanging from the same top.
    pub(super) fn caret_y(&self, text_y: f32, scale: f32) -> f32 {
        text_y - (1.0 - scale) * self.caret_mid
    }

    fn menu_fade(&self, heading: usize) -> f32 {
        // The game menu fades in over 100 ms, after its 120 ms wait for a
        // second tap; every other menu over the original 160 ms.
        let fade_s = if heading == 19 { 0.10 } else { 0.16 };
        slot_ui::ease((self.menu_opened.elapsed().as_secs_f32() / fade_s).clamp(0.0, 1.0))
    }

    /// A menu page's ground, the bar under its selected row and its rows.
    /// `scales`: each row's label scale (fit_scales), or none for 0.79.
    fn draw_menu_rows(&self, heading: usize, items: &[usize], selected: usize,
                      scales: &[(f32, f32)], out: &mut Vec<Draw>) {
        // The original Slot quick menu's 52px pitch and type. Six rows at
        // most on screen; a longer page slides its list (as Controls).
        let shown = items.len().min(self.visible_rows());
        let top = self.menu_rows_top(shown);
        self.draw_menu_ground(top, out);
        let fade = self.menu_fade(heading);
        // The main and game menus leave the right of a wide screen to the
        // selected cart.
        let side = self.menu_side_room(heading, items);
        let x1 = out_w() as f32 - side.unwrap_or(PILL_RIGHT);
        self.draw_menu_cursor(top, selected, items.len(), x1, fade, out);
        for (i, index) in items.iter().copied().enumerate() {
            let Some((dy, shows)) = self.menu_row_place(i, items.len()) else { continue };
            let x = 37.0;
            let y = top + dy + 8.0 + (1.0 - fade) * 8.0;
            let chosen = i == selected;
            let idle = (heading == 148 && self.general_row_dimmed(index))
                || (heading == 151 && Self::saves_row_idle(i));
            let alpha = fade * shows * if idle { 0.4 } else if chosen { 1.0 } else { menu_look::ROW_REST };
            let opens = self.menu_row_opens(heading, index);
            match scales.get(i) {
                Some(&(scale, _)) if scale > 0.0 => self.text_scaled(index, x, y, scale, alpha, out),
                _ => self.text_fit(index, x, y, x1 - x - if opens { 36.0 } else { 14.0 }, alpha, out),
            }
            if opens {
                let (tex, w, h) = self.menu_opens;
                let w = w as f32 * 0.79;
                out.push(Draw::Tex {
                    x: (x1 - 14.0 - w).round(), y, w, h: h as f32 * 0.79, tex,
                    alpha: fade * shows * if chosen { 1.0 } else { 0.4 },
                });
            }
        }
        if let Some(room) = side {
            self.draw_menu_side(room, top, shown, fade, out);
        }
    }

    /// What frames a menu page's rows: its scroll bar, its path line and
    /// title, and its keycap legend. Drawn after the rows and their values,
    /// so a long page's sliding rows pass under it.
    fn draw_menu_chrome(&self, heading: usize, count: usize, selected: usize,
                        values: bool, out: &mut Vec<Draw>) {
        let top = self.menu_rows_top(count.min(self.visible_rows()));
        let fade = self.menu_fade(heading);
        // The in-game Screen page leaves the game in view above its rows,
        // and its legend goes either side of the seated cart.
        if self.screen_panel() {
            self.draw_menu_scrollbar(top, count, fade, out);
            if values {
                // B Back | L1 R1 Size; where the screen already has the
                // system's shape there is no size: arrows Change.
                let right = if self.screen_page_sizes() { (self.screen_size_hint, self.screen_key_hints[1]) }
                    else { (self.settings_hints[1], self.screen_key_hints[2]) };
                self.draw_flanking_hints([(self.settings_hints[0], self.screen_key_hints[0]), right], out);
            }
            return;
        }
        self.draw_menu_bands(top, count, out);
        self.draw_menu_scrollbar(top, count, fade, out);
        self.draw_menu_header(self.menu_path(heading), self.menu_textures.get(heading).copied(), fade, out);
        // All menus use the original Slot keycap generator. The paused game
        // screen additionally advertises its active SAVE / LOAD shortcuts.
        let hints: &[(slot_gfx::TexId, u32)] = if values {
            // The original Back / Change physical keycap legend.
            &self.settings_hints
        } else if heading == 19 {
            let linked = self.game.as_ref().is_some_and(|session|
                session.linked() || session.requires_relaunch());
            // A paused Link never offers save/load, even in its keycap hints.
            if linked { &self.game_menu_hints[..1] } // B Back only
            else { &self.game_menu_hints }
        } else if heading == 151 && selected >= SAVES_SHARE_ROW {
            // Share Core Saves and Sync Saves with RomM are toggles.
            &self.settings_hints
        } else { &self.menu_hints };
        let gap = if !values && heading == 19 { 15.0 } else { LEGEND_GAP };
        for (tex, w, x) in centred_hints(hints, gap) {
            out.push(Draw::Tex {
                x, y: self.legend_y(), w: w as f32, h: HINT_H as f32,
                tex, alpha: if values { 1.0 } else { fade },
            });
        }
    }

    /// Resolve the inserted cart, including mixed shelves and regional NES moulds.
    /// Reuse texture IDs so switching themes or cartridges cannot grow the GL cache.
    fn prepare_link_art(&mut self) {
        let shelf = &self.shelves[self.active];
        let Some(cart) = shelf.carts.get(shelf.index) else { return };
        let key = Hardware::new(slot_ui::bitmaps::console_slot_family(cart),
            self.settings.appearance.bar_theme(), self.link_mode == LinkKind::Wireless);
        let previous = self.link_art.as_ref();
        if previous.is_some_and(|art| art.key == key) { return; }
        let faces = link_hardware::hardware_art(key);
        let mut upload = |face: slot_ui::CartFace, old: Option<Sprite>| {
            let tex = if let Some(sprite) = old {
                self.gpu.update_face(sprite.tex, &face);
                sprite.tex
            } else { self.gpu.create_face(&face) };
            Sprite { tex, w: face.w, h: face.h }
        };
        // Retain the four small release-button slots when switching away from RFU.
        let old_buttons = previous.and_then(|s| s.buttons);
        let buttons = faces.buttons.map(|pair| {
            let [host, join] = pair;
            [(0, host), (1, join)].map(|(i, [left, right])| {
                [upload(left, old_buttons.map(|b| b[i][0])),
                 upload(right, old_buttons.map(|b| b[i][1]))]
            })
        }).or(old_buttons);
        let [host,join] = faces.grips;
        self.link_art = Some(LinkSprites {
            key, buttons,
            grips: [upload(host, previous.map(|s|s.grips[0])),
                    upload(join, previous.map(|s|s.grips[1]))],
            wall: upload(faces.wall, previous.map(|s|s.wall)),
            front: upload(faces.front, previous.map(|s|s.front)),
            socket: upload(faces.socket, previous.map(|s|s.socket)),
            rim: upload(faces.rim, previous.map(|s|s.rim)),
            contact: upload(faces.contact, previous.map(|s|s.contact)),
            nose: upload(faces.nose, previous.map(|s|s.nose)),
        });
    }

    fn draw_multiplayer(&self, out: &mut Vec<Draw>) {
        out.push(Draw::Rect {
            x: 0.0, y: 0.0, w: out_w() as f32, h: OUT_H as f32,
            // Its cables and plugs are drawn for a dark ground, in every theme.
            colour: menu_style::dark_ground(),
        });
        let Some(art) = &self.link_art else { return };
        link_screen::draw_link_art_with(self.link_phase, self.link_millis(), art, &self.link_marks,
            self.link_inet.entry.is_none(), out);
        let ink = usize::from(art.key.dark_ink());
        // While the Internet tab has its own heading, never fall back to the
        // Local text: if its face is not rendered yet, draw no heading.
        let wanted = self.link_inet.heading(self.link_phase, self.link_millis());
        let internet_heading = wanted.as_ref()
            .filter(|text| **text == self.link_inet_face_text)
            .and(self.link_inet_face);
        let text = if wanted.is_some() { internet_heading } else { match self.link_phase {
            LinkMenu::Pick(role) => Some(self.link_role_faces[role.index()]),
            LinkMenu::Working { step, .. } => Some(self.link_step_faces[step.index()]),
            LinkMenu::Linked { .. } => Some(self.link_connected_face),
            LinkMenu::Failed { fail, .. } => fail.shown()
                .and_then(|i| self.link_fail_faces.get(i).copied()),
            LinkMenu::Unplug { .. } => None,
        } };
        if let Some((tex, w, h)) = text {
            out.push(Draw::Tex {
                x: link_heading_x(w),
                y: LINK_TEXT_Y, w: w as f32, h: h as f32,
                tex, alpha: 1.0,
            });
        }
        // LAN / WAN: the slide switch top right, level with the heading,
        // unless a long heading reaches it on a narrow screen.
        let switch_x = out_w() as f32 - 16.0 - slot_ui::NET_SWITCH_W as f32;
        if text.is_none_or(|(_, w, _)| link_heading_x(w) + w as f32 + 8.0 <= switch_x) {
            let [track, knob, house, globe] = self.net_switch_faces;
            let heading_h = self.link_role_faces[0].2 as f32;
            let y = (LINK_TEXT_Y + (heading_h - slot_ui::NET_SWITCH_H as f32) / 2.0).round();
            let t = slot_ui::ease(self.net_switch_t.clamp(0.0, 1.0));
            let [lan, wan] = slot_ui::NET_KNOB_X;
            let kx = (switch_x + lan + (wan - lan) * t).round();
            let ky = y + slot_ui::NET_KNOB_Y;
            let face = |(tex, w, h): (slot_gfx::TexId, u32, u32), x: f32, y: f32, alpha: f32|
                Draw::Tex { x, y, w: w as f32, h: h as f32, tex, alpha };
            out.push(face(track, switch_x, y, 1.0));
            out.push(face(knob, kx, ky, 1.0));
            let (ix, iy) = (kx + slot_ui::NET_ICON_X, ky + slot_ui::NET_ICON_Y);
            if t < 1.0 { out.push(face(house, ix, iy, 1.0 - t)); }
            if t > 0.0 { out.push(face(globe, ix, iy, t)); }
        }
        let stock: &[(slot_gfx::TexId, u32)] =
            match link_legend_profile(self.link_phase, self.link_mode_switchable()) {
                LinkLegendProfile::PickerWithMode => &self.link_switch_hints[ink],
                LinkLegendProfile::Picker => &self.link_hints[ink],
                LinkLegendProfile::Searching => &self.link_hints[ink][..1],
                LinkLegendProfile::ConnectedMenu => &self.link_connected_hints[ink],
                LinkLegendProfile::Failed => &self.link_failed_hints[ink],
                LinkLegendProfile::None => &[],
            };
        let inet = &self.link_inet_hints[ink];
        let mut hints: Vec<(slot_gfx::TexId, u32)> = stock.to_vec();
        if self.link_inet.entry.is_some() {
            // B Back | UP Digit | arrows Move | A Join
            hints = vec![self.link_connected_hints[ink][0], inet[3], inet[4], inet[5]];
        } else if matches!(self.link_phase, LinkMenu::Pick(_)) {
            // Right after B Cancel: the tab the shoulder button switches to.
            let tab = if self.link_inet.internet { inet[1] } else { inet[0] };
            hints.insert(1.min(hints.len()), tab);
        } else if self.link_inet.can_share(self.link_phase) {
            hints.push(inet[2]);
        }
        // Too wide for the screen (GBA's five keycaps on a 4:3 or narrower
        // screen): closer together, then the arrows without "Swap".
        let mut gap = link_legend_gap(&hints);
        if gap.is_none() {
            let swap = self.link_switch_hints[ink][2].0;
            for hint in hints.iter_mut().filter(|hint| hint.0 == swap) {
                *hint = self.link_swap_icon[ink];
            }
            gap = link_legend_gap(&hints);
        }
        let gap = gap.unwrap_or(LINK_LEGEND_MIN_GAP);
        let hints = hints.as_slice();
        let seen = |w: u32| w.saturating_sub(HINT_EDGE) as f32;
        let mut x = link_legend_start(hints, gap);
        for &(tex, w) in hints {
            out.push(Draw::Tex {
                x, y: LINK_LEGEND_Y,
                w: w as f32, h: HINT_H as f32,
                tex, alpha: 1.0,
            });
            x += (seen(w) + gap).round();
        }
    }

    /// Draw an optional version label on a common vertical centerline and
    /// return the true right edge for the following AVAILABLE glyph.
    fn draw_update_value(&self, face: Option<(slot_gfx::TexId,u32,u32)>,
                         x: f32, center_y: f32, alpha: f32, out: &mut Vec<Draw>) -> f32 {
        if let Some((tex,w,h)) = face {
            let scale = (185.0 / w.max(1) as f32).min(0.79);
            let width = w as f32 * scale;
            let height = h as f32 * scale;
            out.push(Draw::Tex {
                x, y: center_y - height / 2.0,
                w: width, h: height, tex, alpha,
            });
            return x + width;
        }
        x
    }

    /// About's DOWNLOADING line, with its percentage once known.
    fn set_download_text(&mut self, text: &str) {
        let Some(&(tex, _, _)) = self.menu_textures.get(114) else { return };
        let face = quick_value_face(text, true);
        self.gpu.update_face(tex, &face);
        self.menu_textures[114] = (tex, face.w, face.h);
    }

    fn draw_update_status_centered(&self, index: usize, x: f32, center_y: f32,
                                   max_w: f32, alpha: f32, out: &mut Vec<Draw>) {
        let (tex,w,h) = self.menu_textures[index];
        if w == 0 || h == 0 { return; }
        let scale = (max_w / w as f32).min(0.79);
        let height = h as f32 * scale;
        out.push(Draw::Tex {
            x, y: center_y - height / 2.0,
            w: w as f32 * scale, h: height, tex, alpha,
        });
    }

    fn draw_overlay(&self, out: &mut Vec<Draw>) {
        if self.draw_shelves_overlay(out) { return; }
        // Light and Original: the pages' text turns dark on their light grounds.
        let ink = self.menu_ink();
        if ink.is_some() { out.push(Draw::Ink(ink)); }
        let start = out.len();
        self.pill_band.set(None);
        self.draw_overlay_page(out);
        if ink.is_some() {
            self.theme_hints(start, out);
            out.push(Draw::Ink(None));
        }
        // The in-game Screen page stays white over the game; on Light's or
        // Original's bar its chosen row takes the theme's ink (Siham
        // 2026-11-16: white on the light bar couldn't be read).
        if self.screen_panel() {
            if let (Some(theme), Some([x0, x1, y, h])) = (menu_style::ink_map(), self.pill_band.get()) {
                let pill: Vec<slot_gfx::TexId> = self.menu_pill.iter().map(|p| p.0).collect();
                let tail = out.split_off(start);
                for draw in tail {
                    match draw {
                        Draw::Tex { x, y: ty, w, h: th, tex, .. }
                            if !pill.contains(&tex)
                                && (ty + th / 2.0) > y && (ty + th / 2.0) < y + h
                                && x + w / 2.0 > x0 && x + w / 2.0 < x1 => {
                            out.push(Draw::Ink(Some(theme)));
                            out.push(draw);
                            out.push(Draw::Ink(None));
                        }
                        _ => out.push(draw),
                    }
                }
            }
        }
    }

    fn draw_overlay_page(&self, out: &mut Vec<Draw>) {
        match self.overlay {
            ShelfOverlay::None | ShelfOverlay::Shelves { .. } => {}
            ShelfOverlay::Menu { row } => {
                let items = [1, 2, 3, 48, 25, 31];
                self.draw_original_style_menu(0, &items, row, out);
                if self.update_status == 1 {
                    // A butter-yellow capsule at the right of About's row,
                    // a little before its › so they never touch, in its own
                    // colours on every theme and wallpaper (Siham 2026-11-17).
                    let x1 = out_w() as f32 - self.menu_side_room(0, &items).unwrap_or(PILL_RIGHT);
                    let chevron = (x1 - 14.0 - self.menu_opens.1 as f32 * 0.79).round();
                    let label_end = (37.0 + self.menu_textures[31].1 as f32 * 0.79).round() + 16.0;
                    let (tex, w, h) = self.update_badge;
                    let right = chevron - 12.0;
                    let scale = ((right - label_end) / w.max(1) as f32).min(1.0);
                    if w > 0 && scale >= 0.6 {
                        let (w, h) = (w as f32 * scale, h as f32 * scale);
                        let centre = settings_row_top(6) + 5.5 * QUICK_PITCH;
                        let ink = self.menu_ink();
                        if ink.is_some() { out.push(Draw::Ink(None)); }
                        out.push(Draw::Tex {
                            x: (right - w).round(), y: (centre - h / 2.0).round(), w, h, tex,
                            alpha: self.menu_fade(0),
                        });
                        if ink.is_some() { out.push(Draw::Ink(ink)); }
                    }
                }
            }
            ShelfOverlay::Library { row } => {
                let values = self.library_value_faces();
                let scales = self.value_row_scales(&LIBRARY_ROWS, &values);
                self.draw_menu_rows(5, &LIBRARY_ROWS, row, &scales, out);
                self.draw_page_values(5, &LIBRARY_ROWS, row, &values, &scales, out);
                self.draw_menu_chrome(5, LIBRARY_ROWS.len(), row, false, out);
            }
            ShelfOverlay::SavesStates { row } => {
                let values = self.saves_value_faces();
                let scales = self.value_row_scales(&SAVES_ROWS, &values);
                self.draw_menu_rows(151, &SAVES_ROWS, row, &scales, out);
                self.draw_page_values(151, &SAVES_ROWS, row, &values, &scales, out);
                self.draw_menu_chrome(151, SAVES_ROWS.len(), row, false, out);
            }
            ShelfOverlay::Settings { row } => {
                self.draw_original_style_menu(26, settings_rows(), row, out);
            }
            ShelfOverlay::GeneralSettings { row } => {
                self.draw_settings_menu(82, &INTERFACE_PAGE_ROWS, row, &INTERFACE_PAGE_KEYS, out);
            }
            ShelfOverlay::WallpaperSettings { row } => {
                let (rows, keys) = self.wallpaper_rows();
                self.draw_settings_menu(148, &rows, row.min(rows.len() - 1), &keys, out);
            }
            ShelfOverlay::InterfaceSettings { row } => {
                let (rows, keys) = interface_rows();
                self.draw_settings_menu(59, rows, row, keys, out);
            }
            ShelfOverlay::RotateSettings { row } => {
                self.draw_settings_menu(147, &ROTATE_ROWS, row, &ROTATE_KEYS, out);
            }
            ShelfOverlay::EmulationSettings { row } => {
                self.draw_settings_menu(50, &[51, 52, 55], row, &[10, 11, 12], out);
            }
            ShelfOverlay::PauseEmulationSettings { row } => {
                // One value per visible row, from the same list as the rows.
                let (rows, keys) = self.pause_emulation();
                self.draw_settings_menu(50, &rows, row, &keys, out);
                // Identify the selected core without changing Slot's menu
                // layout or existing keycap typography.
                let (tex, w, h) = self.emulation_core_labels[self.selected_emulation_core().index()];
                // Match text_fit's 0.79x header scale and exactly the heading
                // baseline at y=33; right align to the same 32px inset.
                // Core names are short enough to never overlap the left title.
                let scale = 0.79f32;
                let fade = slot_ui::ease(
                    (self.menu_opened.elapsed().as_secs_f32() / 0.16).clamp(0.0, 1.0));
                out.push(Draw::Tex {
                    x: out_w() as f32 - 32.0 - w as f32 * scale, y: menu_look::TITLE_Y,
                    w: w as f32 * scale, h: h as f32 * scale,
                    tex, alpha: menu_look::ROW_REST * fade,
                });
            }
            ShelfOverlay::ScreenSettings { row } => {
                self.draw_original_style_menu(117, &SYSTEM_TITLES, row, out);
            }
            ShelfOverlay::ScreenSystemSettings {platform,row} => {
                let rows=Self::system_page_rows(platform);
                let keys=Self::system_page_keys(platform);
                self.draw_settings_menu(Self::screen_title(platform),&rows,row,&keys,out);
            }
            ShelfOverlay::GameplaySettings { row } => {
                self.draw_settings_menu(33, &GAMEPLAY_ROWS, row, &GAMEPLAY_KEYS, out);
            }
            ShelfOverlay::About { row } => {
                // The native KONKR sticker stays intact, shifted up to make room
                // for two transparent menu rows and a non-focusable status line.
                out.push(Draw::Rect {
                    x: 0.0, y: 0.0, w: out_w() as f32, h: OUT_H as f32,
                    colour: menu_ground(),
                });
                // Narrower than the 720 of the KONKR's 3:2 (4:3 640, Retroid
                // Pocket Classic 552, 1:1 480): the sticker shrinks to the
                // width and the rows keep their right margin. It once went
                // off-screen there: (out_w - STICKER_W) wrapped as a u32.
                let width = out_w() as f32;
                let narrower = (width - 720.0).min(0.0);
                if let Some(tex) = self.about_texture {
                    let (x, y, w, h) = about_sticker_box();
                    // A soft shadow under it and a light edge round it: the
                    // dark sticker stands out on every theme's ground
                    // (Siham 2026-11-15).
                    // Both follow its shape, socket notch included: the
                    // sticker itself, inked flat black, then flat white.
                    let flat = |c: f32| Draw::Ink(Some(slot_gfx::InkMap { paper: [c; 3], ink: [c; 3] }));
                    let grown = |g: f32, dy: f32, alpha: f32| Draw::Tex {
                        x: x - g, y: y - g + dy, w: w + 2.0 * g, h: h + 2.0 * g, tex, alpha };
                    out.push(flat(0.0));
                    for (grow, alpha) in [(6.0f32, 0.07f32), (4.0, 0.10), (2.0, 0.16)] {
                        out.push(grown(grow, 4.0, alpha));
                    }
                    out.push(flat(1.0));
                    out.push(grown(1.5, 0.0, 0.26));
                    out.push(Draw::Ink(None));
                    // The device's sticker, as printed, in every theme.
                    out.push(Draw::Tex { x, y, w, h, tex, alpha: 1.0 });
                    out.push(Draw::Ink(self.menu_ink()));
                }
                let fade = slot_ui::ease(
                    (self.menu_opened.elapsed().as_secs_f32() / 0.16).clamp(0.0, 1.0));
                let top = about_row_top();
                self.draw_menu_cursor(top, row, 2, out_w() as f32 - PILL_RIGHT, fade, out);
                let lit = |chosen: bool| if chosen { fade } else { menu_look::ROW_REST * fade };
                self.text_fit(if about_primary_action(self.update_status) == 21 {110} else {95},
                    37.0, top + 8.0, 590.0 + narrower, lit(row == 0), out);
                // Automatic Checks has all the room up to its On / Off: a
                // fixed room shrank it on narrow screens (Siham 2026-11-15).
                let auto_x = 592.0 + narrower;
                self.text_fit(98, 37.0, top + QUICK_PITCH + 8.0, auto_x - 12.0 - 37.0, lit(row == 1), out);
                self.text_fit(if self.update_auto {105}else{104},
                    auto_x, top + QUICK_PITCH + 8.0, 95.0, lit(row == 1), out);
                // Equal first-pixel baselines: the third status line begins
                // exactly one QUICK_PITCH after the second line, rather than
                // being vertically centered using a different offset.
                // Its optional version is still centered against AVAILABLE.
                let status_top = top + 2.0 * QUICK_PITCH + 8.0;
                if matches!(self.update_status, 1 | 5) {
                    let (label_w, label_h) = {
                        let (_, w, h) = self.menu_textures[115];
                        (w.max(1) as f32, h as f32)
                    };
                    let status_center = status_top +
                        label_h * (0.79f32.min((420.0 + narrower) / label_w)) / 2.0;
                    let right = self.draw_update_value(self.update_version_face,
                        37.0, status_center, 0.90 * fade, out);
                    self.draw_update_status_centered(115, right + 17.0,
                        status_center, 600.0 + narrower - right, 0.78 * fade, out);
                } else {
                    let status = match self.update_status {
                        2 => 112, 3 => 114, 4 => 113, _ => 111,
                    };
                    let (_, w, h) = self.menu_textures[status];
                    let status_center = status_top +
                        h as f32 * (0.79f32.min((610.0 + narrower) / w.max(1) as f32)) / 2.0;
                    self.draw_update_status_centered(status, 37.0,
                        status_center, 610.0 + narrower, 0.78 * fade, out);
                }
                let hints = [self.menu_hints[0], self.menu_hints[1], self.about_logs_hint];
                for (tex,w,x) in centred_hints(&hints, LEGEND_GAP) {
                    out.push(Draw::Tex {
                        x, y: 427.0, w: w as f32, h: HINT_H as f32,
                        tex, alpha: fade,
                    });
                }
            }
            // Waiting out a second slot. tap: the paused game, no menu yet.
            ShelfOverlay::GameMenu { .. } if self.menu_reveal_at.is_some_and(|at| Instant::now() < at) => {}
            ShelfOverlay::GameMenu { row } => {
                let relaunch = self.game.as_ref().is_some_and(GameSession::requires_relaunch);
                let indices: Vec<usize> = self.game_menu_rows().iter()
                    .map(|item| pause_row_text(*item, relaunch)).collect();
                self.draw_original_style_menu(19, &indices, row, out);
            }
            ShelfOverlay::PauseSettings { row } => {
                self.draw_original_style_menu(26, self.pause_settings_rows(), row, out);
            }
            ShelfOverlay::PauseScreenSettings { row } => {
                let items=self.pause_screen_page_rows();
                let keys: Vec<usize>=(0..items.len())
                    .filter_map(|index|self.pause_screen_page_key(index)).collect();
                self.draw_settings_menu(32, &items, row, &keys, out);
            }
            ShelfOverlay::PauseGameplaySettings { row } => {
                let (rows, keys) = self.pause_gameplay();
                self.draw_settings_menu(33, &rows, row, &keys, out);
            }
            ShelfOverlay::Multiplayer => self.draw_multiplayer(out),
            ShelfOverlay::CartStudio => self.draw_cart_studio(out),
            ShelfOverlay::CartLibrary => self.draw_cart_library(out),
            ShelfOverlay::Romm => self.draw_romm(out),
            ShelfOverlay::ControlsSettings { .. } | ShelfOverlay::ControlsPort { .. }
            | ShelfOverlay::LayoutPorts { .. } | ShelfOverlay::LayoutPort { .. }
            | ShelfOverlay::Hotkeys { .. } | ShelfOverlay::Cheats { .. } =>
                self.draw_controls(out),
            ShelfOverlay::Core => {
                self.draw_original_core_picker(out);
            }
            ShelfOverlay::States => {
                // Rendered separately as full-screen original Slot Polaroids.
            }
        }
    }

    /// Slot's original board/lid/socket/chip geometry and timelines.
    /// The picker state machine is the unmodified upstream core_picker.rs.
    fn draw_original_core_picker(&self, out: &mut Vec<Draw>) {
        let Some(picker) = self.picker else { return };
        let now = self.born.elapsed().as_millis() as u64;
        let progress = picker.openness(now);
        let lift = lift_of(progress);
        let shelf = &self.shelves[self.active];
        let (rest, scale) = shelf.selected_at();
        let platform = shelf.carts.get(shelf.index).map_or(Platform::Gba, |c| c.platform);
        let shape = shelf.carts.get(shelf.index).map_or(slot_ui::Body::Gba, slot_ui::body_of);
        let cart = slot_ui::shelf_cart_body(shape, rest, scale);
        let layout = slot_ui::board_layout_body(shape);
        let board = board_toward(cart, layout.rest, progress);
        let zoom = layout.zoom(board);

        if let Some(tex) = self.board_texture {
            out.push(Draw::Tex {
                x: board.x, y: board.y, w: board.w, h: board.h,
                tex, alpha: 1.0,
            });
        }
        // Where the sockets sit: the two of the board, or a Pokemon game's
        // three (PokeDaisy's where the title chip was).
        let sockets: Vec<(Core, f32, f32)> = match self.picker_three {
            Some(three) => three.socket.map_or_else(Vec::new, |at| three.cores.into_iter()
                .zip(at).map(|(core, (u, v))| (core, u, v)).collect()),
            None if layout.sockets => [picker.left(), picker.right()].into_iter().enumerate()
                .map(|(i, core)| (core, layout.socket_u[i], layout.socket_v)).collect(),
            None => Vec::new(),
        };
        for (core, su, sv) in sockets {
            let tex = self.socket_textures[core.index()];
            let (x, y) = layout.at(board, su, sv);
            out.push(Draw::Tex {
                x: x.round(), y: y.round(),
                w: SOCKET_W as f32 * zoom, h: SOCKET_H as f32 * zoom,
                tex, alpha: 1.0,
            });
        }

        let chip = picker.chip(now);
        let (u, chip_v, shadow_at, chip_size) = match self.picker_three {
            Some(three) => {
                let ((u0, v0), (u1, v1)) = (three.chip[chip.from.min(2)], three.chip[chip.to.min(2)]);
                (u0 + (u1 - u0) * chip.along, v0 + (v1 - v0) * chip.along, three.shadow_at, three.size)
            }
            None => (layout.chip_u[0] + (layout.chip_u[1] - layout.chip_u[0]) * chip.across,
                layout.chip_v, layout.shadow_at, layout.chip),
        };
        if chip.lift > 0.0 {
            let (cx, cy) = layout.at(board,
                u + shadow_at.0, chip_v + shadow_at.1);
            let (w, h) = (layout.shadow.0 * zoom, layout.shadow.1 * zoom);
            out.push(Draw::Tex {
                x: cx - w / 2.0, y: cy - h / 2.0, w, h,
                tex: self.chip_shadow, alpha: 0.6 * chip.lift * lift,
            });
        }
        let (chips, blank) = if layout.sockets {
            (&self.chip_textures, self.chip_blank)
        } else {
            let blank = match platform {
                Platform::Nes => self.home_chip_blanks[0],
                Platform::Snes => self.home_chip_blanks[1],
                Platform::Md => self.home_chip_blanks[2],
                Platform::Sms => self.home_chip_blanks[3],
                Platform::Gg => self.home_chip_blanks[4],
                Platform::Ngp => self.home_chip_blanks[5],
                Platform::Ws => self.home_chip_blanks[6],
                Platform::Lynx => self.home_chip_blanks[7],
                Platform::Pokemini => self.home_chip_blanks[8],
                Platform::Pico8 => self.home_chip_blanks[9],
                _ => self.pak_chip_blank,
            };
            (&self.pak_chip_textures, blank)
        };
        let mut tex = chip.seated.map(|core| chips[core.index()]).unwrap_or(blank);
        // The three-chip pak board's shorter chips.
        if let (Some(three), Some(short)) = (self.picker_three.filter(|t| t.socket.is_none()), self.pak_three_textures) {
            tex = chip.seated.and_then(|core| three.cores.iter().position(|c| *c == core))
                .map_or(short[3], |at| short[at]);
        }
        let (x, y) = layout.at(board, u, chip_v - layout.hop * chip.lift);
        let body = grown(slot_ui::Placed {
            x: x + chip.shake, y,
            w: chip_size.0 * zoom, h: chip_size.1 * zoom,
        }, TURN_PAD as f32 * zoom);
        out.push(Draw::Turned {
            x: body.x.round(), y: body.y.round(),
            w: body.w, h: body.h, tex, alpha: 1.0, turn: chip.tip,
        });

        // The original lid slides up first (160ms), lifts and tilts (260ms)
        // and returns smoothly from any partial openness on B/A (320ms).
        let (lid, turn) = lid_toward(cart, slot_ui::lid_rest_body(shape), progress);
        let k = lid.w / slot_ui::lid_at(1.0).0.w;
        let (w, h) = (168.0 * k, 18.0 * k);
        out.push(Draw::Tex {
            x: lid.x + (lid.w - w) / 2.0,
            y: lid.y + lid.h + 29.0 * k - h / 2.0,
            w, h,
            tex: self.chip_shadow,
            alpha: 0.8 * lift,
        });
        let (mut behind,front)=self.clear_layers();
        // The board has left the cart: its lifted front shows only its own
        // plastic and label, not the board a clear shell shows on the shelf.
        if front.is_some() { behind = [None, None]; }
        let face=grown(lid,TURN_PAD as f32*lid.w/slot_ui::body_box(shape).0 as f32);
        for tex in behind.into_iter().flatten()
            .chain(front
                .or_else(|| self.picker_lid.0.filter(|_| self.picker_lid.1))
                .or_else(|| shelf.face(shelf.index))) {
            out.push(Draw::Turned {
                x:face.x,y:face.y,w:face.w,h:face.h,
                tex,alpha:1.0,turn,
            });
        }

        // Restore the original Slot keycap legends. The original positions
        // anchor Cancel to the board's left edge, Swap in the centre and
        // Choose to the board's right edge. Shrink uniformly only if needed
        // to preserve >=12px spacing on small screens / font changes.
        let legends = if self.shelf_ink().is_some() { self.core_legend_dark } else { self.core_legend_faces };
        let widths = legends.map(|(_, w)| w.saturating_sub(HINT_EDGE));
        let positions = crate::core_legend::positions(widths);
        // A chip that can't move (a Neo Geo file's core, Splore): no Swap.
        let fixed = self.picker.is_some_and(|picker| picker.fixed());
        for (at, ((tex, w), (x, scale))) in legends.into_iter().zip(positions).enumerate() {
            if fixed && at == 1 { continue; }
            let height = HINT_H as f32 * scale;
            out.push(Draw::Tex {
                x: x.round(),
                y: 386.0 + (HINT_H as f32 - height) / 2.0,
                w: w as f32 * scale,
                h: height,
                tex,
                alpha: lift,
            });
        }
    }

    /// Android removes the Polaroid's RetroArch copy too.
    fn forget_polaroid(&self, core: &str, stamp: &str) {
        if let Some(uri) = self.requested_uri.as_deref() {
            let event = serde_json::json!({
                "uri": uri, "core": core, "deleted_stamp": stamp
            }).to_string();
            SAVE_SYNC.lock().unwrap_or_else(|e| e.into_inner()).push_back(event);
        }
    }

    fn delete_selected_polaroid(&mut self) {
        let mut deleted=None;
        if let (Some(game),Some(p))=(self.game.as_mut(),self.polaroids.as_mut()) {
            if let Some(entry)=p.selected() {
                if let Err(err)=game.delete_manual(&entry.stamp) {
                    set_error_message("Couldn't delete this save state.", &err);
                    p.keep();
                } else {
                    deleted=Some((game.core_kind().as_str(), entry.stamp.clone()));
                    p.remove_selected();
                }
            }
        }
        if let Some((core,stamp))=deleted { self.forget_polaroid(core,&stamp); }
        if self.polaroids.as_ref().is_some_and(|p|p.is_empty()) {
            self.overlay=ShelfOverlay::None;
        } else {
            self.update_polaroid_title();
        }
    }

    fn update_polaroid_title(&mut self) {
        let Some(p)=self.polaroids.as_mut() else {return;};
        let title=if p.confirming() { slot_ui::CONFIRM_TITLE.to_owned() }
            else { p.title(&stamp_now()) };
        let face=title_face(&title);
        let tex=match self.polaroid_title_texture {
            Some(tex)=>{
                self.gpu.update_face(tex, &face);
                tex
            }
            None=>{
                let tex=self.gpu.create_face(&face);
                self.polaroid_title_texture=Some(tex);
                tex
            }
        };
        p.set_title_face(Some(tex));
    }

    fn open_polaroids(&mut self) {
        let Some(game)=self.game.as_ref() else {return;};
        let entries=game.history();
        if entries.is_empty() {
            set_message("No save states yet. Press SELECT + R1 to save.".into());
            return;
        }
        let mut polaroids=Polaroids::new(entries);
        let mut faces=Vec::new();
        for (index,entry) in polaroids.entries.iter().enumerate() {
            let face=photo_face(entry);
            let tex=if let Some(tex)=self.photo_textures.get(index).copied() {
                self.gpu.update_face(tex, &face);
                tex
            } else {
                let tex=self.gpu.create_face(&face);
                self.photo_textures.push(tex);
                tex
            };
            faces.push(tex);
        }
        polaroids.set_faces(faces);
        let hints=[("B","Back"),("Y","Delete"),("A","Load"),("X","Undo")];
        let hint_faces=hints.map(|(key,label)| {
            let face=hint_face(crate::controls::menu_keycap(key, self.controls.menu_swap_ab),label);
            self.gpu.create_face(&face)
        });
        polaroids.set_hint_faces(hint_faces.to_vec());
        let confirm_faces=slot_ui::CONFIRM_LEGEND.map(|(key,label)| {
            let face=hint_face(crate::controls::menu_keycap(key, self.controls.menu_swap_ab),label);
            self.gpu.create_face(&face)
        });
        polaroids.set_confirm_faces(confirm_faces.to_vec());
        if game.undo_available() {polaroids.set_undo(Some("undo"));}
        self.polaroids=Some(polaroids);
        self.update_polaroid_title();
        self.overlay=ShelfOverlay::States;
    }

    // 400ms hold then the original accelerated 110/85/65/50ms repeat
    // in Slot-owned menus. Android key repeats are filtered at MainActivity.
    fn tick_menu_repeat(&mut self) {
        let Some((key,due,count))=self.menu_held else {return};
        if matches!(self.overlay,ShelfOverlay::None) {
            self.menu_held=None;
            return;
        }
        let now=self.born.elapsed().as_millis() as u64;
        if now<due {return;}
        const GAPS:[u64;4]=[110,85,65,50];
        self.menu_held=Some((key,now+GAPS[count.min(3)],count+1));
        self.menu_repeat_dispatch=true;
        self.handle(Input::Key(key,true));
        self.menu_repeat_dispatch=false;
    }
    fn draw(&mut self, vsync_frame_ns: i64, mut on_pcm_ready: impl FnMut()) {
        if !self.gpu.graphics_ready() { return; }
        self.update_menu_palette();
        // Includes Slot-side input/menu/RA/status preparation in "other_us".
        // The enclosing JNI caller's library gate and later EGL swap remain
        // outside the scope; Kotlin times the broader nativeDrawFrame call.
        let phase_native_started = Instant::now();
        LINK_ACTIVE.store(self.game.as_ref().is_some_and(|game| game.linked()), Ordering::Release);
        self.tick_menu_repeat();
        // A game on screen, no menu or polaroid over it: Android keeps the
        // screen on (MainActivity.keepScreenOn).
        let unpaused = self.game.is_some() && matches!(self.overlay, ShelfOverlay::None)
            && self.polaroids.is_none() && !self.exiting_screen && !self.lid_covered;
        GAME_UNPAUSED.store(unpaused, Ordering::Release);
        if !unpaused || self.rewind_held { clear_rumble(); }
        let (dim, sleep) = self.settings.rotate_delays();
        ROTATE_DELAYS.store(u32::from(dim) << 16 | u32::from(sleep)
            | u32::from(self.settings.rotate_shelf_cover == 3) << 30
            | 1 << 31, Ordering::Release);
        self.refresh_system_status();
        self.refresh_slot_copy_face();
        let missing_mix = self.advance_missing();
        let wallpaper_at = Instant::now();
        // One wallpaper upload a frame.
        if !self.poll_wallpaper() { self.poll_shelf_wallpapers(); }
        crate::frame_watch::part(crate::frame_watch::PART_WALLPAPER, wallpaper_at.elapsed());
        let before_at = Instant::now();
        // Poll the already-running achievement evaluator without blocking GL.
        // Only Android's foreground UI displays notices; the worker never
        // touches JNI, an Activity, the renderer or the filesystem here.
        if let Some(service) = self.ra_service.as_ref() {
            let status = service.sync_status();
            let detail = service.sync_detail();
            let progress = service.sync_progress();
            let pending = service.sync_pending();
            let unsaved = service.has_unsaved_awards();
            let key = u64::from(status as u8)
                | (u64::from(progress.map_or(256u16, u16::from)) << 8)
                | ((pending as u64) << 24)
                | ((unsaved as u64) << 25)
                | (u64::from(detail as u8) << 26);
            if RA_STATUS_KEY.swap(key, Ordering::AcqRel) != key {
                let snapshot = serde_json::json!({
                    "state": format!("{status:?}"),
                    "detail": format!("{detail:?}"),
                    "pending": pending,
                    "unsaved": unsaved,
                    "progress": progress,
                }).to_string();
                *RA_STATUS.lock().unwrap_or_else(|e| e.into_inner()) = Some(snapshot);
            }
            // Bound per-frame overhead and pending Android UI messages.
            for _ in 0..4 {
                let Some(notice) = service.take_notice() else { break };
                let payload = ra_notice_payload(
                    &notice.title, &notice.detail,
                    notice.kind == slot_achievements::NoticeKind::Earned,
                    notice.badge.as_deref(),
                );
                let mut queue = RA_NOTICES.lock().unwrap_or_else(|e| e.into_inner());
                queue_ra_notice(&mut queue, payload);
            }
        }
        let now = Instant::now();
        // After an idle gap (the settled shelf draws at 4 Hz) the first frame
        // advances one display frame, not a 50 ms jump: a scroll that wakes
        // the carousel then starts as smoothly as one that continues.
        let elapsed = (now - self.last).as_secs_f32();
        let mut dt = if elapsed > 0.1 && self.game.is_none() { 1.0 / 60.0 }
            else { elapsed.min(0.05) };
        self.last = now;
        // Game Info's time: one add a frame, while the game really runs. A
        // gap (slot. in the background) counts at most a quarter second.
        if GAME_UNPAUSED.load(Ordering::Relaxed) && !self.lid_covered && self.lid_t == 0.0 {
            self.play_time += elapsed.min(0.25);
        }
        let mut cover_with_hold = false;
        {
            let goal = if self.lid_covered { 1.0 } else { 0.0 };
            // From rest (the shelf may have been drawing at 4 Hz) the first
            // step is one display frame; after that real time, so a slow
            // frame does not stretch the animation.
            let at_rest = self.lid_t == 0.0 || self.lid_t == 1.0;
            // The lid's first animated frame, timed on Android's uptime, to
            // set against the lid's key and the display's turn (beta43).
            if at_rest && self.lid_t != goal {
                crate::game::log_launch_timing(&format!("Rotate: [{}] lid animation starts ({})",
                    uptime_ms(), if self.lid_covered { "closing" } else { "opening" }));
            }
            let lid_dt = if at_rest { 1.0 / 60.0 } else { elapsed.min(0.1) };
            let step = lid_dt / LID_ANIMATION_S;
            self.lid_t = if goal > self.lid_t { (self.lid_t + step).min(goal) } else { (self.lid_t - step).max(goal) };
        }

        // Measure real libretro simulation FPS independently of the optional
        // performance HUD. No display toggles or simulation timing changes.
        // The 1Hz snapshot is read on Android's existing 100ms UI heartbeat.
        if self.game.is_some() {
            let show_fps = self.current_emulation().show_fps;
            if show_fps {
                PERF_ACTIVE.store(true, Ordering::Release);
            } else if PERF_ACTIVE.swap(false, Ordering::AcqRel) {
                *PERF_STATUS.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
            self.perf_rendered += 1;
            let span = self.perf_last.elapsed().as_secs_f64();
            if span >= 1.0 {
                let emu = self.perf_emulated as f64 / span;
                let render = self.perf_rendered as f64 / span;
                let catchup = 100.0 * self.perf_catchup as f64 /
                    self.perf_rendered.max(1) as f64;
                let (target, core, linked) = self.game.as_ref().map_or(
                    (60.0, "none", false),
                    |g| (g.fps, g.core_kind().as_str(), g.linked()));
                let profile = self.current_emulation();
                let requested = profile.run_ahead_frames;
                let probe = self.game.as_mut()
                    .map(|g| g.take_runahead_probe()).unwrap_or_default();
                *EMULATION_DIAGNOSTICS.lock().unwrap_or_else(|e|e.into_inner()) =
                    Some(format!(
                        "core={core} emu_fps={emu:.3} target_fps={target:.3} display_callbacks_fps={render:.3} catchup_percent={catchup:.1} pacing_mode=retroarch frame_debt_ms={:.3} emu_step0_1s={} emu_step1_1s={} emu_step_multi_1s={} runahead_requested={} runahead_calls={} runahead_snapshot_us={} runahead_speculative_us={} runahead_restore_us={} runahead_max_us={} runahead_failures={} runahead_second_instance_calls={} runahead_preemptive_rollbacks={} ra_sync_rate_adjust={:.5} ra_display_hz={:.3} frame_delay_ms={:.1} ra_loop={} linked={linked}",
                        self.game_accum * 1000.0,
                        self.perf_step0, self.perf_step1, self.perf_step_multi,
                        requested, probe.calls, probe.snapshots_us,
                        probe.speculative_us, probe.restores_us, probe.max_us,
                        probe.failures, probe.second_instance_calls,
                        probe.preemptive_rollbacks, self.ra_adjust,
                        self.ra_sync_steps.display_hz(
                            f64::from(DISPLAY_REFRESH_MILLIHZ.load(Ordering::Acquire)) / 1000.0),
                        self.frame_delay.delay_ns() as f64 / 1e6,
                        if RA_SWAP_LOOP.load(Ordering::Acquire) { "swap" } else { "vsync" }));
                if show_fps {
                    *PERF_STATUS.lock().unwrap_or_else(|e|e.into_inner()) =
                        Some(format!("EMU {emu:.1}/{target:.1} | DISPLAY {render:.1} | CATCHUP {catchup:.0}%"));
                }
                self.perf_last = now;
                self.perf_rendered = 0;
                self.perf_emulated = 0;
                self.perf_catchup = 0;
                self.perf_step0 = 0;
                self.perf_step1 = 0;
                self.perf_step_multi = 0;
            }
        } else {
            self.perf_last = now;
            self.perf_rendered = 0;
            self.perf_emulated = 0;
            self.perf_catchup = 0;
            self.perf_step0 = 0;
            self.perf_step1 = 0;
            self.perf_step_multi = 0;
            *EMULATION_DIAGNOSTICS.lock().unwrap_or_else(|e|e.into_inner()) = None;
            if PERF_ACTIVE.swap(false, Ordering::AcqRel) {
                *PERF_STATUS.lock().unwrap_or_else(|e|e.into_inner()) = None;
            }
        }

        // RetroArch Frame Delay: wait after VSync before reading the buttons,
        // so the core runs on fresher input for the same refresh.
        self.frame_delay_used_ns = self.frame_delay_wanted(vsync_frame_ns);
        if let Some(delay) = self.frame_delay_used_ns {
            let wait = (vsync_frame_ns + delay - crate::ra_sync::monotonic_ns()).min(delay);
            if wait > 0 { std::thread::sleep(Duration::from_nanos(wait as u64)); }
        }
        let events: Vec<Input> = INPUT
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .drain(..)
            .collect();
        // Before onPause's Reset among them (jni_exports::EXTERNAL_BEGIN).
        if jni_exports::EXTERNAL_BEGIN.swap(false, Ordering::AcqRel) { self.external_pending = true; }
        for event in events {
            self.handle(event);
        }
        self.publish_menu_keys();
        self.poll_multiplayer();
        if self.picker.is_some_and(|p| p.finished(self.born.elapsed().as_millis() as u64)) {
            self.picker = None;
            self.overlay = ShelfOverlay::None;
        }
        self.settle_studio_flight();
        self.touch_tick();
        self.shelves_glide();
        if let Some(at) = self.picker_auto_seat {
            let now = self.born.elapsed().as_millis() as u64;
            if !matches!(self.overlay, ShelfOverlay::Core) || self.picker.is_none_or(|p| p.closing()) {
                self.picker_auto_seat = None;
            } else if now >= at {
                self.picker_auto_seat = None;
                self.seat_core_picker();
            }
        }
        if self.menu_reveal_at.is_some_and(|at| Instant::now() >= at) {
            self.menu_reveal_at = None;
            self.menu_seen = 0; // the menu's own fade-in, now
        }
        if !matches!(self.overlay, ShelfOverlay::GameMenu { .. }) { self.menu_reveal_at = None; }
        // slot. original: the slot rises with the in-game menu and slides
        // back down as the game resumes, over a power-on's 220 ms.
        if self.game.is_none() || matches!(self.overlay, ShelfOverlay::States | ShelfOverlay::Multiplayer) {
            self.footer = 0.0;
            self.screen_closing = false;
            self.screen_direct = false;
        } else {
            let step = dt / SCREEN_POWER_ON_S;
            self.footer = if self.screen_panel() && !self.screen_closing { (self.footer + step).min(1.0) }
                else { (self.footer - step).max(0.0) };
            // Leaving the Screen page: once the slot is down, Settings (or
            // the game, when its hotkey opened it).
            if self.screen_closing && self.footer <= 0.0 {
                self.screen_closing = false;
                if self.screen_panel() {
                    self.overlay = if self.screen_direct { ShelfOverlay::None }
                        else { ShelfOverlay::PauseSettings { row: 0 } };
                }
            }
            if !self.screen_panel() { self.screen_direct = false; self.screen_hotkey_held = false; }
        }
        // Multiplayer's LAN / WAN knob slides over 220 ms, as the slot.
        {
            let wan = if self.link_inet.internet || self.link_inet.entry.is_some() { 1.0 } else { 0.0 };
            if matches!(self.overlay, ShelfOverlay::Multiplayer) {
                let step = dt / SCREEN_POWER_ON_S;
                self.net_switch_t = if wan > self.net_switch_t { (self.net_switch_t + step).min(wan) }
                    else { (self.net_switch_t - step).max(wan) };
            } else {
                self.net_switch_t = wan;
            }
        }
        let menus_at = Instant::now();
        {
            use crate::frame_watch::{span, SPAN_MENU_CONTROLS, SPAN_MENU_LIBRARY, SPAN_MENU_LOOK, SPAN_MENU_ROMM, SPAN_MENU_STUDIO};
            let at = Instant::now();
            self.prepare_controls();
            span(SPAN_MENU_CONTROLS, at.elapsed());
            let at = Instant::now();
            self.prepare_menu_look();
            span(SPAN_MENU_LOOK, at.elapsed());
            let at = Instant::now();
            self.prepare_cart_studio();
            span(SPAN_MENU_STUDIO, at.elapsed());
            let at = Instant::now();
            self.prepare_cart_library();
            span(SPAN_MENU_LIBRARY, at.elapsed());
            let at = Instant::now();
            self.prepare_romm();
            span(SPAN_MENU_ROMM, at.elapsed());
        }
        crate::frame_watch::span(crate::frame_watch::SPAN_MENUS, menus_at.elapsed());
        // Update full-screen menus and spring-smooth the selection bar.
        // Keep animation entirely in rendering: never delay controller input.
        let (kind, selected, count) = match self.overlay {
            ShelfOverlay::Menu { row } => (1u8, row, 6usize),
            ShelfOverlay::Library { row } => (2u8, row, LIBRARY_ROWS.len()),
            ShelfOverlay::SavesStates { row } => (33u8, row, SAVES_ROWS.len()),
            ShelfOverlay::GameMenu { row } => (3u8, row, self.game_menu_rows().len()),
            ShelfOverlay::PauseSettings { row } => (13u8, row, self.pause_settings_rows().len()),
            ShelfOverlay::PauseScreenSettings { row } =>
                (14u8, row, self.pause_screen_page_rows().len()),
            ShelfOverlay::PauseGameplaySettings { row } => (15u8, row, self.pause_gameplay().1.len()),
            ShelfOverlay::Multiplayer => (11u8, 0, 0usize),
            ShelfOverlay::Settings { row } => (4u8, row, settings_rows().len()),
            ShelfOverlay::ControlsSettings { .. } | ShelfOverlay::ControlsPort { .. }
            | ShelfOverlay::LayoutPorts { .. } | ShelfOverlay::LayoutPort { .. }
            | ShelfOverlay::Hotkeys { .. } | ShelfOverlay::Cheats { .. } =>
                self.controls_cursor().unwrap_or((0, 0, 0)),
            ShelfOverlay::GeneralSettings { row } =>
                (12u8, row, self.general_rows().len()),
            ShelfOverlay::About { row } => (17u8, row, 2usize),
            ShelfOverlay::InterfaceSettings { row } => (10u8, row, interface_rows().0.len()),
            ShelfOverlay::RotateSettings { row } => (18u8, row, ROTATE_ROWS.len()),
            ShelfOverlay::WallpaperSettings { row } => (19u8, row, self.wallpaper_rows().0.len()),
            ShelfOverlay::EmulationSettings { row } => (8u8, row, 3usize),
            ShelfOverlay::PauseEmulationSettings { row } =>
                (9u8, row, self.pause_emulation_rows().len()),
            ShelfOverlay::ScreenSettings { row } => (5u8, row, SYSTEM_ORDER.len()),
            ShelfOverlay::ScreenSystemSettings { platform,row } =>
                (16u8,row,Self::system_page_rows(platform).len()),
            ShelfOverlay::GameplaySettings { row } => (6u8, row, GAMEPLAY_KEYS.len()),
            ShelfOverlay::CartStudio => (30u8, 0, 0),
            ShelfOverlay::CartLibrary => (31u8, 0, 0),
            ShelfOverlay::Romm => (32u8, 0, 0),
            _ => (0, 0, 0),
        };
        // A page of more than six rows shows six and slides its list: the
        // bar moves freely inside them, and the list follows it only once it
        // comes within a row of an edge. The bar's place is in the list's
        // own frame, so both glide together.
        let row_top = |engine: &Self| if kind == 17 { about_row_top() }
            else { engine.menu_rows_top(count.min(engine.visible_rows())) };
        if kind != self.menu_seen {
            self.menu_seen = kind;
            self.menu_opened = now;
            self.menu_scroll_top = crate::controls::scroll_follow_in(0, selected, count, self.visible_rows());
            self.menu_scroll_y = self.menu_scroll_top as f32 * QUICK_PITCH;
            if count > 0 {
                self.menu_cursor_y = row_top(self) + selected as f32 * QUICK_PITCH;
            }
        } else if count > 0 {
            self.menu_scroll_top = crate::controls::scroll_follow_in(self.menu_scroll_top, selected, count, self.visible_rows());
            let goal = row_top(self) + selected as f32 * QUICK_PITCH;
            let response = 1.0 - (-dt * 24.0).exp();
            self.menu_cursor_y += (goal - self.menu_cursor_y) * response;
            self.menu_scroll_y += (self.menu_scroll_top as f32 * QUICK_PITCH - self.menu_scroll_y) * response;
        }

        // Upstream CRT: 160ms extinction, 350ms black hold, 450ms ejection.
        // Do not move the cart during screen power-off or the blank hold.
        if self.exiting_screen || self.eject_hold_remaining > 0.0 {
            if self.exiting_screen {
                self.screen_power = (self.screen_power - dt / SCREEN_POWER_OFF_S).max(0.0);
            } else {
                self.eject_hold_remaining = (self.eject_hold_remaining - dt).max(0.0);
            }
            self.gpu.set_screen_power(self.screen_power);
            if !self.exiting_screen && self.game.is_none() {
                // Keep the GPU on the shelf's workload through the black hold:
                // a frozen (dt = 0) shelf frame is drawn over the hold picture
                // at 1/255 opacity, so its textures are really sampled (an
                // occluded draw would be culled) and the GPU clock stays up.
                cover_with_hold = true;
                dt = 0.0;
            } else {
                let mut commands = Vec::new();
                self.push_hold_scene(&mut commands);
                self.gpu.fit(self.size);
                self.gpu.begin_frame();
                self.gpu.draw_list(&commands);
                self.gpu.end_frame(self.size);
                if self.screen_power <= 0.0 { self.exiting_screen = false; }
                return;
            }
        }
        if self.game.is_some() {
            self.screen_power = (self.screen_power + dt / SCREEN_POWER_ON_S).min(1.0);
            self.gpu.set_screen_power(self.screen_power);
        }
        // Holding physical MENU saves, ejects, and returns to shelf without
        // requiring a second keypress or sending MENU to libretro.
        if self.game.is_some()
            && self.mode_down_at.is_some_and(|t| t.elapsed().as_millis() >= 650)
        {
            self.mode_down_at = None;
            crate::game::log_launch_timing(&format!(
                "Exit requested by held MODE key (rewind_held={})", self.rewind_held));
            self.handle(Input::Exit);
        }

        // A long A press skips loading the auto-save state, but preserves SRAM.
        // At least 400ms must pass before treating a held A as a fresh launch.
        if let Some(when) = self.a_down_at {
            if when.elapsed().as_millis() >= 400 {
                self.fresh_launch = true;
                self.a_down_at = None;
            }
        }
        // Mirror original Slot: the cartridge seats at 450ms and remains
        // seated until 730ms (or until the core is ready, if later).
        // Use the hold to initialize mGBA/gpSP and restore SRAM/auto-state
        // instead of running this work only AFTER the insertion has ended.
        if self.inserted && self.game.is_none() {
            // Read wall time, not accumulated clipped frame dt: a slow core
            // load must be hidden INSIDE the hold, not added after it.
            let inserted_for = self.insert_started.map_or(0.0, |t| t.elapsed().as_secs_f32());
            let mut core_opened_now = false;
            if inserted_for >= SEATED_AT && self.a_down_at.is_none()
                && self.pending_game.is_none()
            {
                if let Some(local) = self.prepared_rom.take() {
                    core_opened_now = true;
                    crate::frame_watch::core_started();
                    let core_at = Instant::now();
                    self.prepare_game_core(&local);
                    crate::frame_watch::did(crate::frame_watch::WORK_CORE);
                    crate::frame_watch::core_prepared(core_at.elapsed());
                }
            }
            // Re-read wall time after core.load() and save-state import.
            let inserted_for = self.insert_started.map_or(0.0, |t| t.elapsed().as_secs_f32());
            if inserted_for < INSERT_S && self.pending_game.is_some() && !core_opened_now {
                // The frame after the core opened, the cart still seated:
                // the coming screen is made ready (3.2 perf).
                self.ready_coming_screen();
            }
            if inserted_for >= INSERT_S {
                if let Some(session) = self.pending_game.take() {
                    crate::game::log_launch_timing(&format!(
                        "Start CRT: {}ms since A (upstream target 730ms)",
                        self.insert_started.map_or(0, |at| at.elapsed().as_millis())
                    ));
                    crate::frame_watch::insert_done();
                    self.buttons = 0;
                    // RETROARCH resamples to 48kHz; open AudioTrack there once.
                    let ra_sync = !session.linked();
                    SAMPLE_RATE.store(if ra_sync { crate::ra_sync::OUTPUT_RATE }
                        else { session.sample_rate }, Ordering::Release);
                    self.ra_resampler.reset();
                    self.ra_sync_steps.reset();
                    let mut session = session;
                    let top = session.sun_top();
                    session.set_sun(self.current_emulation().sun(light_sensor(), top));
                    apply_motion(Some(&session), self.current_emulation().motion());
                    self.game = Some(session);
                    self.play_time = 0.0;
                    // Focus and Recents were persisted during core
                    // preparation; avoid synchronous IO/rebuilding the
                    // carousel in the first game-frame callback.
                    self.game_accum = 0.0;
                    let look_at = Instant::now();
                    // The game's look is applied once, below, with its own
                    // screen size (apply_video_geometry): no look applied
                    // first with the last game's size (3.2 perf).
                    // The exact Slot upstream geometry: GB/GBC 160x144
                    // is centred within the 240x160 libretro canvas.
                    let platform = self.current_cart_platform();
                    // Read while the cart was seated, else now.
                    let ready = match self.screen_ready.take() {
                        Some(ready) => ready,
                        None => {
                            let session = self.game.as_ref().expect("game just set");
                            self.screen_settings(session)
                        }
                    };
                    let ScreenReady { mode, source, frame, square, view, intro } = ready;
                    self.video_mode = mode;
                    let reported = self.game.as_ref().map_or((0, 0), GameSession::frame_size);
                    crate::log_info(&format!("Screen size {:?} ({source}) for {platform:?}: core {}x{}, chosen for {}x{}, square {square}, panel {}x{}",
                        self.video_mode, reported.0, reported.1, frame.0, frame.1, panel_size().0, panel_size().1));
                    self.pokedaisy_view = view;
                    self.pokedaisy_intro = intro;
                    self.pokedaisy_menu = false;
                    self.pokedaisy_hud.clear();
                    let settings_ms = look_at.elapsed().as_millis();
                    self.apply_video_geometry();
                    crate::game::log_launch_timing(&format!(
                        "Screen on work: screen settings {settings_ms}ms, shader and look {}ms",
                        look_at.elapsed().as_millis() - settings_ms));
                    self.screen_power = 0.0;
                    self.exiting_screen = false;
                    self.gpu.set_screen_power(0.0);
                    PLAYING.store(true, Ordering::Release);
                }
            }
        }

        if self.game.is_some() && (self.lid_covered || self.lid_t > 0.0) {
            // RG Rotate closed: the game waits under its cover. No libretro
            // frame, so no audio either, until the lid is open again.
            if let Some(game) = self.game.as_mut() { game.set_link_paused(true); }
            self.game_accum = 0.0;
            self.draw_lid_game();
            return;
        }
        if self.game.is_some() && matches!(self.overlay, ShelfOverlay::States) {
            // Polaroids are a full-screen, paused overlay. Render once per
            // display frame (not from a key event) and never advance libretro
            // or enqueue audio while browsing saved states.
            self.game_accum = 0.0;
            let mut commands = Vec::new();
            if let Some(p) = self.polaroids.as_mut() {
                p.set_undo(self.game.as_ref()
                    .is_some_and(|game| game.undo_available())
                    .then_some("undo"));
                p.draw(None, Printed::default(), None, Printed::default(), &mut commands);
            }
            self.gpu.fit(self.size);
            self.gpu.begin_frame();
            self.gpu.draw_list(&commands);
            self.gpu.end_frame(self.size);
            return;
        }
        if self.game.is_some() && matches!(self.overlay,
            ShelfOverlay::GameMenu { .. } | ShelfOverlay::Multiplayer |
            ShelfOverlay::PauseSettings { .. } | ShelfOverlay::PauseScreenSettings { .. } |
            ShelfOverlay::PauseGameplaySettings { .. } | ShelfOverlay::PauseEmulationSettings { .. } |
            ShelfOverlay::LayoutPorts { game: true, .. } | ShelfOverlay::LayoutPort { game: true, .. } |
            ShelfOverlay::Cheats { .. }) {
            // Coordinate the pause with the remote game. Without this
            // the other Cable peer times out and gpSP RFU can drift.
            if let Some(game) = self.game.as_mut() { game.set_link_paused(true); }
            // No libretro frames are evaluated while a native overlay is
            // visible. Poll transport atomics here as well; otherwise a remote
            // End Link or a broken socket is invisible until the local player
            // closes the menu, potentially leaving a permanent "Linked" UI.
            if let Some(fault) = self.game.as_ref().and_then(GameSession::link_fault) {
                self.peer_link_stopped(fault);
            }
            self.game_accum = 0.0;
            // The paused game as it plays: under its overlay, which the
            // Screen page changes live.
            let bezel = self.game_bezel();
            let mut commands = vec![Draw::Game];
            if let Some((tex,[x,y,w,h])) = bezel {
                commands.push(Draw::Tex { x,y,w,h,tex,alpha:1.0 });
            }
            if let Some(session) = self.game.as_ref().filter(|s| s.core_kind() == Core::PokeDaisy) {
                self.pokedaisy_hud.draw(self.gpu.game_rect(), session.frame_size(), &mut commands);
            }
            self.draw_overlay(&mut commands);
            // The original Slot app draws the HUD AFTER draw_game_menu()
            // (app.rs), including during its Linked animation. The four
            // cyan/purple chain badges are already loaded into our Hud but
            // used to disappear behind the Multiplayer overlay because we
            // skipped this final HUD pass. Keep the exact upstream z-order.
            // The Screen page shows the game, and its toasts (Integer,
            // Original Ratio, Stretch) at the top as in game.
            if matches!(self.overlay, ShelfOverlay::Multiplayer) || self.screen_panel() {
                self.hud.draw(self.born.elapsed().as_millis() as u64, &mut commands);
            }
            self.gpu.fit(self.size);
            self.gpu.begin_frame();
            self.gpu.draw_list(&commands);
            self.gpu.end_frame(self.size);
            return;
        }
        if self.game.is_some() && self.select_down_at.is_some_and(
            |at| at.elapsed().as_millis() >= SELECT_CHORD_MS)
            && !self.select_chorded && !self.select_forwarded
        {
            self.select_forwarded = true;
            self.buttons |= ButtonMask::SELECT;
        }
        let profile = self.current_emulation();
        // Only a game draws its bezel: fitting one on the shelf, each time
        // the selected cart's system changed, held a shelf switch ~200ms on
        // the KONKR (beta41 bench); the game's first frame fits it.
        let bezel = if self.game.is_some() { self.game_bezel() } else { None }; // resolved before mutably borrowing GameSession
        let cart_platform=self.current_cart_platform();
        let live=self.live_game_options(); // Copy, zero allocations on 60/120Hz callback
        FF_AUDIO_ENABLED.store(live.ff_sound,Ordering::Release);
        if live.ff_speed == 0 && (self.ff_held || self.ff_latched) {
            // Fast Forward was set to Off while held or latched: a 0x
            // multiplier would freeze the game, so drop back to 1x.
            self.ff_held = false;
            self.ff_latched = false;
            AUDIO_SPEED_PERMILLE.store(1000, Ordering::Release);
            self.hud.set_ff(FfState::Off);
        }
        // Android's frame callback switches GLSurfaceView to Choreographer
        // frame requests while a solo game runs (RetroArch pacing). No engine
        // work or GL resource ownership ever leaves the GL render thread.
        // Linked sessions keep continuous rendering and wall-clock pacing.
        let solo = self.game.as_ref().is_some_and(|session| !session.linked());
        DISPLAY_SYNC.store(solo, Ordering::Release);
        RA_SWAP_LOOP.store(solo && !profile.responsive_loop, Ordering::Release);
        // Grab clear-cart textures before borrowing the mutable emulator
        // session. They are used only for the 220ms CRT start transition.
        let (transition_back, transition_front) = self.clear_layers();
        let a12_native_started = Instant::now();
        let mut frame_phases = crate::frame_phases::FramePhaseSample::default();
        self.apply_layouts(live.turbo);
        let blending = self.game_blending(cart_platform);
        if let Some(game) = self.game.as_mut() { game.set_frame_blending(blending); }
        let dither = cart_platform == Platform::Md && self.game_dither();
        if self.tilt_on {
            let [x, y] = self.tilt.step(slot_retro::sensors::gravity());
            self.gpu.set_light(x, y);
        }
        // The slot sliding back down over the game just resumed.
        let mut footer_draws = Vec::new();
        self.draw_game_footer(&mut footer_draws);
        // Read before the session's borrow: the low battery sign (#87).
        let mut low_battery = Vec::new();
        self.push_low_battery(&mut low_battery);
        // PokeDaisy's view and menu as this frame leaves them.
        let mut pd_view_seen = None;
        let mut pd_menu_now = self.pokedaisy_menu;
        if let Some(session) = self.game.as_mut() {
            // Resume only after the remote has also left its pause menu;
            // LinkPlay::frame holds either core while peer_paused is true.
            session.set_link_paused(false);
            session.set_throttle(if self.rewind_held { slot_retro::Throttle::Rewinding }
                else if self.ff_held || self.ff_latched { slot_retro::Throttle::FastForward }
                else { slot_retro::Throttle::Normal });
            // In-game HUD replaces the shelf chrome; never let the upper
            // Android indicators remain visible on the first game frame.
            SHELF_INDICATOR_FLAGS.store(0, Ordering::Release);
            let mut stepped = 0;
            let mut peer_fault = None;
            // RETROARCH mode runs Run-Ahead as RetroArch's Preemptive Frames.
            let mut preemptive = false;
            if self.rewind_held {
                self.ra_clock.reset();
                self.ra_sync_steps.reset();
                // Rewind is mutually exclusive with forward libretro frames.
                // Restore up to 3 historical snapshots per render frame and
                // never pass rewound audio into the normal AudioTrack.
                self.game_accum = 0.0;
                // At 1x this restores 10 snapshots/s; at 2x (upstream feel)
                // 20/s; at 5x, 50/s. Native Slot replays ~60 of its 30
                // captures per second. Each restored state
                // refreshes just one video frame, with no forward PCM or
                // speculative Run-Ahead; keep bounded work per callback.
                let rewind_period = crate::settings::rewind_step_seconds(live.rewind_speed);
                self.rewind_accum = (self.rewind_accum + dt).min(0.30);
                while self.rewind_accum >= rewind_period && stepped < 3 {
                    self.rewind_accum -= rewind_period;
                    let rewind_started = Instant::now();
                    let rewound = session.rewind_step();
                    frame_phases.rewind_work_us = frame_phases.rewind_work_us
                        .saturating_add(rewind_started.elapsed().as_micros() as u64);
                    if !rewound {
                        self.rewind_accum = 0.0;
                        break;
                    }
                    stepped += 1;
                }
                AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
                self.hud.show(HudKind::Rewind, session.rewind_fill(), false,
                    self.born.elapsed().as_millis() as u64);
            } else {
                // Original Slot offers 2/3/4/6x, independent of display Hz.
                let fast = self.ff_held || self.ff_latched;
                // MAX: as many frames as fit in most of one refresh, leaving
                // the rest to draw and to read the keys (Siham 2026-11-11).
                let ff_max = fast && live.ff_speed == crate::settings::FF_MAX;
                let ff_max_until = Instant::now() + Duration::from_micros(FF_MAX_BUDGET_US);
                let multiplier = if fast { f64::from(live.ff_speed) } else { 1.0 };
                let period = 1.0 / session.fps;
                // RETROARCH: the display refresh clocks the core whenever the
                // panel is within RetroArch's 5% timing skew of the core rate.
                // Audio is always resampled to 48kHz in solo play (also while
                // fast-forwarding) so the AudioTrack is never rebuilt.
                let ra_sync = !session.linked();
                let display_hz = f64::from(DISPLAY_REFRESH_MILLIHZ.load(Ordering::Acquire)) / 1000.0;
                let ra_display_clocked = ra_sync && !fast
                    && crate::ra_sync::display_can_clock_core(display_hz, session.fps);
                let wanted_rate = if ra_sync { crate::ra_sync::OUTPUT_RATE } else { session.sample_rate };
                let current_rate = SAMPLE_RATE.load(Ordering::Acquire);
                if current_rate != 0 && current_rate != wanted_rate {
                    // Mode switched mid-game: never play PCM at the wrong rate.
                    SAMPLE_RATE.store(wanted_rate, Ordering::Release);
                    AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
                    AUDIO_OUT_QUEUED_FRAMES.store(0, Ordering::Release);
                    self.ra_resampler.reset();
                }
                let ra_ratio = if ra_sync {
                    let core_rate = f64::from(session.sample_rate);
                    let base = if ra_display_clocked {
                        crate::ra_sync::base_ratio(core_rate, session.fps,
                            self.ra_sync_steps.display_hz(display_hz))
                    } else { f64::from(crate::ra_sync::OUTPUT_RATE) / core_rate };
                    self.ra_adjust = if fast { 1.0 } else {
                        let fifo_frames = AUDIO.lock().unwrap_or_else(|e| e.into_inner()).len() / 2;
                        let track = AUDIO_OUT_QUEUED_FRAMES.load(Ordering::Acquire).max(0) as usize;
                        let capacity = AUDIO_OUT_CAPACITY_FRAMES.load(Ordering::Acquire).max(0) as usize;
                        crate::ra_sync::rate_control_adjust(fifo_frames + track, capacity)
                    };
                    base * self.ra_adjust
                } else {
                    self.ra_adjust = 1.0;
                    self.ra_resampler.reset();
                    1.0
                };
                // When the panel cannot clock the core, accumulate UI VSync
                // time instead of GL wall time (RA CLOCK).
                let ra_clock = ra_sync && !ra_display_clocked && !fast;
                let ra_elapsed = if ra_clock {
                    self.ra_clock.sample(vsync_frame_ns, f64::from(dt))
                } else {
                    self.ra_clock.reset();
                    f64::from(dt)
                };
                // Preemptive Frames: only real frames are shown, and the core
                // rolls back and replays only when the buttons change.
                preemptive = ra_sync && !fast;
                session.set_preemptive_frames(
                    if preemptive { profile.run_ahead_frames } else { 0 });
                // A11: this is the *same* double-GBA lockstep on both ends,
                // even for equal ROMs. One late packet must not make gameplay
                // and audio permanently lose an emulated frame. Restrict
                // recovery to mGBA Cable; gpSP and solo retain their policy.
                let mgba_cable_pacing = session.linked()
                    && session.core_kind() == Core::Mgba && !fast
                    && MGBA_CABLE_RECOVERY.load(Ordering::Acquire);
                if ra_display_clocked {
                    // No fractional debt: run exactly the refreshes that elapsed.
                    let refreshes = if RA_SWAP_LOOP.load(Ordering::Acquire) {
                        self.ra_sync_steps.swap_paced_steps(vsync_frame_ns, display_hz)
                    } else {
                        self.ra_sync_steps.steps(vsync_frame_ns, display_hz)
                    };
                    self.game_accum = period * refreshes as f64;
                } else if mgba_cable_pacing {
                    self.ra_sync_steps.reset();
                    self.game_accum = crate::settings::frame_debt_mgba_cable(
                        self.game_accum, f64::from(dt), period);
                } else if ff_max {
                    self.game_accum = period * FF_MAX_FRAMES as f64;
                } else if ra_clock {
                    self.game_accum = crate::settings::frame_debt_ra_clock(
                        self.game_accum, ra_elapsed, multiplier, period);
                } else {
                    self.game_accum = crate::settings::frame_debt(
                        self.game_accum, f64::from(dt), multiplier, fast);
                }
                // RA CLOCK commits strictly one frame per callback: two-frame
                // catch-up measured worse motion and controls on the KONKR.
                let frame_limit = if ra_display_clocked || mgba_cable_pacing { 2 }
                    else if ff_max { FF_MAX_FRAMES }
                    else if fast { usize::from(live.ff_speed) * 4 }
                    else if ra_clock { 1 } else { 4 };
                // A13.1: revert the experimental second-frame budget.
                // Real-world A13 KONKR/AVD tests suffered repeated Link
                // wait_peer stalls and severe AudioTrack starvation.
                // A12's A11 bounded-debt/2-frame policy was smoother.
                while self.game_accum >= period && stepped < frame_limit {
                    // Cable lockstep must not consume emulation time while
                    // the remote frame has not arrived. NEVER invent a solo
                    // frame or perform speculative Run-Ahead on a held link.
                    session.set_port2(self.buttons2);
                    let advanced = session.advance(self.buttons, live.turbo,
                        live.rewind && !session.linked());
                    let timing = session.last_step_timings();
                    frame_phases.core_us = frame_phases.core_us
                        .saturating_add(timing.core_us);
                    frame_phases.rewind_work_us = frame_phases.rewind_work_us
                        .saturating_add(timing.rewind_work_us);
                    frame_phases.periodic_battery_us = frame_phases.periodic_battery_us
                        .saturating_add(timing.periodic_battery_us);
                    if !advanced {
                        // A waiting mGBA Cable frame is NOT an emulated frame.
                        // Retain at most two frames of debt; only real remote
                        // inputs can advance them later. Never preserve debt
                        // after an actual Link fault or in solo/gpSP.
                        self.game_accum = if mgba_cable_pacing
                            && session.link_fault().is_none() {
                            self.game_accum.min(period * 2.0)
                        } else { 0.0 };
                        if let Some(fault) = session.link_fault() {
                            // Release the session borrow after its last frame, then recover.
                            peer_fault = Some(fault);
                        }
                        break;
                    }
                    // This frame read every pending press: apply held releases.
                    self.unseen_presses = 0;
                    self.buttons &= !self.held_releases;
                    self.held_releases = 0;
                    self.unseen2 = 0;
                    self.buttons2 &= !self.held_releases2;
                    self.held_releases2 = 0;
                    self.perf_emulated = self.perf_emulated.saturating_add(1);
                    frame_phases.core_steps = frame_phases.core_steps.saturating_add(1);
                    self.game_accum -= period;
                    stepped += 1;
                    if ff_max && Instant::now() >= ff_max_until { self.game_accum = 0.0; }
                    let samples = session.take_audio();
                    let queue_pcm = (!fast || (live.ff_sound &&
                        FF_AUDIO_SUPPORTED.load(Ordering::Acquire)))
                        && !samples.is_empty();
                    if queue_pcm {
                        // Only the committed timeline's PCM is forwarded.
                        // Drop AUDIO's lock before waking its Java writer;
                        // speculative Run-Ahead MUST never supply audio.
                        {
                            let mut audio = AUDIO.lock().unwrap_or_else(|e| e.into_inner());
                            // Extracted, host-tested copy of the beta8 FIFO
                            // policy. The observed dropped-old count never
                            // changes ordering, capacity or playback timing.
                            let pcm: &[i16] = if ra_sync {
                                self.ra_resampled.clear();
                                self.ra_resampler.process(&samples, ra_ratio, &mut self.ra_resampled);
                                &self.ra_resampled
                            } else { &samples };
                            // RETROARCH keeps at most one output buffer
                            // waiting here, so audio can never lag the picture
                            // by more than that even if production runs fast.
                            let limit = if ra_sync && !fast {
                                crate::ra_sync::fifo_limit_shorts(
                                    AUDIO_OUT_CAPACITY_FRAMES.load(Ordering::Acquire).max(0) as usize)
                            } else { 96_000 };
                            let dropped = crate::audio_pcm::enqueue_pcm_bounded(
                                &mut audio, pcm, limit);
                            if dropped > 0 {
                                AUDIO_FIFO_OVERFLOW_EVENTS.fetch_add(1, Ordering::Relaxed);
                                AUDIO_FIFO_DROPPED_SHORTS.fetch_add(dropped as u64, Ordering::Relaxed);
                            }
                        }
                        // The Android notification otherwise happens only
                        // AFTER speculation + GLES. Wake AudioTrack as soon
                        // as committed PCM exists, so its writer can run
                        // during speculation.
                        if ra_sync && !fast { on_pcm_ready(); }
                    }
                    // The only consumer has already queued/muted these samples.
                    // Retain its Vec allocation for the next committed frame.
                    // No changes to packet contents, audio scheduling or
                    // speculative Run-Ahead frame ordering.
                    session.recycle_audio(samples);
                }
                // RETROARCH never carries fractional time between refreshes;
                // MAX starts each refresh afresh.
                if ra_display_clocked || ff_max { self.game_accum = 0.0; }
            }
            if stepped > 0 && !self.rewind_held && !self.ff_held && !self.ff_latched
                && profile.run_ahead_frames > 0 && !preemptive {
                // The normal frames and their audio are already consumed.
                // Predicted video never advances the real saved/RA timeline.
                let speculation_started = Instant::now();
                session.set_port2(self.buttons2);
                session.prepare_runahead(self.buttons, live.turbo,
                    profile.run_ahead_frames);
                frame_phases.speculative_us = speculation_started.elapsed().as_micros() as u64;
            }
            // Run-Ahead or Preemptive Frames may have quarantined the session.
            if session.runahead_rollback_failed() {
                // A partially restored speculative core must never tick
                // or reach any periodic/manual/eject save path. Stop audio
                // and force a visible, retryable unsaved-exit warning.
                self.game_accum = 0.0;
                self.rewind_held = false;
                self.ff_held = false;
                self.ff_latched = false;
                clear_rumble();
                PLAYING.store(false, Ordering::Release);
                AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clear();
                self.overlay = ShelfOverlay::GameMenu { row: 0 };
                set_message("Paused to protect your save. Eject twice to quit.".into());
            }
            match stepped {
                0 => self.perf_step0 = self.perf_step0.saturating_add(1),
                1 => self.perf_step1 = self.perf_step1.saturating_add(1),
                _ => self.perf_step_multi = self.perf_step_multi.saturating_add(1),
            }
            if stepped > 1 {
                self.perf_catchup = self.perf_catchup.saturating_add(1);
            }
            if stepped > 0 {
                if self.select_tap_frames > 0 {
                    self.select_tap_frames = self.select_tap_frames.saturating_sub(stepped as u8);
                    if self.select_tap_frames == 0 { self.buttons &= !ButtonMask::SELECT; }
                }
                let upload_started = Instant::now();
                if session.frame_size() == (slot_retro::GBA_W, slot_retro::GBA_H)
                    && session.display_aspect() <= 0.0 {
                    self.gpu.upload_game(session.frame());
                } else {
                    // Home consoles: the frame at its own size, fitted at
                    // the aspect the core reports (it may change mid-game).
                    // NES, SNES and Mega Drive Frame Blending: mixed here,
                    // before any shader draws the picture.
                    let size = session.frame_size();
                    let picture = self.frame_blend.apply(session.slot_blending(), session.frame(), size);
                    let picture = if dither { self.dither_blend.apply(picture, size) } else { picture };
                    self.gpu.upload_game_sized(picture, size);
                    let square = video_mode::square_game(cart_platform, panel_size(),
                        session.square_listed());
                    self.gpu.set_game_fit(if session.core_kind() == Core::PokeDaisy {
                        video_mode::pokedaisy_fit(self.pokedaisy_view, cart_platform, self.video_mode, size)
                    } else {
                        video_mode::home_fit(cart_platform, self.video_mode,
                            session.display_aspect(), session.frame_size(), square)
                    });
                }
                if let Some(hud) = session.take_pokedaisy_hud() {
                    self.pokedaisy_hud.update(&mut self.gpu, hud);
                }
                pd_view_seen = session.take_pokedaisy_view();
                pd_menu_now = session.pokedaisy_menu();
                frame_phases.gpu_upload_us = upload_started.elapsed().as_micros() as u64;
                let peak = session.take_rumble_peak();
                if live.rumble && !self.rewind_held && GAME_UNPAUSED.load(Ordering::Acquire) {
                    RUMBLE_PEAK.fetch_max(scale_rumble(peak, live.rumble_level), Ordering::AcqRel);
                    RUMBLE_STRENGTH.store(scale_rumble(session.rumble_strength(), live.rumble_level), Ordering::Release);
                } else { clear_rumble(); }
            }
            // CPU wall time issuing the GL draw work. This is NOT asynchronous
            // GPU execution and excludes Android's later eglSwapBuffers.
            let submit_started = Instant::now();
            self.gpu.fit(self.size);
            self.gpu.begin_frame();
            frame_phases.submit_setup_us = submit_started.elapsed().as_micros() as u64;
            let build_started = Instant::now();
            let mut commands = Vec::new();
            if self.screen_power < 1.0 {
                // Native Slot v1.7 draws the seated cart and slot behind the
                // CRT picture while the LCD grows to full width (220ms).
                // We use the existing GamePass screen-power shader, without
                // adding a separate full-screen fade or extra libretro frames.
                let shelf = &self.shelves[self.active];
                if let Some(cart) = shelf.carts.get(shelf.index) {
                    let (rest, scale) = shelf.selected_at();
                    slot_ui::draw_ground_tinted(None, 0.0, &mut commands);
                    SlotChrome {
                        cart, face: shelf.face(shelf.index), rest, scale,
                        seat: self.progress, alert: None, dim: 0.0,
                        screen: self.screen_power, game: true, over: false,
                        slot_faces: pick_slot_faces(self.slot_faces, self.home_slot_faces, Some(cart)),
                    }.draw_with_closer(&mut commands, transition_back, transition_front,
                    pick_closer(self.slot_faces, self.home_closers, Some(cart)),
                    !self.inserted && self.progress > 0.0);
                } else {
                    commands.push(Draw::Game);
                }
            } else {
                commands.push(Draw::Game);
            }
            if let Some((tex,[x,y,w,h]))=bezel {
                commands.push(Draw::Tex { x,y,w,h,tex,alpha:1.0 });
            }
            if session.core_kind() == Core::PokeDaisy {
                self.pokedaisy_hud.draw(self.gpu.game_rect(), session.frame_size(), &mut commands);
            }
            commands.extend_from_slice(&footer_draws);
            commands.append(&mut low_battery);
            self.hud.draw(self.born.elapsed().as_millis() as u64, &mut commands);
            frame_phases.submit_build_us = build_started.elapsed().as_micros() as u64;
            let draw_started = Instant::now();
            self.gpu.draw_list(&commands);
            frame_phases.submit_draw_us = draw_started.elapsed().as_micros() as u64;
            let blit_started = Instant::now();
            let end_phases = self.gpu.end_frame_profiled(self.size);
            if std::mem::take(&mut self.screenshot_requested) { capture_screenshot(&self.gpu, self.size); }
            frame_phases.submit_blit_us = blit_started.elapsed().as_micros() as u64;
            frame_phases.end_setup_us = end_phases.setup_us;
            frame_phases.end_geometry_us = end_phases.geometry_us;
            frame_phases.end_bind_us = end_phases.bind_us;
            frame_phases.end_viewport_us = end_phases.viewport_us;
            frame_phases.end_clear_us = end_phases.clear_us;
            frame_phases.end_state_us = end_phases.state_us;
            frame_phases.end_quad_us = end_phases.quad_us;
            frame_phases.end_bezel_us = end_phases.bezel_us;
            // The nested setup/build/draw/blit timers are subsets of submit;
            // do not add them a second time in the native residual.
            frame_phases.gpu_submit_us = submit_started.elapsed().as_micros() as u64;
            frame_phases.native_total_us = phase_native_started.elapsed().as_micros() as u64;
            FRAME_PHASE_PROBE.with(|probe| {
                let mut probe = probe.borrow_mut();
                let window = probe.get_or_insert_with(crate::frame_phases::PhaseWindow::new);
                if let Some(summary) = window.add(frame_phases) {
                    // Exactly one log entry per ~5s; never stringify each draw.
                    crate::game::log_launch_timing(&summary.report());
                }
            });
            a12_draw_sample(session.linked() && session.core_kind() == Core::Mgba,
                stepped, a12_native_started.elapsed().as_micros() as u64,
                MGBA_CABLE_RECOVERY.load(Ordering::Acquire));
            let shut_down = session.shut_down();
            if let Some(view) = pd_view_seen { self.keep_pokedaisy_view(view); }
            if pd_menu_now != self.pokedaisy_menu {
                self.pokedaisy_menu = pd_menu_now;
                self.apply_game_look();
            }
            if let Some(fault) = peer_fault { self.peer_link_stopped(fault); }
            if shut_down {
                crate::game::log_launch_timing("The game shut itself down: ejecting");
                self.handle(Input::Exit);
            }
            return;
        }

        // Shelf mode clears the opt-in clock between cartridges.
        self.ra_clock.reset();
        self.ra_sync_steps.reset();
        self.frame_delay.reset();
        // Shelf mode is not emulation: do not merge idle UI into game phases.
        FRAME_PHASE_PROBE.with(|probe| *probe.borrow_mut() = None);
        let ms = self.born.elapsed().as_millis() as u64;
        let repeated_letter = if !self.inserted && matches!(self.overlay,ShelfOverlay::None) {
            self.shelves[self.active].tick(ms)
        } else {
            self.shelves[self.active].release_hold();
            false
        };
        if repeated_letter { self.show_selected_letter(); }
        let tick_at = Instant::now();
        self.shelves[self.active].update(dt);
        self.tick_triggers();
        crate::frame_watch::span(crate::frame_watch::SPAN_TICK, tick_at.elapsed());
        crate::frame_watch::span(crate::frame_watch::SPAN_BEFORE, before_at.elapsed());
        let upload_at = Instant::now();
        self.relabel_one();
        self.prepare_visible();
        self.prepare_shelves_grid();
        crate::frame_watch::part(crate::frame_watch::PART_UPLOAD, upload_at.elapsed());
        self.start_waiting_picker();
        // Only warm a settled carousel selection. Actual ROM I/O takes place
        // on an independently cancelable Android background executor.
        let warming = !self.inserted && self.game.is_none()
            && self.pending_game.is_none() && !self.awaiting_game
            && self.progress <= 0.0
            && matches!(self.overlay, ShelfOverlay::None)
            && !self.shelves[self.active].is_scrolling()
            && self.last_cart_navigation.elapsed() >= Duration::from_millis(900);
        let candidate = if warming {
            self.shelves[self.active].carts
                .get(self.shelves[self.active].index)
                .map(|cart|cart.rom.to_string_lossy().into_owned())
                // only ROM files are copied ahead (Splore is none)
                .filter(|uri|uri.starts_with("content://"))
        } else { None };
        let mut last = WARM_ROM_URI.lock().unwrap_or_else(|e|e.into_inner());
        if *last != candidate { *last = candidate; }
        drop(last);
        let title_at = Instant::now();
        self.refresh_selected_title();
        crate::frame_watch::span(crate::frame_watch::SPAN_TITLE, title_at.elapsed());
        let focus_at = Instant::now();
        self.persist_focus_if_idle();
        crate::frame_watch::span(crate::frame_watch::SPAN_FOCUS, focus_at.elapsed());
        let state_at = Instant::now();
        // Resolve the optional transparent material before mutably borrowing
        // the selected rack, otherwise Rust rejects the overlapping borrows.
        let (selected_clear_back, selected_clear_front) = self.clear_layers();
        // L2 + R2: the cart leaving, the ring while held (over the slot).
        let ejecting_stem = self.ejecting_stem().map(str::to_owned);
        let mut trigger_draws = Vec::new();
        self.draw_triggers(&mut trigger_draws);
        // The shelf's own wallpaper, else General's, read before the shelf
        // is borrowed for drawing.
        let wallpaper_none = self.wallpaper_none();
        let wallpaper = self.active_wallpaper();
        // A long press lifts the selected cart; it hides while it flies to or
        // from Cart Studio (Siham 2026-11-17). Read before the shelf's borrow.
        let press_lift = self.press_lift();
        let flight_hides = self.studio_flight_hides_cart();
        // A title too long for a narrow screen scrolls (#119).
        let (title_shift, _) = self.title_scroll();
        let lifted = self.shelves[self.active].selected_rect().map(|(x, y, w, h)| self.lifted([x, y, w, h]));
        let mut studio_return = Vec::new();
        self.draw_studio_return(&mut studio_return);
        // The grey of carts whose card is out (beta39), read before the
        // shelf is borrowed.
        let (missing_ink, cart_ink) = (self.missing_ink(), self.cart_ink());
        crate::frame_watch::span(crate::frame_watch::SPAN_STATE, state_at.elapsed());
        let shelf_at = Instant::now();
        let shelf = &mut self.shelves[self.active];

        // Original Slot: 450ms seating + 280ms hold. Start the original
        // insert clip 97ms before full contact; its embedded lead completes
        // exactly at the seating point. Ejection follows CRT-off + black hold.
        let previous_progress = self.progress;
        self.progress = advance_cart(self.progress, self.inserted, dt);
        if self.inserted && previous_progress < INSERT_SOUND_PROGRESS
            && self.progress >= INSERT_SOUND_PROGRESS
        {
            CART_SFX.lock().unwrap_or_else(|e| e.into_inner()).push_back(CART_INSERT_SFX);
            crate::frame_watch::sound();
        }
        if !self.inserted && self.eject_sound_armed
            && self.progress < previous_progress
            && self.progress <= EJECT_SOUND_PROGRESS {
            self.eject_sound_armed = false;
            CART_SFX.lock().unwrap_or_else(|e| e.into_inner()).push_back(CART_EJECT_SFX);
            crate::frame_watch::sound();
        }
        if previous_progress > 0.0 && self.progress <= 0.0 && !self.inserted {
            crate::frame_watch::cart_out();
        }

        // The shelf's carts in `commands` (the closed RG Rotate shrinks them).
        let mut lid_row = 0..0;
        let mut commands = vec![Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: out_w() as f32,
            h: OUT_H as f32,
            // None: the theme's colour, Dark's grey, OLED's black, Light's
            // and Original's own (Siham 2026-11-14). An image covers it.
            colour: if wallpaper_none {
                menu_style::shelf_ground()
            } else { menu_style::page_ground(menu_style::DARK_SHELF) },
        }];
        // Slot v1.7 carousel ground: None has no wallpaper; personal images
        // receive only their Tint (never a fixed scrim).
        // The shelf's own wallpaper, else General's (Interface > Wallpaper).
        // OLED blackens only the ground: a chosen wallpaper shows as in Dark.
        let tint = wallpaper.map_or(0.0, |(_, tint, _)| tint);
        slot_ui::draw_ground_tinted(wallpaper.map(|(tex, _, _)| tex), tint, &mut commands);
        if self.progress == 0.0 {
            if let Some(picker) = self.picker {
                let openness = picker.openness(self.born.elapsed().as_millis() as u64);
                let t = slot_ui::ease(openness);
                let dim = 1.0 + (0.614 - 1.0) * t;
                let span = Some(picker_span(shelf, openness));
                let selected = shelf.carts.get(shelf.index).map(|c| c.stem.as_str());
                let row_from = commands.len();
                let grey = cart_ink;
                if grey.is_some() { commands.push(Draw::Ink(grey)); }
                shelf.draw_row_clear_of(selected, 0.0, 0.26 * t, dim, span, &mut commands);
                if grey.is_some() { commands.push(Draw::Ink(None)); }
                lid_row = row_from..commands.len();
                slot_ui::draw_empty_slot_closed(
                    pick_slot_faces(self.slot_faces, self.home_slot_faces, shelf.carts.get(shelf.index)),
                    pick_closer(self.slot_faces, self.home_closers, shelf.carts.get(shelf.index)), &mut commands);
            } else {
                // Match upstream Refusal: only the shelf carts shake.
                // The physical slot stays fixed; no input delay or layout changes.
                let shake = self.refusal.map_or(0.0, |r| r.offset(ms));
                let row_from = commands.len();
                let hidden = ejecting_stem.as_deref().or_else(|| {
                    (press_lift > 0.0 || flight_hides)
                        .then(|| shelf.carts.get(shelf.index).map(|c| c.stem.as_str())).flatten()
                });
                // In colour, then grey over it as the card goes (a crossfade
                // while it changes; one row only otherwise).
                let grey = missing_mix;
                if grey < 1.0 {
                    shelf.draw_row(hidden, shake, 0.0, 1.0, &mut commands);
                    if let Some([x, y, w, h]) = lifted.filter(|_| press_lift > 0.0 && !flight_hides) {
                        shelf.draw_selected_at(x, y, w, h, 1.0, &mut commands);
                    }
                }
                if grey > 0.0 {
                    commands.push(Draw::Ink(Some(missing_ink)));
                    shelf.draw_row(hidden, shake, 0.0, grey, &mut commands);
                    if let Some([x, y, w, h]) = lifted.filter(|_| press_lift > 0.0 && !flight_hides) {
                        shelf.draw_selected_at(x, y, w, h, grey, &mut commands);
                    }
                    commands.push(Draw::Ink(None));
                }
                lid_row = row_from..commands.len();
                slot_ui::draw_empty_slot_closed(
                    pick_slot_faces(self.slot_faces, self.home_slot_faces, shelf.carts.get(shelf.index)),
                    pick_closer(self.slot_faces, self.home_closers, shelf.carts.get(shelf.index)), &mut commands);
                commands.append(&mut trigger_draws);
                // Official v1.7.0 shelf: system (L1/R1) or letter
                // (D-pad Up/Down) uses a 200/1200/800 ms full-opacity name.
                // Game Titles crossfade back in during the last 800ms, and
                // stay visible after the name expires. Plain Left/Right
                // scrolling swaps the title immediately with no name fade.
                let name = if let Some(since) = self.platform_shown_at {
                    let elapsed=since.elapsed().as_millis() as u64;
                    if platform_name_running(elapsed) {
                        let face=match self.active {
                                RECENTS_RACK=>self.recents_platform_face,
                                FAVORITES_RACK=>self.favorites_platform_face,
                                VIDEO_RACK=>self.video_platform_face,
                                TEST_RACK=>self.test_platform_face,
                                index=>self.platform_faces.get(index).copied()
                                    .unwrap_or(self.test_platform_face),
                            };
                        Some((face,elapsed,true))
                    } else {
                        self.platform_shown_at=None;
                        None
                    }
                } else if let Some(since)=self.letter_shown_at {
                    let elapsed=since.elapsed().as_millis() as u64;
                    if platform_name_running(elapsed) {
                        Some((self.letter_face,elapsed,false))
                    } else {
                        self.letter_shown_at=None;
                        None
                    }
                } else {None};
                // The words over a light Theme wallpaper turn dark.
                let shelf_ink = self.shelf_ink();
                if shelf_ink.is_some() { commands.push(Draw::Ink(shelf_ink)); }
                if let Some((face,elapsed,is_platform))=name {
                    let alpha=slot_name_alpha(elapsed);
                    if alpha>0.0 {
                        if is_platform {draw_platform_name(face,alpha,&mut commands);}
                        else {draw_slot_name(face,alpha,&mut commands);}
                    }
                }
                // HOLD TO DELETE takes the title's place while L2 + R2 are held.
                if self.settings.show_titles && self.triggers.hold.is_none() && self.triggers.ejecting.is_none() {
                    let alpha=name.map_or(1.0,|(_,elapsed,_)|slot_title_alpha(elapsed));
                    slot_ui::draw_slot_title_scrolled(self.selected_title_face,alpha,title_shift,&mut commands);
                }
                if shelf_ink.is_some() { commands.push(Draw::Ink(None)); }
                commands.append(&mut studio_return);
            }
        } else {
            let cart = &shelf.carts[shelf.index];
            let (rest, scale) = shelf.selected_at();
            // Identical 450ms recede/return as upstream slot. v1.7.
            // Use the existing seat progress; never alter the ROM or game timing.
            shelf.draw_row(Some(cart.stem.as_str()), 0.0, self.progress, 1.0, &mut commands);
            SlotChrome {
                cart,
                face: shelf.face(shelf.index),
                rest,
                scale,
                seat: self.progress,
                alert: None,
                dim: 0.0,
                screen: 0.0,
                game: false,
                over: true,
                slot_faces: pick_slot_faces(self.slot_faces, self.home_slot_faces, Some(cart)),
            }
            .draw_with_closer(&mut commands,selected_clear_back,selected_clear_front,
                pick_closer(self.slot_faces, self.home_closers, Some(cart)),
                !self.inserted && self.progress > 0.0);
        }
        // The onboarding keycap belongs to Slot's own rendering lifecycle,
        // never the Android overlay. No hint is drawn behind an open menu.
        if self.progress == 0.0 && matches!(self.overlay, ShelfOverlay::None) {
            let library_shown = self.shelves.iter().any(|s| !s.carts.is_empty());
            if status_has_bar(self.shelf_status) && library_shown {
                // Refresh Library: the scan shows top right, by the labels'
                // (Android's task rail), never over the carts.
            } else if status_has_bar(self.shelf_status) {
                self.draw_first_scan(&mut commands);
            } else if self.shelf_status > 0 {
                let (tex, w, h) = self.shelf_status_faces[usize::from(self.shelf_status)];
                commands.push(Draw::Tex {
                    x: (out_w() as f32 - w as f32) / 2.0,
                    // Keep status text below the Android corner indicators.
                    y: 74.0,
                    w: w as f32, h: h as f32,
                    tex, alpha: 1.0,
                });
                {
                    // Exactly the upstream Slot START keycap; menus hide both
                    // messages and keycaps rather than leaving Android overlays.
                    let (tex,w)=if self.shelf_status==2 {
                        self.shelf_no_games_hint
                    } else {
                        self.shelf_status_start
                    };
                    commands.push(Draw::Tex {
                        x: (out_w() as f32 - w as f32) / 2.0,
                        y: 110.0,
                        w: w as f32, h: HINT_H as f32,
                        tex, alpha: 1.0,
                    });
                    if self.shelf_status==2 {
                        let (tex,w)=self.onboarding_note;
                        commands.push(Draw::Tex {
                            x: (out_w() as f32 - w as f32) / 2.0,
                            y: 140.0,
                            w: w as f32, h: HINT_H as f32,
                            tex, alpha: 0.76,
                        });
                    }
                }
            } else if self.shelves.iter().all(|s| s.carts.is_empty()) {
                let (tex, w) = self.first_run_hint;
                // Shrunk just enough to fit a square screen (Siham 2026-11-09).
                let k = ((out_w() as f32 - 24.0) / w.max(1) as f32).min(1.0);
                let (w, h) = (w as f32 * k, HINT_H as f32 * k);
                commands.push(Draw::Tex {
                    x: (out_w() as f32 - w) / 2.0,
                    y: 34.0 + (HINT_H as f32 - h) / 2.0,
                    w,
                    h,
                    tex, alpha: 1.0,
                });
                let (tex,w)=self.onboarding_note;
                commands.push(Draw::Tex {
                    x: (out_w() as f32 - w as f32) / 2.0,
                    y: 64.0,
                    w: w as f32, h: HINT_H as f32,
                    tex, alpha: 0.76,
                });
            }
        }
        crate::frame_watch::span(crate::frame_watch::SPAN_SHELF, shelf_at.elapsed());
        let overlay_at = Instant::now();
        self.draw_overlay(&mut commands);
        crate::frame_watch::span(crate::frame_watch::SPAN_OVERLAY, overlay_at.elapsed());
        let chrome_at = Instant::now();
        self.draw_menu_task_pill(&mut commands);
        if cover_with_hold {
            let mut hold = Vec::with_capacity(commands.len() + 8);
            self.push_hold_scene(&mut hold);
            hold.extend(commands.iter().filter_map(|draw| ghost(*draw)));
            self.gpu.fit(self.size);
            self.gpu.begin_frame();
            self.gpu.draw_list(&hold);
            self.gpu.end_frame(self.size);
            return;
        }
        let lid = self.game.is_none() && self.lid_t > 0.0;
        if lid { self.push_lid_shelf(&mut commands, lid_row); }
        // Keep the upper indicators on the identical visibility lifecycle
        // as Slot's footer: only on an idle, menu-free cartridge carousel.
        // A first run shows its sentence alone: no clock, battery or icons
        // until the first scan starts (Siham 2026-11-09).
        let first_run = self.shelf_status == 0 && self.shelves.iter().all(|s| s.carts.is_empty());
        // Not over the shelves page: its tiles take the screen (3.0.1-beta12).
        let chrome = self.progress == 0.0 && matches!(self.overlay, ShelfOverlay::None) && !first_run;
        let mut indicator_flags = 0;
        if chrome {
            indicator_flags |= 1; // show upper indicators
            if self.settings.show_wifi { indicator_flags |= 2; }
            if self.settings.show_bluetooth { indicator_flags |= 4; }
            if self.settings.show_integrations { indicator_flags |= 8; }
            let light=self.settings.appearance==crate::settings::Appearance::Light;
            if light { indicator_flags |= 16; } // theme ink for Android Wi-Fi/task glyphs
            // Settings > Status Bar: where the icons and the clock go. On
            // the same line they share it, the icons on the left.
            let near_square = near_square_panel();
            let clock_top = self.settings.clock_on_top || near_square;
            let icons_bottom = self.settings.icons_on_bottom && !near_square;
            let icons = self.settings.show_wifi || self.settings.show_bluetooth
                || self.settings.show_integrations;
            let grouped = icons && clock_top != icons_bottom;
            if icons_bottom { indicator_flags |= 32; }
            if grouped { indicator_flags |= 64; }
            if self.settings.apps_running_only { indicator_flags |= 128; }
            // Each line is inked for what is behind it, the icons as the
            // clock and battery: on the console bar, the bar's own ink; at
            // the top, black on a light wallpaper, white on a dark one.
            let top_dark = self.top_line_dark_ink();
            let line_dark = |bottom: bool| if bottom { self.footer_dark_ink() } else { top_dark };
            if line_dark(icons_bottom) { indicator_flags |= 256; }
            let dark_ink = line_dark(!clock_top);
            if dark_ink != self.footer_inked_dark { self.recolour_footer_text_to(dark_ink); }
            let (clock_w, battery_w) = if lid {
                // The closed Rotate grows them in their corners; the Black
                // cover hides them, the clocks show their own, and over a
                // cover the ink is white.
                if self.settings.rotate_shelf_cover < 3 {
                    let ink_dark = dark_ink && self.settings.rotate_shelf_cover == 0;
                    self.push_lid_status(ink_dark, 1.0, &mut commands);
                }
                (0.0, 0.0)
            } else {
                slot_ui::draw_status_clock(clock_top,
                    self.battery, self.battery_face,
                    Some(if dark_ink {self.bolt_light_texture} else {self.bolt_texture}),
                    self.clock_face, slot_ui::footer_ink(dark_ink), &mut commands)
            };
            // The task pill (beta39), by the battery: Android's icons go
            // before it.
            let battery_w = if lid { battery_w } else {
                let icons_top = icons && !icons_bottom;
                battery_w + self.draw_shelf_task_pill(clock_top, icons_top, battery_w, top_dark, &mut commands)
            };
            // Sharing the line, the icons sit after the clock and before the
            // battery: Android places them from these widths (8 bits each).
            if grouped {
                indicator_flags |= (clock_w.round().clamp(0.0, 255.0) as i32) << 10;
                indicator_flags |= (battery_w.round().clamp(0.0, 255.0) as i32) << 18;
            }
        }
        SHELF_INDICATOR_FLAGS.store(indicator_flags, Ordering::Release);
        crate::frame_watch::span(crate::frame_watch::SPAN_CHROME, chrome_at.elapsed());
        let gpu_at = Instant::now();
        self.gpu.fit(self.size);
        self.gpu.begin_frame();
        self.gpu.draw_list(&commands);
        self.gpu.end_frame(self.size);
        crate::frame_watch::part(crate::frame_watch::PART_GPU, gpu_at.elapsed());
        // Idle throttle is an output-only pacing hint; never change upstream
        // animation timings, input handling, or cartridge prefetch priorities.
        let quiet = !self.inserted
            && self.game.is_none() && self.pending_game.is_none()
            && !self.awaiting_game && !self.external_pending
            && self.progress <= 0.0
            && matches!(self.overlay, ShelfOverlay::None)
            && !status_has_bar(self.shelf_status) // scan and download progress stay animated
            && self.last_cart_navigation.elapsed() >= Duration::from_millis(3200)
            && self.menu_opened.elapsed() >= Duration::from_millis(1200)
            && !self.shelves[self.active].is_scrolling()
            // The delete hold and the eject animate with no key held down
            // anew: dozing there made them stutter (Siham 2026-11-10).
            && self.triggers.hold.is_none() && self.triggers.ejecting.is_none()
            // The RG Rotate's cover animation (clock and battery growing).
            && self.lid_t == if self.lid_covered { 1.0 } else { 0.0 }
            // A long title scrolling its passes (#119).
            && !self.title_scroll().1
            && self.missing_mix == if ROMS_MISSING.load(Ordering::Acquire) { 1.0 } else { 0.0 }
            && self.cart_render.is_idle();
        CAROUSEL_QUIET.store(quiet, Ordering::Release);
    }
}

/// The same draw at 1/255 opacity: invisible, but never culled by the GPU.
fn ghost(draw: Draw) -> Option<Draw> {
    const GHOST: f32 = 1.0 / 255.0;
    match draw {
        Draw::Rect { x, y, w, h, colour: [r, g, b, a] } =>
            Some(Draw::Rect { x, y, w, h, colour: [r, g, b, a * GHOST] }),
        Draw::Tex { x, y, w, h, tex, alpha } =>
            Some(Draw::Tex { x, y, w, h, tex, alpha: alpha * GHOST }),
        Draw::Turned { x, y, w, h, tex, alpha, turn } =>
            Some(Draw::Turned { x, y, w, h, tex, alpha: alpha * GHOST, turn }),
        Draw::Game | Draw::Shot { .. } | Draw::Ink(_) | Draw::Span(_) => None,
    }
}

/// Do not report an unsuccessful core reopen as "solo restored".
/// Always surface the genuine checkpoint error to the user: the durable
/// solo state and battery files remain on disk for safe cartridge relaunch.
/// Match the upstream Eject-first rule even if Android onPause occurs
/// outside the Multiplayer menu. Pure so its fullscreen branch cannot
/// silently regress when editing the lifecycle handling.
fn suspend_returns_to_eject_menu(requires_relaunch: bool,
                                 menu_is_multiplayer: bool) -> bool {
    requires_relaunch || menu_is_multiplayer
}

fn link_boot_failure(context: &str, error: &str,
                     recovery: Result<(), String>) -> String {
    match recovery {
        Ok(()) => {
            crate::game::log_launch_timing(&format!(
                "{context}: {error}; original solo progress restored"));
            "Couldn't start the link. Your game is back to solo.".into()
        }
        Err(cause) => {
            crate::game::log_launch_timing(&format!(
                "{context}: {error}; solo recovery failed: {cause}"));
            "Couldn't start the link. Eject and reinsert the cartridge; your saves are safe.".into()
        }
    }
}

// Keep the upstream centred heading and keycaps. The hints sit below the
// sockets on the console’s 58-pixel bar, including on narrow canvases.
const LINK_TEXT_Y: f32 = 44.0;
const LINK_LEGEND_Y: f32 = OUT_H as f32 - HINT_H as f32 - 11.0;
const LINK_LEGEND_GAP: f32 = 40.0;

fn link_heading_x(width: u32) -> f32 {
    ((out_w() as f32 - width as f32) / 2.0).round()
}

/// Room kept free each side of a legend.
const LEGEND_MARGIN: f32 = 8.0;

/// How wide a legend is drawn, as centred_hints lays it out.
fn legend_width(hints: &[(slot_gfx::TexId, u32)]) -> f32 {
    hints.iter().map(|&(_, w)| w.saturating_sub(HINT_EDGE) as f32).sum::<f32>()
        + LEGEND_GAP * hints.len().saturating_sub(1) as f32
}

/// A legend too wide for the screen: true, and a hint the user can do
/// without should go (Siham 2026-11-09).
fn legend_too_wide(hints: &[(slot_gfx::TexId, u32)]) -> bool {
    legend_width(hints) > out_w() as f32 - 2.0 * LEGEND_MARGIN
}

const LINK_LEGEND_MIN_GAP: f32 = 14.0;

/// The gap that fits the legend on screen with 12 px either side: the
/// original 40 px, or less, down to 14; None if even that is too wide.
fn link_legend_gap(hints: &[(slot_gfx::TexId, u32)]) -> Option<f32> {
    let gaps = hints.len().saturating_sub(1) as f32;
    let keys = hints.iter().map(|(_, w)| w.saturating_sub(HINT_EDGE) as f32).sum::<f32>();
    let room = out_w() as f32 - 24.0 - keys;
    if gaps == 0.0 { return (room >= 0.0).then_some(LINK_LEGEND_GAP); }
    let gap = (room / gaps).min(LINK_LEGEND_GAP).floor();
    (gap >= LINK_LEGEND_MIN_GAP).then_some(gap)
}

fn link_legend_start(hints: &[(slot_gfx::TexId, u32)], gap: f32) -> f32 {
    let total = hints.iter().map(|(_, w)| w.saturating_sub(HINT_EDGE) as f32)
        .sum::<f32>() + gap * hints.len().saturating_sub(1) as f32;
    ((out_w() as f32 - total) / 2.0).round()
}

/// Original Slot / fork draw_game_menu() picks exactly these groups of
/// bottom keycaps. Keep selection separate from the GL scene so changes
/// cannot accidentally show A Link while searching/linked or conceal
/// SELECT Mode on a real gpSP RFU/Cable switchable title.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum LinkLegendProfile {
    PickerWithMode, // B CANCEL | SELECT MODE | arrows SWAP | A LINK
    Picker,         // B CANCEL | arrows SWAP | A LINK
    Searching,      // B CANCEL
    ConnectedMenu,  // B BACK | A END LINK (only reopened overlay)
    Failed,         // A OK
    None,           // initial LINKED / animated Unplug
}

fn link_legend_profile(menu: LinkMenu, selectable_mode: bool) -> LinkLegendProfile {
    match menu {
        LinkMenu::Pick(_) if selectable_mode => LinkLegendProfile::PickerWithMode,
        LinkMenu::Pick(_) => LinkLegendProfile::Picker,
        LinkMenu::Working { .. } => LinkLegendProfile::Searching,
        LinkMenu::Linked { opened: true, .. } => LinkLegendProfile::ConnectedMenu,
        LinkMenu::Linked { opened: false, .. } | LinkMenu::Unplug { .. } =>
            LinkLegendProfile::None,
        LinkMenu::Failed { .. } => LinkLegendProfile::Failed,
    }
}

/// A Link Mode toggle must never lie about actual physical/serial support.
fn link_mode_can_switch(core: Core, chosen: LinkKind, code: &str,
                        title: &str, clean: bool) -> bool {
    if core != Core::Gpsp || !link_kind::link_carried(code, title)
        || !link_kind::has_wireless_adapter(code)
        || !link_kind::gpsp_runtime_mode_supported(code, chosen)
        || !link_kind::gpsp_runtime_mode_supported(code, chosen.other()) { return false; }
    let automatic = link_kind::link_kind(code, title, clean);
    let selected_serial = link_kind::serial_option(chosen, automatic, code, title);
    let opposite_serial = link_kind::serial_option(chosen.other(), automatic, code, title);
    selected_serial != opposite_serial
}

/// These rows are modelled by *action*, not by a hardcoded screen index.
/// It prevents a linked session from routing "Save & Eject" to "Resume".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PauseRow { Resume, States, Multiplayer, Cheats, Settings, SaveEject }
const SOLO_PAUSE_ROWS: &[PauseRow] = &[
    PauseRow::Resume, PauseRow::States, PauseRow::Multiplayer, PauseRow::Cheats,
    PauseRow::Settings, PauseRow::SaveEject,
];
const LINK_PAUSE_ROWS: &[PauseRow] = &[
    PauseRow::Resume, PauseRow::Multiplayer, PauseRow::Settings,
    PauseRow::SaveEject,
];
// If a Link core must be relaunched before solo, it must not expose
// unsafe actions or a misleading Resume control.
const RELAUNCH_PAUSE_ROWS: &[PauseRow] = &[
    PauseRow::SaveEject, PauseRow::Multiplayer,
];
/// A system with no Multiplayer at all shows no Multiplayer row (Siham
/// 2026-11-22): the Game Gear (no second pad, no Gear-to-Gear over the
/// network), the Neo Geo Pocket, the WonderSwan, the Lynx, the Pokemon mini.
const SOLO_ONLY_PAUSE_ROWS: &[PauseRow] = &[
    PauseRow::Resume, PauseRow::States, PauseRow::Cheats,
    PauseRow::Settings, PauseRow::SaveEject,
];
/// Neither Multiplayer nor Cheats: the Neo Geo Pocket, the Lynx, the
/// Pokemon mini and PICO-8, whose cores apply no cheats (3.2.0-beta11).
const PLAIN_PAUSE_ROWS: &[PauseRow] = &[
    PauseRow::Resume, PauseRow::States, PauseRow::Settings, PauseRow::SaveEject,
];
const SOLO_NO_CHEATS_PAUSE_ROWS: &[PauseRow] = &[
    PauseRow::Resume, PauseRow::States, PauseRow::Multiplayer,
    PauseRow::Settings, PauseRow::SaveEject,
];
fn multiplayer_offered(platform: Platform) -> bool {
    !matches!(platform, Platform::Gg | Platform::Ngp | Platform::Ws | Platform::Lynx | Platform::Pokemini
        | Platform::Pico8)
}
/// A core that plays Multiplayer: the home consoles' netplay cores and the
/// handhelds' Link cores. Not bsnes nor Gearsystem (3.2.0-beta11 audit).
fn core_has_multiplayer(core: Core) -> bool {
    core.netplay() || matches!(core, Core::Mgba | Core::Gpsp | Core::Gambatte)
}
/// Whether the paused game offers Cheats: not where its cores apply none
/// (NeoPop, RACE, Handy, Beetle Lynx, PokeMini, fake-08, PICO-8: their
/// retro_cheat_set does nothing), nor the Neo Geo and the PC Engine, which
/// have no cheat pack yet.
fn cheats_offered(platform: Platform) -> bool {
    !matches!(platform, Platform::Ngp | Platform::Lynx | Platform::Pokemini | Platform::Pico8
        | Platform::NeoGeo | Platform::Pce)
}
/// A system one of whose cores makes the handheld rumble (mGBA, gpSP,
/// Gambatte, PokeDaisy, PokeMini ask for rumble; the others never do).
fn rumble_offered(platform: Platform) -> bool {
    matches!(platform, Platform::Gb | Platform::Gbc | Platform::Gba | Platform::Pokemini)
}
/// A core that never runs ahead: bsnes (its first frame after a load),
/// PokeDaisy (its map past the edges), PICO-8's two (their states' size).
fn core_runs_ahead(core: Core) -> bool {
    !matches!(core, Core::Bsnes | Core::PokeDaisy | Core::Pico8 | Core::Fake08)
}
/// The paused game's Emulation page: rows and the Settings key of each,
/// one for one (3.2.0-beta9 drew Run-Ahead's value under two rows).
fn pause_emulation_page(core: Option<Core>, platform: Platform, solar: bool, motion: bool)
    -> (Vec<usize>, Vec<usize>) {
    let mut page = vec![(51, 10), (52, 11)];
    let ahead = core.map_or(true, core_runs_ahead) && platform != Platform::Pico8;
    if ahead { page.push((55, 12)); }
    if solar { page.push((119, SUN_KEY)); } else if motion { page.push((137, MOTION_KEY)); }
    page.into_iter().unzip()
}
/// The paused game's Gameplay page: no Rumble where no core rumbles, no
/// Rewind on the player's own PICO-8 (its 33 MB state).
fn pause_gameplay_page(core: Option<Core>, platform: Platform) -> (Vec<usize>, Vec<usize>) {
    GAMEPLAY_ROWS.into_iter().zip(GAMEPLAY_KEYS)
        .filter(|&(label, _)| label != 30 || rumble_offered(platform))
        .filter(|&(label, _)| label != 37 || core != Some(Core::Pico8))
        .unzip()
}
fn pause_rows(linked: bool, requires_relaunch: bool, offered: bool) -> &'static [PauseRow] {
    pause_rows_with(linked, requires_relaunch, offered, true)
}
/// The pause menu's rows, with Multiplayer (`offered`) and Cheats
/// (`cheats`) only where the game's core plays them.
fn pause_rows_with(linked: bool, requires_relaunch: bool, offered: bool, cheats: bool) -> &'static [PauseRow] {
    if requires_relaunch { RELAUNCH_PAUSE_ROWS }
    else if linked { LINK_PAUSE_ROWS }
    else if offered && cheats { SOLO_PAUSE_ROWS }
    else if offered { SOLO_NO_CHEATS_PAUSE_ROWS }
    else if cheats { SOLO_ONLY_PAUSE_ROWS }
    else { PLAIN_PAUSE_ROWS }
}
fn pause_row_text(row: PauseRow, requires_relaunch: bool) -> usize {
    match row {
        PauseRow::Resume => 20,
        PauseRow::States => 22,
        PauseRow::Multiplayer => 66, // Entry always opens status; End Link stays inside adapter
        PauseRow::Cheats => 134,
        PauseRow::Settings => 25,
        PauseRow::SaveEject if requires_relaunch => 78,
        PauseRow::SaveEject => 21,
    }
}

/// Ejection and resume are never inferred from their displayed row numbers.
#[cfg(test)]
fn game_menu_ejects(row: usize, linked: bool, requires_relaunch: bool) -> bool {
    matches!(pause_rows(linked, requires_relaunch, true).get(row), Some(PauseRow::SaveEject))
}

/// The original Slot HUD distinguishes a remote end and a BIOS mismatch.
fn link_fault_toast(fault: crate::link_play::LinkError) -> Toast {
    match fault {
        crate::link_play::LinkError::BiosMismatch => Toast::BiosMismatch,
        crate::link_play::LinkError::PeerEnded
        | crate::link_play::LinkError::Disconnected
        | crate::link_play::LinkError::InvalidState
        | crate::link_play::LinkError::Timeout => Toast::PeerEnded,
    }
}

/// Map the negotiated on-wire role to upstream Slot Link icon variants.
fn link_badge_for(role: LinkRow, disconnected: bool) -> LinkBadge {
    match (role, disconnected) {
        (LinkRow::Host, false) => LinkBadge::Hosting,
        (LinkRow::Join, false) => LinkBadge::Joined,
        (LinkRow::Host, true) => LinkBadge::HostingLost,
        (LinkRow::Join, true) => LinkBadge::JoinedLost,
    }
}

/// Only the initial "Linked" seating animation is self-dismissing.
/// Reopening from SELECT+HOME must keep End Link/Back visible until input.
fn link_menu_auto_closes(phase: LinkMenu, now: u64) -> bool {
    matches!(phase, LinkMenu::Linked { since, opened: false, .. }
        if now.saturating_sub(since) >= 1000)
}

/// Hold the screen through the cable or RFU release and its settling time.
fn link_unplug_complete(phase: LinkMenu, now: u64) -> bool {
    // Includes the RFU’s initial button press and the return to the free pose.
    matches!(phase, LinkMenu::Unplug { since, .. }
        if now.saturating_sub(since) >= 420)
}

/// A library refresh may never terminate an inserted or pending game.
fn may_apply_library_update(game_active: bool, inserted: bool, awaiting: bool) -> bool {
    !game_active && !inserted && !awaiting
}

/// A clean install has three shelves but zero cartridges. Never enqueue
/// nonexistent artwork for the first draw (or after clearing the library).
fn selected_prefetch_entry(shelves: &[Shelf], active: usize) -> Option<(usize, usize)> {
    let shelf = shelves.get(active)?;
    shelf.carts.get(shelf.index).map(|_| (active, shelf.index))
}

/// Restart on the exact platform + cart when possible; if a hidden/removed
/// Recents or Favorites rack no longer contains it, recover the source cart.
fn restore_saved_focus(shelves:&mut [Shelf], focus:&crate::selection::Focus)->Option<usize> {
    let candidate=shelves.get(focus.rack).and_then(|shelf|
        shelf.carts.iter().position(|cart|cart.rom.to_string_lossy()==focus.uri.as_str()));
    if let Some(index)=candidate {
        shelves[focus.rack].select(index);
        return Some(focus.rack);
    }
    for rack in (0..Platform::ALL.len()).chain(std::iter::once(VIDEO_RACK)) {
        if let Some(index)=shelves.get(rack).and_then(|shelf|
            shelf.carts.iter().position(|cart|cart.rom.to_string_lossy()==focus.uri.as_str())) {
            shelves[rack].select(index);
            return Some(rack);
        }
    }
    None
}

/// A rebuilt carousel (launch, update, scan) opens on the GBA shelf rather
/// than GBA Video behind it, when the GBA shelf has carts (Siham 2026-11-10).
pub(super) fn gba_before_video(shelves: &[Shelf], active: usize) -> usize {
    if active == VIDEO_RACK && shelves.first().is_some_and(|s| !s.carts.is_empty()) { 0 } else { active }
}

/// Preserve stable ROM identity and platform on a Cart Studio shelf rebuild.
/// Missing/removed games safely fall back to the first available cartridge.
fn restore_shelf_focus(shelves: &mut [Shelf], active: usize, uris: &[Option<PathBuf>]) -> usize {
    for (shelf, wanted) in shelves.iter_mut().zip(uris.iter()) {
        if let Some(path) = wanted {
            if let Some(index) = shelf.carts.iter().position(|cart| cart.rom == *path) {
                shelf.select(index);
            }
        }
    }
    if shelves.get(active).is_some_and(|shelf| !shelf.carts.is_empty()) {
        active
    } else {
        next_nonempty_shelf(shelves, active.min(shelves.len().saturating_sub(1)), 1)
    }
}

/// Slot. keeps the label timer alive even when fade-in alpha is initially zero.
/// Time-based expiry (not alpha-based) is essential for reliable GBA startup
/// and every L1/R1 GB/GBC/GBA switch on a high-refresh-rate Android panel.
/// Game Info's time played: 42:13 under an hour, 1:02:13 from one.
fn played_text(seconds: f32) -> String {
    let s = seconds.max(0.0) as u64;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h == 0 { format!("{m}:{s:02}") } else { format!("{h}:{m:02}:{s:02}") }
}

fn platform_name_running(elapsed_ms: u64) -> bool {
    elapsed_ms < 200 + 1200 + 800
}

/// Official upstream v1.7 name / title animation is full opacity (1.0).
/// Hide titles until 1400ms, then crossfade during the 800ms fade-out.
fn slot_title_alpha(elapsed_ms: u64) -> f32 {
    if elapsed_ms < 1400 { return 0.0; }
    (1.0 - slot_name_alpha(elapsed_ms)).clamp(0.0, 1.0)
}

fn slot_name_alpha(elapsed_ms: u64) -> f32 {
    let level = if elapsed_ms < 200 {
        elapsed_ms as f32 / 200.0
    } else if elapsed_ms < 1400 {
        1.0
    } else {
        (1.0 - (elapsed_ms - 1400) as f32 / 800.0).max(0.0)
    };
    slot_ui::ease(level)
}

/// Native About rows share one cursor. The third status line never receives focus.
/// The About sticker on screen (x, y, w, h): the canvas' width at most,
/// never above its own size (720 on the KONKR's 3:2, 480 on the RG Rotate).
fn about_sticker_box() -> (f32, f32, f32, f32) {
    let width = out_w() as f32;
    let sticker_w = about_sticker_width() as f32;
    let scale = ((width - 24.0) / sticker_w).min(1.0);
    let (w, h) = (sticker_w * scale, STICKER_H as f32 * scale);
    ((width - w) / 2.0, 30.0, w, h)
}

/// The About sticker's own width: the screen's, less its margins, from
/// slot_ui::STICKER_MIN_W to STICKER_W. A handheld's label is printed that
/// wide rather than shrunk (the KONKR's keeps its 660).
fn about_sticker_width() -> u32 {
    if is_konkr() { return STICKER_W; }
    (out_w().saturating_sub(24)).clamp(slot_ui::STICKER_MIN_W, STICKER_W)
}
/// The About rows start just under the sticker: where they always were on a
/// full-size sticker, higher as it shrinks (Siham 2026-11-15).
fn about_row_top() -> f32 {
    let (_, y, _, h) = about_sticker_box();
    (y + h + 14.0).min(270.0)
}

/// The KONKR keeps its own logos; every other handheld gets slot.'s
/// wordmark and its own name (Siham, 2026-11-04).
fn about_sticker(fields: &StickerFields) -> slot_ui::UndoFace {
    if is_konkr() {
        sticker_face_konkr(fields)
    } else {
        let (name, model) = DEVICE_NAME.lock().unwrap_or_else(|e| e.into_inner()).clone();
        slot_ui::sticker_face_handheld_w(fields, &name, &model, about_sticker_width())
    }
}

/// A verified downloaded release (READY=5) remains eligible for installation.
fn about_primary_action(status: u8) -> i32 {
    if matches!(status, 1 | 5) { 21 } else { 19 }
}

/// Physical D-pad UP and DOWN must move the selection in opposite
/// directions, including wraparound; never leak navigation to libretro.
/// Rows that L1 / R1 skip in a list menu: one screen (Siham 2026-11-15).
const MENU_PAGE_ROWS: usize = 6;

fn move_menu_row(row: usize, key: i32, count: usize, repeating: bool) -> usize {
    if count == 0 { return 0; }
    // L1 / R1: a screen up / down, stopping at the first and last rows (a
    // short page goes straight to them).
    match key {
        102 => return row.min(count - 1).saturating_sub(MENU_PAGE_ROWS),
        103 => return (row + MENU_PAGE_ROWS).min(count - 1),
        _ => {}
    }
    if repeating {
        return match key {
            19 => row.saturating_sub(1),
            20 => (row + 1).min(count - 1),
            _ => row.min(count - 1),
        };
    }
    match key {
        19 => (row + count - 1) % count,
        20 => (row + 1) % count,
        _ => row % count,
    }
}

/// Match Slot's centred menu row layout on the 720x480 design canvas.
/// Move draws down by `dy` (the slot rising with the in-game menu).
fn shift_down(draws: &mut [Draw], dy: f32) {
    if dy == 0.0 { return; }
    for draw in draws {
        match draw {
            Draw::Rect { y, .. } | Draw::Tex { y, .. } | Draw::Turned { y, .. } => *y += dy,
            Draw::Game | Draw::Shot { .. } | Draw::Ink(_) | Draw::Span(_) => {}
        }
    }
}

fn settings_row_top(rows: usize) -> f32 {
    ((OUT_H as f32 - QUICK_PITCH * rows as f32) / 2.0).round()
}

/// Same easing timeline for the shelf and CRT-composited exit frames.
/// Separating progress from rendering makes the animation durations testable.
fn advance_cart(progress: f32, inserted: bool, dt: f32) -> f32 {
    if inserted {
        (progress + dt / SEATED_AT).min(1.0)
    } else {
        (progress - dt / EJECT_S).max(0.0)
    }
}

/// Return the current shelf if it is the only populated one, and never
/// select an empty GB/GBC/GBA shelf. Safe even if all libraries are empty.
fn next_nonempty_shelf(shelves: &[Shelf], current: usize, delta: i32) -> usize {
    let mut counts: Vec<usize> = shelves.iter().map(|s| s.carts.len()).collect();
    // GBA Video sits behind the GBA shelf: never a stop of its own, the
    // GBA's place one even when only GBA Video has carts.
    let current = if current == VIDEO_RACK { 0 } else { current };
    if let Some(video) = counts.get_mut(VIDEO_RACK).map(std::mem::take) {
        if let Some(gba) = counts.first_mut() { *gba += video; }
    }
    crate::library::next_populated(current, delta, &counts)
}

/// The slot bar a cart goes into: its console's own for NES, Famicom, Super
/// Nintendo and Mega Drive carts. Custom theme colours keep the drawn bar
/// for every system.
/// The GBA's bar in `home_slot`'s order.
const GBA_SLOT: usize = 6;


/// A picture the cold renderer needs from its first frame, made off the GL
/// thread (beta41): the slot bars in both themes, every console's bars in
/// the three bar themes, the GB and GBC bezels, the carts' shadows. In
/// upload order.
enum StartJob { Band(&'static [u8]), Bezel(bool), Shadow(Option<GbShell>), HomeShadow(slot_ui::HomeMould) }

/// A StartJob's result, ready for the GPU.
enum StartPicture { Texture(Vec<u8>, u32, u32), Face(slot_ui::CartFace) }

impl StartJob {
    fn all() -> Vec<StartJob> {
        use slot_ui::bitmaps::{home_slot_png, slot_bar_png};
        let mut jobs = Vec::new();
        for light in [false, true] {
            let (back, front) = slot_bar_png(light);
            jobs.extend([StartJob::Band(back), StartJob::Band(front)]);
        }
        for theme in 0..3 {
            for family in 0..slot_ui::bitmaps::CONSOLE_SLOTS {
                let (back, front) = home_slot_png(family, theme);
                jobs.extend([StartJob::Band(back), StartJob::Band(front)]);
            }
        }
        jobs.extend([StartJob::Bezel(false), StartJob::Bezel(true)]);
        jobs.extend([StartJob::Shadow(None), StartJob::Shadow(Some(GbShell::Notched)),
            StartJob::Shadow(Some(GbShell::Rounded))]);
        jobs.extend(slot_ui::HomeMould::ALL.map(StartJob::HomeShadow));
        jobs
    }

    fn make(&self) -> StartPicture {
        match *self {
            StartJob::Band(png) => {
                let (rgba, w, h) = slot_ui::native_png(
                    png, slot_ui::bitmaps::SLOT_W, slot_ui::bitmaps::SLOT_H);
                let wide = (w as f32 * out_w() as f32 / slot_ui::bitmaps::SLOT_W as f32)
                    .round() as u32;
                StartPicture::Texture(slot_ui::refit_band(&rgba, w, h, wide), wide, h)
            }
            StartJob::Bezel(gbc) => {
                let (rgba, w, h) = slot_ui::native_png(slot_ui::bitmaps::bezel_master(gbc), 720, 480);
                StartPicture::Texture(rgba, w, h)
            }
            StartJob::Shadow(None) => StartPicture::Face(cart_shadow()),
            StartJob::Shadow(Some(shell)) => StartPicture::Face(gb_cart_shadow(shell)),
            StartJob::HomeShadow(mould) => StartPicture::Face(slot_ui::home_cart_shadow(mould)),
        }
    }
}

impl StartJob {
    /// Its cache name: what it is drawn from and at what size. Only the
    /// pictures decoded from a PNG are kept (the shadows are quick).
    fn cache_key(&self, scale: f32) -> Option<String> {
        // The shadows are drawn by code, not from a PNG: kept per build (the
        // CI's commit), so a changed shadow is never read from an old cache.
        let build = option_env!("GITHUB_SHA").unwrap_or("dev");
        let shadow = match *self {
            StartJob::Shadow(shell) => format!("shadow {shell:?} {build}"),
            StartJob::HomeShadow(mould) => format!("home shadow {mould:?} {build}"),
            _ => String::new(),
        };
        let (png, what): (&[u8], &str) = match *self {
            StartJob::Band(png) => (png, "band"),
            StartJob::Bezel(gbc) => (slot_ui::bitmaps::bezel_master(gbc), "bezel"),
            StartJob::Shadow(_) | StartJob::HomeShadow(_) => (&[], shadow.as_str()),
        };
        if build == "dev" && png.is_empty() { return None; }
        let mut input = format!("slot-start-v1:{what}:{}:{}:", scale.to_bits(), out_w()).into_bytes();
        input.extend_from_slice(png);
        let hash = ring::digest::digest(&ring::digest::SHA256, &input);
        Some(hash.as_ref().iter().map(|b| format!("{b:02x}")).collect())
    }

    /// From the cache when drawn there before at this size (beta42: the
    /// KONKR spent 2.2 s on these on every start), else made and kept.
    fn make_cached(&self, cache: Option<&crate::cart_disk_cache::DiskCache>, scale: f32) -> StartPicture {
        let key = cache.and(self.cache_key(scale));
        let face_kind = matches!(self, StartJob::Shadow(_) | StartJob::HomeShadow(_));
        if let (Some(cache), Some(key)) = (cache, key.as_deref()) {
            if let Some(face) = cache.read(key) {
                return if face_kind { StartPicture::Face(face) }
                    else { StartPicture::Texture(face.rgba, face.w, face.h) };
            }
        }
        let picture = self.make();
        if let (Some(cache), Some(key)) = (cache, key.as_deref()) {
            let _ = match &picture {
                StartPicture::Texture(rgba, w, h) =>
                    cache.write(key, &slot_ui::CartFace { rgba: rgba.clone(), w: *w, h: *h }),
                StartPicture::Face(face) => cache.write(key, face),
            };
        }
        picture
    }
}

/// Makes StartJob::all() on up to four threads; joined once GL is up.
fn start_pixels() -> std::thread::JoinHandle<Vec<StartPicture>> {
    let scale = slot_ui::density::scale();
    let cache = PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        .map(|(root, _)| crate::cart_disk_cache::DiskCache::at(root.join("Cache").join("start-pictures")));
    std::thread::spawn(move || {
        let cache = cache.as_ref();
        let jobs = StartJob::all();
        let n = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 4);
        let mut out: Vec<Option<StartPicture>> = (0..jobs.len()).map(|_| None).collect();
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..n).map(|k| {
                let jobs = &jobs;
                scope.spawn(move || slot_ui::density::with_scale(scale, || {
                    jobs.iter().enumerate().skip(k).step_by(n)
                        .map(|(i, job)| (i, job.make_cached(cache, scale))).collect::<Vec<_>>()
                }))
            }).collect();
            for worker in workers {
                for (i, picture) in worker.join().expect("a start picture worker") { out[i] = Some(picture); }
            }
        });
        out.into_iter().map(|p| p.expect("every start picture made")).collect()
    })
}

fn pick_slot_faces(
    faces: Option<(slot_gfx::TexId, slot_gfx::TexId)>,
    home: [(slot_gfx::TexId, slot_gfx::TexId); slot_ui::bitmaps::CONSOLE_SLOTS],
    cart: Option<&slot_store::Cart>,
) -> Option<(slot_gfx::TexId, slot_gfx::TexId)> {
    faces?;
    // No cart yet (a fresh install): the GBA's bar, in the chosen theme
    // (Siham 2026-11-09). The old plain bar is no longer shown.
    Some(home[cart.map_or(GBA_SLOT, slot_ui::bitmaps::console_slot_family)])
}

/// What closes the slot a cart goes into: the NES flap, the Super Nintendo's
/// flaps, the Mega Drive's doors, the Famicom's open slot. None for the GBA /
/// Game Boy bar and for custom theme colours.
fn pick_closer(
    faces: Option<(slot_gfx::TexId, slot_gfx::TexId)>,
    closers: [Option<slot_ui::SlotCloser>; 4],
    cart: Option<&slot_store::Cart>,
) -> Option<slot_ui::SlotCloser> {
    faces?;
    closers.get(cart.and_then(slot_ui::bitmaps::home_slot_family)?).copied().flatten()
}

/// The screen columns the core picker covers at `progress`.
fn picker_span(shelf: &Shelf, progress: f32) -> [f32; 2] {
    let (rest, scale) = shelf.selected_at();
    let body = shelf.carts.get(shelf.index).map_or(slot_ui::Body::Gba, slot_ui::body_of);
    slot_ui::picker_span_body(body, slot_ui::shelf_cart_body(body, rest, scale), progress)
}

#[cfg(test)]
mod ui_feedback_tests;
#[cfg(test)]
mod settings_pages_tests;

#[cfg(test)]
mod link_hardware_tests;

mod jni_exports;
mod controls_ui;
mod menu_look;
mod cheats_ui;
mod cart_studio;
mod cart_library;
mod romm;
mod carousel_delete;
mod touch;
mod pokedaisy_hud;
mod shelves_grid;
use carousel_delete::VIDEO_RACK;
#[cfg(all(test, target_os = "linux"))]
pub(crate) use jni_exports::exercise_session_graphics_recovery;

/// Every row of every Settings and game menu shows its value, on every
/// screen shape and for every choice the row cycles through. A key missing
/// from setting_face would draw a blank value (the NES/SNES/MD Shader row,
/// 2026-11-04, after the Default Core rows on 2026-11-02).
#[cfg(test)]
mod texture_pool_tests {
    use super::cyclic_distance;

    #[test]
    fn the_first_carts_left_neighbour_is_the_last_cart() {
        assert_eq!(cyclic_distance(0, 9, 10), 1);
        assert_eq!(cyclic_distance(9, 0, 10), 1);
        assert_eq!(cyclic_distance(0, 5, 10), 5);
        assert_eq!(cyclic_distance(3, 4, 10), 1);
        assert_eq!(cyclic_distance(0, 0, 1), 0);
        assert_eq!(cyclic_distance(0, 0, 0), 0);
    }
}

#[cfg(test)]
mod settings_value_tests {
    use super::*;

    #[test]
    fn lcd_effect_follows_shader_on_every_new_lcd_handheld_and_panel() {
        for (platform, shader) in [(Platform::Gg, SHADER_GG_KEY), (Platform::Ngp, SHADER_NGP_KEY),
            (Platform::Ws, SHADER_WS_KEY), (Platform::Lynx, SHADER_LYNX_KEY)] {
            for panel in PANELS {
                for in_game in [false, true] {
                    let (base_rows, base_keys) = if in_game {
                        (Engine::screen_rows_on(platform, panel), Engine::screen_keys_on(platform, panel))
                    } else {
                        (Engine::system_rows_on(platform, panel), Engine::system_keys_on(platform, panel))
                    };
                    let (mut rows, mut keys) = (base_rows.to_vec(), base_keys.to_vec());
                    let (look_rows, look_keys) = Engine::look_rows(platform);
                    rows.extend(look_rows); keys.extend(look_keys);
                    let (extra_rows, extra_keys) = Engine::extra_rows(platform, in_game);
                    rows.extend(extra_rows); keys.extend(extra_keys);
                    Engine::palette_after_shader(&mut rows, &mut keys);
                    Engine::lcd_after_shader(platform, &mut rows, &mut keys);
                    assert_eq!(rows.len(), keys.len());
                    let at = keys.iter().position(|&k| k == shader).unwrap() + 1;
                    let lcd = Settings::lcd_effect_key(platform).unwrap();
                    assert_eq!(keys[at], lcd, "{platform:?} {panel:?}");
                    assert_eq!(MENU_TEXT[rows[at]], "LCD Effect");
                    assert_eq!(keys.iter().filter(|&&k| k == lcd).count(), 1);
                    if platform == Platform::Ws {
                        assert_eq!(keys[at + 1], crate::settings::WS_PALETTE_KEY);
                    }
                }
            }
        }
        for platform in [Platform::Sms, Platform::Nes, Platform::Snes, Platform::Md, Platform::Pokemini] {
            assert!(!Engine::look_rows(platform).1.iter().any(|&k| Settings::lcd_effect_platform(k).is_some()));
        }
    }

    #[test]
    fn pokemini_frame_blending_comes_second_to_last() {
        let keys = Engine::system_page_keys(Platform::Pokemini);
        let at = |key| keys.iter().position(|&k| k == key).unwrap();
        let blend = at(Settings::blending_key(Platform::Pokemini));
        assert_eq!(at(crate::settings::POKEMINI_LCD_MODE_KEY) + 1, blend);
        assert_eq!(blend + 1, at(crate::settings::POKEMINI_SHAKE_KEY));
    }

    #[test]
    fn the_nes_page_offers_hide_side_bars() {
        let keys = Engine::system_page_keys(Platform::Nes);
        let at = keys.iter().position(|&k| k == crate::settings::LEFT_BAR_NES_KEY).unwrap();
        assert_eq!(MENU_TEXT[Engine::system_page_rows(Platform::Nes)[at]], "Hide Side Bars");
        let mut extras = crate::settings::CoreExtras::default();
        extras.hide_left_bar_nes = true;
        for (core, prefix, filter) in [
            (Core::Fceumm, "fceumm", "fceumm_sndlowpass"),
            (Core::Nestopia, "nestopia", "nestopia_audio_filter"),
        ] {
            let options = extras.options(Platform::Nes, core);
            assert_eq!(options.len(), 3, "{core:?}: both side bars and audio filter");
            for side in ["h_left", "h_right"] {
                let key = format!("{prefix}_overscan_{side}");
                assert_eq!(options.iter().find(|(name, _)| *name == key).map(|(_, value)| *value),
                    Some("8"), "{core:?}: {key}");
            }
            assert_eq!(options.iter().find(|(name, _)| *name == filter).map(|(_, value)| *value),
                Some("disabled"), "{core:?}: audio OFF by default");
            extras.nes_audio_filter = true;
            assert_eq!(extras.options(Platform::Nes, core).iter()
                .find(|(name, _)| *name == filter).map(|(_, value)| *value),
                Some("enabled"), "{core:?}: audio filter ON");
            extras.nes_audio_filter = false;
        }
    }

    #[test]
    fn a_games_own_option_shows_its_own_value() {
        use crate::settings::*;
        let system = Settings::default();
        let mut game = system.clone();
        game.hide_left_bar_sms = !system.hide_left_bar_sms;
        game.no_sprite_limit_gg = !system.no_sprite_limit_gg;
        game.ws_palette = 2;
        game.pokemini_no_shake = !system.pokemini_no_shake;
        let profile = EmulationProfile::from_settings(&system);
        for (key, cart) in [(LEFT_BAR_SMS_KEY, Platform::Sms), (SPRITE_LIMIT_GG_KEY, Platform::Gg),
            (WS_PALETTE_KEY, Platform::Ws), (POKEMINI_SHAKE_KEY, Platform::Pokemini)] {
            let shown = |scoped: &Settings| setting_face(key, &system, scoped, &profile, cart,
                VideoMode::Aspect, &[]);
            assert_ne!(shown(&game), shown(&system), "key {key}: the game's own value");
        }
    }

    #[test]
    fn each_system_page_is_titled_with_its_own_name() {
        // Reported on 2.0.0: NES showed "Screen Size", SNES "NES" and Mega
        // Drive "Super Nintendo".
        for platform in Platform::ALL {
            assert_eq!(MENU_TEXT[Engine::screen_title(platform)], platform.name(),
                "Settings > Systems > {platform:?} heading");
        }
        for (platform, title) in SYSTEM_ORDER.iter().zip(SYSTEM_TITLES) {
            assert_eq!(MENU_TEXT[title], platform.name(), "Systems list row");
        }
    }

    #[test]
    fn each_system_page_changes_only_its_own_system() {
        // Default core, shader, colour/palette and overlay keys of each page.
        let own: [(Platform, &[usize]); 6] = [
            (Platform::Gba, &[29, 0, 22, 25]),
            (Platform::Gbc, &[30, 21, 23, 26]),
            (Platform::Gb, &[31, 1, 3, 27]),
            (Platform::Nes, &[41, 47, 44]),
            (Platform::Snes, &[42, 48, 45]),
            (Platform::Md, &[43, 49, 46]),
        ];
        for (platform, keys) in own {
            for panel in PANELS {
                for key in Engine::system_keys_on(platform, panel) {
                    assert!(*key == SCREEN_SIZE_KEY || keys.contains(key),
                        "Settings > Systems > {platform:?} on {panel:?} shows key {key}");
                    if let Some(core) = Engine::core_key_platform(*key) {
                        assert_eq!(core, platform, "default core row of {platform:?}");
                    }
                }
            }
        }
    }

    const PANELS: [(u32, u32); 7] = [(960, 640), (1240, 1080), (1920, 1080),
        (720, 720), (1024, 768), (1280, 720), (1920, 1152)];

    #[test]
    fn every_system_has_its_settings_page_on_every_screen() {
        // The screens slot. ships on (systems_table_tests::SCREENS).
        assert_eq!(PANELS, crate::systems_table_tests::SCREENS.map(|(_, panel)| panel));
        let mut order = SYSTEM_ORDER.to_vec();
        order.sort_by_key(|p| *p as usize);
        assert_eq!(order, Platform::ALL, "Settings > Systems lists every system once");
        for platform in Platform::ALL {
            for panel in PANELS {
                // The Pokemon mini's rows may all be its own options.
                assert!(!Engine::system_keys_on(platform, panel).is_empty()
                    || !Engine::extra_rows(platform, false).1.is_empty(), "{platform:?} on {panel:?}");
                assert!(!Engine::screen_keys_on(platform, panel).is_empty()
                    || !Engine::extra_rows(platform, true).1.is_empty(), "{platform:?} game menu on {panel:?}");
            }
        }
    }

    fn menu_keys() -> Vec<(String, usize, Platform)> {
        let mut keys = Vec::new();
        for platform in Platform::ALL {
            for panel in PANELS {
                let system = (Engine::system_rows_on(platform, panel),
                    Engine::system_keys_on(platform, panel));
                let screen = (Engine::screen_rows_on(platform, panel),
                    Engine::screen_keys_on(platform, panel));
                assert_eq!(system.0.len(), system.1.len(),
                    "Settings > Systems > {platform:?} on {panel:?}: rows and values differ");
                assert_eq!(screen.0.len(), screen.1.len(),
                    "{platform:?} game menu Screen on {panel:?}: rows and values differ");
                for key in system.1 {
                    keys.push((format!("Settings > Systems > {platform:?} on {panel:?}"), *key, platform));
                }
                for key in screen.1 {
                    keys.push((format!("{platform:?} game menu > Screen on {panel:?}"), *key, platform));
                }
            }
            let (look_rows, look_keys) = Engine::look_rows(platform);
            assert_eq!(look_rows.len(), look_keys.len());
            for key in look_keys {
                keys.push((format!("{platform:?} LCD Effect / Frame Blending"), key, platform));
            }
            // The 3.0 systems' own options, each with a label and a value.
            for in_game in [false, true] {
                let (rows, extras) = Engine::extra_rows(platform, in_game);
                assert_eq!(rows.len(), extras.len(), "{platform:?}: own options' rows and values differ");
                assert!(rows.iter().all(|&row| row < MENU_TEXT.len()), "{platform:?}: own option label");
                for key in extras {
                    keys.push((format!("{platform:?} own options (in game {in_game})"), key, platform));
                }
            }
            for key in GAMEPLAY_KEYS {
                keys.push(("Gameplay".into(), key, platform));
            }
            for key in INTERFACE_KEYS.into_iter().chain([10, 11, 12]).chain(16..=20)
                // 85 is Android's "Lock the screen" right, not a setting.
                .chain(ROTATE_KEYS.into_iter().filter(|&key| key != 85)) {
                keys.push(("Interface/Emulation/General".into(), key, platform));
            }
        }
        // A GBA Video cart's Screen: Shader, Upscaler, Color Correction.
        for key in [0, 70, 22] {
            keys.push(("GBA Video game menu > Screen".into(), key, Platform::Gba));
        }
        assert_eq!(GAMEPLAY_ROWS.len(), GAMEPLAY_KEYS.len());
        assert_eq!(INTERFACE_ROWS.len(), INTERFACE_KEYS.len());
        keys
    }

    fn face(settings: &Settings, key: usize, cart: Platform, video: VideoMode) -> Option<usize> {
        setting_face(key, settings, settings, &EmulationProfile::from_settings(settings),
            cart, video, &Platform::ALL.map(Core::default_for))
    }

    #[test]
    fn home_consoles_show_vignette_and_curvature_under_their_shader() {
        for platform in crate::settings::CRT_PLATFORMS {
            let (rows, keys) = Engine::system_page(platform);
            let shader = keys.iter().position(|&k| Some(k) == Settings::home_shader_key(platform)).unwrap();
            assert_eq!(&rows[shader + 1..shader + 3], &[187, 188], "{platform:?}: {rows:?}");
            assert_eq!(keys[shader + 1], Settings::crt_key(platform, false).unwrap());
            assert_eq!(keys[shader + 2], Settings::crt_key(platform, true).unwrap());
        }
        let (rows, _) = Engine::system_page(Platform::Gba);
        assert!(!rows.contains(&187) && !rows.contains(&188));
        let (rows, keys) = Engine::system_page(Platform::NeoGeo);
        assert!(rows.contains(&185) && keys.contains(&crate::settings::NEOGEO_REGION_KEY));
        let (rows, keys) = Engine::system_page(Platform::Pce);
        assert!(rows.contains(&186) && keys.contains(&crate::settings::PCE_PAD_KEY));
        let s = Settings::default();
        assert_eq!(SETTING_VALUE_NAMES[face(&s, crate::settings::NEOGEO_REGION_KEY, Platform::NeoGeo, VideoMode::Aspect).unwrap()], "EUROPE");
        assert_eq!(SETTING_VALUE_NAMES[face(&s, crate::settings::PCE_PAD_KEY, Platform::Pce, VideoMode::Aspect).unwrap()], "2 BUTTONS");
        assert_eq!(SETTING_VALUE_NAMES[face(&s, crate::settings::VIGNETTE_NES_KEY, Platform::Nes, VideoMode::Aspect).unwrap()], "OFF");
    }

    #[test]
    fn every_settings_row_shows_its_value() {
        for (menu, key, platform) in menu_keys() {
            let mut settings = Settings::default();
            let mut seen = std::collections::BTreeSet::new();
            // Walk every choice the row offers, both ways.
            for step in 0..40 {
                seen.insert(face(&settings, key, platform, VideoMode::Aspect));
                for video in [VideoMode::Actual, VideoMode::Aspect, VideoMode::Stretch] {
                    let value = face(&settings, key, platform, video).unwrap_or_else(||
                        panic!("{menu}: key {key} shows no value"));
                    assert!(value < SETTING_VALUE_NAMES.len(),
                        "{menu}: key {key} points at value {value}, past the names");
                }
                settings.change(key, step < 20);
            }
            // Screen Size and Default Core keep their own files, and General
            // (16-20) changes through cycle_general_row, not Settings::change.
            let own_file = key == SCREEN_SIZE_KEY || Engine::core_key_platform(key).is_some()
                || (16..=20).contains(&key);
            assert!(own_file || seen.len() > 1, "{menu}: key {key} never changes its value");
        }
    }

    #[test]
    fn every_core_and_bezel_has_a_name() {
        for core in Core::ALL {
            assert!(core_value(core) < SETTING_VALUE_NAMES.len(), "{core:?}");
            if matches!(core, Core::Fake08 | Core::Pico8) {
                assert_eq!(SETTING_VALUE_NAMES[core_value(core)], core.text(), "{core:?}");
            }
        }
        use crate::settings::Bezel;
        for bezel in Bezel::GB.into_iter().chain(Bezel::GBC).chain(Bezel::GBA)
            .chain(Bezel::NES).chain(Bezel::SNES).chain(Bezel::MD)
            .chain(Bezel::SMS).chain(Bezel::GG).chain(Bezel::NGP).chain(Bezel::WS)
            .chain(Bezel::LYNX).chain(Bezel::POKEMINI) {
            assert!(bezel_value(bezel) < SETTING_VALUE_NAMES.len(), "{bezel:?}");
        }
        // slot.'s own: each its own name, AUTO for the two that follow the game.
        assert_eq!(SETTING_VALUE_NAMES[bezel_value(Bezel::NgpAuto)], "AUTO");
        assert_eq!(SETTING_VALUE_NAMES[bezel_value(Bezel::WsColor)], "WONDERSWAN COLOR");
        assert_eq!(SETTING_VALUE_NAMES[bezel_value(Bezel::PokemonMini)], "POKEMON MINI");
        assert_eq!(SETTING_VALUE_NAMES[ROTATE_VALUES], "INSERTED CART");
    }
}

#[cfg(all(test, target_os = "linux"))]
mod library_update_tests {
    use super::*;

    fn entries(names: &[&str]) -> Vec<RomEntry> {
        let json: Vec<String> = names.iter().map(|n| format!(
            r#"{{"title":"{n}","platform":"GBA","uri":"content://roms/{n}.gba","code":"A{n}E"}}"#)).collect();
        serde_json::from_str(&format!("[{}]", json.join(","))).unwrap()
    }

    #[test]
    fn new_roms_join_the_shelves_in_place_and_the_same_roms_rebuild() {
        // The same ROMs with another title: everything is built again.
        let renamed = |names: &[&str], title: &str| {
            let mut list = entries(names);
            list[0].title = title.into();
            list
        };
        let _surface = slot_gfx::TestEgl::new();
        let mut engine = Engine::new(1, Some(&entries(&["AA", "CC"]))).unwrap();
        let gba = Platform::Gba as usize;
        assert_eq!(engine.shelves[gba].carts.len(), 2);
        engine.shelves[gba].select(1);
        assert!(engine.update_library(2, Some(&entries(&["AA", "BB", "CC"]))));
        assert_eq!(engine.library_version, 2);
        let stems: Vec<&str> = engine.shelves[gba].carts.iter().map(|c| c.stem.as_str()).collect();
        assert_eq!(stems.len(), 3);
        assert_eq!(engine.shelves[gba].carts[engine.shelves[gba].index].rom,
                   PathBuf::from("content://roms/CC.gba"), "the selected cart stays selected");
        assert!(engine.update_library(3, Some(&entries(&["CC", "BB", "AA"]))),
                "the very same carts sent again: nothing to rebuild");
        assert_eq!(engine.library_version, 3);
        assert!(!engine.update_library(4, Some(&renamed(&["CC", "BB", "AA"], "Other"))),
                "the same ROMs with another title build everything again");
    }
}


#[cfg(test)]
mod audio_and_refresh_parity_tests {
    use super::*;

    #[test]
    fn audio_options_appear_only_in_system_settings_not_paused_game() {
        use crate::settings::{NES_AUDIO_FILTER_KEY, MD_FM_SOUND_KEY, WS_AUDIO_FILTER_KEY};
        for (platform, key) in [
            (Platform::Nes, NES_AUDIO_FILTER_KEY),
            (Platform::Md, MD_FM_SOUND_KEY),
            (Platform::Ws, WS_AUDIO_FILTER_KEY),
        ] {
            assert!(Engine::system_page_keys(platform).contains(&key));
            assert!(Engine::extra_rows(platform, false).1.contains(&key));
            assert!(!Engine::extra_rows(platform, true).1.contains(&key));
        }
    }

    #[test]
    fn audio_values_preserve_rotate_value_offsets() {
        assert_eq!(MENU_TEXT[176], "Audio Filter");
        assert_eq!(SETTING_VALUE_NAMES[AUDIO_OPTION_VALUES], "RF");
        assert_eq!(SETTING_VALUE_NAMES[AUDIO_OPTION_VALUES + 1], "Soft");
        assert_eq!(SETTING_VALUE_NAMES[AUDIO_OPTION_VALUES + 2], "MAME");
        assert_eq!(SETTING_VALUE_NAMES[AUDIO_OPTION_VALUES + 3], "Nuked");
        assert_eq!(SETTING_VALUE_NAMES[AUDIO_OPTION_VALUES + 4], "Analog");
        assert_eq!(SETTING_VALUE_NAMES[ROTATE_VALUES], "INSERTED CART");
    }

    #[test]
    fn high_refresh_native_only_on_exact_native_refresh_multiples() {
        // KPA 60 Hz, 120 Hz panels: 60 Hz pulldown avoids 72/75 fps throttling.
        for hz in [60.0, 120.0] {
            assert!(handheld_core_60hz_for_display(72.0, hz));
            assert!(handheld_core_60hz_for_display(75.47, hz));
        }
        // A rare native-refresh panel should keep original timing.
        assert!(!handheld_core_60hz_for_display(72.0, 72.0));
        assert!(!handheld_core_60hz_for_display(72.0, 144.0));
        assert!(!handheld_core_60hz_for_display(75.47, 75.47));
        assert!(!handheld_core_60hz_for_display(75.47, 150.0));
        assert!(handheld_core_60hz_for_display(72.0, 0.0));
    }
}
