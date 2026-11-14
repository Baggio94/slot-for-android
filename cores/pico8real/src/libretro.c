// libretro front of the real PICO-8: libpico8_libretro.so.
//
// PICO-8 itself is not in this library: it is the player's own copy of the
// Raspberry Pi build (pico8_64 and pico8.dat), found in the system (BIOS)
// directory as pico-8/pico8_64 and pico-8/pico8.dat. A cart named
// splore.p8 (or the core option pico8_boot=splore) opens Splore instead of a cart.
#include "p8.h"
#include <time.h>

#include <stdarg.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#define RETRO_API_VERSION 1
#define ENV_GET_SYSTEM_DIRECTORY 9
#define ENV_SET_PIXEL_FORMAT 10
#define ENV_GET_LOG_INTERFACE 27
#define ENV_GET_SAVE_DIRECTORY 31
#define ENV_GET_INPUT_BITMASKS (51 | 0x10000)
#define ENV_GET_VARIABLE 15
struct retro_variable { const char *key, *value; };
#define PIXEL_FORMAT_XRGB8888 1
#define DEVICE_JOYPAD 1
#define JOYPAD_MASK 256
// slot.'s own environment call: an HTTP GET/POST to a file, for Splore.
#define ENV_SLOT_GET_HTTP (0x20000 | 0x5038)

struct retro_system_info { const char *library_name, *library_version, *valid_extensions; bool need_fullpath, block_extract; };
struct retro_game_geometry { unsigned base_width, base_height, max_width, max_height; float aspect_ratio; };
struct retro_system_timing { double fps, sample_rate; };
struct retro_system_av_info { struct retro_game_geometry geometry; struct retro_system_timing timing; };
struct retro_game_info { const char *path; const void *data; size_t size; const char *meta; };
typedef void (*retro_log_printf_t)(int level, const char *fmt, ...);
struct retro_log_callback { retro_log_printf_t log; };
struct slot_http { int (*get)(const char *url, const char *post_path, const char *out_path); };

typedef bool (*retro_environment_t)(unsigned cmd, void *data);
typedef void (*retro_video_refresh_t)(const void *data, unsigned w, unsigned h, size_t pitch);
typedef void (*retro_audio_sample_t)(int16_t l, int16_t r);
typedef size_t (*retro_audio_sample_batch_t)(const int16_t *data, size_t frames);
typedef void (*retro_input_poll_t)(void);
typedef int16_t (*retro_input_state_t)(unsigned port, unsigned device, unsigned index, unsigned id);

static retro_environment_t env;
static retro_video_refresh_t video;
static retro_audio_sample_batch_t audio_batch;
static retro_input_poll_t input_poll;
static retro_input_state_t input_state;
static retro_log_printf_t logp;
static struct slot_http http;
static bool bitmasks;

static uint32_t screen[128 * 128];
static double host_ms;
static double audio_owed;
static int reset_frames;
static bool running;

#ifdef __ANDROID__
#include <android/log.h>
#endif
// The core's own log beside PICO-8's log.txt: frontends may drop theirs.
static FILE *log_file;

static void log_line(const char *s) {
    if (log_file) { fprintf(log_file, "%s\n", s); fflush(log_file); }
#ifdef __ANDROID__
    __android_log_write(ANDROID_LOG_INFO, "pico8", s);
#endif
    if (logp) logp(1, "%s\n", s);
    else fprintf(stderr, "%s\n", s);
}

static int http_get(const char *url, const char *post, const char *out) {
    if (!http.get) { p8_logf("pico8: no HTTP in this frontend: %s", url); return -1; }
    return http.get(url, post, out);
}

unsigned retro_api_version(void) { return RETRO_API_VERSION; }
void retro_set_environment(retro_environment_t cb) {
    env = cb;
    struct retro_log_callback l = {0};
    if (env(ENV_GET_LOG_INTERFACE, &l)) logp = l.log;
}
void retro_set_video_refresh(retro_video_refresh_t cb) { video = cb; }
void retro_set_audio_sample(retro_audio_sample_t cb) { (void)cb; }
void retro_set_audio_sample_batch(retro_audio_sample_batch_t cb) { audio_batch = cb; }
void retro_set_input_poll(retro_input_poll_t cb) { input_poll = cb; }
void retro_set_input_state(retro_input_state_t cb) { input_state = cb; }
void retro_set_controller_port_device(unsigned port, unsigned device) { (void)port; (void)device; }

void retro_init(void) {
    p8_log = log_line;
    p8_http_get = http_get;
}
void retro_deinit(void) {}

void retro_get_system_info(struct retro_system_info *info) {
    memset(info, 0, sizeof *info);
    info->library_name = "PICO-8";
    info->library_version = "0.2.7";
    info->valid_extensions = "p8|png";
    info->need_fullpath = true;
    info->block_extract = true;
}

