// The SDL2 functions pico8_64 imports, as a headless stand-in: one window,
// one renderer, textures in PICO-8's heap. The frame the host shows is the
// texture last copied to the renderer (or the window surface, on the
// software path). Input arrives as keyboard state plus key events made from
// its changes. Strings handed to PICO-8 live at a fixed address.
#include "p8.h"

#include <pthread.h>
#include <unistd.h>
#include <stdlib.h>
#include <string.h>

#define SDL_PIXELFORMAT_ARGB8888 0x16362004u
#define AUDIO_S16LSB 0x8010

struct sdl_rect { int32_t x, y, w, h; };
struct sdl_version { uint8_t major, minor, patch; };
struct sdl_display_mode { uint32_t format; int32_t w, h, refresh; void *driverdata; };
struct sdl_renderer_info {
    const char *name;
    uint32_t flags, num_texture_formats, texture_formats[16];
    int32_t max_w, max_h;
};
struct sdl_pixel_format {
    uint32_t format;
    void *palette;
    uint8_t bits, bytes, pad[2];
    uint32_t rmask, gmask, bmask, amask;
    uint8_t rloss, gloss, bloss, aloss, rshift, gshift, bshift, ashift;
    int32_t refcount;
    void *next;
};
struct sdl_surface {
    uint32_t flags;
    struct sdl_pixel_format *format;
    int32_t w, h, pitch;
    void *pixels, *userdata;
    int32_t locked;
    void *list_blitmap;
    struct sdl_rect clip;
    void *map;
    int32_t refcount;
};
struct sdl_audio_spec {
    int32_t freq;
    uint16_t format;
    uint8_t channels, silence;
    uint16_t samples, padding;
    uint32_t size;
    void *callback, *userdata;
};
struct sdl_window { uint32_t magic; int32_t x, y, w, h; struct sdl_surface *surface; };
struct sdl_renderer { uint32_t magic; struct sdl_window *window; };

/* ---- fixed strings ---- */
#define STRINGS (SHIM->strings)
#define STRINGS_SIZE sizeof(SHIM->strings)
#define strings_used (SHIM->strings_used)
static const char *fixed(const char *s) {
    for (size_t at = 0; at < strings_used; at += strlen(STRINGS + at) + 1)
        if (!strcmp(STRINGS + at, s)) return STRINGS + at;
    size_t l = strlen(s) + 1;
    if (strings_used + l > STRINGS_SIZE) return "";
    memcpy(STRINGS + strings_used, s, l);
    strings_used += l;
    return STRINGS + strings_used - l;
}
static void strings_init(void) {
    strings_used = 0;
    // in a fixed order, so a string's address is the same in every process
    static const char *const all[] = {"", "slot", "software", "Slot Display", "slot."};
    for (size_t i = 0; i < sizeof all / sizeof all[0]; i++) fixed(all[i]);
}

/* ---- trivial ---- */
static int ok0(void) { return 0; }
static int one(void) { return 1; }
static void *null0(void) { return NULL; }
static void nop(void) {}
static int s_Init(uint32_t f) { (void)f; return 0; }
static void s_GetVersion(struct sdl_version *v) { v->major = 2; v->minor = 0; v->patch = 20; }
static const char *s_GetError(void) { return fixed(""); }
static const char *s_driver(int i) { (void)i; return fixed("slot"); }
static const char *s_cur_driver(void) { return fixed("slot"); }
static const char *s_GetDisplayName(int i) { (void)i; return fixed("Slot Display"); }
static int s_GetDisplayBounds(int i, struct sdl_rect *r) {
    (void)i;
    *r = (struct sdl_rect){0, 0, SHIM->display_w, SHIM->display_h};
    return 0;
}
static int s_display_mode(int i, struct sdl_display_mode *m) {
    (void)i;
    *m = (struct sdl_display_mode){SDL_PIXELFORMAT_ARGB8888, SHIM->display_w, SHIM->display_h, 60, NULL};
    return 0;
}
static int s_GetDisplayMode(int i, int idx, struct sdl_display_mode *m) { (void)idx; return s_display_mode(i, m); }

