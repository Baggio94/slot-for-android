#!/usr/bin/env bash
# One-time setup of the headless emulation bench on a Linux host:
#   - RetroArch, pinned release, built without window, sound or menu
#   - the bench proxy core
#   - a link-time stand-in for libEGL (slot.'s test build links it, never calls it)
#   - slot.'s nine cores (build-cores-linux.sh)
# usage: setup.sh [CORE...]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HERE="$ROOT/tools/emulation-bench"
B="$ROOT/.build/bench"
RA_TAG="${RA_TAG:-v1.22.2}"
mkdir -p "$B"

if [ ! -x "$B/retroarch/retroarch" ]; then
  [ -d "$B/retroarch/.git" ] ||
    git clone -q --depth 1 --branch "$RA_TAG" https://github.com/libretro/RetroArch.git "$B/retroarch"
  (cd "$B/retroarch" && ./configure --disable-x11 --disable-wayland --disable-kms --disable-egl \
    --disable-opengl --disable-opengl1 --disable-opengl_core --disable-vulkan --disable-sdl \
    --disable-sdl2 --disable-alsa --disable-pulse --disable-ffmpeg --disable-qt --disable-cheevos \
    --disable-discord --disable-libusb --disable-udev --disable-freetype --disable-materialui \
    --disable-xmb --disable-ozone --disable-networking > "$B/retroarch-configure.log" 2>&1 &&
    make -j"$(nproc)" > "$B/retroarch-build.log" 2>&1)
fi

cc -O2 -fPIC -shared -fvisibility=hidden -Wall -Wextra -Wno-unused-parameter \
  -I "$B/retroarch/libretro-common/include" "$HERE/proxy/bench_proxy.c" -ldl -o "$B/bench_proxy.so"

mkdir -p "$B/egl-stub"
for f in eglBindAPI eglChooseConfig eglCreateContext eglCreatePbufferSurface eglCreateWindowSurface \
  eglDestroyContext eglDestroySurface eglGetCurrentDisplay eglGetCurrentSurface eglGetDisplay \
  eglGetError eglGetFrameTimestampSupportedANDROID eglGetFrameTimestampsANDROID \
  eglGetNextFrameIdANDROID eglGetPlatformDisplay eglGetProcAddress eglInitialize eglMakeCurrent \
  eglQueryString eglQuerySurface eglSurfaceAttrib eglSwapBuffers eglSwapInterval eglTerminate; do
  echo "void *$f(void) { return 0; }"
done > "$B/egl-stub/stub.c"
cc -shared -fPIC "$B/egl-stub/stub.c" -o "$B/egl-stub/libEGL.so"

"$HERE/build-cores-linux.sh" "$@"