void retro_get_system_av_info(struct retro_system_av_info *info) {
    memset(info, 0, sizeof *info);
    info->geometry = (struct retro_game_geometry){128, 128, 128, 128, 1.0f};
    info->timing.fps = 60.0;
    info->timing.sample_rate = p8_audio_rate() ? p8_audio_rate() : 22050;
}

static int exists(const char *p) { struct stat st; return !stat(p, &st) && S_ISREG(st.st_mode); }

static int find_pico8(const char *sys, char *exe, size_t n) {
    static const char *const where[] = {"%s/pico-8/pico8_64", "%s/pico8_64"};
    for (size_t i = 0; i < 2; i++) {
        snprintf(exe, n, where[i], sys);
        char dat[600];
        snprintf(dat, sizeof dat, "%.*s/pico8.dat", (int)(strrchr(exe, '/') - exe), exe);
        if (exists(exe) && exists(dat)) return 0;
    }
    return -1;
}

static void mkdirs(const char *path) {
    char p[600];
    snprintf(p, sizeof p, "%s", path);
    for (char *s = p + 1; *s; s++)
        if (*s == '/') { *s = 0; mkdir(p, 0755); *s = '/'; }
    mkdir(p, 0755);
}

bool retro_load_game(const struct retro_game_info *game) {
    const char *sys = NULL, *save = NULL;
    env(ENV_GET_SYSTEM_DIRECTORY, &sys);
    env(ENV_GET_SAVE_DIRECTORY, &save);
    if (!env(ENV_SLOT_GET_HTTP, &http)) http.get = NULL;
    bitmasks = env(ENV_GET_INPUT_BITMASKS, NULL);
    int fmt = PIXEL_FORMAT_XRGB8888;
    if (!env(ENV_SET_PIXEL_FORMAT, &fmt)) return false;
    if (!sys) { p8_logf("pico8: no system directory"); return false; }
    char exe[600];
    if (find_pico8(sys, exe, sizeof exe)) {
        p8_logf("pico8: PICO-8 is not installed: put pico8_64 and pico8.dat (Raspberry Pi build) in %s/pico-8", sys);
        return false;
    }
    char home[600];
    snprintf(home, sizeof home, "%s/pico-8", save ? save : sys);
    char carts[640];
    snprintf(carts, sizeof carts, "%s/carts", home);
    mkdirs(carts);
    char log_path[640];
    snprintf(log_path, sizeof log_path, "%s/core.log", home);
    if (log_file) fclose(log_file);
    log_file = fopen(log_path, "w");
    p8_log_fd = log_file ? fileno(log_file) : -1;

    const char *args[12];
    int n = 0;
    args[n++] = "-home";
    args[n++] = home;
    args[n++] = "-root_path";
    args[n++] = carts;
    const char *path = game && game->path ? game->path : NULL;
    const char *base = path ? strrchr(path, '/') : NULL;
    base = base ? base + 1 : path;
    // pico8_boot = splore: the frontend says this cart is Splore (slot.
    // passes a cached copy under another name); else a cart named splore.*
    struct retro_variable boot = {"pico8_boot", NULL};
    int splore = env(ENV_GET_VARIABLE, &boot) && boot.value ? !strcmp(boot.value, "splore")
                                                             : (!path || !strncasecmp(base, "splore.", 7));
    if (!path || splore) {
        args[n++] = "-splore";
    } else {
        args[n++] = "-run";
        args[n++] = path;
    }
    char err[256];
    p8_logf("pico8: %s %s", args[n - (splore ? 1 : 2)], splore ? "" : path);
    if (p8_boot(exe, home, n, args, err, sizeof err)) {
        p8_logf("pico8: %s", err);
        return false;
    }
    host_ms = 0;
    audio_owed = 0;
    running = true;
    // Run the boot up to PICO-8 opening its audio, so av_info has its rate.
    static const uint8_t none[P8_KEYS];
    for (int i = 0; i < 8 && !p8_audio_rate(); i++) p8_frame(none);
    host_ms = SHIM->vclock_ms;
    return true;
}

bool retro_load_game_special(unsigned type, const struct retro_game_info *info, size_t num) {
    (void)type; (void)info; (void)num;
    return false;
}

void retro_unload_game(void) {
    p8_shutdown();
    running = false;
    if (log_file) { p8_log_fd = -1; fclose(log_file); log_file = NULL; }
}

unsigned retro_get_region(void) { return 0; }

void retro_reset(void) { reset_frames = 4; }  // Ctrl+R: PICO-8 restarts the cart

