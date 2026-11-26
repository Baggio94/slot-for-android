#!/usr/bin/env python3
"""mGBA's libretro port: a Game Boy palette turned off shows while the game runs.

slot. turns a DMG game's palette off with mgba_gb_colors_preset (1: the game's
own Game Boy Color colours, as a GBC shows it). The port read that option only
when the game started, and while it ran only mgba_gb_colors: the screen kept
the last palette until the next launch. Here the option is read again with the
palette, and its colours (mGBA's own lookup, GBOverrideColorFind, as at boot)
are set the same way a palette is. A game the lookup doesn't know keeps the
palette mgba_gb_colors names, as at boot. A Game Boy Color game is never
touched (it colours itself).

Used for slot.'s mGBA (scripts/build-mgba-android.sh) and PokeDaisy's
(cores/pokedaisy). Save states and libretro entry points stay unchanged.
"""
from pathlib import Path
import sys

if len(sys.argv) != 2:
    raise SystemExit("usage: patch-mgba-gb-palette-off.py SOURCE_DIR")
source = Path(sys.argv[1]) / "src/platform/libretro/libretro.c"
data = source.read_text()
if "_updateGbColors" not in data:
    end = '\t\t\tcore->reloadConfigOption(core, "gb.pal", NULL);\n\t\t\tbreak;\n\t\t}\n\t}\n}\n'
    if data.count(end) != 1:
        raise SystemExit("mGBA palette-off patch cannot find _updateGbPal's end")
    function = r'''
/* slot.: mgba_gb_colors_preset read again with the palette while the game runs: 0 the palette mgba_gb_colors names,
 * any other the game's own colours from that lookup, as at boot (a palette turned off shows at once). */
static void _updateGbColors(void) {
	if (core->platform(core) != mPLATFORM_GB) {
		return;
	}
	struct retro_variable var = { .key = "mgba_gb_colors_preset", .value = 0 };
	int preset = 0;
	if (environCallback(RETRO_ENVIRONMENT_GET_VARIABLE, &var) && var.value) {
		preset = strtol(var.value, NULL, 10);
	}
	struct GB* gb = core->board;
	if (!preset || !gb->memory.rom || gb->model >= GB_MODEL_CGB) {
		_updateGbPal();
		return;
	}
	struct GBCartridgeOverride override;
	memset(&override, 0, sizeof(override));
	const struct GBCartridge* cart = (const struct GBCartridge*) &gb->memory.rom[0x100];
	override.headerCrc32 = doCrc32(cart, sizeof(*cart));
	if (!GBOverrideColorFind(&override, preset)) {
		_updateGbPal();
		return;
	}
	char key[16];
	int i;
	for (i = 0; i < 12; ++i) {
		snprintf(key, sizeof(key), "gb.pal[%d]", i);
		mCoreConfigSetUIntValue(&core->config, key, override.gbColors[i] & 0xFFFFFF);
	}
	core->reloadConfigOption(core, "gb.pal", NULL);
}
'''
    data = data.replace(end, end + function, 1)
    call = "#ifdef M_CORE_GB\n\t\t_updateGbPal();\n#endif\n\t}\n"
    if data.count(call) != 1:
        raise SystemExit("mGBA palette-off patch cannot find the live option update")
    data = data.replace(call, "#ifdef M_CORE_GB\n\t\t_updateGbColors();\n#endif\n\t}\n", 1)
    include = "#include <mgba/internal/gb/overrides.h>\n"
    if data.count(include) != 1:
        raise SystemExit("mGBA palette-off patch cannot find the overrides include")
    data = data.replace(include, include + "#include <mgba-util/crc32.h>\n", 1)
    source.write_text(data)
assert source.read_text().count("_updateGbColors();") == 1
print("Applied: patch-mgba-gb-palette-off.py")
