#!/bin/bash
# Build p8test (static aarch64, NDK) in the slot. Android container: SLOT_SRC=slot-p8real ~/build/slot-run.sh cores/pico8real/test/build-test.sh
set -e
cd "$(dirname "$0")/.."
NDK=${ANDROID_NDK_HOME:-$(ls -d /opt/android-sdk/ndk/* /root/android-sdk/ndk/* 2>/dev/null | tail -1)}
CC="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin/clang --target=aarch64-linux-android31"
mkdir -p .build
$CC -O2 -g -w -c src/heap.c -o .build/heap.o
$CC -static -O2 -g -Wall -Wextra -Wno-unused-parameter -Wno-sign-compare -Wno-missing-field-initializers \
    -o .build/p8test test/p8test.c src/core.c src/loader.c src/imports.c src/sdl.c .build/heap.o src/switch.S -lm -ldl
echo built .build/p8test
