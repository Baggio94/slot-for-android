// pico8_64's libc imports, bound to Android's libc. Most are the same
// function; the rest are wrapped here because the glibc symbol has no bionic
// twin (__xstat, __ctype_b_loc, ...), because PICO-8 may keep the result
// across frames (FILE*, malloc, getenv) and a save state must not hold a
// pointer into this process, or because the host decides (wget, the clock).
#define _GNU_SOURCE
#include "p8.h"

#include <ctype.h>
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <locale.h>
#include <math.h>
#include <stdarg.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
#include <utime.h>

void (*p8_log)(const char *line);
int p8_log_fd = 2;
void p8_log_raw(const char *s, size_t n) {
    if (p8_log_fd >= 0) { ssize_t r = write(p8_log_fd, s, n); (void)r; }
}
int (*p8_http_get)(const char *url, const char *post_path, const char *out_path);

void p8_logf(const char *fmt, ...) {
    char buf[1024];
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(buf, sizeof buf, fmt, ap);
    va_end(ap);
    if (p8_log) p8_log(buf);
    else fprintf(stderr, "%s\n", buf);
}

/* ---- glibc's ctype tables, at a fixed address ---- */

struct p8ctype {
    const uint16_t *b_ptr;
    const int32_t *lower_ptr, *upper_ptr;
    uint16_t b[384];
    int32_t lower[384], upper[384];
};
#define CTYPE ((struct p8ctype *)(P8_SHIM + 0x80000))

static void ctype_init(void) {
    struct p8ctype *c = CTYPE;
    for (int i = -128; i < 256; i++) {
        uint16_t v = 0;
        int ch = i & 0xff;
        if (i >= 0 && i < 128) {
            if (isupper(ch)) v |= 0x100;
            if (islower(ch)) v |= 0x200;
            if (isalpha(ch)) v |= 0x400;
            if (isdigit(ch)) v |= 0x800;
            if (isxdigit(ch)) v |= 0x1000;
            if (isspace(ch)) v |= 0x2000;
            if (isprint(ch)) v |= 0x4000;
            if (isgraph(ch)) v |= 0x8000;
            if (ch == ' ' || ch == '\t') v |= 0x1;
            if (iscntrl(ch)) v |= 0x2;
            if (ispunct(ch)) v |= 0x4;
            if (isalnum(ch)) v |= 0x8;
        }
        c->b[i + 128] = v;
        c->lower[i + 128] = (i >= 'A' && i <= 'Z') ? i + 32 : i;
        c->upper[i + 128] = (i >= 'a' && i <= 'z') ? i - 32 : i;
    }
    c->b_ptr = c->b + 128;
    c->lower_ptr = c->lower + 128;
    c->upper_ptr = c->upper + 128;
}
static const uint16_t **w_ctype_b_loc(void) { return &CTYPE->b_ptr; }
static const int32_t **w_ctype_tolower_loc(void) { return &CTYPE->lower_ptr; }
static const int32_t **w_ctype_toupper_loc(void) { return &CTYPE->upper_ptr; }

/* ---- files: PICO-8 holds handles in its heap, the host FILE* beside ---- */

#define P8FILE_MAGIC 0x50384649u
enum { F_FILE, F_LOG, F_NULL };
struct p8file {
    uint32_t magic;
    int32_t kind;
    FILE *host;
    int64_t pos;
    uint32_t gen;
    char mode[8];
    char path[512];
    struct p8file *next;
};
// Which process, and which load in it, a handle's host FILE* belongs to: a
// new random value at start and on each state load, so a state made in
// another process (which counted its loads the same way) never matches.
uint32_t p8_generation;

static char logline[2][512];
static size_t loglen[2];
static void log_put(int which, const char *s, size_t n) {
    for (size_t i = 0; i < n; i++) {
        char c = s[i];
        if (c == '\n' || loglen[which] == sizeof logline[0] - 1) {
            logline[which][loglen[which]] = 0;
            p8_logf("pico8%s: %s", which ? "!" : "", logline[which]);
            loglen[which] = 0;
            if (c == '\n') continue;
        }
        logline[which][loglen[which]++] = c;
    }
}

