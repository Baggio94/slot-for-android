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
        assertEquals(listOf("lynxboot.img"), BiosLibrary.namesFor(Systems.LYNX))
        assertEquals(listOf("bios.gg"), BiosLibrary.namesFor(Systems.GG))
        assertEquals(listOf("bios_U.sms", "bios_E.sms", "bios_J.sms"), BiosLibrary.namesFor(Systems.SMS))
        assertEquals(listOf("bios_MD.bin"), BiosLibrary.namesFor(Systems.MD))
        assertEquals(listOf("bios.min"), BiosLibrary.namesFor(Systems.POKEMINI))
        assertTrue(BiosLibrary.namesFor(Systems.NGP).isEmpty())
        assertTrue(BiosLibrary.namesFor(Systems.WS).isEmpty())
    }

    @Test fun allRecognizedSystemsAreEligibleForAutomaticRommBios() {
        val supported = setOf(Systems.GB, Systems.GBC, Systems.GBA, Systems.LYNX,
            Systems.GG, Systems.SMS, Systems.MD, Systems.POKEMINI)
        assertEquals(supported, BiosLibrary.ROMM_AUTO_PLATFORMS)
        for (system in supported) {
            assertEquals(BiosLibrary.namesFor(system), BiosLibrary.automaticNamesFor(system))
        }
        for (system in listOf(Systems.NGP, Systems.WS, Systems.NES, Systems.SNES)) {
            assertTrue(BiosLibrary.automaticNamesFor(system).isEmpty())
        }
    }

    @Test fun regionalBiosAreRequestedIndependentlyAndExistingFilesStay() {
        val platforms = setOf(Systems.SMS, Systems.GG, Systems.GBA, Systems.NES)
        val installed = setOf("bios_U.sms", "gba_bios.bin")
        assertEquals(setOf("bios_e.sms", "bios_j.sms", "bios.gg"),
            BiosLibrary.missingNames(platforms) { it in installed })
        assertTrue(BiosLibrary.missingNames(platforms) { true }.isEmpty())
    }

    @Test fun theNewSystemsBiosAreCheckedToo() {
        assertTrue(BiosLibrary.valid("lynxboot.img", ByteArray(512)))
        assertTrue(BiosLibrary.valid("LYNXBOOT.IMG", ByteArray(512)))
        assertFalse(BiosLibrary.valid("lynxboot.img", ByteArray(1_024)))
        assertTrue(BiosLibrary.valid("bios.gg", ByteArray(1_024)))
        assertTrue(BiosLibrary.valid("bios_u.sms", ByteArray(8_192)))
        assertTrue(BiosLibrary.valid("bios_E.sms", ByteArray(262_144)))
        assertFalse(BiosLibrary.valid("bios_U.sms", ByteArray(5_000)))
        assertTrue(BiosLibrary.valid("bios.min", ByteArray(4_096)))
        val tmss = ByteArray(16_384).also { "GENESIS OS".toByteArray().copyInto(it, 0x120) }
        assertTrue(BiosLibrary.valid("bios_MD.bin", tmss))
        assertTrue(BiosLibrary.valid("bios_MD.bin", tmss.copyOf(2_048)))
        assertFalse(BiosLibrary.valid("bios_MD.bin", ByteArray(2_048)))
        assertEquals("Lynx", BiosLibrary.systemOf("lynxboot.img"))
        assertEquals("Master System", BiosLibrary.systemOf("bios_j.sms"))
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
