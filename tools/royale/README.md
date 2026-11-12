# crt-royale-fast

`convert.py` turns the libretro slang passes of crt-royale-fast
(`src-fast/`, from libretro/slang-shaders `crt/shaders/crt-royale/src-fast`)
into the GLSL ES 3.00 files slot-gfx builds in
(`crates/slot-gfx/src/royale/pass0..7.glsl`), with every parameter fixed at
the preset's default (flat screen, no curvature).

    python3 tools/royale/convert.py tools/royale/src-fast crates/slot-gfx/src/royale

crt-royale (C) 2014 TroggleMonkey, crt-royale-fast by Hyllian, GPL-2.0-or-later.
The phosphor masks (`mask-*.rgba`, 64x64 RGBA) are the preset's three
`*ResizeTo64` PNG files as raw pixels.
