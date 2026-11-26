// A small ELF loader for pico8_64: maps it at P8_IMAGE, applies its
// relocations, binds its imports through a resolver, and keeps .symtab so
// functions can be found by name.
#include "p8.h"

#include <elf.h>
#include <errno.h>
#include <fcntl.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

#ifndef MAP_FIXED_NOREPLACE
#define MAP_FIXED_NOREPLACE 0x100000
#endif

struct import { uint64_t off, thunk; uint32_t type; uint8_t weak; char *name; };
static int nthunks;

// Calls go through a thunk at a fixed address (see switch.S): it restores
// the host's x18 first, and the GOT holds the same value in every process.
static uint64_t make_thunk(void *target) {
    size_t len = (size_t)(p8_thunk_template_end - p8_thunk_template);
    uint64_t at = P8_TRAMP + 0x100 + (uint64_t)nthunks * len;
    if (at + len > P8_TRAMP + 0x10000) return 0;
    nthunks++;
    memcpy((void *)at, p8_thunk_template, len);
    *(uint64_t *)(at + len - 8) = (uint64_t)target;
    return at;
}

static struct {
    uint8_t *file;
    size_t file_len;
    struct import *imports;
    int nimports;
    const Elf64_Sym *symtab;
    size_t nsyms;
    const char *strtab;
    uint64_t span;
} L;

static int fail(char *err, size_t n, const char *msg, const char *arg) {
    snprintf(err, n, "%s%s%s", msg, arg ? ": " : "", arg ? arg : "");
    return -1;
}

static uint64_t fnv1a(const uint8_t *p, size_t n) {
    uint64_t h = 0xcbf29ce484222325ULL;
    for (size_t i = 0; i < n; i++) { h ^= p[i]; h *= 0x100000001b3ULL; }
    return h;
}

static int bind_one(const struct import *im, p8_resolver resolve, char *missing, size_t mlen) {
    if (im->thunk) { *(uint64_t *)(P8_IMAGE + im->off) = im->thunk; return 0; }
    void *addr = resolve(im->name);
    if (!addr && !im->weak) {
        size_t l = strlen(missing);
        if (l + strlen(im->name) + 2 < mlen) { if (l) strcat(missing, " "); strcat(missing, im->name); }
        return -1;
    }
    *(uint64_t *)(P8_IMAGE + im->off) = (uint64_t)addr;
    return 0;
}

int p8_rebind_imports(p8_resolver resolve) {
    char missing[1024] = "";
    int bad = 0;
    for (int i = 0; i < L.nimports; i++) bad |= bind_one(&L.imports[i], resolve, missing, sizeof missing);
    if (bad) p8_logf("pico8: unresolved imports: %s", missing);
    return bad ? -1 : 0;
}