// Pad to PICO-8's keyboard: player 1 on the arrows, Z (O) and X (X),
// player 2 on ESDF, Tab (O) and Q (X); Start is Enter, PICO-8's pause menu.
static void read_keys(uint8_t *k) {
    memset(k, 0, P8_KEYS);
    input_poll();
    for (unsigned port = 0; port < 2; port++) {
        uint32_t b = 0;
        if (bitmasks) b = (uint16_t)input_state(port, DEVICE_JOYPAD, 0, JOYPAD_MASK);
        else
            for (unsigned id = 0; id < 16; id++)
                if (input_state(port, DEVICE_JOYPAD, 0, id)) b |= 1u << id;
        static const uint8_t p1[16] = {29, 29, 0, 40, 82, 81, 80, 79, 27, 27};
        static const uint8_t p2[16] = {43, 43, 0, 0, 8, 7, 22, 9, 20, 20};
        const uint8_t *map = port ? p2 : p1;
        for (unsigned id = 0; id < 16; id++)
            if ((b >> id & 1) && map[id]) k[map[id]] = 1;
    }
    if (reset_frames > 0) {
        reset_frames--;
        k[224] = 1;          // left Ctrl
        if (reset_frames < 3) k[21] = 1;  // R
    }
}

static void present(void) {
    int w, h, pitch;
    const uint8_t *p = p8_screen(&w, &h, &pitch);
    if (!p || w < 128 || h < 128) {
        video(screen, 128, 128, 128 * 4);
        return;
    }
    // PICO-8 draws its 128x128 screen scaled up: take each cell's middle pixel
    for (int y = 0; y < 128; y++) {
        const uint32_t *row = (const uint32_t *)(p + (size_t)((y * h + h / 2) / 128) * pitch);
        for (int x = 0; x < 128; x++) screen[y * 128 + x] = row[(x * w + w / 2) / 128];
    }
    video(screen, 128, 128, 128 * 4);
}

void retro_run(void) {
    uint8_t keys[P8_KEYS];
    read_keys(keys);
    host_ms += 1000.0 / 60.0;
    // PICO-8 paces itself on SDL_Delay; it runs as many frames as fit.
    // (double: vclock is unsigned, and under 100 ms "vclock - 100" wrapped round)
    double vclock = SHIM->vclock_ms;
    if (host_ms < vclock - 100 || host_ms > vclock + 100) host_ms = vclock;
    for (int i = 0; i < 3 && running && SHIM->vclock_ms < host_ms; i++)
        if (p8_frame(keys) == P8_EXIT) { running = false; p8_logf("pico8: exited"); }
    present();
    static unsigned runs;
    if (++runs % 600 == 1) {
        int w = 0, h = 0, pitch = 0;
        const uint8_t *px = p8_screen(&w, &h, &pitch);
        struct timespec now;
        clock_gettime(CLOCK_MONOTONIC, &now);
        p8_logf("pico8: run %u wall %lld host %.0f vclock %u presents %llu kind %u %dx%d px %08x", runs,
                (long long)now.tv_sec * 1000 + now.tv_nsec / 1000000, host_ms, SHIM->vclock_ms, (unsigned long long)SHIM->frames, SHIM->frame_kind, w, h,
                px ? *(const uint32_t *)(px + (size_t)(h / 2) * pitch + (w / 2) * 4) : 0);
    }

    int rate = p8_audio_rate();
    if (rate && audio_batch) {
        audio_owed += rate / 60.0;
        int n = (int)audio_owed;
        audio_owed -= n;
        static int16_t mono[8192], stereo[8192];
        if (n > 4096) n = 4096;
        int ch = p8_audio(mono, n);
        for (int i = 0; i < n; i++) {
            stereo[2 * i] = ch == 2 ? mono[2 * i] : mono[i];
            stereo[2 * i + 1] = ch == 2 ? mono[2 * i + 1] : mono[i];
        }
        audio_batch(stereo, n);
    }
}

size_t retro_serialize_size(void) { return p8_state_size(); }
bool retro_serialize(void *data, size_t size) { return p8_state_save(data, size) == 0; }
bool retro_unserialize(const void *data, size_t size) {
    if (p8_state_load(data, size)) { p8_logf("pico8: state of %zu bytes not loaded", size); return false; }
    running = !p8_exited();
    host_ms = SHIM->vclock_ms;
    static unsigned loads;
    if (loads++ < 3) p8_logf("pico8: state loaded (%zu bytes)", size);  // not every rewind step
    return true;
}

void *retro_get_memory_data(unsigned id) { (void)id; return NULL; }
size_t retro_get_memory_size(unsigned id) { (void)id; return 0; }
void retro_cheat_reset(void) {}
void retro_cheat_set(unsigned index, bool enabled, const char *code) { (void)index; (void)enabled; (void)code; }
