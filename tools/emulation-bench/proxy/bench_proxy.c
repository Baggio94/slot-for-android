/*
 * Bench proxy core: a libretro core that loads the real core (BENCH_CORE) and
 * sits between it and the frontend (slot. or RetroArch). Both frontends then
 * get the very same scripted pad input, and the proxy records what each
 * frontend actually does with the core:
 *
 *   - every environment answer (core option values, pixel format, ...)
 *   - per frame: video hash and size, audio frame count and hash, how many
 *     times input was polled and read, and the core's own run time
 *   - at the end: battery save and savestate hashes
 *
 * Nothing here changes the emulation: every call is forwarded unchanged,
 * except the pad state, which comes from the script so both runs press the
 * same buttons on the same frames.
 *
 * Environment:
 *   BENCH_CORE   path of the real libretro core (required)
 *   BENCH_LOG    JSON-lines log path (default: bench.jsonl)
 *   BENCH_INPUT  input script: "FRAME MASK" lines, MASK = RetroPad bits
 *                (B=0 Y=1 SELECT=2 START=3 UP=4 DOWN=5 LEFT=6 RIGHT=7 A=8
 *                X=9 L=10 R=11), held until the next line
 *   BENCH_DUMP   directory: raw RGB frames are written there (PPM)
 *   BENCH_DUMP_EVERY  dump one frame every N frames (default 600)
 *   BENCH_PCM    raw interleaved s16 stereo output of the core
 *   BENCH_FILL_DEFAULTS  answer unset options with the core's declared default
 *   BENCH_FORCE  "key=value;key=value": core options forced in both frontends
 *   BENCH_GAME_DATA  always give the core the ROM in memory too
 *   BENCH_EXIT_AT  end the process after this many frames (RetroArch side)
 *   BENCH_KEEP   directory: the final savestate and battery save are kept there
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include "libretro.h"

#define EXPORT __attribute__((visibility("default")))

static void *real;
static FILE *logf_, *pcmf;
static retro_environment_t fe_env;
static retro_video_refresh_t fe_video;
static retro_audio_sample_t fe_sample;
static retro_audio_sample_batch_t fe_batch;
static retro_input_poll_t fe_poll;
static retro_input_state_t fe_state;

static enum retro_pixel_format pixfmt = RETRO_PIXEL_FORMAT_0RGB1555;
static uint64_t frame;
static uint64_t vhash, ahash;
static unsigned vw, vh, vdup, vcalls;
static uint64_t aframes;
static unsigned polls, reads;
static char order[8];
static unsigned order_len;
static uint64_t run_us;

static uint32_t *script_frames, *script_masks;
static size_t script_len, script_pos;
static uint32_t pad_mask;
static unsigned dump_every = 600;
static const char *dump_dir;

#define FNV_OFFSET 1469598103934665603ULL
#define FNV_PRIME 1099511628211ULL
static uint64_t fnv(uint64_t h, const void *data, size_t len) {
  const uint8_t *p = data;
  for (size_t i = 0; i < len; i++) { h ^= p[i]; h *= FNV_PRIME; }
  return h;
}

static void *sym(const char *name) {
  void *f = real ? dlsym(real, name) : NULL;
  if (!f && strncmp(name, "slot_", 5) != 0) {
    fprintf(stderr, "bench proxy: real core lacks %s\n", name);
    abort();
  }
  return f;
}
#define REAL(ret, name, ...) ((ret (*)(__VA_ARGS__))sym(#name))

static void json_str(FILE *f, const char *s) {
  fputc('"', f);
  for (; s && *s; s++) {
    if (*s == '"' || *s == '\\') { fputc('\\', f); fputc(*s, f); }
    else if ((unsigned char)*s < 0x20) fprintf(f, "\\u%04x", *s);
    else fputc(*s, f);
  }
  fputc('"', f);
}

static void load_real(void) {
  if (real) return;
  const char *path = getenv("BENCH_CORE");
  if (!path) { fprintf(stderr, "bench proxy: set BENCH_CORE\n"); abort(); }
  real = dlopen(path, RTLD_NOW | RTLD_LOCAL);
  if (!real) { fprintf(stderr, "bench proxy: %s\n", dlerror()); abort(); }
  const char *log = getenv("BENCH_LOG");
  logf_ = fopen(log ? log : "bench.jsonl", "w");
  if (!logf_) { perror("bench proxy log"); abort(); }
  fprintf(logf_, "{\"event\":\"open\",\"core\":");
  json_str(logf_, path);
  fprintf(logf_, "}\n");
  const char *pcm = getenv("BENCH_PCM");
  if (pcm) pcmf = fopen(pcm, "wb");
  dump_dir = getenv("BENCH_DUMP");
  if (getenv("BENCH_DUMP_EVERY")) dump_every = (unsigned)atoi(getenv("BENCH_DUMP_EVERY"));
  if (!dump_every) dump_every = 600;
  const char *input = getenv("BENCH_INPUT");
  if (input) {
    FILE *f = fopen(input, "r");
    if (!f) { perror("bench proxy input"); abort(); }
    size_t cap = 0;
    unsigned long fr, mask;
    char line[256];
    while (fgets(line, sizeof line, f)) {
      if (line[0] == '#' || sscanf(line, "%lu %li", &fr, (long *)&mask) != 2) continue;
      if (script_len == cap) {
        cap = cap ? cap * 2 : 64;
        script_frames = realloc(script_frames, cap * sizeof *script_frames);
        script_masks = realloc(script_masks, cap * sizeof *script_masks);
      }
      script_frames[script_len] = (uint32_t)fr;
      script_masks[script_len++] = (uint32_t)mask;
    }
    fclose(f);
  }
}

/* ---- declared option defaults (BENCH_FILL_DEFAULTS) ----
 * Simulates a frontend that answers an option it never set with the core's
 * own declared default (the first value of SET_VARIABLES), as RetroArch does. */