static struct p8file *file_new(int kind, FILE *host, const char *path, const char *mode) {
    struct p8file *f = p8_calloc(1, sizeof *f);
    if (!f) return NULL;
    f->magic = P8FILE_MAGIC;
    f->kind = kind;
    f->host = host;
    f->gen = p8_generation;
    snprintf(f->path, sizeof f->path, "%s", path ? path : "");
    snprintf(f->mode, sizeof f->mode, "%s", mode ? mode : "");
    f->next = SHIM->files;
    SHIM->files = f;
    return f;
}

static void file_unlink(struct p8file *f) {
    for (struct p8file **p = &SHIM->files; *p; p = &(*p)->next)
        if (*p == f) { *p = f->next; break; }
}

// The host FILE behind a handle, reopened if it came from a save state.
static FILE *hostf(void *h) {
    struct p8file *f = h;
    if (!f || f->magic != P8FILE_MAGIC) return NULL;
    if (f->kind != F_FILE) return NULL;
    if (f->gen != p8_generation) {
        f->gen = p8_generation;
        f->host = NULL;
        if (f->path[0]) {
            const char *m = f->mode[0] == 'w' ? (strchr(f->mode, 'b') ? "r+b" : "r+") : f->mode;
            f->host = fopen(f->path, m);
            if (!f->host && f->mode[0] == 'w') f->host = fopen(f->path, f->mode);
            if (f->host) fseeko(f->host, f->pos, SEEK_SET);
        }
        if (!f->host) p8_logf("pico8: could not reopen %s", f->path);
    }
    return f->host;
}

void p8_files_save_positions(void) {
    for (struct p8file *f = SHIM->files; f; f = f->next)
        if (f->kind == F_FILE && f->gen == p8_generation && f->host) {
            fflush(f->host);
            f->pos = ftello(f->host);
        }
}

void p8_files_after_load(void) {
    uint32_t old = p8_generation;
    do p8_generation = arc4random(); while (!p8_generation || p8_generation == old);
}

static int is_log(void *h) { struct p8file *f = h; return f && f->magic == P8FILE_MAGIC && f->kind == F_LOG; }
static int log_which(void *h) { return h == (void *)SHIM->std_files[2]; }

