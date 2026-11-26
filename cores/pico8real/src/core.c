// Booting PICO-8, running it a frame at a time, its picture and sound, and
// save states.
#include "p8.h"

#include <errno.h>
#include <dlfcn.h>
#include <signal.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

#ifndef MAP_FIXED_NOREPLACE
#define MAP_FIXED_NOREPLACE 0x100000
#endif

void p8_sdl_init(void);
void p8_set_keys(const uint8_t *keys);

static struct p8image img;
static int mapped, booted, exited;
static uint64_t splore_init, splore_inited, load_config, set_running_item;  // PICO-8's own (see splore_ready)
static char cart_info[700];

extern int p8_threads;  // PICO-8's own threads still running (sdl.c)

// Waits up to `ms` for PICO-8's threads (Splore's downloads) to end.
static int threads_done(int ms) {
    for (int i = 0; i < ms / 10 && __atomic_load_n(&p8_threads, __ATOMIC_SEQ_CST); i++) usleep(10000);
    return !__atomic_load_n(&p8_threads, __ATOMIC_SEQ_CST);
}

/* ---- a crash inside PICO-8 is written to the log before the process goes ---- */

static struct sigaction old_actions[32];
static const int fatal[] = {SIGSEGV, SIGBUS, SIGILL, SIGFPE, SIGTRAP, SIGABRT};

static char *hex(char *p, const char *label, uint64_t v) {
    while (*label) *p++ = *label++;
    *p++ = '0'; *p++ = 'x';
    for (int s = 60; s >= 0; s -= 4) *p++ = "0123456789abcdef"[(v >> s) & 15];
    return p;
}
// A code address as an offset into pico8_64 when it is one: what objdump shows.
static uint64_t image_off(uint64_t a) { return a >= img.base && a < img.base + img.span ? a - img.base : a; }

static void on_fatal(int sig, siginfo_t *si, void *uc_) {
    ucontext_t *uc = uc_;
    char line[256], *p = line;
    const char *head = "pico8: fatal signal ";
    while (*head) *p++ = *head++;
    *p++ = '0' + sig / 10; *p++ = '0' + sig % 10;
    p = hex(p, " addr ", (uint64_t)si->si_addr);
    p = hex(p, " pc ", image_off(uc->uc_mcontext.pc));
    p = hex(p, " lr ", image_off(uc->uc_mcontext.regs[30]));
    p = hex(p, " sp ", uc->uc_mcontext.sp);
    *p++ = '\n';
    p8_log_raw(line, (size_t)(p - line));
    // where: pc, lr, then the frame-pointer chain, by library
    uint64_t at[10] = {uc->uc_mcontext.pc, uc->uc_mcontext.regs[30]};
    int n = 2;
    for (uint64_t *fp = (uint64_t *)uc->uc_mcontext.regs[29]; n < 10 && fp && !((uint64_t)fp & 7); fp = (uint64_t *)fp[0]) {
        if (fp[1] == 0) break;
        at[n++] = fp[1];
        if (fp[0] <= (uint64_t)fp) break;
    }
    for (int i = 0; i < n; i++) {
        Dl_info di;
        if (image_off(at[i]) != at[i])
            p8_logf("  #%d pico8_64+0x%llx", i, (unsigned long long)image_off(at[i]));
        else if (dladdr((void *)at[i], &di) && di.dli_fname)
            p8_logf("  #%d %s+0x%llx (%s)", i, di.dli_fname, (unsigned long long)(at[i] - (uint64_t)di.dli_fbase),
                    di.dli_sname ? di.dli_sname : "?");
        else p8_logf("  #%d 0x%llx", i, (unsigned long long)at[i]);
    }
    // then Android's own handler: the tombstone
    sigaction(sig, &old_actions[sig], NULL);
    raise(sig);
}

