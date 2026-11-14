#!/usr/bin/env bash
# Reproducible arm64 builds of the NES, SNES and Mega Drive libretro cores,
# each from a pinned upstream commit with the core's own Android recipe
# (jni/Android.mk). No BIOS or ROMs. Usage: build-home-core-android.sh CORE
#   fceumm nestopia snes9x bsnes genesis_plus_gx picodrive fake08
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NDK="${ANDROID_NDK_HOME:-${ANDROID_SDK_ROOT:?Set ANDROID_NDK_HOME or ANDROID_SDK_ROOT}/ndk/27.2.12479018}"
[ -x "$NDK/ndk-build" ] || { echo "Android NDK ndk-build not found in $NDK" >&2; exit 1; }
LLVM="$(echo "$NDK"/toolchains/llvm/prebuilt/*/bin)"
CORE="${1:?core name}"
SUBMODULES=0
# path=commit: a submodule held at a later upstream commit than the core
# records, for fixes the core has not picked up yet.
SUBMODULE_PINS=()
OWNER=libretro
LIBNAME=retro
# name | upstream repository | pinned commit | jni folder | licence file
case "$CORE" in
  fceumm)          REPO=libretro-fceumm;  PIN=ac0d00a7fc01166f84c7bac01d1084ebfd2b865e; JNI=jni;                         LICENCE=Copying ;;
  nestopia)        REPO=nestopia;         PIN=f1e02d12664834536edbf7352995a3d0cd1caef4; JNI=libretro/jni;                LICENCE=COPYING ;;
  snes9x)          REPO=snes9x;           PIN=fae2fea08f74180759ef540ee94259213f503480; JNI=libretro/jni;                LICENCE=LICENSE ;;
  bsnes)           REPO=bsnes;            PIN=63ee22f7af8f5d1ada295c2c98fd7bf261fa62a7; JNI=bsnes/target-libretro/jni;  LICENCE=LICENSE.txt ;;
  genesis_plus_gx) REPO=Genesis-Plus-GX;  PIN=f27655b592ef43f317272d79883bcaa10d3ac8ad; JNI=libretro/jni;                LICENCE=LICENSE.txt ;;
  picodrive)       REPO=picodrive;        PIN=1890c2932234c9d30f4cd3851d02228baae8f09e; JNI=jni;                         LICENCE=COPYING; SUBMODULES=1 ;;
  # PICO-8: fake-08 lives outside the libretro organisation and its module is libfake08.so.
  # Its Lua, z8lua, at the head of fake-08's z8lua branch (one-line if,
  # sandbox and chr fixes; jtothebell/fake-08#282 makes the same update).
  fake08)          REPO=fake-08; OWNER=jtothebell; PIN=814991a2571ad3970e386cef48f3b148aa1c27b9; JNI=platform/libretro/jni; LICENCE=LICENSE.MD; SUBMODULES=1; LIBNAME=fake08
                   SUBMODULE_PINS=(libs/z8lua=3f345520341534219d3ba2bb40dd10b6c1f2db9b) ;;
  *) echo "Unknown home console core: $CORE" >&2; exit 1 ;;
esac
OUT="$ROOT/.build/$CORE"
SRC="$OUT/source"
mkdir -p "$OUT"
if [ ! -d "$SRC/.git" ]; then
  git init -q "$SRC"
  git -C "$SRC" remote add origin "https://github.com/$OWNER/$REPO.git"
fi
# Start from clean trees: the last build's patches would block a checkout, and
# patches that touch the same file only apply in order, onto the one before
# (fake08's input and save state patches).
if git -C "$SRC" rev-parse -q --verify HEAD >/dev/null; then
  git -C "$SRC" checkout -q -- .
  git -C "$SRC" submodule foreach -q --recursive git checkout -q -- .
fi
if [ "$(git -C "$SRC" rev-parse HEAD 2>/dev/null || true)" != "$PIN" ]; then
  git -C "$SRC" fetch --depth=1 origin "$PIN"
  git -C "$SRC" checkout --detach -q FETCH_HEAD
fi
if [ "$SUBMODULES" = 1 ]; then
  git -C "$SRC" submodule update --init --depth=1 --recursive
fi
SUBMODULE_LIST=()
for pin in ${SUBMODULE_PINS[@]+"${SUBMODULE_PINS[@]}"}; do
  path="${pin%%=*}"; commit="${pin#*=}"
  if [ "$(git -C "$SRC/$path" rev-parse HEAD)" != "$commit" ]; then
    git -C "$SRC/$path" fetch --depth=1 origin "$commit"
    git -C "$SRC/$path" checkout --detach -q "$commit"
  fi
  SUBMODULE_LIST+=("\"$path@$commit\"")
done
# Patches for this core, applied strictly: never a half-patched source.
PATCHES=()
for patch_file in "$ROOT/cores/$CORE"/*.patch; do
  [ -e "$patch_file" ] || continue
  patch="$(basename "$patch_file")"
  if git -C "$SRC" apply --reverse --check "$patch_file" >/dev/null 2>&1; then
    echo "Already applied: $patch"
  elif git -C "$SRC" apply --check "$patch_file"; then
    git -C "$SRC" apply "$patch_file"
    echo "Applied: $patch"
  else
    echo "Patch mismatch (refusing to use unsafe $CORE source): $patch" >&2
    exit 1
  fi
  PATCHES+=("\"$patch\"")
done
# Only the KONKR ABI, static libc++, 16 KiB pages.
"$NDK/ndk-build" -C "$SRC/$JNI" -j"$(nproc)" \
  APP_ABI=arm64-v8a APP_PLATFORM=android-31 APP_STL=c++_static \
  APP_SUPPORT_FLEXIBLE_PAGE_SIZES=true \
  NDK_PROJECT_PATH="$SRC/$JNI/.." \
  NDK_OUT="$OUT/obj" NDK_LIBS_OUT="$OUT/libs"
LIB="$OUT/libs/arm64-v8a/lib$LIBNAME.so"
[ -s "$LIB" ] || { echo "Missing $CORE libretro library" >&2; exit 1; }
SYMBOLS="$("$LLVM/llvm-nm" -D --defined-only "$LIB")"
for symbol in retro_run retro_serialize retro_unserialize retro_get_system_av_info; do
  grep -q " $symbol$" <<< "$SYMBOLS" || {
    echo "Missing libretro symbol in $CORE: $symbol" >&2
    exit 1
  }
done
if "$LLVM/llvm-readelf" -d "$LIB" | grep -F 'libc++_shared.so'; then
  echo "$CORE must link libc++ statically" >&2
  exit 1
fi
DEST="$ROOT/android/app/src/main/jniLibs/arm64-v8a"
mkdir -p "$DEST" "$ROOT/android/app/src/main/assets/licenses"
cp "$LIB" "$DEST/lib${CORE}_libretro.so"
cp "$SRC/$LICENCE" "$ROOT/android/app/src/main/assets/licenses/$CORE-$(basename "$LICENCE").txt"
PATCH_LIST="$(IFS=,; echo "${PATCHES[*]:-}")"
SUBMODULE_JSON=""
if [ "${#SUBMODULE_LIST[@]}" -gt 0 ]; then
  SUBMODULE_JSON=",\"submodules\":[$(IFS=,; echo "${SUBMODULE_LIST[*]}")]"
fi
cat > "$ROOT/android/app/src/main/assets/core-build-$CORE.json" <<JSON
{"core":"$CORE","revision":"$PIN","patches":[$PATCH_LIST]$SUBMODULE_JSON}
JSON
echo "$CORE arm64 core: $DEST/lib${CORE}_libretro.so"