static uint32_t s_GetTicks(void) {
    // A loop waiting on the clock alone would never end: let time pass if
    // PICO-8 asks very often without presenting.
    if (++SHIM->gettick_spin > 20000) SHIM->vclock_ms++;
    return SHIM->vclock_ms;
}
// The clock when PICO-8 last presented or gave the host its turn.
static uint32_t turn_vclock;
// SDL_Delay (through p8_yield_delay): 1 when PICO-8 has waited 50 ms
// without presenting, and the host should run.
int p8_pre_delay(uint32_t ms) {
    SHIM->vclock_ms += ms;
    if (SHIM->vclock_ms - turn_vclock < 50) return 0;
    turn_vclock = SHIM->vclock_ms;
    return 1;
}

/* ---- window and renderer ---- */
static struct sdl_pixel_format *argb_format(void) {
    struct sdl_pixel_format *f = p8_calloc(1, sizeof *f);
    f->format = SDL_PIXELFORMAT_ARGB8888;
    f->bits = 32; f->bytes = 4;
    f->rmask = 0xff0000; f->gmask = 0xff00; f->bmask = 0xff; f->amask = 0xff000000;
    f->rshift = 16; f->gshift = 8; f->bshift = 0; f->ashift = 24;
    f->refcount = 1;
    return f;
}
static struct sdl_surface *surface_new(int w, int h, void *pixels, int pitch) {
    struct sdl_surface *s = p8_calloc(1, sizeof *s);
    s->format = argb_format();
    s->w = w; s->h = h;
    s->pitch = pitch ? pitch : w * 4;
    s->pixels = pixels ? pixels : p8_calloc((size_t)h, (size_t)s->pitch);
    s->clip = (struct sdl_rect){0, 0, w, h};
    s->refcount = 1;
    if (!pixels) s->flags = 0x80000000u;  // ours to free
    return s;
}
static void surface_free(struct sdl_surface *s) {
    if (!s || --s->refcount > 0) return;
    if (s->flags & 0x80000000u) p8_free(s->pixels);
    p8_free(s->format);
    p8_free(s);
}

static void *s_CreateWindow(const char *title, int x, int y, int w, int h, uint32_t flags) {
    p8_logf("sdl: CreateWindow %s %dx%d flags %#x", title ? title : "", w, h, flags);
    struct sdl_window *win = p8_calloc(1, sizeof *win);
    win->magic = 0x57494e44;
    win->x = x; win->y = y; win->w = w; win->h = h;
    SHIM->window = (uint64_t)win;
    return win;
}
static void s_DestroyWindow(struct sdl_window *w) {
    if (!w) return;
    surface_free(w->surface);
    if (SHIM->window == (uint64_t)w) SHIM->window = 0;
    p8_free(w);
}
static void s_GetWindowSize(struct sdl_window *w, int *x, int *y) {
    if (x) *x = w ? w->w : 0;
    if (y) *y = w ? w->h : 0;
}
static void s_GetWindowPosition(struct sdl_window *w, int *x, int *y) {
    if (x) *x = w ? w->x : 0;
    if (y) *y = w ? w->y : 0;
}
static uint32_t s_GetWindowID(void *w) { (void)w; return 1; }
static void *s_GetWindowSurface(struct sdl_window *w) {
    if (!w) return NULL;
    if (!w->surface || w->surface->w != w->w || w->surface->h != w->h) {
        surface_free(w->surface);
        w->surface = surface_new(w->w, w->h, NULL, 0);
        p8_logf("sdl: window surface %dx%d", w->w, w->h);
    }
    return w->surface;
}
static void *s_GetKeyboardFocus(void) { return (void *)SHIM->window; }
static void *s_GL_CreateContext(void *w) { (void)w; return (void *)(P8_SHIM + 0x9ff00); }

