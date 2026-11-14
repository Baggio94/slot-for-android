// PICO-8 (the official aarch64 Linux build) run inside slot.'s process.
//
// pico8_64 is loaded by our own ELF loader at a fixed address, its imports
// bound to Android's libc (with wrappers for the few glibc-only symbols) and
// to an SDL2 stand-in. It runs its own main loop on a stack of its own; each
// SDL_RenderPresent switches back to the host, so one retro_run is one
// presented frame. Everything PICO-8 can write lives at fixed addresses: its
// RW segment, its heap (a dlmalloc mspace), its stack and the SDL stand-in's
// objects. A save state is a copy of those, so it also loads in a later
// process.
#pragma once
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#define P8_BASE        0x2000000000ULL
#define P8_IMAGE       (P8_BASE)                  /* pico8_64, up to 16 MB */
#define P8_IMAGE_MAX   0x01000000ULL
#define P8_TRAMP       (P8_BASE + 0x01000000ULL)  /* start trampoline */
#define P8_SHIM        (P8_BASE + 0x01010000ULL)  /* struct p8shim */
#define P8_SHIM_SIZE   0x00100000ULL
#define P8_STACK_LO    (P8_BASE + 0x02000000ULL)
#define P8_STACK_SIZE  0x01000000ULL              /* 16 MB, low page a guard */
#define P8_STACK_TOP   (P8_STACK_LO + P8_STACK_SIZE)
/* Every stack PICO-8 code runs on is a 16 MB-aligned region whose first
 * word holds the host's x18 (Android's shadow call stack) for the import
 * thunks to restore; a guard follows, then the stack. */
#define P8_STACK_ALIGN 0x01000000ULL
#define P8_STACK_SLOT  0x4000ULL                  /* the x18 page */
#define P8_STACK_GUARD 0x10000ULL                 /* slot page + guard */
#define P8_CBSTACK     (P8_BASE + 0x03000000ULL)  /* host-thread calls (audio) */
#define P8_WSTACKS     (P8_BASE + 0x50000000ULL)  /* PICO-8's own threads */
#define P8_WSTACK_N    16
#define P8_HEAP        (P8_BASE + 0x04000000ULL)
#define P8_HEAP_MAX    0x40000000ULL              /* reserved, touched as used */

/* Callee-saved state of a stack: what p8_switch saves and loads. */
struct p8ctx {
    uint64_t x[10];   /* x19..x28 */
    uint64_t fp, lr, sp;
    uint64_t d[8];    /* d8..d15 */
    uint64_t x18, pad;
};
_Static_assert(sizeof(struct p8ctx) == 184, "p8ctx layout is shared with switch.S");

enum { P8_RUN = 0, P8_FRAME = 1, P8_EXIT = 2, P8_IDLE = 3 };

/* What a frame's input is: the keys PICO-8 sees held, by SDL scancode. */
#define P8_KEYS 512

struct p8file;
struct p8tex;

/* Everything at P8_SHIM: saved with the state. */
struct p8shim {
    uint32_t magic;
    uint32_t vclock_ms;         /* SDL_GetTicks */
    uint64_t frames;            /* presented frames */
    struct p8ctx ctx;           /* PICO-8's stack while the host runs */
    uint8_t keys[P8_KEYS];      /* SDL_GetKeyboardState */
    uint8_t prev_keys[P8_KEYS]; /* what the events so far told PICO-8 */
    uint32_t rand_state;
    /* audio, as SDL_OpenAudio was asked */
    int32_t audio_freq;
    uint16_t audio_format;
    uint8_t audio_channels, audio_open;
    uint16_t audio_samples;
    uint8_t audio_paused, pad0;
    uint64_t audio_callback, audio_userdata;
    double audio_carry;         /* fractional samples owed */
    /* SDL objects */
    uint64_t window, renderer, window_surface;
    struct p8tex *copy_tex;     /* first texture copied since the clear */
    int32_t copy_src[4];
    uint32_t window_id;
    int32_t display_w, display_h;
    /* files */
    struct p8file *files;       /* open files, for reopening after a load */
    uint64_t std_files[3];      /* stdin, stdout, stderr as PICO-8 sees them */
    char home[512];             /* getenv("HOME"), getcwd */
    char exe[512];              /* readlink("/proc/self/exe") */
    char clipboard[256];
    uint64_t heap;              /* the mspace */
    /* SDL events not yet polled */
    uint8_t events[64][56];
    uint32_t ev_head, ev_count;
    uint64_t frame_surface;     /* SDL_Surface last presented by UpdateWindowSurface */
    uint32_t frame_kind;        /* 0 none, 1 texture, 2 window surface */
    uint32_t gettick_spin;      /* GetTicks calls since the last present */
    uint32_t strings_used;
    char strings[8192];         /* strings SDL hands PICO-8 */
};