static void *w_fopen(const char *path, const char *mode) {
    FILE *h = fopen(path, mode);
    if (!h) return NULL;
    struct p8file *f = file_new(F_FILE, h, path, mode);
    if (!f) { fclose(h); errno = ENOMEM; }
    return f;
}
static void *w_fdopen(int fd, const char *mode) {
    FILE *h = fdopen(fd, mode);
    if (!h) return NULL;
    char path[512] = "", link[64];
    snprintf(link, sizeof link, "/proc/self/fd/%d", fd);
    ssize_t n = readlink(link, path, sizeof path - 1);
    path[n > 0 ? n : 0] = 0;
    return file_new(F_FILE, h, path, mode[0] == 'w' ? "r+" : mode);
}
static void *w_freopen(const char *path, const char *mode, void *h) {
    struct p8file *f = h;
    if (!f || f->magic != P8FILE_MAGIC) return NULL;
    FILE *old = hostf(f);
    if (old) fclose(old);
    FILE *n = path ? fopen(path, mode) : NULL;
    p8_logf("pico8: freopen %s %s -> %s", path ? path : "(null)", mode, n ? "ok" : "failed");
    if (!n) { file_unlink(f); f->magic = 0; return NULL; }
    if (f->kind != F_FILE) {  // a std stream: becomes a file; the slot keeps pointing at it
        f->kind = F_FILE;
        f->next = SHIM->files;
        SHIM->files = f;
    }
    f->host = n;
    f->gen = p8_generation;
    snprintf(f->path, sizeof f->path, "%s", path);
    snprintf(f->mode, sizeof f->mode, "%s", mode);
    return f;
}
static int w_fclose(void *h) {
    struct p8file *f = h;
    if (!f || f->magic != P8FILE_MAGIC) return EOF;
    if (f->kind != F_FILE) return 0;
    FILE *x = hostf(f);
    file_unlink(f);
    f->magic = 0;
    p8_free(f);
    return x ? fclose(x) : EOF;
}
static size_t w_fread(void *p, size_t s, size_t n, void *h) { FILE *x = hostf(h); return x ? fread(p, s, n, x) : 0; }
static size_t w_fwrite(const void *p, size_t s, size_t n, void *h) {
    if (is_log(h)) { log_put(log_which(h), p, s * n); return n; }
    FILE *x = hostf(h);
    return x ? fwrite(p, s, n, x) : 0;
}
static int w_fgetc(void *h) { FILE *x = hostf(h); return x ? fgetc(x) : EOF; }
static char *w_fgets(char *s, int n, void *h) { FILE *x = hostf(h); return x ? fgets(s, n, x) : NULL; }
static int w_fputc(int c, void *h) {
    if (is_log(h)) { char ch = c; log_put(log_which(h), &ch, 1); return c & 0xff; }
    FILE *x = hostf(h);
    return x ? fputc(c, x) : EOF;
}
static int w_fputs(const char *s, void *h) {
    if (is_log(h)) { log_put(log_which(h), s, strlen(s)); return 1; }
    FILE *x = hostf(h);
    return x ? fputs(s, x) : EOF;
}
static int w_fprintf(void *h, const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int r;
    if (is_log(h)) {
        char buf[2048];
        r = vsnprintf(buf, sizeof buf, fmt, ap);
        log_put(log_which(h), buf, strnlen(buf, sizeof buf));
    } else {
        FILE *x = hostf(h);
        r = x ? vfprintf(x, fmt, ap) : -1;
    }
    va_end(ap);
    return r;
}
static int w_printf(const char *fmt, ...) {
    char buf[2048];
    va_list ap;
    va_start(ap, fmt);
    int r = vsnprintf(buf, sizeof buf, fmt, ap);
    va_end(ap);
    log_put(0, buf, strnlen(buf, sizeof buf));
    return r;
}
static int w_puts(const char *s) { log_put(0, s, strlen(s)); log_put(0, "\n", 1); return 1; }
static int w_putchar(int c) { char ch = c; log_put(0, &ch, 1); return c & 0xff; }
static int w_feof(void *h) { FILE *x = hostf(h); return x ? feof(x) : 1; }
static int w_ferror(void *h) { FILE *x = hostf(h); return x ? ferror(x) : 0; }
static int w_fflush(void *h) { if (!h) return 0; FILE *x = hostf(h); return x ? fflush(x) : 0; }
static int w_fseek(void *h, long o, int w) { FILE *x = hostf(h); return x ? fseek(x, o, w) : -1; }
static int w_fseeko(void *h, off_t o, int w) { FILE *x = hostf(h); return x ? fseeko(x, o, w) : -1; }
static long w_ftell(void *h) { FILE *x = hostf(h); return x ? ftell(x) : -1; }
static off_t w_ftello(void *h) { FILE *x = hostf(h); return x ? ftello(x) : -1; }
static void w_rewind(void *h) { FILE *x = hostf(h); if (x) rewind(x); }

/* ---- the rest of libc ---- */

static int w_xstat(int ver, const char *path, struct stat *st) { (void)ver; return stat(path, st); }
static int *w_errno_location(void) { return &errno; }
static void w_assert_fail(const char *a, const char *file, unsigned line, const char *fn) {
    p8_logf("pico8: assertion failed: %s (%s:%u %s)", a, file, line, fn);
    p8_exit(134);
}
static void w_abort(void) {
    p8_logf("pico8: abort()");
    p8_exit(134);
}
static void w_exit(int code) {
    p8_logf("pico8: exit(%d)", code);
    p8_exit(code);
}
static int w_libc_start_main(void) {
    p8_logf("pico8: __libc_start_main called");
    p8_exit(1);
}