static void *s_CreateRenderer(struct sdl_window *w, int index, uint32_t flags) {
    p8_logf("sdl: CreateRenderer %d flags %#x", index, flags);
    struct sdl_renderer *r = p8_calloc(1, sizeof *r);
    r->magic = 0x52454e44;
    r->window = w;
    SHIM->renderer = (uint64_t)r;
    return r;
}
static void s_DestroyRenderer(void *r) {
    if (SHIM->renderer == (uint64_t)r) SHIM->renderer = 0;
    p8_free(r);
}
static int s_renderer_info(struct sdl_renderer_info *i) {
    memset(i, 0, sizeof *i);
    i->name = fixed("software");
    i->flags = 2 | 8;
    i->num_texture_formats = 1;
    i->texture_formats[0] = SDL_PIXELFORMAT_ARGB8888;
    i->max_w = i->max_h = 4096;
    return 0;
}
static int s_GetRenderDriverInfo(int idx, struct sdl_renderer_info *i) { (void)idx; return s_renderer_info(i); }
static int s_GetRendererInfo(void *r, struct sdl_renderer_info *i) { (void)r; return s_renderer_info(i); }

static void *s_CreateTexture(void *r, uint32_t format, int access, int w, int h) {
    (void)r;
    p8_logf("sdl: CreateTexture %#x access %d %dx%d", format, access, w, h);
    struct p8tex *t = p8_calloc(1, sizeof *t);
    t->format = format;
    t->access = access;
    t->w = w; t->h = h;
    t->pitch = w * 4;
    t->pixels = p8_calloc((size_t)h, (size_t)t->pitch);
    return t;
}
static void s_DestroyTexture(struct p8tex *t) {
    if (!t) return;
    if (SHIM->copy_tex == t) { SHIM->copy_tex = NULL; SHIM->frame_kind = 0; }
    p8_free(t->pixels);
    p8_free(t);
}
static int s_UpdateTexture(struct p8tex *t, const struct sdl_rect *r, const void *pixels, int pitch) {
    struct sdl_rect a = r ? *r : (struct sdl_rect){0, 0, t->w, t->h};
    if (a.x < 0 || a.y < 0 || a.x + a.w > t->w || a.y + a.h > t->h) return -1;
    for (int y = 0; y < a.h; y++)
        memcpy(t->pixels + (size_t)(a.y + y) * t->pitch + a.x * 4, (const uint8_t *)pixels + (size_t)y * pitch,
               (size_t)a.w * 4);
    return 0;
}
static int s_RenderCopy(void *r, struct p8tex *t, const struct sdl_rect *src, const struct sdl_rect *dst) {
    (void)r; (void)dst;
    SHIM->copy_tex = t;
    if (src) memcpy(SHIM->copy_src, src, sizeof SHIM->copy_src);
    else { SHIM->copy_src[0] = SHIM->copy_src[1] = 0; SHIM->copy_src[2] = t->w; SHIM->copy_src[3] = t->h; }
    SHIM->frame_kind = 1;
    return 0;
}
// Called by the yields in switch.S just before they hand the frame to the host.
void p8_pre_present(void *r) {
    (void)r;
    SHIM->frames++;
    SHIM->gettick_spin = 0;
    // Vsync: a present takes the rest of a 60 Hz frame, as on a real
    // display. PICO-8 relies on it where it doesn't wait on SDL_Delay (its
    // boot, its menus); without it its clock would hardly move there.
    static uint32_t last_present;
    uint32_t frame = 16 + (SHIM->frames % 3 != 0);  // 16.67 ms on average
    if (SHIM->vclock_ms - last_present < frame) SHIM->vclock_ms = last_present + frame;
    last_present = SHIM->vclock_ms;
    turn_vclock = SHIM->vclock_ms;
}
void p8_pre_window(struct sdl_window *w) {
    p8_pre_present(NULL);
    if (w && w->surface) { SHIM->frame_surface = (uint64_t)w->surface; SHIM->frame_kind = 2; }
}
static void *s_CreateRGBSurfaceFrom(void *pixels, int w, int h, int depth, int pitch, uint32_t rm, uint32_t gm,
                                    uint32_t bm, uint32_t am) {
    (void)depth; (void)rm; (void)gm; (void)bm; (void)am;
    return surface_new(w, h, pixels, pitch);
}

