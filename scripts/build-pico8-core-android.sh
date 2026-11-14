#!/usr/bin/env bash
# Build slot.'s real PICO-8 core: libpico8_libretro.so (cores/pico8real).
# It holds no part of PICO-8: it loads the player's own Raspberry Pi build
# (pico8_64 + pico8.dat) from the BIOS folder at run time.
# Usage: build-pico8-core-android.sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NDK="${ANDROID_NDK_HOME:-${ANDROID_SDK_ROOT:?Set ANDROID_NDK_HOME or ANDROID_SDK_ROOT}/ndk/27.2.12479018}"
LLVM="$(echo "$NDK"/toolchains/llvm/prebuilt/*/bin)"
CC="$LLVM/clang --target=aarch64-linux-android31"
SRC="$ROOT/cores/pico8real"
OUT="$ROOT/.build/pico8real"
mkdir -p "$OUT"
FLAGS=(-O2 -g -fPIC -Wall -Wextra -Wno-unused-parameter -Wno-sign-compare -Wno-missing-field-initializers)
$CC "${FLAGS[@]}" -w -c "$SRC/src/heap.c" -o "$OUT/heap.o"
cat > "$OUT/exports.map" <<'MAP'
{ global: retro_*; local: *; };
MAP
$CC "${FLAGS[@]}" -shared -o "$OUT/libpico8_libretro.so" \
  "$SRC/src/libretro.c" "$SRC/src/core.c" "$SRC/src/loader.c" "$SRC/src/imports.c" "$SRC/src/sdl.c" \
  "$OUT/heap.o" "$SRC/src/switch.S" \
  -Wl,--version-script="$OUT/exports.map" -Wl,-z,max-page-size=16384 -Wl,--build-id -lm -llog
LIB="$OUT/libpico8_libretro.so"
SYMBOLS="$("$LLVM/llvm-nm" -D --defined-only "$LIB")"
for symbol in retro_run retro_serialize retro_unserialize retro_get_system_av_info; do
  grep -q " $symbol$" <<< "$SYMBOLS" || { echo "Missing libretro symbol: $symbol" >&2; exit 1; }
done
DEST="$ROOT/android/app/src/main/jniLibs/arm64-v8a"
mkdir -p "$DEST" "$ROOT/android/app/src/main/assets/licenses"
cp "$LIB" "$DEST/libpico8_libretro.so"
cp "$SRC/LICENCE-dlmalloc.txt" "$ROOT/android/app/src/main/assets/licenses/pico8-dlmalloc.txt"
cat > "$ROOT/android/app/src/main/assets/core-build-pico8.json" <<JSON
{"core":"pico8","revision":"$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo local)","patches":[]}
JSON
echo "pico8 arm64 core: $DEST/libpico8_libretro.so"
