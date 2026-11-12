package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.assertNull
import org.junit.Test

class RommTest {
    @Test
    fun serverAddressesGetTheirScheme() {
        assertEquals("http://192.168.1.20:8080", Romm.normalizeServer(" 192.168.1.20:8080/ "))
        assertEquals("http://nas:8080", Romm.normalizeServer("nas:8080"))
        assertEquals("http://romm.local", Romm.normalizeServer("romm.local"))
        assertEquals("https://romm.example.com", Romm.normalizeServer("romm.example.com"))
        assertEquals("https://romm.example.com/sub", Romm.normalizeServer("romm.example.com/sub/api/"))
        assertEquals("http://romm.example.com", Romm.normalizeServer("http://romm.example.com"))
        assertNull(Romm.normalizeServer(""))
        assertNull(Romm.normalizeServer("ftp://romm.example.com"))
        assertNull(Romm.normalizeServer("my server"))
    }

    @Test
    fun rommSlugsAreSlotsSystems() {
        assertEquals(Systems.GBA, Romm.platformOf("gba"))
        assertEquals(Systems.NES, Romm.platformOf("famicom"))
        assertEquals(Systems.SNES, Romm.platformOf("sfam"))
        assertEquals(Systems.MD, Romm.platformOf("genesis"))
        assertNull(Romm.platformOf("psx"))
    }

    @Test
    fun eachSystemGoesToItsEsDeFolder() {
        assertEquals("gba", Romm.folderNames("gba", Systems.GBA).first())
        assertEquals("megadrive", Romm.folderNames("genesis", Systems.MD).first())
        assertEquals(listOf("famicom", "nes"), Romm.folderNames("famicom", Systems.NES))
        assertEquals("snes", Romm.folderNames("snes", Systems.SNES).first())
    }

    @Test
    fun headersAreReadAsTheShelfReadsThem() {
        val gb = ByteArray(0x150)
        "POKEMON YEL".toByteArray().copyInto(gb, 0x134)
        "BYTE".toByteArray().copyInto(gb, 0x13f)
        gb[0x143] = 0x80.toByte()
        gb[0x14a] = 1
        val header = Romm.parseHeader(Systems.GBC, gb)!!
        assertEquals("POKEMON YEL", header.gbTitle)
        assertEquals("BYTE", header.gbCode)
        assertEquals(0x80, header.gbCgbFlag)
        assertFalse(header.gbJapan)
        val gba = ByteArray(0x150)
        "AXVE".toByteArray().copyInto(gba, 0xac)
        assertEquals("AXVE", Romm.parseHeader(Systems.GBA, gba)!!.code)
        assertNull(Romm.parseHeader(Systems.GB, ByteArray(16)))
    }

    @Test
    fun onlyGameBoyFilesBareOrZippedHaveAHeaderRead() {
        fun game(file: String, platform: String) = Romm.Game(1, platform, "gb", file, file, 1, "")
        assertTrue(Romm.wantsHeader(game("Tetris (World).gb", Systems.GB)))
        assertTrue(Romm.wantsHeader(game("Tetris (World).zip", Systems.GB)))
        assertFalse(Romm.wantsHeader(game("Tetris (World).7z", Systems.GB)))
        assertFalse(Romm.wantsHeader(game("Sonic (USA).md", Systems.MD)))
    }

    @Test
    fun theArtSetNamesMatchAsTheCarouselDoes() {
        val catalog = OfficialArtMatcher.Catalog.fromNames(Systems.GBA, mapOf(
            "11111111" to "Apotris (World) (v3.4.5)",
            "22222222" to "Apotris (World) (v4.1.0)",
            "33333333" to "Advance Wars (USA)",
            "44444444" to "Advance Wars (Europe)",
        ))
        assertEquals("22222222", OfficialArtMatcher.fromCatalog(Systems.GBA,
            "Apotris (Ultimate Remix Soundpack) (Tudd) (v1.1) (Aftermarket)", ByteArray(0), catalog) { it })
        assertEquals("33333333", OfficialArtMatcher.fromCatalog(Systems.GBA,
            "Advance Wars (USA) (Rev 1)", ByteArray(0), catalog) { it })
    }