#define MAX_DEFAULTS 1024
static char *def_keys[MAX_DEFAULTS], *def_values[MAX_DEFAULTS];
static size_t def_count;

static void remember_defaults(const struct retro_variable *vars) {
  for (; vars && vars->key && def_count < MAX_DEFAULTS; vars++) {
    const char *semi = vars->value ? strchr(vars->value, ';') : NULL;
    if (!semi) continue;
    semi++;
    while (*semi == ' ') semi++;
    size_t len = strcspn(semi, "|");
    def_keys[def_count] = strdup(vars->key);
    def_values[def_count++] = strndup(semi, len);
  }
}

static const char *declared_default(const char *key) {
  for (size_t i = 0; key && i < def_count; i++)
    if (!strcmp(def_keys[i], key)) return def_values[i];
  return NULL;
}

/* ---- environment ---- */
static bool env_cb(unsigned cmd, void *data) {
  bool ok = fe_env ? fe_env(cmd, data) : false;
  unsigned base = cmd & 0xffff;
  static int fill = -1;
  if (fill < 0) fill = getenv("BENCH_FILL_DEFAULTS") != NULL;
  if (fill && base == RETRO_ENVIRONMENT_SET_VARIABLES && data) remember_defaults(data);
  /* A headless RetroArch tells the core nobody looks at the picture, and
     Snes9x then skips drawing: always say video and audio are wanted. */
  if (base == 47 && (cmd & RETRO_ENVIRONMENT_EXPERIMENTAL) && data) {
    *(int *)data = 3;
    ok = true;
  }
  bool filled = false;
  /* BENCH_FORCE="key=value;key=value": the same value in both frontends. */
  if (base == RETRO_ENVIRONMENT_GET_VARIABLE && data && getenv("BENCH_FORCE")) {
    struct retro_variable *v = data;
    static char forced[4096];
    snprintf(forced, sizeof forced, ";%s;", getenv("BENCH_FORCE"));
    char needle[512];
    snprintf(needle, sizeof needle, ";%s=", v->key ? v->key : "");
    char *at = strstr(forced, needle);
    if (at) {
      at += strlen(needle);
      static char value[512];
      size_t len = strcspn(at, ";");
      if (len >= sizeof value) len = sizeof value - 1;
      memcpy(value, at, len);
      value[len] = 0;
      v->value = value;
      ok = filled = true;
    }
  }
  if (fill && !filled && base == RETRO_ENVIRONMENT_GET_VARIABLE && data && !ok) {
    struct retro_variable *v = data;
    const char *d = declared_default(v->key);
    if (d) { v->value = d; ok = filled = true; }
  }
  if (!logf_) return ok;
  if (base == RETRO_ENVIRONMENT_GET_VARIABLE && data) {
    const struct retro_variable *v = data;
    fprintf(logf_, "{\"event\":\"get_variable\",\"frame\":%llu,\"key\":",
            (unsigned long long)frame);
    json_str(logf_, v->key);
    fprintf(logf_, ",\"value\":");
    if (ok && v->value) json_str(logf_, v->value); else fputs("null", logf_);
    if (filled) fputs(",\"filled\":true", logf_);
    fputs("}\n", logf_);
  } else if (base == RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE) {
    if (ok && data && *(bool *)data)
      fprintf(logf_, "{\"event\":\"variable_update\",\"frame\":%llu}\n", (unsigned long long)frame);
  } else {
    if (base == RETRO_ENVIRONMENT_SET_PIXEL_FORMAT && ok && data)
      pixfmt = *(const enum retro_pixel_format *)data;
    /* Polled every frame by some cores: log only the first answer. */
    static uint8_t seen[0x10000 >> 3];
    unsigned key = (cmd & RETRO_ENVIRONMENT_EXPERIMENTAL ? 0x8000 : 0) | (base & 0x7fff);
    if (!(seen[key >> 3] & (1u << (key & 7)))) {
      seen[key >> 3] |= (uint8_t)(1u << (key & 7));
      fprintf(logf_, "{\"event\":\"env\",\"frame\":%llu,\"cmd\":%u,\"experimental\":%s,\"ok\":%s",
              (unsigned long long)frame, base,
              cmd & RETRO_ENVIRONMENT_EXPERIMENTAL ? "true" : "false", ok ? "true" : "false");
      if (base == RETRO_ENVIRONMENT_SET_PIXEL_FORMAT && data)
        fprintf(logf_, ",\"pixel_format\":%d", *(const int *)data);
      if (ok && data && (base == RETRO_ENVIRONMENT_GET_LANGUAGE ||
                         base == RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION ||
                         base == RETRO_ENVIRONMENT_GET_INPUT_BITMASKS))
        fprintf(logf_, ",\"value\":%u", *(const unsigned *)data);
      if (ok && data && (base == RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY ||
                         base == RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY)) {
        fputs(",\"path\":", logf_);
        json_str(logf_, *(const char **)data);
      }
      fputs("}\n", logf_);
    }
  }
  return ok;
}

