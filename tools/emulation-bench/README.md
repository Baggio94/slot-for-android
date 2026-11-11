# Emulation bench: slot. vs RetroArch

Headless, on a Linux host. Runs the same ROM with the same pad input in
slot.'s own `GameSession` and in RetroArch, both on the very same core build,
and compares every frame's picture and sound, the battery save, and the core
options and environment answers each frontend gives the core.

Nothing here changes the app: slot.'s side is an ignored Linux-only test
(`slot_bench.rs`, compiled only with `--features runtime-tests`).

## How it works

`proxy/bench_proxy.c` is a libretro core that loads the real core and sits
between it and the frontend. Both frontends load the proxy, so:

- both get the same scripted pad input (`input_script.py`, or your own
  `FRAME MASK` file);
- the proxy logs, per frame, the picture hash, the audio hash and sample
  count, how the pad was polled, and the core's run time; plus every core
  option value and environment answer, and the final save and savestate.

Cores come from the APK's own pins and patches (`build-cores-linux.sh` runs
`scripts/build-*-android.sh` up to the Android compile step, then builds for
the host). RetroArch is a pinned release built without window, sound or menu.

## Use

    tools/emulation-bench/setup.sh                 # once: RetroArch, proxy, 9 cores
    tools/emulation-bench/run.sh CORE PLATFORM ROM [FRAMES] [INPUT]
    python3 tools/emulation-bench/summary.py .build/bench/results

`CORE`: mgba gpsp gambatte fceumm nestopia snes9x bsnes genesis_plus_gx
picodrive. `PLATFORM`: gba gb gbc nes snes md.

Each game runs five times (see the header of `run.sh`) and gets four
comparisons:

| comparison | question it answers |
|---|---|
| defaults | what players get: slot. as shipped vs RetroArch as shipped |
| frontend | same options, no Rewind: is anything left in the frontend itself? |
| rewind   | what slot.'s Rewind captures change in the emulation |
| options  | what answering every core option (declared default) would change |

Savestate hashes are shown but not judged: some cores keep host data in them.

## Limits

Headless runs say nothing about frame pacing, input latency, audio output
or display: those are measured on the device (see the device part of the
bench). Vulkan vs GL does not matter here, none of slot.'s cores render
with the GPU.
