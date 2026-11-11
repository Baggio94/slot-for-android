#!/usr/bin/env bash
# Builds slot.'s nine libretro cores for the Linux host, from the very same
# pinned commits and patches as the APK, so the bench compares frontends and
# never two different cores.
#
# The source is prepared by the APK's own scripts (scripts/build-*-android.sh)
# with a stand-in NDK that stops right before the Android compile step: the
# pin and the patch list therefore have one single source of truth. The host
# compile follows each core's own Linux recipe.
#
# usage: tools/emulation-bench/build-cores-linux.sh [CORE...]
#   CORE: mgba gambatte gpsp fceumm nestopia snes9x bsnes genesis_plus_gx picodrive
# output: .build/bench/cores/<core>_libretro.so (+ <core>.stamp)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$ROOT/.build/bench/cores"
FAKE="$ROOT/.build/bench/fake-ndk"
JOBS="$(nproc)"
ALL=(mgba gambatte gpsp fceumm nestopia snes9x bsnes genesis_plus_gx picodrive)
[ $# -gt 0 ] || set -- "${ALL[@]}"
mkdir -p "$OUT"

# Stand-in NDK: every Android compile entry point refuses with a marker,
# after the real script has fetched the pin and applied every patch.
make_fake_ndk() {
  local bin="$FAKE/toolchains/llvm/prebuilt/linux-x86_64/bin"
  mkdir -p "$bin" "$FAKE/build/cmake"
  printf '#!/bin/sh\necho BENCH_SOURCE_READY >&2\nexit 97\n' > "$FAKE/ndk-build"
  cp "$FAKE/ndk-build" "$bin/aarch64-linux-android31-clang"
  cp "$FAKE/ndk-build" "$bin/aarch64-linux-android31-clang++"
  cp "$FAKE/ndk-build" "$bin/llvm-ar"
  chmod +x "$FAKE/ndk-build" "$bin"/*
  echo 'message(FATAL_ERROR "BENCH_SOURCE_READY")' > "$FAKE/build/cmake/android.toolchain.cmake"
}

# prepare CORE: run the APK script; success means it reached the compile step.
prepare() {
  local core=$1 script log="$ROOT/.build/bench/prepare-$1.log"
  case "$core" in
    mgba|gambatte|gpsp) script="$ROOT/scripts/build-$core-android.sh" ;;
    *) script="$ROOT/scripts/build-home-core-android.sh" ;;
  esac
  set +e
  ANDROID_NDK_HOME="$FAKE" bash "$script" "$core" > "$log" 2>&1
  local status=$?
  set -e
  if ! grep -q BENCH_SOURCE_READY "$log"; then
    echo "Source preparation failed for $core (status $status):" >&2
    tail -20 "$log" >&2
    exit 1
  fi
}

install_core() {
  local core=$1 lib=$2 src=$3
  [ -s "$lib" ] || { echo "Missing $core library: $lib" >&2; exit 1; }
  cp "$lib" "$OUT/${core}_libretro.so"
  {
    echo "core=$core"
    echo "revision=$(git -C "$src" rev-parse HEAD)"
    echo "patched_files=$(git -C "$src" status --porcelain | wc -l)"
    echo "sha256=$(sha256sum "$OUT/${core}_libretro.so" | cut -d' ' -f1)"
  } > "$OUT/$core.stamp"
  echo "Built $core -> $OUT/${core}_libretro.so"
}

make_fake_ndk
for core in "$@"; do
  prepare "$core"
  src="$ROOT/.build/$core/source"
  log="$ROOT/.build/bench/build-$core.log"
  case "$core" in
    mgba)
      cmake -S "$src" -B "$ROOT/.build/bench/mgba-host" -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DBUILD_LIBRETRO=ON -DSKIP_LIBRARY=ON \
        -DBUILD_QT=OFF -DBUILD_SDL=OFF -DBUILD_GL=OFF -DBUILD_GLES2=OFF -DBUILD_GLES3=OFF \
        -DBUILD_SHARED=OFF -DBUILD_STATIC=OFF -DUSE_FFMPEG=OFF -DUSE_PNG=OFF \
        -DUSE_SQLITE3=OFF -DUSE_LUA=OFF -DUSE_JSON_C=OFF -DUSE_LIBZIP=OFF -DUSE_LZMA=OFF \
        -DENABLE_SCRIPTING=OFF -DBUILD_LTO=OFF -DENABLE_DEBUGGERS=OFF \
        -DCMAKE_POSITION_INDEPENDENT_CODE=ON > "$log" 2>&1
      cmake --build "$ROOT/.build/bench/mgba-host" --target mgba_libretro -j"$JOBS" >> "$log" 2>&1
      install_core mgba "$ROOT/.build/bench/mgba-host/mgba_libretro.so" "$src" ;;
    gambatte)
      make -C "$src" -j"$JOBS" platform=unix > "$log" 2>&1
      install_core gambatte "$src/gambatte_libretro.so" "$src" ;;
    gpsp)
      make -C "$src" -j"$JOBS" platform=unix > "$log" 2>&1
      install_core gpsp "$src/gpsp_libretro.so" "$src" ;;
    fceumm|genesis_plus_gx|picodrive)
      make -C "$src" -f Makefile.libretro -j"$JOBS" platform=unix > "$log" 2>&1
      install_core "$core" "$src/${core}_libretro.so" "$src" ;;
    nestopia|snes9x)
      make -C "$src/libretro" -j"$JOBS" platform=unix > "$log" 2>&1
      install_core "$core" "$src/libretro/${core}_libretro.so" "$src" ;;
    bsnes)
      make -C "$src/bsnes" -j"$JOBS" target=libretro binary=library > "$log" 2>&1
      install_core bsnes "$(find "$src/bsnes/out" -name 'bsnes_libretro.so' -print -quit)" "$src" ;;
    *) echo "Unknown core: $core" >&2; exit 1 ;;
  esac
done