static void crash_reporter(void) {
    static int installed;
    if (installed) return;
    installed = 1;
    struct sigaction sa;
    memset(&sa, 0, sizeof sa);
    sa.sa_sigaction = on_fatal;
    sa.sa_flags = SA_SIGINFO | SA_ONSTACK;
    sigemptyset(&sa.sa_mask);
    for (size_t i = 0; i < sizeof fatal / sizeof fatal[0]; i++) sigaction(fatal[i], &sa, &old_actions[fatal[i]]);
}

// Whether [addr, addr+len) holds only anonymous memory: what an earlier
// load of this library left behind, which it may take back.
static int only_anonymous(uint64_t addr, uint64_t len) {
    FILE *f = fopen("/proc/self/maps", "r");
    if (!f) return 0;
    char line[512];
    int ok = 1;
    while (ok && fgets(line, sizeof line, f)) {
        unsigned long long lo, hi;
        int name = 0;
        if (sscanf(line, "%llx-%llx %*s %*s %*s %*s %n", &lo, &hi, &name) < 2) continue;
        if (hi <= addr || lo >= addr + len) continue;
        const char *n = line + name;
        if (*n && *n != '\n' && strncmp(n, "[anon:", 6)) ok = 0;
    }
    fclose(f);
    return ok;
}

int p8_map_fixed(uint64_t addr, uint64_t len, int prot, int extra) {
    void *m = mmap((void *)addr, len, prot, MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE | extra, -1, 0);
    if (m == (void *)addr) return 0;
    if (m != MAP_FAILED) munmap(m, len);
    if (errno == EEXIST && only_anonymous(addr, len)) {
        m = mmap((void *)addr, len, prot, MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED | extra, -1, 0);
        if (m == (void *)addr) return 0;
    }
    return -1;
}
#define map_fixed p8_map_fixed

static int map_regions(char *err, size_t errlen) {
    if (mapped) {
        mprotect((void *)P8_TRAMP, 0x10000, PROT_READ | PROT_WRITE);
        madvise((void *)P8_SHIM, P8_SHIM_SIZE, MADV_DONTNEED);
        madvise((void *)(P8_STACK_LO + P8_STACK_GUARD), P8_STACK_SIZE - P8_STACK_GUARD, MADV_DONTNEED);
        return 0;
    }
    if (map_fixed(P8_TRAMP, 0x10000, PROT_READ | PROT_WRITE, 0) ||
        map_fixed(P8_SHIM, P8_SHIM_SIZE, PROT_READ | PROT_WRITE, 0) ||
        map_fixed(P8_STACK_LO, P8_STACK_SIZE, PROT_READ | PROT_WRITE, MAP_NORESERVE) ||
        map_fixed(P8_CBSTACK, P8_STACK_ALIGN, PROT_READ | PROT_WRITE, MAP_NORESERVE) ||
        map_fixed(P8_WSTACKS, P8_WSTACK_N * P8_STACK_ALIGN, PROT_READ | PROT_WRITE, MAP_NORESERVE)) {
        snprintf(err, errlen, "cannot map PICO-8's memory at %#llx: %s", P8_BASE, strerror(errno));
        return -1;
    }
    // each stack region: its x18 page, a guard, then the stack
    mprotect((void *)(P8_STACK_LO + P8_STACK_SLOT), P8_STACK_GUARD - P8_STACK_SLOT, PROT_NONE);
    mprotect((void *)(P8_CBSTACK + P8_STACK_SLOT), P8_STACK_GUARD - P8_STACK_SLOT, PROT_NONE);
    for (int i = 0; i < P8_WSTACK_N; i++)
        mprotect((void *)(P8_WSTACKS + i * P8_STACK_ALIGN + P8_STACK_SLOT), P8_STACK_GUARD - P8_STACK_SLOT, PROT_NONE);
    mapped = 1;
    return 0;
}

