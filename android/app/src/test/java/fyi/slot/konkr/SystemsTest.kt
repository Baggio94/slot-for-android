package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SystemsTest {
    @Test fun extensionsFileCartsOnTheirShelf() {
        assertEquals("NES", Systems.platformFor("FC"))
        assertEquals("SNES", Systems.platformFor("smc"))
        assertEquals("MD", Systems.platformFor("gen"))
        assertEquals("GB", Systems.platformFor("gb"))
        assertEquals(null, Systems.platformFor("iso"))
    }

    @Test fun coreFoldersFollowTheNativePickOnlyWhenItRunsTheSystem() {
        assertEquals("bsnes", Systems.coreFolder("bsnes", "sfc"))
        assertEquals("Snes9x", Systems.coreFolder("mgba", "sfc"))
        assertEquals("Genesis Plus GX", Systems.coreFolder("", "md"))
        assertEquals("PicoDrive", Systems.coreFolder("picodrive", "bin"))
        assertEquals("Nestopia", Systems.coreFolder("nestopia", "nes"))
        assertEquals("mGBA", Systems.coreFolder("gambatte", "gba"))
        assertEquals("Gambatte", Systems.coreFolder("gambatte", "gbc"))
        // Private files use the same key as the native side.
        assertEquals("genesis_plus_gx", Systems.keyForFolder("Genesis Plus GX"))
        assertEquals("mgba", Systems.keyForFolder("mGBA"))
        assertEquals("FCEUmm", Systems.folderForKey("fceumm"))
    }

    @Test fun binNeedsASegaHeader() {
        val sega = ByteArray(0x200).also { "SEGA GENESIS    ".toByteArray().copyInto(it, 0x100) }
        assertTrue(Systems.acceptsHeader("bin", sega))
        assertFalse(Systems.acceptsHeader("bin", ByteArray(0x200)))
        assertTrue(Systems.acceptsHeader("md", ByteArray(0x200)))
    }

    @Test fun nesBatteryComesFromTheInesHeader() {
        val rom = ByteArray(0x150).also {
            "NES".toByteArray().copyInto(it)
            it[3] = 0x1a
            it[6] = 0x02
        }
        assertTrue(Systems.homeHeader("nes", rom, "Zelda (USA).nes").battery)
        rom[6] = 0
        assertFalse(Systems.homeHeader("nes", rom, "Mario.nes").battery)
        assertEquals("J", Systems.homeHeader("nes", rom, "Mario (Japan).nes").region)
    }

    @Test fun snesHeaderGivesTitleRegionAndBattery() {
        val rom = ByteArray(0x10000)
        val base = 0xffc0
        "ZELDA".padEnd(21).toByteArray().copyInto(rom, base)
        rom[base + 0x15] = 0x21          // HiROM
        rom[base + 0x16] = 0x02          // ROM + RAM + battery
        rom[base + 0x18] = 0x03          // 8 KiB SRAM
        rom[base + 0x19] = 0x00          // Japan
        rom[base + 0x1c] = 0xff.toByte(); rom[base + 0x1d] = 0xff.toByte()
        val header = Systems.homeHeader("sfc", rom, "game.sfc")
        assertEquals("ZELDA", header.title)
        assertEquals("J", header.region)
        assertTrue(header.battery)
        // The file name's No-Intro region wins over the header.
        assertEquals("U", Systems.homeHeader("sfc", rom, "Zelda (USA).sfc").region)
    }

    @Test fun megaDriveHeaderGivesRegionAndBattery() {
        val rom = ByteArray(0x200)
        "SEGA MEGA DRIVE ".toByteArray().copyInto(rom, 0x100)
        "PHANTASY STAR".toByteArray().copyInto(rom, 0x150)
        "RA".toByteArray().copyInto(rom, 0x1b0)
        rom[0x1b2] = 0xf8.toByte()
        "J".toByteArray().copyInto(rom, 0x1f0)
        val header = Systems.homeHeader("md", rom, "ps.md")
        assertEquals("PHANTASY STAR", header.title)
        assertEquals("J", header.region)
        assertTrue(header.battery)
        "JUE".toByteArray().copyInto(rom, 0x1f0)
        assertEquals("U", Systems.homeHeader("md", rom, "ps.md").region)
        "E  ".toByteArray().copyInto(rom, 0x1f0)
        assertEquals("E", Systems.homeHeader("md", rom, "ps.md").region)
        "4  ".toByteArray().copyInto(rom, 0x1f0)
        assertEquals("U", Systems.homeHeader("md", rom, "ps.md").region)
    }

    @Test fun everySystemHasOneCompleteRow() {
        assertEquals(listOf("GBA", "GB", "GBC", "NES", "SNES", "MD"), Systems.PLATFORMS)
        for (row in Systems.TABLE) {
            val p = row.platform
            assertTrue(p, row.extensions.isNotEmpty() && row.folders.isNotEmpty() && row.romm.isNotEmpty())
            assertEquals(p, 2, row.cores.size)
            for (core in row.cores) assertEquals(p, core, Systems.keyForFolder(Systems.folderForKey(core)))
            for (ext in row.extensions) assertTrue("$p .$ext", ext in Systems.ROM_EXTENSIONS)
            for (slug in row.romm) assertEquals("$p $slug", p, Romm.platformOf(slug))
            assertEquals(p, row.home, p in Systems.HOME_PLATFORMS)
            assertTrue(p, row.retroAchievements > 0 && row.screenScraper > 0)
        }
        assertEquals(Systems.TABLE.size, Systems.TABLE.map { it.retroAchievements }.toSet().size)
        assertEquals(Systems.TABLE.size, Systems.TABLE.map { it.screenScraper }.toSet().size)
    }

    @Test fun ourOwnFoldersAreNeverSkippedByTheScan() {
        // RomLibrary skips ES-DE folders of systems slot. does not run
        // (gamegear, ngp, wonderswan...): a system joining the table must
        // leave that list, or its carts are never found.
        for (row in Systems.TABLE) for (folder in row.folders) {
            assertFalse("${row.platform} $folder", folder in RomLibrary.OTHER_SYSTEM_DIRECTORIES)
            assertFalse("${row.platform} $folder", folder in RomLibrary.IGNORED_DIRECTORIES)
        }
    }

    @Test fun theTableKeepsTheSixSystemsAsTheyWere() {
        assertEquals(setOf("gba", "gb", "gbc", "nes", "fc", "unf", "unif", "sfc", "smc", "md", "gen", "smd", "bin"),
            Systems.ROM_EXTENSIONS)
        assertEquals(setOf("NES", "SNES", "MD"), Systems.HOME_PLATFORMS)
        assertEquals("GBA", Systems.platformFor("GBA"))
        assertEquals("GBC", Systems.platformFor("gbc"))
        assertEquals("MD", Systems.platformFor("bin"))
        assertEquals(mapOf("GB" to 4, "GBA" to 5, "GBC" to 6, "NES" to 7, "SNES" to 3, "MD" to 1),
            Systems.PLATFORMS.associateWith { Systems.retroAchievementsId(it) })
        assertEquals(mapOf("GB" to null, "GBA" to null, "GBC" to null, "NES" to 3, "SNES" to 4, "MD" to 1),
            Systems.PLATFORMS.associateWith { HomeCartLabels.systemId(it) })
        assertEquals("MD", Romm.platformOf("Sega-Mega-Drive"))
        assertEquals(null, Romm.platformOf("gamegear"))
        assertEquals(listOf("famicom", "nes"), Romm.folderNames("famicom", "NES"))
        assertEquals(listOf("sfc", "snes"), Romm.folderNames("sfam", "SNES"))
        assertEquals(listOf("nes", "famicom"), Romm.folderNames("nes", "NES"))
        assertEquals(listOf("snes", "sfc", "snesna"), Romm.folderNames("snes", "SNES"))
        assertEquals(listOf("megadrive", "genesis", "megadrivejp"), Romm.folderNames("genesis", "MD"))
        assertEquals(listOf("gb"), Romm.folderNames("gb", "GB"))
    }
}