int p8_load_image(const char *path, struct p8image *img, p8_resolver resolve, char *err, size_t errlen) {
    int fd = open(path, O_RDONLY | O_CLOEXEC);
    if (fd < 0) return fail(err, errlen, "cannot open", path);
    struct stat st;
    if (fstat(fd, &st) || st.st_size < (off_t)sizeof(Elf64_Ehdr)) { close(fd); return fail(err, errlen, "cannot stat", path); }
    L.file_len = st.st_size;
    L.file = malloc(L.file_len);
    size_t got = 0;
    while (got < L.file_len) {
        ssize_t r = read(fd, L.file + got, L.file_len - got);
        if (r <= 0) break;
        got += r;
    }
    close(fd);
    if (got != L.file_len) return fail(err, errlen, "short read", path);

    const Elf64_Ehdr *eh = (const Elf64_Ehdr *)L.file;
    if (memcmp(eh->e_ident, ELFMAG, SELFMAG) || eh->e_ident[EI_CLASS] != ELFCLASS64 || eh->e_machine != EM_AARCH64 ||
        eh->e_type != ET_DYN)
        return fail(err, errlen, "not an aarch64 PIE", path);
    const Elf64_Phdr *ph = (const Elf64_Phdr *)(L.file + eh->e_phoff);
    uint64_t span = 0, rx_end = 0, rw_lo = UINT64_MAX, rw_hi = 0;
    const Elf64_Dyn *dyn = NULL;
    for (int i = 0; i < eh->e_phnum; i++) {
        if (ph[i].p_type == PT_LOAD) {
            uint64_t end = ph[i].p_vaddr + ph[i].p_memsz;
            if (end > span) span = end;
            if (ph[i].p_flags & PF_X) rx_end = ph[i].p_vaddr + ph[i].p_memsz;
            else if (ph[i].p_flags & PF_W) {
                if (ph[i].p_vaddr < rw_lo) rw_lo = ph[i].p_vaddr;
                if (end > rw_hi) rw_hi = end;
            }
        } else if (ph[i].p_type == PT_DYNAMIC) {
            dyn = (const Elf64_Dyn *)(L.file + ph[i].p_offset);
        } else if (ph[i].p_type == PT_TLS) {
            return fail(err, errlen, "TLS is not supported", path);
        }
    }
    span = (span + 0xffff) & ~0xffffULL;
    if (span > P8_IMAGE_MAX || !dyn || rw_lo == UINT64_MAX) return fail(err, errlen, "unexpected layout", path);
    L.span = span;

    if (p8_map_fixed(P8_IMAGE, span, PROT_READ | PROT_WRITE, 0))
        return fail(err, errlen, "address range is taken", strerror(errno));
    for (int i = 0; i < eh->e_phnum; i++)
        if (ph[i].p_type == PT_LOAD && ph[i].p_filesz)
            memcpy((void *)(P8_IMAGE + ph[i].p_vaddr), L.file + ph[i].p_offset, ph[i].p_filesz);

    // dynamic section
    uint64_t rela = 0, relasz = 0, jmprel = 0, pltrelsz = 0, symtab = 0, strtab = 0;
    for (const Elf64_Dyn *d = dyn; d->d_tag != DT_NULL; d++) {
        switch (d->d_tag) {
        case DT_RELA: rela = d->d_un.d_ptr; break;
        case DT_RELASZ: relasz = d->d_un.d_val; break;
        case DT_JMPREL: jmprel = d->d_un.d_ptr; break;
        case DT_PLTRELSZ: pltrelsz = d->d_un.d_val; break;
        case DT_SYMTAB: symtab = d->d_un.d_ptr; break;
        case DT_STRTAB: strtab = d->d_un.d_ptr; break;
        }
    }
    const Elf64_Sym *dsym = (const Elf64_Sym *)(P8_IMAGE + symtab);
    const char *dstr = (const char *)(P8_IMAGE + strtab);
    L.imports = calloc((relasz + pltrelsz) / sizeof(Elf64_Rela) + 1, sizeof(struct import));
    L.nimports = 0;
    nthunks = 0;
    char missing[1024] = "";
    int bad = 0;
    for (int pass = 0; pass < 2; pass++) {
        const Elf64_Rela *r = (const Elf64_Rela *)(P8_IMAGE + (pass ? jmprel : rela));
        size_t n = (pass ? pltrelsz : relasz) / sizeof(Elf64_Rela);
        for (size_t i = 0; i < n; i++) {
            uint32_t type = ELF64_R_TYPE(r[i].r_info), si = ELF64_R_SYM(r[i].r_info);
            uint64_t *where = (uint64_t *)(P8_IMAGE + r[i].r_offset);
            if (type == R_AARCH64_RELATIVE) {
                *where = P8_IMAGE + r[i].r_addend;
            } else if (type == R_AARCH64_JUMP_SLOT || type == R_AARCH64_GLOB_DAT || type == R_AARCH64_ABS64) {
                const Elf64_Sym *s = &dsym[si];
                if (s->st_shndx != SHN_UNDEF) {  // defined in pico8_64 itself
                    *where = P8_IMAGE + s->st_value + (type == R_AARCH64_ABS64 ? r[i].r_addend : 0);
                    continue;
                }
                struct import *im = &L.imports[L.nimports++];
                im->off = r[i].r_offset;
                im->type = type;
                im->weak = ELF64_ST_BIND(s->st_info) == STB_WEAK;
                im->name = strdup(dstr + s->st_name);
                void *target = type == R_AARCH64_JUMP_SLOT ? resolve(im->name) : NULL;
                if (target && !(im->thunk = make_thunk(target))) return fail(err, errlen, "too many imports", NULL);
                bad |= bind_one(im, resolve, missing, sizeof missing);
            } else {
                char t[32];
                snprintf(t, sizeof t, "%u", type);
                return fail(err, errlen, "unsupported relocation", t);
            }
        }
    }
    if (bad) return fail(err, errlen, "unresolved imports", missing);

    // .symtab, for finding functions by name
    const Elf64_Shdr *sh = (const Elf64_Shdr *)(L.file + eh->e_shoff);
    if (eh->e_shoff && eh->e_shnum)
        for (int i = 0; i < eh->e_shnum; i++)
            if (sh[i].sh_type == SHT_SYMTAB) {
                L.symtab = (const Elf64_Sym *)(L.file + sh[i].sh_offset);
                L.nsyms = sh[i].sh_size / sizeof(Elf64_Sym);
                L.strtab = (const char *)(L.file + sh[sh[i].sh_link].sh_offset);
            }

    uint64_t text_end = (rx_end + 0xffff) & ~0xffffULL;
    if (text_end > (rw_lo & ~0xffffULL)) text_end = rw_lo & ~0x3fffULL;
    __builtin___clear_cache((char *)P8_IMAGE, (char *)(P8_IMAGE + text_end));
    if (mprotect((void *)P8_IMAGE, text_end, PROT_READ | PROT_EXEC))
        return fail(err, errlen, "mprotect", strerror(errno));

    img->base = P8_IMAGE;
    img->span = span;
    img->rx_start = P8_IMAGE;
    img->rx_end = P8_IMAGE + text_end;
    img->rw_start = P8_IMAGE + (rw_lo & ~0xfffULL);
    img->rw_end = P8_IMAGE + rw_hi;
    img->hash = fnv1a(L.file, L.file_len);
    return 0;
}

uint64_t p8_symbol(const char *name, uint64_t *size) {
    for (size_t i = 0; i < L.nsyms; i++) {
        const Elf64_Sym *s = &L.symtab[i];
        if (s->st_shndx != SHN_UNDEF && s->st_value && !strcmp(L.strtab + s->st_name, name)) {
            if (size) *size = s->st_size;
            return P8_IMAGE + s->st_value;
        }
    }
    return 0;
}

void p8_unload_image(void) {
    if (L.span) munmap((void *)P8_IMAGE, L.span);
    for (int i = 0; i < L.nimports; i++) free(L.imports[i].name);
    free(L.imports);
    free(L.file);
    memset(&L, 0, sizeof L);
}