/* ---- video ---- */
static void dump_frame(const void *data, unsigned w, unsigned h, size_t pitch) {
  char path[1024];
  snprintf(path, sizeof path, "%s/frame-%06llu.ppm", dump_dir, (unsigned long long)frame);
  FILE *f = fopen(path, "wb");
  if (!f) return;
  fprintf(f, "P6\n%u %u\n255\n", w, h);
  for (unsigned y = 0; y < h; y++) {
    const uint8_t *row = (const uint8_t *)data + y * pitch;
    for (unsigned x = 0; x < w; x++) {
      uint8_t rgb[3];
      if (pixfmt == RETRO_PIXEL_FORMAT_XRGB8888) {
        uint32_t p = ((const uint32_t *)row)[x];
        rgb[0] = p >> 16; rgb[1] = p >> 8; rgb[2] = p;
      } else if (pixfmt == RETRO_PIXEL_FORMAT_RGB565) {
        uint16_t p = ((const uint16_t *)row)[x];
        unsigned r = p >> 11, g = (p >> 5) & 63, b = p & 31;
        rgb[0] = (r << 3) | (r >> 2); rgb[1] = (g << 2) | (g >> 4); rgb[2] = (b << 3) | (b >> 2);
      } else {
        uint16_t p = ((const uint16_t *)row)[x];
        unsigned r = (p >> 10) & 31, g = (p >> 5) & 31, b = p & 31;
        rgb[0] = (r << 3) | (r >> 2); rgb[1] = (g << 3) | (g >> 2); rgb[2] = (b << 3) | (b >> 2);
      }
      fwrite(rgb, 1, 3, f);
    }
  }
  fclose(f);
}