int p8_boot(const char *exe, const char *home, int argc, const char *const *argv, char *err, size_t errlen) {
    if (booted) { snprintf(err, errlen, "already running"); return -1; }
    crash_reporter();
    // The last PICO-8's downloads end before its memory is taken back.
    if (!threads_done(35000)) { snprintf(err, errlen, "the last PICO-8 is still downloading"); return -1; }
    if (mapped && img.base) { p8_unload_image(); p8_heap_release(); memset(&img, 0, sizeof img); }
    if (map_regions(err, errlen)) return -1;
    memset(SHIM, 0, sizeof *SHIM);
    SHIM->magic = P8_SHIM_MAGIC;
    SHIM->display_w = 1280;
    SHIM->display_h = 720;
    SHIM->rand_state = 1;
    snprintf(SHIM->home, sizeof SHIM->home, "%s", home);
    snprintf(SHIM->exe, sizeof SHIM->exe, "%s", exe);
    if (p8_heap_init()) { snprintf(err, errlen, "cannot map PICO-8's heap"); return -1; }
    p8_sdl_init();
    p8_imports_init();
    if (p8_load_image(exe, &img, p8_resolve, err, errlen)) return -1;
    uint64_t main_fn = p8_symbol("main", NULL);
    splore_init = p8_symbol("splore_init", NULL);
    splore_inited = p8_symbol("splore_inited", NULL);
    if (!splore_inited) splore_init = 0;
    load_config = p8_symbol("codo_load_stable_config", NULL);
    set_running_item = p8_symbol("set_running_item_from_nfo", NULL);
    if (!main_fn) { snprintf(err, errlen, "pico8_64 has no main"); return -1; }

    // argv: pointers then strings, in the shim
    uint64_t *av = (uint64_t *)(P8_SHIM + 0xa0000);
    char *str = (char *)(av + 64);
    int n = 0;
    for (int i = -1; i < argc && n < 62; i++) {
        const char *a = i < 0 ? exe : argv[i];
        size_t l = strlen(a) + 1;
        memcpy(str, a, l);
        av[n++] = (uint64_t)str;
        str += l;
    }
    av[n] = 0;
    av[n + 1] = 0;  // envp: empty

    // start: main(argc, argv, envp), then exit with what it returns
    uint32_t *t = (uint32_t *)P8_TRAMP;
    t[0] = 0xaa1403e0;  // mov x0, x20
    t[1] = 0xaa1503e1;  // mov x1, x21
    t[2] = 0xaa1603e2;  // mov x2, x22
    t[3] = 0xd63f0260;  // blr x19
    t[4] = 0x58000050;  // ldr x16, =p8_exit
    t[5] = 0xd61f0200;  // br x16
    *(uint64_t *)(t + 6) = (uint64_t)p8_exit;
    __builtin___clear_cache((char *)P8_TRAMP, (char *)(P8_TRAMP + 64));
    mprotect((void *)P8_TRAMP, 0x10000, PROT_READ | PROT_EXEC);

    memset(&SHIM->ctx, 0, sizeof SHIM->ctx);
    SHIM->ctx.x[0] = main_fn;
    SHIM->ctx.x[1] = (uint64_t)n;
    SHIM->ctx.x[2] = (uint64_t)av;
    SHIM->ctx.x[3] = (uint64_t)(av + n + 1);
    SHIM->ctx.lr = P8_TRAMP;
    SHIM->ctx.sp = P8_STACK_TOP - 256;
    booted = 1;
    exited = 0;
    return 0;
}