/* ---- input ---- */
static const uint8_t *s_GetKeyboardState(int *n) {
    if (n) *n = P8_KEYS;
    return SHIM->keys;
}
static uint16_t mods(void) {
    const uint8_t *k = SHIM->keys;
    return (k[225] ? 1 : 0) | (k[229] ? 2 : 0) | (k[224] ? 0x40 : 0) | (k[228] ? 0x80 : 0) | (k[226] ? 0x100 : 0) |
           (k[230] ? 0x200 : 0) | (k[227] ? 0x400 : 0);
}
static int s_GetModState(void) { return mods(); }
static int32_t keycode(int sc) {
    static const char punct[] = "\r\x1b\b\t -=[]\\#;'`,./";
    if (sc >= 4 && sc <= 29) return 'a' + sc - 4;
    if (sc >= 30 && sc <= 38) return '1' + sc - 30;
    if (sc == 39) return '0';
    if (sc >= 40 && sc <= 56) return (uint8_t)punct[sc - 40];
    if (sc == 76) return 127;
    return sc | 0x40000000;
}
static int32_t s_GetKeyFromScancode(int sc) { return keycode(sc); }
static const char *s_GetScancodeName(int sc) {
    char b[16];
    int32_t k = keycode(sc);
    if (k >= 'a' && k <= 'z') { b[0] = (char)(k - 32); b[1] = 0; }
    else if (k >= '0' && k <= '9') { b[0] = (char)k; b[1] = 0; }
    else snprintf(b, sizeof b, "K%d", sc);
    return fixed(b);
}
static uint32_t s_GetMouseState(int *x, int *y) {
    if (x) *x = 0;
    if (y) *y = 0;
    return 0;
}

static int s_PollEvent(void *e) {
    if (!SHIM->ev_count) return 0;
    if (!e) return 1;
    memcpy(e, SHIM->events[SHIM->ev_head], 56);
    SHIM->ev_head = (SHIM->ev_head + 1) % 64;
    SHIM->ev_count--;
    return 1;
}
static void push_event(const uint8_t *ev) {
    if (SHIM->ev_count == 64) return;
    memcpy(SHIM->events[(SHIM->ev_head + SHIM->ev_count) % 64], ev, 56);
    SHIM->ev_count++;
}

// The host's input for the next frame: key events for every change, then the state.
void p8_set_keys(const uint8_t *keys) {
    for (int sc = 0; sc < P8_KEYS; sc++) {
        if (!!keys[sc] == !!SHIM->prev_keys[sc]) continue;
        SHIM->keys[sc] = keys[sc] ? 1 : 0;
        uint8_t ev[56] = {0};
        uint32_t type = keys[sc] ? 0x300 : 0x301;
        int32_t sym = keycode(sc);
        uint16_t m = mods();
        memcpy(ev + 0, &type, 4);
        memcpy(ev + 4, &SHIM->vclock_ms, 4);
        uint32_t wid = 1;
        memcpy(ev + 8, &wid, 4);
        ev[12] = keys[sc] ? 1 : 0;
        memcpy(ev + 16, &sc, 4);
        memcpy(ev + 20, &sym, 4);
        memcpy(ev + 24, &m, 2);
        push_event(ev);
        SHIM->prev_keys[sc] = SHIM->keys[sc];
    }
}