static void video_cb(const void *data, unsigned w, unsigned h, size_t pitch) {
  vcalls++;
  vw = w; vh = h;
  if (!data) {
    vdup++;
  } else if (data != RETRO_HW_FRAME_BUFFER_VALID) {
    size_t bpp = pixfmt == RETRO_PIXEL_FORMAT_XRGB8888 ? 4 : 2;
    uint64_t h64 = FNV_OFFSET;
    for (unsigned y = 0; y < h; y++)
      h64 = fnv(h64, (const uint8_t *)data + y * pitch, w * bpp);
    vhash = h64;
    if (dump_dir && frame % dump_every == 0) dump_frame(data, w, h, pitch);
  }
  if (fe_video) fe_video(data, w, h, pitch);
}

/* ---- audio ---- */
static void audio_cb(int16_t l, int16_t r) {
  int16_t s[2] = {l, r};
  ahash = fnv(ahash, s, sizeof s);
  aframes++;
  if (pcmf) fwrite(s, sizeof s, 1, pcmf);
  if (fe_sample) fe_sample(l, r);
}
static size_t audio_batch_cb(const int16_t *data, size_t frames) {
  ahash = fnv(ahash, data, frames * 4);
  aframes += frames;
  if (pcmf) fwrite(data, 4, frames, pcmf);
  return fe_batch ? fe_batch(data, frames) : frames;
}

/* ---- input ---- */
static void poll_cb(void) {
  polls++;
  if (order_len < sizeof order - 1) order[order_len++] = 'P';
  if (fe_poll) fe_poll();
}
static int16_t state_cb(unsigned port, unsigned device, unsigned index, unsigned id) {
  reads++;
  if (order_len < sizeof order - 1 && (order_len == 0 || order[order_len - 1] != 'R'))
    order[order_len++] = 'R';
  if (fe_state) fe_state(port, device, index, id); /* keep the frontend's own bookkeeping */
  if (port != 0 || (device & RETRO_DEVICE_MASK) != RETRO_DEVICE_JOYPAD) return 0;
  if (id == RETRO_DEVICE_ID_JOYPAD_MASK) return (int16_t)pad_mask;
  return id < 16 ? (int16_t)((pad_mask >> id) & 1) : 0;
}