    private fun zipOf(name: String, rom: ByteArray, stored: Boolean): ByteArray {
        val out = java.io.ByteArrayOutputStream()
        java.util.zip.ZipOutputStream(out).use { zip ->
            val entry = java.util.zip.ZipEntry(name)
            if (stored) {
                entry.method = java.util.zip.ZipEntry.STORED
                entry.size = rom.size.toLong()
                entry.compressedSize = rom.size.toLong()
                entry.crc = java.util.zip.CRC32().apply { update(rom) }.value
            }
            zip.putNextEntry(entry)
            zip.write(rom)
            zip.closeEntry()
        }
        return out.toByteArray()
    }

    @Test
    fun aZippedGamesHeaderIsReadFromTheZipsFirstBytes() {
        val rom = ByteArray(256 * 1024) { (it * 31 % 251).toByte() }
        "POKEMON YEL".toByteArray().copyInto(rom, 0x134)
        rom[0x143] = 0x80.toByte()
        for (stored in listOf(false, true)) {
            val zip = zipOf("Pokemon Yellow (USA).gbc", rom, stored)
            val head = Romm.zipHead(zip.copyOf(minOf(zip.size, 16 * 1024)))!!
            assertEquals(0x150, head.size)
            assertEquals("POKEMON YEL", Romm.parseHeader(Systems.GBC, head)!!.gbTitle)
        }
        assertNull(Romm.zipHead(zipOf("readme.txt", rom, false).copyOf(16 * 1024)))
        assertNull(Romm.zipHead(ByteArray(64)))
    }

    @Test
    fun pairingCodesGetTheirDash() {
        assertEquals("ABC", Romm.formatPairingCode("abc", grew = true))
        assertEquals("ABCD-", Romm.formatPairingCode("abcd", grew = true))
        assertEquals("ABCD", Romm.formatPairingCode("ABCD", grew = false))
        assertEquals("ABCD-E", Romm.formatPairingCode("ABCD-e", grew = true))
        assertEquals("ABCD-EFGH", Romm.formatPairingCode("abcdefghij", grew = true))
        assertNull(Romm.formatPairingCode("rmm_abc", grew = true))
        assertNull(Romm.formatPairingCode("rm", grew = true))
    }

    @Test
    fun hacksAndOtherRegionsWearTheirGamesLabel() {
        val names = mapOf(
            "00000001" to "Castlevania - Aria of Sorrow (USA)",
            "00000002" to "Castlevania - Aria of Sorrow (Europe) (En,Fr,De)",
            "00000003" to "Castlevania (USA)",
            "00000004" to "Super Mario World (USA)",
            "00000005" to "Super Mario World (Europe) (Rev 1)",
            "00000006" to "Crash Bandicoot XS (Europe) (En,Fr,De,Es,It,Nl)",
        )
        val catalog = OfficialArtMatcher.Catalog.fromNames(Systems.GBA, names)
        val byName = names.entries.associate { it.value.lowercase() to it.key }
        val keys = catalog.entries.map { it.titleKey }.filter { it.split(' ').size >= 2 }
            .distinct().sortedByDescending { it.length }
        fun crc(title: String) = ArtNames.match(Systems.GBA, title, byName, catalog, keys)
        assertEquals("00000002", crc("Castlevania - Aria of Sorrow (Europe) (En,Fr,De)"))
        assertEquals("00000001", crc("Castlevania - Aria of Sorrow (USA) (Recolor) (JonataGuitar) (v1.2)"))
        assertEquals("00000001", crc("Castlevania - Aria of Sorrow (Subset - Bonus)"))
        assertEquals("00000001", crc("Castlevania - Aria of Sorrow Reprise (Hack)"))
        assertNull(crc("Castlevania - Maria of Sorrow (Simple HUD) (Hack)"))
        assertEquals("00000004", crc("SMW - Grand Poo World 3 (Barbarian) (Hack)"))
        assertEquals("00000006", crc("Crash Bandicoot XS (USA)"))
        assertNull(crc("Celeste Classic (World) (v1.2) (Aftermarket)"))
    }
}
