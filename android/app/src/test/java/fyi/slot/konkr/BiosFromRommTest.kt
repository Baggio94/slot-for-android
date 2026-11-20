package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** BIOS fetched from RomM: only the files slot. knows, whole and valid. */
class BiosFromRommTest {
    @Test fun eachSystemAsksForItsOwnBios() {
        assertEquals(listOf("gba_bios.bin"), BiosLibrary.namesFor(Systems.GBA))
        assertEquals(listOf("gbc_bios.bin"), BiosLibrary.namesFor(Systems.GBC))
        assertEquals(listOf("gb_bios.bin"), BiosLibrary.namesFor(Systems.GB))
        assertTrue(BiosLibrary.namesFor(Systems.SNES).isEmpty())
    }

    @Test fun onlyWholeKnownBiosAreKept() {
        val gba = ByteArray(16_384).also { it[0] = 0x18 }
        assertTrue(BiosLibrary.valid("gba_bios.bin", gba))
        assertFalse(BiosLibrary.valid("gba_bios.bin", ByteArray(16_384)))
        assertFalse(BiosLibrary.valid("gba_bios.bin", gba.copyOf(16_000)))
        assertTrue(BiosLibrary.valid("gb_bios.bin", ByteArray(256)))
        assertTrue(BiosLibrary.valid("gbc_bios.bin", ByteArray(2_304)))
        assertFalse(BiosLibrary.valid("scph5501.bin", ByteArray(524_288)))
    }

    @Test fun theToastNamesTheSystems() {
        assertEquals("GBA", BiosLibrary.systemOf("gba_bios.bin"))
        assertEquals("GBC", BiosLibrary.systemOf("gbc_bios.bin"))
        assertEquals("GB", BiosLibrary.systemOf("gb_bios.bin"))
    }
}