// PICO-8 sets up Splore's lists (favourites.txt and the rest) as it boots,
// but not for a cart it runs straight away (-run): the pause menu's
// Favourite then adds to a list never made, and crashes, and its heart
// can't show a favourite. They are set up once the cart runs; that only
// reads PICO-8's own files. A state from before this has none either.
// Favourite saves what Splore knows of the running cart (its title and
// author), which Splore would have set as it started a BBS cart.
static void splore_ready(void) {
    if (!splore_init || !SHIM->frames || *(volatile int32_t *)splore_inited) return;
    p8_call_on(P8_CBSTACK, (void *)splore_init, 0, 0, 0);
    if (cart_info[0] && load_config && set_running_item) {
        uint64_t nfo = p8_call_on(P8_CBSTACK, (void *)load_config, (uint64_t)cart_info, 0, 0);
        if (nfo) p8_call_on(P8_CBSTACK, (void *)set_running_item, nfo, 0, 0);
    }
}

void p8_set_cart_info(const char *nfo_path) {
    snprintf(cart_info, sizeof cart_info, "%s", nfo_path ? nfo_path : "");
}

// Run PICO-8 up to its next presented frame.
int p8_frame(const uint8_t *keys) {
    if (!booted || exited) return P8_EXIT;
    splore_ready();
    if (keys) p8_set_keys(keys);
    *(uint64_t *)P8_STACK_LO = p8_get_x18();  // for the import thunks
    uint64_t r = p8_switch(&p8_host_ctx, &SHIM->ctx, 0);
    if (r == P8_EXIT) exited = 1;
    return (int)r;
}

int p8_exited(void) { return exited; }

// The picture: 32-bit pixels, ARGB in memory order B,G,R,A.
const uint8_t *p8_screen(int *w, int *h, int *pitch) {
    if (SHIM->frame_kind == 1 && SHIM->copy_tex) {
        struct p8tex *t = SHIM->copy_tex;
        int x = SHIM->copy_src[0], y = SHIM->copy_src[1];
        *w = SHIM->copy_src[2];
        *h = SHIM->copy_src[3];
        *pitch = t->pitch;
        return t->pixels + (size_t)y * t->pitch + (size_t)x * 4;
    }
    if (SHIM->frame_kind == 2 && SHIM->frame_surface) {
        struct { uint32_t flags; void *fmt; int32_t w, h, pitch; uint8_t *pixels; } *s = (void *)SHIM->frame_surface;
        *w = s->w; *h = s->h; *pitch = s->pitch;
        return s->pixels;
    }
    return NULL;
}

int p8_audio_rate(void) { return SHIM->audio_open ? SHIM->audio_freq : 0; }

// Mix `frames` stereo-or-mono frames of 16-bit audio into out (as PICO-8 opened it).
int p8_audio(int16_t *out, int frames) {
    int ch = SHIM->audio_channels ? SHIM->audio_channels : 1;
    if (!SHIM->audio_open || SHIM->audio_paused || !SHIM->audio_callback || exited) {
        memset(out, 0, (size_t)frames * ch * 2);
        return ch;
    }
    p8_call_on(P8_CBSTACK, (void *)SHIM->audio_callback, SHIM->audio_userdata, (uint64_t)out,
               (uint64_t)frames * ch * 2);
    return ch;
}

/* ---- save states ---- */

#define STATE_MAGIC 0x54533850u  /* "P8ST" */
#define STATE_VERSION 1
#define STACK_GRAIN 0x10000ULL
#define HEAP_GRAIN 0x400000ULL
struct state_header {
    uint32_t magic, version;
    uint64_t image_hash;
    uint64_t rw_len, shim_len, stack_len, heap_len;
};

static void layout(struct state_header *h) {
    h->magic = STATE_MAGIC;
    h->version = STATE_VERSION;
    h->image_hash = img.hash;
    h->rw_len = img.rw_end - img.rw_start;
    h->shim_len = sizeof(struct p8shim);
    uint64_t sp = SHIM->ctx.sp & ~0xfULL;
    h->stack_len = (P8_STACK_TOP - sp + STACK_GRAIN - 1) & ~(STACK_GRAIN - 1);
    h->heap_len = (p8_heap_used() + HEAP_GRAIN - 1) & ~(HEAP_GRAIN - 1);
}