static char *w_getenv(const char *name) {
    if (!strcmp(name, "HOME")) return SHIM->home;
    return NULL;
}
static int w_setenv(const char *n, const char *v, int o) { (void)n; (void)v; (void)o; return 0; }
static ssize_t w_readlink(const char *path, char *buf, size_t n) {
    if (!strcmp(path, "/proc/self/exe")) {
        size_t l = strlen(SHIM->exe);
        if (l > n) l = n;
        memcpy(buf, SHIM->exe, l);
        return l;
    }
    return readlink(path, buf, n);
}
static char *w_getcwd(char *buf, size_t n) {
    size_t l = strlen(SHIM->home) + 1;
    if (!buf) { buf = p8_malloc(n > l ? n : l); if (!buf) return NULL; n = n > l ? n : l; }
    if (l > n) { errno = ERANGE; return NULL; }
    memcpy(buf, SHIM->home, l);
    return buf;
}

static int w_rand(void) {
    SHIM->rand_state = SHIM->rand_state * 1103515245u + 12345u;
    uint32_t a = SHIM->rand_state >> 16;
    SHIM->rand_state = SHIM->rand_state * 1103515245u + 12345u;
    return (int)((a << 15 | (SHIM->rand_state >> 17)) & 0x7fffffff);
}
static void w_srand(unsigned s) { SHIM->rand_state = s; }

// Our own sort: bionic's would return from PICO-8's comparator with x18
// clobbered (no thunk is between them).
static void swap_bytes(uint8_t *a, uint8_t *b, size_t n) {
    while (n--) { uint8_t t = *a; *a++ = *b; *b++ = t; }
}
static void sift(uint8_t *base, size_t root, size_t n, size_t sz, int (*cmp)(const void *, const void *)) {
    for (;;) {
        size_t child = 2 * root + 1;
        if (child >= n) return;
        if (child + 1 < n && cmp(base + child * sz, base + (child + 1) * sz) < 0) child++;
        if (cmp(base + root * sz, base + child * sz) >= 0) return;
        swap_bytes(base + root * sz, base + child * sz, sz);
        root = child;
    }
}
static void w_qsort(void *b, size_t n, size_t sz, int (*cmp)(const void *, const void *)) {
    uint8_t *base = b;
    if (n < 2 || !sz) return;
    if (n <= 16) {  // insertion sort, stable for short lists
        for (size_t i = 1; i < n; i++)
            for (size_t j = i; j > 0 && cmp(base + (j - 1) * sz, base + j * sz) > 0; j--)
                swap_bytes(base + (j - 1) * sz, base + j * sz, sz);
        return;
    }
    for (size_t i = n / 2; i-- > 0;) sift(base, i, n, sz, cmp);
    for (size_t end = n - 1; end > 0; end--) {
        swap_bytes(base, base + end * sz, sz);
        sift(base, 0, end, sz, cmp);
    }
}

// wget "URL" -q -O "FILE" [--post-file=FILE]: fetched by the host.
static const char *quoted(const char *p, char *out, size_t n) {
    while (*p == ' ') p++;
    size_t i = 0;
    if (*p == '"') {
        p++;
        while (*p && *p != '"') { if (i + 1 < n) out[i++] = *p; p++; }
        if (*p) p++;
    } else {
        while (*p && *p != ' ') { if (i + 1 < n) out[i++] = *p; p++; }
    }
    out[i] = 0;
    return p;
}
static int w_system(const char *cmd) {
    if (!cmd) return 0;
    if (strncmp(cmd, "wget ", 5)) {
        p8_logf("pico8: system(%s) refused", cmd);
        return 127 << 8;
    }
    char url[1024] = "", out[512] = "", post[512] = "", tok[1024];
    const char *p = quoted(cmd + 5, url, sizeof url);
    while (*p) {
        p = quoted(p, tok, sizeof tok);
        if (!strcmp(tok, "-O")) p = quoted(p, out, sizeof out);
        else if (!strncmp(tok, "--post-file=", 12)) snprintf(post, sizeof post, "%s", tok + 12);
        else if (!tok[0]) break;
    }
    if (!url[0] || !out[0] || !p8_http_get) { p8_logf("pico8: wget not handled: %s", cmd); return 1 << 8; }
    int r = p8_http_get(url, post[0] ? post : NULL, out);
    return r ? (4 << 8) : 0;  // wget's "network failure"
}