#define P8_SHIM_MAGIC 0x50385348u /* "P8SH" */
_Static_assert(offsetof(struct p8shim, ctx) == 0x10, "switch.S has &SHIM->ctx");
#define SHIM ((struct p8shim *)P8_SHIM)

/* switch.S */
/* call PICO-8 code on the stack region at `region` (16 MB, see above) */
uint64_t p8_call_on(uint64_t region, void *fn, uint64_t a, uint64_t b, uint64_t c);
extern const uint8_t p8_thunk_template[], p8_thunk_template_end[];
uint64_t p8_get_x18(void);
extern int p8_exit_code;
uint64_t p8_switch(struct p8ctx *save, struct p8ctx *load, uint64_t value);
void p8_yield_present(void *renderer);   /* SDL_RenderPresent */
int p8_yield_window(void *window);       /* SDL_UpdateWindowSurface */
void p8_yield_delay(uint32_t ms);        /* SDL_Delay */
void p8_exit(int code) __attribute__((noreturn));
int p8_setjmp(void *buf);
void p8_longjmp(void *buf, int value) __attribute__((noreturn));
extern struct p8ctx p8_host_ctx;

/* core.c: an anonymous mapping at exactly addr (reclaiming our own leftovers) */
int p8_map_fixed(uint64_t addr, uint64_t len, int prot, int extra);

/* loader.c */
typedef void *(*p8_resolver)(const char *name);
struct p8image {
    uint64_t base, span;
    uint64_t rx_start, rx_end;   /* text, absolute */
    uint64_t rw_start, rw_end;   /* RW segment incl. bss, absolute */
    uint64_t hash;               /* FNV-1a of the file */
};
int p8_load_image(const char *path, struct p8image *img, p8_resolver resolve, char *err, size_t errlen);
int p8_rebind_imports(p8_resolver resolve);
uint64_t p8_symbol(const char *name, uint64_t *size);
void p8_unload_image(void);

/* imports.c */
void *p8_resolve(const char *name);
void p8_files_save_positions(void);
void p8_files_after_load(void);
extern uint32_t p8_generation;
extern void (*p8_log)(const char *line);
void p8_logf(const char *fmt, ...) __attribute__((format(printf, 1, 2)));
// The log as a file descriptor, for what a signal handler writes (-1: none).
extern int p8_log_fd;
void p8_log_raw(const char *s, size_t n);
/* wget "url" -q -O "file": the host fetches, 0 on success */
extern int (*p8_http_get)(const char *url, const char *post_path, const char *out_path);
void p8_imports_init(void);
void p8_imports_after_load(void);

/* heap */
void *p8_malloc(size_t n);
void *p8_calloc(size_t n, size_t m);
void *p8_realloc(void *p, size_t n);
void p8_free(void *p);
size_t p8_heap_used(void);
void p8_heap_after_load(void);
int p8_heap_init(void);
void p8_heap_release(void);

/* sdl.c */
void *p8_sdl_resolve(const char *name);
struct p8tex {
    uint32_t format;
    int32_t access, w, h, pitch;
    uint8_t *pixels;
};

/* core.c */
int p8_boot(const char *exe, const char *home, int argc, const char *const *argv, char *err, size_t errlen);
int p8_frame(const uint8_t *keys);
int p8_exited(void);
const uint8_t *p8_screen(int *w, int *h, int *pitch);
int p8_audio_rate(void);
int p8_audio(int16_t *out, int frames);
size_t p8_state_size(void);
int p8_state_save(void *buf, size_t len);
int p8_state_load(const void *buf, size_t len);
void p8_shutdown(void);
