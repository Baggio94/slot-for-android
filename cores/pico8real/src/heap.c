// PICO-8's heap: a dlmalloc mspace at the fixed address P8_HEAP, so its
// contents (and every pointer into it) stay valid when a save state is
// loaded in a later process.
#define ONLY_MSPACES 1
#define MSPACES 1
#define USE_LOCKS 1
#define HAVE_MMAP 0
#define HAVE_MORECORE 0
#define MALLOC_ALIGNMENT 16
#define FOOTERS 0
#define P8_FIXED_MAGIC 0x5038485041ULL
#define ABORT p8_heap_abort()
#define MALLOC_FAILURE_ACTION
static void p8_heap_abort(void);
#include "../vendor/dlmalloc.c"

#include "p8.h"
#include <sys/mman.h>

static void p8_heap_abort(void) {
    p8_logf("pico8: heap corrupted");
    abort();
}

static int reserved;

int p8_heap_init(void) {
    if (!reserved) {
        if (p8_map_fixed(P8_HEAP, P8_HEAP_MAX, PROT_READ | PROT_WRITE, MAP_NORESERVE)) return -1;
        reserved = 1;
    } else {
        // fresh pages: drop whatever the last run left
        madvise((void *)P8_HEAP, P8_HEAP_MAX, MADV_DONTNEED);
    }
    SHIM->heap = (uint64_t)create_mspace_with_base((void *)P8_HEAP, P8_HEAP_MAX, 1);
    return SHIM->heap ? 0 : -1;
}

void p8_heap_release(void) {
    if (reserved) munmap((void *)P8_HEAP, P8_HEAP_MAX);
    reserved = 0;
}

int p8_trace_big;
#define TRACE(n, r) do { if (p8_trace_big && (n) >= 65536) p8_logf("heap: %zu at %p from %#llx", (size_t)(n), r, \
    (unsigned long long)((uint64_t)__builtin_return_address(0) - P8_IMAGE)); } while (0)
void *p8_malloc(size_t n) { void *r = mspace_malloc((mspace)SHIM->heap, n); TRACE(n, r); return r; }
void *p8_calloc(size_t n, size_t m) { void *r = mspace_calloc((mspace)SHIM->heap, n, m); TRACE(n * m, r); return r; }
void *p8_realloc(void *p, size_t n) { void *r = mspace_realloc((mspace)SHIM->heap, p, n); TRACE(n, r); return r; }
void p8_free(void *p) { if (p) mspace_free((mspace)SHIM->heap, p); }

// Bytes from P8_HEAP that hold anything: up to and including the top chunk's header.
size_t p8_heap_used(void) {
    mstate m = (mstate)SHIM->heap;
    return (size_t)((uint8_t *)m->top - (uint8_t *)P8_HEAP) + TOP_FOOT_SIZE + 2 * sizeof(size_t);
}

void p8_heap_after_load(void) {
    // A loaded heap may have used less than the one it replaced; the pages
    // past it are never read before being written, so nothing to do. Keep the
    // hook so the state code reads symmetrically.
}