/* ---- libretro API ---- */
EXPORT void retro_set_environment(retro_environment_t cb) {
  load_real();
  fe_env = cb;
  REAL(void, retro_set_environment, retro_environment_t)(env_cb);
}
EXPORT void retro_set_video_refresh(retro_video_refresh_t cb) {
  load_real(); fe_video = cb;
  REAL(void, retro_set_video_refresh, retro_video_refresh_t)(video_cb);
}
EXPORT void retro_set_audio_sample(retro_audio_sample_t cb) {
  load_real(); fe_sample = cb;
  REAL(void, retro_set_audio_sample, retro_audio_sample_t)(audio_cb);
}
EXPORT void retro_set_audio_sample_batch(retro_audio_sample_batch_t cb) {
  load_real(); fe_batch = cb;
  REAL(void, retro_set_audio_sample_batch, retro_audio_sample_batch_t)(audio_batch_cb);
}
EXPORT void retro_set_input_poll(retro_input_poll_t cb) {
  load_real(); fe_poll = cb;
  REAL(void, retro_set_input_poll, retro_input_poll_t)(poll_cb);
}
EXPORT void retro_set_input_state(retro_input_state_t cb) {
  load_real(); fe_state = cb;
  REAL(void, retro_set_input_state, retro_input_state_t)(state_cb);
}
EXPORT void retro_init(void) { load_real(); REAL(void, retro_init, void)(); }
EXPORT unsigned retro_api_version(void) { load_real(); return REAL(unsigned, retro_api_version, void)(); }
EXPORT void retro_get_system_info(struct retro_system_info *info) {
  load_real(); REAL(void, retro_get_system_info, struct retro_system_info *)(info);
}
EXPORT void retro_get_system_av_info(struct retro_system_av_info *info) {
  REAL(void, retro_get_system_av_info, struct retro_system_av_info *)(info);
  if (logf_)
    fprintf(logf_, "{\"event\":\"av_info\",\"fps\":%.6f,\"sample_rate\":%.3f,\"base\":[%u,%u],\"max\":[%u,%u],\"aspect\":%.6f}\n",
            info->timing.fps, info->timing.sample_rate, info->geometry.base_width,
            info->geometry.base_height, info->geometry.max_width, info->geometry.max_height,
            info->geometry.aspect_ratio);
}
EXPORT void retro_set_controller_port_device(unsigned port, unsigned device) {
  if (logf_) fprintf(logf_, "{\"event\":\"port_device\",\"frame\":%llu,\"port\":%u,\"device\":%u}\n",
                     (unsigned long long)frame, port, device);
  REAL(void, retro_set_controller_port_device, unsigned, unsigned)(port, device);
}
EXPORT void retro_reset(void) {
  if (logf_) fprintf(logf_, "{\"event\":\"reset\",\"frame\":%llu}\n", (unsigned long long)frame);
  REAL(void, retro_reset, void)();
}

static void log_end(const char *why);

static uint64_t now_us(void) {
  struct timespec ts;
  clock_gettime(CLOCK_MONOTONIC, &ts);
  return (uint64_t)ts.tv_sec * 1000000u + ts.tv_nsec / 1000u;
}

/* Run-Ahead timeline (BENCH_TIMELINE=1, RetroArch side). RetroArch's
 * Run-Ahead runs each shown frame as: the real frame, a savestate, N hidden
 * frames (the last one is shown), then loads the savestate back. A run is
 * "real" when no savestate is pending, and the pad script follows real time,
 * as a player's hand does. With BENCH_PAD_EXTERNAL=1 the frontend's own test
 * (slot.) sets the pad with bench_set_pad() and marks each shown picture
 * with bench_present(). */
static int timeline = -1, in_spec, pad_external = -1;
static uint64_t realtime, presents;

EXPORT void bench_set_pad(uint32_t mask) { pad_mask = mask; }
EXPORT void bench_present(void) {
  if (logf_) fprintf(logf_, "{\"event\":\"present\",\"t\":%llu,\"video\":\"%016llx\"}\n",
                     (unsigned long long)presents, (unsigned long long)vhash);
  presents++;
}

