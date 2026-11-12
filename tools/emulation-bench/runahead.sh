#!/usr/bin/env bash
# Run-Ahead bench: on which shown frame does each button press first change
# the picture, in slot. (Preemptive Frames, as the app runs Run-Ahead solo)
# and in RetroArch (Run-Ahead, single instance, as on the KPA), with 0, 1 and
# 2 frames.
#
# For a script of K button events, the game runs K+1 times with the first
# 0, 1, ... K events: runs k and k+1 differ only by event k, so the first
# shown frame where their pictures differ is that event's response.
#
# usage: runahead.sh CORE PLATFORM ROM [EVENTS]
# env:   RETROARCH, BENCH_OUT (default .build/bench/runahead), BENCH_BIOS
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HERE="$ROOT/tools/emulation-bench"
CORE=$1 PLATFORM=$2 ROM="$(realpath "$3")" EVENTS=${4:-12}
RA="${RETROARCH:-$ROOT/.build/bench/retroarch/retroarch}"
PROXY="$ROOT/.build/bench/bench_proxy.so"
REAL="$ROOT/.build/bench/cores/${CORE}_libretro.so"
EGL_STUB="$ROOT/.build/bench/egl-stub"
NAME="$(basename "${ROM%.*}" | tr -c 'A-Za-z0-9._-' '_' | sed 's/_*$//')"
OUT="${BENCH_OUT:-$ROOT/.build/bench/runahead}/$CORE/$NAME"
rm -rf "$OUT"
mkdir -p "$OUT/scripts"

# The bench's usual pad script, cut to its first EVENTS changes.
python3 "$HERE/input_script.py" 4000 | grep -v "^#" | head -n "$EVENTS" > "$OUT/events.txt" || true
LAST=$(tail -1 "$OUT/events.txt" | cut -d' ' -f1)
FRAMES=$((LAST + 90))
for k in $(seq 0 "$EVENTS"); do head -n "$k" "$OUT/events.txt" > "$OUT/scripts/$k.txt"; done
export BENCH_CORE="$REAL"

slot_run() { # DIR N SCRIPT
  mkdir -p "$1"
  env BENCH_SLOT_PROXY="$PROXY" BENCH_SLOT_CORE="$CORE" BENCH_SLOT_PLATFORM="$PLATFORM" \
    BENCH_SLOT_ROM="$ROM" BENCH_SLOT_WORK="$1/work" BENCH_SLOT_FRAMES="$FRAMES" \
    ${BENCH_BIOS:+BENCH_SLOT_BIOS="$BENCH_BIOS"} BENCH_SLOT_REWIND=0 \
    BENCH_SLOT_RUNAHEAD="$2" BENCH_SLOT_SCRIPT="$3" BENCH_PAD_EXTERNAL=1 \
    BENCH_LOG="$1/log.jsonl" \
    LD_LIBRARY_PATH="$EGL_STUB" RUSTFLAGS="-L $EGL_STUB" \
    cargo test -q --manifest-path "$ROOT/Cargo.toml" -p slot-android --features runtime-tests \
    --lib emulation_bench -- --ignored --nocapture > "$1/run.log" 2>&1 ||
    { tail -30 "$1/run.log" >&2; exit 1; }
  rm -rf "$1/work"
}

ra_run() { # DIR N SCRIPT
  mkdir -p "$1/saves" "$1/states" "$1/system"
  if [ -n "${BENCH_BIOS:-}" ]; then cp "$BENCH_BIOS"/* "$1/system/" 2>/dev/null || true; fi
  local on=false; [ "$2" -gt 0 ] && on=true
  sed -e "s#@DIR@#$1#g" -e "s#^run_ahead_enabled = .*#run_ahead_enabled = \"$on\"#" \
    "$HERE/retroarch.cfg" > "$1/retroarch.cfg"
  printf 'run_ahead_frames = "%s"\nrun_ahead_secondary_instance = "false"\npreemptive_frames_enable = "false"\n' \
    "$(( $2 > 0 ? $2 : 1 ))" >> "$1/retroarch.cfg"
  : > "$1/options.cfg"
  env BENCH_TIMELINE=1 BENCH_INPUT="$3" BENCH_EXIT_AT="$FRAMES" BENCH_LOG="$1/log.jsonl" \
    timeout 600 "$RA" -c "$1/retroarch.cfg" -L "$PROXY" "$ROM" --max-frames="$((FRAMES * 4))" \
    > "$1/run.log" 2>&1 || { tail -30 "$1/run.log" >&2; exit 1; }
}

for n in 0 1 2; do
  for k in $(seq 0 "$EVENTS"); do
    slot_run "$OUT/slot-ra$n/$k" "$n" "$OUT/scripts/$k.txt"
    ra_run "$OUT/retroarch-ra$n/$k" "$n" "$OUT/scripts/$k.txt"
  done
done
python3 "$HERE/runahead_report.py" "$OUT" | tee "$OUT/report.txt"