#define F(name) { #name, (void *)name }
#define W(name, fn) { #name, (void *)fn }
static const struct { const char *name; void *fn; } table[] = {
    W(__assert_fail, w_assert_fail), W(__ctype_b_loc, w_ctype_b_loc), W(__ctype_tolower_loc, w_ctype_tolower_loc),
    W(__ctype_toupper_loc, w_ctype_toupper_loc), W(__errno_location, w_errno_location), W(__isoc99_sscanf, sscanf),
    W(__libc_start_main, w_libc_start_main), W(__xstat64, w_xstat), W(__xstat, w_xstat), W(_setjmp, p8_setjmp),
    W(longjmp, p8_longjmp), W(abort, w_abort), W(exit, w_exit),
    F(acos), F(asin), F(atan2), F(atan), F(cos), F(cosh), F(exp), F(fmod), F(frexp), F(ldexp), F(log10), F(log),
    F(pow), F(sin), F(sincos), F(sinh), F(sqrt), F(sqrtf), F(tan), F(tanh),
    W(malloc, p8_malloc), W(calloc, p8_calloc), W(realloc, p8_realloc), W(free, p8_free),
    F(chmod), F(clock), F(clock_gettime), F(close), F(closedir), F(difftime), F(gmtime), F(localeconv), F(localtime),
    F(memchr), F(memcmp), F(memcpy), F(memmove), F(memset), F(mkdir), F(mkstemp), F(mktime), F(open), F(opendir),
    W(qsort, w_qsort), F(readdir), F(remove), F(rename), F(setlocale), F(sprintf), F(stpcpy), F(strcasecmp), F(strcat),
    F(strchr), F(strcmp), F(strcoll), F(strcpy), F(strerror), F(strftime), F(strlen), F(strncmp), F(strncpy),
    F(strpbrk), F(strspn), F(strstr), F(strtod), F(strtok), F(time), F(utime),
    W(fopen, w_fopen), W(fopen64, w_fopen), W(fdopen, w_fdopen), W(freopen, w_freopen), W(freopen64, w_freopen),
    W(fclose, w_fclose), W(feof, w_feof), W(ferror, w_ferror), W(fflush, w_fflush), W(fgetc, w_fgetc),
    W(getc, w_fgetc), W(fgets, w_fgets), W(fprintf, w_fprintf), W(fputc, w_fputc), W(fputs, w_fputs),
    W(fread, w_fread), W(fwrite, w_fwrite), W(fseek, w_fseek), W(fseeko64, w_fseeko), W(ftell, w_ftell),
    W(ftello64, w_ftello), W(rewind, w_rewind), W(printf, w_printf), W(puts, w_puts), W(putchar, w_putchar),
    W(getenv, w_getenv), W(setenv, w_setenv), W(readlink, w_readlink), W(getcwd, w_getcwd), W(rand, w_rand),
    W(srand, w_srand), W(system, w_system),
};

void *p8_resolve(const char *name) {
    if (!strncmp(name, "SDL_", 4)) return p8_sdl_resolve(name);
    if (!strcmp(name, "stdin")) return &SHIM->std_files[0];
    if (!strcmp(name, "stdout")) return &SHIM->std_files[1];
    if (!strcmp(name, "stderr")) return &SHIM->std_files[2];
    for (size_t i = 0; i < sizeof table / sizeof table[0]; i++)
        if (!strcmp(table[i].name, name)) return table[i].fn;
    return NULL;
}

// Once per boot, after the heap exists.
void p8_imports_init(void) {
    ctype_init();
    for (int i = 0; i < 3; i++)
        SHIM->std_files[i] = (uint64_t)file_new(i == 0 ? F_NULL : F_LOG, NULL, NULL, NULL);
    // the std streams are not files: keep them off the reopen list
    SHIM->files = NULL;
}

// After loading a state in this process: the tables live outside the saved shim.
void p8_imports_after_load(void) {
    ctype_init();
    p8_files_after_load();
}