/* ---- clipboard ---- */
static char *s_GetClipboardText(void) {
    size_t l = strlen(SHIM->clipboard) + 1;
    char *s = p8_malloc(l);
    if (s) memcpy(s, SHIM->clipboard, l);
    return s;
}
static int s_HasClipboardText(void) { return SHIM->clipboard[0] != 0; }
static int s_SetClipboardText(const char *t) {
    snprintf(SHIM->clipboard, sizeof SHIM->clipboard, "%s", t ? t : "");
    return 0;
}

/* ---- audio ---- */
static int s_OpenAudio(struct sdl_audio_spec *want, struct sdl_audio_spec *have) {
    p8_logf("sdl: OpenAudio %d Hz format %#x ch %d samples %d", want->freq, want->format, want->channels,
            want->samples);
    SHIM->audio_freq = want->freq;
    SHIM->audio_format = want->format;
    SHIM->audio_channels = want->channels;
    SHIM->audio_samples = want->samples;
    SHIM->audio_callback = (uint64_t)want->callback;
    SHIM->audio_userdata = (uint64_t)want->userdata;
    SHIM->audio_open = 1;
    SHIM->audio_paused = 1;
    if (have) {
        *have = *want;
        have->silence = 0;
        have->size = (uint32_t)want->samples * want->channels * ((want->format & 0xff) / 8);
    }
    return 0;
}
static void s_PauseAudio(int p) { SHIM->audio_paused = p ? 1 : 0; }

/* ---- threads: real ones ---- */
struct thread_start { void *fn, *data; };
int p8_threads;  // PICO-8 threads still running (atomic)
static uint32_t wstacks_used;  // bit per stack region at P8_WSTACKS
static int wstack_take(void) {
    for (;;) {
        uint32_t used = __atomic_load_n(&wstacks_used, __ATOMIC_SEQ_CST);
        int i = __builtin_ctz(~used | (1u << P8_WSTACK_N));
        if (i >= P8_WSTACK_N) return -1;
        if (__atomic_compare_exchange_n(&wstacks_used, &used, used | 1u << i, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST))
            return i;
    }
}
static void *thread_main(void *arg) {
    struct thread_start s = *(struct thread_start *)arg;
    free(arg);
    int i;
    while ((i = wstack_take()) < 0) usleep(10000);
    p8_call_on(P8_WSTACKS + (uint64_t)i * P8_STACK_ALIGN, s.fn, (uint64_t)s.data, 0, 0);
    __atomic_and_fetch(&wstacks_used, ~(1u << i), __ATOMIC_SEQ_CST);
    __atomic_sub_fetch(&p8_threads, 1, __ATOMIC_SEQ_CST);
    return NULL;
}
static void *s_CreateThread(void *fn, const char *name, void *data) {
    p8_logf("sdl: CreateThread %s", name ? name : "");
    struct thread_start *s = malloc(sizeof *s);
    s->fn = fn;
    s->data = data;
    pthread_t t;
    pthread_attr_t a;
    pthread_attr_init(&a);
    pthread_attr_setstacksize(&a, 64 << 10);  // PICO-8 runs on its own stack region
    __atomic_add_fetch(&p8_threads, 1, __ATOMIC_SEQ_CST);
    int r = pthread_create(&t, &a, thread_main, s);
    pthread_attr_destroy(&a);
    if (r) { __atomic_sub_fetch(&p8_threads, 1, __ATOMIC_SEQ_CST); free(s); return NULL; }
    pthread_detach(t);
    // PICO-8 only detaches it: a token does
    return (void *)(P8_SHIM + 0x9ff80);
}

static void s_free(void *p) { p8_free(p); }

