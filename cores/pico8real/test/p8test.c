// Runs PICO-8 headless under the core's engine: boots, steps frames, dumps
// pictures, tests save states. Built static for aarch64 (NDK) and run
// under qemu-aarch64 on the build box.
//
//   p8test PICO8_64 HOME FRAMES [options] [-- pico8 args]
//   options: -dump F,F,...   write frame-F.ppm for those frames
//            -keys F:SC:D,...  at frame F set scancode SC down (D=1) or up (0)
//            -save F FILE     write a state at frame F
//            -load FILE       load a state before the first frame
//            -rewind A B      at frame B, load the state from frame A, replay to B, compare
#include "../src/p8.h"

#include <stdlib.h>
#include <string.h>
#include <time.h>

static long frame;
static void logline(const char *s) { fprintf(stderr, "[%ld] %s\n", frame, s); }
#include <sys/wait.h>
#include <unistd.h>
static int http(const char *url, const char *post, const char *out) {
    // the build box's curl (a native binary: qemu-user execs it directly)
    pid_t pid = fork();
    if (!pid) {
        if (post) {
            char data[600];
            snprintf(data, sizeof data, "@%s", post);
            execl("/usr/bin/curl", "curl", "-sfL", "--data-binary", data, "-o", out, url, (char *)NULL);
        }
        execl("/usr/bin/curl", "curl", "-sfL", "-o", out, url, (char *)NULL);
        _exit(127);
    }
    int st = 0;
    waitpid(pid, &st, 0);
    fprintf(stderr, "[%ld] http %s -> %s: %d\n", frame, url, out, st);
    return st ? -1 : 0;
}
static double now(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec + t.tv_nsec / 1e9;
}
static uint64_t screen_hash(void) {
    int w, h, pitch;
    const uint8_t *p = p8_screen(&w, &h, &pitch);
    if (!p) return 0;
    uint64_t x = 0xcbf29ce484222325ULL;
    for (int y = 0; y < h; y++)
        for (int i = 0; i < w * 4; i++) { x ^= p[y * pitch + i]; x *= 0x100000001b3ULL; }
    return x;
}
static void dump(long f) {
    int w, h, pitch;
    const uint8_t *p = p8_screen(&w, &h, &pitch);
    if (!p) { fprintf(stderr, "frame %ld: no picture\n", f); return; }
    char name[64];
    snprintf(name, sizeof name, "frame-%ld.ppm", f);
    FILE *o = fopen(name, "wb");
    fprintf(o, "P6 %d %d 255\n", w, h);
    for (int y = 0; y < h; y++)
        for (int x = 0; x < w; x++) {
            const uint8_t *px = p + y * pitch + x * 4;
            fputc(px[2], o); fputc(px[1], o); fputc(px[0], o);
        }
    fclose(o);
    fprintf(stderr, "frame %ld: %dx%d -> %s\n", f, w, h, name);
}
static int in_list(const char *list, long f) {
    if (!list) return 0;
    for (const char *p = list; *p;) {
        if (strtol(p, NULL, 10) == f) return 1;
        p = strchr(p, ',');
        if (!p) break;
        p++;
    }
    return 0;
}

