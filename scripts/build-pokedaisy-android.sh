#!/usr/bin/env bash
# Reproducible arm64 build of PokeDaisy's libretro core for Game Boy, Game
# Boy Color and GBA Pokemon games: mGBA 0.10.5 with PokeDaisy's companion
# (party, bag, map, EXPAND, HUD, quick menu) layered over it, from a pinned
# upstream commit and its own CMake project (libretro/CMakeLists.txt).
# Only retro_* is exported, so it sits beside slot.'s own mGBA.
# No BIOS or ROMs. Requires git, cmake and Android NDK 27+.
#
# Based on PokeDaisy by Lidor Itzhari - https://github.com/lidor30/pokedaisy
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NDK="${ANDROID_NDK_HOME:-${ANDROID_SDK_ROOT:?Set ANDROID_NDK_HOME or ANDROID_SDK_ROOT}/ndk/27.2.12479018}"
[ -f "$NDK/build/cmake/android.toolchain.cmake" ] || { echo "Android NDK not found in $NDK" >&2; exit 1; }
case "$(uname -s)" in
  Darwin) NDK_HOST="darwin-x86_64" ;;
  Linux) NDK_HOST="linux-x86_64" ;;
  *) echo "Unsupported build host: $(uname -s)" >&2; exit 1 ;;
esac
LLVM="$NDK/toolchains/llvm/prebuilt/$NDK_HOST/bin"
# PokeDaisy 1.6.2.1 (2026-11-24) and the mGBA it pins (0.10.5).
PIN=717f4f49979f4aa5c4c8f4cd64638a0fa935d844
MGBA_PIN=26b7884bc25a5933960f3cdcd98bac1ae14d42e2
OUT="$ROOT/.build/pokedaisy"
SRC="$OUT/source"
BUILD="$OUT/android-arm64"
# The source: kept by slot. as a tarball of exactly what this core builds
# from (PIN with its mGBA), checked by its SHA-256, since lidor30/pokedaisy
# left GitHub (26 Nov 2026). POKEDAISY_SOURCE names it; without it, the
# upstream repository at PIN, if it can be reached.
SOURCE_SHA256=aff1ddcd60720b1eda59bd11a5d744de3f5f267bff878aae59580cd0839cfc73
mkdir -p "$OUT"
if [ -n "${POKEDAISY_SOURCE:-}" ]; then
  [ "$(shasum -a 256 "$POKEDAISY_SOURCE" | cut -d' ' -f1)" = "$SOURCE_SHA256" ] || {
    echo "PokeDaisy source tarball isn't the one slot. keeps ($SOURCE_SHA256)" >&2
    exit 1
  }
  rm -rf "$SRC" && mkdir -p "$SRC"
  tar -xzf "$POKEDAISY_SOURCE" -C "$SRC" --strip-components=1
else
  if [ ! -d "$SRC/.git" ]; then
    git init -q "$SRC"
    git -C "$SRC" remote add origin https://github.com/lidor30/pokedaisy.git
  fi
  if [ "$(git -C "$SRC" rev-parse HEAD 2>/dev/null || true)" != "$PIN" ]; then
    git -C "$SRC" fetch --depth=1 origin "$PIN"
    git -C "$SRC" checkout --detach -q FETCH_HEAD
  fi
  # The one submodule this core builds from (the DS cores, armGBA and the
  # rest are never fetched).
  git -C "$SRC" submodule update --init --depth=1 third_party/mgba
  [ "$(git -C "$SRC/third_party/mgba" rev-parse HEAD)" = "$MGBA_PIN" ] || {
    echo "PokeDaisy pins another mGBA than $MGBA_PIN" >&2
    exit 1
  }
fi
# Patches for this core, applied strictly: never a half-patched source.
# The source sits inside slot.'s own checkout: git apply must not take it
# for a subfolder of slot. (it would skip every file, quietly).
export GIT_CEILING_DIRECTORIES="$OUT"
PATCHES=()
for patch_file in "$ROOT/cores/pokedaisy"/*.patch; do
  [ -e "$patch_file" ] || continue
  patch="$(basename "$patch_file")"
  if (cd "$SRC" && git apply --reverse --check "$patch_file") >/dev/null 2>&1; then
    echo "Already applied: $patch"
  elif (cd "$SRC" && git apply --check "$patch_file"); then
    (cd "$SRC" && git apply "$patch_file")
    echo "Applied: $patch"
  else
    echo "Patch mismatch (refusing to use unsafe PokeDaisy source): $patch" >&2
    exit 1
  fi
  PATCHES+=("\"$patch\"")
done
cmake -S "$SRC/libretro" -B "$BUILD" \
  -DCMAKE_TOOLCHAIN_FILE="$NDK/build/cmake/android.toolchain.cmake" \
  -DANDROID_ABI=arm64-v8a -DANDROID_PLATFORM=android-31 \
  -DANDROID_SUPPORT_FLEXIBLE_PAGE_SIZES=ON \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_POLICY_VERSION_MINIMUM=3.5
cmake --build "$BUILD" --target pokedaisy_gbc_libretro -j "$(nproc 2>/dev/null || sysctl -n hw.ncpu)"
LIB="$BUILD/pokedaisy_gbc_libretro.so"
[ -s "$LIB" ] || { echo "PokeDaisy libretro core missing" >&2; exit 1; }
SYMBOLS="$("$LLVM/llvm-nm" -D --defined-only "$LIB")"
for symbol in retro_init retro_load_game retro_run retro_serialize retro_unserialize \
  retro_get_memory_data retro_get_system_av_info; do
  grep -q " $symbol$" <<< "$SYMBOLS" || {
    echo "Missing libretro symbol in PokeDaisy: $symbol" >&2
    exit 1
  }
done
# Its mGBA must stay inside: anything but retro_* leaving the object could
# bind to slot.'s own mGBA, loaded in the same process.
if grep -v ' retro_' <<< "$SYMBOLS" | grep -q ' [TDB] '; then
  echo "PokeDaisy exports more than retro_*" >&2
  exit 1
fi
if "$LLVM/llvm-readelf" -d "$LIB" | grep -F 'libc++_shared.so'; then
  echo "PokeDaisy must not need libc++_shared.so" >&2
  exit 1
fi
DEST="$ROOT/android/app/src/main/jniLibs/arm64-v8a"
LICENCES="$ROOT/android/app/src/main/assets/licenses"
mkdir -p "$DEST" "$LICENCES"
cp "$LIB" "$DEST/libpokedaisy_libretro.so"
# GPL 3.0 with its section 7(b) attribution (NOTICE), and the MPL 2.0 of
# the mGBA inside it.
cp "$SRC/LICENSE" "$LICENCES/pokedaisy-LICENSE.txt"
cp "$SRC/NOTICE" "$LICENCES/pokedaisy-NOTICE.txt"
cp "$SRC/third_party/mgba/LICENSE" "$LICENCES/pokedaisy-mgba-LICENSE.txt"
PATCH_LIST="$(IFS=,; echo "${PATCHES[*]:-}")"
cat > "$ROOT/android/app/src/main/assets/core-build-pokedaisy.json" <<JSON
{"core":"pokedaisy","revision":"$PIN","mgba":"$MGBA_PIN","source_sha256":"$SOURCE_SHA256","patches":[$PATCH_LIST]}
JSON
echo "PokeDaisy arm64 core: $DEST/libpokedaisy_libretro.so"