#define E(name, fn) { "SDL_" #name, (void *)fn }
static const struct { const char *name; void *fn; } table[] = {
    E(Init, s_Init), E(InitSubSystem, s_Init), E(GetVersion, s_GetVersion), E(SetHint, one), E(ClearError, nop),
    E(GetError, s_GetError), E(GetNumVideoDrivers, one), E(GetVideoDriver, s_driver),
    E(GetCurrentVideoDriver, s_cur_driver), E(GetNumVideoDisplays, one), E(GetDisplayName, s_GetDisplayName),
    E(GetDisplayBounds, s_GetDisplayBounds), E(GetDesktopDisplayMode, s_display_mode),
    E(GetCurrentDisplayMode, s_display_mode), E(GetDisplayMode, s_GetDisplayMode), E(GetNumDisplayModes, one),
    E(GetTicks, s_GetTicks), E(Delay, p8_yield_delay),
    E(CreateWindow, s_CreateWindow), E(DestroyWindow, s_DestroyWindow), E(GetWindowSize, s_GetWindowSize),
    E(GetWindowPosition, s_GetWindowPosition), E(GetWindowID, s_GetWindowID), E(GetWindowDisplayIndex, ok0),
    E(GetWindowSurface, s_GetWindowSurface), E(UpdateWindowSurface, p8_yield_window),
    E(SetWindowTitle, nop), E(SetWindowIcon, nop), E(SetWindowGrab, nop), E(RaiseWindow, nop),
    E(WarpMouseInWindow, nop), E(GetKeyboardFocus, s_GetKeyboardFocus), E(ShowCursor, ok0),
    E(GL_SetAttribute, ok0), E(GL_CreateContext, s_GL_CreateContext), E(GL_DeleteContext, nop),
    E(CreateRenderer, s_CreateRenderer), E(DestroyRenderer, s_DestroyRenderer), E(GetNumRenderDrivers, one),
    E(GetRenderDriverInfo, s_GetRenderDriverInfo), E(GetRendererInfo, s_GetRendererInfo),
    E(SetRenderDrawColor, ok0), E(RenderClear, ok0), E(RenderCopy, s_RenderCopy), E(RenderPresent, p8_yield_present),
    E(CreateTexture, s_CreateTexture), E(DestroyTexture, s_DestroyTexture), E(UpdateTexture, s_UpdateTexture),
    E(CreateRGBSurfaceFrom, s_CreateRGBSurfaceFrom), E(FreeSurface, surface_free), E(LockSurface, ok0),
    E(UnlockSurface, nop),
    E(PollEvent, s_PollEvent), E(GetKeyboardState, s_GetKeyboardState), E(GetModState, s_GetModState),
    E(GetKeyFromScancode, s_GetKeyFromScancode), E(GetScancodeName, s_GetScancodeName),
    E(GetMouseState, s_GetMouseState),
    E(NumJoysticks, ok0), E(IsGameController, ok0), E(JoystickOpen, null0), E(JoystickClose, nop),
    E(JoystickGetAxis, ok0), E(JoystickGetButton, ok0), E(JoystickInstanceID, ok0), E(JoystickNameForIndex, null0),
    E(JoystickNumAxes, ok0), E(JoystickNumBalls, ok0), E(JoystickNumButtons, ok0),
    E(GameControllerAddMapping, one), E(GameControllerClose, nop), E(GameControllerGetAttached, ok0),
    E(GameControllerGetAxis, ok0), E(GameControllerGetButton, ok0), E(GameControllerGetJoystick, null0),
    E(GameControllerMapping, null0), E(GameControllerNameForIndex, null0), E(GameControllerOpen, null0),
    E(GetClipboardText, s_GetClipboardText), E(HasClipboardText, s_HasClipboardText),
    E(SetClipboardText, s_SetClipboardText), E(free, s_free),
    E(GetNumAudioDrivers, one), E(GetAudioDriver, s_driver), E(GetCurrentAudioDriver, s_cur_driver),
    E(OpenAudio, s_OpenAudio), E(PauseAudio, s_PauseAudio), E(LockAudio, nop), E(UnlockAudio, nop),
    E(CreateThread, s_CreateThread), E(DetachThread, nop),
};

void *p8_sdl_resolve(const char *name) {
    for (size_t i = 0; i < sizeof table / sizeof table[0]; i++)
        if (!strcmp(table[i].name, name)) return table[i].fn;
    return NULL;
}

void p8_sdl_init(void) { strings_init(); }