int main(int argc, char **argv) {
    if (argc < 4) { fprintf(stderr, "usage: p8test PICO8_64 HOME FRAMES [options] [-- args]\n"); return 2; }
    const char *exe = argv[1], *home = argv[2];
    long frames = atol(argv[3]);
    const char *dumps = NULL, *keys = NULL, *save_file = NULL, *load_file = NULL;
    long save_at = -1, rw_a = -1, rw_b = -1;
    int i = 4;
    for (; i < argc; i++) {
        if (!strcmp(argv[i], "-dump")) dumps = argv[++i];
        else if (!strcmp(argv[i], "-keys")) keys = argv[++i];
        else if (!strcmp(argv[i], "-save")) { save_at = atol(argv[++i]); save_file = argv[++i]; }
        else if (!strcmp(argv[i], "-load")) load_file = argv[++i];
        else if (!strcmp(argv[i], "-rewind")) { rw_a = atol(argv[++i]); rw_b = atol(argv[++i]); }
        else if (!strcmp(argv[i], "--")) { i++; break; }
    }
    extern int p8_trace_big;
    p8_trace_big = getenv("P8_TRACE") != NULL;
    p8_log = logline;
    p8_http_get = http;
    char err[256];
    if (p8_boot(exe, home, argc - i, (const char *const *)argv + i, err, sizeof err)) {
        fprintf(stderr, "boot: %s\n", err);
        return 1;
    }
    if (load_file) {
        FILE *f = fopen(load_file, "rb");
        fseek(f, 0, SEEK_END);
        long n = ftell(f);
        fseek(f, 0, SEEK_SET);
        void *b = malloc(n);
        fread(b, 1, n, f);
        fclose(f);
        // the state needs one frame of boot first so the image is loaded; it is
        if (p8_state_load(b, n)) { fprintf(stderr, "load failed\n"); return 1; }
        fprintf(stderr, "loaded %s (%ld bytes)\n", load_file, n);
        free(b);
    }
    static uint8_t held[P8_KEYS];
    void *rw_state = NULL;
    size_t rw_len = 0;
    uint64_t rw_hash = 0;
    double t0 = now(), worst = 0;
    int16_t audio[2048 * 2];
    uint64_t audio_sum = 0;
    for (frame = 1; frame <= frames; frame++) {
        if (keys)
            for (const char *p = keys; *p;) {
                long f = strtol(p, (char **)&p, 10);
                long sc = strtol(p + 1, (char **)&p, 10);
                long d = strtol(p + 1, (char **)&p, 10);
                if (f == frame && sc >= 0 && sc < P8_KEYS) held[sc] = (uint8_t)d;
                if (*p == ',') p++;
            }
        double a = now();
        int r = p8_frame(held);
        double dt = now() - a;
        if (dt > worst && frame > 5) worst = dt;
        int rate = p8_audio_rate();
        if (rate) {
            int n = rate / 60;
            int ch = p8_audio(audio, n);
            for (int k = 0; k < n * ch; k++) audio_sum += (uint64_t)(audio[k] < 0 ? -audio[k] : audio[k]);
        }
        if (r == P8_EXIT) { fprintf(stderr, "exit at frame %ld (code %d)\n", frame, p8_exit_code); break; }
        if (in_list(dumps, frame)) dump(frame);
        if (frame == save_at) {
            size_t n = p8_state_size();
            void *b = malloc(n);
            if (p8_state_save(b, n)) fprintf(stderr, "save failed\n");
            FILE *f = fopen(save_file, "wb");
            fwrite(b, 1, n, f);
            fclose(f);
            fprintf(stderr, "frame %ld: state %zu bytes -> %s, screen %016llx\n", frame, n, save_file,
                    (unsigned long long)screen_hash());
            free(b);
        }
        if (frame == rw_a) {
            rw_len = p8_state_size();
            rw_state = malloc(rw_len);
            double s0 = now();
            p8_state_save(rw_state, rw_len);
            fprintf(stderr, "frame %ld: rewind state %zu bytes in %.2f ms\n", frame, rw_len, (now() - s0) * 1e3);
        }
        if (frame == rw_b && rw_state && !rw_hash) {
            rw_hash = screen_hash();
            double s0 = now();
            if (p8_state_load(rw_state, rw_len)) { fprintf(stderr, "rewind load failed\n"); return 1; }
            fprintf(stderr, "frame %ld: screen %016llx; loaded frame %ld in %.2f ms\n", frame,
                    (unsigned long long)rw_hash, rw_a, (now() - s0) * 1e3);
            frame = rw_a;
        } else if (frame == rw_b && rw_hash) {
            uint64_t h2 = screen_hash();
            fprintf(stderr, "frame %ld after replay: screen %016llx %s\n", frame, (unsigned long long)h2,
                    h2 == rw_hash ? "MATCH" : "DIFFERS");
        }
    }
    double el = now() - t0;
    fprintf(stderr, "%ld frames in %.2f s (%.2f ms/frame, worst %.2f ms), vclock %u ms, presents %llu, audio %llu\n",
            frame - 1, el, el * 1e3 / (frame - 1 > 0 ? frame - 1 : 1), worst * 1e3, SHIM->vclock_ms,
            (unsigned long long)SHIM->frames, (unsigned long long)audio_sum);
    return 0;
}