EXPORT void retro_run(void) {
  if (timeline < 0) timeline = getenv("BENCH_TIMELINE") != NULL;
  if (pad_external < 0) pad_external = getenv("BENCH_PAD_EXTERNAL") != NULL;
  int is_real = !timeline || !in_spec;
  uint64_t at = timeline ? (is_real ? realtime : realtime - 1) : frame;
  while (!pad_external && script_pos < script_len && script_frames[script_pos] <= at)
    pad_mask = script_masks[script_pos++];
  vcalls = vdup = polls = reads = order_len = 0;
  aframes = 0;
  ahash = FNV_OFFSET;
  uint64_t t = now_us();
  REAL(void, retro_run, void)();
  run_us = now_us() - t;
  order[order_len] = 0;
  if (logf_)
    fprintf(logf_, "{\"event\":\"frame\",\"frame\":%llu,\"pad\":%u,\"video\":\"%016llx\",\"w\":%u,\"h\":%u,"
                   "\"video_calls\":%u,\"dupes\":%u,\"audio_frames\":%llu,\"audio\":\"%016llx\","
                   "\"polls\":%u,\"reads\":%u,\"order\":\"%s\",\"run_us\":%llu,\"real\":%d,\"t\":%llu}\n",
            (unsigned long long)frame, pad_mask, (unsigned long long)vhash, vw, vh, vcalls, vdup,
            (unsigned long long)aframes, (unsigned long long)ahash, polls, reads, order,
            (unsigned long long)run_us, is_real, (unsigned long long)at);
  frame++;
  if (is_real) realtime++;
  /* RetroArch's --max-frames counts presented frames, which some cores
     (Snes9x) never advance with a null video driver: stop here instead. */
  static long exit_at = -2;
  if (exit_at == -2) exit_at = getenv("BENCH_EXIT_AT") ? atol(getenv("BENCH_EXIT_AT")) : -1;
  if (exit_at > 0 && (timeline ? realtime : frame) >= (uint64_t)exit_at) {
    log_end("exit_at");
    if (logf_) fclose(logf_);
    if (pcmf) fclose(pcmf);
    _exit(0);
  }
}

EXPORT size_t retro_serialize_size(void) { return REAL(size_t, retro_serialize_size, void)(); }
EXPORT bool retro_serialize(void *data, size_t size) {
  in_spec = 1;
  return REAL(bool, retro_serialize, void *, size_t)(data, size);
}
EXPORT bool retro_unserialize(const void *data, size_t size) {
  if (logf_) fprintf(logf_, "{\"event\":\"unserialize\",\"frame\":%llu,\"size\":%zu}\n",
                     (unsigned long long)frame, size);
  in_spec = 0;
  return REAL(bool, retro_unserialize, const void *, size_t)(data, size);
}
EXPORT void retro_cheat_reset(void) { REAL(void, retro_cheat_reset, void)(); }
EXPORT void retro_cheat_set(unsigned i, bool on, const char *code) {
  REAL(void, retro_cheat_set, unsigned, bool, const char *)(i, on, code);
}
EXPORT bool retro_load_game(const struct retro_game_info *game) {
  /* BENCH_GAME_DATA=1: always hand the core the ROM in memory as well as
     its path, whatever the frontend did, to rule that difference out. */
  struct retro_game_info copy;
  if (game && !game->data && game->path && getenv("BENCH_GAME_DATA")) {
    FILE *f = fopen(game->path, "rb");
    if (f) {
      fseek(f, 0, SEEK_END);
      long size = ftell(f);
      fseek(f, 0, SEEK_SET);
      void *buf = malloc((size_t)size);
      if (buf && fread(buf, 1, (size_t)size, f) == (size_t)size) {
        copy = *game;
        copy.data = buf;
        copy.size = (size_t)size;
        game = &copy;
      }
      fclose(f);
    }
  }
  bool ok = REAL(bool, retro_load_game, const struct retro_game_info *)(game);
  if (logf_) {
    fprintf(logf_, "{\"event\":\"load_game\",\"ok\":%s,\"path\":", ok ? "true" : "false");
    json_str(logf_, game ? game->path : NULL);
    fprintf(logf_, ",\"has_data\":%s}\n", game && game->data ? "true" : "false");
  }
  return ok;
}
EXPORT bool retro_load_game_special(unsigned type, const struct retro_game_info *info, size_t n) {
  return REAL(bool, retro_load_game_special, unsigned, const struct retro_game_info *, size_t)(type, info, n);
}
EXPORT unsigned retro_get_region(void) { return REAL(unsigned, retro_get_region, void)(); }
EXPORT void *retro_get_memory_data(unsigned id) { return REAL(void *, retro_get_memory_data, unsigned)(id); }
EXPORT size_t retro_get_memory_size(unsigned id) { return REAL(size_t, retro_get_memory_size, unsigned)(id); }