size_t p8_state_size(void) {
    if (!booted) return 0;
    struct state_header h;
    layout(&h);
    return sizeof h + h.rw_len + h.shim_len + h.stack_len + h.heap_len;
}

int p8_state_save(void *buf, size_t len) {
    if (!booted || exited) return -1;
    // A thread (a download) changes memory as it is copied: no state then.
    if (__atomic_load_n(&p8_threads, __ATOMIC_SEQ_CST)) {
        static int told;
        if (!told++) p8_logf("pico8: no state while PICO-8 is downloading");
        return -1;
    }
    struct state_header h;
    layout(&h);
    if (len < sizeof h + h.rw_len + h.shim_len + h.stack_len + h.heap_len) return -1;
    p8_files_save_positions();
    uint8_t *p = buf;
    memcpy(p, &h, sizeof h); p += sizeof h;
    memcpy(p, (void *)img.rw_start, h.rw_len); p += h.rw_len;
    memcpy(p, (void *)P8_SHIM, h.shim_len); p += h.shim_len;
    memcpy(p, (void *)(P8_STACK_TOP - h.stack_len), h.stack_len); p += h.stack_len;
    memcpy(p, (void *)P8_HEAP, h.heap_len);
    // a larger buffer (a state size asked for before the heap shrank) is padded
    p += h.heap_len;
    memset(p, 0, len - (size_t)(p - (uint8_t *)buf));
    return 0;
}

int p8_state_load(const void *buf, size_t len) {
    if (!booted) return -1;
    struct state_header h;
    if (len < sizeof h) return -1;
    memcpy(&h, buf, sizeof h);
    if (h.magic != STATE_MAGIC || h.version != STATE_VERSION) { p8_logf("pico8: not a PICO-8 state"); return -1; }
    if (h.image_hash != img.hash) { p8_logf("pico8: state is from another pico8_64"); return -1; }
    if (h.rw_len != img.rw_end - img.rw_start || h.shim_len != sizeof(struct p8shim) || h.stack_len > P8_STACK_SIZE - 0x10000 ||
        h.heap_len > P8_HEAP_MAX || len < sizeof h + h.rw_len + h.shim_len + h.stack_len + h.heap_len) {
        p8_logf("pico8: state has another layout (%zu bytes)", len);
        return -1;
    }
    // A running thread would write over what the state brings back.
    if (!threads_done(5000)) { p8_logf("pico8: state not loaded: PICO-8 is still downloading"); return -1; }
    const uint8_t *p = (const uint8_t *)buf + sizeof h;
    memcpy((void *)img.rw_start, p, h.rw_len); p += h.rw_len;
    memcpy((void *)P8_SHIM, p, h.shim_len); p += h.shim_len;
    memcpy((void *)(P8_STACK_TOP - h.stack_len), p, h.stack_len); p += h.stack_len;
    memcpy((void *)P8_HEAP, p, h.heap_len);
    p8_rebind_imports(p8_resolve);
    p8_imports_after_load();
    p8_heap_after_load();
    exited = 0;
    return 0;
}

// Everything goes; PICO-8's threads (Splore's downloads) finish first. One
// still running keeps PICO-8's memory until the next boot waits for it.
void p8_shutdown(void) {
    if (!booted) return;
    booted = 0;
    if (!threads_done(3000)) {
        p8_logf("pico8: a download is still running; PICO-8's memory stays");
        return;
    }
    p8_unload_image();
    p8_heap_release();
    if (mapped) {
        munmap((void *)P8_TRAMP, 0x10000);
        munmap((void *)P8_SHIM, P8_SHIM_SIZE);
        munmap((void *)P8_STACK_LO, P8_STACK_SIZE);
        munmap((void *)P8_CBSTACK, P8_STACK_ALIGN);
        munmap((void *)P8_WSTACKS, P8_WSTACK_N * P8_STACK_ALIGN);
        mapped = 0;
    }
    memset(&img, 0, sizeof img);
}
