#!/usr/bin/env bash
# Runs one game in slot. and in RetroArch (headless, Linux), through the bench
# proxy core. Runs:
#   slot          slot. as shipped (fresh-install settings, Rewind on)
#   slot-norewind the same without Rewind captures
#   slot-pure     without Rewind, and every core option answered (the
#                 core's declared default when slot. sets none; the proxy
#                 simulates it)
#   ra-defaults   RetroArch with its own defaults (Rewind off)
#   ra-matched    RetroArch forced onto slot-pure's option values
# Comparisons:
#   defaults  slot vs ra-defaults: what players get on each side
#   frontend  slot-pure vs ra-matched: what is left is the frontend itself
#   rewind    slot vs slot-norewind: what Rewind captures change
#   options   slot-norewind vs slot-pure: what answering every option changes
#
# usage: run.sh CORE PLATFORM ROM [FRAMES] [INPUT_SCRIPT]
# env:   RETROARCH  RetroArch binary (default .build/bench/retroarch/retroarch)
#        BENCH_OUT  results folder (default .build/bench/results)
#        BENCH_BIOS optional BIOS folder, given to both frontends
#        BENCH_DUMP_EVERY  also save one picture every N frames
#        BENCH_FORCE / BENCH_GAME_DATA  see proxy/bench_proxy.c
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HERE="$ROOT/tools/emulation-bench"
CORE=$1 PLATFORM=$2 ROM="$(realpath "$3")" FRAMES=${4:-3600}
INPUT=${5:-}
RA="${RETROARCH:-$ROOT/.build/bench/retroarch/retroarch}"
PROXY="$ROOT/.build/bench/bench_proxy.so"
REAL="$ROOT/.build/bench/cores/${CORE}_libretro.so"
[ -x "$RA" ] || { echo "No RetroArch at $RA (run setup.sh)" >&2; exit 1; }
[ -s "$PROXY" ] || { echo "No proxy core; run setup.sh" >&2; exit 1; }
[ -s "$REAL" ] || { echo "No $CORE core; run setup.sh $CORE" >&2; exit 1; }
NAME="$(basename "${ROM%.*}" | tr -c 'A-Za-z0-9._-' '_' | sed 's/_*$//')"
OUT="${BENCH_OUT:-$ROOT/.build/bench/results}/$CORE/$NAME"
rm -rf "$OUT"
mkdir -p "$OUT"
if [ -z "$INPUT" ]; then
  INPUT="$OUT/input.txt"
  python3 "$HERE/input_script.py" "$FRAMES" > "$INPUT"
fi
export BENCH_CORE="$REAL" BENCH_INPUT="$(realpath "$INPUT")"
EGL_STUB="$ROOT/.build/bench/egl-stub"

# slot_run DIR [VAR=VALUE...]: slot.'s own GameSession, through the proxy.
slot_run() {
  local dir=$1; shift
  mkdir -p "$dir"
  env "$@" BENCH_SLOT_PROXY="$PROXY" BENCH_SLOT_CORE="$CORE" BENCH_SLOT_PLATFORM="$PLATFORM" \
    BENCH_SLOT_ROM="$ROM" BENCH_SLOT_WORK="$dir/work" BENCH_SLOT_FRAMES="$FRAMES" \
    ${BENCH_BIOS:+BENCH_SLOT_BIOS="$BENCH_BIOS"} \
    BENCH_LOG="$dir/log.jsonl" BENCH_PCM="$dir/audio.raw" BENCH_KEEP="$dir" \
    ${BENCH_DUMP_EVERY:+BENCH_DUMP="$dir" BENCH_DUMP_EVERY="$BENCH_DUMP_EVERY"} \
    LD_LIBRARY_PATH="$EGL_STUB" RUSTFLAGS="-L $EGL_STUB" \
    cargo test -q --manifest-path "$ROOT/Cargo.toml" -p slot-android --features runtime-tests \
    --lib emulation_bench -- --ignored --nocapture > "$dir/run.log" 2>&1 ||
    { tail -30 "$dir/run.log" >&2; exit 1; }
}

# ra_run DIR OPTIONS_FILE: headless RetroArch, through the proxy.
ra_run() {
  local dir=$1 opts=$2
  mkdir -p "$dir/saves" "$dir/states" "$dir/system"
  if [ -n "${BENCH_BIOS:-}" ]; then cp "$BENCH_BIOS"/* "$dir/system/" 2>/dev/null || true; fi
  sed -e "s#@DIR@#$dir#g" "$HERE/retroarch.cfg" > "$dir/retroarch.cfg"
  cp "$opts" "$dir/options.cfg"
  env BENCH_EXIT_AT="$FRAMES" BENCH_LOG="$dir/log.jsonl" BENCH_PCM="$dir/audio.raw" BENCH_KEEP="$dir" \
    ${BENCH_DUMP_EVERY:+BENCH_DUMP="$dir" BENCH_DUMP_EVERY="$BENCH_DUMP_EVERY"} \
    timeout 600 "$RA" -c "$dir/retroarch.cfg" -L "$PROXY" "$ROM" --max-frames="$((FRAMES * 2))" \
    > "$dir/run.log" 2>&1 || { tail -30 "$dir/run.log" >&2; exit 1; }
}

slot_run "$OUT/slot"
slot_run "$OUT/slot-norewind" BENCH_SLOT_REWIND=0
slot_run "$OUT/slot-pure" BENCH_SLOT_REWIND=0 BENCH_FILL_DEFAULTS=1
: > "$OUT/empty-options.cfg"
ra_run "$OUT/ra-defaults" "$OUT/empty-options.cfg"
python3 "$HERE/options_from_log.py" "$OUT/slot-pure/log.jsonl" > "$OUT/slot-options.cfg"
ra_run "$OUT/ra-matched" "$OUT/slot-options.cfg"

compare() {
  echo "== $CORE / $NAME / $1"
  python3 "$HERE/compare.py" "$2/log.jsonl" "$3/log.jsonl" --json "$OUT/compare-$1.json" |
    tee "$OUT/compare-$1.txt"
}
compare defaults "$OUT/slot" "$OUT/ra-defaults"
compare frontend "$OUT/slot-pure" "$OUT/ra-matched"
compare rewind "$OUT/slot" "$OUT/slot-norewind"
compare options "$OUT/slot-norewind" "$OUT/slot-pure"