static void log_end(const char *why) {
  if (!logf_) return;
  size_t sram_size = retro_get_memory_size(RETRO_MEMORY_SAVE_RAM);
  void *sram = retro_get_memory_data(RETRO_MEMORY_SAVE_RAM);
  uint64_t sram_hash = sram && sram_size ? fnv(FNV_OFFSET, sram, sram_size) : 0;
  size_t state_size = retro_serialize_size();
  uint64_t state_hash = 0;
  void *state = state_size ? malloc(state_size) : NULL;
  if (state && retro_serialize(state, state_size)) {
    state_hash = fnv(FNV_OFFSET, state, state_size);
    const char *keep = getenv("BENCH_KEEP");
    if (keep) {
      char path[1024];
      snprintf(path, sizeof path, "%s/end-state.bin", keep);
      FILE *f = fopen(path, "wb");
      if (f) { fwrite(state, 1, state_size, f); fclose(f); }
      if (sram && sram_size) {
        snprintf(path, sizeof path, "%s/end-sram.bin", keep);
        f = fopen(path, "wb");
        if (f) { fwrite(sram, 1, sram_size, f); fclose(f); }
      }
    }
  }
  free(state);
  fprintf(logf_, "{\"event\":\"end\",\"why\":\"%s\",\"frames\":%llu,\"sram_size\":%zu,\"sram\":\"%016llx\","
                 "\"state_size\":%zu,\"state\":\"%016llx\"}\n",
          why, (unsigned long long)frame, sram_size, (unsigned long long)sram_hash, state_size,
          (unsigned long long)state_hash);
  fflush(logf_);
}

EXPORT void retro_unload_game(void) {
  log_end("unload");
  REAL(void, retro_unload_game, void)();
}
EXPORT void retro_deinit(void) {
  REAL(void, retro_deinit, void)();
  if (logf_) { fflush(logf_); fclose(logf_); logf_ = NULL; }
  if (pcmf) { fclose(pcmf); pcmf = NULL; }
}

/* slot.'s private core exports, forwarded only when the real core has them. */
EXPORT size_t slot_mgba_audio_sidecar_size(void) {
  size_t (*f)(void) = sym("slot_mgba_audio_sidecar_size"); return f ? f() : 0;
}
EXPORT bool slot_mgba_audio_sidecar(void *p, size_t n, bool save) {
  bool (*f)(void *, size_t, bool) = sym("slot_mgba_audio_sidecar"); return f && f(p, n, save);
}
EXPORT bool slot_mgba_clone_active_save_ram(void *p, size_t n, size_t *out) {
  bool (*f)(void *, size_t, size_t *) = sym("slot_mgba_clone_active_save_ram"); return f && f(p, n, out);
}
EXPORT void slot_mgba_set_sun(int v) {
  void (*f)(int) = sym("slot_mgba_set_sun"); if (f) f(v);
}
EXPORT uint32_t slot_mgba_gb_link_fault(void) {
  uint32_t (*f)(void) = sym("slot_mgba_gb_link_fault"); return f ? f() : 0;
}
EXPORT int slot_gpsp_backend_is_interpreter(void) {
  int (*f)(void) = sym("slot_gpsp_backend_is_interpreter"); return f ? f() : 0;
}
EXPORT size_t slot_gpsp_audio_sidecar_size(void) {
  size_t (*f)(void) = sym("slot_gpsp_audio_sidecar_size"); return f ? f() : 0;
}
EXPORT bool slot_gpsp_audio_sidecar(void *p, size_t n, bool save) {
  bool (*f)(void *, size_t, bool) = sym("slot_gpsp_audio_sidecar"); return f && f(p, n, save);
}
